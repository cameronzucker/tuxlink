//! Form-XML parsing per spec §3 wire format + §10 hardening.

use crate::forms::types::FormPayload;
use crate::forms::validation;

/// Detect whether an attachment is a Winlink form XML (`RMS_Express_Form_*.xml`)
/// and extract its form_id. Returns None if the attachment is not a form.
///
/// The form_id is the basename between "RMS_Express_Form_" prefix and ".xml"
/// suffix (e.g., "ICS213_Initial" for "RMS_Express_Form_ICS213_Initial.xml").
/// Per spec §10, the result is validated against the safe form_id regex; an
/// attachment with an unsafe basename (path traversal etc.) returns None.
pub fn detect_form_attachment(filename: &str) -> Option<String> {
    const PREFIX: &str = "RMS_Express_Form_";
    const SUFFIX: &str = ".xml";
    let stripped = filename.strip_prefix(PREFIX)?;
    let id = stripped.strip_suffix(SUFFIX)?;
    if validation::is_valid_form_id(id) {
        Some(id.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_ics213_attachment() {
        assert_eq!(
            detect_form_attachment("RMS_Express_Form_ICS213_Initial.xml"),
            Some("ICS213_Initial".to_string())
        );
    }

    #[test]
    fn detects_ics309_attachment() {
        assert_eq!(
            detect_form_attachment("RMS_Express_Form_ICS309_Initial.xml"),
            Some("ICS309_Initial".to_string())
        );
    }

    #[test]
    fn ignores_non_form_attachment() {
        assert_eq!(detect_form_attachment("photo.jpg"), None);
        assert_eq!(detect_form_attachment("data.xml"), None);
        assert_eq!(detect_form_attachment("RMS_Express_Form_.xml"), None);
        assert_eq!(detect_form_attachment("RMS_Express_Form_ICS213"), None);
    }

    #[test]
    fn rejects_unsafe_form_id() {
        assert_eq!(
            detect_form_attachment("RMS_Express_Form_../etc/passwd.xml"),
            None
        );
        assert_eq!(
            detect_form_attachment("RMS_Express_Form_foo bar.xml"),
            None
        );
    }
}

// parse_form_xml — implemented in T1.5.
pub fn parse_form_xml(_bytes: &[u8]) -> Result<FormPayload, String> {
    todo!("T1.5")
}
