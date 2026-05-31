//! Public types for the forms module. Mirrored on the TS side in src/forms/types.ts.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq)]
pub struct FormDef {
    pub id: &'static str,
    pub name: &'static str,
    pub fields: &'static [FormField],
    pub subject_template: &'static str,
    pub body_template: &'static str,
    pub display_form: &'static str,
    pub reply_template: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FormField {
    pub id: &'static str,
    pub label: &'static str,
    pub kind: FieldKind,
    pub required: bool,
    pub max_length: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    Text,
    LongText,
    Date,
    Time,
    Boolean,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormPayload {
    pub form_id: String,
    pub form_parameters: FormParameters,
    pub fields: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormParameters {
    pub xml_file_version: String,
    pub rms_express_version: String,
    pub submission_datetime: String,
    pub senders_callsign: String,
    pub grid_square: String,
    pub display_form: String,
    pub reply_template: String,
}
