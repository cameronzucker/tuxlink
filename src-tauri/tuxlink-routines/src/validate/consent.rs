//! Consent-closure checks (spec §10 layer 1's consent class, plan-3 task 4):
//! does an automatic (unattended) routine's transmit closure carry a
//! recorded operator acknowledgment, and does an unattended call path reach
//! into attended territory where nobody is present to click consent?
//!
//! **`TX_MODE_UNDECLARED` is not implemented here.** The plan's task-4
//! clarification is binding: `transmit_mode` is a required field on
//! `RoutineDef` (`types.rs`), so "a routine whose closure transmits with no
//! declared mode" is structurally impossible in the v1 schema — every
//! routine has a mode. The three codes below are the real checks that fill
//! the gap that clarification identifies.
//!
//! **Transmit closure**, shared by all three checks below: any step in a
//! routine's own tracks OR in any routine transitively reachable via
//! `Control::Call` (through `ctx.routine_def`, cycle-guarded with a
//! visited-set) whose action descriptor has `transmits: true`. This reuses
//! the exact walk shape `structure.rs`'s `CALL_RECURSION`/`closure_reaches`
//! check established: the initial routine is scanned directly (no `ctx`
//! lookup needed for it — it may be an unsaved draft not yet registered
//! under its own name), and every `Control::Call` target is resolved via
//! `ctx.routine_def` and recursed into, with a `HashSet<String>` of routine
//! names guarding against infinite recursion on a cycle. An unknown action
//! (no descriptor: `ctx.action_descriptor` returns `None`) never counts as
//! transmitting — `UNKNOWN_ACTION` (`refs.rs`) already reports it
//! separately. A missing callee (`ctx.routine_def` returns `None`) is
//! skipped silently here — that's `CALL_TARGET_MISSING`'s (`structure.rs`)
//! problem, not this module's.
//!
//! - [`AUTO_TX_UNACKED`] (Error): `transmit_mode: automatic` + a non-empty
//!   transmit closure + `transmit_ack` missing, or present with an empty
//!   `by` or `at` field. The caller's own mode governs its own run — a
//!   routine that runs unattended and *might* transmit (directly or via a
//!   step it calls into) must carry a recorded operator acknowledgment.
//! - [`MIXED_MODE_STALL`] (Warning): an automatic routine's call closure
//!   reaches an attended-mode routine whose OWN transmit closure (that
//!   routine's own steps plus whatever IT in turn calls) is non-empty. An
//!   unattended scheduled run of the automatic routine will eventually walk
//!   into that attended routine's transmitting step and stall waiting on a
//!   consent click nobody is present to give.
//! - [`ATTENDED_UNDER_SCHEDULE`] (Warning): the direct form of the same
//!   stall class — an attended-mode routine with a non-empty transmit
//!   closure that also carries a `Trigger::Schedule`. A manual-trigger
//!   attended routine with the same transmit closure is fine: an operator
//!   is the one clicking "run" and stays present.

use std::collections::HashSet;

use crate::types::{Control, RoutineDef, Step, StepId, TransmitMode, Trigger};

use super::context::ValidationContext;
use super::findings::Finding;

pub const AUTO_TX_UNACKED: &str = "AUTO_TX_UNACKED";
pub const MIXED_MODE_STALL: &str = "MIXED_MODE_STALL";
pub const ATTENDED_UNDER_SCHEDULE: &str = "ATTENDED_UNDER_SCHEDULE";

/// Append every consent-closure finding for `def` into `findings`.
pub fn check(def: &RoutineDef, ctx: &dyn ValidationContext, findings: &mut Vec<Finding>) {
    check_auto_tx_unacked(def, ctx, findings);
    check_mixed_mode_stall(def, ctx, findings);
    check_attended_under_schedule(def, ctx, findings);
}

/// A single transmitting step found by [`scan_routine_for_transmit`]:
/// `routine` names whichever routine in the closure actually owns the step
/// (may differ from the routine the scan started at, if the transmit lives
/// behind one or more `Call` hops).
struct TransmitHit {
    routine: String,
    track: String,
    step: StepId,
    action: String,
}

/// Depth-first search for the first transmitting step in `rd`'s transmit
/// closure (see module doc): `rd`'s own steps first, then each `Call`
/// target's closure, recursively. `visited` guards against looping forever
/// on a call cycle — a routine name already in `visited` is not re-entered.
fn scan_routine_for_transmit(
    rd: &RoutineDef,
    ctx: &dyn ValidationContext,
    visited: &mut HashSet<String>,
) -> Option<TransmitHit> {
    if !visited.insert(rd.routine.clone()) {
        return None;
    }

    for track in &rd.tracks {
        for step in &track.steps {
            match step {
                Step::Action(a) => {
                    if let Some(descriptor) = ctx.action_descriptor(&a.action) {
                        if descriptor.transmits {
                            return Some(TransmitHit {
                                routine: rd.routine.clone(),
                                track: track.name.clone(),
                                step: a.id.clone(),
                                action: a.action.clone(),
                            });
                        }
                    }
                    // No descriptor: UNKNOWN_ACTION's problem, not ours.
                }
                Step::Control(c) => {
                    let Control::Call {
                        routine: callee, ..
                    } = &c.control
                    else {
                        continue;
                    };
                    // Missing callee: CALL_TARGET_MISSING's problem, skip silently.
                    if let Some(callee_def) = ctx.routine_def(callee) {
                        if let Some(hit) = scan_routine_for_transmit(&callee_def, ctx, visited) {
                            return Some(hit);
                        }
                    }
                }
            }
        }
    }

    None
}

fn transmit_ack_is_valid(def: &RoutineDef) -> bool {
    match &def.transmit_ack {
        Some(ack) => !ack.by.trim().is_empty() && !ack.at.trim().is_empty(),
        None => false,
    }
}

fn describe_hit_location(def: &RoutineDef, hit: &TransmitHit) -> String {
    if hit.routine == def.routine {
        String::new()
    } else {
        format!(" (in routine \"{}\")", hit.routine)
    }
}

fn check_auto_tx_unacked(
    def: &RoutineDef,
    ctx: &dyn ValidationContext,
    findings: &mut Vec<Finding>,
) {
    if def.transmit_mode != TransmitMode::Automatic {
        return;
    }

    let mut visited = HashSet::new();
    let Some(hit) = scan_routine_for_transmit(def, ctx, &mut visited) else {
        return;
    };

    if transmit_ack_is_valid(def) {
        return;
    }

    findings.push(
        Finding::error(
            AUTO_TX_UNACKED,
            def.routine.clone(),
            format!(
                "routine \"{}\" runs automatically and its transmit closure includes step \"{}\" \
                 (action \"{}\"){}, but transmit_ack is missing or incomplete — automatic \
                 transmission requires a recorded operator acknowledgment",
                def.routine,
                hit.step.0,
                hit.action,
                describe_hit_location(def, &hit),
            ),
        )
        .with_track(hit.track.clone())
        .with_step(hit.step),
    );
}

/// An attended-mode routine, found somewhere in an automatic routine's call
/// closure, whose OWN transmit closure is non-empty.
struct MixedModeProblem {
    attended_routine: String,
    transmit_routine: String, // The routine that actually owns the transmitting step
    step: StepId,
    action: String,
}

/// Depth-first search over `rd`'s call closure (`rd` itself, then every
/// routine transitively reachable via `Control::Call`) for the first node
/// that is attended-mode AND has a non-empty transmit closure of its own.
/// `visited` guards against looping forever on a call cycle, mirroring
/// [`scan_routine_for_transmit`]'s pattern.
fn find_attended_transmitting_in_closure(
    rd: &RoutineDef,
    ctx: &dyn ValidationContext,
    visited: &mut HashSet<String>,
) -> Option<MixedModeProblem> {
    if !visited.insert(rd.routine.clone()) {
        return None;
    }

    if rd.transmit_mode == TransmitMode::Attended {
        let mut own_visited = HashSet::new();
        if let Some(hit) = scan_routine_for_transmit(rd, ctx, &mut own_visited) {
            return Some(MixedModeProblem {
                attended_routine: rd.routine.clone(),
                transmit_routine: hit.routine,
                step: hit.step,
                action: hit.action,
            });
        }
    }

    for track in &rd.tracks {
        for step in &track.steps {
            let Step::Control(c) = step else { continue };
            let Control::Call {
                routine: callee, ..
            } = &c.control
            else {
                continue;
            };
            if let Some(callee_def) = ctx.routine_def(callee) {
                if let Some(problem) =
                    find_attended_transmitting_in_closure(&callee_def, ctx, visited)
                {
                    return Some(problem);
                }
            }
            // Missing callee: CALL_TARGET_MISSING's problem, skip silently.
        }
    }

    None
}

fn check_mixed_mode_stall(
    def: &RoutineDef,
    ctx: &dyn ValidationContext,
    findings: &mut Vec<Finding>,
) {
    if def.transmit_mode != TransmitMode::Automatic {
        return;
    }

    for track in &def.tracks {
        for step in &track.steps {
            let Step::Control(c) = step else { continue };
            let Control::Call {
                routine: callee, ..
            } = &c.control
            else {
                continue;
            };
            let Some(callee_def) = ctx.routine_def(callee) else {
                continue; // CALL_TARGET_MISSING's problem, not ours.
            };

            let mut visited = HashSet::new();
            if let Some(problem) =
                find_attended_transmitting_in_closure(&callee_def, ctx, &mut visited)
            {
                let location_clause = if problem.attended_routine == problem.transmit_routine {
                    format!("in routine \"{}\"", problem.attended_routine)
                } else {
                    format!(
                        "in routine \"{}\" (reached from attended callee \"{}\")",
                        problem.transmit_routine, problem.attended_routine
                    )
                };
                findings.push(
                    Finding::warning(
                        MIXED_MODE_STALL,
                        def.routine.clone(),
                        format!(
                            "routine \"{}\" runs automatically and its call step \"{}\" reaches \
                             attended routine \"{}\", whose transmit closure includes step \
                             \"{}\" (action \"{}\") {} — an unattended scheduled run \
                             will pause at that step for a consent click nobody is present to give",
                            def.routine,
                            c.id.0,
                            problem.attended_routine,
                            problem.step.0,
                            problem.action,
                            location_clause,
                        ),
                    )
                    .with_track(track.name.clone())
                    .with_step(c.id.clone()),
                );
            }
        }
    }
}

fn check_attended_under_schedule(
    def: &RoutineDef,
    ctx: &dyn ValidationContext,
    findings: &mut Vec<Finding>,
) {
    if def.transmit_mode != TransmitMode::Attended {
        return;
    }

    let has_schedule_trigger = def
        .triggers
        .iter()
        .any(|t| matches!(t, Trigger::Schedule { .. }));
    if !has_schedule_trigger {
        return;
    }

    let mut visited = HashSet::new();
    let Some(hit) = scan_routine_for_transmit(def, ctx, &mut visited) else {
        return;
    };

    findings.push(
        Finding::warning(
            ATTENDED_UNDER_SCHEDULE,
            def.routine.clone(),
            format!(
                "routine \"{}\" is attended-mode but has a schedule trigger, and its transmit \
                 closure includes step \"{}\" (action \"{}\"){} — a scheduled unattended run will \
                 pause at that step for a consent click nobody is present to give",
                def.routine,
                hit.step.0,
                hit.action,
                describe_hit_location(def, &hit),
            ),
        )
        .with_track(hit.track.clone())
        .with_step(hit.step),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::ActionDescriptor;
    use crate::types::{
        ActionStep, BusyPolicy, ControlStep, IfMissed, OnInterrupted, RoutineDef, Track,
        TransmitAck,
    };
    use crate::validate::context::StaticContext;
    use crate::validate::findings::Severity;
    use serde_json::json;

    const RADIO_TX: ActionDescriptor = ActionDescriptor {
        name: "radio.tx",
        needs_radio: true,
        transmits: true,
        needs_internet: false,
    };
    const LOCAL_NOTE: ActionDescriptor = ActionDescriptor {
        name: "local.note",
        needs_radio: false,
        transmits: false,
        needs_internet: false,
    };

    fn tx_action(id: &str) -> Step {
        Step::Action(ActionStep {
            id: StepId(id.into()),
            action: "radio.tx".into(),
            params: json!({}),
            timeout_s: None,
            on_radio_busy: BusyPolicy::Wait,
        })
    }

    fn local_action(id: &str) -> Step {
        Step::Action(ActionStep {
            id: StepId(id.into()),
            action: "local.note".into(),
            params: json!({}),
            timeout_s: None,
            on_radio_busy: BusyPolicy::Wait,
        })
    }

    fn call_step(id: &str, routine: &str) -> Step {
        Step::Control(ControlStep {
            id: StepId(id.into()),
            control: Control::Call {
                routine: routine.into(),
                args: json!({}),
                sync: true,
            },
        })
    }

    fn track(name: &str, steps: Vec<Step>) -> Track {
        Track {
            name: name.into(),
            steps,
        }
    }

    fn base_ctx() -> StaticContext {
        StaticContext::new()
            .with_action(RADIO_TX)
            .with_action(LOCAL_NOTE)
    }

    fn routine_named(
        name: &str,
        mode: TransmitMode,
        triggers: Vec<Trigger>,
        tracks: Vec<Track>,
    ) -> RoutineDef {
        RoutineDef {
            routine: name.into(),
            schema_version: crate::types::SUPPORTED_SCHEMA_VERSION,
            transmit_mode: mode,
            transmit_ack: None,
            on_interrupted: OnInterrupted::Stay,
            inputs: vec![],
            triggers,
            tracks,
        }
    }

    fn schedule_trigger() -> Trigger {
        Trigger::Schedule {
            every: "30m".into(),
            align: None,
            window: None,
            if_missed: IfMissed::Skip,
        }
    }

    // --- AUTO_TX_UNACKED: the consent matrix -----------------------------

    #[test]
    fn automatic_with_no_transmit_closure_is_not_flagged() {
        let def = routine_named(
            "r1",
            TransmitMode::Automatic,
            vec![Trigger::Manual],
            vec![track("t1", vec![local_action("s1")])],
        );
        let ctx = base_ctx();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn automatic_with_transmit_closure_and_no_ack_is_flagged() {
        let def = routine_named(
            "r1",
            TransmitMode::Automatic,
            vec![Trigger::Manual],
            vec![track("t1", vec![tx_action("s1")])],
        );
        let ctx = base_ctx();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        let hits: Vec<_> = findings
            .iter()
            .filter(|f| f.code == AUTO_TX_UNACKED)
            .collect();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].severity, Severity::Error);
        assert_eq!(hits[0].step, Some(StepId("s1".into())));
        assert!(hits[0].message.contains("radio.tx"), "{:?}", hits[0]);
    }

    #[test]
    fn automatic_with_transmit_closure_and_valid_ack_is_not_flagged() {
        let mut def = routine_named(
            "r1",
            TransmitMode::Automatic,
            vec![Trigger::Manual],
            vec![track("t1", vec![tx_action("s1")])],
        );
        def.transmit_ack = Some(TransmitAck {
            by: "KK7ABC".into(),
            at: "2026-07-13T20:00:00Z".into(),
        });
        let ctx = base_ctx();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().all(|f| f.code != AUTO_TX_UNACKED));
    }

    #[test]
    fn automatic_with_transmit_closure_and_empty_ack_fields_is_flagged() {
        let mut def = routine_named(
            "r1",
            TransmitMode::Automatic,
            vec![Trigger::Manual],
            vec![track("t1", vec![tx_action("s1")])],
        );
        def.transmit_ack = Some(TransmitAck {
            by: String::new(),
            at: String::new(),
        });
        let ctx = base_ctx();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().any(|f| f.code == AUTO_TX_UNACKED));
    }

    #[test]
    fn automatic_with_transmit_closure_and_whitespace_only_by_is_flagged() {
        let mut def = routine_named(
            "r1",
            TransmitMode::Automatic,
            vec![Trigger::Manual],
            vec![track("t1", vec![tx_action("s1")])],
        );
        def.transmit_ack = Some(TransmitAck {
            by: "   ".into(), // spaces only
            at: "2026-07-13T20:00:00Z".into(),
        });
        let ctx = base_ctx();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        let hits: Vec<_> = findings
            .iter()
            .filter(|f| f.code == AUTO_TX_UNACKED)
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "whitespace-only by should trigger AUTO_TX_UNACKED"
        );
        assert_eq!(hits[0].severity, Severity::Error);
    }

    #[test]
    fn automatic_with_transmit_closure_and_whitespace_only_at_is_flagged() {
        let mut def = routine_named(
            "r1",
            TransmitMode::Automatic,
            vec![Trigger::Manual],
            vec![track("t1", vec![tx_action("s1")])],
        );
        def.transmit_ack = Some(TransmitAck {
            by: "KK7ABC".into(),
            at: "\n".into(), // newline only
        });
        let ctx = base_ctx();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        let hits: Vec<_> = findings
            .iter()
            .filter(|f| f.code == AUTO_TX_UNACKED)
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "whitespace-only at should trigger AUTO_TX_UNACKED"
        );
        assert_eq!(hits[0].severity, Severity::Error);
    }

    #[test]
    fn attended_with_transmit_closure_is_never_auto_tx_unacked() {
        let def = routine_named(
            "r1",
            TransmitMode::Attended,
            vec![Trigger::Manual],
            vec![track("t1", vec![tx_action("s1")])],
        );
        let ctx = base_ctx();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().all(|f| f.code != AUTO_TX_UNACKED));
    }

    // --- MIXED_MODE_STALL --------------------------------------------------

    #[test]
    fn automatic_calling_attended_transmitting_callee_is_mixed_mode_stall() {
        let callee = routine_named(
            "b",
            TransmitMode::Attended,
            vec![Trigger::Manual],
            vec![track("t1", vec![tx_action("tx1")])],
        );
        let def = routine_named(
            "r1",
            TransmitMode::Automatic,
            vec![Trigger::Manual],
            vec![track("t1", vec![call_step("c1", "b")])],
        );
        let ctx = base_ctx().with_routine(callee);
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        let hits: Vec<_> = findings
            .iter()
            .filter(|f| f.code == MIXED_MODE_STALL)
            .collect();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].severity, Severity::Warning);
        assert_eq!(hits[0].step, Some(StepId("c1".into())));
        assert!(hits[0].message.contains("\"b\""), "{:?}", hits[0]);
        assert!(hits[0].message.contains("tx1"), "{:?}", hits[0]);
    }

    #[test]
    fn automatic_calling_attended_non_transmitting_callee_is_not_flagged() {
        let callee = routine_named(
            "b",
            TransmitMode::Attended,
            vec![Trigger::Manual],
            vec![track("t1", vec![local_action("s1")])],
        );
        let def = routine_named(
            "r1",
            TransmitMode::Automatic,
            vec![Trigger::Manual],
            vec![track("t1", vec![call_step("c1", "b")])],
        );
        let ctx = base_ctx().with_routine(callee);
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().all(|f| f.code != MIXED_MODE_STALL));
    }

    #[test]
    fn automatic_calling_automatic_transmitting_callee_is_not_mixed_mode_stall() {
        let callee = routine_named(
            "b",
            TransmitMode::Automatic,
            vec![Trigger::Manual],
            vec![track("t1", vec![tx_action("tx1")])],
        );
        let def = routine_named(
            "r1",
            TransmitMode::Automatic,
            vec![Trigger::Manual],
            vec![track("t1", vec![call_step("c1", "b")])],
        );
        let ctx = base_ctx().with_routine(callee);
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().all(|f| f.code != MIXED_MODE_STALL));
    }

    #[test]
    fn automatic_calling_attended_with_nested_transmit_names_both_callees() {
        // r1 (automatic) -> b (attended) -> c (attended, transmits)
        // Message should name both b (attended callee reached directly) and c (owner of transmit step)
        let c = routine_named(
            "c",
            TransmitMode::Attended,
            vec![Trigger::Manual],
            vec![track("t1", vec![tx_action("txc")])],
        );
        let b = routine_named(
            "b",
            TransmitMode::Attended,
            vec![Trigger::Manual],
            vec![track("t1", vec![call_step("c2", "c")])],
        );
        let def = routine_named(
            "r1",
            TransmitMode::Automatic,
            vec![Trigger::Manual],
            vec![track("t1", vec![call_step("c1", "b")])],
        );
        let ctx = base_ctx().with_routine(b).with_routine(c);
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        let hits: Vec<_> = findings
            .iter()
            .filter(|f| f.code == MIXED_MODE_STALL)
            .collect();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].severity, Severity::Warning);
        assert_eq!(hits[0].step, Some(StepId("c1".into())));
        // Message should mention both b (attended callee reached directly) and c (owner of transmit)
        assert!(
            hits[0].message.contains("\"b\""),
            "message should name attended callee b: {}",
            hits[0].message
        );
        assert!(
            hits[0].message.contains("\"c\""),
            "message should name transmit owner c: {}",
            hits[0].message
        );
        assert!(
            hits[0].message.contains("txc"),
            "message should name the step action: {}",
            hits[0].message
        );
    }

    // --- ATTENDED_UNDER_SCHEDULE --------------------------------------------

    #[test]
    fn attended_with_schedule_trigger_and_transmit_closure_is_flagged() {
        let def = routine_named(
            "r1",
            TransmitMode::Attended,
            vec![schedule_trigger()],
            vec![track("t1", vec![tx_action("s1")])],
        );
        let ctx = base_ctx();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        let hits: Vec<_> = findings
            .iter()
            .filter(|f| f.code == ATTENDED_UNDER_SCHEDULE)
            .collect();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].severity, Severity::Warning);
        assert_eq!(hits[0].step, Some(StepId("s1".into())));
    }

    #[test]
    fn attended_with_manual_trigger_and_transmit_closure_is_fine() {
        let def = routine_named(
            "r1",
            TransmitMode::Attended,
            vec![Trigger::Manual],
            vec![track("t1", vec![tx_action("s1")])],
        );
        let ctx = base_ctx();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().all(|f| f.code != ATTENDED_UNDER_SCHEDULE));
    }

    #[test]
    fn attended_with_schedule_trigger_and_no_transmit_closure_is_not_flagged() {
        let def = routine_named(
            "r1",
            TransmitMode::Attended,
            vec![schedule_trigger()],
            vec![track("t1", vec![local_action("s1")])],
        );
        let ctx = base_ctx();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().all(|f| f.code != ATTENDED_UNDER_SCHEDULE));
    }

    // --- Transitive closure + cycle safety ---------------------------------

    #[test]
    fn transmit_closure_reaches_through_two_call_hops() {
        // r1 (automatic) -> b -> c, where only c actually transmits.
        let c = routine_named(
            "c",
            TransmitMode::Attended,
            vec![Trigger::Manual],
            vec![track("t1", vec![tx_action("txc")])],
        );
        let b = routine_named(
            "b",
            TransmitMode::Attended,
            vec![Trigger::Manual],
            vec![track("t1", vec![call_step("c2", "c")])],
        );
        let def = routine_named(
            "r1",
            TransmitMode::Automatic,
            vec![Trigger::Manual],
            vec![track("t1", vec![call_step("c1", "b")])],
        );
        let ctx = base_ctx().with_routine(b).with_routine(c);
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(
            findings.iter().any(|f| f.code == AUTO_TX_UNACKED),
            "{findings:?}"
        );
    }

    #[test]
    fn a_call_cycle_that_never_transmits_is_cycle_safe_and_produces_no_findings() {
        // r1 (automatic) -> b (attended) -> r1 (cycle back to root); neither
        // step transmits. Registering "r1" under its own name in ctx makes
        // the cycle real (not just an absent lookup) so the visited-set
        // guard is actually exercised, mirroring structure.rs's sibling
        // cycle test — this must terminate, not hang, and must not
        // false-flag anything.
        let b = routine_named(
            "b",
            TransmitMode::Attended,
            vec![Trigger::Manual],
            vec![track("t1", vec![call_step("c2", "r1")])],
        );
        let def = routine_named(
            "r1",
            TransmitMode::Automatic,
            vec![Trigger::Manual],
            vec![track("t1", vec![call_step("c1", "b")])],
        );
        let ctx = base_ctx().with_routine(b).with_routine(def.clone());
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn a_sibling_call_cycle_that_never_touches_the_root_is_cycle_safe() {
        // b -> c -> b (cycle entirely among callees); root r1 (automatic)
        // calls b but is never itself part of the loop. Nothing transmits.
        let c = routine_named(
            "c",
            TransmitMode::Attended,
            vec![Trigger::Manual],
            vec![track("t1", vec![call_step("c3", "b")])],
        );
        let b = routine_named(
            "b",
            TransmitMode::Attended,
            vec![Trigger::Manual],
            vec![track("t1", vec![call_step("c2", "c")])],
        );
        let def = routine_named(
            "r1",
            TransmitMode::Automatic,
            vec![Trigger::Manual],
            vec![track("t1", vec![call_step("c1", "b")])],
        );
        let ctx = base_ctx().with_routine(b).with_routine(c);
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.is_empty(), "{findings:?}");
    }
}
