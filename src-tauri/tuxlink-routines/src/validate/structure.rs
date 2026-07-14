//! Structural checks (spec §10 layer 1, plan-3 task 3): the static twins of
//! `executor.rs`'s runtime graph-walk semantics — reachability, retry
//! wiring, branch cycles, and call recursion.
//!
//! **Per-track graph.** Every check below except the call checks builds the
//! same directed graph over one track's `steps` array and reuses it, so the
//! edge rules live in exactly one place ([`build_graph`]) and mirror
//! `executor.rs::run_track_shared` exactly:
//!
//! - An `Action`, `Retry`, `Delay`, or `Call` step (any non-terminal,
//!   non-`Branch` step) has one outgoing "sequence" edge to the next step
//!   in the array — the same `idx += 1` every one of those arms takes at
//!   runtime. `Retry`'s wrapped target is reached by the retry mechanism
//!   itself, not by this sequence edge — see `retry_target_ids` below and
//!   [`UNREACHABLE_STEP`]'s exemption.
//! - A `Branch` step has one outgoing edge per arm (`then`, `else`): the
//!   arm's first step id if non-empty (only the first id is ever a real
//!   jump target — `executor.rs` only inspects `arm.first()`), or a
//!   fall-through sequence edge to the next step if the arm is empty. A
//!   dangling arm target (an id naming no step in this track) contributes
//!   no edge — v1 does not add a code for that case; the runtime error
//!   ("branch target not found") reports it during an actual run.
//! - `End` has no outgoing edge — it terminates the run.
//!
//! The graph carries one synthetic sentinel node at index `steps.len()`
//! ("OUT") representing "the track's step array ran out." A sequence edge
//! from the LAST step targets OUT exactly when `executor.rs`'s
//! `while idx < track.steps.len()` loop would exit normally
//! (`TrackEnd::Completed`) along that path — so "OUT is reachable from the
//! start" is precisely [`NO_TERMINAL_PATH`]'s condition.
//!
//! A track with zero steps trivially completes without ever taking a step
//! (`run_track_shared`'s `while` loop never runs) and is treated as
//! vacuously fine here — [`NO_TERMINAL_PATH`] and [`UNREACHABLE_STEP`] both
//! skip it; there is no authored step content to warn about or fail to
//! reach.

use std::collections::HashSet;

use crate::types::{Control, RoutineDef, Step, StepId, Track};

use super::context::ValidationContext;
use super::findings::Finding;

pub const UNREACHABLE_STEP: &str = "UNREACHABLE_STEP";
pub const NO_TERMINAL_PATH: &str = "NO_TERMINAL_PATH";
pub const RETRY_ZERO_ATTEMPTS: &str = "RETRY_ZERO_ATTEMPTS";
pub const RETRY_TARGET_MISSING: &str = "RETRY_TARGET_MISSING";
pub const RETRY_TARGET_NOT_ACTION: &str = "RETRY_TARGET_NOT_ACTION";
pub const BRANCH_CYCLE: &str = "BRANCH_CYCLE";
pub const BRANCH_TARGET_MISSING: &str = "BRANCH_TARGET_MISSING";
pub const CALL_RECURSION: &str = "CALL_RECURSION";
pub const CALL_TARGET_MISSING: &str = "CALL_TARGET_MISSING";

/// Append every structural finding for `def` into `findings`. Retry/graph
/// checks are pure over `def`; the call checks need `ctx.routine_def` to
/// walk a call closure beyond `def` itself.
pub fn check(def: &RoutineDef, ctx: &dyn ValidationContext, findings: &mut Vec<Finding>) {
    for track in &def.tracks {
        check_retry_controls(def, track, findings);
        check_graph_properties(def, track, findings);
    }
    check_calls(def, ctx, findings);
}

fn find_index(track: &Track, id: &StepId) -> Option<usize> {
    track.steps.iter().position(|s| s.id() == id)
}

// --- Retry wiring (RETRY_ZERO_ATTEMPTS / RETRY_TARGET_MISSING / RETRY_TARGET_NOT_ACTION) ---

fn check_retry_controls(def: &RoutineDef, track: &Track, findings: &mut Vec<Finding>) {
    for step in &track.steps {
        let Step::Control(c) = step else { continue };
        let Control::Retry {
            step: target,
            attempts,
            ..
        } = &c.control
        else {
            continue;
        };

        if *attempts == 0 {
            findings.push(
                Finding::error(
                    RETRY_ZERO_ATTEMPTS,
                    def.routine.clone(),
                    format!(
                        "retry step \"{}\" has attempts: 0 — its target \"{}\" can never execute",
                        c.id.0, target.0
                    ),
                )
                .with_track(track.name.clone())
                .with_step(c.id.clone()),
            );
        }

        match find_index(track, target) {
            None => {
                findings.push(
                    Finding::error(
                        RETRY_TARGET_MISSING,
                        def.routine.clone(),
                        format!(
                            "retry step \"{}\" targets \"{}\", which is not a step in track \"{}\"",
                            c.id.0, target.0, track.name
                        ),
                    )
                    .with_track(track.name.clone())
                    .with_step(c.id.clone()),
                );
            }
            Some(idx) => {
                if !matches!(track.steps[idx], Step::Action(_)) {
                    findings.push(
                        Finding::error(
                            RETRY_TARGET_NOT_ACTION,
                            def.routine.clone(),
                            format!(
                                "retry step \"{}\" targets \"{}\", which is not an action step",
                                c.id.0, target.0
                            ),
                        )
                        .with_track(track.name.clone())
                        .with_step(c.id.clone()),
                    );
                }
            }
        }
    }
}

/// The set of step ids that are some reachable `Retry`'s wrapped target in
/// `track` — exempt from [`UNREACHABLE_STEP`] (see module doc: they are
/// reached by the retry mechanism, not by a graph edge). Only targets whose
/// wrapping Retry step IS reachable are included; if the Retry itself is
/// unreachable, its target is not exempted.
fn retry_target_ids<'a>(track: &'a Track, reachable: &HashSet<usize>) -> HashSet<&'a StepId> {
    track
        .steps
        .iter()
        .enumerate()
        .filter_map(|(i, s)| match s {
            Step::Control(c) => match &c.control {
                Control::Retry { step, .. } if reachable.contains(&i) => Some(step),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

// --- Graph (UNREACHABLE_STEP / NO_TERMINAL_PATH / BRANCH_CYCLE) ---

/// Build the sequence+branch adjacency list described in the module doc.
/// Node `steps.len()` is the synthetic OUT sentinel; array size is
/// `steps.len() + 1` so every sequence edge off the last step is a valid
/// index.
///
/// Also returns a list of dangling branch targets (branches that name
/// nonexistent step ids) as tuples of (branch_step_index, target_id).
fn build_graph(track: &Track) -> (Vec<Vec<usize>>, Vec<(usize, StepId)>) {
    let n = track.steps.len();
    let index_of: std::collections::HashMap<&StepId, usize> = track
        .steps
        .iter()
        .enumerate()
        .map(|(i, s)| (s.id(), i))
        .collect();
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n + 1];
    let mut dangling_targets: Vec<(usize, StepId)> = Vec::new();

    for (i, step) in track.steps.iter().enumerate() {
        match step {
            Step::Action(_) => adj[i].push(i + 1),
            Step::Control(c) => match &c.control {
                Control::Branch { then, r#else, .. } => {
                    for arm in [then, r#else] {
                        match arm.first() {
                            Some(target) => {
                                if let Some(&ti) = index_of.get(target) {
                                    adj[i].push(ti);
                                } else {
                                    // Dangling arm target: record for error reporting.
                                    dangling_targets.push((i, target.clone()));
                                }
                            }
                            None => adj[i].push(i + 1), // empty arm falls through
                        }
                    }
                }
                Control::Retry { .. } | Control::Delay { .. } | Control::Call { .. } => {
                    adj[i].push(i + 1);
                }
                Control::End { .. } => {} // terminal: no outgoing edge
            },
        }
    }
    (adj, dangling_targets)
}

fn reachable_from_start(adj: &[Vec<usize>]) -> HashSet<usize> {
    let mut seen = HashSet::new();
    let mut stack = vec![0usize];
    seen.insert(0usize);
    while let Some(u) = stack.pop() {
        for &v in &adj[u] {
            if seen.insert(v) {
                stack.push(v);
            }
        }
    }
    seen
}

/// DFS cycle detection over `adj` (white/gray/black coloring). Returns the
/// first back-edge found, `(from, to)`, i.e. `from` jumps back to the
/// still-on-stack ancestor `to`. `adj`'s OUT sentinel node has no outgoing
/// edges by construction, so it can never be part of a cycle — a `Some`
/// result always indexes into real steps.
fn find_cycle(adj: &[Vec<usize>]) -> Option<(usize, usize)> {
    #[derive(Clone, Copy, PartialEq)]
    enum Color {
        White,
        Gray,
        Black,
    }
    fn visit(u: usize, adj: &[Vec<usize>], color: &mut [Color]) -> Option<(usize, usize)> {
        color[u] = Color::Gray;
        for &v in &adj[u] {
            match color[v] {
                Color::Gray => return Some((u, v)),
                Color::White => {
                    if let Some(found) = visit(v, adj, color) {
                        return Some(found);
                    }
                }
                Color::Black => {}
            }
        }
        color[u] = Color::Black;
        None
    }

    let mut color = vec![Color::White; adj.len()];
    for start in 0..adj.len() {
        if color[start] == Color::White {
            if let Some(found) = visit(start, adj, &mut color) {
                return Some(found);
            }
        }
    }
    None
}

fn check_graph_properties(def: &RoutineDef, track: &Track, findings: &mut Vec<Finding>) {
    let (adj, dangling_targets) = build_graph(track);
    let n = track.steps.len();

    // Emit errors for any branch arms that name nonexistent step ids.
    for (branch_idx, target_id) in dangling_targets {
        findings.push(
            Finding::error(
                BRANCH_TARGET_MISSING,
                def.routine.clone(),
                format!(
                    "branch step \"{}\" targets \"{}\", which is not a step in track \"{}\"",
                    track.steps[branch_idx].id().0,
                    target_id.0,
                    track.name
                ),
            )
            .with_track(track.name.clone())
            .with_step(track.steps[branch_idx].id().clone()),
        );
    }

    if n > 0 {
        let reachable = reachable_from_start(&adj);
        let retry_targets = retry_target_ids(track, &reachable);

        for (i, step) in track.steps.iter().enumerate() {
            if !reachable.contains(&i) && !retry_targets.contains(step.id()) {
                findings.push(
                    Finding::error(
                        UNREACHABLE_STEP,
                        def.routine.clone(),
                        format!(
                            "step \"{}\" in track \"{}\" is never reached by sequential flow or a branch jump",
                            step.id().0, track.name
                        ),
                    )
                    .with_track(track.name.clone())
                    .with_step(step.id().clone()),
                );
            }
        }

        if reachable.contains(&n) {
            findings.push(
                Finding::warning(
                    NO_TERMINAL_PATH,
                    def.routine.clone(),
                    format!(
                        "track \"{}\" can run past its last step without hitting an explicit End",
                        track.name
                    ),
                )
                .with_track(track.name.clone()),
            );
        }
    }

    if let Some((from, to)) = find_cycle(&adj) {
        findings.push(
            Finding::error(
                BRANCH_CYCLE,
                def.routine.clone(),
                format!(
                    "step \"{}\" in track \"{}\" jumps back to step \"{}\", forming a cycle — \
                     routines must terminate (the runtime's {}-step budget is defense-in-depth, \
                     not the primary guard)",
                    track.steps[from].id().0,
                    track.name,
                    track.steps[to].id().0,
                    crate::executor::MAX_STEPS_PER_TRACK
                ),
            )
            .with_track(track.name.clone())
            .with_step(track.steps[from].id().clone()),
        );
    }
}

// --- Calls (CALL_TARGET_MISSING / CALL_RECURSION) ---

fn check_calls(def: &RoutineDef, ctx: &dyn ValidationContext, findings: &mut Vec<Finding>) {
    for track in &def.tracks {
        for step in &track.steps {
            let Step::Control(c) = step else { continue };
            let Control::Call {
                routine: callee, ..
            } = &c.control
            else {
                continue;
            };

            // Recursion first: `closure_reaches` short-circuits on
            // `callee == def.routine` without needing a `ctx` lookup, so a
            // direct or transitive self-call is caught even when `def`
            // itself (still being drafted, maybe unsaved) isn't registered
            // in `ctx` under its own name.
            let mut visited = HashSet::new();
            if closure_reaches(callee, &def.routine, ctx, &mut visited) {
                findings.push(
                    Finding::error(
                        CALL_RECURSION,
                        def.routine.clone(),
                        format!(
                            "call step \"{}\" invokes \"{callee}\", whose call closure eventually \
                             calls \"{}\" again — routines must not recurse",
                            c.id.0, def.routine
                        ),
                    )
                    .with_track(track.name.clone())
                    .with_step(c.id.clone()),
                );
            }

            // A call back to `def.routine` itself is always "known" (it's
            // literally the routine being validated) regardless of whether
            // `ctx`'s store happens to have it registered under its own
            // name yet — only check existence for every OTHER callee name.
            if callee != &def.routine && ctx.routine_def(callee).is_none() {
                findings.push(
                    Finding::error(
                        CALL_TARGET_MISSING,
                        def.routine.clone(),
                        format!(
                            "call step \"{}\" invokes \"{callee}\", which is not a known routine",
                            c.id.0
                        ),
                    )
                    .with_track(track.name.clone())
                    .with_step(c.id.clone()),
                );
            }
        }
    }
}

/// Does the call closure reachable from `current` (following `Control::Call`
/// through `ctx.routine_def`, depth-first) ever reach `target`? `visited`
/// guards against looping forever on a cycle that does NOT involve `target`
/// (e.g. B -> C -> B while checking whether A's closure reaches A).
fn closure_reaches(
    current: &str,
    target: &str,
    ctx: &dyn ValidationContext,
    visited: &mut HashSet<String>,
) -> bool {
    if current == target {
        return true;
    }
    if !visited.insert(current.to_string()) {
        return false;
    }
    let Some(rd) = ctx.routine_def(current) else {
        return false;
    };
    for track in &rd.tracks {
        for step in &track.steps {
            let Step::Control(c) = step else { continue };
            let Control::Call {
                routine: callee, ..
            } = &c.control
            else {
                continue;
            };
            if closure_reaches(callee, target, ctx, visited) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        ActionStep, BusyPolicy, ControlStep, OnInterrupted, RoutineDef, TransmitMode, Trigger,
    };
    use crate::validate::context::StaticContext;
    use serde_json::json;

    fn action(id: &str) -> Step {
        Step::Action(ActionStep {
            id: StepId(id.into()),
            action: "local.note".into(),
            params: json!({}),
            timeout_s: None,
            on_radio_busy: BusyPolicy::Wait,
        })
    }

    fn end(id: &str) -> Step {
        Step::Control(ControlStep {
            id: StepId(id.into()),
            control: Control::End {
                failed: false,
                reason: None,
            },
        })
    }

    fn branch(id: &str, on: &str, then: Vec<&str>, r#else: Vec<&str>) -> Step {
        Step::Control(ControlStep {
            id: StepId(id.into()),
            control: Control::Branch {
                on: on.into(),
                then: then.into_iter().map(|s| StepId(s.into())).collect(),
                r#else: r#else.into_iter().map(|s| StepId(s.into())).collect(),
            },
        })
    }

    fn retry(id: &str, target: &str, attempts: u32) -> Step {
        Step::Control(ControlStep {
            id: StepId(id.into()),
            control: Control::Retry {
                step: StepId(target.into()),
                attempts,
                backoff_s: 0,
            },
        })
    }

    fn call(id: &str, routine: &str) -> Step {
        Step::Control(ControlStep {
            id: StepId(id.into()),
            control: Control::Call {
                routine: routine.into(),
                args: json!({}),
                sync: true,
            },
        })
    }

    fn routine_named(name: &str, tracks: Vec<Track>) -> RoutineDef {
        RoutineDef {
            routine: name.into(),
            schema_version: crate::types::SUPPORTED_SCHEMA_VERSION,
            transmit_mode: TransmitMode::Attended,
            transmit_ack: None,
            on_interrupted: OnInterrupted::Stay,
            inputs: vec![],
            triggers: vec![Trigger::Manual],
            tracks,
        }
    }

    fn track(name: &str, steps: Vec<Step>) -> Track {
        Track {
            name: name.into(),
            steps,
        }
    }

    // --- UNREACHABLE_STEP ---------------------------------------------

    #[test]
    fn a_step_with_no_incoming_edge_is_unreachable() {
        // s1 -> e1 (End, terminal). s3 has nothing pointing at it.
        let def = routine_named(
            "r1",
            vec![track("t1", vec![action("s1"), end("e1"), action("s3")])],
        );
        let ctx = StaticContext::new();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        let unreachable: Vec<_> = findings
            .iter()
            .filter(|f| f.code == UNREACHABLE_STEP)
            .collect();
        assert_eq!(unreachable.len(), 1);
        assert_eq!(unreachable[0].step, Some(StepId("s3".into())));
    }

    #[test]
    fn a_normal_sequential_track_has_no_unreachable_steps() {
        let def = routine_named(
            "r1",
            vec![track("t1", vec![action("s1"), action("s2"), end("e1")])],
        );
        let ctx = StaticContext::new();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().all(|f| f.code != UNREACHABLE_STEP));
    }

    #[test]
    fn a_retry_target_positioned_out_of_natural_flow_is_exempt_from_unreachable_step() {
        // r1 (Retry targeting s2) -> e1 (End, terminal). s2 sits after the
        // End, unreachable by any graph edge, but it is r1's wrapped
        // target — the exemption must apply.
        let def = routine_named(
            "r1",
            vec![track(
                "t1",
                vec![retry("r1", "s2", 1), end("e1"), action("s2")],
            )],
        );
        let ctx = StaticContext::new();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(
            findings.iter().all(|f| f.code != UNREACHABLE_STEP),
            "{findings:?}"
        );
    }

    // --- NO_TERMINAL_PATH ------------------------------------------------

    #[test]
    fn a_track_with_no_end_step_warns_no_terminal_path() {
        let def = routine_named("r1", vec![track("t1", vec![action("s1")])]);
        let ctx = StaticContext::new();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert_eq!(
            findings
                .iter()
                .filter(|f| f.code == NO_TERMINAL_PATH)
                .count(),
            1
        );
        assert_eq!(findings[0].severity, super::super::Severity::Warning);
    }

    #[test]
    fn a_track_that_always_hits_end_does_not_warn_no_terminal_path() {
        let def = routine_named("r1", vec![track("t1", vec![action("s1"), end("e1")])]);
        let ctx = StaticContext::new();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().all(|f| f.code != NO_TERMINAL_PATH));
    }

    #[test]
    fn an_empty_track_never_warns_no_terminal_path() {
        let def = routine_named("r1", vec![track("t1", vec![])]);
        let ctx = StaticContext::new();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.is_empty());
    }

    // --- RETRY_ZERO_ATTEMPTS / RETRY_TARGET_MISSING / RETRY_TARGET_NOT_ACTION ---

    #[test]
    fn retry_with_zero_attempts_is_flagged() {
        let def = routine_named(
            "r1",
            vec![track("t1", vec![retry("r1", "s1", 0), action("s1")])],
        );
        let ctx = StaticContext::new();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().any(|f| f.code == RETRY_ZERO_ATTEMPTS));
    }

    #[test]
    fn retry_with_nonzero_attempts_is_not_flagged() {
        let def = routine_named(
            "r1",
            vec![track("t1", vec![retry("r1", "s1", 3), action("s1")])],
        );
        let ctx = StaticContext::new();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().all(|f| f.code != RETRY_ZERO_ATTEMPTS));
    }

    #[test]
    fn retry_target_missing_from_the_track_is_flagged() {
        let def = routine_named("r1", vec![track("t1", vec![retry("r1", "ghost", 3)])]);
        let ctx = StaticContext::new();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().any(|f| f.code == RETRY_TARGET_MISSING));
    }

    #[test]
    fn retry_target_present_in_the_track_is_not_flagged_missing() {
        let def = routine_named(
            "r1",
            vec![track("t1", vec![retry("r1", "s1", 3), action("s1")])],
        );
        let ctx = StaticContext::new();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().all(|f| f.code != RETRY_TARGET_MISSING));
    }

    #[test]
    fn retry_target_that_is_not_an_action_is_flagged() {
        let def = routine_named(
            "r1",
            vec![track("t1", vec![retry("r1", "e1", 3), end("e1")])],
        );
        let ctx = StaticContext::new();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().any(|f| f.code == RETRY_TARGET_NOT_ACTION));
    }

    #[test]
    fn retry_target_that_is_an_action_is_not_flagged_not_action() {
        let def = routine_named(
            "r1",
            vec![track("t1", vec![retry("r1", "s1", 3), action("s1")])],
        );
        let ctx = StaticContext::new();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().all(|f| f.code != RETRY_TARGET_NOT_ACTION));
    }

    // --- BRANCH_CYCLE ------------------------------------------------

    #[test]
    fn a_backward_branch_jump_is_a_cycle() {
        // Exact shape of executor.rs's branch_cycle_hits_the_step_budget test.
        let def = routine_named(
            "r1",
            vec![track(
                "t1",
                vec![action("a1"), branch("b1", "a1.go", vec!["a1"], vec![])],
            )],
        );
        let ctx = StaticContext::new();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        let cycles: Vec<_> = findings.iter().filter(|f| f.code == BRANCH_CYCLE).collect();
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].severity, super::super::Severity::Error);
        assert!(cycles[0].message.contains("a1"), "{:?}", cycles[0].message);
        assert!(cycles[0].message.contains("b1"), "{:?}", cycles[0].message);
    }

    #[test]
    fn forward_only_branches_are_not_a_cycle() {
        let def = routine_named(
            "r1",
            vec![track(
                "t1",
                vec![
                    action("s1"),
                    branch("b1", "s1.connected", vec!["s2"], vec!["s3"]),
                    action("s2"),
                    end("e1"),
                    action("s3"),
                    end("e2"),
                ],
            )],
        );
        let ctx = StaticContext::new();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().all(|f| f.code != BRANCH_CYCLE));
    }

    // --- CALL_TARGET_MISSING / CALL_RECURSION ---

    #[test]
    fn call_to_an_unregistered_routine_is_flagged_missing() {
        let def = routine_named("r1", vec![track("t1", vec![call("c1", "nope")])]);
        let ctx = StaticContext::new();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().any(|f| f.code == CALL_TARGET_MISSING));
        // No registered def to recurse into, so recursion never fires here.
        assert!(findings.iter().all(|f| f.code != CALL_RECURSION));
    }

    #[test]
    fn call_to_a_registered_routine_is_not_flagged_missing() {
        let callee = routine_named("callee", vec![track("t1", vec![end("e1")])]);
        let def = routine_named("r1", vec![track("t1", vec![call("c1", "callee")])]);
        let ctx = StaticContext::new().with_routine(callee);
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().all(|f| f.code != CALL_TARGET_MISSING));
    }

    #[test]
    fn a_call_chain_that_returns_to_the_root_routine_is_recursion() {
        // r1 -> callee "b", whose own closure calls "r1" again.
        let b = routine_named("b", vec![track("t1", vec![call("c2", "r1")])]);
        let def = routine_named("r1", vec![track("t1", vec![call("c1", "b")])]);
        let ctx = StaticContext::new().with_routine(b);
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        let recursion: Vec<_> = findings
            .iter()
            .filter(|f| f.code == CALL_RECURSION)
            .collect();
        assert_eq!(recursion.len(), 1);
        assert_eq!(recursion[0].step, Some(StepId("c1".into())));
        assert!(
            recursion[0].message.contains("\"b\""),
            "{:?}",
            recursion[0].message
        );
        assert!(
            recursion[0].message.contains("\"r1\""),
            "{:?}",
            recursion[0].message
        );
    }

    #[test]
    fn a_call_chain_that_never_returns_to_the_root_is_not_recursion() {
        let b = routine_named("b", vec![track("t1", vec![end("e1")])]);
        let def = routine_named("r1", vec![track("t1", vec![call("c1", "b")])]);
        let ctx = StaticContext::new().with_routine(b);
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().all(|f| f.code != CALL_RECURSION));
    }

    #[test]
    fn a_sibling_cycle_that_never_touches_the_root_does_not_infinite_loop_or_false_flag() {
        // b -> c -> b (a cycle entirely among callees), root "r1" calls "b"
        // but is never itself part of the loop.
        let c = routine_named("c", vec![track("t1", vec![call("c3", "b")])]);
        let b = routine_named("b", vec![track("t1", vec![call("c2", "c")])]);
        let def = routine_named("r1", vec![track("t1", vec![call("c1", "b")])]);
        let ctx = StaticContext::new().with_routine(b).with_routine(c);
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().all(|f| f.code != CALL_RECURSION));
    }

    #[test]
    fn direct_self_call_is_recursion() {
        let def = routine_named("r1", vec![track("t1", vec![call("c1", "r1")])]);
        let ctx = StaticContext::new();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().any(|f| f.code == CALL_RECURSION));
    }

    // --- BRANCH_TARGET_MISSING ------------------------------------------------

    #[test]
    fn branch_with_dangling_then_target_is_flagged() {
        // b1 has then-arm targeting nonexistent "ghost".
        let def = routine_named(
            "r1",
            vec![track(
                "t1",
                vec![action("s1"), branch("b1", "s1.go", vec!["ghost"], vec![])],
            )],
        );
        let ctx = StaticContext::new();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        let missing: Vec<_> = findings
            .iter()
            .filter(|f| f.code == BRANCH_TARGET_MISSING)
            .collect();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].step, Some(StepId("b1".into())));
        assert!(
            missing[0].message.contains("ghost"),
            "{:?}",
            missing[0].message
        );
        assert!(
            missing[0].message.contains("b1"),
            "{:?}",
            missing[0].message
        );
    }

    #[test]
    fn branch_with_dangling_else_target_is_flagged() {
        // b1 has else-arm targeting nonexistent "ghost".
        let def = routine_named(
            "r1",
            vec![track(
                "t1",
                vec![action("s1"), branch("b1", "s1.go", vec![], vec!["ghost"])],
            )],
        );
        let ctx = StaticContext::new();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        let missing: Vec<_> = findings
            .iter()
            .filter(|f| f.code == BRANCH_TARGET_MISSING)
            .collect();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].step, Some(StepId("b1".into())));
    }

    #[test]
    fn branch_with_both_arms_valid_is_not_flagged() {
        // b1 has valid then and else targets.
        let def = routine_named(
            "r1",
            vec![track(
                "t1",
                vec![
                    action("s1"),
                    branch("b1", "s1.go", vec!["s2"], vec!["s3"]),
                    action("s2"),
                    end("e1"),
                    action("s3"),
                    end("e2"),
                ],
            )],
        );
        let ctx = StaticContext::new();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(
            findings.iter().all(|f| f.code != BRANCH_TARGET_MISSING),
            "{findings:?}"
        );
    }

    #[test]
    fn unreachable_retry_does_not_exempt_its_target_from_unreachable_step() {
        // Counter-example: e0 (End, terminal), then r1 (Retry targeting s2),
        // then s2 (Action). r1 is unreachable because e0 terminates. s2 is
        // unreachable by any graph edge AND not exempted (because r1 is not
        // reachable). Both should be flagged UNREACHABLE_STEP.
        let def = routine_named(
            "r1",
            vec![track(
                "t1",
                vec![end("e0"), retry("r1", "s2", 1), action("s2")],
            )],
        );
        let ctx = StaticContext::new();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        let unreachable: Vec<_> = findings
            .iter()
            .filter(|f| f.code == UNREACHABLE_STEP)
            .collect();
        // Both r1 and s2 should be unreachable.
        assert_eq!(unreachable.len(), 2);
        let unreachable_ids: std::collections::HashSet<_> =
            unreachable.iter().map(|f| &f.step).collect();
        assert!(unreachable_ids.contains(&Some(StepId("r1".into()))));
        assert!(unreachable_ids.contains(&Some(StepId("s2".into()))));
    }
}
