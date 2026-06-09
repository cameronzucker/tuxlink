//! Structured B2F events emitted by the session/handshake/telnet layers
//! for the smart auth-failure diagnostics. See spec §6.3.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::winlink::inbound_selection::PendingProposalDto;

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
    /// The CMS offered a batch of inbound proposals for operator review.
    ///
    /// SAFETY-CRITICAL: proposal strings MUST be redacted by the producer
    /// (`PendingProposalDto::from_proposal_redacted`). The
    /// `serde_lockdown_no_credential_fields_in_any_variant` test in this
    /// file's tests mod catches this.
    InboundProposalsOffered {
        request_id: u64,
        proposals: Vec<PendingProposalDto>,
        attempt_id: AttemptId,
    },
}

/// Sink trait the session/handshake/telnet layers emit through.
/// Send + Sync so the Tauri ui_commands can hold one in an Arc.
pub trait B2fEventSink: Send + Sync {
    fn push(&self, event: B2fEvent);
}

/// Sink that emits each pushed B2fEvent on the Tauri "b2f-event" channel.
/// Used by ui_commands to forward backend events to the React shell.
///
/// The struct is NOT cfg(test) — it's used by ui_commands.rs at runtime
/// to wire B2fEvent emission into the Tauri event channel that the React
/// useAuthDiagnostic hook (Task 18) subscribes to.
pub struct TauriEventSink {
    app: tauri::AppHandle,
}

impl TauriEventSink {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl B2fEventSink for TauriEventSink {
    fn push(&self, event: B2fEvent) {
        // Failures to emit are logged but don't propagate — the backend
        // continues even if the React side missed an event.
        if let Err(e) = tauri::Emitter::emit(&self.app, "b2f-event", event) {
            // Use eprintln since b2f_events.rs doesn't have a logger yet;
            // production code will plug in tracing once the wider tracing
            // strategy is settled.
            eprintln!("TauriEventSink: failed to emit b2f-event: {e}");
        }
    }
}

/// In-memory sink for unit tests. Records every push.
#[cfg(test)]
pub struct VecEventSink {
    inner: std::sync::Mutex<Vec<B2fEvent>>,
}

#[cfg(test)]
impl Default for VecEventSink {
    fn default() -> Self {
        Self { inner: std::sync::Mutex::new(Vec::new()) }
    }
}

#[cfg(test)]
impl VecEventSink {
    pub fn new() -> Self {
        Self::default()
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
    use crate::winlink::inbound_selection::PendingProposalDto;
    use crate::winlink::proposal::Proposal;

    fn clean_proposal() -> Proposal {
        Proposal {
            code: 'C',
            msg_type: "EM".to_string(),
            mid: "3F8KD2MABCDE".to_string(),
            size: 200,
            compressed_size: 80,
        }
    }

    // Test (a): InboundProposalsOffered serializes with all required fields.
    #[test]
    fn inbound_proposals_offered_serializes_correctly() {
        let dto = PendingProposalDto::from_proposal_redacted(&clean_proposal());
        let event = B2fEvent::InboundProposalsOffered {
            attempt_id: AttemptId(7),
            request_id: 3,
            proposals: vec![dto],
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"kind\":\"inbound_proposals_offered\""), "missing kind tag: {json}");
        assert!(json.contains("\"attempt_id\":7"), "missing attempt_id: {json}");
        assert!(json.contains("\"request_id\":3"), "missing request_id: {json}");
        assert!(json.contains("\"mid\""), "missing mid key: {json}");
        assert!(json.contains("\"uncompressed_size\""), "missing uncompressed_size key: {json}");
        assert!(json.contains("\"compressed_size\""), "missing compressed_size key: {json}");
    }

    // Test (b): SECURITY — a token-carrying MID does NOT appear in the serialized event.
    #[test]
    fn inbound_proposals_offered_redacts_credential_token_in_mid() {
        let poisoned_proposal = Proposal {
            code: 'C',
            msg_type: "EM".into(),
            mid: "X ;PR: 72768415".into(),
            size: 100,
            compressed_size: 50,
        };
        let dto = PendingProposalDto::from_proposal_redacted(&poisoned_proposal);
        let event = B2fEvent::InboundProposalsOffered {
            attempt_id: AttemptId(1),
            request_id: 99,
            proposals: vec![dto],
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(
            !json.contains("72768415"),
            "credential token leaked into serialized InboundProposalsOffered: {json}"
        );
    }

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
            B2fEvent::InboundProposalsOffered {
                attempt_id: AttemptId(1),
                request_id: 1,
                proposals: vec![PendingProposalDto::from_proposal_redacted(&clean_proposal())],
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
