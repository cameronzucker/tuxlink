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

/// Same as redact_wire_line but for any free-form text — finds and
/// scrubs ;PQ:/;PR: tokens anywhere in the string (not just at the
/// start of a line). Used for the `B2fEvent::RemoteErrorReceived.raw`
/// field per spec §6.2 finding R1 #10.
pub fn redact_freeform(text: &str) -> Cow<'_, str> {
    if !(text.contains(";PR:") || text.contains(";PQ:")) {
        return Cow::Borrowed(text);
    }
    // Strategy: split on ;PR: / ;PQ: markers, replace the next whitespace-
    // delimited token. This handles tokens at any position in the string.
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        let pq_pos = rest.find(";PQ:");
        let pr_pos = rest.find(";PR:");
        let (pos, marker_len) = match (pq_pos, pr_pos) {
            (None, None) => {
                out.push_str(rest);
                break;
            }
            (Some(pq), None) => (pq, 4),
            (None, Some(pr)) => (pr, 4),
            (Some(pq), Some(pr)) => {
                if pq < pr { (pq, 4) } else { (pr, 4) }
            }
        };
        out.push_str(&rest[..pos + marker_len]);
        rest = &rest[pos + marker_len..];
        // Skip the single space (if present) then the token.
        let after_space = rest.trim_start_matches(' ');
        let space_len = rest.len() - after_space.len();
        if space_len > 0 {
            out.push(' ');
        }
        // Token ends at the next whitespace, CR, or LF.
        let token_end = after_space
            .find(|c: char| c.is_whitespace())
            .unwrap_or(after_space.len());
        out.push_str("<redacted>");
        rest = &after_space[token_end..];
    }
    Cow::Owned(out)
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
}
