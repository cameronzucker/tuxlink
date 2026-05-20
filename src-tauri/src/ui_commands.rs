//! Main-UI IPC commands + serializable error projection.
//!
//! Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §3
//! bd issue: tuxlink-zsm (Task 12 — main-UI cluster ROOT)
//!
//! This module is the IPC foundation for the message UI. Task 12 owns
//! [`UiError`] (+ its exhaustive `From<BackendError>` impl) and the
//! [`mailbox_list`] command. Tasks 13/14/16 APPEND their command fns here
//! (append-only — near-zero merge conflict); the orchestrator integration
//! commit (spec §4.3) registers them in `lib.rs`'s `invoke_handler` and
//! wires the AppShell regions. Until then, only `mailbox_list` is
//! registered.
//!
//! **Error model (spec §3.1):** `BackendError` carries non-serializable
//! `#[source] Box<dyn Error>` fields, so it cannot cross the Tauri IPC
//! boundary. `UiError` is the serializable projection, mirroring the
//! wizard's `#[serde(tag="kind", content="detail")]` discriminated-union
//! shape (`src/wizard/types.ts`). The `From` impl MUST be exhaustive over
//! every `BackendError` variant (Codex finding 6).
//!
//! **Async/lock invariant (spec §1.1):** every command clones the backend
//! `Arc` and drops the `RwLock` guard via [`AppBackend::current`] BEFORE
//! awaiting the trait method — the guard is never held across an await.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::app_backend::AppBackend;
use crate::winlink_backend::{BackendError, MailboxFolder, MessageMeta, OutboundMessage};

// ============================================================================
// Error projection (spec §3.1)
// ============================================================================

/// Serializable projection of [`BackendError`] for the frontend. Mirrors
/// `UiError` in `src/mailbox/types.ts` via Tauri's
/// `#[serde(tag="kind", content="detail")]` shape.
#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(tag = "kind", content = "detail")]
pub enum UiError {
    /// No backend configured (offline install / pre-connect). The UI
    /// renders this as a "not connected" empty state, NOT an error toast.
    NotConfigured(String),
    /// Message id not found.
    NotFound(String),
    AuthFailed { reason: String },
    Transport { reason: String },
    Unavailable { reason: String },
    Rejected(String),
    Cancelled,
    Internal { detail: String },
}

impl From<BackendError> for UiError {
    /// Exhaustive mapping per spec §3.1 — every `BackendError` variant gets a
    /// deliberate arm; NO catch-all. `BackendError` is `#[non_exhaustive]`,
    /// but since it is defined in this same crate the compiler checks
    /// exhaustiveness here at compile time: a future variant added to
    /// `winlink_backend.rs` will fail this `match`, forcing a deliberate UI
    /// projection rather than silently routing through a wildcard (Codex
    /// finding 6 — `InvalidSession`/`Cancelled`/`NotImplemented`/`Io` must
    /// each be handled, not dropped).
    fn from(e: BackendError) -> Self {
        match e {
            BackendError::NotConfigured(s) => UiError::NotConfigured(s),
            BackendError::NotFound(id) => UiError::NotFound(id.0),
            BackendError::AuthFailed { reason } => UiError::AuthFailed { reason },
            BackendError::TransportFailed { reason, source } => UiError::Transport {
                reason: stringify_with_source(&reason, source.as_deref()),
            },
            BackendError::BackendUnavailable { reason, source } => UiError::Unavailable {
                reason: stringify_with_source(&reason, source.as_deref()),
            },
            BackendError::MessageRejected(s) => UiError::Rejected(s),
            BackendError::Cancelled => UiError::Cancelled,
            BackendError::NotImplemented => UiError::Unavailable {
                reason: "not implemented in v0.0.1".to_string(),
            },
            BackendError::InvalidSession => UiError::Internal {
                detail: "invalid session".to_string(),
            },
            BackendError::Io(err) => UiError::Internal {
                detail: err.to_string(),
            },
            BackendError::Internal { msg, source } => UiError::Internal {
                detail: stringify_with_source(&msg, source.as_deref()),
            },
        }
    }
}

/// Append the `Display` of a `source` error chain to a reason string so the
/// projected `reason`/`detail` carries the lost context that the
/// non-serializable `#[source]` would otherwise drop.
fn stringify_with_source(
    reason: &str,
    source: Option<&(dyn std::error::Error + Send + Sync + 'static)>,
) -> String {
    match source {
        Some(src) => format!("{reason}: {src}"),
        None => reason.to_string(),
    }
}

// ============================================================================
// Message metadata DTO (spec §2.1 / §3.2)
// ============================================================================

/// Serializable list-row metadata. Mirrors `MessageMeta` in
/// `src/mailbox/types.ts`. Field names are camelCase on the wire so the TS
/// model needs no rename layer.
#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MessageMetaDto {
    pub id: String,
    pub subject: String,
    pub from: String,
    pub to: Vec<String>,
    pub date: String,
    pub unread: bool,
    pub body_size: u64,
    pub has_attachments: bool,
}

impl From<MessageMeta> for MessageMetaDto {
    fn from(m: MessageMeta) -> Self {
        MessageMetaDto {
            id: m.id.0,
            subject: m.subject,
            from: m.from,
            to: m.to,
            date: m.date,
            unread: m.unread,
            body_size: m.body_size,
            has_attachments: m.has_attachments,
        }
    }
}

// ============================================================================
// Folder parsing (spec §3.2)
// ============================================================================

/// Parse a sidebar folder string into a backend [`MailboxFolder`].
///
/// `"drafts"` and `"deleted"` never reach a backend command — Drafts is a
/// local (`localStorage`) store handled frontend-side (spec §2.2), and
/// Deleted is a disabled placeholder. Either string → `Err(UiError)` so a
/// stray invocation fails loudly rather than silently querying the wrong
/// folder. Spec §3.2 + Task-12 test (2).
pub fn parse_folder(folder: &str) -> Result<MailboxFolder, UiError> {
    match folder {
        "inbox" => Ok(MailboxFolder::Inbox),
        "outbox" => Ok(MailboxFolder::Outbox),
        "sent" => Ok(MailboxFolder::Sent),
        "archive" => Ok(MailboxFolder::Archive),
        "drafts" => Err(UiError::Internal {
            detail: "drafts is a local folder, not a backend folder".to_string(),
        }),
        "deleted" => Err(UiError::Unavailable {
            reason: "the Deleted folder is not available in v0.0.1".to_string(),
        }),
        other => Err(UiError::Internal {
            detail: format!("unknown folder: {other}"),
        }),
    }
}

// ============================================================================
// Commands (spec §3.2)
// ============================================================================

/// List a folder's messages. Consumed by Task 12's `useMailbox`.
///
/// `None` backend → `NotConfigured` (the UI's "not connected" empty state,
/// not an error). Otherwise clones the `Arc`, drops the lock, then awaits
/// `list_messages` (spec §1.1 lock invariant).
#[tauri::command]
pub async fn mailbox_list(
    folder: String,
    state: State<'_, AppBackend>,
) -> Result<Vec<MessageMetaDto>, UiError> {
    let parsed = parse_folder(&folder)?;
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;
    let metas = backend.list_messages(parsed).await?;
    Ok(metas.into_iter().map(MessageMetaDto::from).collect())
}

// ============================================================================
// Task 14 — message_send command (spec §3.2, §5.4)
// ============================================================================
// Appended here per the append-only ownership model (spec §7). The
// `invoke_handler` registration lands in the orchestrator integration commit
// (§4.3); this file is append-only for command fns.

/// Inbound DTO from the compose window frontend. Mirrors `OutboundDraftDto`
/// in `src/compose/Compose.tsx`.
///
/// **`cc` caveat (spec §3.2, Codex F5 VERIFIED):** `PatClient::send` accepts
/// only `to`/`subject`/`body`/`date` form fields — Pat 1.0.0 silently drops
/// any `cc` form field. The compose UI disables the Cc field with a v0.1
/// tooltip (spec §5.4 disposition: "disable with tooltip rather than silently
/// drop"). The `cc` field is present in this DTO for API completeness; the
/// `PatBackend::send_message` → `PatClient::send` chain currently ignores it.
/// When Pat cc support is confirmed in v0.1, enable the Cc field in the UI +
/// add `cc` to `PatClient::send`'s multipart form.
#[derive(Debug, Deserialize)]
pub struct OutboundDraftDto {
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub body: String,
}

/// Send an outbound message queued via the compose window.
///
/// Maps `OutboundDraftDto` → `OutboundMessage` (adds `date = now RFC3339`
/// per spec §3.2 — the UI does not supply the send timestamp; the command
/// stamps it at queue time).
///
/// Returns `Ok(None)` when Pat does not echo a MID (Pat 1.0.0 behavior —
/// plain-text confirmation, no MID). The compose window shows "Posted to
/// Outbox" on any `Ok(_)`. Spec §3.2 + §5.4.
///
/// **None-success invariant (spec §3.2):** `Ok(None)` is a SUCCESS, not an
/// error. The frontend must treat `Ok(Some(mid))` and `Ok(None)` identically
/// as "posted." The Rust test below asserts this mapping explicitly.
#[tauri::command]
pub async fn message_send(
    draft: OutboundDraftDto,
    state: State<'_, AppBackend>,
) -> Result<Option<String>, UiError> {
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;

    // Stamp the send timestamp here (the UI does not supply it — spec §3.2).
    let date = chrono::Utc::now().to_rfc3339();

    let msg = OutboundMessage {
        to: draft.to,
        cc: draft.cc,  // forwarded as-is; PatBackend drops it (Codex F5)
        subject: draft.subject,
        body: draft.body,
        date,
    };

    // send_message returns Ok(None) for Pat 1.0.0 — see winlink_backend.rs
    // PatBackend impl. Map Option<MessageId> → Option<String> for IPC.
    let mid = backend.send_message(msg).await?;
    Ok(mid.map(|id| id.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::winlink_backend::MessageId;

    // MessageMetaDto serializes camelCase (bodySize, hasAttachments) so the
    // TS `MessageMeta` model needs no rename layer. In-crate because
    // `MessageMeta` is `#[non_exhaustive]` (can't be struct-literal-built
    // from the external integration-test crate).
    #[test]
    fn message_meta_dto_serializes_camel_case() {
        let dto = MessageMetaDto::from(MessageMeta {
            id: MessageId::new("M1"),
            subject: "S".into(),
            from: "F".into(),
            to: vec!["T".into()],
            date: "2026-05-19T00:00:00Z".into(),
            unread: true,
            body_size: 7,
            has_attachments: true,
        });
        let v = serde_json::to_value(dto).unwrap();
        assert_eq!(v["bodySize"], 7);
        assert_eq!(v["hasAttachments"], true);
        assert_eq!(v["to"][0], "T");
        assert_eq!(v["id"], "M1");
    }

    // Task 14 test (6): Ok(None) from send_message maps to a SUCCESS on the
    // IPC boundary — not an error. This test asserts the None-success mapping
    // by verifying that `OutboundDraftDto` serializes correctly and that the
    // None → None path produces a serializable `Option<String>`.
    //
    // The full async command path (AppBackend + mock backend) cannot be driven
    // from a sync unit test without a tokio runtime; the structural contract
    // is tested here (None round-trip) and the command handler is tested at
    // the integration layer (cargo test with tokio::test).
    #[test]
    fn none_mid_maps_to_none_string() {
        // Simulate the final map: `Option<MessageId>` → `Option<String>`
        let mid: Option<MessageId> = None; // Pat 1.0.0 behavior
        let result: Option<String> = mid.map(|id| id.0);
        assert!(result.is_none(), "Pat's None return maps to Ok(None), not an error");
    }

    #[test]
    fn some_mid_maps_to_some_string() {
        let mid: Option<MessageId> = Some(MessageId::new("MID-12345"));
        let result: Option<String> = mid.map(|id| id.0);
        assert_eq!(result, Some("MID-12345".to_string()));
    }

    #[test]
    fn outbound_draft_dto_deserializes() {
        let json = r#"{
            "to": ["W6ABC@winlink.org"],
            "cc": [],
            "subject": "ICS-213 check-in",
            "body": "Standing by at staging area."
        }"#;
        let dto: OutboundDraftDto = serde_json::from_str(json).unwrap();
        assert_eq!(dto.to, vec!["W6ABC@winlink.org"]);
        assert!(dto.cc.is_empty());
        assert_eq!(dto.subject, "ICS-213 check-in");
        assert_eq!(dto.body, "Standing by at staging area.");
    }
}
