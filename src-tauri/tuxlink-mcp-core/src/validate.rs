//! Pure input validators for the MCP write + compose tiers (phase 3.4).
//!
//! These run BEFORE the egress gate at the port impl boundary: an agent-supplied
//! value that fails validation must be rejected as a malformed request
//! ([`crate::ports::WritePortError::Invalid`]) WITHOUT consuming the armed grant
//! and WITHOUT reaching the gate, so a bad input can never be mistaken for a
//! denied egress. Every function here is pure (no I/O) except
//! [`validate_attachment_dest`], which canonicalizes the requested parent against
//! the base as a symlink-escape defense after a cheap component scan.
//!
//! All checks fail closed: the first violation wins and is returned as a typed
//! [`ValidationError`]. The router maps `ValidationError` (via
//! `From<ValidationError> for WritePortError`) onto an `invalid_request` tool
//! error.

use std::path::{Component, Path, PathBuf};

use thiserror::Error;

/// Why an agent-supplied input was rejected before it could reach a gated port.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// An attachment destination was an absolute path (e.g. `/etc/passwd`).
    #[error("path must be relative to the attachment base, not absolute")]
    AbsolutePath,
    /// An attachment destination contained a `..` parent-traversal component.
    #[error("path must not contain parent-directory (`..`) components")]
    ParentTraversal,
    /// The destination, once joined + canonicalized, escaped the base directory
    /// (e.g. via a symlink inside the base).
    #[error("path escapes the attachment base directory")]
    EscapesBase,
    /// A required string was empty (after trimming).
    #[error("value must not be empty")]
    Empty,
    /// A string carried disallowed control characters.
    #[error("value must not contain control characters")]
    ControlChars,
    /// A string exceeded its maximum allowed length. Carries the limit.
    #[error("value exceeds the maximum length of {0}")]
    TooLong(usize),
    /// A header-bearing field (address/subject) carried a CR or LF, which could
    /// inject additional headers.
    #[error("value must not contain CR or LF (header injection)")]
    HeaderInjection,
    /// A numeric value fell outside its accepted range. Carries a description.
    #[error("value out of range: {0}")]
    OutOfRange(String),
    /// A Maidenhead grid locator was malformed (wrong length or field shape).
    #[error("grid must be a 4- or 6-char Maidenhead locator (e.g. EM75 or EM75xx)")]
    InvalidGrid,
    /// A frequency list was empty, too long, or carried an out-of-band dial.
    #[error("frequency list invalid: {0}")]
    InvalidFrequencies(String),
}

/// Validate an attachment destination `requested` (relative) against the
/// attachment `base` directory, returning the joined absolute-ish path on
/// success.
///
/// Defense in two layers:
/// 1. A pure component scan rejects an absolute path, a root/prefix component,
///    or any `..` parent-traversal BEFORE the filesystem is touched.
/// 2. The joined path's parent is canonicalized and asserted to live under the
///    canonicalized base, defeating a symlink inside the base that points
///    outside it. A parent that cannot be canonicalized is treated as an escape.
pub fn validate_attachment_dest(base: &Path, requested: &str) -> Result<PathBuf, ValidationError> {
    if requested.trim().is_empty() {
        return Err(ValidationError::Empty);
    }

    let req_path = Path::new(requested);
    if req_path.is_absolute() {
        return Err(ValidationError::AbsolutePath);
    }

    // Component scan: reject before any FS access.
    for comp in req_path.components() {
        match comp {
            Component::ParentDir => return Err(ValidationError::ParentTraversal),
            Component::RootDir | Component::Prefix(_) => return Err(ValidationError::AbsolutePath),
            Component::CurDir | Component::Normal(_) => {}
        }
    }

    let joined = base.join(req_path);

    // Symlink-escape defense: canonicalize the joined parent + the base and
    // assert containment. The dest file itself need not exist yet, but its
    // parent directory must, and must resolve under the base.
    let parent = joined.parent().unwrap_or(base);
    let canon_parent = parent
        .canonicalize()
        .map_err(|_| ValidationError::EscapesBase)?;
    let canon_base = base
        .canonicalize()
        .map_err(|_| ValidationError::EscapesBase)?;
    if !canon_parent.starts_with(&canon_base) {
        return Err(ValidationError::EscapesBase);
    }

    Ok(joined)
}

/// Validate a single email-style address: no CR/LF (header injection), no
/// control characters, at most 64 bytes.
pub fn validate_address(addr: &str) -> Result<(), ValidationError> {
    if addr.contains('\r') || addr.contains('\n') {
        return Err(ValidationError::HeaderInjection);
    }
    if addr.chars().any(|c| c.is_control()) {
        return Err(ValidationError::ControlChars);
    }
    if addr.len() > 64 {
        return Err(ValidationError::TooLong(64));
    }
    Ok(())
}

/// Validate a message subject: no CR/LF (header injection), no other control
/// characters (NUL/BEL/etc. must not reach the RFC5322 header block), at most
/// 256 bytes. CR/LF are reported as [`ValidationError::HeaderInjection`] (the
/// header-splitting class); any other control char is
/// [`ValidationError::ControlChars`]. Mirrors [`validate_address`]'s control
/// rejection.
pub fn validate_subject(s: &str) -> Result<(), ValidationError> {
    if s.contains('\r') || s.contains('\n') {
        return Err(ValidationError::HeaderInjection);
    }
    if s.chars().any(|c| c.is_control()) {
        return Err(ValidationError::ControlChars);
    }
    if s.len() > 256 {
        return Err(ValidationError::TooLong(256));
    }
    Ok(())
}

/// Validate a message body: multi-line is fine, but at most 65536 bytes.
pub fn validate_body(s: &str) -> Result<(), ValidationError> {
    if s.len() > 65536 {
        return Err(ValidationError::TooLong(65536));
    }
    Ok(())
}

/// Validate an ARDOP drive level: `0..=100`.
pub fn validate_drive_level(v: u8) -> Result<(), ValidationError> {
    if v > 100 {
        return Err(ValidationError::OutOfRange("drive_level 0..=100".to_string()));
    }
    Ok(())
}

/// Validate a VARA bandwidth in Hz: one of `{500, 2300, 2750}`.
pub fn validate_vara_bandwidth(hz: u32) -> Result<(), ValidationError> {
    match hz {
        500 | 2300 | 2750 => Ok(()),
        _ => Err(ValidationError::OutOfRange(
            "vara bandwidth must be one of 500, 2300, 2750 Hz".to_string(),
        )),
    }
}

/// Validate a Maidenhead grid locator (station-intelligence tier, phase 3.2).
///
/// Accepts a 4- or 6-character locator matching the monolith's
/// `validate_grid_input` shape:
/// - chars 1-2: field letters `A..=R` (case-insensitive),
/// - chars 3-4: square digits `0..=9`,
/// - chars 5-6 (optional, both-or-neither): subsquare letters `a..=x`
///   (case-insensitive).
///
/// Pure (no I/O). A 3-char or 5-char input, or any out-of-range field, is
/// rejected as [`ValidationError::InvalidGrid`].
pub fn validate_grid(grid: &str) -> Result<(), ValidationError> {
    let bytes = grid.as_bytes();
    if bytes.len() != 4 && bytes.len() != 6 {
        return Err(ValidationError::InvalidGrid);
    }
    // Field letters A..R (case-insensitive).
    let field_ok = |b: u8| (b'A'..=b'R').contains(&b.to_ascii_uppercase());
    if !field_ok(bytes[0]) || !field_ok(bytes[1]) {
        return Err(ValidationError::InvalidGrid);
    }
    // Square digits 0..9.
    if !bytes[2].is_ascii_digit() || !bytes[3].is_ascii_digit() {
        return Err(ValidationError::InvalidGrid);
    }
    // Optional subsquare letters a..x (case-insensitive).
    if bytes.len() == 6 {
        let sub_ok = |b: u8| (b'A'..=b'X').contains(&b.to_ascii_uppercase());
        if !sub_ok(bytes[4]) || !sub_ok(bytes[5]) {
            return Err(ValidationError::InvalidGrid);
        }
    }
    Ok(())
}

/// Validate a candidate dial-frequency list (kHz): `1..=11` entries, each within
/// the HF amateur span `1800.0..=30000.0` kHz. Pure.
pub fn validate_frequencies_khz(freqs: &[f64]) -> Result<(), ValidationError> {
    if freqs.is_empty() {
        return Err(ValidationError::InvalidFrequencies(
            "at least one frequency is required".to_string(),
        ));
    }
    if freqs.len() > 11 {
        return Err(ValidationError::InvalidFrequencies(
            "at most 11 frequencies are allowed".to_string(),
        ));
    }
    for f in freqs {
        if !(1800.0..=30000.0).contains(f) {
            return Err(ValidationError::InvalidFrequencies(format!(
                "{f} kHz is outside the 1800..=30000 kHz HF range"
            )));
        }
    }
    Ok(())
}

/// Validate an optional `history_hours` bound for a station search: when present,
/// it must not exceed 720 hours (30 days). `None` is always valid. Pure.
pub fn validate_history_hours(hours: Option<u32>) -> Result<(), ValidationError> {
    match hours {
        Some(h) if h > 720 => Err(ValidationError::OutOfRange(
            "history_hours must be <= 720".to_string(),
        )),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // --- attachment dest: component-scan rejections (no FS dependency) ---

    #[test]
    fn absolute_path_is_rejected() {
        let base = tempfile::tempdir().unwrap();
        assert_eq!(
            validate_attachment_dest(base.path(), "/etc/passwd"),
            Err(ValidationError::AbsolutePath)
        );
    }

    #[test]
    fn parent_traversal_is_rejected() {
        let base = tempfile::tempdir().unwrap();
        assert_eq!(
            validate_attachment_dest(base.path(), "../../etc/passwd"),
            Err(ValidationError::ParentTraversal)
        );
    }

    #[test]
    fn embedded_parent_traversal_is_rejected() {
        let base = tempfile::tempdir().unwrap();
        assert_eq!(
            validate_attachment_dest(base.path(), "a/../../b"),
            Err(ValidationError::ParentTraversal)
        );
    }

    #[test]
    fn empty_dest_is_rejected() {
        let base = tempfile::tempdir().unwrap();
        assert_eq!(
            validate_attachment_dest(base.path(), "   "),
            Err(ValidationError::Empty)
        );
    }

    // --- attachment dest: accepted relative paths under a real base ---

    #[test]
    fn simple_relative_dest_is_accepted_and_under_base() {
        let base = tempfile::tempdir().unwrap();
        let out = validate_attachment_dest(base.path(), "roster.txt").unwrap();
        assert!(out.starts_with(base.path()), "result must be under the base");
    }

    #[test]
    fn nested_relative_dest_is_accepted_when_parent_exists() {
        let base = tempfile::tempdir().unwrap();
        fs::create_dir(base.path().join("sub")).unwrap();
        let out = validate_attachment_dest(base.path(), "sub/roster.txt").unwrap();
        assert!(out.starts_with(base.path()));
    }

    // --- attachment dest: symlink-escape defense ---

    #[test]
    fn symlink_in_base_escaping_outside_is_rejected() {
        // base/escape -> /tmp (outside the base). A write to base/escape/x must
        // be rejected by the canonicalize-and-contain check even though the
        // component scan saw only Normal components.
        let base = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let link = base.path().join("escape");
        match std::os::unix::fs::symlink(outside.path(), &link) {
            Ok(()) => {
                assert_eq!(
                    validate_attachment_dest(base.path(), "escape/x"),
                    Err(ValidationError::EscapesBase),
                    "a symlink in the base pointing outside must be rejected"
                );
            }
            Err(_) => {
                // Symlink creation unsupported on this fs: the component-scan
                // rejections above still cover the primary attack class.
            }
        }
    }

    // --- drive level ---

    #[test]
    fn drive_level_bounds() {
        assert!(validate_drive_level(0).is_ok());
        assert!(validate_drive_level(100).is_ok());
        assert!(matches!(
            validate_drive_level(101),
            Err(ValidationError::OutOfRange(_))
        ));
        assert!(matches!(
            validate_drive_level(200),
            Err(ValidationError::OutOfRange(_))
        ));
    }

    // --- vara bandwidth ---

    #[test]
    fn vara_bandwidth_set() {
        assert!(validate_vara_bandwidth(500).is_ok());
        assert!(validate_vara_bandwidth(2300).is_ok());
        assert!(validate_vara_bandwidth(2750).is_ok());
        assert!(matches!(
            validate_vara_bandwidth(2301),
            Err(ValidationError::OutOfRange(_))
        ));
    }

    // --- address ---

    #[test]
    fn address_header_injection_is_rejected() {
        assert_eq!(
            validate_address("a@b.com\r\nBcc: evil@x.com"),
            Err(ValidationError::HeaderInjection)
        );
    }

    #[test]
    fn address_control_chars_rejected() {
        assert_eq!(
            validate_address("a\u{0007}b@x.com"),
            Err(ValidationError::ControlChars)
        );
    }

    #[test]
    fn address_too_long_rejected() {
        let addr = "a".repeat(65);
        assert_eq!(validate_address(&addr), Err(ValidationError::TooLong(64)));
    }

    #[test]
    fn address_ok() {
        assert!(validate_address("W1AW@winlink.org").is_ok());
    }

    // --- subject + body ---

    #[test]
    fn subject_newline_is_header_injection() {
        assert_eq!(
            validate_subject("hello\nworld"),
            Err(ValidationError::HeaderInjection)
        );
    }

    #[test]
    fn subject_carriage_return_is_header_injection() {
        assert_eq!(
            validate_subject("hello\rworld"),
            Err(ValidationError::HeaderInjection)
        );
    }

    #[test]
    fn subject_nul_is_control_chars() {
        // A non-CRLF control char (NUL) must be rejected so it can never reach
        // the RFC5322 header block.
        assert_eq!(
            validate_subject("hello\u{0}world"),
            Err(ValidationError::ControlChars)
        );
    }

    #[test]
    fn subject_bell_is_control_chars() {
        assert_eq!(
            validate_subject("ding\u{0007}"),
            Err(ValidationError::ControlChars)
        );
    }

    #[test]
    fn subject_plain_ascii_is_ok() {
        assert!(validate_subject("ICS-213 General Message").is_ok());
    }

    #[test]
    fn subject_too_long_rejected() {
        let s = "x".repeat(257);
        assert_eq!(validate_subject(&s), Err(ValidationError::TooLong(256)));
    }

    #[test]
    fn body_allows_multiline_but_caps_length() {
        assert!(validate_body("line one\nline two\n").is_ok());
        let big = "x".repeat(65537);
        assert_eq!(validate_body(&big), Err(ValidationError::TooLong(65536)));
    }

    // --- grid (station-intelligence) ---

    #[test]
    fn grid_four_char_is_accepted() {
        assert!(validate_grid("EM75").is_ok());
        assert!(validate_grid("CN87").is_ok());
        assert!(validate_grid("AA00").is_ok());
        assert!(validate_grid("RR99").is_ok());
    }

    #[test]
    fn grid_six_char_is_accepted() {
        assert!(validate_grid("EM75xx").is_ok());
        assert!(validate_grid("CN87ux").is_ok());
        // Case-insensitive field + subsquare.
        assert!(validate_grid("em75XA").is_ok());
    }

    #[test]
    fn grid_three_char_is_rejected() {
        assert_eq!(validate_grid("EM7"), Err(ValidationError::InvalidGrid));
    }

    #[test]
    fn grid_five_char_is_rejected() {
        assert_eq!(validate_grid("EM75x"), Err(ValidationError::InvalidGrid));
    }

    #[test]
    fn grid_out_of_range_fields_are_rejected() {
        // Field letter past R.
        assert_eq!(validate_grid("ZZ00"), Err(ValidationError::InvalidGrid));
        // Non-digit square.
        assert_eq!(validate_grid("EMxx"), Err(ValidationError::InvalidGrid));
        // Subsquare past x.
        assert_eq!(validate_grid("EM75yz"), Err(ValidationError::InvalidGrid));
    }

    // --- frequencies ---

    #[test]
    fn frequencies_valid_set_is_accepted() {
        assert!(validate_frequencies_khz(&[7104.0]).is_ok());
        assert!(validate_frequencies_khz(&[1800.0, 30000.0, 14105.0]).is_ok());
    }

    #[test]
    fn frequencies_empty_is_rejected() {
        assert!(matches!(
            validate_frequencies_khz(&[]),
            Err(ValidationError::InvalidFrequencies(_))
        ));
    }

    #[test]
    fn frequencies_twelve_items_is_rejected() {
        let twelve = vec![7104.0; 12];
        assert!(matches!(
            validate_frequencies_khz(&twelve),
            Err(ValidationError::InvalidFrequencies(_))
        ));
    }

    #[test]
    fn frequencies_out_of_range_is_rejected() {
        // Below 1800 kHz.
        assert!(matches!(
            validate_frequencies_khz(&[1500.0]),
            Err(ValidationError::InvalidFrequencies(_))
        ));
        // Above 30000 kHz.
        assert!(matches!(
            validate_frequencies_khz(&[30000.1]),
            Err(ValidationError::InvalidFrequencies(_))
        ));
    }

    // --- history hours ---

    #[test]
    fn history_hours_none_is_ok() {
        assert!(validate_history_hours(None).is_ok());
    }

    #[test]
    fn history_hours_within_cap_is_ok() {
        assert!(validate_history_hours(Some(0)).is_ok());
        assert!(validate_history_hours(Some(720)).is_ok());
    }

    #[test]
    fn history_hours_over_cap_is_rejected() {
        assert!(matches!(
            validate_history_hours(Some(721)),
            Err(ValidationError::OutOfRange(_))
        ));
    }
}
