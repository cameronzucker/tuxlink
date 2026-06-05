//! Wire-text sanitizer — strips credential-bearing protocol-line content
//! BEFORE the bytes reach a tracing macro (spec §5.6 CRITICAL fix).
//!
//! Field-name redaction CANNOT catch credentials inside a `msg` string
//! (e.g., `format!(";PR: {response}\r")`). Wire-emitting callsites MUST
//! route through this helper.

use once_cell::sync::Lazy;
use regex::RegexSet;
use std::borrow::Cow;

/// Patterns that match wire-text lines carrying credential material.
/// On match, the matched line is replaced with a context-preserving redaction.
static WIRE_PATTERNS: Lazy<RegexSet> = Lazy::new(|| {
    RegexSet::new(&[
        r"(?i)^;PR:\s*\S+",
        r"(?i)^;PQ:\s*\S+",
        r"(?i)^auth\s+\S+\s+\S+",
    ])
    .expect("wire patterns must compile")
});

/// Context tag identifying what kind of wire emission is happening.
///
/// `Credential` and `PasswordResponse` always redact the whole line.
/// `Generic` runs the line through `WIRE_PATTERNS` for content-aware redaction.
#[derive(Debug, Clone, Copy)]
pub enum WireContext {
    Generic,
    PasswordResponse,
    Credential,
}

/// Sanitize a wire-text line for safe logging.
///
/// Returns `Cow::Borrowed(raw)` when no pattern matched (zero allocation for
/// the common case). Returns `Cow::Owned(...)` when redaction was applied.
pub fn sanitize_wire_line(raw: &str, ctx: WireContext) -> Cow<'_, str> {
    match ctx {
        WireContext::Credential | WireContext::PasswordResponse => {
            Cow::Owned("<redacted>".into())
        }
        WireContext::Generic => {
            for idx in WIRE_PATTERNS.matches(raw).iter() {
                return Cow::Owned(redact_match(raw, idx));
            }
            Cow::Borrowed(raw)
        }
    }
}

fn redact_match(raw: &str, pattern_idx: usize) -> String {
    // Preserve protocol context (e.g., ";PR: ") + redact the credential value.
    // The pattern indices correspond to the WIRE_PATTERNS slice order.
    match pattern_idx {
        0 => preserve_prefix_redact(raw, ";PR:"),
        1 => preserve_prefix_redact(raw, ";PQ:"),
        2 => "<redacted AUTH>".into(),
        _ => "<redacted>".into(),
    }
}

fn preserve_prefix_redact(raw: &str, prefix: &str) -> String {
    let trailing = if raw.ends_with('\r') { "\r" } else { "" };
    format!("{} <redacted>{}", prefix, trailing)
}

#[cfg(test)]
mod tests {
    use super::{sanitize_wire_line, WireContext};
    use std::borrow::Cow;

    #[test]
    fn pr_line_is_redacted_with_prefix_preserved() {
        let raw = ";PR: 72768415\r";
        let out = sanitize_wire_line(raw, WireContext::Generic);
        assert_eq!(out, ";PR: <redacted>\r");
        assert!(matches!(out, Cow::Owned(_)));
    }

    #[test]
    fn pq_line_is_redacted_with_prefix_preserved() {
        let raw = ";PQ: 23753528\r";
        let out = sanitize_wire_line(raw, WireContext::Generic);
        assert_eq!(out, ";PQ: <redacted>\r");
    }

    #[test]
    fn auth_line_is_redacted_whole() {
        let raw = "AUTH alice hunter2";
        let out = sanitize_wire_line(raw, WireContext::Generic);
        assert_eq!(out, "<redacted AUTH>");
    }

    #[test]
    fn benign_wire_text_passes_through_borrowed() {
        let raw = ";FW: K0XYZ-10\r";
        let out = sanitize_wire_line(raw, WireContext::Generic);
        assert_eq!(out, ";FW: K0XYZ-10\r");
        assert!(matches!(out, Cow::Borrowed(_)));
    }

    #[test]
    fn credential_context_always_redacts_regardless_of_content() {
        let raw = "innocent-looking-data";
        let out = sanitize_wire_line(raw, WireContext::Credential);
        assert_eq!(out, "<redacted>");
    }

    #[test]
    fn password_response_context_always_redacts_regardless_of_content() {
        let raw = "anything";
        let out = sanitize_wire_line(raw, WireContext::PasswordResponse);
        assert_eq!(out, "<redacted>");
    }

    #[test]
    fn empty_string_in_generic_context_passes_through() {
        let raw = "";
        let out = sanitize_wire_line(raw, WireContext::Generic);
        assert_eq!(out, "");
        assert!(matches!(out, Cow::Borrowed(_)));
    }

    #[test]
    fn case_insensitive_pr_match() {
        let raw = ";pr: 99999999\r";
        let out = sanitize_wire_line(raw, WireContext::Generic);
        assert_eq!(out, ";PR: <redacted>\r");
    }
}
