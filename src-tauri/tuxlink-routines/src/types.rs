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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransmitAck {
    pub by: String,
    pub at: String,
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

/// Control-flow step payloads (spec §6 "Control flow").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "control", rename_all = "lowercase")]
pub enum Control {
    Branch {
        /// Variable path, e.g. "s1.connected".
        on: String,
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

impl RoutineDef {
    pub fn parse(json: &str) -> Result<Self, RoutineParseError> {
        let def: RoutineDef = serde_json::from_str(json)?;
        if def.schema_version != SUPPORTED_SCHEMA_VERSION {
            return Err(RoutineParseError::UnsupportedSchemaVersion(def.schema_version));
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
                Control::Branch { on, then, r#else } => {
                    assert_eq!(on, "s1.connected");
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
    fn unknown_schema_version_is_a_parse_error() {
        let bumped = SPEC_EXAMPLE.replace("\"schema_version\": 1", "\"schema_version\": 99");
        let err = RoutineDef::parse(&bumped).unwrap_err();
        assert!(matches!(err, RoutineParseError::UnsupportedSchemaVersion(99)));
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
