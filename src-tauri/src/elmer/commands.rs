//! Tauri commands for the Elmer agent pane (Task 8b, tuxlink-13v2l).
//!
//! ## Command surface (the React ↔ Rust contract)
//!
//! | Command | AC | Notes |
//! |---|---|---|
//! | `elmer_send` | AC-2 | Single-flight send; emits `elmer-turn`/`elmer-chip`/`elmer-outcome` |
//! | `elmer_stop` | AC-4 | Abort-first cancel |
//! | `egress_rearm` | AC-10 | Rearm the egress guard |
//! | `outbox_staged_list` | AC-3 | Non-tainting outbox read |
//! | `elmer_prepare_outbox_approval` | AC-3 | Freeze staging + issue approval DTO |
//! | `elmer_connect` | AC-3 | Digest-gated flush |
//!
//! ## Security invariants
//!
//! - **AC-8:** `EgressAuthority::Operator` NEVER appears in `src/elmer/` or in
//!   `approval_gated_flush`. The real authority boundary is mcp-core's
//!   `guarded_egress(.., Agent, ..)` — the invoker passes `CallAuthority::Agent`
//!   and the egress guard enforces it. The grep-gate test below verifies this.
//! - **AC-5:** No Tauri command parameter deserializes `Vec<Message>` or
//!   `Conversation`. The agent's transcript is owned by `ElmerSession`; the
//!   React pane never supplies a transcript.
//! - **AC-9:** `clear_taint(` and `quarantine_and_rearm(` have single callers:
//!   `egress_rearm` (via `session.rearm`) and the `tuxlink-security` tests.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter as _, State};

use tuxlink_agent_runner::RunOutcome;
use tuxlink_mcp_core::ports::{OutboxReadPort, StagedRecordDto};

use crate::elmer::approval::OutboxApproval;
use crate::elmer::events::{ElmerEvent, EV_CHIP, EV_OUTCOME, EV_TURN};
use crate::elmer::session::ElmerSession;
use crate::mcp_ports::FlushError;
use crate::session_log::SessionLogState;
use crate::ui_core::security_commands::EgressStatusDto;
use crate::ui_core::security::EgressGuard;

// ---------------------------------------------------------------------------
// DTOs — the React ↔ Rust serialization boundary
// ---------------------------------------------------------------------------

/// Serializable snapshot of the staged outbox (AC-3).
///
/// Matches the shape of [`StagedRecordDto`] but is a distinct type so the
/// Tauri command boundary is self-describing.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StagedRecordView {
    pub mid: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub body: String,
}

impl From<StagedRecordDto> for StagedRecordView {
    fn from(r: StagedRecordDto) -> Self {
        Self {
            mid: r.mid,
            to: r.to,
            cc: r.cc,
            subject: r.subject,
            body: r.body,
        }
    }
}

/// Serializable (and deserializable) outbox approval token that crosses the
/// Tauri boundary. Maps 1:1 to [`OutboxApproval`] — the internal token carries
/// no extra state that cannot be round-tripped.
///
/// `elmer_prepare_outbox_approval` serializes this OUT to React; `elmer_connect`
/// receives it IN from React. The digest + epoch + expiry are all included so
/// the Rust side can re-verify without keeping server-side session state beyond
/// what [`ElmerSession`] already holds.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboxApprovalDto {
    pub approval_id: String,
    pub digest: String,
    pub session_epoch: u64,
    pub expires_unix: u64,
}

impl From<OutboxApproval> for OutboxApprovalDto {
    fn from(a: OutboxApproval) -> Self {
        Self {
            approval_id: a.approval_id,
            digest: a.digest,
            session_epoch: a.session_epoch,
            expires_unix: a.expires_unix,
        }
    }
}

impl From<OutboxApprovalDto> for OutboxApproval {
    fn from(d: OutboxApprovalDto) -> Self {
        OutboxApproval {
            approval_id: d.approval_id,
            digest: d.digest,
            session_epoch: d.session_epoch,
            expires_unix: d.expires_unix,
        }
    }
}

// ---------------------------------------------------------------------------
// EventSink builder
// ---------------------------------------------------------------------------

/// Build an [`crate::elmer::session::EventSink`] that emits Tauri events via
/// the managed [`AppHandle`].
///
/// The sink clones the handle so it can be called from the spawned run task.
/// Emit failures are swallowed (fire-and-forget, matching the existing pattern
/// in `bootstrap.rs` / `session_log_emit.rs`).
fn make_event_sink(app: AppHandle) -> crate::elmer::session::EventSink {
    Arc::new(move |event: ElmerEvent| {
        let channel = match &event {
            ElmerEvent::Turn { .. } => EV_TURN,
            ElmerEvent::Chip { .. } => EV_CHIP,
            ElmerEvent::Outcome { .. } => EV_OUTCOME,
        };
        let _ = app.emit(channel, &event);
    })
}

// ---------------------------------------------------------------------------
// RunOutcome → ElmerEvent::Outcome mapper
// ---------------------------------------------------------------------------

fn outcome_to_event(outcome: &RunOutcome) -> ElmerEvent {
    let (outcome_kind, detail) = match outcome {
        RunOutcome::Completed(text) => ("done".into(), text.clone()),
        RunOutcome::Cancelled => ("cancelled".into(), String::new()),
        RunOutcome::NeedsOperator(msg) => ("needsOperator".into(), msg.clone()),
        RunOutcome::InvalidAction(msg) => ("invalidAction".into(), msg.clone()),
        RunOutcome::ToolDenied(msg) => ("toolDenied".into(), msg.clone()),
    };
    ElmerEvent::Outcome { outcome_kind, detail }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Send a user message to the Elmer agent.
///
/// **Single-flight (REJECT):** if a run is already in progress the command
/// returns an error immediately without blocking. The React caller should
/// disable the send input while a turn is in progress.
///
/// Emits [`EV_TURN`] / [`EV_CHIP`] / [`EV_OUTCOME`] events as the run
/// progresses. An [`EV_OUTCOME`] event is ALWAYS emitted at the end of the
/// run (success or failure), so the React pane can restore its input state.
#[tauri::command]
pub async fn elmer_send(
    msg: String,
    session: State<'_, Arc<ElmerSession>>,
    app: AppHandle,
) -> Result<(), String> {
    let sink = make_event_sink(app.clone());
    let session = Arc::clone(&session);
    let outcome = session.send(msg, sink).await;

    // Always emit the terminal outcome event so the pane can update its state.
    let _ = app.emit(EV_OUTCOME, outcome_to_event(&outcome));

    // NeedsOperator is the single-flight reject ("a turn is already running") and
    // bound/provider-error path — surface it as a command error. All other terminal
    // outcomes are reported via the EV_OUTCOME event the pane already consumes.
    match outcome {
        RunOutcome::NeedsOperator(msg) => Err(msg),
        _ => Ok(()),
    }
}

/// Stop the in-flight Elmer run (abort-first cancel, AC-4).
///
/// Fires the cancellation token + three ungated abort calls before awaiting
/// the run's terminus. Returns immediately after the abort cycle completes.
#[tauri::command]
pub async fn elmer_stop(session: State<'_, Arc<ElmerSession>>) {
    session.cancel_and_abort().await;
}

/// Rearm the egress guard and reset the session (AC-10).
///
/// Cancels any in-flight run, clears taint, arms the egress guard for
/// `duration_secs`, and resets the conversation. Returns the new egress
/// status so the React pane can update the arm indicator.
#[tauri::command]
pub async fn egress_rearm(
    duration_secs: u64,
    session: State<'_, Arc<ElmerSession>>,
    guard: State<'_, Arc<EgressGuard>>,
    log: State<'_, Arc<SessionLogState>>,
) -> Result<EgressStatusDto, String> {
    use crate::winlink_backend::{LogLevel, LogSource};

    if duration_secs == 0 {
        return Err("arm duration must be greater than zero".to_string());
    }

    let _deadline = session.rearm(duration_secs).await;

    log.append_operator_line(
        LogLevel::Info,
        LogSource::Backend,
        format!("[elmer] egress rearmed for {duration_secs}s"),
    );

    // Return the current guard state for the ribbon to reflect.
    let remaining = guard.armed_remaining();
    Ok(EgressStatusDto {
        armed: remaining > 0,
        armed_remaining_secs: remaining,
        tainted: guard.is_tainted(),
    })
}

/// List the staged outbox without touching the read-marker or taint (AC-3).
///
/// The React pane calls this to populate the approval review surface before
/// calling `elmer_prepare_outbox_approval`.
#[tauri::command]
pub async fn outbox_staged_list(
    outbox: State<'_, Arc<dyn OutboxReadPort + Send + Sync>>,
) -> Result<Vec<StagedRecordView>, String> {
    let records = outbox
        .list_staged()
        .await
        .map_err(|e| format!("outbox read failed: {e:?}"))?;
    Ok(records.into_iter().map(StagedRecordView::from).collect())
}

/// Freeze staging and issue a one-shot outbox approval token (AC-3 P0-3).
///
/// Reads the current outbox, computes a SHA-256 digest, sets
/// `staging_frozen = true` (denying the four compose tools while the
/// operator reviews), and returns an [`OutboxApprovalDto`] the React pane
/// holds until the operator confirms or dismisses.
///
/// On dismiss: call `elmer_stop` (which clears the freeze via rearm or cancel)
/// or call `elmer_connect` with the DTO (which clears the freeze on all paths).
#[tauri::command]
pub async fn elmer_prepare_outbox_approval(
    session: State<'_, Arc<ElmerSession>>,
) -> Result<OutboxApprovalDto, String> {
    const TTL_SECS: u64 = 300; // 5-minute approval window
    let approval = session
        .prepare_approval(TTL_SECS)
        .await
        .map_err(|e| format!("approval error: {e:?}"))?;
    Ok(OutboxApprovalDto::from(approval))
}

/// Flush the staged outbox through the digest-gated approval (AC-3 P1).
///
/// Re-reads the live outbox, recomputes the digest, and — ONLY on exact
/// match with the approval token — drives `EgressPort::cms_connect`. The
/// staging freeze is cleared on ALL exit paths (success, digest mismatch,
/// epoch change, expiry, or flush error) so the compose tools re-enable.
#[tauri::command]
pub async fn elmer_connect(
    approval: OutboxApprovalDto,
    session: State<'_, Arc<ElmerSession>>,
) -> Result<(), String> {
    let token = OutboxApproval::from(approval);
    session.connect_approved(token).await.map_err(|e| match e {
        FlushError::DigestMismatch => "outbox changed since approval — flush denied".to_string(),
        FlushError::EpochMismatch => "session epoch changed — re-arm and re-approve".to_string(),
        FlushError::Expired => "approval token expired — re-approve".to_string(),
        FlushError::Denied(msg) => format!("egress denied: {msg}"),
        FlushError::Failed(msg) => format!("flush failed: {msg}"),
        FlushError::ReadError(msg) => format!("outbox read error: {msg}"),
    })
}

// ---------------------------------------------------------------------------
// Security grep-gate tests (AC-8, AC-9, AC-5)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod security_gate_tests {
    /// AC-8 (P1-B widened): `EgressAuthority::Operator` must NOT appear in
    /// `src/elmer/` NOR in `src/mcp_ports.rs::approval_gated_flush`.
    ///
    /// This is a compile-time grep gate: if the literal appears in elmer/* or
    /// in the approval_gated_flush section, someone has directly minted Operator
    /// authority inside the agent path, bypassing the security boundary.
    #[test]
    fn no_operator_authority_in_elmer_or_flush() {
        // Grep the elmer source files for EgressAuthority::Operator.
        let elmer_src = [
            include_str!("commands.rs"),
            include_str!("events.rs"),
            include_str!("provider.rs"),
            include_str!("session.rs"),
            include_str!("approval.rs"),
            include_str!("executor.rs"),
            include_str!("mod.rs"),
        ];
        for (i, src) in elmer_src.iter().enumerate() {
            assert!(
                !src.contains("EgressAuthority::Operator"),
                "EgressAuthority::Operator found in elmer source #{i} — AC-8 violation"
            );
        }

        // Also check approval_gated_flush in mcp_ports.rs.
        let mcp_ports = include_str!("../mcp_ports.rs");
        // Find the approval_gated_flush section: everything from that fn to the
        // next pub fn / pub struct / end of file. We approximate by checking that
        // the literal doesn't appear anywhere in mcp_ports at the Elmer-reachable
        // flush helper — `guarded_egress` uses `EgressAuthority::Agent`, not Operator.
        // The full file check is safe because Operator authority legitimately
        // appears in the monolith-level egress port impl (which is NOT reachable
        // from Elmer's code path). We therefore only grep the `approval_gated_flush`
        // function body.
        let flush_start = mcp_ports
            .find("pub(crate) async fn approval_gated_flush")
            .unwrap_or(0);
        // Find the next `pub` item after the function start to bound the region.
        let flush_region = &mcp_ports[flush_start..];
        let flush_end = flush_region
            .find("\npub ")
            .or_else(|| flush_region.find("\n#["))
            .unwrap_or(flush_region.len());
        let flush_body = &flush_region[..flush_end];
        assert!(
            !flush_body.contains("EgressAuthority::Operator"),
            "EgressAuthority::Operator found in approval_gated_flush — AC-8 violation"
        );
    }

    /// AC-9: `clear_taint(` and `quarantine_and_rearm(` must have no callers
    /// in `src/elmer/` other than through `egress_rearm` → `session.rearm()`.
    ///
    /// The elmer source is not permitted to call these directly; the session
    /// wraps them in the single-flight + cancellation protocol.
    #[test]
    fn no_direct_clear_taint_or_quarantine_in_elmer() {
        let elmer_src = [
            include_str!("commands.rs"),
            include_str!("events.rs"),
            include_str!("provider.rs"),
            include_str!("approval.rs"),
            include_str!("executor.rs"),
            include_str!("mod.rs"),
            // session.rs IS the single caller — exclude it from the grep-gate.
        ];
        for (i, src) in elmer_src.iter().enumerate() {
            assert!(
                !src.contains("clear_taint("),
                "`clear_taint(` found in elmer source #{i} (not session.rs) — AC-9 violation"
            );
            assert!(
                !src.contains("quarantine_and_rearm("),
                "`quarantine_and_rearm(` found in elmer source #{i} (not session.rs) — AC-9 violation"
            );
        }
    }

    /// AC-5: No Elmer Tauri command deserializes `Vec<Message>` or
    /// `Conversation`. The agent's transcript is owned by `ElmerSession`;
    /// the React pane must never supply a transcript.
    ///
    /// This test guards against a future regression where someone adds a
    /// command parameter that could receive a React-crafted message list.
    #[test]
    fn no_command_deserializes_conversation_or_message_vec() {
        let commands_src = include_str!("commands.rs");
        // Neither the type name "Conversation" nor "Vec<Message>" should appear
        // as a Tauri command parameter type.
        assert!(
            !commands_src.contains("Vec<Message>"),
            "`Vec<Message>` found in commands.rs — AC-5 violation: agent transcript must not be operator-supplied"
        );
        // "Conversation" as a parameter type — the struct name alone is a proxy.
        // Note: it may appear in import paths; the check is for it as a bare
        // parameter type which would look like `: Conversation` or `Conversation,`.
        // We check for `State<'_, Conversation>` and `conversation: Conversation`
        // patterns:
        assert!(
            !commands_src.contains(": Conversation)") && !commands_src.contains(": Conversation,"),
            "`Conversation` found as a command parameter type in commands.rs — AC-5 violation"
        );
    }
}
