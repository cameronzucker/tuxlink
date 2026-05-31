//! Input validation for the forms module (spec §10).

/// Maximum bytes for an inbound form XML attachment. Enforced at
/// `parse_form_xml` boundary; rejection is UiError::Internal before
/// allocation. 256 KiB is well above any plausible Winlink form.
pub const MAX_FORM_XML_BYTES: usize = 256 * 1024;

/// Maximum number of `<variables>` fields per form payload. Anything
/// beyond is rejected as malicious or malformed.
pub const MAX_FORM_FIELDS: usize = 256;

/// Maximum XML element nesting depth during parse. Defense against
/// pathological nesting bombs.
pub const MAX_XML_NESTING_DEPTH: u16 = 8;

/// Maximum total XML events the parser will consume. Defense against
/// quadratic-blowup attacks (many small elements).
pub const MAX_XML_EVENTS: u32 = 10_000;

/// Validate a form ID extracted from an attachment filename.
///
/// Spec §10: `^[A-Za-z0-9_-]{1,64}$`. Path-traversal-safe; documented as
/// load-bearing for v0.5+ catalog-cache use.
pub fn is_valid_form_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 64 {
        return false;
    }
    id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_form_ids() {
        assert!(is_valid_form_id("ICS213_Initial"));
        assert!(is_valid_form_id("ICS309_Initial"));
        assert!(is_valid_form_id("Position_Initial"));
        assert!(is_valid_form_id("a"));
        assert!(is_valid_form_id("a_b-c_1"));
        assert!(is_valid_form_id(&"X".repeat(64)));
    }

    #[test]
    fn rejects_invalid_form_ids() {
        assert!(!is_valid_form_id(""), "empty");
        assert!(!is_valid_form_id(&"X".repeat(65)), ">64 chars");
        assert!(!is_valid_form_id("../etc/passwd"), "path traversal");
        assert!(!is_valid_form_id("foo bar"), "whitespace");
        assert!(!is_valid_form_id("foo.bar"), "dot");
        assert!(!is_valid_form_id("foo/bar"), "slash");
        assert!(!is_valid_form_id("foo\\bar"), "backslash");
        assert!(!is_valid_form_id("Ünïcödë"), "non-ASCII");
        assert!(!is_valid_form_id("foo\x00bar"), "null");
    }

    #[test]
    fn size_cap_is_256_kib() {
        assert_eq!(MAX_FORM_XML_BYTES, 262_144);
    }
}
