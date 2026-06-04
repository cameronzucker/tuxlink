//! Structured B2F events emitted by the session/handshake/telnet layers
//! for the smart auth-failure diagnostics. See spec §6.3.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

/// Monotonic per-attempt correlation ID. Every event from one
/// cms_connect / cms_connect_test invocation shares the same AttemptId.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct AttemptId(pub u64);

impl AttemptId {
    /// Mint a fresh process-monotonic AttemptId. Used at the top of
    /// cms_connect / cms_connect_test.
    pub fn fresh() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        AttemptId(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransportFailureKind {
    Dns,
    TcpRefused,
    TcpTimeout,
    TlsHandshake,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionPhase {
    PreHandshake,
    DuringHandshake,
    PostHandshake,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FailureMode {
    NetworkUnreachable,
    ClientRejected,
    PasswordRejected,
    CallsignRejected,
    SessionDroppedAfterAuth,
    TemporaryServerUnavailability,
    Uncategorized,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum B2fEvent {
    TcpConnected { host: String, port: u16, attempt_id: AttemptId },
    TlsHandshakeStarted { attempt_id: AttemptId },
    TlsHandshakeCompleted { attempt_id: AttemptId },
    RemoteSidReceived { sid: String, attempt_id: AttemptId },
    /// `;PQ:` received. The VALUE is intentionally absent (privacy §6.1).
    /// SAFETY-CRITICAL: do NOT add a `challenge: String` field here.
    /// The serde-lockdown test in this file's tests mod catches this.
    SecureChallengeReceived { attempt_id: AttemptId },
    /// `;PR:` sent. The VALUE is intentionally absent (privacy §6.1).
    SecureResponseSent { attempt_id: AttemptId },
    /// Proves the CMS accepted our handshake — emitted when the first
    /// non-`***` `F`-prefixed protocol byte is received from the server.
    /// Mode 5 discriminator (spec §6.4) requires this; without it, a
    /// `;PR`-rejected drop mis-classifies as "credentials are fine."
    PostAuthExchangeStarted { attempt_id: AttemptId },
    /// `*** ...` line received during handshake or exchange. The `raw`
    /// field is pre-scrubbed by `redaction::redact_freeform`.
    RemoteErrorReceived { raw: String, attempt_id: AttemptId },
    /// (Legacy/back-compat) The handshake completed at the protocol level.
    /// Kept for test fixtures but NOT used as the Mode 5 discriminator —
    /// use PostAuthExchangeStarted instead.
    HandshakeCompleted { attempt_id: AttemptId },
    ConnectionClosed {
        phase: ConnectionPhase,
        transport_kind: Option<TransportFailureKind>,
        attempt_id: AttemptId,
    },
    AuthClassified {
        mode: FailureMode,
        raw: Option<String>,
        attempt_id: AttemptId,
    },
}

/// Sink trait the session/handshake/telnet layers emit through.
/// Send + Sync so the Tauri ui_commands can hold one in an Arc.
pub trait B2fEventSink: Send + Sync {
    fn push(&self, event: B2fEvent);
}

/// In-memory sink for unit tests. Records every push.
#[cfg(test)]
pub struct VecEventSink {
    inner: std::sync::Mutex<Vec<B2fEvent>>,
}

#[cfg(test)]
impl VecEventSink {
    pub fn new() -> Self {
        Self { inner: std::sync::Mutex::new(Vec::new()) }
    }
    pub fn snapshot(&self) -> Vec<B2fEvent> {
        self.inner.lock().unwrap().clone()
    }
}

#[cfg(test)]
impl B2fEventSink for VecEventSink {
    fn push(&self, event: B2fEvent) {
        self.inner.lock().unwrap().push(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attempt_ids_are_monotonic_within_a_process() {
        let a = AttemptId::fresh();
        let b = AttemptId::fresh();
        let c = AttemptId::fresh();
        assert!(b.0 > a.0);
        assert!(c.0 > b.0);
    }

    #[test]
    fn failure_mode_serializes_as_snake_case() {
        let json = serde_json::to_string(&FailureMode::PasswordRejected).unwrap();
        assert_eq!(json, "\"password_rejected\"");
    }

    #[test]
    fn b2f_event_remote_error_received_serializes_with_kind_tag() {
        let event = B2fEvent::RemoteErrorReceived {
            raw: "Unknown client types are not allowed".to_string(),
            attempt_id: AttemptId(42),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"kind\":\"remote_error_received\""), "got: {json}");
        assert!(json.contains("\"attempt_id\":42"));
    }

    #[test]
    fn serde_lockdown_no_credential_fields_in_any_variant() {
        // R2 #11: future maintainer might add a debug `challenge` field
        // to SecureChallengeReceived; this test fails before such a
        // change can land, catching the privacy regression.
        let variants = vec![
            B2fEvent::TcpConnected { host: "x".into(), port: 1, attempt_id: AttemptId(1) },
            B2fEvent::TlsHandshakeStarted { attempt_id: AttemptId(1) },
            B2fEvent::TlsHandshakeCompleted { attempt_id: AttemptId(1) },
            B2fEvent::RemoteSidReceived { sid: "B2FHM$".into(), attempt_id: AttemptId(1) },
            B2fEvent::SecureChallengeReceived { attempt_id: AttemptId(1) },
            B2fEvent::SecureResponseSent { attempt_id: AttemptId(1) },
            B2fEvent::PostAuthExchangeStarted { attempt_id: AttemptId(1) },
            B2fEvent::RemoteErrorReceived { raw: "x".into(), attempt_id: AttemptId(1) },
            B2fEvent::HandshakeCompleted { attempt_id: AttemptId(1) },
            B2fEvent::ConnectionClosed {
                phase: ConnectionPhase::PostHandshake,
                transport_kind: None,
                attempt_id: AttemptId(1),
            },
            B2fEvent::AuthClassified {
                mode: FailureMode::PasswordRejected,
                raw: None,
                attempt_id: AttemptId(1),
            },
        ];
        for v in variants {
            let json = serde_json::to_string(&v).unwrap();
            let lower = json.to_lowercase();
            // Check for field names, not substrings (e.g., "challenge": not "...challenge_received")
            for forbidden in ["\"challenge\":", "\"response\":", "\"pq\":", "\"pr\":", "\"token\":", "\"password\":"] {
                assert!(
                    !lower.contains(forbidden),
                    "variant {v:?} serialized a forbidden field: {forbidden} -> {json}"
                );
            }
        }
    }

    #[test]
    fn event_sink_records_calls_in_order() {
        let sink = VecEventSink::new();
        sink.push(B2fEvent::TcpConnected {
            host: "cms-z.winlink.org".into(),
            port: 8772,
            attempt_id: AttemptId(1),
        });
        sink.push(B2fEvent::HandshakeCompleted { attempt_id: AttemptId(1) });
        assert_eq!(sink.snapshot().len(), 2);
    }
}
