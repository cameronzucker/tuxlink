//! Damage Assessment — structured damage/survey report for EmComm incidents.
//!
//! Field schema mirrors `Damage_Assessment_Initial.html` from the WLE Standard
//! Templates catalog. Field IDs are lowercase (per spec §3 wire convention).
//! 12 fixed property-category rows + 3 optional "Other" categories + totals.

use crate::forms::types::{FieldKind, FormDef, FormField};

const FIELDS: &[FormField] = &[
    // Header fields
    FormField { id: "title",           label: "Title",                 kind: FieldKind::Text,     required: false, max_length: Some(60) },
    FormField { id: "status",          label: "Status",                kind: FieldKind::Text,     required: true,  max_length: Some(30) },
    FormField { id: "jur",             label: "Jurisdiction",          kind: FieldKind::Text,     required: true,  max_length: Some(60) },
    FormField { id: "surarea",         label: "Survey Area",           kind: FieldKind::Text,     required: true,  max_length: Some(60) },
    FormField { id: "datetime1",       label: "Event Date",            kind: FieldKind::Text,     required: false, max_length: Some(30) },
    FormField { id: "date",            label: "Survey Date",           kind: FieldKind::Date,     required: false, max_length: None },
    FormField { id: "misnum",          label: "Mission/Incident #",    kind: FieldKind::Text,     required: false, max_length: Some(30) },
    FormField { id: "event",           label: "Event Type",            kind: FieldKind::Text,     required: false, max_length: Some(60) },
    FormField { id: "other",           label: "Other Event",           kind: FieldKind::Text,     required: false, max_length: Some(60) },
    FormField { id: "surteam",         label: "Survey Team",           kind: FieldKind::Text,     required: false, max_length: Some(60) },
    FormField { id: "templateversion", label: "Template Version",      kind: FieldKind::Text,     required: false, max_length: Some(20) },
    // Category 1: Houses
    FormField { id: "aff1",    label: "Houses — Affected",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "min1",    label: "Houses — Minor",       kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "maj1",    label: "Houses — Major",       kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "des1",    label: "Houses — Totaled",     kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "total1",  label: "Houses — Total #",     kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "dollar1", label: "Houses — Costs",       kind: FieldKind::Text, required: false, max_length: Some(20) },
    // Category 2: Apartment Complex
    FormField { id: "aff2",    label: "Apartments — Affected", kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "min2",    label: "Apartments — Minor",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "maj2",    label: "Apartments — Major",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "des2",    label: "Apartments — Totaled",  kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "total2",  label: "Apartments — Total #",  kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "dollar2", label: "Apartments — Costs",    kind: FieldKind::Text, required: false, max_length: Some(20) },
    // Category 3: Mobile Homes
    FormField { id: "aff3",    label: "Mobile Homes — Affected", kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "min3",    label: "Mobile Homes — Minor",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "maj3",    label: "Mobile Homes — Major",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "des3",    label: "Mobile Homes — Totaled",  kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "total3",  label: "Mobile Homes — Total #",  kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "dollar3", label: "Mobile Homes — Costs",    kind: FieldKind::Text, required: false, max_length: Some(20) },
    // Category 4: Residential High Rise
    FormField { id: "aff4",    label: "Res High Rise — Affected", kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "min4",    label: "Res High Rise — Minor",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "maj4",    label: "Res High Rise — Major",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "des4",    label: "Res High Rise — Totaled",  kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "total4",  label: "Res High Rise — Total #",  kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "dollar4", label: "Res High Rise — Costs",    kind: FieldKind::Text, required: false, max_length: Some(20) },
    // Category 5: Commercial High Rise
    FormField { id: "aff5",    label: "Comm High Rise — Affected", kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "min5",    label: "Comm High Rise — Minor",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "maj5",    label: "Comm High Rise — Major",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "des5",    label: "Comm High Rise — Totaled",  kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "total5",  label: "Comm High Rise — Total #",  kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "dollar5", label: "Comm High Rise — Costs",    kind: FieldKind::Text, required: false, max_length: Some(20) },
    // Category 6: Public Buildings
    FormField { id: "aff6",    label: "Public Buildings — Affected", kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "min6",    label: "Public Buildings — Minor",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "maj6",    label: "Public Buildings — Major",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "des6",    label: "Public Buildings — Totaled",  kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "total6",  label: "Public Buildings — Total #",  kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "dollar6", label: "Public Buildings — Costs",    kind: FieldKind::Text, required: false, max_length: Some(20) },
    // Category 7: Small Business
    FormField { id: "aff7",    label: "Small Business — Affected", kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "min7",    label: "Small Business — Minor",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "maj7",    label: "Small Business — Major",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "des7",    label: "Small Business — Totaled",  kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "total7",  label: "Small Business — Total #",  kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "dollar7", label: "Small Business — Costs",    kind: FieldKind::Text, required: false, max_length: Some(20) },
    // Category 8: Factories/Industrial
    FormField { id: "aff8",    label: "Factories/Industrial — Affected", kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "min8",    label: "Factories/Industrial — Minor",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "maj8",    label: "Factories/Industrial — Major",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "des8",    label: "Factories/Industrial — Totaled",  kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "total8",  label: "Factories/Industrial — Total #",  kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "dollar8", label: "Factories/Industrial — Costs",    kind: FieldKind::Text, required: false, max_length: Some(20) },
    // Category 9: Roads
    FormField { id: "aff9",    label: "Roads — Affected", kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "min9",    label: "Roads — Minor",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "maj9",    label: "Roads — Major",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "des9",    label: "Roads — Totaled",  kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "total9",  label: "Roads — Total #",  kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "dollar9", label: "Roads — Costs",    kind: FieldKind::Text, required: false, max_length: Some(20) },
    // Category 10: Bridges
    FormField { id: "aff10",    label: "Bridges — Affected", kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "min10",    label: "Bridges — Minor",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "maj10",    label: "Bridges — Major",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "des10",    label: "Bridges — Totaled",  kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "total10",  label: "Bridges — Total #",  kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "dollar10", label: "Bridges — Costs",    kind: FieldKind::Text, required: false, max_length: Some(20) },
    // Category 11: Electrical Distribution
    FormField { id: "aff11",    label: "Electrical — Affected", kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "min11",    label: "Electrical — Minor",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "maj11",    label: "Electrical — Major",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "des11",    label: "Electrical — Totaled",  kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "total11",  label: "Electrical — Total #",  kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "dollar11", label: "Electrical — Costs",    kind: FieldKind::Text, required: false, max_length: Some(20) },
    // Category 12: Schools
    FormField { id: "aff12",    label: "Schools — Affected", kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "min12",    label: "Schools — Minor",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "maj12",    label: "Schools — Major",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "des12",    label: "Schools — Totaled",  kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "total12",  label: "Schools — Total #",  kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "dollar12", label: "Schools — Costs",    kind: FieldKind::Text, required: false, max_length: Some(20) },
    // Category 13: Other (user-named)
    FormField { id: "other13",  label: "Other Category 13 Name", kind: FieldKind::Text, required: false, max_length: Some(40) },
    FormField { id: "aff13",    label: "Other 13 — Affected",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "min13",    label: "Other 13 — Minor",       kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "maj13",    label: "Other 13 — Major",       kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "des13",    label: "Other 13 — Totaled",     kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "total13",  label: "Other 13 — Total #",     kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "dollar13", label: "Other 13 — Costs",       kind: FieldKind::Text, required: false, max_length: Some(20) },
    // Category 14: Other (user-named)
    FormField { id: "other14",  label: "Other Category 14 Name", kind: FieldKind::Text, required: false, max_length: Some(40) },
    FormField { id: "aff14",    label: "Other 14 — Affected",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "min14",    label: "Other 14 — Minor",       kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "maj14",    label: "Other 14 — Major",       kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "des14",    label: "Other 14 — Totaled",     kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "total14",  label: "Other 14 — Total #",     kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "dollar14", label: "Other 14 — Costs",       kind: FieldKind::Text, required: false, max_length: Some(20) },
    // Category 15: Other (user-named)
    FormField { id: "other15",  label: "Other Category 15 Name", kind: FieldKind::Text, required: false, max_length: Some(40) },
    FormField { id: "aff15",    label: "Other 15 — Affected",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "min15",    label: "Other 15 — Minor",       kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "maj15",    label: "Other 15 — Major",       kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "des15",    label: "Other 15 — Totaled",     kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "total15",  label: "Other 15 — Total #",     kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "dollar15", label: "Other 15 — Costs",       kind: FieldKind::Text, required: false, max_length: Some(20) },
    // Totals + comments
    FormField { id: "dollar16", label: "Total Dollar Cost",       kind: FieldKind::Text,     required: false, max_length: Some(30) },
    FormField { id: "comments", label: "Comments",                kind: FieldKind::LongText, required: false, max_length: Some(2000) },
];

const SUBJECT_TEMPLATE: &str = "Damage Assessment-<var Status>-<var Jur>-<var SurArea>";

const BODY_TEMPLATE: &str = r#"<var Title>

Event Date: <var DateTime1>

Survey Date: <var Date>
Survey Area: <var SurArea>

Jurisdication: <var Jur>
Misson/Incident #: <var MisNum>
Event Type: <var Event>
OTHER: <var Other>

Survey Team: <var SurTeam>
-------------------

SURVEY REPORT - CATEGORIES

HOUSES - Counts

Affected: <var Aff1>
Minor: <var Min1>
Major: <var Maj1>
Totaled: <var Des1>
Total number: <var total1>
Costs: <var Dollar1>

APARTMENT COMPLEX - Counts

Affected: <var Aff2>
Minor: <var Min2>
Major: <var Maj2>
Totaled: <var Des2>
Total number: <var total2>
Costs: <var Dollar2>

MOBILE HOMES - Counts

Affected: <var Aff3>
Minor: <var Min3>
Major: <var Maj3>
Totaled: <var Des3>
Total number: <var total3>
Costs: <var Dollar3>

RESIDENTIAL HIGH RISE - Counts

Affected: <var Aff4>
Minor: <var Min4>
Major: <var Maj4>
Totaled: <var Des4>
Total number: <var total4>
Costs: <var Dollar4>

COMMERCIAL HIGH RISE - Counts

Affected: <var Aff5>
Minor: <var Min5>
Major: <var Maj5>
Totaled: <var Des5>
Total number: <var total5>
Costs: <var Dollar5>

PUBLIC BUILDINGS - Counts

Affected: <var Aff6>
Minor: <var Min6>
Major: <var Maj6>
Totaled: <var Des6>
Total number: <var total6>
Costs: <var Dollar6>

SMALL BUISNESS - Counts

Affected: <var Aff7>
Minor: <var Min7>
Major: <var Maj7>
Totaled: <var Des7>
Total number: <var total7>
Costs: <var Dollar7>

FACTORIES/INDUSTRIAL - Counts

Affected: <var Aff8>
Minor: <var Min8>
Major: <var Maj8>
Totaled: <var Des8>
Total number: <var total8>
Costs: <var Dollar8>

ROADS - Counts

Affected: <var Aff9>
Minor: <var Min9>
Major: <var Maj9>
Totaled: <var Des9>
Total number: <var total9>
Costs: <var Dollar9>

BRIDGES - Counts

Affected: <var Aff10>
Minor: <var Min10>
Major: <var Maj10>
Totaled: <var Des10>
Total number: <var total10>
Costs: <var Dollar10>

ELECTRICAL DISTRIBUTION - Counts

Affected: <var Aff11>
Minor: <var Min11>
Major: <var Maj11>
Totaled: <var Des11>
Total number: <var total11>
Costs: <var Dollar11>

SCHOOLS - Counts

Affected: <var Aff12>
Minor: <var Min12>
Major: <var Maj12>
Totaled: <var Des12>
Total number: <var total12>
Costs: <var Dollar12>

<var Other13> - Counts

Affected: <var Aff13>
Minor: <var Min13>
Major: <var Maj13>
Totaled: <var Des13>
Total number: <var total13>
Costs: <var Dollar13>

<var Other14> - Counts

Affected: <var Aff14>
Minor: <var Min14>
Major: <var Maj14>
Totaled: <var Des14>
Total number: <var total14>
Costs: <var Dollar14>

<var Other15> - Counts

Affected: <var Aff15>
Minor: <var Min15>
Major: <var Maj15>
Totaled: <var Des15>
Total number: <var total15>
Costs: <var Dollar15>

TOTAL DOLLAR Cost: <var Dollar16>
-------------------------

Comments:(if any)

<var Comments>

-------------------------
Express Sender: [<var MsgSender>]
Senders Template Version: <var Templateversion>
"#;

pub const DAMAGE_ASSESSMENT_INITIAL: FormDef = FormDef {
    id: "Damage_Assessment_Initial",
    name: "Damage Assessment",
    fields: FIELDS,
    subject_template: SUBJECT_TEMPLATE,
    body_template: BODY_TEMPLATE,
    display_form: "Damage_Assessment_Viewer.html",
    reply_template: "",
};
