//! Integration test: wire sanitizer blocks credential leakage through tracing.
//!
//! Spec §5.6, plan acceptance §10.2 #14.
//!
//! Verifies that the `sanitize_wire_line` helper used at wire-emission callsites
//! redacts `;PR:` token bytes BEFORE they could reach a tracing subscriber.
//! The test does NOT need a live subscriber — it directly exercises the sanitizer
//! function that all handshake wire-emission callsites must call.

use tuxlink_lib::logging::wire_sanitize::{sanitize_wire_line, WireContext};
use tuxlink_lib::winlink::secure::secure_login_response;

/// The reference vectors from wl2k-go secure_test.go. We pre-compute the token
/// so the test knows what byte sequence must NOT appear in the sanitized output.
///
/// challenge = "23753528", password = "FOOBAR" → token = "72768415"
#[test]
fn wire_sanitizer_redacts_pr_token_before_emission() {
    let challenge = "23753528";
    let password = "FOOBAR";

    // Compute the real token (the function is deterministic).
    let token = secure_login_response(challenge, password);
    assert_eq!(token, "72768415", "pre-condition: token computation matches reference");

    // Build the ;PR: line as handshake.rs would.
    let response_line = format!(";PR: {token}\r");

    // Sanitize via the Generic context (same as handshake.rs callsite).
    let sanitized = sanitize_wire_line(&response_line, WireContext::Generic);

    // CRITICAL: the 8-digit token must NOT appear in the sanitized output.
    assert!(
        !sanitized.contains(&token),
        "WIRE LEAK: sanitized output contains the ;PR: token bytes {:?}; got: {:?}",
        token,
        sanitized,
    );

    // The ;PR: prefix must be preserved (context-preserving redaction).
    assert!(
        sanitized.starts_with(";PR:"),
        "sanitized output must preserve the ;PR: prefix; got: {:?}",
        sanitized,
    );

    // The redacted marker must be present.
    assert!(
        sanitized.contains("<redacted>"),
        "sanitized output must contain <redacted> marker; got: {:?}",
        sanitized,
    );
}

/// Credential context always redacts regardless of content.
#[test]
fn credential_context_always_redacts() {
    let token = secure_login_response("23753528", "FOOBAR");
    let line = format!(";PR: {token}\r");
    let sanitized = sanitize_wire_line(&line, WireContext::Credential);
    assert_eq!(sanitized, "<redacted>", "Credential context must fully redact");
    assert!(!sanitized.contains(&token));
}

/// PasswordResponse context always redacts regardless of content.
#[test]
fn password_response_context_always_redacts() {
    let password = "hunter2hunter2";
    let sanitized = sanitize_wire_line(password, WireContext::PasswordResponse);
    assert_eq!(sanitized, "<redacted>", "PasswordResponse context must fully redact");
    assert!(!sanitized.contains(password));
}

/// A second reference vector: hunter2hunter2/23753528.
/// secure_login_response("23753528", "hunter2hunter2") is the test the plan's
/// acceptance criteria reference. The exact token value is deterministic.
#[test]
fn wire_sanitizer_blocks_hunter2hunter2_flow() {
    // The challenge "23753528" with password "hunter2hunter2".
    // Note: "FOOBAR" gives 72768415; "hunter2hunter2" gives a different token.
    // We compute it and verify the sanitizer blocks whatever it is.
    let challenge = "23753528";
    let password = "hunter2hunter2";
    let token = secure_login_response(challenge, password);

    // Verify it's 8 digits.
    assert_eq!(token.len(), 8, "token must be exactly 8 digits");
    assert!(token.chars().all(|c| c.is_ascii_digit()), "token must be all digits");

    let response_line = format!(";PR: {token}\r");
    let sanitized = sanitize_wire_line(&response_line, WireContext::Generic);

    assert!(
        !sanitized.contains(&token),
        "WIRE LEAK: sanitized output contains the token for hunter2hunter2 flow; got: {:?}",
        sanitized,
    );
    assert!(sanitized.contains("<redacted>"));
}

/// Benign wire lines pass through unchanged (zero-allocation fast path).
#[test]
fn benign_wire_line_passes_through() {
    let line = ";FW: K0XYZ-10\r";
    let sanitized = sanitize_wire_line(line, WireContext::Generic);
    assert_eq!(sanitized, line);
}
