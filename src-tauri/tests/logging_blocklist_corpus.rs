//! Repo-derived field-name corpus test (spec §5.8).
//!
//! This test contains the curated list of field names actually used (or
//! plausibly-used) in tracing emission sites across src-tauri/src/. Each is
//! asserted to be EITHER correctly blocked or correctly allowed. New
//! credential-shaped names that land without blocklist updates fail this test.

use tuxlink_lib::logging::redact::should_redact_field;

/// Field names that MUST be blocked. Curated from grep of credential-related
/// callsites + the spec's §5.2 blocklist. When the implementation adds new
/// credential-adjacent fields, add them here.
const MUST_BLOCK: &[&str] = &[
    "password", "passwd", "pwd",
    "password_input", "peer_password", "station_password", "secure_response",
    "token", "auth_token", "access_token", "refresh_token", "oauth_token",
    "bearer", "bearer_token", "consent_token",
    "secret", "client_secret", "private_key", "api_key", "apikey",
    "auth", "authorization", "auth_header", "credential", "credentials",
    "secure_login_response", "secure_login_challenge", "challenge_response",
    "challenge", "response",
    "session_cookie", "sessionid", "session_id", "cookie",
    "signature", "nonce", "hmac", "salt",
    "keyring_value", "keyring_secret",
];

/// Plausibly-emitted field names that MUST pass through unredacted. Curated
/// from grep of non-credential emission sites.
const MUST_PASS: &[&str] = &[
    // Common operational fields
    "callsign", "gateway", "transport", "frequency_hz", "bandwidth",
    "attempt_id", "boot_id", "seq",
    "error", "error_kind", "error_count",
    "duration_ms", "elapsed_ms", "byte_count", "frame_count",
    "device", "port", "host", "address", "protocol",
    "level", "target", "module", "file", "line",
    // Plausible-but-benign names that look credential-shaped
    "password_hint_index", "challenge_round_number", "nonce_count_total",
    "key_event_handler", "cookie_jar_path", "auth_required_count",
    "token_count", "signature_validation_disabled", "salt_buffer_size",
    "credential_provider_name", "session_id_format_version",
];

#[test]
fn must_block_corpus_is_blocked() {
    for name in MUST_BLOCK {
        assert!(
            should_redact_field(name),
            "blocklist regression: {name} should be redacted but is not"
        );
    }
}

#[test]
fn must_pass_corpus_passes_through() {
    for name in MUST_PASS {
        assert!(
            !should_redact_field(name),
            "blocklist over-match: {name} should NOT be redacted but is"
        );
    }
}
