//! Serialize form data to WLE-compatible wire format per spec §3.

use crate::forms::types::{FormDef, FormParameters};
use std::collections::HashMap;

/// Serialize a form's field values to WLE-compatible XML bytes (UTF-8 with BOM).
///
/// Per spec §3:
/// - `<?xml version="1.0"?>` (no encoding attr)
/// - All `<variables>` element names lowercase
/// - `<form_parameters>` emits 7 elements in WLE order
/// - Empty fields emit `<field></field>` (not self-closing)
/// - Special chars (<, >, &) XML-escaped; " and ' left as-is per WLE
/// - UTF-8 BOM prefix (3 bytes: 0xEF 0xBB 0xBF)
pub fn serialize_form_xml(
    form: &FormDef,
    params: &FormParameters,
    field_values: &HashMap<String, String>,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(2048);
    // UTF-8 BOM
    out.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    out.extend_from_slice(b"<?xml version=\"1.0\"?>\r\n");
    out.extend_from_slice(b"<RMS_Express_Form>\r\n");
    // form_parameters in WLE order
    out.extend_from_slice(b"<form_parameters>\r\n");
    push_element(&mut out, "xml_file_version", &params.xml_file_version);
    push_element(&mut out, "rms_express_version", &params.rms_express_version);
    push_element(&mut out, "submission_datetime", &params.submission_datetime);
    push_element(&mut out, "senders_callsign", &params.senders_callsign);
    push_element(&mut out, "grid_square", &params.grid_square);
    push_element(&mut out, "display_form", &params.display_form);
    push_element(&mut out, "reply_template", &params.reply_template);
    out.extend_from_slice(b"</form_parameters>\r\n");
    // variables in field-declaration order from FormDef
    out.extend_from_slice(b"<variables>\r\n");
    for field in form.fields {
        let value = field_values.get(field.id).map(String::as_str).unwrap_or("");
        push_element(&mut out, field.id, value);
    }
    out.extend_from_slice(b"</variables>\r\n");
    out.extend_from_slice(b"</RMS_Express_Form>\r\n");
    out
}

/// Write a single XML element with value. Lowercases the name; XML-escapes the
/// value (`<` `>` `&` only, matching WLE — `"` and `'` left as-is).
fn push_element(out: &mut Vec<u8>, name: &str, value: &str) {
    out.push(b'<');
    out.extend_from_slice(name.to_ascii_lowercase().as_bytes());
    out.push(b'>');
    for ch in value.chars() {
        match ch {
            '<' => out.extend_from_slice(b"&lt;"),
            '>' => out.extend_from_slice(b"&gt;"),
            '&' => out.extend_from_slice(b"&amp;"),
            _ => {
                let mut buf = [0u8; 4];
                out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            }
        }
    }
    out.extend_from_slice(b"</");
    out.extend_from_slice(name.to_ascii_lowercase().as_bytes());
    out.extend_from_slice(b">\r\n");
}

/// Serialize a catalog form's field values to WLE-compatible XML bytes (UTF-8 with
/// BOM). Used for catalog-served WLE Standard Forms whose definition is NOT in the
/// static `BUNDLED_FORMS` table — i.e., the ~245 forms whose authoritative shape
/// is the HTML template itself, not a Rust `FormDef`. The XML envelope shape
/// matches `serialize_form_xml`; the only difference is that the `<variables>`
/// block iterates `field_values` directly (sorted by key for deterministic
/// output) instead of walking a static `FormDef.fields` slice.
///
/// The `_form_id` parameter is reserved for future per-form variant handling
/// (e.g. emitting `<form_id>` into the envelope, or pulling out fields specific
/// to a given form type). For P1 it's unused — the form id is communicated via
/// the `display_form` / `reply_template` fields inside `FormParameters` per the
/// WLE filename convention.
pub fn serialize_catalog_form_xml(
    _form_id: &str,
    params: &FormParameters,
    field_values: &HashMap<String, String>,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(2048);
    // UTF-8 BOM
    out.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    out.extend_from_slice(b"<?xml version=\"1.0\"?>\r\n");
    out.extend_from_slice(b"<RMS_Express_Form>\r\n");
    // form_parameters in WLE order
    out.extend_from_slice(b"<form_parameters>\r\n");
    push_element(&mut out, "xml_file_version", &params.xml_file_version);
    push_element(&mut out, "rms_express_version", &params.rms_express_version);
    push_element(&mut out, "submission_datetime", &params.submission_datetime);
    push_element(&mut out, "senders_callsign", &params.senders_callsign);
    push_element(&mut out, "grid_square", &params.grid_square);
    push_element(&mut out, "display_form", &params.display_form);
    push_element(&mut out, "reply_template", &params.reply_template);
    out.extend_from_slice(b"</form_parameters>\r\n");
    // variables sorted alphabetically by key for deterministic XML — without a
    // static FormDef there is no natural declaration order to preserve.
    out.extend_from_slice(b"<variables>\r\n");
    let mut keys: Vec<&String> = field_values.keys().collect();
    keys.sort();
    for k in keys {
        let v = field_values.get(k).map(String::as_str).unwrap_or("");
        push_element(&mut out, k, v);
    }
    out.extend_from_slice(b"</variables>\r\n");
    out.extend_from_slice(b"</RMS_Express_Form>\r\n");
    out
}

/// Render the body-template string (`Msg:` block) with `<var fieldid>` placeholders
/// substituted from field values. Case-insensitive on field name (matches WLE+Pat
/// behavior).
pub fn render_body_template(template: &str, field_values: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(template.len() + 256);
    let mut chars = template.char_indices().peekable();
    while let Some((i, ch)) = chars.next() {
        if ch == '<' && template[i..].starts_with("<var ") {
            // Find end of <var X>
            if let Some(end) = template[i..].find('>') {
                let var_section = &template[i + 5..i + end];  // skip "<var "
                let field_id = var_section.trim().to_ascii_lowercase();
                let value = field_values.get(&field_id).cloned().unwrap_or_default();
                out.push_str(&value);
                // skip ahead past the closing '>'
                for _ in 0..end {
                    chars.next();
                }
                continue;
            }
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forms::types::{FieldKind, FormDef, FormField};

    const TEST_FORM: FormDef = FormDef {
        id: "Test_Initial",
        name: "Test Form",
        fields: &[
            FormField { id: "alpha", label: "A", kind: FieldKind::Text, required: false, max_length: None },
            FormField { id: "beta",  label: "B", kind: FieldKind::Text, required: false, max_length: None },
        ],
        subject_template: "<var alpha>",
        body_template: "Hello <var alpha>; from <var beta>.",
        display_form: "Test_Initial_Viewer.html",
        reply_template: "Test_SendReply.0",
    };

    #[test]
    fn xml_starts_with_bom_then_declaration() {
        let params = FormParameters {
            xml_file_version: "1.0".into(),
            rms_express_version: "Tuxlink/0.3.0".into(),
            ..Default::default()
        };
        let values = HashMap::new();
        let xml = serialize_form_xml(&TEST_FORM, &params, &values);
        assert_eq!(&xml[0..3], &[0xEF, 0xBB, 0xBF], "UTF-8 BOM");
        assert!(xml[3..].starts_with(b"<?xml version=\"1.0\"?>"), "declaration");
    }

    #[test]
    fn variables_are_lowercase() {
        let params = FormParameters::default();
        let mut values = HashMap::new();
        values.insert("alpha".to_string(), "A-VALUE".to_string());
        let xml = serialize_form_xml(&TEST_FORM, &params, &values);
        let xml_str = String::from_utf8_lossy(&xml);
        assert!(xml_str.contains("<alpha>A-VALUE</alpha>"));
        assert!(!xml_str.contains("<Alpha>"));
    }

    #[test]
    fn empty_fields_get_open_close_tags() {
        let params = FormParameters::default();
        let values = HashMap::new();  // no values
        let xml = serialize_form_xml(&TEST_FORM, &params, &values);
        let xml_str = String::from_utf8_lossy(&xml);
        assert!(xml_str.contains("<alpha></alpha>"));
        assert!(xml_str.contains("<beta></beta>"));
        assert!(!xml_str.contains("<alpha/>"), "no self-closing");
    }

    #[test]
    fn special_chars_in_values_are_xml_escaped() {
        let params = FormParameters::default();
        let mut values = HashMap::new();
        values.insert("alpha".into(), "<script>&\"'".into());
        let xml = serialize_form_xml(&TEST_FORM, &params, &values);
        let xml_str = String::from_utf8_lossy(&xml);
        assert!(xml_str.contains("&lt;script&gt;&amp;\"'"));
    }

    #[test]
    fn form_parameters_emit_in_wle_order() {
        let params = FormParameters {
            xml_file_version: "1.0".into(),
            rms_express_version: "RMS".into(),
            submission_datetime: "20260530143000".into(),
            senders_callsign: "N0CALL".into(),
            grid_square: "FM18".into(),
            display_form: "X_Viewer.html".into(),
            reply_template: "X_SendReply.0".into(),
        };
        let xml = String::from_utf8_lossy(&serialize_form_xml(&TEST_FORM, &params, &HashMap::new())).to_string();
        let pos_xml = xml.find("<xml_file_version>").unwrap();
        let pos_ver = xml.find("<rms_express_version>").unwrap();
        let pos_dt = xml.find("<submission_datetime>").unwrap();
        let pos_call = xml.find("<senders_callsign>").unwrap();
        let pos_grid = xml.find("<grid_square>").unwrap();
        let pos_df = xml.find("<display_form>").unwrap();
        let pos_rt = xml.find("<reply_template>").unwrap();
        assert!(pos_xml < pos_ver && pos_ver < pos_dt && pos_dt < pos_call
                && pos_call < pos_grid && pos_grid < pos_df && pos_df < pos_rt,
                "form_parameters elements must be in WLE order");
    }

    #[test]
    fn render_body_substitutes_vars_case_insensitive() {
        let mut values = HashMap::new();
        values.insert("alpha".into(), "WORLD".into());
        values.insert("beta".into(), "JANE".into());
        let body = render_body_template("Hello <var alpha>; from <var beta>.", &values);
        assert_eq!(body, "Hello WORLD; from JANE.");
        // Case-insensitive match — `<var Alpha>` substitutes from values["alpha"]
        let body2 = render_body_template("<var Alpha> <var BETA>", &values);
        assert_eq!(body2, "WORLD JANE");
    }

    #[test]
    fn render_body_leaves_unknown_vars_as_empty() {
        let values = HashMap::new();
        let body = render_body_template("Hello <var unknown>!", &values);
        assert_eq!(body, "Hello !");
    }

    // ---- serialize_catalog_form_xml -----------------------------------

    #[test]
    fn catalog_xml_starts_with_bom_then_declaration() {
        let params = FormParameters {
            xml_file_version: "1.0".into(),
            rms_express_version: "Tuxlink/0.3.0".into(),
            ..Default::default()
        };
        let values = HashMap::new();
        let xml = serialize_catalog_form_xml("AnyForm", &params, &values);
        assert_eq!(&xml[0..3], &[0xEF, 0xBB, 0xBF], "UTF-8 BOM");
        assert!(xml[3..].starts_with(b"<?xml version=\"1.0\"?>"), "declaration");
    }

    #[test]
    fn catalog_xml_emits_form_parameters_in_wle_order() {
        let params = FormParameters {
            xml_file_version: "1.0".into(),
            rms_express_version: "Tuxlink/0.3.0".into(),
            submission_datetime: "20260601120000".into(),
            senders_callsign: "N0CALL".into(),
            grid_square: "FN42".into(),
            display_form: "ICS205_Viewer.html".into(),
            reply_template: "ICS205_SendReply.0".into(),
        };
        let xml = serialize_catalog_form_xml("ICS205", &params, &HashMap::new());
        let xml_str = String::from_utf8_lossy(&xml).to_string();
        let p1 = xml_str.find("<xml_file_version>").unwrap();
        let p2 = xml_str.find("<rms_express_version>").unwrap();
        let p3 = xml_str.find("<submission_datetime>").unwrap();
        let p4 = xml_str.find("<senders_callsign>").unwrap();
        let p5 = xml_str.find("<grid_square>").unwrap();
        let p6 = xml_str.find("<display_form>").unwrap();
        let p7 = xml_str.find("<reply_template>").unwrap();
        assert!(p1 < p2 && p2 < p3 && p3 < p4 && p4 < p5 && p5 < p6 && p6 < p7);
        assert!(xml_str.contains("<display_form>ICS205_Viewer.html</display_form>"));
        assert!(xml_str.contains("<reply_template>ICS205_SendReply.0</reply_template>"));
    }

    #[test]
    fn catalog_xml_sorts_variable_keys_alphabetically() {
        let params = FormParameters::default();
        let mut values = HashMap::new();
        values.insert("zebra".to_string(), "z-val".to_string());
        values.insert("alpha".to_string(), "a-val".to_string());
        values.insert("mango".to_string(), "m-val".to_string());
        let xml = serialize_catalog_form_xml("Form", &params, &values);
        let xml_str = String::from_utf8_lossy(&xml).to_string();
        let pa = xml_str.find("<alpha>").unwrap();
        let pm = xml_str.find("<mango>").unwrap();
        let pz = xml_str.find("<zebra>").unwrap();
        assert!(pa < pm && pm < pz, "variables must sort alphabetically by key");
        assert!(xml_str.contains("<alpha>a-val</alpha>"));
        assert!(xml_str.contains("<mango>m-val</mango>"));
        assert!(xml_str.contains("<zebra>z-val</zebra>"));
    }

    #[test]
    fn catalog_xml_escapes_special_chars() {
        let params = FormParameters::default();
        let mut values = HashMap::new();
        values.insert("payload".into(), "<script>&\"'".into());
        let xml = serialize_catalog_form_xml("Form", &params, &values);
        let xml_str = String::from_utf8_lossy(&xml).to_string();
        assert!(xml_str.contains("<payload>&lt;script&gt;&amp;\"'</payload>"));
    }

    #[test]
    fn catalog_xml_empty_values_produce_open_close_tags() {
        let params = FormParameters::default();
        let mut values = HashMap::new();
        values.insert("empty_field".into(), "".into());
        let xml = serialize_catalog_form_xml("Form", &params, &values);
        let xml_str = String::from_utf8_lossy(&xml).to_string();
        assert!(xml_str.contains("<empty_field></empty_field>"));
        assert!(!xml_str.contains("<empty_field/>"), "no self-closing");
    }

    #[test]
    fn catalog_xml_envelope_structure_matches_serialize_form_xml() {
        // The catalog serializer is the without-FormDef twin. It must emit
        // the same wrapper element tree: RMS_Express_Form → form_parameters
        // → variables → close. Iteration source differs (sorted field_values
        // vs walked FormDef.fields); structure must not.
        let params = FormParameters {
            xml_file_version: "1.0".into(),
            rms_express_version: "Tuxlink".into(),
            ..Default::default()
        };
        let mut values = HashMap::new();
        values.insert("alpha".into(), "A".into());
        let xml = serialize_catalog_form_xml("Form", &params, &values);
        let xml_str = String::from_utf8_lossy(&xml).to_string();
        assert!(xml_str.contains("<RMS_Express_Form>"));
        assert!(xml_str.contains("<form_parameters>"));
        assert!(xml_str.contains("</form_parameters>"));
        assert!(xml_str.contains("<variables>"));
        assert!(xml_str.contains("</variables>"));
        assert!(xml_str.contains("</RMS_Express_Form>"));
        // form_parameters precedes variables which precedes the close tag.
        let p_fp = xml_str.find("<form_parameters>").unwrap();
        let p_var = xml_str.find("<variables>").unwrap();
        let p_close = xml_str.find("</RMS_Express_Form>").unwrap();
        assert!(p_fp < p_var && p_var < p_close);
    }
}
