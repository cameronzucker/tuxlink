//! Winlink Check-In — net check-in form, WLE Standard-Forms-aligned.
//!
//! Field schema mirrors `Winlink_Check_In_Initial.html` / `Winlink Check-in.txt`
//! from the WLE Standard Templates catalog. Field IDs are lowercase (per spec
//! §3 wire convention); the WLE viewer's case-insensitive `<var>` substitution
//! handles the case mismatch with the body template's CamelCase placeholders
//! (matches how `ics213.rs`, `bulletin.rs`, etc. are wired).
//!
//! The body template is a verbatim copy of WLE's `Winlink Check-in.txt`
//! starting AFTER the `Msg:` framing line, preserving the section headers
//! (0. HEADER, 1. STATION, 2. SESSION, 3. LOCATION, 4. COMMENTS). The wire
//! subject is just `<var Newsubject>` — the operator-supplied subject line.
//!
//! The native [`CheckInForm`] React component emits payload keys matching
//! every operator-visible field below + the three template-metadata fields
//! (`datetime`, `templateversion`, `mapfilename`) which are auto-filled at
//! submit time so the WLE viewer renders correctly.
//!
//! Template-machinery fields the WLE form uses internally but tuxlink does NOT
//! emit on the wire (they're not referenced in the body template):
//! `extgps`, `gps`, `gpsvalid`, `internetcheck`, `timestamp`, `timestamp2`,
//! `addformat`, `w3w`, `testlonfld`, `attached_file`, the `b*` radio mirrors.

use crate::forms::types::{FieldKind, FormDef, FormField};

const FIELDS: &[FormField] = &[
    // 0. HEADER
    FormField { id: "organization",    label: "Organization",      kind: FieldKind::Text,     required: true,  max_length: Some(60) },
    FormField { id: "newsubject",      label: "Subject",           kind: FieldKind::Text,     required: true,  max_length: Some(80) },
    FormField { id: "exercise_id",     label: "Exercise ID",       kind: FieldKind::Text,     required: false, max_length: Some(25) },

    // 1. STATION
    FormField { id: "datetime",        label: "Date/Time",         kind: FieldKind::Text,     required: true,  max_length: Some(30) },
    FormField { id: "msgto",           label: "To",                kind: FieldKind::Text,     required: true,  max_length: Some(60) },
    FormField { id: "msgsender",       label: "From",              kind: FieldKind::Text,     required: true,  max_length: Some(12) },
    FormField { id: "contactname",     label: "Station Contact",   kind: FieldKind::Text,     required: true,  max_length: Some(60) },
    FormField { id: "assigned",        label: "Initial Operators", kind: FieldKind::Text,     required: false, max_length: Some(60) },

    // 2. SESSION
    FormField { id: "status",          label: "Type",              kind: FieldKind::Text,     required: true,  max_length: Some(13) },
    FormField { id: "service",         label: "Service",           kind: FieldKind::Text,     required: true,  max_length: Some(13) },
    FormField { id: "band",            label: "Band",              kind: FieldKind::Text,     required: true,  max_length: Some(13) },
    FormField { id: "session",         label: "Session",           kind: FieldKind::Text,     required: true,  max_length: Some(15) },

    // 3. LOCATION
    FormField { id: "location",        label: "Location",          kind: FieldKind::Text,     required: false, max_length: Some(60) },
    FormField { id: "maplat",          label: "Latitude",          kind: FieldKind::Text,     required: false, max_length: Some(15) },
    FormField { id: "maplon",          label: "Longitude",         kind: FieldKind::Text,     required: false, max_length: Some(15) },
    FormField { id: "mgrs",            label: "MGRS",              kind: FieldKind::Text,     required: false, max_length: Some(20) },
    FormField { id: "grid",            label: "Grid Square",       kind: FieldKind::Text,     required: false, max_length: Some(8)  },
    FormField { id: "locationsource",  label: "Location Source",   kind: FieldKind::Text,     required: false, max_length: Some(20) },

    // 4. COMMENTS
    FormField { id: "comments",        label: "Comments",          kind: FieldKind::LongText, required: false, max_length: Some(2000) },

    // Template machinery (auto-filled at submit; no UI). Required by the WLE
    // viewer's body-template <var> substitution — omit them and the rendered
    // body has literal `<var Templateversion>` / `<var Mapfilename>` lines.
    FormField { id: "templateversion", label: "Template Version",  kind: FieldKind::Text,     required: false, max_length: Some(40) },
    FormField { id: "mapfilename",     label: "Map Filename",      kind: FieldKind::Text,     required: false, max_length: Some(40) },
];

const SUBJECT_TEMPLATE: &str = "<var Newsubject>";

const BODY_TEMPLATE: &str = r#"Winlink Check-in
0. HEADER
  0a: Organization:	<var Organization>
  0b: Subject:	<var Newsubject>
  0c: Event/Exercise ID: <var exercise_id>

1. STATION
  1a. Date/Time:	<var DateTime>
  1b. To:	<var MsgTo>
  1c. From:	<var MsgSender>
  1d. Station Contact Name:	<var ContactName>
  1e. Initial Operators:	<var Assigned>

2. SESSION
  2a. Type:	<var Status>
  2b. Service:	<var Service>
  2c. Band:	<var Band>
  2d. Session:	<var Session>

3. LOCATION
  3a. Location:	<var Location>
  3b. Latitude:	<var mapLat>
  3c. Longitude:	<var mapLon>
  3d. MGRS:	<var MGRS>
  3e. Grid Square:	<var Grid>
  3f. Location sources:	<var locationSource>
-------------------------------------------------------------
4a COMMENTS:

<var Comments>


-------------------------------------------------------------

<var Templateversion>
Map file name: <var Mapfilename>

[No changes or editing of this message are allowed]
----"#;

pub const WINLINK_CHECK_IN: FormDef = FormDef {
    id: "Winlink_Check-In",
    name: "Winlink Check-In",
    fields: FIELDS,
    subject_template: SUBJECT_TEMPLATE,
    body_template: BODY_TEMPLATE,
    display_form: "Winlink_Check_In_Viewer.html",
    reply_template: "",
};
