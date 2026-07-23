//! Typed phase artifacts for Elmer's multi-phase "build me a routine"
//! workflow (Routine CI slice 1a, Task 1).
//!
//! Each workflow phase (`Router -> Intent -> Feasibility -> Draft -> Emit ->
//! Ci -> Present`) produces one of the plain-data structs below. They are the
//! type contract every later task in this slice builds against: no behavior
//! lives here, only shapes + round-trip serde. Convention mirrors the
//! `ActionInfo` DTO pattern in `crate::routines::commands`: `Debug, Clone,
//! PartialEq, Serialize, Deserialize` on every artifact, `#[serde(rename_all
//! = "camelCase")]` on structs, `#[serde(rename_all = "lowercase")]` on
//! simple-tag enums.
//!
//! `DraftBranch`'s inner shape is modeled on
//! `tuxlink_routines::types::Control::Branch` (the brief named
//! `DraftNode.branch: Option<...>` without fixing the inner type). `Depth`
//! and `PhaseName` are dictated by the design's phase pipeline (Tasks 2/7/8),
//! not the Task 1 brief.

use serde::{Deserialize, Deserializer, Serialize};

// ---------------------------------------------------------------------------
// Lenient field coercion for real small-model output (tuxlink-ch4po F1b)
// ---------------------------------------------------------------------------
//
// The StubModel that CI feeds these artifacts hands back clean, schema-exact
// JSON, so the strict derived `Deserialize` was invisibly fine. Real "build me
// a routine" models (qwen-class) do NOT: they routinely NEST an object where a
// flat-string field is declared — e.g. `"trigger": {"schedule": "hourly"}`
// instead of `"trigger": "hourly at :00"` — or emit a bare string where a list
// is declared. Before this, the Intent phase's `serde_json::from_value` rejected
// that with `invalid type: map, expected a string`, the workflow stopped at
// Intent, and the whole Full arm saved NOTHING (the crux measurement was
// impossible). These `deserialize_with` adapters coerce the realistic
// off-shape variants rather than reject them.
//
// This is the type-directed home for the coercion the F1b handoff describes —
// preferred over a blind value-walk in `phases::parse_artifact`, which cannot
// tell a declared-`String` field (coerce) from a declared-`Value` field like
// `DraftNode.params` (leave arbitrary JSON untouched). The struct fields carry
// that knowledge; the adapters key off it. A plain string / array round-trips
// unchanged, so `Serialize -> Deserialize` still holds and the strict CI stub
// path is unaffected.

/// Coerce any JSON value to a `String`: a JSON string is used verbatim; anything
/// else (object / array / number / bool / null) is rendered as its compact JSON
/// text so a nested answer survives as inspectable content instead of aborting
/// the phase.
fn value_to_string(v: serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s,
        other => other.to_string(),
    }
}

/// serde `deserialize_with` adapter for a declared-`String` field that tolerates
/// a non-string JSON value by compacting it (see [`value_to_string`]).
fn de_string_lenient<'de, D>(de: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(value_to_string(serde_json::Value::deserialize(de)?))
}

/// serde `deserialize_with` adapter for a declared-`Vec<String>` field that
/// tolerates (a) an array whose elements are non-strings (each coerced via
/// [`value_to_string`]), (b) a single bare value where a list was expected
/// (wrapped into a one-element vec), or (c) `null` (empty vec).
fn de_vec_string_lenient<'de, D>(de: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(match serde_json::Value::deserialize(de)? {
        serde_json::Value::Null => Vec::new(),
        serde_json::Value::Array(items) => items.into_iter().map(value_to_string).collect(),
        other => vec![value_to_string(other)],
    })
}

/// The operator's (or agent's) stated goal for a routine before any drafting
/// starts — the workflow's phase-0 input, captured verbatim rather than
/// inferred, so later phases can be checked against what was actually asked
/// for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Intent {
    #[serde(deserialize_with = "de_string_lenient")]
    pub outcome: String,
    #[serde(deserialize_with = "de_string_lenient")]
    pub trigger: String,
    #[serde(deserialize_with = "de_string_lenient")]
    pub success: String,
    #[serde(deserialize_with = "de_string_lenient")]
    pub failure: String,
    #[serde(deserialize_with = "de_vec_string_lenient")]
    pub side_effects: Vec<String>,
    /// Named values the routine is expected to persist across runs (e.g. a
    /// last-contacted station). Empty when the intent implies no
    /// cross-run state.
    #[serde(deserialize_with = "de_vec_string_lenient")]
    pub persisted_values: Vec<String>,
}

/// One catalog action projected down to the compact fields the workflow's
/// affordance-discovery phase needs — a subset of
/// `crate::routines::commands::ActionInfo` (which carries UI-only fields
/// like `label`/`example_params` that this phase does not need).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AffordanceAction {
    #[serde(deserialize_with = "de_string_lenient")]
    pub name: String,
    pub transmits: bool,
    pub needs_radio: bool,
    pub writes_config: bool,
    #[serde(deserialize_with = "de_vec_string_lenient")]
    pub params: Vec<String>,
    #[serde(deserialize_with = "de_vec_string_lenient")]
    pub outputs: Vec<String>,
}

/// The affordance-discovery phase's output: what the catalog can currently
/// do, plus what the intent seems to need but the catalog does not (yet)
/// offer — the signal a later phase uses to say "I can't build this."
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Affordances {
    pub actions: Vec<AffordanceAction>,
    #[serde(deserialize_with = "de_vec_string_lenient")]
    pub missing_primitives: Vec<String>,
}

/// A draft node's branch metadata — the flattened `then`/`else` step-id
/// lists mirroring `tuxlink_routines::types::Control::Branch`, carried as
/// plain `String` ids so [`Draft`] has no dependency on the routines crate
/// (Task 1 constraint: no external-crate calls). **Inferred**: the brief
/// names `branch: Option<...>` on [`DraftNode`] without specifying the
/// inner shape; this mirrors the real `Control::Branch` `then`/`else` pair
/// as the closest verbatim source in the codebase.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftBranch {
    #[serde(deserialize_with = "de_vec_string_lenient")]
    pub then: Vec<String>,
    #[serde(rename = "else", deserialize_with = "de_vec_string_lenient")]
    pub r#else: Vec<String>,
}

/// One node in a draft routine graph — a routine step's shape, as plain
/// data (not yet a validated `tuxlink_routines::types::Step`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftNode {
    #[serde(deserialize_with = "de_string_lenient")]
    pub id: String,
    #[serde(deserialize_with = "de_string_lenient")]
    pub action: String,
    /// Arbitrary step params — NOT coerced: a `DraftNode`'s params are declared
    /// as free-form JSON, so a nested object here is the intended shape, not the
    /// off-shape string coercion the flat-string fields tolerate.
    pub params: serde_json::Value,
    pub branch: Option<DraftBranch>,
}

/// The drafting phase's output: a candidate routine graph, not yet
/// validated or saved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub nodes: Vec<DraftNode>,
}

/// Pass/fail verdict for a [`CiReport`] — a simple tag, no payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CiVerdict {
    Green,
    Red,
}

/// One CI finding against a [`Draft`] — an owned mirror of
/// `tuxlink_routines::validate::findings::Finding`. Owned rather than
/// borrowing `Finding` directly because `Finding.code` is `&'static str`
/// (tied to the validator's static finding vocabulary), which this
/// artifact does not want to be coupled to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiFinding {
    pub code: String,
    pub severity: String,
    pub message: String,
}

/// The CI phase's output: whether the draft passed static validation, and
/// why not if it didn't.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiReport {
    pub verdict: CiVerdict,
    pub findings: Vec<CiFinding>,
}

/// The final phase's output: a human-readable summary of what got built,
/// the decisions Elmer inferred along the way (so the operator can correct
/// them), and what still needs an explicit ack before the routine can run
/// for real.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Present {
    #[serde(deserialize_with = "de_string_lenient")]
    pub built: String,
    #[serde(deserialize_with = "de_vec_string_lenient")]
    pub inferred_decisions: Vec<String>,
    #[serde(deserialize_with = "de_string_lenient")]
    pub failure_behavior: String,
    #[serde(deserialize_with = "de_vec_string_lenient")]
    pub gaps: Vec<String>,
    #[serde(deserialize_with = "de_vec_string_lenient")]
    pub acks_required: Vec<String>,
}

/// Which phase a [`PhaseRecord`] describes, in workflow pipeline order:
/// `Router` (depth selection) -> `Intent` -> `Feasibility` (produces the
/// [`Affordances`] artifact) -> `Draft` -> `Emit` (model calls the edit
/// verbs) -> `Ci` (deterministic validate + wire-walk) -> `Present`.
/// Model-turn phases carry a nonzero `prompt_tokens` in their
/// [`PhaseRecord`]; the deterministic `Ci` and the slice-1a template
/// `Present` carry zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PhaseName {
    Router,
    Intent,
    Feasibility,
    Draft,
    Emit,
    Ci,
    Present,
}

/// The workflow depth the router selects for an intent. `Minimal` collapses
/// the front phases (Intent/Feasibility/Draft) and runs Author -> Ci ->
/// Present; `Full` runs every phase. The router (Task 8) sets it; it is
/// defined here because [`WorkflowRun::depth`] records the depth a run used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Depth {
    Minimal,
    Full,
}

/// One phase's execution record within a [`WorkflowRun`] — what ran, its
/// token cost, and a short human-readable outcome summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhaseRecord {
    pub name: PhaseName,
    pub prompt_tokens: u64,
    pub outcome: String,
    /// The phase model turn's RAW output, captured for post-hoc inspection
    /// (tuxlink-ch4po F1b debug aid). For an artifact phase this is the model's
    /// `final_text` (the JSON the phase tried to parse — so a capture FAILURE is
    /// diagnosable from the bundle instead of invisible behind `NullTranscript`);
    /// for the Emit phase, which answers in tool calls not text, a compact dump
    /// of the issued calls; for a non-`Completed` turn, the bounded outcome kind.
    /// Bounded in size at the capture site. `None` for phases that never run a
    /// model turn (Ci, template Present). `#[serde(default)]` +
    /// `skip_serializing_if` keep older bundles and the CI stub path unaffected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_output: Option<String>,
}

/// The top-level record of one workflow run: which phases actually ran,
/// what got saved (if anything), the final presentation (if the run made
/// it that far), and why the run stopped when it did not reach its target
/// depth.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRun {
    pub depth: Depth,
    pub phases_run: Vec<PhaseRecord>,
    pub saved_routine: Option<String>,
    pub present: Option<Present>,
    pub stopped_reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intent_roundtrips_through_json() {
        let intent = Intent {
            outcome: "connect nearest 20m gateway hourly".into(),
            trigger: "schedule: hourly at :00".into(),
            success: "mail pulled".into(),
            failure: "log and retry next cycle".into(),
            side_effects: vec!["radio TX".into()],
            persisted_values: vec![],
        };
        let json = serde_json::to_string(&intent).unwrap();
        let back: Intent = serde_json::from_str(&json).unwrap();
        assert_eq!(intent, back);
    }

    // --- F1b: lenient coercion of real small-model (qwen-class) off-shapes ---

    // The exact re-pilot failure shape: qwen nests an OBJECT where the flat
    // `trigger` (and here `success`) String is declared. Before the lenient
    // adapters this was `invalid type: map, expected a string` and the Full arm
    // captured nothing. Now the nested object survives as its compact JSON text.
    #[test]
    fn intent_coerces_nested_object_string_fields() {
        let raw = serde_json::json!({
            "outcome": "pull VARA mail hourly",
            "trigger": { "schedule": "hourly at :00" },
            "success": ["mail pulled"],
            "failure": "log and retry",
            "sideEffects": "radio TX",
            "persistedValues": null
        });
        let intent: Intent = serde_json::from_value(raw).expect("nested shapes coerce");
        assert_eq!(intent.outcome, "pull VARA mail hourly");
        // Object -> compact JSON text.
        assert_eq!(intent.trigger, r#"{"schedule":"hourly at :00"}"#);
        // Array -> compact JSON text (a String field, not a list).
        assert_eq!(intent.success, r#"["mail pulled"]"#);
        assert_eq!(intent.failure, "log and retry");
        // Bare string where a list was declared -> one-element vec.
        assert_eq!(intent.side_effects, vec!["radio TX".to_string()]);
        // null -> empty vec.
        assert!(intent.persisted_values.is_empty());
    }

    // A Vec<String> that arrives with non-string elements coerces each element
    // rather than rejecting the whole field.
    #[test]
    fn vec_string_field_coerces_non_string_elements() {
        let raw = serde_json::json!({
            "actions": [],
            "missingPrimitives": ["propagation.predict", { "name": "x" }, 7]
        });
        let aff: Affordances = serde_json::from_value(raw).expect("mixed elements coerce");
        assert_eq!(
            aff.missing_primitives,
            vec![
                "propagation.predict".to_string(),
                r#"{"name":"x"}"#.to_string(),
                "7".to_string(),
            ]
        );
    }

    // A plain, schema-exact Intent (the CI StubModel shape) still round-trips
    // byte-for-byte through the lenient path — the tolerance never mutates
    // already-correct output.
    #[test]
    fn intent_plain_string_fields_are_unchanged_by_lenient_path() {
        let json = r#"{"outcome":"o","trigger":"t","success":"s","failure":"f","sideEffects":["a","b"],"persistedValues":[]}"#;
        let intent: Intent = serde_json::from_str(json).expect("plain parses");
        assert_eq!(intent.trigger, "t");
        assert_eq!(intent.side_effects, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn ci_report_green_when_no_errors() {
        let report = CiReport {
            verdict: CiVerdict::Green,
            findings: vec![],
        };
        assert!(matches!(report.verdict, CiVerdict::Green));
    }

    #[test]
    fn draft_roundtrips_through_json() {
        let draft = Draft {
            nodes: vec![
                DraftNode {
                    id: "s1".into(),
                    action: "radio.connect".into(),
                    params: serde_json::json!({ "bands": ["20m"] }),
                    branch: None,
                },
                DraftNode {
                    id: "s2".into(),
                    action: "control.branch".into(),
                    params: serde_json::Value::Null,
                    branch: Some(DraftBranch {
                        then: vec!["s3".into()],
                        r#else: vec!["s4".into()],
                    }),
                },
            ],
        };
        let json = serde_json::to_string(&draft).unwrap();
        let back: Draft = serde_json::from_str(&json).unwrap();
        assert_eq!(draft, back);
    }

    #[test]
    fn present_roundtrips_through_json() {
        let present = Present {
            built: "hourly 20m gateway connect".into(),
            inferred_decisions: vec!["defaulted retry to 3 attempts".into()],
            failure_behavior: "log and retry next cycle".into(),
            gaps: vec!["no station-set configured yet".into()],
            acks_required: vec!["writes_config".into()],
        };
        let json = serde_json::to_string(&present).unwrap();
        let back: Present = serde_json::from_str(&json).unwrap();
        assert_eq!(present, back);
    }

    #[test]
    fn workflow_run_roundtrips_through_json() {
        let run = WorkflowRun {
            depth: Depth::Full,
            phases_run: vec![PhaseRecord {
                name: PhaseName::Intent,
                prompt_tokens: 128,
                outcome: "captured".into(),
                raw_output: None,
            }],
            saved_routine: Some("morning-ics-cycle".into()),
            present: None,
            stopped_reason: None,
        };
        let json = serde_json::to_string(&run).unwrap();
        let back: WorkflowRun = serde_json::from_str(&json).unwrap();
        assert_eq!(run, back);
    }
}
