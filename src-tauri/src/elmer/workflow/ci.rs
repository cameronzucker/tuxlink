//! Routine CI wrapper (Routine CI slice 1a, Task 5): the Elmer workflow's
//! `Ci` phase (see [`super::artifacts::PhaseName::Ci`]) — a thin,
//! deterministic adapter from `tuxlink_routines::validate::validate` onto
//! this workflow's own [`super::artifacts::CiReport`] shape, so later
//! workflow phases (Present, Task 7) depend on this crate's artifact types
//! rather than reaching into `tuxlink_routines::validate::findings::Finding`
//! directly.
//!
//! No behavior beyond the map: `validate` already runs every wired
//! sub-check (`refs`, `capability`, `contracts`, `structure`, `consent`,
//! `triggers`, `params`) internally, so this module must NOT call any of
//! those (e.g. `structure::check`) a second time.

use tuxlink_routines::types::RoutineDef;
use tuxlink_routines::validate::{validate, Severity, ValidationContext};

use super::artifacts::{CiFinding, CiReport, CiVerdict};

/// Run static validation over `def` and translate the result into this
/// workflow's [`CiReport`] shape. Verdict is `Red` iff at least one
/// [`Finding`](tuxlink_routines::validate::Finding) is `Severity::Error`;
/// a warnings-only (or clean) result is `Green`, with any warning findings
/// still attached to `CiReport::findings` so the `Present` phase (Task 7)
/// can surface them even on a passing run.
pub fn run_routine_ci(def: &RoutineDef, ctx: &dyn ValidationContext) -> CiReport {
    let findings = validate(def, ctx);

    let verdict = if findings.iter().any(|f| f.severity == Severity::Error) {
        CiVerdict::Red
    } else {
        CiVerdict::Green
    };

    let findings = findings
        .into_iter()
        .map(|f| CiFinding {
            code: f.code.to_string(),
            severity: format!("{:?}", f.severity).to_lowercase(),
            message: f.message.clone(),
        })
        .collect();

    CiReport { verdict, findings }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tuxlink_routines::types::{
        ActionStep, BusyPolicy, OnInterrupted, Step, StepId, Track, TransmitMode, Trigger,
        SUPPORTED_SCHEMA_VERSION,
    };
    use tuxlink_routines::validate::StaticContext;

    /// Same shape as `tuxlink_routines::validate::tests::trivially_valid_routine`
    /// (one empty track, manual trigger, attended mode) — the smallest def
    /// that `validate()` itself asserts returns zero findings.
    fn clean_routine() -> RoutineDef {
        RoutineDef {
            routine: "clean".into(),
            schema_version: SUPPORTED_SCHEMA_VERSION,
            transmit_mode: TransmitMode::Attended,
            transmit_ack: None,
            write_ack: None,
            on_interrupted: OnInterrupted::Stay,
            inputs: vec![],
            triggers: vec![Trigger::Manual],
            tracks: vec![Track {
                name: "t".into(),
                steps: vec![],
            }],
        }
    }

    #[test]
    fn clean_routine_is_green_with_no_findings() {
        let def = clean_routine();
        let ctx = StaticContext::new();

        let report = run_routine_ci(&def, &ctx);

        assert_eq!(report.verdict, CiVerdict::Green);
        assert!(report.findings.is_empty());
    }

    /// Red-path fixture: a step whose action is not registered in the
    /// context. This fires `refs::check`'s `UNKNOWN_ACTION`, which is
    /// `Severity::Error` (see `tuxlink-routines/src/validate/refs.rs`).
    ///
    /// **Deviation from the Task 5 brief's named fixture**: the brief asked
    /// for a two-parallel-tracks-on-one-rig `RoutineDef` producing `Red`
    /// with a `SAME_RIG_PARALLEL_LANES` finding. Reading
    /// `tuxlink-routines/src/validate/capability.rs::same_rig_parallel_lanes_finding`
    /// shows that finding is built with `Finding::warning(...)`, not
    /// `Finding::error(...)` — so on `origin/main` today,
    /// `SAME_RIG_PARALLEL_LANES` alone can never produce a `Red` verdict
    /// under this task's Red-iff-any-Error rule; it would assert `Green`
    /// with a warning attached, not `Red`. Per the brief's own fallback
    /// ("write the closest error-producing fixture you can... flag the
    /// SAME_RIG_PARALLEL_LANES specifics for the Task 12 scorer work"),
    /// this test uses `UNKNOWN_ACTION` (an Error-severity finding) to prove
    /// the Red path instead. Flagging for Task 12 (the scorer): if the
    /// scorer's corpus expects `SAME_RIG_PARALLEL_LANES` to gate a routine
    /// red, either the validator's severity needs revisiting or the scorer
    /// must not treat it as blocking.
    #[test]
    fn routine_with_unregistered_action_is_red() {
        let def = RoutineDef {
            routine: "broken".into(),
            schema_version: SUPPORTED_SCHEMA_VERSION,
            transmit_mode: TransmitMode::Attended,
            transmit_ack: None,
            write_ack: None,
            on_interrupted: OnInterrupted::Stay,
            inputs: vec![],
            triggers: vec![Trigger::Manual],
            tracks: vec![Track {
                name: "t".into(),
                steps: vec![Step::Action(ActionStep {
                    id: StepId("s1".into()),
                    action: "radio.mystery".into(),
                    params: serde_json::json!({}),
                    timeout_s: None,
                    on_radio_busy: BusyPolicy::Wait,
                })],
            }],
        };
        // "radio.mystery" is never registered via `.with_action(...)`, so
        // `ctx.action_descriptor("radio.mystery")` is `None` and
        // `refs::check` fires `UNKNOWN_ACTION`.
        let ctx = StaticContext::new();

        let report = run_routine_ci(&def, &ctx);

        assert_eq!(report.verdict, CiVerdict::Red);
        assert!(report
            .findings
            .iter()
            .any(|f| f.code == "UNKNOWN_ACTION" && f.severity == "error"));
    }

    #[test]
    fn warning_only_findings_stay_green_but_are_still_attached() {
        // A radio-needing action with no rig configured produces a
        // Severity::Warning NO_RIG_CONFIGURED finding (capability.rs) —
        // this must not flip the verdict to Red, but must still show up in
        // CiReport::findings so a later Present phase can surface it.
        use tuxlink_routines::action::ActionDescriptor;

        const RADIO_CONNECT: ActionDescriptor = ActionDescriptor {
            writes_config: false,
            name: "radio.connect",
            label: "",
            description: "",
            needs_radio: true,
            transmits: true,
            needs_internet: false,
            example_params: None,
            allowed_values: None,
            params: &[],
            outputs: &[],
            dry_run_shape: None,
        };

        let def = RoutineDef {
            routine: "warns-only".into(),
            schema_version: SUPPORTED_SCHEMA_VERSION,
            transmit_mode: TransmitMode::Attended,
            transmit_ack: None,
            write_ack: None,
            on_interrupted: OnInterrupted::Stay,
            inputs: vec![],
            triggers: vec![Trigger::Manual],
            tracks: vec![Track {
                name: "t".into(),
                steps: vec![Step::Action(ActionStep {
                    id: StepId("s1".into()),
                    action: "radio.connect".into(),
                    params: serde_json::json!({}),
                    timeout_s: None,
                    on_radio_busy: BusyPolicy::Wait,
                })],
            }],
        };
        let ctx = StaticContext::new().with_action(RADIO_CONNECT);

        let report = run_routine_ci(&def, &ctx);

        assert_eq!(report.verdict, CiVerdict::Green);
        assert!(report
            .findings
            .iter()
            .any(|f| f.code == "NO_RIG_CONFIGURED" && f.severity == "warning"));
    }
}
