//! Serde value types for routine definitions (spec §14).
//!
//! The export format IS the storage format: these types round-trip the JSON
//! files under the config directory's `routines/`.

use serde::{Deserialize, Serialize};

use crate::error::RoutineParseError;

pub const SUPPORTED_SCHEMA_VERSION: u32 = 1;

/// A step's stable identifier within its routine (e.g. `"s1"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StepId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransmitMode {
    Attended,
    Automatic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OnInterrupted {
    Stay,
    Resume,
}

/// Contention policy when a radio step wants a lease someone else holds (spec §9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BusyPolicy {
    #[default]
    Wait,
    Fail,
}

/// Operator acknowledgment recorded when `transmit_mode` is set to automatic
/// (spec §4). Recorded only by a UI act; MCP cannot supply it.
///
/// REUSED for both consent classes (spec §4, C3): the transmit ack
/// (`RoutineDef.transmit_ack`) and the config-write ack
/// (`RoutineDef.write_ack`). `closure_digest` binds the acknowledgment to the
/// exact closure the operator signed (the sha256 hex from
/// [`consent_closure::closure_digest`](crate::consent_closure::closure_digest));
/// a digest-less legacy ack is treated as stale by the validator. Additive on
/// the v1 shape: absent -> `None`, and `skip_serializing_if` keeps a legacy ack
/// round-tripping without a `closure_digest` key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransmitAck {
    pub by: String,
    pub at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closure_digest: Option<String>,
}

/// Declared routine input parameter (bound at invocation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputDecl {
    pub name: String,
    #[serde(default)]
    pub required: bool,
}

/// Missed-fire policy for schedules (spec §8): the anacron choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IfMissed {
    #[default]
    Skip,
    RunOnceOnLaunch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Trigger {
    Schedule {
        /// Interval like "30m", "2h", "45s" — parsed by `scheduler::every_seconds` (lands with the scheduler).
        every: String,
        /// Optional alignment: "hour" | "day".
        #[serde(default, skip_serializing_if = "Option::is_none")]
        align: Option<String>,
        /// Optional local-time window "HH:MM-HH:MM".
        #[serde(default, skip_serializing_if = "Option::is_none")]
        window: Option<String>,
        #[serde(default)]
        if_missed: IfMissed,
    },
    Manual,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionStep {
    pub id: StepId,
    /// Catalog action name, e.g. "radio.connect" (spec §6).
    pub action: String,
    #[serde(default)]
    pub params: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_s: Option<u64>,
    #[serde(default)]
    pub on_radio_busy: BusyPolicy,
}

/// Comparison operator for `Control::Branch`'s comparison form (round-2
/// missing link #2, bd tuxlink-iizmk): `{"on": "s1.k_index", "op": "gte",
/// "value": 4}`. Absent `op` keeps the original strict-boolean branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Lte,
    Gt,
    Gte,
}

/// Control-flow step payloads (spec §6 "Control flow").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "control", rename_all = "lowercase")]
pub enum Control {
    Branch {
        /// Variable path, e.g. "s1.connected" (nested paths reach deep
        /// output fields: "s1.indices.k_index").
        on: String,
        /// Comparison operator; `None` = strict-boolean branch. `op` and
        /// `value` are supplied together or not at all (the executor rejects
        /// a lone half verbatim).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        op: Option<CmpOp>,
        /// Right-hand side for `op`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<serde_json::Value>,
        then: Vec<StepId>,
        #[serde(rename = "else", default)]
        r#else: Vec<StepId>,
    },
    Delay {
        /// Relative delay like "+5m" / "300s", or aligned "next:hour".
        delay: String,
    },
    Retry {
        /// Step to wrap (must be an action step id in the same track).
        step: StepId,
        attempts: u32,
        #[serde(default)]
        backoff_s: u64,
    },
    Call {
        /// Name of the routine (or composite library step) to invoke.
        routine: String,
        #[serde(default)]
        args: serde_json::Value,
        /// true = await the child's result; false = fire-and-forget (spec §7).
        #[serde(default = "default_true")]
        sync: bool,
    },
    End {
        #[serde(default)]
        failed: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

fn default_true() -> bool {
    true
}

/// A control step: `{ "id": …, "control": "<kind>", …payload }`.
/// Flattened so the wire shape matches spec §14 exactly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControlStep {
    pub id: StepId,
    #[serde(flatten)]
    pub control: Control,
}

/// A canvas node. Untagged: an object with an "action" key is an action step,
/// one with a "control" key is a control step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Step {
    Action(ActionStep),
    Control(ControlStep),
}

impl Step {
    pub fn id(&self) -> &StepId {
        match self {
            Step::Action(a) => &a.id,
            Step::Control(c) => &c.id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub name: String,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutineDef {
    pub routine: String,
    pub schema_version: u32,
    pub transmit_mode: TransmitMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transmit_ack: Option<TransmitAck>,
    /// Config-write acknowledgment (C3, spec §4) — the `writes_config` sibling
    /// of `transmit_ack`, recorded when `transmit_mode` is automatic and the
    /// routine's write closure is non-empty. Additive on v1: absent -> `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write_ack: Option<TransmitAck>,
    #[serde(default = "default_on_interrupted")]
    pub on_interrupted: OnInterrupted,
    #[serde(default)]
    pub inputs: Vec<InputDecl>,
    pub triggers: Vec<Trigger>,
    pub tracks: Vec<Track>,
}

fn default_on_interrupted() -> OnInterrupted {
    OnInterrupted::Stay
}

/// Does this object look like a STEP rather than a track? Steps carry an
/// `action` or a `control` discriminant; tracks carry `name` + `steps`.
fn looks_like_step(o: &serde_json::Map<String, serde_json::Value>) -> bool {
    (o.contains_key("action") || o.contains_key("control")) && !o.contains_key("steps")
}

/// Does this object look like a TRACK rather than a step?
fn looks_like_track(o: &serde_json::Map<String, serde_json::Value>) -> bool {
    o.contains_key("steps") && !o.contains_key("action") && !o.contains_key("control")
}

/// Localise a parse failure to a JSON path, with domain-aware advice.
///
/// Runs ONLY after serde has already rejected the payload, so it costs nothing
/// on the happy path and never changes what is accepted. Returns `None` when it
/// cannot do better than serde, in which case serde's message stands.
///
/// The messages name the PATH, because the field name alone is not actionable:
/// `tracks[]` and `tracks[].steps[]` are both arrays of objects, so putting a
/// step in a track slot is a natural mistake whose serde error ("missing field
/// `name`") points at a level the author never touched.
pub(crate) fn structural_diagnosis(v: &serde_json::Value) -> Option<String> {
    let root = match v.as_object() {
        Some(o) => o,
        None => return Some("the definition must be a JSON object".to_string()),
    };

    if !root.contains_key("routine") {
        // The exact misdirection observed in the field: the author supplied
        // `name`, serde asked for `name` (from a nested Track), and the author
        // "fixed" the wrong level. Say which key this schema wants.
        return Some(if root.contains_key("name") {
            "top level: missing field `routine` — the routine's NAME belongs in \
             `routine`, but you supplied it as `name`. Rename the top-level `name` \
             key to `routine`"
                .to_string()
        } else {
            "top level: missing field `routine` (the routine's NAME string)".to_string()
        });
    }

    match root.get("tracks") {
        None => return Some("top level: missing field `tracks` (a list of tracks)".to_string()),
        Some(serde_json::Value::Array(tracks)) => {
            for (i, t) in tracks.iter().enumerate() {
                let o = match t.as_object() {
                    Some(o) => o,
                    None => return Some(format!("tracks[{i}] must be an object")),
                };
                if looks_like_step(o) {
                    return Some(format!(
                        "tracks[{i}] is a STEP, not a track — steps belong in \
                         tracks[N].steps, not directly in tracks[]. Move this object \
                         into the `steps` list of the track it belongs to"
                    ));
                }
                if !o.contains_key("name") {
                    return Some(format!("tracks[{i}]: missing field `name`"));
                }
                match o.get("steps") {
                    None => return Some(format!("tracks[{i}]: missing field `steps`")),
                    Some(serde_json::Value::Array(steps)) => {
                        for (j, s) in steps.iter().enumerate() {
                            let so = match s.as_object() {
                                Some(so) => so,
                                None => {
                                    return Some(format!("tracks[{i}].steps[{j}] must be an object"))
                                }
                            };
                            if looks_like_track(so) {
                                return Some(format!(
                                    "tracks[{i}].steps[{j}] is a TRACK, not a step — it has \
                                     its own `steps` list. Tracks belong at the top level's \
                                     `tracks`, not nested inside a track's steps"
                                ));
                            }
                            if !so.contains_key("action") && !so.contains_key("control") {
                                return Some(format!(
                                    "tracks[{i}].steps[{j}]: a step needs either `action` \
                                     (an action step) or `control` (branch / end / delay / \
                                     retry / call)"
                                ));
                            }
                            if !so.contains_key("id") {
                                return Some(format!("tracks[{i}].steps[{j}]: missing field `id`"));
                            }
                        }
                    }
                    Some(_) => return Some(format!("tracks[{i}]: `steps` must be a list")),
                }
            }
        }
        Some(_) => return Some("top level: `tracks` must be a list of tracks".to_string()),
    }

    if let Some(t) = root.get("triggers") {
        if !t.is_array() {
            return Some(
                "top level: `triggers` must be a LIST of trigger objects, e.g. \
                 [{\"type\": \"manual\"}]"
                    .to_string(),
            );
        }
    }
    None
}

impl RoutineDef {
    pub fn parse(json: &str) -> Result<Self, RoutineParseError> {
        let def: RoutineDef = match serde_json::from_str(json) {
            Ok(d) => d,
            Err(e) => {
                // serde named the field but not the path. Try to localise
                // before surfacing a message the author cannot act on.
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(json) {
                    if let Some(msg) = structural_diagnosis(&v) {
                        return Err(RoutineParseError::Structural(msg));
                    }
                }
                return Err(RoutineParseError::Json(e));
            }
        };
        if def.schema_version != SUPPORTED_SCHEMA_VERSION {
            return Err(RoutineParseError::UnsupportedSchemaVersion(
                def.schema_version,
            ));
        }
        Ok(def)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC_EXAMPLE: &str = r#"{
      "routine": "morning-ics-cycle",
      "schema_version": 1,
      "transmit_mode": "automatic",
      "transmit_ack": { "by": "KK7ABC", "at": "2026-07-13T20:00:00Z" },
      "on_interrupted": "stay",
      "inputs": [],
      "triggers": [
        { "type": "schedule", "every": "30m", "align": "hour",
          "window": "06:00-22:00", "if_missed": "skip" }
      ],
      "tracks": [
        { "name": "connect-cycle", "steps": [
          { "id": "s1", "action": "radio.connect",
            "params": { "stations": "@station-set:or-gateways",
                        "bands": ["40m", "80m"], "listen_before_tx_s": 5 },
            "timeout_s": 300, "on_radio_busy": "wait" },
          { "id": "s2", "control": "branch", "on": "s1.connected",
            "then": ["s3"], "else": ["s4"] }
        ] }
      ]
    }"#;

    #[test]
    fn parses_the_spec_example() {
        let def = RoutineDef::parse(SPEC_EXAMPLE).expect("spec example must parse");
        assert_eq!(def.routine, "morning-ics-cycle");
        assert_eq!(def.schema_version, 1);
        assert_eq!(def.transmit_mode, TransmitMode::Automatic);
        assert_eq!(def.on_interrupted, OnInterrupted::Stay);
        assert_eq!(def.transmit_ack.as_ref().unwrap().by, "KK7ABC");
        assert_eq!(def.tracks.len(), 1);
        assert_eq!(def.tracks[0].steps.len(), 2);
        match &def.tracks[0].steps[0] {
            Step::Action(a) => {
                assert_eq!(a.id.0, "s1");
                assert_eq!(a.action, "radio.connect");
                assert_eq!(a.timeout_s, Some(300));
                assert_eq!(a.on_radio_busy, BusyPolicy::Wait);
            }
            other => panic!("step 0 should be an action, got {other:?}"),
        }
        match &def.tracks[0].steps[1] {
            Step::Control(c) => match &c.control {
                Control::Branch {
                    on,
                    op,
                    value,
                    then,
                    r#else,
                } => {
                    assert_eq!(on, "s1.connected");
                    assert_eq!((*op, value.as_ref()), (None, None));
                    assert_eq!(then, &vec![StepId("s3".into())]);
                    assert_eq!(r#else, &vec![StepId("s4".into())]);
                }
                other => panic!("expected branch, got {other:?}"),
            },
            other => panic!("step 1 should be a control, got {other:?}"),
        }
    }

    #[test]
    fn serializes_back_to_equivalent_json() {
        let def = RoutineDef::parse(SPEC_EXAMPLE).unwrap();
        let round = RoutineDef::parse(&serde_json::to_string(&def).unwrap()).unwrap();
        assert_eq!(def, round);
    }

    #[test]
    fn old_def_without_write_ack_or_closure_digest_parses_and_reserializes_without_them() {
        // A v1 def predating C3: no write_ack, and a transmit_ack with no
        // closure_digest. Must parse (additive #[serde(default)]) and, thanks
        // to skip_serializing_if, re-serialize without injecting the new keys.
        let def = RoutineDef::parse(SPEC_EXAMPLE).unwrap();
        assert!(def.write_ack.is_none());
        assert!(def.transmit_ack.as_ref().unwrap().closure_digest.is_none());

        let reserialized = serde_json::to_value(&def).unwrap();
        let obj = reserialized.as_object().unwrap();
        assert!(
            !obj.contains_key("write_ack"),
            "absent write_ack must not serialize"
        );
        let ack = obj["transmit_ack"].as_object().unwrap();
        assert!(
            !ack.contains_key("closure_digest"),
            "absent closure_digest must not serialize"
        );
    }

    #[test]
    fn write_ack_and_closure_digest_round_trip_when_present() {
        let with_new_fields = SPEC_EXAMPLE
            .replace(
                "\"transmit_ack\": { \"by\": \"KK7ABC\", \"at\": \"2026-07-13T20:00:00Z\" },",
                "\"transmit_ack\": { \"by\": \"KK7ABC\", \"at\": \"2026-07-13T20:00:00Z\", \
                 \"closure_digest\": \"abc123\" }, \
                 \"write_ack\": { \"by\": \"KK7ABC\", \"at\": \"2026-07-13T20:05:00Z\", \
                 \"closure_digest\": \"def456\" },",
            );
        let def = RoutineDef::parse(&with_new_fields).unwrap();
        assert_eq!(
            def.transmit_ack.as_ref().unwrap().closure_digest.as_deref(),
            Some("abc123")
        );
        let wa = def.write_ack.as_ref().unwrap();
        assert_eq!(wa.by, "KK7ABC");
        assert_eq!(wa.closure_digest.as_deref(), Some("def456"));

        let round = RoutineDef::parse(&serde_json::to_string(&def).unwrap()).unwrap();
        assert_eq!(def, round);
    }

    #[test]
    fn unknown_schema_version_is_a_parse_error() {
        let bumped = SPEC_EXAMPLE.replace("\"schema_version\": 1", "\"schema_version\": 99");
        let err = RoutineDef::parse(&bumped).unwrap_err();
        assert!(matches!(
            err,
            RoutineParseError::UnsupportedSchemaVersion(99)
        ));
    }

    // --- structural_diagnosis: localise the fault (tuxlink-mrp4u) ----------

    /// The exact shape from base/S3/build/attempt-1: a STEP object sitting in
    /// the `tracks` array. serde reports "missing field `name`" (it is trying
    /// to read the step as a Track) with a byte offset, naming a level the
    /// author never touched. 24 saves, 23 of them byte-identical, zero output.
    #[test]
    fn a_step_object_in_the_tracks_array_is_named_by_path() {
        let json = r#"{
          "routine": "nws-weather-check", "schema_version": 1,
          "transmit_mode": "attended", "triggers": [{"type":"manual"}],
          "tracks": [
            {"name":"track-1","steps":[{"action":"local.log","id":"s1","params":{}}]},
            {"action":"data.find_stations","id":"s2","params":{}}
          ]
        }"#;
        let err = RoutineDef::parse(json).unwrap_err();
        let m = err.to_string();
        assert!(m.contains("tracks[1]"), "must name the path: {m}");
        assert!(m.contains("STEP"), "must say what the object actually is: {m}");
        assert!(
            !m.contains("missing field `name`"),
            "must NOT surface serde's field-only message: {m}"
        );
    }

    /// The regression the old message CAUSED: the author read "missing field
    /// `name`" and renamed its correct top-level `routine` key to `name`. The
    /// diagnosis must name that specific swap, not just the missing field.
    #[test]
    fn a_top_level_name_instead_of_routine_is_called_out_by_name() {
        let json = r#"{
          "name": "nws-weather-check", "schema_version": 1,
          "transmit_mode": "attended", "triggers": [{"type":"manual"}],
          "tracks": [{"name":"t1","steps":[{"action":"local.log","id":"s1","params":{}}]}]
        }"#;
        let err = RoutineDef::parse(json).unwrap_err();
        let m = err.to_string();
        assert!(m.contains("top level"), "must name the level: {m}");
        assert!(m.contains("`routine`"), "must name the key this schema wants: {m}");
        assert!(
            m.contains("`name`"),
            "must name the key the author actually supplied: {m}"
        );
    }

    #[test]
    fn a_track_nested_in_a_steps_list_is_named_by_path() {
        let json = r#"{
          "routine": "r", "schema_version": 1, "transmit_mode": "attended",
          "triggers": [{"type":"manual"}],
          "tracks": [{"name":"t1","steps":[{"name":"inner","steps":[]}]}]
        }"#;
        let m = RoutineDef::parse(json).unwrap_err().to_string();
        assert!(m.contains("tracks[0].steps[0]"), "{m}");
        assert!(m.contains("TRACK"), "{m}");
    }

    #[test]
    fn a_step_with_neither_action_nor_control_is_named_by_path() {
        let json = r#"{
          "routine": "r", "schema_version": 1, "transmit_mode": "attended",
          "triggers": [{"type":"manual"}],
          "tracks": [{"name":"t1","steps":[{"id":"s1","params":{}}]}]
        }"#;
        let m = RoutineDef::parse(json).unwrap_err().to_string();
        assert!(m.contains("tracks[0].steps[0]"), "{m}");
        assert!(m.contains("action") && m.contains("control"), "{m}");
    }

    #[test]
    fn a_valid_definition_still_parses_and_the_precheck_never_runs() {
        // The pre-check only runs after serde has already rejected a payload,
        // so it can never change what is accepted.
        let def = RoutineDef::parse(SPEC_EXAMPLE).expect("spec example must parse");
        assert_eq!(def.routine, "morning-ics-cycle");
        assert!(structural_diagnosis(&serde_json::from_str(SPEC_EXAMPLE).unwrap()).is_none());
    }

    #[test]
    fn an_unlocalisable_failure_still_falls_back_to_serdes_message() {
        // Bad enum variant deep in a trigger: the pre-check has no better
        // answer, so serde's message must survive rather than be swallowed.
        let json = r#"{
          "routine": "r", "schema_version": 1, "transmit_mode": "attended",
          "triggers": [{"type":"cron","expr":"* * * * *"}],
          "tracks": [{"name":"t1","steps":[{"action":"local.log","id":"s1","params":{}}]}]
        }"#;
        let err = RoutineDef::parse(json).unwrap_err();
        assert!(matches!(err, RoutineParseError::Json(_)), "{err}");
        assert!(err.to_string().contains("cron"), "{err}");
    }

    #[test]
    fn transmit_mode_and_ack_shape_survive_rename_all() {
        // serde rename_all on enums renames TAGS only (project pitfall):
        // assert the exact wire strings so a refactor can't silently change them.
        let j = serde_json::to_value(TransmitMode::Automatic).unwrap();
        assert_eq!(j, serde_json::json!("automatic"));
        let j = serde_json::to_value(OnInterrupted::Stay).unwrap();
        assert_eq!(j, serde_json::json!("stay"));
        let j = serde_json::to_value(BusyPolicy::Fail).unwrap();
        assert_eq!(j, serde_json::json!("fail"));
    }
}
