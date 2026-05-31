//! ICS-213 General Message — the canonical EmComm form.
//!
//! Field schema mirrors `ICS213_Initial.html` from the WLE Standard Templates
//! catalog. Field IDs are lowercase (per spec §3 wire convention); WLE template
//! placeholders like `<var Subjectline>` resolve to our lowercase `subjectline`
//! field via case-insensitive lookup in render_body_template.

use crate::forms::types::{FieldKind, FormDef, FormField};

const FIELDS: &[FormField] = &[
    FormField { id: "inc_name",          label: "Incident Name",          kind: FieldKind::Text,     required: false, max_length: Some(30) },
    FormField { id: "to_name",           label: "To (Name and Position)", kind: FieldKind::Text,     required: true,  max_length: Some(60) },
    FormField { id: "fm_name",           label: "From (Name and Position)", kind: FieldKind::Text,   required: true,  max_length: Some(60) },
    FormField { id: "subjectline",       label: "Subject",                kind: FieldKind::Text,     required: true,  max_length: Some(50) },
    FormField { id: "mdate",             label: "Date",                   kind: FieldKind::Date,     required: true,  max_length: None },
    FormField { id: "mtime",             label: "Time",                   kind: FieldKind::Time,     required: true,  max_length: None },
    FormField { id: "message",           label: "Message",                kind: FieldKind::LongText, required: true,  max_length: Some(4000) },
    FormField { id: "approved_name",     label: "Approved by",            kind: FieldKind::Text,     required: false, max_length: Some(60) },
    FormField { id: "approved_postitle", label: "Position/Title",         kind: FieldKind::Text,     required: false, max_length: Some(60) },
    FormField { id: "isexercise",        label: "Is exercise",            kind: FieldKind::Boolean,  required: false, max_length: None },
];

const SUBJECT_TEMPLATE: &str = "<var subjectline> - <var mdate> <var mtime>";

const BODY_TEMPLATE: &str = r#"GENERAL MESSAGE (ICS 213)
<var formtitle>
<var isexercise>
1. Incident Name: <var inc_name>
2. To (Name and Position): <var to_name>
3. From (Name and Position): <var fm_name>
4. Subject: <var subjectline>
5. Date: <var mdate>
6. Time: <var mtime>
7. Message:

<var message>

8. Approved by: <var approved_name>
8a. Position/Title: <var approved_postitle>
------------------------------------
Sending Station: Tuxlink
[No changes or editing of this message are allowed]
"#;

pub const ICS213_INITIAL: FormDef = FormDef {
    id: "ICS213_Initial",
    name: "ICS-213 General Message",
    fields: FIELDS,
    subject_template: SUBJECT_TEMPLATE,
    body_template: BODY_TEMPLATE,
    display_form: "ICS213_Initial_Viewer.html",
    reply_template: "ICS213_SendReply.0",
};
