//! Centralized credential-equivalent redaction for B2F wire lines.
//!
//! See design spec §6.1 + §6.2. The (;PQ, ;PR) token pair is offline-
//! brute-forceable per R2's entropy analysis (~26.6 bits, public salt) —
//! both MUST be scrubbed before any sink. This module is the single
//! source of truth for that scrubbing.

use std::borrow::Cow;

use once_cell::sync::Lazy;
use regex::Regex;

// Case-insensitive `; PR :` or `; PQ :` with optional whitespace around the
// colon. Captures the full matched token so replace_all can reconstruct the
// marker and substitute <redacted> for the value. Works at any position in
// the string — embedded, prefix-anchored, or in free-form error messages.
//
// once_cell::sync::Lazy is used (not std::sync::LazyLock) to preserve the
// `rust-version = "1.75"` MSRV declared in Cargo.toml — LazyLock is stable
// since 1.80, and the clippy::incompatible_msrv lint is wired in CI.
static CRED_TOKEN_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i);\s*p[qr]\s*:\s*\S+").expect("static regex compiles")
});

/// Scrub credential-equivalent tokens from a B2F wire line. Returns a
/// Cow because most lines (no `;PR`/`;PQ`) pass through unchanged.
///
/// Handles embedded, lowercase, and whitespace-variant markers (e.g.
/// `; PR : 12345`, `;pr: 12345`, `debug saw ;PR: 72768415 from client`).
pub fn redact_wire_line(line: &str) -> Cow<'_, str> {
    if !CRED_TOKEN_RE.is_match(line) {
        return Cow::Borrowed(line);
    }
    // Replace each match: preserve the ;PR:/;PQ: marker portion (everything
    // up to and including the colon), then append " <redacted>".
    Cow::Owned(
        CRED_TOKEN_RE
            .replace_all(line, |caps: &regex::Captures| {
                let matched = caps.get(0).unwrap().as_str();
                // Find the `:` and reproduce the marker portion.
                if let Some(colon_pos) = matched.find(':') {
                    let prefix = &matched[..=colon_pos]; // includes the colon
                    format!("{prefix} <redacted>")
                } else {
                    "<redacted>".to_string()
                }
            })
            .into_owned(),
    )
}

/// Same as `redact_wire_line` but for any free-form text — finds and
/// scrubs ;PQ:/;PR: tokens anywhere in the string (not just at the
/// start of a line). Used for the `B2fEvent::RemoteErrorReceived.raw`
/// field per spec §6.2 finding R1 #10.
///
/// Implementation delegates to `redact_wire_line`; both functions now share
/// the single regex-based scanner, which handles embedded + lowercase +
/// whitespace variants uniformly.
pub fn redact_freeform(text: &str) -> Cow<'_, str> {
    redact_wire_line(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_pr_response_token() {
        // Canonical wl2k-go vector: challenge "23753528", password "FOOBAR" → response "72768415".
        let line = ";PR: 72768415\r";
        let redacted = redact_wire_line(line);
        assert!(!redacted.contains("72768415"), "got: {redacted:?}");
        assert!(redacted.contains(";PR:"), "must keep the ;PR: marker for log readability");
    }

    #[test]
    fn redacts_pq_challenge_token_symmetrically() {
        // Per R2 #2 entropy analysis: the (challenge, response) pair enables
        // offline brute-force. Challenge MUST be redacted symmetrically.
        let line = ";PQ: 23753528\r";
        let redacted = redact_wire_line(line);
        assert!(!redacted.contains("23753528"), "got: {redacted:?}");
        assert!(redacted.contains(";PQ:"));
    }

    #[test]
    fn redacts_both_directions_with_arrow_prefix() {
        // The telnet.rs WireTap emits "> " for outbound + "< " for inbound.
        let inbound = "< ;PQ: 23753528\r";
        let outbound = "> ;PR: 72768415\r";
        assert!(!redact_wire_line(inbound).contains("23753528"));
        assert!(!redact_wire_line(outbound).contains("72768415"));
    }

    #[test]
    fn pass_through_non_credential_lines_unchanged() {
        // No copy when nothing matches.
        let line = "*** Unknown client types are not allowed on production servers\r";
        let redacted = redact_wire_line(line);
        assert_eq!(redacted, line);
        // Borrowed Cow path:
        assert!(matches!(redacted, std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn redact_freeform_scrubs_embedded_tokens() {
        // Defense-in-depth: a misbehaving CMS could echo the token back in
        // an error message. The freeform variant scrubs anywhere in the text.
        let text = "Server saw ;PR: 72768415 from client; rejecting";
        let redacted = redact_freeform(text);
        assert!(!redacted.contains("72768415"), "got: {redacted:?}");
    }

    #[test]
    fn redact_freeform_scrubs_pq_anywhere() {
        let text = "Challenge ;PQ: 23753528 sent at 10:42 UTC";
        let redacted = redact_freeform(text);
        assert!(!redacted.contains("23753528"));
    }

    // --- New tests for Codex BLOCKER #1: embedded / lowercase / whitespace variants ---

    #[test]
    fn redacts_embedded_pr_token_not_at_line_start() {
        // Codex finding #1: today's prefix-anchored matcher leaks this.
        let line = "< *** debug saw ;PR: 72768415 from client; rejecting\r";
        let redacted = redact_wire_line(line);
        assert!(!redacted.contains("72768415"), "got: {redacted:?}");
    }

    #[test]
    fn redacts_lowercase_marker() {
        let line = ";pr: 72768415\r";
        let redacted = redact_wire_line(line);
        assert!(!redacted.contains("72768415"), "got: {redacted:?}");
    }

    #[test]
    fn redacts_whitespace_variant() {
        let line = "; PR : 72768415\r";
        let redacted = redact_wire_line(line);
        assert!(!redacted.contains("72768415"), "got: {redacted:?}");
    }

    #[test]
    fn redacts_pq_lowercase_and_whitespace() {
        let line = ";pq:  23753528\r"; // double-space (extra whitespace before value)
        let redacted = redact_wire_line(line);
        assert!(!redacted.contains("23753528"), "got: {redacted:?}");
    }
}
