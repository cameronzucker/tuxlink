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
/// Originally per spec §10: `^[A-Za-z0-9_-]{1,64}$`. Relaxed (2026-06-04
/// Codex adrev finding P1.2) to also accept SPACE, DOT, and AMPERSAND
/// because the bundled WLE catalog has form filenames that legitimately
/// contain those characters — e.g. `Quick Message Initial`,
/// `Hawaii Siren Report`, `NY & NJ State Forms` paths route through
/// stems containing `&`. Without this relaxation, a tuxlink-authored
/// catalog form sent to another tuxlink station was REJECTED by its
/// own receive path because the round-tripped form_id failed validation
/// (round-trip parity is load-bearing for self-send smoke tests and
/// for any future loopback verification).
///
/// What stays REJECTED (the security-load-bearing set):
/// - Path separators (`/`, `\`)
/// - Path-traversal sentinel (`..` — implicitly: `.` is allowed but
///   the WLE catalog never produces `..` as a stem, and `..` alone is
///   too short to be a real form_id — accepted nonetheless because
///   downstream code constructs `RMS_Express_Form_<id>.xml` which
///   does NOT directory-traverse on its own; the path resolution at
///   `forms/<token>/<id>` uses a canonical-prefix check in the
///   loopback HTTP server, not regex validation here)
/// - NUL byte and other control chars (anything below 0x20)
/// - Non-ASCII (Unicode form IDs would break the WLE filename
///   conventions on case-insensitive filesystems)
/// - Empty string or >64 chars
///
/// Path-traversal safety is preserved because the catalog walker
/// (`forms::wle_templates::list`) emits stems from real on-disk files
/// — a remote sender can't inject a synthetic stem like `..` and have
/// it match the catalog. The validation here is a defense-in-depth
/// guard against malformed inbound attachment filenames before they
/// reach any filesystem code path.
pub fn is_valid_form_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 64 {
        return false;
    }
    id.bytes().all(|b| {
        b.is_ascii_alphanumeric()
            || b == b'_'
            || b == b'-'
            || b == b' '
            || b == b'.'
            || b == b'&'
    })
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

    /// 2026-06-04 Codex adrev P1.2: bundled WLE catalog has form filenames
    /// with spaces / dots / ampersands. The previously-rejected character
    /// set caused tuxlink-to-tuxlink round-trips to fail on the receive
    /// side. These IDs must now be accepted; the security set
    /// (path separators, NUL, non-ASCII) remains rejected below.
    #[test]
    fn accepts_wle_catalog_form_ids_with_space_dot_ampersand() {
        // Real WLE catalog stems that the prior regex rejected:
        assert!(is_valid_form_id("Quick Message Initial"), "WLE 'Quick Message Initial'");
        assert!(is_valid_form_id("Hawaii Siren Report"), "WLE 'Hawaii Siren Report'");
        assert!(is_valid_form_id("Bulletin Initial"), "WLE 'Bulletin Initial'");
        // Dot is allowed (some WLE templates have versioned stems like `v1.0`):
        assert!(is_valid_form_id("Form.v1"), "dot allowed");
        // Ampersand appears in folder names (`NY & NJ State Forms`);
        // operator-custom forms could include `&` in stems:
        assert!(is_valid_form_id("Foo & Bar Initial"), "ampersand allowed");
    }

    #[test]
    fn rejects_invalid_form_ids() {
        assert!(!is_valid_form_id(""), "empty");
        assert!(!is_valid_form_id(&"X".repeat(65)), ">64 chars");
        // NB: `../etc/passwd` is now rejected because of the `/`, not the `.`;
        // the dot itself is now allowed (see accepts_wle_catalog_form_ids_*).
        assert!(!is_valid_form_id("../etc/passwd"), "path traversal via slash");
        assert!(!is_valid_form_id("foo/bar"), "slash");
        assert!(!is_valid_form_id("foo\\bar"), "backslash");
        assert!(!is_valid_form_id("Ünïcödë"), "non-ASCII");
        assert!(!is_valid_form_id("foo\x00bar"), "null");
        // Control chars below 0x20 remain rejected (newline, tab):
        assert!(!is_valid_form_id("foo\nbar"), "newline");
        assert!(!is_valid_form_id("foo\tbar"), "tab");
    }

    #[test]
    fn size_cap_is_256_kib() {
        assert_eq!(MAX_FORM_XML_BYTES, 262_144);
    }
}
