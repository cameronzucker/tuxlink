//! Forms integration tests — round-trip + cross-spec parity.

use std::collections::HashMap;
use tuxlink_lib::forms::{catalog, parse, serialize, types::FormParameters};

#[test]
fn ics213_serialize_parse_round_trips() {
    let form = catalog::find_form("ICS213_Initial").expect("ICS213 bundled");
    let params = FormParameters {
        xml_file_version: "1.0".into(),
        rms_express_version: "Tuxlink/0.3.0".into(),
        submission_datetime: "20260530143000".into(),
        senders_callsign: "N0CALL".into(),
        grid_square: "FM18".into(),
        display_form: form.display_form.into(),
        reply_template: form.reply_template.into(),
    };
    let mut values = HashMap::new();
    values.insert("inc_name".into(), "HURRICANE WALDO".into());
    values.insert("to_name".into(), "JOHN OPERATOR".into());
    values.insert("fm_name".into(), "JANE OPERATOR".into());
    values.insert("subjectline".into(), "REQUEST SUPPLIES".into());
    values.insert("mdate".into(), "2026-05-30".into());
    values.insert("mtime".into(), "14:30Z".into());
    values.insert("message".into(), "Need bandages by 1700Z.".into());

    let xml = serialize::serialize_form_xml(form, &params, &values);
    let parsed = parse::parse_form_xml(&xml).expect("round-trip parse succeeds");

    assert_eq!(parsed.form_parameters.display_form, "ICS213_Initial_Viewer.html");
    assert_eq!(parsed.form_parameters.reply_template, "ICS213_SendReply.0");
    assert_eq!(parsed.form_parameters.senders_callsign, "N0CALL");

    for (id, expected) in &[
        ("inc_name", "HURRICANE WALDO"),
        ("to_name", "JOHN OPERATOR"),
        ("subjectline", "REQUEST SUPPLIES"),
        ("mtime", "14:30Z"),
        ("message", "Need bandages by 1700Z."),
    ] {
        let actual = parsed.fields.iter().find(|(k, _)| k == id).map(|(_, v)| v.as_str());
        assert_eq!(actual, Some(*expected), "field {} mismatch", id);
    }
}

#[test]
fn ics213_body_template_substitutes_correctly() {
    let form = catalog::find_form("ICS213_Initial").unwrap();
    let mut values = HashMap::new();
    values.insert("inc_name".into(), "WALDO".into());
    values.insert("subjectline".into(), "TEST".into());
    values.insert("isexercise".into(), "** THIS IS AN EXERCISE **".into());
    let body = serialize::render_body_template(form.body_template, &values);
    assert!(body.contains("1. Incident Name: WALDO"));
    assert!(body.contains("4. Subject: TEST"));
    assert!(body.contains("** THIS IS AN EXERCISE **"));
}
