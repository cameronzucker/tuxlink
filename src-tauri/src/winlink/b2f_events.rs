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
}
