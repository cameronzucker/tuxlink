//! Winlink Bulletin — broadcast message to named recipients.
//!
//! Field schema mirrors `Bulletin Initial.html` from the WLE Standard Templates
//! catalog. Field IDs are lowercase (per spec §3 wire convention).

use crate::forms::types::{FieldKind, FormDef, FormField};

const FIELDS: &[FormField] = &[
    FormField { id: "level",             label: "Precedence Level",    kind: FieldKind::Text,     required: true,  max_length: Some(20) },
    FormField { id: "subjectline",       label: "Subject",             kind: FieldKind::Text,     required: true,  max_length: Some(80) },
    FormField { id: "bullnr",            label: "Bulletin #",          kind: FieldKind::Text,     required: true,  max_length: Some(10) },
    FormField { id: "title",             label: "Title",               kind: FieldKind::Text,     required: false, max_length: Some(60) },
    FormField { id: "name",              label: "For (Recipient)",     kind: FieldKind::Text,     required: true,  max_length: Some(60) },
    FormField { id: "from_name",         label: "Bulletin From",       kind: FieldKind::Text,     required: true,  max_length: Some(60) },
    FormField { id: "activitydatetime1", label: "Date/Time",           kind: FieldKind::Text,     required: true,  max_length: Some(30) },
    FormField { id: "message",           label: "Message",             kind: FieldKind::LongText, required: true,  max_length: Some(4000) },
    FormField { id: "templateversion",   label: "Template Version",    kind: FieldKind::Text,     required: false, max_length: Some(20) },
];

const SUBJECT_TEMPLATE: &str = "<var Level>/ <var Subjectline> #<var BullNr>";

const BODY_TEMPLATE: &str = r#"<var Title> Bulletin For: <var Name>
Precedence: <var Level>
Bulletin #: <var BullNr>
Bulletin From: <var From_Name>
Date/Time: <var ActivityDateTime1>

<var Message>

------------------------------------
Express Sending Station: <MsgSender>
Express Version: <ProgramVersion>
Template Version: <var Templateversion>
"#;

pub const BULLETIN_INITIAL: FormDef = FormDef {
    id: "Bulletin_Initial",
    name: "Bulletin",
    fields: FIELDS,
    subject_template: SUBJECT_TEMPLATE,
    body_template: BODY_TEMPLATE,
    display_form: "Bulletin Viewer.html",
    reply_template: "",
};
