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

use serde::{Deserialize, Serialize};

/// The operator's (or agent's) stated goal for a routine before any drafting
/// starts — the workflow's phase-0 input, captured verbatim rather than
/// inferred, so later phases can be checked against what was actually asked
/// for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Intent {
    pub outcome: String,
    pub trigger: String,
    pub success: String,
    pub failure: String,
    pub side_effects: Vec<String>,
    /// Named values the routine is expected to persist across runs (e.g. a
    /// last-contacted station). Empty when the intent implies no
    /// cross-run state.
    pub persisted_values: Vec<String>,
}

/// One catalog action projected down to the compact fields the workflow's
/// affordance-discovery phase needs — a subset of
/// `crate::routines::commands::ActionInfo` (which carries UI-only fields
/// like `label`/`example_params` that this phase does not need).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AffordanceAction {
    pub name: String,
    pub transmits: bool,
    pub needs_radio: bool,
    pub writes_config: bool,
    pub params: Vec<String>,
    pub outputs: Vec<String>,
}

/// The affordance-discovery phase's output: what the catalog can currently
/// do, plus what the intent seems to need but the catalog does not (yet)
/// offer — the signal a later phase uses to say "I can't build this."
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Affordances {
    pub actions: Vec<AffordanceAction>,
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
    pub then: Vec<String>,
    #[serde(rename = "else")]
    pub r#else: Vec<String>,
}

/// One node in a draft routine graph — a routine step's shape, as plain
/// data (not yet a validated `tuxlink_routines::types::Step`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftNode {
    pub id: String,
    pub action: String,
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
    pub built: String,
    pub inferred_decisions: Vec<String>,
    pub failure_behavior: String,
    pub gaps: Vec<String>,
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
