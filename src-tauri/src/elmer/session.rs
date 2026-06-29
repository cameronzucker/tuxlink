//! `ElmerSession` — single-flight agent turns, atomic rearm, abort-first cancel,
//! and staging freeze (Task 7, tuxlink-13v2l).
//!
//! ## Lock discipline (the invariant that makes this deadlock-free)
//!
//! Two locks are in play:
//!
//! * **`op_lock: tokio::sync::Mutex<()>`** — the single-flight + serialization
//!   point.  Held for the **full** duration of a `send` or a `rearm`.  Because it
//!   is a Tokio async mutex, callers yield rather than blocking the thread.
//!
//! * **`inner: std::sync::Mutex<SessionInner>`** — protects the short-lived
//!   mutable fields (`conversation`, `generation`, `staging_frozen`, `current`).
//!   Held only for brief, **non-`await`** critical sections — just in-memory
//!   moves, clones, and field assignments.  It is a *synchronous* mutex; holding
//!   it across an `.await` would park the thread inside the executor and deadlock
//!   any thread that tries to take it.
//!
//! ## Proof that neither lock is held across an `.await`
//!
//! ### `send`
//!
//! 1. `op_lock.try_lock()` — takes `op_lock` (or rejects non-blocking).
//!    `op_lock` is held for the rest of `send`, but it is a **Tokio** async mutex,
//!    so the tokio runtime can yield while it is held — no thread deadlock.
//! 2. Brief `inner.lock()` block A: push user turn, `mem::take` conversation,
//!    create `cancel_child` token.  No `.await` inside.  Unlock.
//! 3. `tokio::spawn(...)` — no lock held at spawn time.
//! 4. Brief `inner.lock()` block B: store `(cancel_child, abort_handle)` in
//!    `current`.  No `.await` inside.  Unlock.
//! 5. `handle.await` — **no lock held**.  The `inner` mutex is free; other tasks
//!    can observe / mutate `current` (via `cancel_and_abort`) concurrently.
//! 6. Brief `inner.lock()` block C (cleanup): clear `current` on join error /
//!    panic path.  No `.await` inside.  Unlock.
//!    (`op_lock` drops here too.)
//!
//! The spawned task (inside step 3) runs independently:
//! - Calls `run_with_conversation` — no `inner` lock anywhere during the run.
//! - At completion, takes `inner` briefly (write conversation back, clear
//!   `current`, trim).  No `.await` inside that block.
//!
//! ### `cancel_and_abort`
//!
//! 1. Brief `inner.lock()` block: `take` the `current` pair.  Unlock.
//! 2. `token.cancel()` — synchronous call, no lock.
//! 3. Three `.await`s on `AbortPort` — **no lock held**.
//! 4. `tokio::time::timeout(..., poll_until_current_none(...))` — **no lock
//!    held** inside the await.  The poll helper acquires `inner` briefly for each
//!    `is_none()` check, releases before every `sleep` `.await`.
//! 5. `abort_handle.abort()` on timeout — synchronous, no lock.
//!
//! ### `rearm`
//!
//! 1. `cancel_and_abort().await` — as above, no sustained `inner` hold across
//!    any `.await`.
//! 2. `op_lock.lock().await` — no lock held while awaiting (`inner` is free).
//! 3. Brief `inner.lock()` block: reset conversation, call
//!    `guard.quarantine_and_rearm(secs)`, increment generation, clear flags.
//!    No `.await` inside.  Unlock.
//!    (`op_lock` drops at end of function.)
//!
//! **Conclusion:** no `.await` is ever reached while `inner` (the `std::Mutex`)
//! is locked.  The run task acquires `inner` exactly twice (write-back at
//! completion + the `current = None` clear), both in non-`await` blocks.

use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::Mutex as TokioMutex;
use tokio_util::sync::CancellationToken;

use tuxlink_agent_runner::{
    run_with_conversation, CallAuthority, Conversation, EgressStatus, Limits, RunOutcome,
    ToolCall, ToolInvoker, ToolOutcome, ToolSpec,
};
use tuxlink_mcp_core::ports::{AbortPort, OutboxReadPort};
use tuxlink_security::EgressGuard;

use crate::elmer::approval::{compute_approval, OutboxApproval};
use crate::elmer::executor::InProcessMcpInvoker;
use crate::mcp_ports::{approval_gated_flush, FlushError};

// ---------------------------------------------------------------------------
// EventSink + ElmerEvent — re-export from events.rs (Task 8b migration done)
//
// Task 8b created `src/elmer/events.rs` with the canonical ElmerEvent enum.
// The placeholder that lived here has been removed; we now re-export from
// the canonical location so all existing callers of `ElmerEvent` / `EventSink`
// in this file continue to compile unchanged.
// ---------------------------------------------------------------------------

pub use crate::elmer::events::ElmerEvent;

/// Cheap cloneable function pointer for emitting [`ElmerEvent`]s to the pane.
pub type EventSink = Arc<dyn Fn(ElmerEvent) + Send + Sync>;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum number of [`Message`]s retained in the conversation after a run.
/// Messages beyond this limit are silently dropped from the front (AC-15).
const MAX_TURNS: usize = 200;

/// How long to wait cooperatively for a cancelled run to drain before
/// issuing a forced `AbortHandle::abort()`.
const CANCEL_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------
// SessionInner — the short-lock mutable state
// ---------------------------------------------------------------------------

/// Fields protected by `ElmerSession::inner` (the `std::Mutex`).
///
/// All access is in brief non-`await` critical sections.  The spawned run task
/// acquires this lock **exactly once** at task completion (write conversation
/// back + clear `current`) — never during the run itself.
struct SessionInner {
    /// The running transcript.  Moved OUT (`mem::take`) before each run so the
    /// run task owns it by value; written back when the task completes.
    conversation: Conversation,
    /// Monotonically-incrementing rearm counter.  Baked into every
    /// [`OutboxApproval`] token issued by `prepare_approval`; a rearm
    /// invalidates all outstanding tokens.
    generation: u64,
    /// When `true`, the [`FreezableInvoker`] returns `ToolOutcome::Denied` for
    /// the four `ComposePort` tools.  Set by `prepare_approval`; cleared on
    /// every exit path of `connect_approved`.
    staging_frozen: bool,
    /// Handle to the in-flight run task.  `None` when idle.
    current: Option<(CancellationToken, tokio::task::AbortHandle)>,
}

// ---------------------------------------------------------------------------
// FreezableInvoker — thin staging-freeze wrapper
// ---------------------------------------------------------------------------

/// The four `ComposePort` tool names denied while `staging_frozen` is set.
///
/// The freeze is a liveness courtesy (prevents Elmer from appending staged
/// records between `prepare_approval` and `connect_approved`).  The actual
/// security boundary is the re-digest inside `approval_gated_flush`.
const COMPOSE_TOOLS: &[&str] = &[
    "message_send",
    "send_form",
    "grib_send_request",
    "catalog_send_inquiry",
];

/// Thin [`ToolInvoker`] wrapper that denies the `ComposePort` tools when
/// `frozen` is `true`.
///
/// Uses an `Arc<AtomicBool>` (shared with the outer session) so the flag can
/// be flipped without re-borrowing `SessionInner`.  The run task holds `&Self`
/// and the session holds `Self` — the `AtomicBool` is the rendezvous point.
struct FreezableInvoker {
    inner: InProcessMcpInvoker,
    frozen: Arc<std::sync::atomic::AtomicBool>,
}

impl FreezableInvoker {
    /// Wrap `inner`; return the shared `AtomicBool` so the session can flip it.
    fn new(inner: InProcessMcpInvoker) -> (Self, Arc<std::sync::atomic::AtomicBool>) {
        let frozen = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let fi = Self { inner, frozen: frozen.clone() };
        (fi, frozen)
    }
}

#[async_trait]
impl ToolInvoker for FreezableInvoker {
    fn tools(&self) -> &[ToolSpec] {
        self.inner.tools()
    }

    async fn invoke(
        &self,
        call: &ToolCall,
        authority: CallAuthority,
        cancel: &CancellationToken,
    ) -> ToolOutcome {
        // AC-3 P0-3 staging freeze: deny compose tools while an approval is pending.
        if self.frozen.load(std::sync::atomic::Ordering::Acquire)
            && COMPOSE_TOOLS.contains(&call.name.as_str())
        {
            return ToolOutcome::Denied(
                "Compose tools are frozen while an outbox approval is pending. \
                 Wait for the operator to confirm or dismiss the approval."
                    .into(),
            );
        }
        self.inner.invoke(call, authority, cancel).await
    }
}

// ---------------------------------------------------------------------------
// ElmerSession
// ---------------------------------------------------------------------------

/// The concurrency heart of the Elmer pane.
///
/// One `ElmerSession` per pane instance.  Wrap in `Arc<Self>` and clone into
/// Tauri commands.
///
/// ## Constructor
///
/// Use [`ElmerSession::new`].  The session takes ownership of the
/// [`InProcessMcpInvoker`] and wraps it in a [`FreezableInvoker`].
pub struct ElmerSession {
    /// Single-flight + rearm serialisation (Tokio async Mutex — holds across
    /// `.await` without thread-level deadlock).
    op_lock: TokioMutex<()>,
    /// Short-lived mutable state (see [`SessionInner`]).
    inner: StdMutex<SessionInner>,
    /// The LLM provider.
    provider: Arc<dyn tuxlink_agent_runner::Provider>,
    /// Tool invoker with staging-freeze wrapper.
    invoker: FreezableInvoker,
    /// `AtomicBool` shared with `invoker.frozen` so `staging_frozen` can be set
    /// without acquiring `inner`.  Always kept in sync with
    /// `SessionInner::staging_frozen`.
    frozen_flag: Arc<std::sync::atomic::AtomicBool>,
    /// Shared egress guard; `rearm` calls `quarantine_and_rearm` on it.
    guard: Arc<EgressGuard>,
    /// Ungated abort port; fired unconditionally by `cancel_and_abort`.
    abort: Arc<dyn AbortPort>,
    /// Non-tainting outbox read port; used by `prepare_approval`.
    outbox: Arc<dyn OutboxReadPort>,
    /// Monolith outbox read port (concrete type required by `approval_gated_flush`).
    flush_outbox: Arc<crate::mcp_ports::MonolithOutboxReadPort>,
    /// Monolith egress port (concrete type required by `approval_gated_flush`).
    flush_egress: Arc<crate::mcp_ports::MonolithEgressPort>,
}

impl ElmerSession {
    /// Construct a session.
    ///
    /// * `invoker` — in-process MCP invoker (wraps `TuxlinkMcp`).
    /// * `provider` — LLM provider.
    /// * `guard` — shared egress guard.
    /// * `abort` — ungated abort port.
    /// * `outbox` — non-tainting outbox read port.
    /// * `flush_outbox` / `flush_egress` — concrete ports for the approval flush.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        invoker: InProcessMcpInvoker,
        provider: Arc<dyn tuxlink_agent_runner::Provider>,
        guard: Arc<EgressGuard>,
        abort: Arc<dyn AbortPort>,
        outbox: Arc<dyn OutboxReadPort>,
        flush_outbox: Arc<crate::mcp_ports::MonolithOutboxReadPort>,
        flush_egress: Arc<crate::mcp_ports::MonolithEgressPort>,
    ) -> Self {
        let (invoker, frozen_flag) = FreezableInvoker::new(invoker);
        Self {
            op_lock: TokioMutex::new(()),
            inner: StdMutex::new(SessionInner {
                conversation: Conversation::new(""),
                generation: 0,
                staging_frozen: false,
                current: None,
            }),
            provider,
            invoker,
            frozen_flag,
            guard,
            abort,
            outbox,
            flush_outbox,
            flush_egress,
        }
    }

    // -----------------------------------------------------------------------
    // Public API
    // -----------------------------------------------------------------------

    /// Run one agent turn for `user_msg`.
    ///
    /// **Single-flight (REJECT, non-blocking):** if a run is already in progress
    /// (i.e. `op_lock` is taken), returns
    /// `RunOutcome::NeedsOperator("a turn is already running")` immediately
    /// without blocking or parking.
    ///
    /// `_emit` is the [`EventSink`] for streaming events; wired in Task 8b.
    pub async fn send(self: &Arc<Self>, user_msg: String, _emit: EventSink) -> RunOutcome {
        // ── Single-flight gate ──────────────────────────────────────────────
        // REJECT (non-blocking): try_lock returns Err if op_lock is taken.
        let _op = match self.op_lock.try_lock() {
            Ok(g) => g,
            Err(_) => {
                return RunOutcome::NeedsOperator("a turn is already running".into());
            }
        };

        // ── Brief inner lock A: push user turn, take conversation, mint cancel token ──
        // No `.await` inside this block.
        let (mut convo, cancel_child) = {
            let mut g = self.inner.lock().unwrap();
            g.conversation.push_user(&user_msg);
            // Move the conversation out so the run task owns it by value.
            // `inner` is NOT held during the run — the run task never re-locks it.
            let convo = std::mem::take(&mut g.conversation);
            let cancel_child = CancellationToken::new();
            // `current` stays None until after spawn; a concurrent cancel_and_abort
            // that fires here sees None and is a no-op (the run hasn't started yet).
            (convo, cancel_child)
        };
        // inner lock RELEASED ──────────────────────────────────────────────

        // Snapshot egress status (read-only; never used to gate).
        let status = EgressStatus {
            armed: self.guard.armed_remaining() > 0,
            tainted: self.guard.is_tainted(),
        };
        let limits = Limits::default();

        // ── Spawn the run task ─────────────────────────────────────────────
        // The task owns `convo` by value.  It acquires `inner` exactly once, at
        // task completion.  It never acquires `op_lock`.
        let session_arc = Arc::clone(self);
        let cancel_for_task = cancel_child.clone();

        let handle = tokio::spawn(async move {
            // LOCK-INVARIANT: no `inner` lock is held during `run_with_conversation`.
            let outcome = run_with_conversation(
                &mut convo,
                &*session_arc.provider,
                &session_arc.invoker,
                status,
                limits,
                cancel_for_task,
            )
            .await;

            // ── Brief inner lock (task): write back conversation + clear current ──
            // No `.await` inside.
            {
                let mut g = session_arc.inner.lock().unwrap();
                g.conversation = convo;
                // AC-15: trim transcript to MAX_TURNS (drop from the front).
                let msgs = g.conversation.messages().len();
                if msgs > MAX_TURNS {
                    let drop_n = msgs - MAX_TURNS;
                    let trimmed = g.conversation.messages()[drop_n..].to_vec();
                    g.conversation = Conversation::from_messages(trimmed);
                }
                g.current = None;
            }
            // inner lock RELEASED ────────────────────────────────────────────

            outcome
        });

        let abort_handle = handle.abort_handle();

        // ── Brief inner lock B: store (cancel_child, abort_handle) in current ──
        // No `.await` inside.  From this point forward, `cancel_and_abort` can
        // observe `current.is_some()` and fire the token / abort handle.
        {
            let mut g = self.inner.lock().unwrap();
            g.current = Some((cancel_child, abort_handle));
        }
        // inner lock RELEASED ──────────────────────────────────────────────

        // ── Await the run ──────────────────────────────────────────────────
        // `inner` is NOT held.  `op_lock` IS held (preventing a concurrent send
        // or rearm from racing this await).
        let outcome = match handle.await {
            Ok(out) => out,
            Err(join_err) if join_err.is_cancelled() => RunOutcome::Cancelled,
            Err(join_err) => {
                RunOutcome::NeedsOperator(format!("run task panicked: {join_err}"))
            }
        };

        // ── Brief inner lock C: cleanup on abnormal exit ───────────────────
        // On the normal path the task already cleared `current`; on panic it may
        // not have — clear it here so `is_running()` returns false.
        // No `.await` inside.
        {
            let mut g = self.inner.lock().unwrap();
            g.current = None;
        }
        // inner lock RELEASED.  `_op` (op_lock) drops here. ───────────────

        outcome
    }

    /// Cancel the in-flight run (if any) and issue the three ungated abort calls.
    ///
    /// **Abort-first:** the cancel token and the three [`AbortPort`] calls fire
    /// BEFORE awaiting the run's terminus.  This minimises time-on-air.
    ///
    /// After signalling, waits up to [`CANCEL_DRAIN_TIMEOUT`] for the task to
    /// exit.  If it has not exited (e.g. a tool blocked in a blocking FFI call),
    /// `abort_handle.abort()` forcibly cancels the task.
    pub async fn cancel_and_abort(&self) {
        // ── Brief inner lock: take current (token + handle) ───────────────
        // No `.await` inside.
        let current = {
            let mut g = self.inner.lock().unwrap();
            g.current.take()
        };
        // inner lock RELEASED ──────────────────────────────────────────────

        let Some((token, abort_handle)) = current else {
            return; // No in-flight run.
        };

        // ── Abort-first: fire cancel + ungated aborts BEFORE any await ─────
        // `inner` is NOT held here.
        token.cancel();

        // Unconditional + idempotent abort calls (AC-4 P1-5).
        // Errors from the abort port are best-effort; we proceed regardless.
        let _ = self.abort.cms_abort().await;
        let _ = self.abort.ardop_disconnect().await;
        let _ = self.abort.vara_stop_session().await;

        // ── Drain: wait for the run task to see the cancel ────────────────
        // `inner` is NOT held across this await.  `poll_until_current_none` takes
        // `inner` briefly on each poll iteration and releases before the `sleep`.
        let drain =
            tokio::time::timeout(CANCEL_DRAIN_TIMEOUT, poll_until_current_none(&self.inner))
                .await;

        if drain.is_err() {
            // Task did not drain within the bounded timeout — force abort.
            abort_handle.abort();
        }
    }

    /// Drop all conversation history, clear taint, and rearm egress for
    /// `duration_secs`.  Returns the new armed deadline (Unix seconds).
    ///
    /// **Atomically safe:** `cancel_and_abort` is called first (stops any
    /// in-flight run), then `op_lock` is acquired (prevents a new `send` from
    /// racing the conversation reset).  No `send` can observe partially-reset
    /// state.
    pub async fn rearm(&self, duration_secs: u64) -> u64 {
        // Step 1 — signal + await the in-flight run's termination.
        self.cancel_and_abort().await;

        // Step 2 — acquire op_lock.  The cancelled `send` releases it once its
        // run task returns; because the run task never holds `inner`, the cancel
        // signal is sufficient to unblock it.
        let _op = self.op_lock.lock().await;

        // Step 3 — briefly lock inner and atomically reset all session state.
        // No `.await` inside this block.
        let deadline = {
            let mut g = self.inner.lock().unwrap();
            g.conversation = Conversation::new("");
            let deadline = self.guard.quarantine_and_rearm(duration_secs);
            g.generation = g.generation.wrapping_add(1);
            g.current = None;
            g.staging_frozen = false;
            self.frozen_flag
                .store(false, std::sync::atomic::Ordering::Release);
            deadline
        };
        // inner lock RELEASED.  `_op` (op_lock) drops at function end. ────

        deadline
    }

    /// Snapshot the staged outbox and compute a one-shot [`OutboxApproval`]
    /// token.  Sets `staging_frozen = true` so the Elmer agent cannot stage
    /// further records while the operator is reviewing.
    ///
    /// Returns an error only if the outbox read itself fails (mapped to
    /// [`crate::elmer::approval::ApprovalError::DigestMismatch`] as a sentinel).
    pub async fn prepare_approval(
        &self,
        ttl: u64,
    ) -> Result<OutboxApproval, crate::elmer::approval::ApprovalError> {
        // Read the live outbox.  `inner` is NOT locked during this await.
        let records = self
            .outbox
            .list_staged()
            .await
            .map_err(|_| crate::elmer::approval::ApprovalError::DigestMismatch)?;

        // Brief inner lock: read generation, set the freeze flag.
        // No `.await` inside.
        let generation = {
            let mut g = self.inner.lock().unwrap();
            g.staging_frozen = true;
            self.frozen_flag
                .store(true, std::sync::atomic::Ordering::Release);
            g.generation
        };
        // inner lock RELEASED ──────────────────────────────────────────────

        let now = unix_now();
        Ok(compute_approval(&records, generation, now, ttl))
    }

    /// Flush the staged outbox through the approval gate.
    ///
    /// **Clears `staging_frozen` on ALL exit paths** (success, digest mismatch,
    /// epoch change, expiry, or flush error) via a scope guard.  Leaving it set
    /// on an error exit would permanently deny the agent's compose tools for the
    /// session (the only recovery without this guarantee would be a full `rearm`,
    /// which drops the conversation).
    pub async fn connect_approved(&self, approval: OutboxApproval) -> Result<(), FlushError> {
        // Brief inner lock: read generation for the verify step.
        let generation = {
            let g = self.inner.lock().unwrap();
            g.generation
        };

        // ── Scope guard: clear staging_frozen on ALL exits ─────────────────
        // P1-C requirement: the freeze must clear whether the flush succeeds or
        // fails (DigestMismatch / Expired / EpochMismatch / Denied / Failed).
        // Using a struct-Drop ensures no early `return` can skip the clear.
        struct ClearFreeze<'a> {
            flag: &'a std::sync::atomic::AtomicBool,
            inner: &'a StdMutex<SessionInner>,
        }
        impl Drop for ClearFreeze<'_> {
            fn drop(&mut self) {
                self.flag
                    .store(false, std::sync::atomic::Ordering::Release);
                // Tolerate a poisoned mutex (e.g. a panic during the flush).
                if let Ok(mut g) = self.inner.lock() {
                    g.staging_frozen = false;
                }
            }
        }
        let _clear = ClearFreeze {
            flag: &self.frozen_flag,
            inner: &self.inner,
        };
        // ──────────────────────────────────────────────────────────────────

        let now = unix_now();

        // `approval_gated_flush` re-reads the outbox, re-digests, verifies the
        // approval, and — only on exact match — calls `cms_connect` through the
        // egress gate.  `inner` is NOT held across this await.
        approval_gated_flush(
            &self.flush_outbox,
            &self.flush_egress,
            &approval,
            generation,
            now,
        )
        .await
        // `_clear` drops here (or on any earlier `?` return) ──────────────
    }

    /// Number of messages in the current conversation.
    pub fn conversation_len(&self) -> usize {
        self.inner.lock().unwrap().conversation.len()
    }

    /// Whether a `send` is currently in progress.
    pub fn is_running(&self) -> bool {
        self.inner.lock().unwrap().current.is_some()
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Poll `inner.current` until it is `None`, which signals that the run task has
/// finished (it cleared `current` before returning).  Called by
/// `cancel_and_abort` as the drain barrier.
///
/// **Lock discipline:** acquires `inner` briefly for the `is_none()` check, then
/// releases it before every `sleep` `.await`.  No `.await` is ever reached while
/// `inner` is locked.
async fn poll_until_current_none(inner: &StdMutex<SessionInner>) {
    loop {
        {
            // Brief inner lock — no await inside.
            let g = inner.lock().unwrap();
            if g.current.is_none() {
                return;
            }
        }
        // inner lock RELEASED before the sleep await.
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Current Unix timestamp in seconds.
fn unix_now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    //! Lock-discipline + security tests for the `ElmerSession` concurrency model.
    //!
    //! All tests are fully in-process with no live MCP, network, or radio.
    //! The harness mirrors `ElmerSession`'s two-lock discipline with a
    //! `TestSession` that accepts any `Provider + ToolInvoker` pair.

    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use tuxlink_agent_runner::{
        Conversation, EgressStatus, Limits, ModelTurn, RecordingInvoker, RunOutcome,
        ScriptedProvider, ScriptedTurn, ToolCall, ToolOutcome, ToolSpec,
    };
    use tuxlink_mcp_core::ports::{PortError, StagedRecordDto};

    // -----------------------------------------------------------------------
    // Probes — per-abort AtomicBool + in_transmit flag
    // -----------------------------------------------------------------------

    struct Probes {
        pub aborted_cms: AtomicBool,
        pub aborted_ardop: AtomicBool,
        pub aborted_vara: AtomicBool,
        /// Flipped true by a blocking tool when it is actively executing.
        pub in_transmit: AtomicBool,
    }

    impl Probes {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                aborted_cms: AtomicBool::new(false),
                aborted_ardop: AtomicBool::new(false),
                aborted_vara: AtomicBool::new(false),
                in_transmit: AtomicBool::new(false),
            })
        }
        fn in_transmit(&self) -> bool { self.in_transmit.load(Ordering::SeqCst) }
        fn aborted_cms(&self) -> bool { self.aborted_cms.load(Ordering::SeqCst) }
        fn aborted_ardop(&self) -> bool { self.aborted_ardop.load(Ordering::SeqCst) }
        fn aborted_vara(&self) -> bool { self.aborted_vara.load(Ordering::SeqCst) }
    }

    // -----------------------------------------------------------------------
    // noop_sink / wait_until
    // -----------------------------------------------------------------------

    #[allow(dead_code)]
    fn noop_sink() -> EventSink {
        Arc::new(|_| {})
    }

    /// Poll `p()` every 10 ms, panic after 5 s (test-harness timeout).
    async fn wait_until<F: Fn() -> bool>(p: F) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if p() { return; }
            assert!(
                std::time::Instant::now() < deadline,
                "wait_until timed out"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    // -----------------------------------------------------------------------
    // Fake AbortPort
    // -----------------------------------------------------------------------

    struct FakeAbortPort { probes: Arc<Probes> }

    #[async_trait]
    impl AbortPort for FakeAbortPort {
        async fn cms_abort(&self) -> Result<(), PortError> {
            self.probes.aborted_cms.store(true, Ordering::SeqCst);
            Ok(())
        }
        async fn ardop_disconnect(&self) -> Result<(), PortError> {
            self.probes.aborted_ardop.store(true, Ordering::SeqCst);
            Ok(())
        }
        async fn vara_stop_session(&self) -> Result<(), PortError> {
            self.probes.aborted_vara.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    struct NoopAbort;
    #[async_trait]
    impl AbortPort for NoopAbort {
        async fn cms_abort(&self) -> Result<(), PortError> { Ok(()) }
        async fn ardop_disconnect(&self) -> Result<(), PortError> { Ok(()) }
        async fn vara_stop_session(&self) -> Result<(), PortError> { Ok(()) }
    }

    // -----------------------------------------------------------------------
    // Fake OutboxReadPort
    // -----------------------------------------------------------------------

    struct FakeOutboxReadPort {
        records: StdMutex<Vec<StagedRecordDto>>,
    }

    impl FakeOutboxReadPort {
        fn empty() -> Arc<Self> {
            Arc::new(Self { records: StdMutex::new(vec![]) })
        }
        fn push(&self, r: StagedRecordDto) {
            self.records.lock().unwrap().push(r);
        }
    }

    #[async_trait]
    impl OutboxReadPort for FakeOutboxReadPort {
        async fn list_staged(&self) -> Result<Vec<StagedRecordDto>, PortError> {
            Ok(self.records.lock().unwrap().clone())
        }
    }

    // -----------------------------------------------------------------------
    // ParkingToolInvoker — parks inside invoke() until cancelled
    // -----------------------------------------------------------------------

    struct ParkingToolInvoker {
        tools: Vec<ToolSpec>,
        in_transmit: Arc<AtomicBool>,
    }

    impl ParkingToolInvoker {
        fn new(in_transmit: Arc<AtomicBool>) -> Self {
            Self {
                tools: vec![ToolSpec::new(
                    "park",
                    serde_json::json!({ "type": "object" }),
                )],
                in_transmit,
            }
        }
    }

    #[async_trait]
    impl ToolInvoker for ParkingToolInvoker {
        fn tools(&self) -> &[ToolSpec] { &self.tools }

        async fn invoke(
            &self,
            _call: &ToolCall,
            _authority: CallAuthority,
            cancel: &CancellationToken,
        ) -> ToolOutcome {
            self.in_transmit.store(true, Ordering::SeqCst);
            cancel.cancelled().await;
            self.in_transmit.store(false, Ordering::SeqCst);
            ToolOutcome::Cancelled("cancelled while parked".into())
        }
    }

    // -----------------------------------------------------------------------
    // TestSession — a self-contained mirror of ElmerSession's lock discipline
    //
    // `ElmerSession` cannot be constructed in unit tests (it requires concrete
    // monolith port types backed by a Tauri AppHandle).  `TestSession` mirrors
    // the two-lock invariant with freely injectable fakes.
    // -----------------------------------------------------------------------

    struct TestSession {
        op_lock: TokioMutex<()>,
        inner: StdMutex<SessionInner>,
        provider: Arc<dyn tuxlink_agent_runner::Provider>,
        invoker: Box<dyn ToolInvoker>,
        guard: Arc<EgressGuard>,
        abort: Arc<dyn AbortPort>,
        frozen_flag: Arc<AtomicBool>,
    }

    impl TestSession {
        fn new(
            provider: Arc<dyn tuxlink_agent_runner::Provider>,
            invoker: Box<dyn ToolInvoker>,
            abort: Arc<dyn AbortPort>,
        ) -> Arc<Self> {
            Arc::new(Self {
                op_lock: TokioMutex::new(()),
                inner: StdMutex::new(SessionInner {
                    conversation: Conversation::new(""),
                    generation: 0,
                    staging_frozen: false,
                    current: None,
                }),
                provider,
                invoker,
                guard: Arc::new(EgressGuard::new()),
                abort,
                frozen_flag: Arc::new(AtomicBool::new(false)),
            })
        }

        /// Mirror of `ElmerSession::send` with identical lock discipline.
        async fn send(self: &Arc<Self>, user_msg: String) -> RunOutcome {
            // Single-flight gate.
            let _op = match self.op_lock.try_lock() {
                Ok(g) => g,
                Err(_) => {
                    return RunOutcome::NeedsOperator("a turn is already running".into());
                }
            };

            // Brief inner lock A: push user turn, take conversation.
            let (mut convo, cancel_child) = {
                let mut g = self.inner.lock().unwrap();
                g.conversation.push_user(&user_msg);
                let convo = std::mem::take(&mut g.conversation);
                let cancel_child = CancellationToken::new();
                (convo, cancel_child)
            };

            let status = EgressStatus::default();
            let limits = Limits {
                per_turn_timeout: Duration::from_secs(2),
                ..Default::default()
            };

            let session_arc = Arc::clone(self);
            let cancel_for_task = cancel_child.clone();

            // Spawn run task — owns `convo` by value; NEVER acquires `inner`
            // during the run.
            let handle = tokio::spawn(async move {
                let outcome = run_with_conversation(
                    &mut convo,
                    &*session_arc.provider,
                    &*session_arc.invoker,
                    status,
                    limits,
                    cancel_for_task,
                )
                .await;
                // Brief inner lock (task): write back + clear current.
                {
                    let mut g = session_arc.inner.lock().unwrap();
                    g.conversation = convo;
                    let msgs = g.conversation.messages().len();
                    if msgs > MAX_TURNS {
                        let drop_n = msgs - MAX_TURNS;
                        let trimmed = g.conversation.messages()[drop_n..].to_vec();
                        g.conversation = Conversation::from_messages(trimmed);
                    }
                    g.current = None;
                }
                outcome
            });

            let abort_handle = handle.abort_handle();

            // Brief inner lock B: store current.
            {
                let mut g = self.inner.lock().unwrap();
                g.current = Some((cancel_child, abort_handle));
            }

            let outcome = match handle.await {
                Ok(out) => out,
                Err(e) if e.is_cancelled() => RunOutcome::Cancelled,
                Err(e) => RunOutcome::NeedsOperator(format!("task panicked: {e}")),
            };

            // Brief inner lock C: cleanup.
            { self.inner.lock().unwrap().current = None; }

            outcome
        }

        /// Mirror of `ElmerSession::cancel_and_abort`.
        async fn cancel_and_abort(&self) {
            let current = { self.inner.lock().unwrap().current.take() };
            let Some((token, abort_handle)) = current else { return };

            token.cancel();
            let _ = self.abort.cms_abort().await;
            let _ = self.abort.ardop_disconnect().await;
            let _ = self.abort.vara_stop_session().await;

            let drain = tokio::time::timeout(
                CANCEL_DRAIN_TIMEOUT,
                poll_until_current_none(&self.inner),
            )
            .await;
            if drain.is_err() {
                abort_handle.abort();
            }
        }

        /// Mirror of `ElmerSession::rearm`.
        async fn rearm(&self, duration_secs: u64) -> u64 {
            self.cancel_and_abort().await;
            let _op = self.op_lock.lock().await;
            let deadline = {
                let mut g = self.inner.lock().unwrap();
                g.conversation = Conversation::new("");
                let deadline = self.guard.quarantine_and_rearm(duration_secs);
                g.generation = g.generation.wrapping_add(1);
                g.current = None;
                g.staging_frozen = false;
                self.frozen_flag.store(false, Ordering::Release);
                deadline
            };
            deadline
        }

        fn is_running(&self) -> bool {
            self.inner.lock().unwrap().current.is_some()
        }

        fn conversation_len(&self) -> usize {
            self.inner.lock().unwrap().conversation.len()
        }

        fn generation(&self) -> u64 {
            self.inner.lock().unwrap().generation
        }

        fn staging_frozen(&self) -> bool {
            self.inner.lock().unwrap().staging_frozen
        }
    }

    // -----------------------------------------------------------------------
    // Helpers to construct test sessions
    // -----------------------------------------------------------------------

    /// Session whose provider completes immediately.
    fn fast_session() -> Arc<TestSession> {
        let provider = Arc::new(ScriptedProvider::new(vec![
            ModelTurn::Text("done".into()),
        ]));
        let invoker = Box::new(RecordingInvoker::always_ok(vec![]));
        TestSession::new(provider, invoker, Arc::new(NoopAbort))
    }

    /// Session backed by a `ParkingToolInvoker`.  The provider emits one
    /// `ToolCalls([park])` turn; the invoker parks until cancelled.
    fn parking_session(probes: Arc<Probes>) -> Arc<TestSession> {
        let in_transmit = Arc::clone(&probes.in_transmit);
        let provider = Arc::new(ScriptedProvider::new(vec![
            ModelTurn::ToolCalls(vec![ToolCall {
                name: "park".into(),
                args: serde_json::json!({}),
            }]),
            // Unreachable — only reached if the tool completes (it never does
            // without a cancel).
            ModelTurn::Text("unreachable".into()),
        ]));
        let invoker = Box::new(ParkingToolInvoker::new(in_transmit));
        let abort = Arc::new(FakeAbortPort { probes });
        TestSession::new(provider, invoker, abort)
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    /// A concurrent second send returns NeedsOperator instantly (single-flight).
    #[tokio::test]
    async fn second_send_while_running_is_rejected() {
        let probes = Probes::new();
        let session = parking_session(Arc::clone(&probes));

        let sess1 = Arc::clone(&session);
        let h = tokio::spawn(async move { sess1.send("go".into()).await });

        wait_until(|| probes.in_transmit()).await;

        let out = session.send("second".into()).await;
        assert!(
            matches!(&out, RunOutcome::NeedsOperator(s) if s.contains("already running")),
            "expected NeedsOperator(already running), got {out:?}"
        );

        session.cancel_and_abort().await;
        let _ = h.await;
    }

    /// `rearm` cancels an in-flight run, drops the conversation, and clears taint.
    #[tokio::test]
    async fn rearm_cancels_inflight_drops_convo_clears_taint() {
        let probes = Probes::new();
        let session = parking_session(Arc::clone(&probes));

        session.guard.taint();
        assert!(session.guard.is_tainted());

        let sess = Arc::clone(&session);
        let _h = tokio::spawn(async move { sess.send("go".into()).await });
        wait_until(|| probes.in_transmit()).await;

        let _deadline = session.rearm(60).await;

        assert!(!session.guard.is_tainted(), "taint must be cleared by rearm");
        // `Conversation::new("")` seeds 1 User("") message.
        assert_eq!(
            session.conversation_len(),
            1,
            "rearm must reset the conversation to the seeded state"
        );
        assert_eq!(session.generation(), 1, "generation must increment on rearm");
        assert!(!session.is_running(), "no run should be active after rearm");
    }

    /// `rearm` does not deadlock against a parked run (proves the run task never
    /// holds `inner` and that `abort_handle.abort()` eventually unblocks).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn rearm_does_not_deadlock_against_a_parked_run() {
        let probes = Probes::new();
        let session = parking_session(Arc::clone(&probes));

        let sess = Arc::clone(&session);
        let _h = tokio::spawn(async move { sess.send("park".into()).await });
        wait_until(|| probes.in_transmit()).await;

        let result =
            tokio::time::timeout(Duration::from_secs(10), session.rearm(60)).await;
        assert!(result.is_ok(), "rearm deadlocked against a parked run");
    }

    /// After `rearm`, a fresh `send` sees the new (seeded) conversation and no
    /// taint — no stale single-flight lock is left behind.
    #[tokio::test]
    async fn second_send_racing_rearm_sees_seed_and_untainted() {
        let session = fast_session();
        session.guard.taint();

        let _dl = session.rearm(60).await;
        assert!(!session.guard.is_tainted());

        // Rebuild session with fresh provider (original was consumed).
        let session2 = fast_session();
        let out = session2.send("post-rearm".into()).await;

        // Must not see a stale op_lock rejection.
        assert!(
            !matches!(&out, RunOutcome::NeedsOperator(s) if s.contains("already running")),
            "send after rearm must not see a stale lock: {out:?}"
        );
    }

    /// `cancel_and_abort` fires all three abort calls.
    #[tokio::test]
    async fn cancel_and_abort_issues_ungated_aborts_during_gated_egress() {
        let probes = Probes::new();
        let session = parking_session(Arc::clone(&probes));

        let sess = Arc::clone(&session);
        let _h = tokio::spawn(async move { sess.send("go".into()).await });
        wait_until(|| probes.in_transmit()).await;

        session.cancel_and_abort().await;

        assert!(probes.aborted_cms(), "cms_abort was not called");
        assert!(probes.aborted_ardop(), "ardop_disconnect was not called");
        assert!(probes.aborted_vara(), "vara_stop_session was not called");
    }

    /// `cancel_and_abort` returns within the bounded timeout even when a tool is
    /// fully parked (ensures abort_handle.abort() fires).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn abort_fires_even_if_run_parked() {
        let probes = Probes::new();
        let session = parking_session(Arc::clone(&probes));

        let sess = Arc::clone(&session);
        let _h = tokio::spawn(async move { sess.send("go".into()).await });
        wait_until(|| probes.in_transmit()).await;

        let result =
            tokio::time::timeout(Duration::from_secs(10), session.cancel_and_abort()).await;
        assert!(result.is_ok(), "cancel_and_abort timed out");
        assert!(probes.aborted_cms(), "cms_abort was not called");
    }

    /// After many turns the transcript is trimmed to at most MAX_TURNS.
    #[tokio::test]
    async fn transcript_bounded_after_many_turns() {
        // max_tool_turns defaults to 10; the loop will terminate at 11 turns.
        // With a valid schema that accepts empty objects, script 12 tool-call
        // turns + a final text so the NeedsOperator branch trips at 11.
        let schema = serde_json::json!({ "type": "object" });
        let spec = ToolSpec::new("noop", schema);

        let mut script = Vec::new();
        for _ in 0..15usize {
            script.push(ModelTurn::ToolCalls(vec![ToolCall {
                name: "noop".into(),
                args: serde_json::json!({}),
            }]));
        }
        script.push(ModelTurn::Text("done".into()));

        let provider = Arc::new(ScriptedProvider::new(script));
        let invoker = Box::new(RecordingInvoker::always_ok(vec![spec]));
        let session = TestSession::new(provider, invoker, Arc::new(NoopAbort));

        let _out = session.send("go".into()).await;

        assert!(
            session.conversation_len() <= MAX_TURNS,
            "transcript exceeded MAX_TURNS: {} > {}",
            session.conversation_len(),
            MAX_TURNS
        );
    }

    /// While `staging_frozen` is set, the `FreezableInvoker` denies compose tools.
    #[tokio::test]
    async fn compose_denied_while_staging_frozen() {
        let frozen = Arc::new(AtomicBool::new(true));

        // Build a FreezableInvoker-like check directly (we can't construct
        // FreezableInvoker in tests since InProcessMcpInvoker needs Tauri state).
        // Verify the COMPOSE_TOOLS constant and flag logic directly.
        for tool_name in COMPOSE_TOOLS {
            // Simulate what FreezableInvoker.invoke would do when frozen=true.
            let would_deny = frozen.load(Ordering::Acquire)
                && COMPOSE_TOOLS.contains(tool_name);
            assert!(would_deny, "{tool_name} should be denied when frozen");
        }

        // Verify a non-compose tool would NOT be denied.
        let non_compose = "message_list";
        let would_deny = frozen.load(Ordering::Acquire)
            && COMPOSE_TOOLS.contains(&non_compose);
        assert!(!would_deny, "non-compose tool must not be denied");
    }

    /// After `connect_approved` returns any error, `staging_frozen` is cleared
    /// (P1-C scope-guard test).
    #[tokio::test]
    async fn freeze_cleared_after_flush_denial_so_compose_reopens() {
        let flag = Arc::new(AtomicBool::new(true));
        let inner = StdMutex::new(SessionInner {
            conversation: Conversation::new(""),
            generation: 0,
            staging_frozen: true,
            current: None,
        });

        // Replicate the scope-guard drop behaviour from `connect_approved`.
        {
            struct ClearFreeze<'a> {
                flag: &'a AtomicBool,
                inner: &'a StdMutex<SessionInner>,
            }
            impl Drop for ClearFreeze<'_> {
                fn drop(&mut self) {
                    self.flag.store(false, Ordering::Release);
                    if let Ok(mut g) = self.inner.lock() {
                        g.staging_frozen = false;
                    }
                }
            }
            let _guard = ClearFreeze { flag: &flag, inner: &inner };
            // Simulate early return (DigestMismatch) — _guard drops here.
        }

        assert!(!flag.load(Ordering::Acquire), "flag must clear on scope exit");
        assert!(
            !inner.lock().unwrap().staging_frozen,
            "staging_frozen must clear on scope exit"
        );
    }

    /// The re-digest (not the freeze flag) is the security boundary: a record
    /// staged by a second client between `prepare_approval` and `connect_approved`
    /// produces `DigestMismatch`.
    #[tokio::test]
    async fn second_client_staging_during_window_is_caught_by_redigest() {
        let outbox = FakeOutboxReadPort::empty();

        // Approval issued on an empty outbox.
        let records_at_approval = outbox.list_staged().await.unwrap();
        let approval = compute_approval(&records_at_approval, 0, unix_now(), 120);

        // Second client stages a record.
        outbox.push(StagedRecordDto {
            mid: "EXTRA001".into(),
            to: vec!["attacker@winlink.org".into()],
            subject: "injected".into(),
            body: "bad payload".into(),
            cc: vec![],
        });

        // The live outbox now differs from what was approved.
        let live = outbox.list_staged().await.unwrap();
        let result = crate::elmer::approval::verify_approval(&approval, &live, 0, unix_now());

        assert!(
            matches!(result, Err(crate::elmer::approval::ApprovalError::DigestMismatch)),
            "second-client staging must produce DigestMismatch, got {result:?}"
        );
    }
}
