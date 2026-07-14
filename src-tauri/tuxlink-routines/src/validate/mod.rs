//! Continuous static validation (spec §10): one validator, no privileged
//! path. The same `validate()` runs for builder edits, imports, agent
//! submissions, and enable-time (`validate_fleet` layers the fleet-wide
//! checks on top, same ordering contract). Errors block enable/run, never
//! save; warnings are informational.
//!
//! Task 1 landed the skeleton (zero checks wired). Task 2 wired `refs`
//! (`UNRESOLVED_REF`, `UNKNOWN_ACTION`) and `capability`
//! (`NEEDS_INTERNET_OFFGRID`, `NO_RIG_CONFIGURED`, `SAME_RIG_PARALLEL_LANES`).
//! Task 3 wires `contracts` (`UNSATISFIABLE_VAR`, `BRANCH_ON_UNKNOWN`,
//! `CROSS_TRACK_VAR`) and `structure` (`UNREACHABLE_STEP`,
//! `NO_TERMINAL_PATH`, `RETRY_ZERO_ATTEMPTS`, `RETRY_TARGET_MISSING`,
//! `RETRY_TARGET_NOT_ACTION`, `BRANCH_CYCLE`, `CALL_RECURSION`,
//! `CALL_TARGET_MISSING`). Task 4 wires `consent` (`AUTO_TX_UNACKED`,
//! `MIXED_MODE_STALL`, `ATTENDED_UNDER_SCHEDULE`). Later tasks add the
//! remaining per-module check fn. Task 5 wires `fleet`
//! (`SCHEDULE_COLLISION`, `SAME_EFFECT_OVERLAP`) into `validate_fleet` only
//! — it is a cross-routine check over the whole set being enabled, so it
//! has no place in per-routine `validate()`. Plan-4 task 1 (the 2026-07-14
//! one-cadence spec amendment) wires `triggers` (`MULTIPLE_SCHEDULES`) and
//! adds `capability`'s `STEP_TIMEOUT_LIKELY_INSUFFICIENT` (WWV timeout
//! heuristic).

pub mod capability;
pub mod consent;
pub mod context;
pub mod contracts;
pub mod findings;
pub mod fleet;
pub mod refs;
pub mod structure;
pub mod triggers;

pub use context::{StaticContext, StationProfile, ValidationContext};
pub use findings::{Finding, Severity};

use crate::types::{RoutineDef, StepId};

/// Validate a single routine definition against the port. Dispatches to
/// per-module check fns (added task by task; task 2 wires `refs` +
/// `capability`) and returns every `Finding` sorted deterministically by
/// `(code, step)` so UI/MCP output and the fixture-corpus assertions
/// (task 6) are stable across runs and independent of check-fn execution
/// order.
pub fn validate(def: &RoutineDef, ctx: &dyn ValidationContext) -> Vec<Finding> {
    let mut findings: Vec<Finding> = Vec::new();

    refs::check(def, ctx, &mut findings);
    capability::check(def, ctx, &mut findings);
    contracts::check(def, &mut findings);
    structure::check(def, ctx, &mut findings);
    consent::check(def, ctx, &mut findings);
    triggers::check(def, &mut findings);

    sort_findings(&mut findings);
    findings
}

/// Enable-time fleet check: `validate()` on every def, plus cross-routine
/// checks (`fleet` module, task 5) over the set being enabled. Same
/// ordering contract as `validate()`.
///
/// `now_unix` anchors the fleet check's 7-day schedule-collision horizon
/// (`fleet::HORIZON_SECONDS`) — a caller-supplied timestamp, not a hidden
/// clock read, so the check is deterministic and testable at any instant
/// (mirrors `scheduler::next_fire`'s own `now_unix` parameter).
///
/// `utc_offset_seconds` (`local - utc`, `chrono`'s `FixedOffset::local_minus_utc`
/// convention) is the same seam: a `Trigger::Schedule`'s `window` gates in the
/// operator's LOCAL clock (`scheduler::within_window`), so the fleet check's
/// fire-sequence walk must be told the same offset `next_fire` itself expects
/// — this crate is Tauri- and chrono-free, so the caller (the app layer)
/// resolves the actual offset and passes it in as a plain `i32`.
pub fn validate_fleet(
    defs: &[RoutineDef],
    ctx: &dyn ValidationContext,
    now_unix: i64,
    utc_offset_seconds: i32,
) -> Vec<Finding> {
    let mut findings: Vec<Finding> = defs.iter().flat_map(|def| validate(def, ctx)).collect();

    fleet::check(defs, ctx, now_unix, utc_offset_seconds, &mut findings);

    sort_findings(&mut findings);
    findings
}

/// Deterministic ordering contract: sort by `code` (SCREAMING_SNAKE,
/// lexical), then by `step` (no-step findings before stepped ones, then
/// lexically by step id). Lives here, not in `findings.rs`, so no single
/// check module can special-case its own ordering.
fn sort_findings(findings: &mut [Finding]) {
    findings.sort_by(|a, b| {
        a.code
            .cmp(b.code)
            .then_with(|| step_sort_key(&a.step).cmp(&step_sort_key(&b.step)))
    });
}

fn step_sort_key(step: &Option<StepId>) -> Option<&str> {
    step.as_ref().map(|s| s.0.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{OnInterrupted, RoutineDef, Track, TransmitMode, Trigger};

    fn trivially_valid_routine() -> RoutineDef {
        RoutineDef {
            routine: "trivial".into(),
            schema_version: crate::types::SUPPORTED_SCHEMA_VERSION,
            transmit_mode: TransmitMode::Attended,
            transmit_ack: None,
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
    fn skeleton_returns_empty_on_a_trivially_valid_routine() {
        let def = trivially_valid_routine();
        let ctx = StaticContext::new();
        assert_eq!(validate(&def, &ctx), Vec::new());
    }

    #[test]
    fn fleet_skeleton_returns_empty_for_trivially_valid_routines() {
        let def = trivially_valid_routine();
        let ctx = StaticContext::new();
        assert_eq!(validate_fleet(&[def], &ctx, 0, 0), Vec::new());
    }

    #[test]
    fn fleet_skeleton_handles_the_empty_fleet() {
        let ctx = StaticContext::new();
        assert_eq!(validate_fleet(&[], &ctx, 0, 0), Vec::new());
    }

    #[test]
    fn ordering_is_deterministic_by_code_then_step() {
        let mut findings = vec![
            Finding::error("B_CODE", "r1", "m").with_step(StepId("s2".into())),
            Finding::warning("A_CODE", "r1", "m"), // no step: sorts before any A_CODE with a step
            Finding::error("A_CODE", "r1", "m").with_step(StepId("s9".into())),
            Finding::error("A_CODE", "r1", "m").with_step(StepId("s1".into())),
        ];
        sort_findings(&mut findings);
        let ordered: Vec<(&str, Option<String>)> = findings
            .iter()
            .map(|f| (f.code, f.step.as_ref().map(|s| s.0.clone())))
            .collect();
        assert_eq!(
            ordered,
            vec![
                ("A_CODE", None),
                ("A_CODE", Some("s1".into())),
                ("A_CODE", Some("s9".into())),
                ("B_CODE", Some("s2".into())),
            ]
        );
    }

    #[test]
    fn validate_dispatches_refs_and_capability_checks_and_sorts_the_result() {
        use crate::action::ActionDescriptor;
        use crate::types::{ActionStep, BusyPolicy, Control, ControlStep, Step, StepId};

        const RADIO_CONNECT: ActionDescriptor = ActionDescriptor {
            name: "radio.connect",
            needs_radio: true,
            transmits: true,
            needs_internet: false,
        };

        // s1: known action, unresolved @ref (UNRESOLVED_REF) + no rig (NO_RIG_CONFIGURED).
        // s2: unknown action (UNKNOWN_ACTION), which must not also fire a capability finding.
        // e1: explicit End, so this fixture doesn't also pick up task-3's
        // NO_TERMINAL_PATH warning — that check has its own dedicated tests
        // in structure.rs; this test stays scoped to refs+capability.
        let def = RoutineDef {
            routine: "r1".into(),
            schema_version: crate::types::SUPPORTED_SCHEMA_VERSION,
            transmit_mode: TransmitMode::Attended,
            transmit_ack: None,
            on_interrupted: OnInterrupted::Stay,
            inputs: vec![],
            triggers: vec![Trigger::Manual],
            tracks: vec![Track {
                name: "t1".into(),
                steps: vec![
                    Step::Action(ActionStep {
                        id: StepId("s1".into()),
                        action: "radio.connect".into(),
                        params: serde_json::json!({ "stations": "@station-set:or-gateways" }),
                        timeout_s: None,
                        on_radio_busy: BusyPolicy::Wait,
                    }),
                    Step::Action(ActionStep {
                        id: StepId("s2".into()),
                        action: "radio.mystery".into(),
                        params: serde_json::json!({}),
                        timeout_s: None,
                        on_radio_busy: BusyPolicy::Wait,
                    }),
                    Step::Control(ControlStep {
                        id: StepId("e1".into()),
                        control: Control::End {
                            failed: false,
                            reason: None,
                        },
                    }),
                ],
            }],
        };
        let ctx = StaticContext::new().with_action(RADIO_CONNECT); // "radio.mystery" not seeded; entity not seeded

        let findings = validate(&def, &ctx);
        let codes: Vec<&str> = findings.iter().map(|f| f.code).collect();
        // Sorted lexically by code: NO_RIG_CONFIGURED, UNKNOWN_ACTION, UNRESOLVED_REF.
        assert_eq!(
            codes,
            vec!["NO_RIG_CONFIGURED", "UNKNOWN_ACTION", "UNRESOLVED_REF"]
        );
        assert_eq!(
            findings
                .iter()
                .find(|f| f.code == "UNKNOWN_ACTION")
                .unwrap()
                .step,
            Some(StepId("s2".into()))
        );
    }

    #[test]
    fn ordering_is_stable_regardless_of_input_order() {
        let a = Finding::error("X_CODE", "r1", "m").with_step(StepId("s1".into()));
        let b = Finding::error("X_CODE", "r1", "m").with_step(StepId("s2".into()));
        let mut forward = vec![a.clone(), b.clone()];
        let mut backward = vec![b, a];
        sort_findings(&mut forward);
        sort_findings(&mut backward);
        assert_eq!(forward, backward);
    }
}
