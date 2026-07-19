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

use crate::consent_closure::{closure_digest, consent_closure, ConsentClosure};
use crate::types::{Control, RoutineDef, Step, StepId, TransmitAck, TransmitMode, Trigger};

use super::context::ValidationContext;
use super::findings::Finding;

pub const AUTO_TX_UNACKED: &str = "AUTO_TX_UNACKED";
pub const AUTO_WRITE_UNACKED: &str = "AUTO_WRITE_UNACKED";
pub const MIXED_MODE_STALL: &str = "MIXED_MODE_STALL";
pub const MIXED_MODE_STALL_WRITE: &str = "MIXED_MODE_STALL_WRITE";
pub const ATTENDED_UNDER_SCHEDULE: &str = "ATTENDED_UNDER_SCHEDULE";
pub const ATTENDED_WRITE_UNDER_SCHEDULE: &str = "ATTENDED_WRITE_UNDER_SCHEDULE";
pub const WRITE_VALUE_RUNTIME: &str = "WRITE_VALUE_RUNTIME";

/// Which consent class a closure is being computed for. The transmit class
/// (`transmits: true`) and the config-write class (`writes_config: true`) share
/// the exact same closure walk + digest machinery (C3); only the action-level
/// relevance predicate differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConsentClass {
    Transmit,
    Write,
}

impl ConsentClass {
    /// Does this action descriptor participate in this consent class?
    fn is_relevant(self, ctx: &dyn ValidationContext, action: &str) -> bool {
        ctx.action_descriptor(action)
            .map(|d| match self {
                ConsentClass::Transmit => d.transmits,
                ConsentClass::Write => d.writes_config,
            })
            .unwrap_or(false)
    }
}

/// Append every consent-closure finding for `def` into `findings`.
pub fn check(def: &RoutineDef, ctx: &dyn ValidationContext, findings: &mut Vec<Finding>) {
    check_auto_tx_unacked(def, ctx, findings);
    check_auto_write_unacked(def, ctx, findings);
    check_mixed_mode_stall(def, ctx, findings);
    check_mixed_mode_stall_write(def, ctx, findings);
    check_attended_under_schedule(def, ctx, findings);
    check_attended_write_under_schedule(def, ctx, findings);
    check_write_value_runtime(def, ctx, findings);
}

/// A single relevant step found in a routine's closure: `routine` names
/// whichever routine actually owns the step (may differ from the scan root, if
/// the step lives behind one or more `Call` hops).
struct ClosureHit {
    routine: String,
    track: String,
    step: StepId,
    action: String,
}

/// Compute `rd`'s consent closure for `class` via the shared
/// [`consent_closure`](crate::consent_closure::consent_closure) walk (depth-cap
/// aligned with the runtime gate). `lookup` resolves `Call` targets through the
/// validation context.
fn closure_for(
    rd: &RoutineDef,
    ctx: &dyn ValidationContext,
    class: ConsentClass,
) -> ConsentClosure {
    let lookup = |name: &str| ctx.routine_def(name);
    let is_relevant = |action: &str| class.is_relevant(ctx, action);
    consent_closure(rd, &lookup, &is_relevant)
}

/// The first relevant step in DFS pre-order, or `None` if the closure carries
/// none.
fn first_hit(closure: &ConsentClosure) -> Option<ClosureHit> {
    closure.steps.first().map(|s| ClosureHit {
        routine: s.routine.clone(),
        track: s.track.clone(),
        step: s.step.clone(),
        action: s.action.clone(),
    })
}

/// Does `ack` bind the exact `live_digest` the closure currently hashes to?
/// Fires the UNACKED finding when this is `false` — covering all three stale
/// clauses at once: **missing** (`None`), **empty** (blank `by`/`at`), and
/// **digest-mismatched** (a re-edited closure, OR a digest-less legacy ack
/// whose `closure_digest` is `None` and thus never equals a live digest).
fn ack_binds_closure(ack: &Option<TransmitAck>, live_digest: &str) -> bool {
    match ack {
        Some(a) => {
            !a.by.trim().is_empty()
                && !a.at.trim().is_empty()
                && a.closure_digest.as_deref() == Some(live_digest)
        }
        None => false,
    }
}

fn describe_hit_location(def: &RoutineDef, hit: &ClosureHit) -> String {
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

    let closure = closure_for(def, ctx, ConsentClass::Transmit);
    let Some(hit) = first_hit(&closure) else {
        return;
    };

    // The ack must bind the CURRENT closure digest — a re-edited transmit path
    // (or a digest-less legacy ack) is stale and re-fires.
    if ack_binds_closure(&def.transmit_ack, &closure_digest(&closure)) {
        return;
    }

    findings.push(
        Finding::error(
            AUTO_TX_UNACKED,
            def.routine.clone(),
            format!(
                "routine \"{}\" runs automatically and its transmit closure includes step \"{}\" \
                 (action \"{}\"){}, but transmit_ack is missing, incomplete, or no longer matches \
                 the acknowledged closure — automatic transmission requires a current recorded \
                 operator acknowledgment",
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

fn check_auto_write_unacked(
    def: &RoutineDef,
    ctx: &dyn ValidationContext,
    findings: &mut Vec<Finding>,
) {
    if def.transmit_mode != TransmitMode::Automatic {
        return;
    }

    let closure = closure_for(def, ctx, ConsentClass::Write);
    let Some(hit) = first_hit(&closure) else {
        return;
    };

    if ack_binds_closure(&def.write_ack, &closure_digest(&closure)) {
        return;
    }

    findings.push(
        Finding::error(
            AUTO_WRITE_UNACKED,
            def.routine.clone(),
            format!(
                "routine \"{}\" runs automatically and its config-write closure includes step \
                 \"{}\" (action \"{}\"){}, but write_ack is missing, incomplete, or no longer \
                 matches the acknowledged closure — automatic config writes require a current \
                 recorded operator acknowledgment",
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
/// closure, whose OWN closure (transmit or write, per the search class) is
/// non-empty.
struct MixedModeProblem {
    attended_routine: String,
    relevant_routine: String, // The routine that actually owns the relevant step
    step: StepId,
    action: String,
}

/// Depth-first search over `rd`'s call closure (`rd` itself, then every
/// routine transitively reachable via `Control::Call`) for the first node
/// that is attended-mode AND has a non-empty closure of its own for `class`.
/// `visited` guards against looping forever on a call cycle.
fn find_attended_relevant_in_closure(
    rd: &RoutineDef,
    ctx: &dyn ValidationContext,
    class: ConsentClass,
    visited: &mut HashSet<String>,
) -> Option<MixedModeProblem> {
    if !visited.insert(rd.routine.clone()) {
        return None;
    }

    if rd.transmit_mode == TransmitMode::Attended {
        if let Some(hit) = first_hit(&closure_for(rd, ctx, class)) {
            return Some(MixedModeProblem {
                attended_routine: rd.routine.clone(),
                relevant_routine: hit.routine,
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
                    find_attended_relevant_in_closure(&callee_def, ctx, class, visited)
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
                find_attended_relevant_in_closure(&callee_def, ctx, ConsentClass::Transmit, &mut visited)
            {
                let location_clause = if problem.attended_routine == problem.relevant_routine {
                    format!("in routine \"{}\"", problem.attended_routine)
                } else {
                    format!(
                        "in routine \"{}\" (reached from attended callee \"{}\")",
                        problem.relevant_routine, problem.attended_routine
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

/// The `writes_config` sibling of [`check_mixed_mode_stall`]: an automatic
/// routine whose call closure reaches an attended routine whose OWN closure
/// WRITES config. A distinct code from the transmit stall (the settings UI
/// keys transmit copy on `MIXED_MODE_STALL`), so it must not reuse it.
fn check_mixed_mode_stall_write(
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
                find_attended_relevant_in_closure(&callee_def, ctx, ConsentClass::Write, &mut visited)
            {
                let location_clause = if problem.attended_routine == problem.relevant_routine {
                    format!("in routine \"{}\"", problem.attended_routine)
                } else {
                    format!(
                        "in routine \"{}\" (reached from attended callee \"{}\")",
                        problem.relevant_routine, problem.attended_routine
                    )
                };
                findings.push(
                    Finding::warning(
                        MIXED_MODE_STALL_WRITE,
                        def.routine.clone(),
                        format!(
                            "routine \"{}\" runs automatically and its call step \"{}\" reaches \
                             attended routine \"{}\", whose config-write closure includes step \
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

    let Some(hit) = first_hit(&closure_for(def, ctx, ConsentClass::Transmit)) else {
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

/// The `writes_config` sibling of [`check_attended_under_schedule`]: a
/// scheduled attended routine whose closure WRITES config will stall an
/// unattended fire at the write-park.
fn check_attended_write_under_schedule(
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

    let Some(hit) = first_hit(&closure_for(def, ctx, ConsentClass::Write)) else {
        return;
    };

    findings.push(
        Finding::warning(
            ATTENDED_WRITE_UNDER_SCHEDULE,
            def.routine.clone(),
            format!(
                "routine \"{}\" is attended-mode but has a schedule trigger, and its config-write \
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

/// `WRITE_VALUE_RUNTIME` (Warning): a `writes_config` step in an automatic
/// routine whose params carry a `$`-reference — the written value is not fixed
/// at authoring time but resolved at run time from whoever/whatever starts the
/// run. One finding per offending top-level param key.
fn check_write_value_runtime(
    def: &RoutineDef,
    ctx: &dyn ValidationContext,
    findings: &mut Vec<Finding>,
) {
    if def.transmit_mode != TransmitMode::Automatic {
        return;
    }

    for track in &def.tracks {
        for step in &track.steps {
            let Step::Action(a) = step else { continue };
            let writes = ctx
                .action_descriptor(&a.action)
                .map(|d| d.writes_config)
                .unwrap_or(false);
            if !writes {
                continue;
            }
            let Some(map) = a.params.as_object() else {
                continue;
            };
            for (key, value) in map {
                if let Some(s) = value.as_str() {
                    if s.starts_with('$') {
                        findings.push(
                            Finding::warning(
                                WRITE_VALUE_RUNTIME,
                                def.routine.clone(),
                                format!(
                                    "step \"{}\" write param \"{}\" is \"{}\" - the value is \
                                     chosen at run time by whoever starts the run",
                                    a.id.0, key, s
                                ),
                            )
                            .with_track(track.name.clone())
                            .with_step(a.id.clone()),
                        );
                    }
                }
            }
        }
    }
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
        writes_config: false,
        name: "radio.tx",
        label: "",
        description: "",
        needs_radio: true,
        transmits: true,
        needs_internet: false,
    };
    const LOCAL_NOTE: ActionDescriptor = ActionDescriptor {
        writes_config: false,
        name: "local.note",
        label: "",
        description: "",
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
            write_ack: None,
            on_interrupted: OnInterrupted::Stay,
            inputs: vec![],
            triggers,
            tracks,
        }
    }

    // A config-write action descriptor + step helpers (mirror RADIO_TX/tx_action).
    const CONFIG_WRITE: ActionDescriptor = ActionDescriptor {
        writes_config: true,
        name: "config.set_ardop",
        label: "",
        description: "",
        needs_radio: false,
        transmits: false,
        needs_internet: false,
    };

    fn write_action(id: &str) -> Step {
        write_action_params(id, json!({}))
    }

    fn write_action_params(id: &str, params: serde_json::Value) -> Step {
        Step::Action(ActionStep {
            id: StepId(id.into()),
            action: "config.set_ardop".into(),
            params,
            timeout_s: None,
            on_radio_busy: BusyPolicy::Wait,
        })
    }

    fn write_ctx() -> StaticContext {
        base_ctx().with_action(CONFIG_WRITE)
    }

    /// The live closure digest for `def`'s transmit class under `ctx` — used to
    /// build a *matching* ack in the "valid" tests.
    fn tx_digest(def: &RoutineDef, ctx: &dyn ValidationContext) -> String {
        closure_digest(&closure_for(def, ctx, ConsentClass::Transmit))
    }

    /// The live closure digest for `def`'s write class under `ctx`.
    fn write_digest(def: &RoutineDef, ctx: &dyn ValidationContext) -> String {
        closure_digest(&closure_for(def, ctx, ConsentClass::Write))
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
        let ctx = base_ctx();
        // A valid ack must now bind the CURRENT closure digest (C3).
        def.transmit_ack = Some(TransmitAck {
            by: "KK7ABC".into(),
            at: "2026-07-13T20:00:00Z".into(),
            closure_digest: Some(tx_digest(&def, &ctx)),
        });
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
            closure_digest: None,
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
            closure_digest: None,
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
            closure_digest: None,
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

    // --- AUTO_TX_UNACKED: the digest-mismatch clause (C3) -----------------

    #[test]
    fn automatic_transmit_ack_with_stale_digest_is_flagged() {
        let mut def = routine_named(
            "r1",
            TransmitMode::Automatic,
            vec![Trigger::Manual],
            vec![track("t1", vec![tx_action("s1")])],
        );
        // by/at are non-empty but the digest names a DIFFERENT closure — stale.
        def.transmit_ack = Some(TransmitAck {
            by: "KK7ABC".into(),
            at: "2026-07-13T20:00:00Z".into(),
            closure_digest: Some("deadbeef-not-the-live-digest".into()),
        });
        let ctx = base_ctx();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        let hits: Vec<_> = findings.iter().filter(|f| f.code == AUTO_TX_UNACKED).collect();
        assert_eq!(hits.len(), 1, "stale digest must fire AUTO_TX_UNACKED");
        assert_eq!(hits[0].severity, Severity::Error);
    }

    #[test]
    fn automatic_transmit_ack_digestless_legacy_is_treated_as_stale_and_flagged() {
        let mut def = routine_named(
            "r1",
            TransmitMode::Automatic,
            vec![Trigger::Manual],
            vec![track("t1", vec![tx_action("s1")])],
        );
        // A legacy ack (pre-C3) carries by/at but no closure_digest.
        def.transmit_ack = Some(TransmitAck {
            by: "KK7ABC".into(),
            at: "2026-07-13T20:00:00Z".into(),
            closure_digest: None,
        });
        let ctx = base_ctx();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(
            findings.iter().any(|f| f.code == AUTO_TX_UNACKED),
            "digest-less legacy transmit_ack must be treated as stale: {findings:?}"
        );
    }

    // --- AUTO_WRITE_UNACKED: the three clauses (C3) -----------------------

    #[test]
    fn automatic_with_no_write_closure_is_not_auto_write_unacked() {
        let def = routine_named(
            "r1",
            TransmitMode::Automatic,
            vec![Trigger::Manual],
            vec![track("t1", vec![local_action("s1")])],
        );
        let ctx = write_ctx();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().all(|f| f.code != AUTO_WRITE_UNACKED));
    }

    #[test]
    fn automatic_with_write_closure_and_no_write_ack_is_flagged() {
        let def = routine_named(
            "r1",
            TransmitMode::Automatic,
            vec![Trigger::Manual],
            vec![track("t1", vec![write_action("s1")])],
        );
        let ctx = write_ctx();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        let hits: Vec<_> = findings
            .iter()
            .filter(|f| f.code == AUTO_WRITE_UNACKED)
            .collect();
        assert_eq!(hits.len(), 1, "missing write_ack must fire");
        assert_eq!(hits[0].severity, Severity::Error);
        assert_eq!(hits[0].step, Some(StepId("s1".into())));
        assert!(hits[0].message.contains("config.set_ardop"), "{:?}", hits[0]);
    }

    #[test]
    fn automatic_with_write_closure_and_empty_write_ack_is_flagged() {
        let mut def = routine_named(
            "r1",
            TransmitMode::Automatic,
            vec![Trigger::Manual],
            vec![track("t1", vec![write_action("s1")])],
        );
        def.write_ack = Some(TransmitAck {
            by: "   ".into(),
            at: String::new(),
            closure_digest: None,
        });
        let ctx = write_ctx();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().any(|f| f.code == AUTO_WRITE_UNACKED));
    }

    #[test]
    fn automatic_with_write_closure_and_stale_digest_write_ack_is_flagged() {
        let mut def = routine_named(
            "r1",
            TransmitMode::Automatic,
            vec![Trigger::Manual],
            vec![track("t1", vec![write_action("s1")])],
        );
        def.write_ack = Some(TransmitAck {
            by: "KK7ABC".into(),
            at: "2026-07-13T20:00:00Z".into(),
            closure_digest: Some("stale-write-digest".into()),
        });
        let ctx = write_ctx();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().any(|f| f.code == AUTO_WRITE_UNACKED));
    }

    #[test]
    fn automatic_with_write_closure_and_digestless_legacy_write_ack_is_flagged() {
        let mut def = routine_named(
            "r1",
            TransmitMode::Automatic,
            vec![Trigger::Manual],
            vec![track("t1", vec![write_action("s1")])],
        );
        def.write_ack = Some(TransmitAck {
            by: "KK7ABC".into(),
            at: "2026-07-13T20:00:00Z".into(),
            closure_digest: None,
        });
        let ctx = write_ctx();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().any(|f| f.code == AUTO_WRITE_UNACKED));
    }

    #[test]
    fn automatic_with_write_closure_and_matching_digest_write_ack_is_not_flagged() {
        let mut def = routine_named(
            "r1",
            TransmitMode::Automatic,
            vec![Trigger::Manual],
            vec![track("t1", vec![write_action("s1")])],
        );
        let ctx = write_ctx();
        def.write_ack = Some(TransmitAck {
            by: "KK7ABC".into(),
            at: "2026-07-13T20:00:00Z".into(),
            closure_digest: Some(write_digest(&def, &ctx)),
        });
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(
            findings.iter().all(|f| f.code != AUTO_WRITE_UNACKED),
            "a write_ack binding the live digest must not fire: {findings:?}"
        );
    }

    #[test]
    fn attended_with_write_closure_is_never_auto_write_unacked() {
        let def = routine_named(
            "r1",
            TransmitMode::Attended,
            vec![Trigger::Manual],
            vec![track("t1", vec![write_action("s1")])],
        );
        let ctx = write_ctx();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().all(|f| f.code != AUTO_WRITE_UNACKED));
    }

    // --- MIXED_MODE_STALL_WRITE -------------------------------------------

    #[test]
    fn automatic_calling_attended_writing_callee_is_mixed_mode_stall_write() {
        let callee = routine_named(
            "b",
            TransmitMode::Attended,
            vec![Trigger::Manual],
            vec![track("t1", vec![write_action("w1")])],
        );
        let def = routine_named(
            "r1",
            TransmitMode::Automatic,
            vec![Trigger::Manual],
            vec![track("t1", vec![call_step("c1", "b")])],
        );
        let ctx = write_ctx().with_routine(callee);
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        let hits: Vec<_> = findings
            .iter()
            .filter(|f| f.code == MIXED_MODE_STALL_WRITE)
            .collect();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].severity, Severity::Warning);
        assert_eq!(hits[0].step, Some(StepId("c1".into())));
        assert!(hits[0].message.contains("\"b\""), "{:?}", hits[0]);
        assert!(hits[0].message.contains("config-write"), "{:?}", hits[0]);
        // Distinct from the transmit stall — must NOT also emit MIXED_MODE_STALL.
        assert!(findings.iter().all(|f| f.code != MIXED_MODE_STALL));
    }

    #[test]
    fn automatic_calling_attended_non_writing_callee_is_not_mixed_mode_stall_write() {
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
        let ctx = write_ctx().with_routine(callee);
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().all(|f| f.code != MIXED_MODE_STALL_WRITE));
    }

    // --- ATTENDED_WRITE_UNDER_SCHEDULE ------------------------------------

    #[test]
    fn attended_with_schedule_trigger_and_write_closure_is_flagged() {
        let def = routine_named(
            "r1",
            TransmitMode::Attended,
            vec![schedule_trigger()],
            vec![track("t1", vec![write_action("s1")])],
        );
        let ctx = write_ctx();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        let hits: Vec<_> = findings
            .iter()
            .filter(|f| f.code == ATTENDED_WRITE_UNDER_SCHEDULE)
            .collect();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].severity, Severity::Warning);
        assert_eq!(hits[0].step, Some(StepId("s1".into())));
    }

    #[test]
    fn attended_with_manual_trigger_and_write_closure_is_not_flagged() {
        let def = routine_named(
            "r1",
            TransmitMode::Attended,
            vec![Trigger::Manual],
            vec![track("t1", vec![write_action("s1")])],
        );
        let ctx = write_ctx();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings
            .iter()
            .all(|f| f.code != ATTENDED_WRITE_UNDER_SCHEDULE));
    }

    // --- WRITE_VALUE_RUNTIME ----------------------------------------------

    #[test]
    fn automatic_write_step_with_dollar_ref_param_is_write_value_runtime() {
        let def = routine_named(
            "r1",
            TransmitMode::Automatic,
            vec![Trigger::Manual],
            vec![track(
                "t1",
                vec![write_action_params(
                    "s1",
                    json!({ "drive_level": "$s0.level" }),
                )],
            )],
        );
        let ctx = write_ctx();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        let hits: Vec<_> = findings
            .iter()
            .filter(|f| f.code == WRITE_VALUE_RUNTIME)
            .collect();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].severity, Severity::Warning);
        assert_eq!(hits[0].step, Some(StepId("s1".into())));
        assert_eq!(
            hits[0].message,
            "step \"s1\" write param \"drive_level\" is \"$s0.level\" - the value is chosen at \
             run time by whoever starts the run"
        );
    }

    #[test]
    fn automatic_write_step_with_literal_param_is_not_write_value_runtime() {
        let def = routine_named(
            "r1",
            TransmitMode::Automatic,
            vec![Trigger::Manual],
            vec![track(
                "t1",
                vec![write_action_params("s1", json!({ "drive_level": 80 }))],
            )],
        );
        let ctx = write_ctx();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().all(|f| f.code != WRITE_VALUE_RUNTIME));
    }

    #[test]
    fn non_write_step_with_dollar_ref_is_not_write_value_runtime() {
        // A $-ref on a non-writes_config action is contracts.rs's concern,
        // not WRITE_VALUE_RUNTIME's.
        let def = routine_named(
            "r1",
            TransmitMode::Automatic,
            vec![Trigger::Manual],
            vec![track(
                "t1",
                vec![Step::Action(ActionStep {
                    id: StepId("s1".into()),
                    action: "local.note".into(),
                    params: json!({ "text": "$s0.value" }),
                    timeout_s: None,
                    on_radio_busy: BusyPolicy::Wait,
                })],
            )],
        );
        let ctx = write_ctx();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().all(|f| f.code != WRITE_VALUE_RUNTIME));
    }

    #[test]
    fn attended_write_step_with_dollar_ref_is_not_write_value_runtime() {
        // WRITE_VALUE_RUNTIME only concerns automatic runs (nobody chooses the
        // value at run time in an attended run — the operator is present).
        let def = routine_named(
            "r1",
            TransmitMode::Attended,
            vec![Trigger::Manual],
            vec![track(
                "t1",
                vec![write_action_params("s1", json!({ "drive_level": "$s0.level" }))],
            )],
        );
        let ctx = write_ctx();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().all(|f| f.code != WRITE_VALUE_RUNTIME));
    }
}
