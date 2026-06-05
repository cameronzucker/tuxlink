//! Winlink Check-In — operator net check-in form.
//!
//! **KNOWN LIMITATION (2026-06-04 Codex full-diff adrev P1):** the FIELDS /
//! SUBJECT_TEMPLATE / BODY_TEMPLATE below were a simplified design choice and
//! do NOT match the bundled WLE `Winlink_Check_In_Initial.html` template's
//! `<var>` placeholder names — the WLE template uses CamelCase names that
//! lower-case to ids like `msgto`, `newsubject`, `organization`, `datetime`,
//! `msgsender`, `contactname`, `assigned`, `status`, `service`, `band`,
//! `session`, `location`, `maplat`, `maplon`, `mgrs`, `grid`, `locationsource`,
//! `comments`.
//!
//! Pending operator decision: (a) full WLE alignment (~18-field form),
//! (b) keep simplified UI but map UI fields → WLE payload at submit time,
//! or (c) ship the simplified subset and have CMS / other Winlink clients
//! treat the form as a generic message instead of a Check-In. See bd
//! follow-up filed alongside this PR.
//!
//! Until that decision lands, the CheckInForm React component is NOT
//! registered in the form registry (`src/forms/index.ts`) — the picker
//! falls through to the WLE webview Check-In template (P1 behavior). The
//! BUNDLED_FORMS entry below stays so backend `find_form` parity tests
//! continue to pass.
//!
//! Original provisional field mapping (kept for reference; rebuild against
//! the WLE template when re-enabling):
//!   tactical_call  — operator or station callsign checking in
//!   op_name        — operator full name
//!   group_net      — net or group name (e.g. "Cascadia ARES Net")
//!   status         — check-in status: "Ready" | "Standby" | "Out"
//!   comments       — free-text comments
//!   grid           — Maidenhead grid square (Maidenhead, 4- or 6-char)
//!   initials       — submitting operator initials

use crate::forms::types::{FieldKind, FormDef, FormField};

const FIELDS: &[FormField] = &[
    FormField { id: "tactical_call", label: "Tactical Call",   kind: FieldKind::Text,     required: true,  max_length: Some(20) },
    FormField { id: "op_name",       label: "Operator Name",   kind: FieldKind::Text,     required: false, max_length: Some(60) },
    FormField { id: "group_net",     label: "Group / Net",     kind: FieldKind::Text,     required: false, max_length: Some(60) },
    FormField { id: "status",        label: "Status",          kind: FieldKind::Text,     required: true,  max_length: Some(10) },
    FormField { id: "comments",      label: "Comments",        kind: FieldKind::LongText, required: false, max_length: Some(500) },
    FormField { id: "grid",          label: "Grid Square",     kind: FieldKind::Text,     required: false, max_length: Some(8) },
    FormField { id: "initials",      label: "Initials",        kind: FieldKind::Text,     required: false, max_length: Some(10) },
];

const SUBJECT_TEMPLATE: &str = "Check-In: <var tactical_call> / <var group_net> / <var status>";

const BODY_TEMPLATE: &str = r#"Winlink Check-In
Tactical Call: <var tactical_call>
Operator Name: <var op_name>
Group/Net: <var group_net>
Status: <var status>
Grid Square: <var grid>
Comments: <var comments>
Initials: <var initials>
------------------------------------
Sending Station: Tuxlink
[No changes or editing of this message are allowed]
"#;

pub const WINLINK_CHECK_IN: FormDef = FormDef {
    id: "Winlink_Check-In",
    name: "Winlink Check-In",
    fields: FIELDS,
    subject_template: SUBJECT_TEMPLATE,
    body_template: BODY_TEMPLATE,
    display_form: "Winlink_Check-In_Viewer.html",
    reply_template: "",
};
