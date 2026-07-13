//! tuxlink-10bkw: backend→webview point-at bridge. New pattern for this
//! codebase: a keyed pending-ack map (request_id → oneshot) so the MCP tool
//! can await the frontend's honest outcome instead of fire-and-forget
//! (fire-and-forget makes Elmer confidently wrong — spec §Elmer point-at).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use tokio::sync::oneshot;

/// Event name the main webview listens on for a point-at request.
pub const POINT_AT_EVENT: &str = "onboarding:point-at";
/// How long the backend waits for the frontend's ack before treating the
/// request as timed out (window closed/minimized, overlay unresponsive, or no
/// listener wired up yet).
pub const ACK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// Payload emitted to the frontend on [`POINT_AT_EVENT`].
#[derive(Debug, Clone, serde::Serialize)]
pub struct PointAtRequest {
    pub request_id: u64,
    pub anchor_id: String,
}

/// The frontend's honest report of what happened for a given `request_id`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PointAtAck {
    /// `"shown"` | `"unknown-anchor"` | `"anchor-unmounted"` | `"overlay-busy"`.
    pub outcome: String,
    /// Present on `"unknown-anchor"` — the registry's valid anchor IDs.
    pub valid_ids: Option<Vec<String>>,
    /// Present on `"anchor-unmounted"` — the registry's "how to open this
    /// surface" line.
    pub open_hint: Option<String>,
}

/// Keyed pending-ack map: one oneshot sender per in-flight `point_at` request.
/// `register` mints a fresh id + receiver; `resolve` is called from the
/// `onboarding_point_at_ack` command when the frontend reports back; `forget`
/// drops a pending entry (used on emit failure and after a timeout) so a late
/// ack for an abandoned request is a documented no-op rather than a leak.
#[derive(Default)]
pub struct PointAtPending {
    next_id: AtomicU64,
    waiting: Mutex<HashMap<u64, oneshot::Sender<PointAtAck>>>,
}

impl PointAtPending {
    /// Mint a fresh request id and register its oneshot channel. The caller
    /// emits [`PointAtRequest { request_id, .. }`] to the frontend, then
    /// awaits the returned receiver (with a timeout).
    pub fn register(&self) -> (u64, oneshot::Receiver<PointAtAck>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.waiting
            .lock()
            .expect("point-at map poisoned")
            .insert(id, tx);
        (id, rx)
    }

    /// Deliver `ack` to the waiter registered for `id`, if any. Returns
    /// `false` when `id` is unknown (already resolved, forgotten after a
    /// timeout, or never registered) — a late ack from the frontend after the
    /// backend gave up is a no-op, never a panic.
    pub fn resolve(&self, id: u64, ack: PointAtAck) -> bool {
        match self.waiting.lock().expect("point-at map poisoned").remove(&id) {
            Some(tx) => tx.send(ack).is_ok(),
            None => false, // late ack after timeout cleanup — ignored
        }
    }

    /// Drop a pending entry without resolving it (emit failure, or the
    /// waiter timed out and is no longer listening). Idempotent.
    pub fn forget(&self, id: u64) {
        self.waiting.lock().expect("point-at map poisoned").remove(&id);
    }
}

/// Frontend ack for a point-at request. Late acks (post-timeout) are no-ops
/// (see [`PointAtPending::resolve`]).
#[tauri::command]
pub async fn onboarding_point_at_ack(
    request_id: u64,
    outcome: String,
    valid_ids: Option<Vec<String>>,
    open_hint: Option<String>,
    pending: tauri::State<'_, std::sync::Arc<PointAtPending>>,
) -> Result<(), ()> {
    pending.resolve(
        request_id,
        PointAtAck {
            outcome,
            valid_ids,
            open_hint,
        },
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ack(outcome: &str) -> PointAtAck {
        PointAtAck {
            outcome: outcome.to_string(),
            valid_ids: None,
            open_hint: None,
        }
    }

    #[tokio::test]
    async fn register_then_resolve_delivers_the_ack() {
        let pending = PointAtPending::default();
        let (id, rx) = pending.register();

        assert!(pending.resolve(id, ack("shown")));

        let delivered = rx.await.expect("the oneshot must deliver the resolved ack");
        assert_eq!(delivered.outcome, "shown");
    }

    #[tokio::test]
    async fn resolve_of_an_unknown_id_returns_false() {
        let pending = PointAtPending::default();
        // Never registered — resolving must report false, not panic.
        assert!(!pending.resolve(9999, ack("shown")));
    }

    #[tokio::test]
    async fn forget_prevents_a_later_resolve() {
        let pending = PointAtPending::default();
        let (id, rx) = pending.register();

        pending.forget(id);

        // A late resolve after forget must be a no-op (false), and the
        // receiver must observe the sender was dropped (Err), not hang.
        assert!(!pending.resolve(id, ack("shown")));
        assert!(rx.await.is_err());
    }

    #[tokio::test]
    async fn resolve_is_single_shot() {
        let pending = PointAtPending::default();
        let (id, rx) = pending.register();

        assert!(pending.resolve(id, ack("shown")));
        // The entry was removed on first resolve, so a second resolve for the
        // same id must report false rather than double-sending.
        assert!(!pending.resolve(id, ack("unknown-anchor")));

        let delivered = rx.await.expect("the first resolve must have delivered");
        assert_eq!(delivered.outcome, "shown");
    }
}
