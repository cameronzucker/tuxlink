//! ICS-309 Communications Log — tracks radio traffic during an incident.
//!
//! Field schema mirrors `Form-309_Initial.html` from the WLE Standard Templates
//! catalog. Field IDs are lowercase (per spec §3 wire convention). The form
//! supports up to 30 log entries (time/from/to/sub per entry); unused entries
//! are left empty and render as blank lines on the wire.

use crate::forms::types::{FieldKind, FormDef, FormField};

const FIELDS: &[FormField] = &[
    // Header fields
    FormField { id: "title",             label: "Title",                   kind: FieldKind::Text,     required: true,  max_length: Some(60) },
    FormField { id: "page",              label: "Page #",                  kind: FieldKind::Text,     required: false, max_length: Some(10) },
    FormField { id: "task",              label: "Task #",                  kind: FieldKind::Text,     required: false, max_length: Some(20) },
    FormField { id: "taskname",          label: "Task Name",               kind: FieldKind::Text,     required: false, max_length: Some(60) },
    FormField { id: "activitydatetime1", label: "Date/Time Prepared",      kind: FieldKind::Text,     required: true,  max_length: Some(30) },
    FormField { id: "opper",             label: "Operational Period #",    kind: FieldKind::Text,     required: false, max_length: Some(20) },
    FormField { id: "opname",            label: "Radio Operator Name",     kind: FieldKind::Text,     required: true,  max_length: Some(60) },
    FormField { id: "operid",            label: "Station ID",              kind: FieldKind::Text,     required: true,  max_length: Some(30) },
    FormField { id: "templateversion",   label: "Template Version",        kind: FieldKind::Text,     required: false, max_length: Some(20) },
    // Log entry 1
    FormField { id: "time1",  label: "Entry 1 — Time",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "from1",  label: "Entry 1 — From",    kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "to1",    label: "Entry 1 — To",      kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "sub1",   label: "Entry 1 — Subject", kind: FieldKind::LongText, required: false, max_length: Some(200) },
    // Log entry 2
    FormField { id: "time2",  label: "Entry 2 — Time",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "from2",  label: "Entry 2 — From",    kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "to2",    label: "Entry 2 — To",      kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "sub2",   label: "Entry 2 — Subject", kind: FieldKind::LongText, required: false, max_length: Some(200) },
    // Log entry 3
    FormField { id: "time3",  label: "Entry 3 — Time",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "from3",  label: "Entry 3 — From",    kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "to3",    label: "Entry 3 — To",      kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "sub3",   label: "Entry 3 — Subject", kind: FieldKind::LongText, required: false, max_length: Some(200) },
    // Log entry 4
    FormField { id: "time4",  label: "Entry 4 — Time",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "from4",  label: "Entry 4 — From",    kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "to4",    label: "Entry 4 — To",      kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "sub4",   label: "Entry 4 — Subject", kind: FieldKind::LongText, required: false, max_length: Some(200) },
    // Log entry 5
    FormField { id: "time5",  label: "Entry 5 — Time",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "from5",  label: "Entry 5 — From",    kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "to5",    label: "Entry 5 — To",      kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "sub5",   label: "Entry 5 — Subject", kind: FieldKind::LongText, required: false, max_length: Some(200) },
    // Log entry 6
    FormField { id: "time6",  label: "Entry 6 — Time",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "from6",  label: "Entry 6 — From",    kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "to6",    label: "Entry 6 — To",      kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "sub6",   label: "Entry 6 — Subject", kind: FieldKind::LongText, required: false, max_length: Some(200) },
    // Log entry 7
    FormField { id: "time7",  label: "Entry 7 — Time",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "from7",  label: "Entry 7 — From",    kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "to7",    label: "Entry 7 — To",      kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "sub7",   label: "Entry 7 — Subject", kind: FieldKind::LongText, required: false, max_length: Some(200) },
    // Log entry 8
    FormField { id: "time8",  label: "Entry 8 — Time",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "from8",  label: "Entry 8 — From",    kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "to8",    label: "Entry 8 — To",      kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "sub8",   label: "Entry 8 — Subject", kind: FieldKind::LongText, required: false, max_length: Some(200) },
    // Log entry 9
    FormField { id: "time9",  label: "Entry 9 — Time",    kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "from9",  label: "Entry 9 — From",    kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "to9",    label: "Entry 9 — To",      kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "sub9",   label: "Entry 9 — Subject", kind: FieldKind::LongText, required: false, max_length: Some(200) },
    // Log entry 10
    FormField { id: "time10", label: "Entry 10 — Time",   kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "from10", label: "Entry 10 — From",   kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "to10",   label: "Entry 10 — To",     kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "sub10",  label: "Entry 10 — Subject",kind: FieldKind::LongText, required: false, max_length: Some(200) },
    // Log entries 11-30 (optional; empty entries produce blank XML elements)
    FormField { id: "time11", label: "Entry 11 — Time",   kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "from11", label: "Entry 11 — From",   kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "to11",   label: "Entry 11 — To",     kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "sub11",  label: "Entry 11 — Subject",kind: FieldKind::LongText, required: false, max_length: Some(200) },
    FormField { id: "time12", label: "Entry 12 — Time",   kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "from12", label: "Entry 12 — From",   kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "to12",   label: "Entry 12 — To",     kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "sub12",  label: "Entry 12 — Subject",kind: FieldKind::LongText, required: false, max_length: Some(200) },
    FormField { id: "time13", label: "Entry 13 — Time",   kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "from13", label: "Entry 13 — From",   kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "to13",   label: "Entry 13 — To",     kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "sub13",  label: "Entry 13 — Subject",kind: FieldKind::LongText, required: false, max_length: Some(200) },
    FormField { id: "time14", label: "Entry 14 — Time",   kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "from14", label: "Entry 14 — From",   kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "to14",   label: "Entry 14 — To",     kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "sub14",  label: "Entry 14 — Subject",kind: FieldKind::LongText, required: false, max_length: Some(200) },
    FormField { id: "time15", label: "Entry 15 — Time",   kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "from15", label: "Entry 15 — From",   kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "to15",   label: "Entry 15 — To",     kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "sub15",  label: "Entry 15 — Subject",kind: FieldKind::LongText, required: false, max_length: Some(200) },
    FormField { id: "time16", label: "Entry 16 — Time",   kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "from16", label: "Entry 16 — From",   kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "to16",   label: "Entry 16 — To",     kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "sub16",  label: "Entry 16 — Subject",kind: FieldKind::LongText, required: false, max_length: Some(200) },
    FormField { id: "time17", label: "Entry 17 — Time",   kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "from17", label: "Entry 17 — From",   kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "to17",   label: "Entry 17 — To",     kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "sub17",  label: "Entry 17 — Subject",kind: FieldKind::LongText, required: false, max_length: Some(200) },
    FormField { id: "time18", label: "Entry 18 — Time",   kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "from18", label: "Entry 18 — From",   kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "to18",   label: "Entry 18 — To",     kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "sub18",  label: "Entry 18 — Subject",kind: FieldKind::LongText, required: false, max_length: Some(200) },
    FormField { id: "time19", label: "Entry 19 — Time",   kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "from19", label: "Entry 19 — From",   kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "to19",   label: "Entry 19 — To",     kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "sub19",  label: "Entry 19 — Subject",kind: FieldKind::LongText, required: false, max_length: Some(200) },
    FormField { id: "time20", label: "Entry 20 — Time",   kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "from20", label: "Entry 20 — From",   kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "to20",   label: "Entry 20 — To",     kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "sub20",  label: "Entry 20 — Subject",kind: FieldKind::LongText, required: false, max_length: Some(200) },
    FormField { id: "time21", label: "Entry 21 — Time",   kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "from21", label: "Entry 21 — From",   kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "to21",   label: "Entry 21 — To",     kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "sub21",  label: "Entry 21 — Subject",kind: FieldKind::LongText, required: false, max_length: Some(200) },
    FormField { id: "time22", label: "Entry 22 — Time",   kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "from22", label: "Entry 22 — From",   kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "to22",   label: "Entry 22 — To",     kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "sub22",  label: "Entry 22 — Subject",kind: FieldKind::LongText, required: false, max_length: Some(200) },
    FormField { id: "time23", label: "Entry 23 — Time",   kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "from23", label: "Entry 23 — From",   kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "to23",   label: "Entry 23 — To",     kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "sub23",  label: "Entry 23 — Subject",kind: FieldKind::LongText, required: false, max_length: Some(200) },
    FormField { id: "time24", label: "Entry 24 — Time",   kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "from24", label: "Entry 24 — From",   kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "to24",   label: "Entry 24 — To",     kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "sub24",  label: "Entry 24 — Subject",kind: FieldKind::LongText, required: false, max_length: Some(200) },
    FormField { id: "time25", label: "Entry 25 — Time",   kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "from25", label: "Entry 25 — From",   kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "to25",   label: "Entry 25 — To",     kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "sub25",  label: "Entry 25 — Subject",kind: FieldKind::LongText, required: false, max_length: Some(200) },
    FormField { id: "time26", label: "Entry 26 — Time",   kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "from26", label: "Entry 26 — From",   kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "to26",   label: "Entry 26 — To",     kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "sub26",  label: "Entry 26 — Subject",kind: FieldKind::LongText, required: false, max_length: Some(200) },
    FormField { id: "time27", label: "Entry 27 — Time",   kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "from27", label: "Entry 27 — From",   kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "to27",   label: "Entry 27 — To",     kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "sub27",  label: "Entry 27 — Subject",kind: FieldKind::LongText, required: false, max_length: Some(200) },
    FormField { id: "time28", label: "Entry 28 — Time",   kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "from28", label: "Entry 28 — From",   kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "to28",   label: "Entry 28 — To",     kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "sub28",  label: "Entry 28 — Subject",kind: FieldKind::LongText, required: false, max_length: Some(200) },
    FormField { id: "time29", label: "Entry 29 — Time",   kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "from29", label: "Entry 29 — From",   kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "to29",   label: "Entry 29 — To",     kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "sub29",  label: "Entry 29 — Subject",kind: FieldKind::LongText, required: false, max_length: Some(200) },
    FormField { id: "time30", label: "Entry 30 — Time",   kind: FieldKind::Text, required: false, max_length: Some(10) },
    FormField { id: "from30", label: "Entry 30 — From",   kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "to30",   label: "Entry 30 — To",     kind: FieldKind::Text, required: false, max_length: Some(30) },
    FormField { id: "sub30",  label: "Entry 30 — Subject",kind: FieldKind::LongText, required: false, max_length: Some(200) },
];

const SUBJECT_TEMPLATE: &str = "Form 309- <var Title> - <var OpName> - <var OperId> - <var ActivityDateTime1>";

const BODY_TEMPLATE: &str = r#"<var Title> - Form 309

PAGE #: <var Page>

Task#: <var Task>
Task Name: <var TaskName>

Date/Time Prepared: <var ActivityDateTime1>
Operational Period #: <var OpPer>

Radio Operator Name:<var OpName>
Station ID: <var OperId>
Express Sender: <var MsgSender>

         LOG
----------------------------
TIME: <var Time1>
STATION ID:
FROM: <var From1>
TO:<var To1>
SUBJECT:
 <var Sub1>

TIME: <var Time2>
STATION ID:
FROM: <var From2>
TO: <var To2>
SUBJECT:
 <var Sub2>

TIME: <var Time3>
STATION ID:
FROM: <var From3>
TO: <var To3>
SUBJECT:
 <var Sub3>

TIME: <var Time4>
STATION ID:
FROM: <var From4>
TO: <var To4>
SUBJECT:
 <var Sub4>

TIME: <var Time5>
STATION ID:
FROM: <var From5>
TO: <var To5>
SUBJECT:
 <var Sub5>

TIME: <var Time6>
STATION ID:
FROM: <var From6>
TO: <var To6>
SUBJECT:
 <var Sub6>

TIME: <var Time7>
STATION ID:
FROM: <var From7>
TO: <var To7>
SUBJECT:
 <var Sub7>

TIME: <var Time8>
STATION ID:
FROM: <var From8>
TO: <var To8>
SUBJECT:
 <var Sub8>

TIME: <var Time9>
STATION ID:
FROM: <var From9>
TO: <var To9>
SUBJECT:
 <var Sub9>

TIME: <var Time10>
STATION ID:
FROM: <var From10>
TO: <var To10>
SUBJECT:
 <var Sub10>

TIME: <var Time11>
STATION ID:
FROM: <var From11>
TO: <var To11>
SUBJECT:
 <var Sub11>

TIME: <var Time12>
STATION ID:
FROM: <var From12>
TO: <var To12>
SUBJECT:
 <var Sub12>

TIME: <var Time13>
STATION ID:
FROM: <var From13>
TO: <var To13>
SUBJECT:
 <var Sub13>

TIME: <var Time14>
STATION ID:
FROM: <var From14>
TO: <var To14>
SUBJECT:
 <var Sub14>

TIME: <var Time15>
STATION ID:
FROM: <var From15>
TO: <var To15>
SUBJECT:
 <var Sub15>

TIME: <var Time16>
STATION ID:
FROM: <var From16>
TO: <var To16>
SUBJECT:
 <var Sub16>

TIME: <var Time17>
STATION ID:
FROM: <var From17>
TO: <var To17>
SUBJECT:
 <var Sub17>

TIME: <var Time18>
STATION ID:
FROM: <var From18>
TO: <var To18>
SUBJECT:
 <var Sub18>

TIME: <var Time19>
STATION ID:
FROM: <var From19>
TO: <var To19>
SUBJECT:
 <var Sub19>

TIME: <var Time20>
STATION ID:
FROM: <var From20>
TO: <var To20>
SUBJECT:
 <var Sub20>

TIME: <var Time21>
STATION ID:
FROM: <var From21>
TO: <var To21>
SUBJECT:
 <var Sub21>

TIME: <var Time22>
STATION ID:
FROM: <var From22>
TO: <var To22>
SUBJECT:
 <var Sub22>

TIME: <var Time23>
STATION ID:
FROM: <var From23>
TO: <var To23>
SUBJECT:
 <var Sub23>

TIME: <var Time24>
STATION ID:
FROM: <var From24>
TO: <var To24>
SUBJECT:
 <var Sub24>

TIME: <var Time25>
STATION ID:
FROM: <var From25>
TO: <var To25>
SUBJECT:
 <var Sub25>

TIME: <var Time26>
STATION ID:
FROM: <var From26>
TO: <var To26>
SUBJECT:
 <var Sub26>

TIME: <var Time27>
STATION ID:
FROM: <var From27>
TO: <var To27>
SUBJECT:
 <var Sub27>

TIME: <var Time28>
STATION ID:
FROM:  <var From28>
TO: <var To28>
SUBJECT:
 <var Sub28>

TIME: <var Time29>
STATION ID:
FROM: <var From29>
TO: <var To29>
SUBJECT:
 <var Sub29>

TIME: <var Time30>
STATION ID:
FROM: <var From30>
TO: <var To30>
SUBJECT:
 <var Sub30>

------------------
Express Sending Station: <MsgSender>
Senders Express Version: <ProgramVersion>
Senders Template Version: <var Templateversion>
[No changes or editing of this message are allowed]
"#;

pub const FORM309_INITIAL: FormDef = FormDef {
    id: "Form-309_Initial",
    name: "ICS-309 Communications Log",
    fields: FIELDS,
    subject_template: SUBJECT_TEMPLATE,
    body_template: BODY_TEMPLATE,
    display_form: "Form-309_Viewer.html",
    reply_template: "",
};
