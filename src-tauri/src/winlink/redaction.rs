//! Centralized credential-equivalent redaction for B2F wire lines.
//!
//! See design spec §6.1 + §6.2. The (;PQ, ;PR) token pair is offline-
//! brute-forceable per R2's entropy analysis (~26.6 bits, public salt) —
//! both MUST be scrubbed before any sink. This module is the single
//! source of truth for that scrubbing.

use std::borrow::Cow;

/// Scrub credential-equivalent tokens from a B2F wire line. Returns a
/// Cow because most lines (no `;PR`/`;PQ`) pass through unchanged.
pub fn redact_wire_line(line: &str) -> Cow<'_, str> {
    if line.contains(";PR:") || line.contains(";PQ:") {
        let mut out = String::with_capacity(line.len());
        for raw in line.split_inclusive('\r') {
            let token_prefix = if let Some(rest) = raw.strip_prefix("> ;PR:") {
                Some(("> ;PR: ", rest))
            } else if let Some(rest) = raw.strip_prefix(";PR:") {
                Some((";PR: ", rest))
            } else if let Some(rest) = raw.strip_prefix("> ;PQ:") {
                Some(("> ;PQ: ", rest))
            } else if let Some(rest) = raw.strip_prefix(";PQ:") {
                Some((";PQ: ", rest))
            } else if let Some(rest) = raw.strip_prefix("< ;PR:") {
                Some(("< ;PR: ", rest))
            } else if let Some(rest) = raw.strip_prefix("< ;PQ:") {
                Some(("< ;PQ: ", rest))
            } else {
                None
            };
            if let Some((prefix, _)) = token_prefix {
                out.push_str(prefix);
                out.push_str("<redacted>");
                if raw.ends_with('\r') {
                    out.push('\r');
                }
            } else {
                out.push_str(raw);
            }
        }
        Cow::Owned(out)
    } else {
        Cow::Borrowed(line)
    }
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
}
