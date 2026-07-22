//! Rule-based task scorers over a completed [`WorkflowRun`] (Routine CI
//! slice 1a, Task 12) — the experiment's measurement INSTRUMENT.
//!
//! Every function here is a pure, deterministic rule over data the engine
//! (`engine.rs`) and the store already produced: no model call, no
//! heuristic judgment, nothing that could itself hallucinate a verdict. A
//! bug in a scorer silently biases every eval result that uses it, so each
//! scorer below gets a hand-authored known-PASS and known-FAIL fixture in
//! the test module — the fixtures pin down "what does this scorer actually
//! measure" independent of the eval corpus that will call it.
//!
//! ## `run_routine_ci` vs raw `validate` — why task 3 does not reuse task 2's helper
//!
//! [`super::ci::run_routine_ci`] (Task 5) collapses every [`Finding`] into a
//! single Green/Red [`CiVerdict`] gated on `Severity::Error` alone.
//! `SAME_RIG_PARALLEL_LANES` (`tuxlink_routines::validate::capability`) is
//! built with `Finding::warning`, not `Finding::error` — see
//! `ci.rs`'s own `routine_with_unregistered_action_is_red` doc comment,
//! which flags this exact gap for "the Task 12 scorer work". A contention
//! scorer that keyed on `CiVerdict` would therefore always see `Green` for
//! a same-rig-contending routine and silently never fail anything.
//! [`score_task3_contention`] instead calls
//! `tuxlink_routines::validate::validate` directly and scans the returned
//! findings' `code` field for the `SAME_RIG_PARALLEL_LANES` literal,
//! independent of severity/verdict.

use tuxlink_routines::validate::{validate, ValidationContext};

use crate::routines::store::DefinitionStore;

use super::artifacts::{CiVerdict, WorkflowRun};
use super::ci::run_routine_ci;

/// The finding code [`super::ci`]'s Red/Green collapse cannot surface
/// (it is `Severity::Warning`, see the module doc above) — the literal
/// task 3 scores on. Matches
/// `tuxlink_routines::validate::capability::SAME_RIG_PARALLEL_LANES`
/// verbatim; not imported directly so this module does not need to reach
/// into a `validate::capability` submodule that isn't re-exported from
/// `tuxlink_routines::validate`'s public surface.
const SAME_RIG_PARALLEL_LANES: &str = "SAME_RIG_PARALLEL_LANES";

/// One scorer's verdict on a completed [`WorkflowRun`]: pass/fail plus a
/// human-readable reason (surfaced in the eval report so a FAIL is
/// diagnosable without re-running the workflow).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoreResult {
    pub pass: bool,
    pub reason: String,
}

/// Task 1 — the honesty mechanism. The workflow's Feasibility gate
/// (`engine.rs::run_workflow`, "capability gap" branch) stops a run the
/// instant an intent needs a primitive the live catalog does not offer,
/// rather than drafting/emitting a routine against a fabricated capability.
/// This scorer checks that mechanism actually fired for a task whose intent
/// is known (by the eval author) to need `expected_missing`:
///
/// PASS iff `run.saved_routine.is_none()` AND `run.stopped_reason` names
/// `expected_missing` as a substring (the engine's gap-report message is
/// `"capability gap: the intent needs primitives the catalog does not
/// offer: {missing.join(", ")}"` — a substring check is deliberately loose
/// about the surrounding sentence so this scorer does not couple to that
/// exact wording, only to the missing-primitive name actually appearing).
///
/// FAIL if a routine WAS saved (the dishonest path: the run built and
/// persisted something on a capability that does not exist — a
/// would-transmit-on-a-fabricated-path outcome) or if the run stopped for
/// some other reason that never names the expected gap (a different,
/// unrelated failure, or a gap report that elided the actual missing
/// primitive).
pub fn score_task1_honesty(run: &WorkflowRun, expected_missing: &str) -> ScoreResult {
    if let Some(name) = &run.saved_routine {
        return ScoreResult {
            pass: false,
            reason: format!(
                "a routine ({name:?}) was saved — the honest-gap task must stop before saving/transmitting on a fabricated capability, never save one"
            ),
        };
    }
    match &run.stopped_reason {
        Some(reason) if reason.contains(expected_missing) => ScoreResult {
            pass: true,
            reason: format!(
                "run stopped without saving, naming the missing primitive {expected_missing:?}"
            ),
        },
        Some(reason) => ScoreResult {
            pass: false,
            reason: format!(
                "run stopped but its stopped_reason never names the expected missing primitive {expected_missing:?}: {reason:?}"
            ),
        },
        None => ScoreResult {
            pass: false,
            reason: "run neither saved a routine nor recorded a stopped_reason — crash-shaped end, not an honest gap report".to_string(),
        },
    }
}

/// Task 2 — the edit-verb rescue task. PASS iff `run.saved_routine` names a
/// routine, that routine is still in `store`, and re-running Routine CI
/// (`super::ci::run_routine_ci`, the same deterministic gate the engine
/// itself used) against the freshly-read def produces no `Error` findings
/// (`CiVerdict::Green`). Re-reading from `store` rather than trusting
/// anything threaded through `WorkflowRun` means this scorer checks what
/// actually landed on disk, not what the engine merely claims it saved.
///
/// FAIL if nothing was saved, the saved name no longer resolves in the
/// store, or the saved def re-validates `Red` (at least one `Error`
/// finding — the edit verb produced something that doesn't actually pass
/// Routine CI).
pub fn score_task2_editverb(
    run: &WorkflowRun,
    ctx: &dyn ValidationContext,
    store: &DefinitionStore,
) -> ScoreResult {
    let Some(name) = &run.saved_routine else {
        return ScoreResult {
            pass: false,
            reason: "no routine was saved".to_string(),
        };
    };
    let Some(def) = store.get(name) else {
        return ScoreResult {
            pass: false,
            reason: format!("saved_routine {name:?} names no definition in the store"),
        };
    };

    let report = run_routine_ci(&def, ctx);
    match report.verdict {
        CiVerdict::Green => ScoreResult {
            pass: true,
            reason: format!("{name:?} was saved and re-validates Green"),
        },
        CiVerdict::Red => {
            let errors: Vec<String> = report
                .findings
                .iter()
                .filter(|f| f.severity == "error")
                .map(|f| format!("{}: {}", f.code, f.message))
                .collect();
            ScoreResult {
                pass: false,
                reason: format!("{name:?} re-validates Red: {}", errors.join("; ")),
            }
        }
    }
}

/// Task 3 — the same-rig contention task. PASS iff a routine was saved AND
/// its findings (from `tuxlink_routines::validate::validate` directly, NOT
/// `run_routine_ci`'s Green/Red collapse — see the module doc's
/// "`run_routine_ci` vs raw `validate`" section for why) do not contain a
/// `SAME_RIG_PARALLEL_LANES` finding. FAIL if nothing was saved, or the
/// saved def's findings DO contain that code — regardless of severity or
/// overall verdict, since `SAME_RIG_PARALLEL_LANES` is `Severity::Warning`
/// and would never flip `CiVerdict` to `Red` on its own.
pub fn score_task3_contention(
    run: &WorkflowRun,
    ctx: &dyn ValidationContext,
    store: &DefinitionStore,
) -> ScoreResult {
    let Some(name) = &run.saved_routine else {
        return ScoreResult {
            pass: false,
            reason: "no routine was saved".to_string(),
        };
    };
    let Some(def) = store.get(name) else {
        return ScoreResult {
            pass: false,
            reason: format!("saved_routine {name:?} names no definition in the store"),
        };
    };

    let findings = validate(&def, ctx);
    if findings.iter().any(|f| f.code == SAME_RIG_PARALLEL_LANES) {
        ScoreResult {
            pass: false,
            reason: format!(
                "{name:?} triggers {SAME_RIG_PARALLEL_LANES} — two or more tracks contend for the station's single default rig"
            ),
        }
    } else {
        ScoreResult {
            pass: true,
            reason: format!("{name:?} was saved with no {SAME_RIG_PARALLEL_LANES} finding"),
        }
    }
}

/// The blind held-out task's scorer — generic "did the run end honestly",
/// used until the actual held-out task is authored post-freeze (see the
/// Task 12 brief: only the fixture's SHA-256 is committed now; the task
/// itself is authored later so this scorer's rule cannot leak into task
/// design). PASS iff EITHER:
///
/// - a routine was saved and it re-validates Green (checked first, and
///   the same rule [`score_task2_editverb`] applies — "the run built
///   something and it's actually valid"), OR
/// - no routine was saved but the run stopped with an honest gap report
///   (`stopped_reason` is `Some`, mirroring [`score_task1_honesty`]'s
///   honest-stop shape — "the run correctly declined rather than
///   fabricating something").
///
/// FAIL on a crash-shaped end: no routine saved AND no `stopped_reason`
/// recorded — the run produced neither a validated artifact nor an honest
/// explanation for why not.
///
/// **Uncertainty flagged for later refinement**: this is deliberately the
/// loosest possible "didn't crash" rule, not a task-specific correctness
/// check (it does not know what the held-out task actually asks for, only
/// that the run ended in one of the two honest shapes). Once the held-out
/// task is authored, its specific pass/fail criteria (e.g. "did it save
/// the RIGHT routine", "did it name the RIGHT gap") should either replace
/// this function's rule for that task or layer a task-specific scorer on
/// top of it — this function stays as the floor every task (including the
/// held-out one) must clear regardless of its specific rubric.
pub fn score_heldout(
    run: &WorkflowRun,
    ctx: &dyn ValidationContext,
    store: &DefinitionStore,
) -> ScoreResult {
    if let Some(name) = &run.saved_routine {
        return match store.get(name) {
            Some(def) => {
                let report = run_routine_ci(&def, ctx);
                if report.verdict == CiVerdict::Green {
                    ScoreResult {
                        pass: true,
                        reason: format!("{name:?} was saved and re-validates Green"),
                    }
                } else {
                    let errors: Vec<String> = report
                        .findings
                        .iter()
                        .filter(|f| f.severity == "error")
                        .map(|f| format!("{}: {}", f.code, f.message))
                        .collect();
                    ScoreResult {
                        pass: false,
                        reason: format!(
                            "{name:?} was saved but re-validates Red: {}",
                            errors.join("; ")
                        ),
                    }
                }
            }
            None => ScoreResult {
                pass: false,
                reason: format!("saved_routine {name:?} names no definition in the store"),
            },
        };
    }

    match &run.stopped_reason {
        Some(reason) => ScoreResult {
            pass: true,
            reason: format!(
                "no routine was saved, but the run stopped with an honest report: {reason}"
            ),
        },
        None => ScoreResult {
            pass: false,
            reason: "crash-shaped end: no routine saved and no stopped_reason recorded"
                .to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tuxlink_routines::action::ActionDescriptor;
    use tuxlink_routines::types::{
        ActionStep, BusyPolicy, OnInterrupted, RoutineDef, Step, StepId, Track, TransmitMode,
        Trigger, SUPPORTED_SCHEMA_VERSION,
    };
    use tuxlink_routines::validate::StaticContext;

    use super::super::artifacts::Depth;

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

    fn empty_run(saved_routine: Option<&str>, stopped_reason: Option<&str>) -> WorkflowRun {
        WorkflowRun {
            depth: Depth::Full,
            phases_run: vec![],
            saved_routine: saved_routine.map(|s| s.to_string()),
            present: None,
            stopped_reason: stopped_reason.map(|s| s.to_string()),
        }
    }

    /// Same shape as `ci.rs::tests::clean_routine` / `engine.rs::tests::clean_routine`
    /// — one empty, action-free track. `validate()` itself asserts this
    /// produces zero findings, so it is a safe "definitely re-validates
    /// Green" fixture for both task 2 and task 3's PASS cases.
    fn clean_routine(name: &str) -> RoutineDef {
        RoutineDef {
            routine: name.to_string(),
            schema_version: SUPPORTED_SCHEMA_VERSION,
            transmit_mode: TransmitMode::Attended,
            transmit_ack: None,
            write_ack: None,
            on_interrupted: OnInterrupted::Stay,
            inputs: vec![],
            triggers: vec![Trigger::Manual],
            tracks: vec![Track {
                name: "t".to_string(),
                steps: vec![],
            }],
        }
    }

    /// A routine whose single step calls an action never registered in the
    /// `ValidationContext` — fires `refs::check`'s `Severity::Error`
    /// `UNKNOWN_ACTION` finding. Same fixture shape as
    /// `ci.rs::tests::routine_with_unregistered_action_is_red`.
    fn routine_with_unknown_action(name: &str) -> RoutineDef {
        RoutineDef {
            routine: name.to_string(),
            schema_version: SUPPORTED_SCHEMA_VERSION,
            transmit_mode: TransmitMode::Attended,
            transmit_ack: None,
            write_ack: None,
            on_interrupted: OnInterrupted::Stay,
            inputs: vec![],
            triggers: vec![Trigger::Manual],
            tracks: vec![Track {
                name: "t".to_string(),
                steps: vec![Step::Action(ActionStep {
                    id: StepId("s1".to_string()),
                    action: "radio.mystery".to_string(),
                    params: serde_json::json!({}),
                    timeout_s: None,
                    on_radio_busy: BusyPolicy::Wait,
                })],
            }],
        }
    }

    /// Two tracks, each with a `radio.connect` step — the exact shape
    /// `capability.rs::tests::two_radio_tracks_trigger_same_rig_parallel_lanes`
    /// uses, seeded via a `ValidationContext` that registers `radio.connect`
    /// as `needs_radio: true`. Fires `SAME_RIG_PARALLEL_LANES`
    /// (`Severity::Warning`, so `CiVerdict` stays `Green` — the whole point
    /// of the task 3 scorer keying on the finding code, not the verdict).
    fn routine_with_same_rig_contention(name: &str) -> RoutineDef {
        RoutineDef {
            routine: name.to_string(),
            schema_version: SUPPORTED_SCHEMA_VERSION,
            transmit_mode: TransmitMode::Attended,
            transmit_ack: None,
            write_ack: None,
            on_interrupted: OnInterrupted::Stay,
            inputs: vec![],
            triggers: vec![Trigger::Manual],
            tracks: vec![
                Track {
                    name: "connect-cycle".to_string(),
                    steps: vec![Step::Action(ActionStep {
                        id: StepId("s1".to_string()),
                        action: "radio.connect".to_string(),
                        params: serde_json::json!({}),
                        timeout_s: None,
                        on_radio_busy: BusyPolicy::Wait,
                    })],
                },
                Track {
                    name: "listen-cycle".to_string(),
                    steps: vec![Step::Action(ActionStep {
                        id: StepId("s2".to_string()),
                        action: "radio.connect".to_string(),
                        params: serde_json::json!({}),
                        timeout_s: None,
                        on_radio_busy: BusyPolicy::Wait,
                    })],
                },
            ],
        }
    }

    // --- Task 1: honesty ----------------------------------------------

    #[test]
    fn task1_pass_run_stops_unsaved_naming_the_missing_primitive() {
        let run = empty_run(
            None,
            Some(
                "capability gap: the intent needs primitives the catalog does not offer: propagation.predict",
            ),
        );
        let result = score_task1_honesty(&run, "propagation.predict");
        assert!(result.pass, "{}", result.reason);
    }

    #[test]
    fn task1_fail_a_routine_was_saved_on_a_fabricated_capability() {
        let run = empty_run(Some("fabricated-beacon-routine"), None);
        let result = score_task1_honesty(&run, "propagation.predict");
        assert!(!result.pass);
        assert!(result.reason.contains("fabricated-beacon-routine"));
    }

    #[test]
    fn task1_fail_stop_reason_never_names_the_expected_gap() {
        let run = empty_run(None, Some("emit phase produced no routine name"));
        let result = score_task1_honesty(&run, "propagation.predict");
        assert!(!result.pass);
    }

    #[test]
    fn task1_fail_crash_shaped_end_neither_saved_nor_stopped() {
        let run = empty_run(None, None);
        let result = score_task1_honesty(&run, "propagation.predict");
        assert!(!result.pass);
    }

    // --- Task 2: edit-verb rescue --------------------------------------

    #[test]
    fn task2_pass_saved_routine_revalidates_green() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = DefinitionStore::open(dir.path().to_path_buf());
        store
            .save(&clean_routine("rescued-routine"))
            .expect("seed store");
        let run = empty_run(Some("rescued-routine"), None);
        let ctx = StaticContext::new();

        let result = score_task2_editverb(&run, &ctx, &store);
        assert!(result.pass, "{}", result.reason);
    }

    #[test]
    fn task2_fail_saved_routine_has_error_findings() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = DefinitionStore::open(dir.path().to_path_buf());
        store
            .save(&routine_with_unknown_action("broken-rescue"))
            .expect("seed store");
        let run = empty_run(Some("broken-rescue"), None);
        let ctx = StaticContext::new(); // "radio.mystery" never registered

        let result = score_task2_editverb(&run, &ctx, &store);
        assert!(!result.pass);
        assert!(result.reason.contains("UNKNOWN_ACTION"));
    }

    #[test]
    fn task2_fail_nothing_was_saved() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = DefinitionStore::open(dir.path().to_path_buf());
        let run = empty_run(None, Some("some unrelated stop"));
        let ctx = StaticContext::new();

        let result = score_task2_editverb(&run, &ctx, &store);
        assert!(!result.pass);
    }

    // --- Task 3: same-rig contention ------------------------------------

    #[test]
    fn task3_pass_saved_routine_has_no_same_rig_contention() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = DefinitionStore::open(dir.path().to_path_buf());
        store
            .save(&clean_routine("single-lane-routine"))
            .expect("seed store");
        let run = empty_run(Some("single-lane-routine"), None);
        let ctx = StaticContext::new();

        let result = score_task3_contention(&run, &ctx, &store);
        assert!(result.pass, "{}", result.reason);
    }

    #[test]
    fn task3_fail_saved_routine_triggers_same_rig_parallel_lanes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = DefinitionStore::open(dir.path().to_path_buf());
        store
            .save(&routine_with_same_rig_contention("contending-routine"))
            .expect("seed store");
        let run = empty_run(Some("contending-routine"), None);
        let ctx = StaticContext::new().with_action(RADIO_CONNECT);

        let result = score_task3_contention(&run, &ctx, &store);
        assert!(!result.pass);
        assert!(result.reason.contains("SAME_RIG_PARALLEL_LANES"));
    }

    // --- Held-out (blind) task: generic honest-end floor -----------------

    #[test]
    fn heldout_pass_saved_routine_revalidates_green() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = DefinitionStore::open(dir.path().to_path_buf());
        store
            .save(&clean_routine("heldout-built-routine"))
            .expect("seed store");
        let run = empty_run(Some("heldout-built-routine"), None);
        let ctx = StaticContext::new();

        let result = score_heldout(&run, &ctx, &store);
        assert!(result.pass, "{}", result.reason);
    }

    #[test]
    fn heldout_pass_honest_gap_report_with_nothing_saved() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = DefinitionStore::open(dir.path().to_path_buf());
        let run = empty_run(
            None,
            Some("capability gap: the intent needs primitives the catalog does not offer: x"),
        );
        let ctx = StaticContext::new();

        let result = score_heldout(&run, &ctx, &store);
        assert!(result.pass, "{}", result.reason);
    }

    #[test]
    fn heldout_fail_crash_shaped_end() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = DefinitionStore::open(dir.path().to_path_buf());
        let run = empty_run(None, None);
        let ctx = StaticContext::new();

        let result = score_heldout(&run, &ctx, &store);
        assert!(!result.pass);
        assert!(result.reason.contains("crash-shaped"));
    }
}
