//! GPS Position Report — compact position/location message.
//!
//! Field schema mirrors `GPS Position Report.html` from the WLE Standard Templates
//! catalog. Field IDs are lowercase (per spec §3 wire convention).
//!
//! Note: The WLE template uses `GPS Position Report.html` as both the compose
//! and display form (single-HTML pattern). The form_id uses `Position_Report`
//! (underscore-separated) since the WLE name `GPS Position Report` contains
//! spaces which violate the `^[A-Za-z0-9_-]{1,64}$` regex.

use crate::forms::types::{FieldKind, FormDef, FormField};

const FIELDS: &[FormField] = &[
    FormField { id: "thetime",  label: "Time",      kind: FieldKind::Text, required: true,  max_length: Some(20) },
    FormField { id: "lat",      label: "Latitude",  kind: FieldKind::Text, required: true,  max_length: Some(20) },
    FormField { id: "lon",      label: "Longitude", kind: FieldKind::Text, required: true,  max_length: Some(20) },
    FormField { id: "message",  label: "Comment",   kind: FieldKind::LongText, required: false, max_length: Some(200) },
];

const SUBJECT_TEMPLATE: &str = "Position Report";

const BODY_TEMPLATE: &str = r#"Time: <var thetime>
Latitude: <var Lat>
Longitude: <var Lon>
Comment: <var Message>
"#;

pub const POSITION_REPORT: FormDef = FormDef {
    id: "Position_Report",
    name: "GPS Position Report",
    fields: FIELDS,
    subject_template: SUBJECT_TEMPLATE,
    body_template: BODY_TEMPLATE,
    display_form: "GPS Position Report.html",
    reply_template: "",
};
