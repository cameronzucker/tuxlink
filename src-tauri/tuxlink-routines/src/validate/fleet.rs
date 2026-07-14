//! Fleet-wide checks (spec §10 layer 2, plan-3 task 5): cross-routine
//! findings only meaningful once you know the WHOLE set being enabled, not
//! any single `RoutineDef` in isolation — the reason `validate_fleet` takes
//! `defs: &[RoutineDef]` where `validate()` takes one.
//!
//! Both checks below reuse the transitive-closure walk shape
//! `consent.rs`/`structure.rs` established (`ctx.routine_def` through
//! `Control::Call`, cycle-guarded with a `HashSet<String>` of routine
//! names): a routine's "radio-contending" and "data.* action" surface is
//! not just its own steps, it's everything reachable through the routines
//! it calls.
//!
//! - [`SCHEDULE_COLLISION`] (Warning): two enabled routines whose call
//!   closures both touch the radio (`ActionDescriptor.needs_radio` OR
//!   `.transmits` — "transmit-OR-lease", since v1 has exactly one default
//!   rig, so ANY radio-touching step from either routine contends for it)
//!   AND whose `Trigger::Schedule` fire sequences — computed with
//!   `scheduler::next_fire`, the same epoch-anchored math the engine's tick
//!   loop uses — literally coincide (share an identical unix instant) at
//!   least once within a **7-day horizon** from the caller-supplied
//!   `now_unix`. The message names both routines and the first (earliest)
//!   coinciding instant.
//! - [`SAME_EFFECT_OVERLAP`] (Warning): two enabled routines whose call
//!   closures share an identical `data.*` action name (name-based, not
//!   descriptor-based — this is about two routines authoring the same
//!   effect, not a capability flag), where BOTH routines have at least one
//!   schedule fire within the same 7-day horizon (their fire times need not
//!   coincide exactly, unlike `SCHEDULE_COLLISION` — this is a "these two
//!   are both active this week and touch the same data" warning, not a
//!   contention warning). One finding per shared action name per pair.
//!
//! Both checks only ever compare routines pairwise within `defs` (the set
//! `validate_fleet` was called with — "the set being enabled"), sorted by
//! routine name first so finding order is deterministic independent of the
//! caller's slice order (the final sort in `mod.rs` only orders by
//! `(code, step)`, and every fleet finding here has no `step`, so the push
//! order established here is what survives that stable sort).

use std::collections::HashSet;

use crate::scheduler::next_fire;
use crate::types::{Control, RoutineDef, Step, Trigger};

use super::context::ValidationContext;
use super::findings::Finding;

pub const SCHEDULE_COLLISION: &str = "SCHEDULE_COLLISION";
pub const SAME_EFFECT_OVERLAP: &str = "SAME_EFFECT_OVERLAP";

/// The fleet-check horizon (spec §10): 7 days from `now_unix`. Documented
/// here as the single source of the number so the collision/overlap
/// messages and the horizon used to compute fire sequences never drift
/// apart from each other.
const HORIZON_SECONDS: i64 = 7 * 86_400;
const HORIZON_DAYS: i64 = HORIZON_SECONDS / 86_400;

/// What a routine's transitive call closure (`consent.rs`-shaped walk)
/// exposes to the fleet checks: whether it touches the radio anywhere, and
/// which `data.*` action names it runs anywhere.
#[derive(Debug, Default)]
struct ClosureInfo {
    needs_radio: bool,
    data_actions: HashSet<String>,
}

fn collect_closure_info(
    rd: &RoutineDef,
    ctx: &dyn ValidationContext,
    visited: &mut HashSet<String>,
    info: &mut ClosureInfo,
) {
    if !visited.insert(rd.routine.clone()) {
        return;
    }

    for track in &rd.tracks {
        for step in &track.steps {
            match step {
                Step::Action(a) => {
                    if a.action.starts_with("data.") {
                        info.data_actions.insert(a.action.clone());
                    }
                    if let Some(descriptor) = ctx.action_descriptor(&a.action) {
                        if descriptor.needs_radio || descriptor.transmits {
                            info.needs_radio = true;
                        }
                    }
                    // No descriptor: UNKNOWN_ACTION (refs.rs) already
                    // reports it; this walk just can't learn anything about
                    // its capability flags, but the literal action NAME
                    // still counts for SAME_EFFECT_OVERLAP.
                }
                Step::Control(c) => {
                    let Control::Call {
                        routine: callee, ..
                    } = &c.control
                    else {
                        continue;
                    };
                    // Missing callee: CALL_TARGET_MISSING's (structure.rs)
                    // problem, skip silently here.
                    if let Some(callee_def) = ctx.routine_def(callee) {
                        collect_closure_info(&callee_def, ctx, visited, info);
                    }
                }
            }
        }
    }
}

fn closure_info(def: &RoutineDef, ctx: &dyn ValidationContext) -> ClosureInfo {
    let mut info = ClosureInfo::default();
    let mut visited = HashSet::new();
    collect_closure_info(def, ctx, &mut visited, &mut info);
    info
}

/// Every unix instant a routine's `Trigger::Schedule`s fire at, strictly
/// after `now_unix` and within the horizon, ascending and deduplicated.
/// Non-schedule triggers (`Trigger::Manual`) never contribute (`next_fire`
/// returns `None` for them, same as `scheduler.rs`'s own contract).
///
/// `utc_offset_seconds` (`local - utc`) is threaded straight through to
/// `next_fire` — a `Trigger::Schedule` with a `window` gates in the
/// operator's LOCAL clock (see `scheduler::within_window`), so the
/// collision/overlap horizon this fn walks must agree with that same clock,
/// not silently re-derive it in UTC.
fn routine_fire_times(def: &RoutineDef, now_unix: i64, utc_offset_seconds: i32) -> Vec<i64> {
    let horizon_end = now_unix + HORIZON_SECONDS;
    let mut times: Vec<i64> = def
        .triggers
        .iter()
        .flat_map(|trigger| {
            fire_times_within_horizon(trigger, now_unix, horizon_end, utc_offset_seconds)
        })
        .collect();
    times.sort_unstable();
    times.dedup();
    times
}

fn fire_times_within_horizon(
    trigger: &Trigger,
    now_unix: i64,
    horizon_end: i64,
    utc_offset_seconds: i32,
) -> Vec<i64> {
    let mut out = Vec::new();
    let mut cursor = now_unix;
    loop {
        match next_fire(trigger, cursor, utc_offset_seconds) {
            Some(t) if t <= horizon_end => {
                out.push(t);
                cursor = t;
            }
            _ => break,
        }
    }
    out
}

/// Append every fleet finding for the set `defs` into `findings`. Called by
/// `validate_fleet` (`mod.rs`) after every individual `validate()` finding
/// has already been collected.
///
/// `utc_offset_seconds` (`local - utc`) anchors the SAME clock a window-gated
/// `Trigger::Schedule` fires in — see `routine_fire_times`.
pub fn check(
    defs: &[RoutineDef],
    ctx: &dyn ValidationContext,
    now_unix: i64,
    utc_offset_seconds: i32,
    findings: &mut Vec<Finding>,
) {
    let mut sorted: Vec<&RoutineDef> = defs.iter().collect();
    sorted.sort_by(|a, b| a.routine.cmp(&b.routine));

    for i in 0..sorted.len() {
        for j in (i + 1)..sorted.len() {
            let a = sorted[i];
            let b = sorted[j];
            check_pair(a, b, ctx, now_unix, utc_offset_seconds, findings);
        }
    }
}

fn check_pair(
    a: &RoutineDef,
    b: &RoutineDef,
    ctx: &dyn ValidationContext,
    now_unix: i64,
    utc_offset_seconds: i32,
    findings: &mut Vec<Finding>,
) {
    let info_a = closure_info(a, ctx);
    let info_b = closure_info(b, ctx);
    let times_a = routine_fire_times(a, now_unix, utc_offset_seconds);
    let times_b = routine_fire_times(b, now_unix, utc_offset_seconds);

    check_schedule_collision(
        a, b, &info_a, &info_b, &times_a, &times_b, now_unix, findings,
    );
    check_same_effect_overlap(a, b, &info_a, &info_b, &times_a, &times_b, findings);
}

#[allow(clippy::too_many_arguments)]
fn check_schedule_collision(
    a: &RoutineDef,
    b: &RoutineDef,
    info_a: &ClosureInfo,
    info_b: &ClosureInfo,
    times_a: &[i64],
    times_b: &[i64],
    now_unix: i64,
    findings: &mut Vec<Finding>,
) {
    if !info_a.needs_radio || !info_b.needs_radio {
        return;
    }
    if times_a.is_empty() || times_b.is_empty() {
        return;
    }

    let set_b: HashSet<i64> = times_b.iter().copied().collect();
    // `times_a` is ascending, so the first hit is the earliest coinciding
    // instant across both sequences (the smallest element of the
    // intersection has to appear as the smallest matching element while
    // scanning `times_a` in order).
    let Some(&collision) = times_a.iter().find(|t| set_b.contains(t)) else {
        return;
    };

    findings.push(Finding::warning(
        SCHEDULE_COLLISION,
        a.routine.clone(),
        format!(
            "routines \"{}\" and \"{}\" both touch the radio (directly or through a call closure) \
             and their schedules coincide within the next {HORIZON_DAYS}d — first collision at unix \
             time {collision} (T+{}s from now)",
            a.routine,
            b.routine,
            collision - now_unix,
        ),
    ));
}

fn check_same_effect_overlap(
    a: &RoutineDef,
    b: &RoutineDef,
    info_a: &ClosureInfo,
    info_b: &ClosureInfo,
    times_a: &[i64],
    times_b: &[i64],
    findings: &mut Vec<Finding>,
) {
    if times_a.is_empty() || times_b.is_empty() {
        return;
    }

    let mut shared: Vec<&String> = info_a
        .data_actions
        .intersection(&info_b.data_actions)
        .collect();
    shared.sort();

    for action in shared {
        findings.push(Finding::warning(
            SAME_EFFECT_OVERLAP,
            a.routine.clone(),
            format!(
                "routines \"{}\" and \"{}\" both run action \"{action}\" (directly or through a call \
                 closure) and are both scheduled within the next {HORIZON_DAYS}d — their effects on \
                 \"{action}\" may overlap",
                a.routine, b.routine,
            ),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::ActionDescriptor;
    use crate::types::{
        ActionStep, BusyPolicy, ControlStep, IfMissed, OnInterrupted, RoutineDef, Step, StepId,
        Track, TransmitMode, Trigger,
    };
    use crate::validate::context::StaticContext;
    use crate::validate::findings::Severity;
    use serde_json::json;

    const NOW: i64 = 1_784_124_420; // same epoch scheduler.rs's own tests anchor to

    const RADIO_CONNECT: ActionDescriptor = ActionDescriptor {
        name: "radio.connect",
        needs_radio: true,
        transmits: true,
        needs_internet: false,
    };
    const WEB_LOOKUP: ActionDescriptor = ActionDescriptor {
        name: "data.web_lookup",
        needs_radio: false,
        transmits: false,
        needs_internet: true,
    };
    const LOCAL_NOTE: ActionDescriptor = ActionDescriptor {
        name: "local.note",
        needs_radio: false,
        transmits: false,
        needs_internet: false,
    };

    fn action_step(id: &str, action: &str) -> Step {
        Step::Action(ActionStep {
            id: StepId(id.into()),
            action: action.into(),
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

    fn schedule(every: &str) -> Trigger {
        Trigger::Schedule {
            every: every.into(),
            align: Some("hour".into()),
            window: None,
            if_missed: IfMissed::Skip,
        }
    }

    fn routine(name: &str, triggers: Vec<Trigger>, steps: Vec<Step>) -> RoutineDef {
        RoutineDef {
            routine: name.into(),
            schema_version: crate::types::SUPPORTED_SCHEMA_VERSION,
            transmit_mode: TransmitMode::Attended,
            transmit_ack: None,
            on_interrupted: OnInterrupted::Stay,
            inputs: vec![],
            triggers,
            tracks: vec![Track {
                name: "t1".into(),
                steps,
            }],
        }
    }

    fn base_ctx() -> StaticContext {
        StaticContext::new()
            .with_action(RADIO_CONNECT)
            .with_action(WEB_LOOKUP)
            .with_action(LOCAL_NOTE)
    }

    // --- SCHEDULE_COLLISION --------------------------------------------

    #[test]
    fn same_rig_30m_and_6h_aligned_schedules_collide_at_the_6h_mark() {
        // Both align:hour, epoch-anchored: every 30m fires on the half-hour
        // grid, every 6h fires on the 6h grid — since 6h is a multiple of
        // 30m, every 6h-mark IS also a 30m-mark, so the first 6h fire after
        // NOW is the first collision instant.
        let a = routine(
            "a",
            vec![schedule("30m")],
            vec![action_step("s1", "radio.connect")],
        );
        let b = routine(
            "b",
            vec![schedule("6h")],
            vec![action_step("s1", "radio.connect")],
        );
        let ctx = base_ctx();
        let mut findings = Vec::new();
        check(&[a, b], &ctx, NOW, 0, &mut findings);

        let hits: Vec<_> = findings
            .iter()
            .filter(|f| f.code == SCHEDULE_COLLISION)
            .collect();
        assert_eq!(hits.len(), 1, "{findings:?}");
        assert_eq!(hits[0].severity, Severity::Warning);
        assert!(hits[0].message.contains("\"a\""), "{}", hits[0].message);
        assert!(hits[0].message.contains("\"b\""), "{}", hits[0].message);

        // Independently compute the expected first 6h-aligned fire and
        // assert the message names that exact instant — the epoch-anchored
        // math this test's docstring claims.
        let expected_first_b_fire = next_fire(&schedule("6h"), NOW, 0).unwrap();
        assert!(
            hits[0].message.contains(&expected_first_b_fire.to_string()),
            "expected message to name unix time {expected_first_b_fire}: {}",
            hits[0].message
        );
    }

    #[test]
    fn non_radio_routines_never_collide() {
        // v1 has a single default rig, so "different rigs" isn't
        // expressible — the negative case is simply routines that never
        // touch the radio at all.
        let a = routine(
            "a",
            vec![schedule("30m")],
            vec![action_step("s1", "local.note")],
        );
        let b = routine(
            "b",
            vec![schedule("6h")],
            vec![action_step("s1", "local.note")],
        );
        let ctx = base_ctx();
        let mut findings = Vec::new();
        check(&[a, b], &ctx, NOW, 0, &mut findings);
        assert!(findings.iter().all(|f| f.code != SCHEDULE_COLLISION));
    }

    #[test]
    fn radio_routines_on_non_coinciding_schedules_do_not_collide() {
        // every=45m against align=hour never lands on the same grid as
        // every=30m against align=hour (scheduler.rs's own
        // aligned_grid_is_stable_for_non_divisor_intervals fixture), so
        // within a short horizon slice they simply never coincide... but
        // over a full 7-day horizon LCM behavior could still coincide, so
        // instead prove the negative with disjoint windows.
        let a = routine(
            "a",
            vec![Trigger::Schedule {
                every: "30m".into(),
                align: Some("hour".into()),
                window: Some("06:00-12:00".into()),
                if_missed: IfMissed::Skip,
            }],
            vec![action_step("s1", "radio.connect")],
        );
        let b = routine(
            "b",
            vec![Trigger::Schedule {
                every: "30m".into(),
                align: Some("hour".into()),
                window: Some("12:00-18:00".into()),
                if_missed: IfMissed::Skip,
            }],
            vec![action_step("s1", "radio.connect")],
        );
        let ctx = base_ctx();
        let mut findings = Vec::new();
        check(&[a, b], &ctx, NOW, 0, &mut findings);
        assert!(
            findings.iter().all(|f| f.code != SCHEDULE_COLLISION),
            "{findings:?}"
        );
    }

    #[test]
    fn radio_touching_call_closure_still_collides() {
        // "a" doesn't directly touch the radio but calls "b" which does —
        // the closure walk (not just a's own steps) must still see it.
        let radio_callee = routine(
            "radio-callee",
            vec![Trigger::Manual],
            vec![action_step("s1", "radio.connect")],
        );
        let a = routine(
            "a",
            vec![schedule("30m")],
            vec![call_step("c1", "radio-callee")],
        );
        let b = routine(
            "b",
            vec![schedule("6h")],
            vec![action_step("s1", "radio.connect")],
        );
        let ctx = base_ctx().with_routine(radio_callee);
        let mut findings = Vec::new();
        check(&[a, b], &ctx, NOW, 0, &mut findings);
        assert!(
            findings.iter().any(|f| f.code == SCHEDULE_COLLISION),
            "{findings:?}"
        );
    }

    #[test]
    fn a_call_cycle_is_cycle_safe_and_does_not_hang() {
        let b = routine(
            "b",
            vec![Trigger::Manual],
            vec![call_step("c1", "a")], // cycles back to "a"
        );
        let a = routine("a", vec![schedule("30m")], vec![call_step("c1", "b")]);
        let other = routine(
            "other",
            vec![schedule("6h")],
            vec![action_step("s1", "radio.connect")],
        );
        let ctx = base_ctx().with_routine(a.clone()).with_routine(b);
        let mut findings = Vec::new();
        // Must terminate (not hang) — the assertion is just that this
        // returns at all; neither "a" nor "b" touches the radio, so no
        // SCHEDULE_COLLISION should fire against "other" either.
        check(&[a, other], &ctx, NOW, 0, &mut findings);
        assert!(findings.iter().all(|f| f.code != SCHEDULE_COLLISION));
    }

    // --- SAME_EFFECT_OVERLAP --------------------------------------------

    #[test]
    fn shared_data_action_on_both_scheduled_routines_is_flagged() {
        let a = routine(
            "a",
            vec![schedule("30m")],
            vec![action_step("s1", "data.web_lookup")],
        );
        let b = routine(
            "b",
            vec![schedule("2h")],
            vec![action_step("s1", "data.web_lookup")],
        );
        let ctx = base_ctx();
        let mut findings = Vec::new();
        check(&[a, b], &ctx, NOW, 0, &mut findings);

        let hits: Vec<_> = findings
            .iter()
            .filter(|f| f.code == SAME_EFFECT_OVERLAP)
            .collect();
        assert_eq!(hits.len(), 1, "{findings:?}");
        assert_eq!(hits[0].severity, Severity::Warning);
        assert!(hits[0].message.contains("\"a\""), "{}", hits[0].message);
        assert!(hits[0].message.contains("\"b\""), "{}", hits[0].message);
        assert!(
            hits[0].message.contains("data.web_lookup"),
            "{}",
            hits[0].message
        );
    }

    #[test]
    fn different_data_actions_are_not_flagged() {
        let a = routine(
            "a",
            vec![schedule("30m")],
            vec![action_step("s1", "data.web_lookup")],
        );
        let b = routine(
            "b",
            vec![schedule("2h")],
            vec![action_step("s1", "local.note")],
        );
        let ctx = base_ctx();
        let mut findings = Vec::new();
        check(&[a, b], &ctx, NOW, 0, &mut findings);
        assert!(findings.iter().all(|f| f.code != SAME_EFFECT_OVERLAP));
    }

    #[test]
    fn shared_data_action_with_only_one_routine_scheduled_is_not_flagged() {
        // "b" has no Schedule trigger (manual only) — it isn't "scheduled
        // within the horizon", so this is not an overlap warning even
        // though both routines run the same data.* action.
        let a = routine(
            "a",
            vec![schedule("30m")],
            vec![action_step("s1", "data.web_lookup")],
        );
        let b = routine(
            "b",
            vec![Trigger::Manual],
            vec![action_step("s1", "data.web_lookup")],
        );
        let ctx = base_ctx();
        let mut findings = Vec::new();
        check(&[a, b], &ctx, NOW, 0, &mut findings);
        assert!(findings.iter().all(|f| f.code != SAME_EFFECT_OVERLAP));
    }

    #[test]
    fn empty_fleet_and_singleton_fleet_produce_no_findings() {
        let ctx = base_ctx();
        let mut findings = Vec::new();
        check(&[], &ctx, NOW, 0, &mut findings);
        assert!(findings.is_empty());

        let a = routine(
            "a",
            vec![schedule("30m")],
            vec![action_step("s1", "radio.connect")],
        );
        let mut findings = Vec::new();
        check(&[a], &ctx, NOW, 0, &mut findings);
        assert!(findings.is_empty());
    }
}
