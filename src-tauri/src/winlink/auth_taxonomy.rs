//! Pure-function CMS auth-response classifier. See spec §3 + §6.4.
//!
//! The classifier consumes a `***`-stripped CMS payload and returns a
//! `FailureMode` (or `Uncategorized`). Classification precedence (§6.4):
//!
//!   1. Mode 6 phrases (maintenance/rate-limit) — checked FIRST so they
//!      don't get absorbed by Mode 2/3/4 substring matches.
//!   2. Mode 2 ("Unknown client") wins over Mode 3/4.
//!   3. Mode 3 ("secure login failed") wins over Mode 4.
//!   4. Mode 4 strict phrase allowlist.
//!   5. Otherwise uncategorized.

use std::io;

use super::b2f_events::{FailureMode, TransportFailureKind};

/// Classify a `***`-stripped CMS payload. Case-insensitive matching.
pub fn classify(payload: &str) -> FailureMode {
    let lower = payload.to_lowercase();

    // §6.4 precedence: Mode 6 first (avoid being absorbed by other matches).
    const MODE6_PHRASES: &[&str] = &[
        "maintenance",
        "temporarily unavailable",
        "try again later",
        "server busy",
        "too many connections",
        "rate limit",
    ];
    if MODE6_PHRASES.iter().any(|p| lower.contains(p)) {
        return FailureMode::TemporaryServerUnavailability;
    }

    // Mode 2 (Unknown client) — distinct from Mode 3/4 in semantics.
    if lower.contains("unknown client") {
        return FailureMode::ClientRejected;
    }

    // Mode 3 (secure login failed) — wins over Mode 4 on co-occurrence.
    if lower.contains("secure login failed") {
        return FailureMode::PasswordRejected;
    }

    // Mode 4 — strict phrase allowlist (R5 revision, R1 #4 + R3 #7 finding).
    const MODE4_PHRASES: &[&str] = &[
        "callsign not authorized",
        "callsign not recognized",
        "callsign not recognised",
        "unknown callsign",
        "callsign denied",
        "callsign suspended",
        "callsign deactivated",
    ];
    if MODE4_PHRASES.iter().any(|p| lower.contains(p)) {
        return FailureMode::CallsignRejected;
    }

    FailureMode::Uncategorized
}

/// Classify a transport-layer `std::io::Error` (DNS / TCP / TLS).
pub fn classify_transport(err: &io::Error) -> TransportFailureKind {
    match err.kind() {
        io::ErrorKind::NotFound => TransportFailureKind::Dns,
        io::ErrorKind::ConnectionRefused => TransportFailureKind::TcpRefused,
        io::ErrorKind::TimedOut => TransportFailureKind::TcpTimeout,
        // rustls failures surface as io::Error::other with a chained source;
        // anything not in the kinds above and arriving during pre-handshake
        // is treated as TLS.
        _ => TransportFailureKind::TlsHandshake,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // === Mode 3 (password rejected) ===
    #[test]
    fn mode3_canonical_wl2k_go_fixture() {
        let s = "[1] Secure login failed - account password does not match. - Disconnecting (88.90.2.192)";
        assert_eq!(classify(s), FailureMode::PasswordRejected);
    }

    #[test]
    fn mode3_bare_secure_login_failed() {
        assert_eq!(classify("Secure login failed"), FailureMode::PasswordRejected);
    }

    #[test]
    fn mode3_case_insensitive() {
        assert_eq!(classify("SECURE LOGIN FAILED"), FailureMode::PasswordRejected);
    }

    // === Mode 2 (client rejected) ===
    #[test]
    fn mode2_unknown_client() {
        let s = "Unknown client types are not allowed on production servers - Disconnecting (88.89.220.254)";
        assert_eq!(classify(s), FailureMode::ClientRejected);
    }

    // === Mode 4 strict phrase allowlist ===
    #[test]
    fn mode4_callsign_not_authorized() {
        assert_eq!(classify("Callsign not authorized"), FailureMode::CallsignRejected);
    }

    #[test]
    fn mode4_callsign_not_recognized() {
        assert_eq!(classify("Callsign not recognized"), FailureMode::CallsignRejected);
    }

    #[test]
    fn mode4_unknown_callsign() {
        assert_eq!(classify("Unknown callsign"), FailureMode::CallsignRejected);
    }

    #[test]
    fn mode4_callsign_suspended() {
        assert_eq!(classify("Callsign suspended"), FailureMode::CallsignRejected);
    }

    // === Cross-mode precedence (R1 #4) ===
    #[test]
    fn mode3_wins_over_mode4_on_cooccurrence() {
        // The Mode 3 payload contains both "callsign" and "not" but Mode 3 wins.
        let s = "Callsign N7CPZ: secure login failed - account password does not match";
        assert_eq!(classify(s), FailureMode::PasswordRejected);
    }

    #[test]
    fn mode4_substring_not_matching_allowlist_falls_through_to_uncategorized() {
        // "callsign is fine" doesn't match the allowlist; falls through.
        let s = "Callsign is fine, but some other transient error";
        assert_eq!(classify(s), FailureMode::Uncategorized);
    }

    // === Mode 6 (maintenance / temporary unavailable) ===
    #[test]
    fn mode6_maintenance_window() {
        let s = "Maintenance window - CMS will return at 14:00 UTC. - Disconnecting";
        assert_eq!(classify(s), FailureMode::TemporaryServerUnavailability);
    }

    #[test]
    fn mode6_too_many_connections() {
        assert_eq!(classify("Too many connections from 88.90.2.192"), FailureMode::TemporaryServerUnavailability);
    }

    #[test]
    fn mode6_temporarily_unavailable() {
        assert_eq!(classify("Server temporarily unavailable - try again later"), FailureMode::TemporaryServerUnavailability);
    }

    // === Uncategorized fallback ===
    #[test]
    fn uncategorized_random_payload() {
        assert_eq!(classify("Some unknown error message"), FailureMode::Uncategorized);
    }

    #[test]
    fn uncategorized_empty_payload() {
        assert_eq!(classify(""), FailureMode::Uncategorized);
    }

    #[test]
    fn uncategorized_whitespace_only() {
        assert_eq!(classify("   \r\n   "), FailureMode::Uncategorized);
    }

    // === Transport classification ===
    #[test]
    fn transport_connection_refused_classifies_as_tcp_refused() {
        let err = io::Error::from(io::ErrorKind::ConnectionRefused);
        assert_eq!(classify_transport(&err), TransportFailureKind::TcpRefused);
    }

    #[test]
    fn transport_timed_out_classifies_as_tcp_timeout() {
        let err = io::Error::from(io::ErrorKind::TimedOut);
        assert_eq!(classify_transport(&err), TransportFailureKind::TcpTimeout);
    }

    #[test]
    fn transport_not_found_classifies_as_dns() {
        let err = io::Error::from(io::ErrorKind::NotFound);
        assert_eq!(classify_transport(&err), TransportFailureKind::Dns);
    }
}
