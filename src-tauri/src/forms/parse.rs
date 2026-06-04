//! Form-XML parsing per spec §3 wire format + §10 hardening.

use crate::forms::types::{FormParameters, FormPayload};
use crate::forms::validation;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

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

/// Parse a Winlink HTML-form XML payload per spec §3 (wire format) and §10 (hardening).
///
/// Hardening invariants (all return `Err` on violation):
/// - Input byte length ≤ `MAX_FORM_XML_BYTES` (checked before Reader allocation).
/// - No DOCTYPE / entity declarations (`Event::DocType` → immediate reject).
/// - Element nesting depth ≤ `MAX_XML_NESTING_DEPTH`.
/// - Total event count ≤ `MAX_XML_EVENTS`.
/// - `<variables>` field count ≤ `MAX_FORM_FIELDS`.
///
/// Returns a `FormPayload` with `form_id` set to empty string; the caller
/// assigns the form_id from the attachment filename.
pub fn parse_form_xml(bytes: &[u8]) -> Result<FormPayload, String> {
    // Size cap fires BEFORE any Reader allocation.
    if bytes.len() > validation::MAX_FORM_XML_BYTES {
        return Err(format!(
            "form XML too large: {} bytes (limit {})",
            bytes.len(),
            validation::MAX_FORM_XML_BYTES
        ));
    }

    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);

    let mut buf = Vec::new();

    // Element name stack — used to determine context.
    let mut stack: Vec<String> = Vec::new();
    // Text accumulator for the current leaf element.
    let mut text_acc: Option<String> = None;

    let mut form_parameters = FormParameters::default();
    let mut fields: Vec<(String, String)> = Vec::new();

    let mut depth: u16 = 0;
    let mut event_count: u32 = 0;

    loop {
        let event = reader.read_event_into(&mut buf).map_err(|e| e.to_string())?;
        event_count += 1;
        if event_count > validation::MAX_XML_EVENTS {
            return Err(format!(
                "form XML exceeds maximum event count ({})",
                validation::MAX_XML_EVENTS
            ));
        }

        match event {
            // DOCTYPE is rejected immediately — no entity expansion allowed.
            Event::DocType(_) => {
                return Err("form XML DOCTYPE rejected (entity expansion not permitted)".into());
            }

            Event::Start(e) => {
                let name = std::str::from_utf8(e.name().as_ref())
                    .map_err(|e| e.to_string())?
                    .to_string();
                depth += 1;
                if depth > validation::MAX_XML_NESTING_DEPTH {
                    return Err(format!(
                        "form XML nesting depth {} exceeds limit ({})",
                        depth,
                        validation::MAX_XML_NESTING_DEPTH
                    ));
                }
                stack.push(name);
                // Start accumulating text for leaf elements inside known sections.
                if let Some(parent) = stack.len().checked_sub(2).and_then(|i| stack.get(i)) {
                    if parent == "form_parameters" || parent == "variables" {
                        text_acc = Some(String::new());
                    }
                }
            }

            Event::Empty(e) => {
                // Self-closing element: treat as Start+End with empty text.
                let name = std::str::from_utf8(e.name().as_ref())
                    .map_err(|e| e.to_string())?
                    .to_string();
                // Check depth would exceed limit if we pushed this element.
                let prospective_depth = depth + 1;
                if prospective_depth > validation::MAX_XML_NESTING_DEPTH {
                    return Err(format!(
                        "form XML nesting depth {} exceeds limit ({})",
                        prospective_depth,
                        validation::MAX_XML_NESTING_DEPTH
                    ));
                }
                // Check if it's a leaf under a known section.
                let parent = stack.last().map(|s| s.as_str());
                match parent {
                    Some("form_parameters") => {
                        apply_form_parameter(&mut form_parameters, &name, "");
                    }
                    Some("variables") => {
                        if fields.len() >= validation::MAX_FORM_FIELDS {
                            return Err(format!(
                                "form XML exceeds maximum field count ({})",
                                validation::MAX_FORM_FIELDS
                            ));
                        }
                        fields.push((name, String::new()));
                    }
                    _ => {}
                }
                // Empty elements don't push onto the stack — they are atomic.
            }

            Event::End(_) => {
                let committed_text = text_acc.take().unwrap_or_default();
                if let Some(leaf_name) = stack.last() {
                    let parent = stack.len().checked_sub(2).and_then(|i| stack.get(i));
                    match parent.map(|s| s.as_str()) {
                        Some("form_parameters") => {
                            apply_form_parameter(&mut form_parameters, leaf_name, &committed_text);
                        }
                        Some("variables") => {
                            if fields.len() >= validation::MAX_FORM_FIELDS {
                                return Err(format!(
                                    "form XML exceeds maximum field count ({})",
                                    validation::MAX_FORM_FIELDS
                                ));
                            }
                            fields.push((leaf_name.clone(), committed_text));
                        }
                        _ => {}
                    }
                }
                stack.pop();
                depth = depth.saturating_sub(1);
            }

            Event::Text(e) => {
                if let Some(acc) = text_acc.as_mut() {
                    let s = e.decode().map_err(|e| e.to_string())?;
                    acc.push_str(&s);
                }
            }

            Event::GeneralRef(e) => {
                // Predefined XML entities and numeric character references.
                // Only decoded when inside a leaf element accumulator; inter-element
                // whitespace nodes have text_acc = None and are ignored here too.
                if let Some(acc) = text_acc.as_mut() {
                    // Use the BytesRef built-in helper for numeric character refs
                    // (&#NNN; or &#xHH;). For named refs, decode() gives us the name.
                    if e.is_char_ref() {
                        // resolve_char_ref returns Ok(Some(char)) on valid refs.
                        let ch = e.resolve_char_ref()
                            .map_err(|err| err.to_string())?
                            .ok_or_else(|| "invalid character reference (code 0)".to_string())?;
                        acc.push(ch);
                    } else {
                        let name = e.decode().map_err(|err| err.to_string())?;
                        let decoded = match name.as_ref() {
                            "amp"  => "&",
                            "lt"   => "<",
                            "gt"   => ">",
                            "quot" => "\"",
                            "apos" => "'",
                            other  => {
                                return Err(format!("unknown entity reference: &{};", other));
                            }
                        };
                        acc.push_str(decoded);
                    }
                }
            }

            Event::Eof => break,

            // All other events (PI, Comment, CData, Decl) are silently ignored.
            _ => {}
        }

        buf.clear();
    }

    Ok(FormPayload {
        form_id: String::new(),
        form_parameters,
        fields,
    })
}

/// Map a `<form_parameters>` child element name + text value onto the
/// `FormParameters` struct. Unknown field names are silently ignored.
fn apply_form_parameter(params: &mut FormParameters, name: &str, value: &str) {
    match name {
        "xml_file_version" => params.xml_file_version = value.to_string(),
        "rms_express_version" => params.rms_express_version = value.to_string(),
        "submission_datetime" => params.submission_datetime = value.to_string(),
        "senders_callsign" => params.senders_callsign = value.to_string(),
        "grid_square" => params.grid_square = value.to_string(),
        "display_form" => params.display_form = value.to_string(),
        "reply_template" => params.reply_template = value.to_string(),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------------------
    // T1.5 parse_form_xml tests
    // ---------------------------------------------------------------------------

    const SAMPLE_XML: &str = r#"<?xml version="1.0"?>
<RMS_Express_Form>
  <form_parameters>
    <xml_file_version>1.0</xml_file_version>
    <rms_express_version>Tuxlink/0.3.0</rms_express_version>
    <submission_datetime>20260530143000</submission_datetime>
    <senders_callsign>N0CALL</senders_callsign>
    <grid_square>FM18</grid_square>
    <display_form>ICS213_Initial_Viewer.html</display_form>
    <reply_template>ICS213_SendReply.0</reply_template>
  </form_parameters>
  <variables>
    <inc_name>HURRICANE WALDO</inc_name>
    <to_name>JOHN OPERATOR</to_name>
    <fm_name>JANE OPERATOR</fm_name>
    <subjectline>REQUEST SUPPLIES</subjectline>
    <mdate>2026-05-30</mdate>
    <mtime>14:30Z</mtime>
    <message>Need bandages.</message>
    <approved_name>JANE OPERATOR</approved_name>
  </variables>
</RMS_Express_Form>"#;

    #[test]
    fn parses_well_formed_form_xml() {
        let payload = parse_form_xml(SAMPLE_XML.as_bytes()).expect("parse should succeed");
        assert_eq!(payload.form_id, "");  // form_id is set by caller (from attachment name)
        assert_eq!(payload.form_parameters.display_form, "ICS213_Initial_Viewer.html");
        assert_eq!(payload.form_parameters.rms_express_version, "Tuxlink/0.3.0");
        assert_eq!(payload.form_parameters.senders_callsign, "N0CALL");
        let inc_name = payload.fields.iter().find(|(k, _)| k == "inc_name").map(|(_, v)| v.as_str());
        assert_eq!(inc_name, Some("HURRICANE WALDO"));
        let mtime = payload.fields.iter().find(|(k, _)| k == "mtime").map(|(_, v)| v.as_str());
        assert_eq!(mtime, Some("14:30Z"));
    }

    #[test]
    fn rejects_oversized_xml() {
        let huge = vec![b'<'; validation::MAX_FORM_XML_BYTES + 1];
        let result = parse_form_xml(&huge);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too large"));
    }

    #[test]
    fn rejects_billion_laughs_doctype() {
        let malicious = r#"<?xml version="1.0"?>
<!DOCTYPE lolz [
  <!ENTITY lol "lol">
  <!ENTITY lol2 "&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;">
]>
<RMS_Express_Form>&lol2;</RMS_Express_Form>"#;
        let result = parse_form_xml(malicious.as_bytes());
        assert!(result.is_err(), "DOCTYPE/entity bomb must be rejected");
    }

    #[test]
    fn rejects_deeply_nested_xml() {
        let mut bomb = String::from("<?xml version=\"1.0\"?>");
        for _ in 0..20 {
            bomb.push_str("<x>");
        }
        for _ in 0..20 {
            bomb.push_str("</x>");
        }
        let result = parse_form_xml(bomb.as_bytes());
        assert!(result.is_err(), "depth-20 nesting must exceed MAX_XML_NESTING_DEPTH=8");
    }

    #[test]
    fn rejects_too_many_fields() {
        let mut payload = String::from(r#"<?xml version="1.0"?><RMS_Express_Form><variables>"#);
        for i in 0..300 {
            payload.push_str(&format!("<f{0}>v</f{0}>", i));
        }
        payload.push_str("</variables></RMS_Express_Form>");
        let result = parse_form_xml(payload.as_bytes());
        assert!(result.is_err(), "300 fields must exceed MAX_FORM_FIELDS=256");
    }

    // ---------------------------------------------------------------------------
    // T1.4 detect_form_attachment tests
    // ---------------------------------------------------------------------------

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
    fn decodes_predefined_entity_references() {
        let xml = r#"<?xml version="1.0"?>
<RMS_Express_Form>
  <variables>
    <message>A &amp; B &lt; C &gt; D</message>
  </variables>
</RMS_Express_Form>"#;
        let payload = parse_form_xml(xml.as_bytes()).expect("parse should succeed");
        let msg = payload.fields.iter().find(|(k, _)| k == "message").map(|(_, v)| v.as_str());
        assert_eq!(msg, Some("A & B < C > D"));
    }

    #[test]
    fn rejects_unknown_entity_reference() {
        let xml = r#"<?xml version="1.0"?>
<RMS_Express_Form>
  <variables>
    <message>hello &unknown;</message>
  </variables>
</RMS_Express_Form>"#;
        let result = parse_form_xml(xml.as_bytes());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown entity reference"));
    }

    #[test]
    fn preserves_leading_and_trailing_whitespace_in_text() {
        let xml = r#"<?xml version="1.0"?>
<RMS_Express_Form>
  <variables>
    <message>  trailing whitespace  </message>
  </variables>
</RMS_Express_Form>"#;
        let payload = parse_form_xml(xml.as_bytes()).expect("parse should succeed");
        let msg = payload.fields.iter().find(|(k, _)| k == "message").map(|(_, v)| v.as_str());
        assert_eq!(msg, Some("  trailing whitespace  "));
    }

    #[test]
    fn rejects_unsafe_form_id() {
        // Path-traversal sentinels (slashes) are still rejected.
        assert_eq!(
            detect_form_attachment("RMS_Express_Form_../etc/passwd.xml"),
            None
        );
        assert_eq!(
            detect_form_attachment("RMS_Express_Form_foo/bar.xml"),
            None
        );
        // Control characters (newline, tab, NUL) are still rejected.
        assert_eq!(
            detect_form_attachment("RMS_Express_Form_foo\nbar.xml"),
            None
        );
        assert_eq!(
            detect_form_attachment("RMS_Express_Form_foo\x00bar.xml"),
            None
        );
    }

    /// 2026-06-04 Codex adrev P1.2: bundled WLE catalog has form filenames
    /// with spaces (e.g. `RMS_Express_Form_Quick Message Initial.xml`,
    /// `RMS_Express_Form_Hawaii Siren Report.xml`). The pre-P1.2
    /// validator rejected these — the receive side could not parse a
    /// form tuxlink itself sent. Detection must now succeed for
    /// space-bearing IDs while continuing to reject path separators
    /// and control chars (above).
    #[test]
    fn accepts_wle_catalog_form_ids_with_spaces() {
        assert_eq!(
            detect_form_attachment("RMS_Express_Form_Quick Message Initial.xml"),
            Some("Quick Message Initial".to_string())
        );
        assert_eq!(
            detect_form_attachment("RMS_Express_Form_Hawaii Siren Report.xml"),
            Some("Hawaii Siren Report".to_string())
        );
        assert_eq!(
            detect_form_attachment("RMS_Express_Form_Bulletin Initial.xml"),
            Some("Bulletin Initial".to_string())
        );
        // Dotted stems (versioning, etc.) are accepted:
        assert_eq!(
            detect_form_attachment("RMS_Express_Form_Form.v1.xml"),
            Some("Form.v1".to_string())
        );
    }
}
