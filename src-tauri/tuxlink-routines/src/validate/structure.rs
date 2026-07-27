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
pub const ARM_FALLTHROUGH_LEAK: &str = "ARM_FALLTHROUGH_LEAK";
pub const CALL_RECURSION: &str = "CALL_RECURSION";
pub const CALL_TARGET_MISSING: &str = "CALL_TARGET_MISSING";
pub const BRANCH_OP_VALUE_PAIR: &str = "BRANCH_OP_VALUE_PAIR";
pub const BRANCH_BOTH_ARMS_EMPTY: &str = "BRANCH_BOTH_ARMS_EMPTY";
pub const TX_ONLY_ON_FAILURE_ARM: &str = "TX_ONLY_ON_FAILURE_ARM";

/// Append every structural finding for `def` into `findings`. Retry/graph
/// checks are pure over `def`; the call checks need `ctx.routine_def` to
/// walk a call closure beyond `def` itself.
pub fn check(def: &RoutineDef, ctx: &dyn ValidationContext, findings: &mut Vec<Finding>) {
    for track in &def.tracks {
        let before = findings.len();
        check_retry_controls(def, track, findings);
        check_graph_properties(def, track, findings);
        check_arm_fallthrough_leaks(def, track, findings);
        cross_reference_terminal_and_leak(&mut findings[before..]);
        check_branch_shapes(def, track, findings);
        check_tx_only_on_failure_arm(def, track, ctx, findings);
    }
    check_calls(def, ctx, findings);
}

/// BRANCH_OP_VALUE_PAIR / BRANCH_BOTH_ARMS_EMPTY (tuxlink-rrk51, lnctz
/// evidence): both shapes save and validate clean today and then do nothing
/// useful — op-without-value is a guaranteed runtime step error
/// (`executor.rs::eval_branch_condition`'s "op and value must be supplied
/// together" arm), so it is an Error here, same rationale as
/// `UNKNOWN_READ_SOURCE`'s severity; an all-empty-arms branch falls through
/// to the same next step on BOTH outcomes (`ARM_FALLTHROUGH_LEAK`
/// deliberately skips it), deciding nothing, which in the observed corpus
/// was always an editing accident, so it earns a Warning.
fn check_branch_shapes(def: &RoutineDef, track: &Track, findings: &mut Vec<Finding>) {
    for step in &track.steps {
        let Step::Control(c) = step else { continue };
        let Control::Branch {
            op,
            value,
            then,
            r#else,
            ..
        } = &c.control
        else {
            continue;
        };
        if op.is_some() != value.is_some() {
            let (present, missing) = if op.is_some() {
                ("op", "value")
            } else {
                ("value", "op")
            };
            findings.push(
                Finding::error(
                    BRANCH_OP_VALUE_PAIR,
                    def.routine.clone(),
                    format!(
                        "branch \"{}\" has {present} without {missing} — this fails at run \
                         time. Supply op AND value together to compare, or neither for the \
                         strict-boolean form (on must then resolve to a boolean)",
                        c.id.0
                    ),
                )
                .with_track(track.name.clone())
                .with_step(c.id.clone()),
            );
        }
        if then.is_empty() && r#else.is_empty() {
            findings.push(
                Finding::warning(
                    BRANCH_BOTH_ARMS_EMPTY,
                    def.routine.clone(),
                    format!(
                        "branch \"{}\" has an empty then AND an empty else: both outcomes \
                         fall through to the same next step, so the branch decides nothing. \
                         Point at least one arm at a target step — then runs when the \
                         condition is TRUE, else when it is FALSE",
                        c.id.0
                    ),
                )
                .with_track(track.name.clone())
                .with_step(c.id.clone()),
            );
        }
    }
}

/// TX_ONLY_ON_FAILURE_ARM (tuxlink-rrk51): a transmitting step reachable
/// ONLY through the else arm of a strict-boolean `*.connected` branch runs
/// exclusively when the connection FAILED. The lnctz corpus shows models
/// wiring a success confirmation there while narrating it as the success
/// path (skill/S4/rev_off: the model diagnosed exactly this inversion,
/// re-wired it inverted again, and nothing told it which arm the step landed
/// in). A failure alert is a legitimate resident of that arm, so this is a
/// Warning that teaches both readings, like `ARM_FALLTHROUGH_LEAK`. Scoped
/// to `.connected` strict-boolean branches deliberately: for threshold gates
/// (k_index gte 4) the else arm is often the CORRECT transmit path, and a
/// broader check would misfire there.
fn check_tx_only_on_failure_arm(
    def: &RoutineDef,
    track: &Track,
    ctx: &dyn ValidationContext,
    findings: &mut Vec<Finding>,
) {
    let tx_steps: Vec<usize> = track
        .steps
        .iter()
        .enumerate()
        .filter_map(|(i, s)| match s {
            Step::Action(a) => ctx
                .action_descriptor(&a.action)
                .filter(|d| d.transmits)
                .map(|_| i),
            _ => None,
        })
        .collect();
    if tx_steps.is_empty() {
        return;
    }

    let (adj, _) = build_graph(track);
    let base_reach = reachable_from_start(&adj);

    for (i, step) in track.steps.iter().enumerate() {
        let Step::Control(c) = step else { continue };
        let Control::Branch {
            on,
            op,
            then,
            r#else,
            ..
        } = &c.control
        else {
            continue;
        };
        if op.is_some() || !on.ends_with(".connected") || r#else.is_empty() {
            continue;
        }
        // The branch must test a CONNECT ATTEMPT's own output (Codex
        // 2026-07-27 P2): `.connected` also appears on status reads (e.g. a
        // modem_status snapshot), where the else arm means "modem idle", not
        // "a dial failed" — and dialing WHEN idle is exactly right there.
        // The producer must be a same-track action the context classifies as
        // outbox-flushing (a connect); anything else carries no
        // failed-attempt semantics and is skipped.
        let producer_is_connect = on.split('.').next().is_some_and(|producer_id| {
            track.steps.iter().any(|s| match s {
                Step::Action(a) => a.id.0 == producer_id && ctx.flushes_outbox(&a.action),
                _ => false,
            })
        });
        if !producer_is_connect {
            continue;
        }
        let Some(else_entry) = r#else.first().and_then(|t| find_index(track, t)) else {
            continue; // dangling target: BRANCH_TARGET_MISSING's job
        };
        // Both arms entering the same step claims no exclusivity; and if the
        // then arm shares the else entry, removing the edge would cut both.
        let then_entry = match then.first() {
            Some(t) => find_index(track, t),
            None => Some(i + 1),
        };
        if then_entry == Some(else_entry) {
            continue;
        }
        let mut pruned = adj.clone();
        pruned[i].retain(|&v| v != else_entry);
        let pruned_reach = reachable_from_start(&pruned);

        for &t in &tx_steps {
            if base_reach.contains(&t) && !pruned_reach.contains(&t) {
                let (tx_id, tx_action) = match &track.steps[t] {
                    Step::Action(a) => (&a.id, a.action.as_str()),
                    _ => unreachable!("tx_steps only holds action indices"),
                };
                findings.push(
                    Finding::warning(
                        TX_ONLY_ON_FAILURE_ARM,
                        def.routine.clone(),
                        format!(
                            "\"{}\" ({tx_action}) transmits and is reachable ONLY through \
                             branch \"{}\"'s else arm — it runs exclusively when \"{on}\" is \
                             FALSE (the connection failed). If it is a failure alert, that \
                             is correct; if it is meant to confirm a successful connection, \
                             it never will — move it into the then arm (condition TRUE)",
                            tx_id.0, c.id.0
                        ),
                    )
                    .with_track(track.name.clone())
                    .with_step(tx_id.clone()),
                );
            }
        }
    }
}

/// tuxlink-lnctz: [`NO_TERMINAL_PATH`] and [`ARM_FALLTHROUGH_LEAK`] are
/// INDEPENDENTLY satisfiable, and a model that reads them as one problem
/// livelocks. Observed in Ladder-2 (`base/S4/rev_off`, `skill/E1/rev_on`):
/// the model adds an End to terminate the then-arm, which clears the leak
/// but not the terminal path (the End lands mid-track, so the track tail
/// still falls off); it reads the surviving finding as "that edit failed",
/// removes the End, the leak returns instructing the same edit, and the
/// cycle repeats until the turn budget is gone. Every call returns ok — no
/// rejection is involved, so only the finding TEXT can break it.
///
/// When both fire for one track, each says so and says what the other edit
/// will and will not do. Scoped to a single track's finding slice, so a
/// leak on track A never cross-references a terminal path on track B.
fn cross_reference_terminal_and_leak(track_findings: &mut [Finding]) {
    let has_leak = track_findings.iter().any(|f| f.code == ARM_FALLTHROUGH_LEAK);
    let has_no_terminal = track_findings.iter().any(|f| f.code == NO_TERMINAL_PATH);
    if !(has_leak && has_no_terminal) {
        return;
    }
    for f in track_findings.iter_mut() {
        if f.code == NO_TERMINAL_PATH {
            f.message.push_str(
                ". This track ALSO has an ARM_FALLTHROUGH_LEAK: they are SEPARATE problems \
                 needing SEPARATE End controls. An End that terminates a branch arm does not \
                 terminate this fall-through path",
            );
        } else if f.code == ARM_FALLTHROUGH_LEAK {
            f.message.push_str(
                ". This track ALSO has NO_TERMINAL_PATH: they are SEPARATE problems. The end \
                 control described above clears THIS finding only - NO_TERMINAL_PATH will \
                 correctly persist until the track's fall-through path gets its own End. Do NOT \
                 remove the end control you just added because NO_TERMINAL_PATH is still present",
            );
        }
    }
}

/// ARM_FALLTHROUGH_LEAK (tuxlink-ilrav, battery S1 post-6epl8-1 qwen
/// evidence): branch arms are jump targets with fall-through, so an arm
/// whose fall-through path reaches the OTHER arm's entry step runs that
/// arm's steps too. The observed real-world failure: then-arm success-log
/// placed directly before the else-arm's `radio.aprs_send` transmitted a
/// false "no gateway" alert on every SUCCESSFUL cycle, and validation said
/// nothing.
///
/// Warning, not error: exclusive-prefix-shared-tail ("then does extra
/// steps, then falls into the path both arms share") is only encodable in
/// exactly this shape, so an intentional convergence exists. The message
/// teaches both readings. The walk follows pure fall-through (actions,
/// delay/retry/call) and stops at any End (terminated) or Branch (a new
/// decision point, not a leak).
fn check_arm_fallthrough_leaks(def: &RoutineDef, track: &Track, findings: &mut Vec<Finding>) {
    let n = track.steps.len();
    for (i, step) in track.steps.iter().enumerate() {
        let Step::Control(c) = step else { continue };
        let Control::Branch { then, r#else, .. } = &c.control else {
            continue;
        };
        // An arm's entry index: its first target, or fall-through for an
        // empty arm. Dangling targets are BRANCH_TARGET_MISSING's job.
        let entry = |arm: &[StepId]| -> Option<usize> {
            match arm.first() {
                Some(t) => find_index(track, t),
                None => Some(i + 1),
            }
        };
        let (Some(then_entry), Some(else_entry)) = (entry(then), entry(r#else)) else {
            continue;
        };
        if then_entry == else_entry {
            continue; // both arms converge immediately: no exclusivity claimed
        }
        for (this_entry, other_entry, this_name, other_name) in [
            (then_entry, else_entry, "then", "else"),
            (else_entry, then_entry, "else", "then"),
        ] {
            let mut j = this_entry;
            let leaked = loop {
                if j == other_entry {
                    break true;
                }
                if j >= n {
                    break false; // ran off the track end (NO_TERMINAL_PATH's job)
                }
                match &track.steps[j] {
                    Step::Action(_) => j += 1,
                    Step::Control(c2) => match &c2.control {
                        Control::End { .. } => break false,
                        Control::Branch { .. } => break false,
                        // Retry executes its wrapped target before advancing
                        // (Codex 2026-07-22 P2): a retry on this arm's path
                        // whose target IS the other arm's entry runs that
                        // arm's first step - the same leak through a
                        // different door.
                        Control::Retry { step: target, .. } => {
                            if find_index(track, target) == Some(other_entry) {
                                break true;
                            }
                            j += 1;
                        }
                        _ => j += 1,
                    },
                }
            };
            if leaked {
                findings.push(
                    Finding::warning(
                        ARM_FALLTHROUGH_LEAK,
                        def.routine.clone(),
                        format!(
                            "branch \"{}\": the \"{}\" path falls through into \"{}\", the \"{}\" arm's first step - after the \"{}\" arm's steps run, execution CONTINUES into the \"{}\" arm's steps (arms are jump targets, not exclusive blocks). If the arms should be exclusive, insert an end control after the \"{}\" arm's steps; if the \"{}\" arm is deliberately a shared tail, ignore this",
                            track.steps[i].id().0,
                            this_name,
                            track.steps[other_entry].id().0,
                            other_name,
                            this_name,
                            other_name,
                            this_name,
                            other_name,
                        ),
                    )
                    .with_track(track.name.clone())
                    .with_step(track.steps[i].id().clone()),
                );
            }
        }
    }
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
            // tuxlink-lnctz: name the step execution actually falls off, and
            // the placement that fixes it. The bare "track can run past its
            // last step" text carried NO anchor, while the co-occurring
            // ARM_FALLTHROUGH_LEAK carried an imperative one ("insert an end
            // control after the then arm's steps"). A weak model followed the
            // only anchored instruction it had, landed the End mid-track, saw
            // this finding survive, reverted, and livelocked — 34 turns in
            // Ladder-2 base/S4/rev_off. An anchor of equal strength here is
            // the fix. No MCP tool is named: this crate stays tool-agnostic.
            let leavers: Vec<String> = track
                .steps
                .iter()
                .enumerate()
                .filter(|(i, _)| reachable.contains(i) && adj[*i].contains(&n))
                .map(|(_, s)| format!("\"{}\"", s.id().0))
                .collect();
            let message = if leavers.is_empty() {
                format!(
                    "track \"{}\" can run past its last step without hitting an explicit End",
                    track.name
                )
            } else {
                let named = leavers.join(", ");
                format!(
                    "track \"{}\" can run past its last step without hitting an explicit End - \
                     execution leaves the track after step {named}. Add an End control AFTER \
                     {named}, or make {named} an End. Only the step(s) named here fall off the \
                     end; an End placed elsewhere in the track does not clear this",
                    track.name
                )
            };
            findings.push(
                Finding::warning(NO_TERMINAL_PATH, def.routine.clone(), message)
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
                op: None,
                value: None,
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
            write_ack: None,
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

    // --- tuxlink-lnctz: anchors + cross-reference (Ladder-2 livelock) -----

    #[test]
    fn no_terminal_path_names_the_step_execution_falls_off_and_where_the_end_goes() {
        let def = routine_named("r1", vec![track("t1", vec![action("s1"), action("s2")])]);
        let ctx = StaticContext::new();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        let f = findings
            .iter()
            .find(|f| f.code == NO_TERMINAL_PATH)
            .expect("NO_TERMINAL_PATH");
        // The anchor is the whole point: without a named step the model has
        // nowhere to put the End and follows whatever other finding IS
        // anchored (that is the livelock).
        assert!(f.message.contains("\"s2\""), "must name the fall-off step: {}", f.message);
        assert!(f.message.contains("AFTER"), "must state placement: {}", f.message);
        assert!(!f.message.contains("\"s1\""), "s1 does not fall off: {}", f.message);
    }

    #[test]
    fn every_step_that_falls_off_the_end_is_named() {
        // Both arms run off the track end, so both are legitimate End sites.
        let def = routine_named(
            "r1",
            vec![track(
                "t1",
                vec![branch("b1", "x", vec!["a1"], vec!["a2"]), action("a1"), action("a2")],
            )],
        );
        let ctx = StaticContext::new();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        let f = findings
            .iter()
            .find(|f| f.code == NO_TERMINAL_PATH)
            .expect("NO_TERMINAL_PATH");
        assert!(f.message.contains("\"a2\""), "{}", f.message);
    }

    #[test]
    fn a_track_with_both_a_leak_and_no_terminal_path_cross_references_them() {
        // The Ladder-2 base/S4/rev_off shape: the then-arm (a1, a2) falls
        // through into a3, the else arm's entry (leak), and a3 also runs off
        // the track end (no terminal path). Both fire; the model must be told
        // they need SEPARATE Ends, or it removes the End it just added.
        let def = routine_named(
            "r1",
            vec![track(
                "t1",
                vec![
                    branch("b1", "x", vec!["a1"], vec!["a3"]),
                    action("a1"),
                    action("a2"),
                    action("a3"),
                ],
            )],
        );
        let ctx = StaticContext::new();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        let leak = findings
            .iter()
            .find(|f| f.code == ARM_FALLTHROUGH_LEAK)
            .expect("ARM_FALLTHROUGH_LEAK");
        let term = findings
            .iter()
            .find(|f| f.code == NO_TERMINAL_PATH)
            .expect("NO_TERMINAL_PATH");
        assert!(leak.message.contains(NO_TERMINAL_PATH), "leak must name it: {}", leak.message);
        assert!(
            leak.message.contains("Do NOT remove"),
            "leak must forbid the revert that closes the loop: {}",
            leak.message
        );
        assert!(
            term.message.contains(ARM_FALLTHROUGH_LEAK),
            "terminal must name it: {}",
            term.message
        );
    }

    #[test]
    fn a_terminal_path_finding_alone_is_not_cross_referenced() {
        let def = routine_named("r1", vec![track("t1", vec![action("s1")])]);
        let ctx = StaticContext::new();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        let f = findings
            .iter()
            .find(|f| f.code == NO_TERMINAL_PATH)
            .expect("NO_TERMINAL_PATH");
        assert!(!f.message.contains(ARM_FALLTHROUGH_LEAK), "{}", f.message);
    }

    #[test]
    fn the_cross_reference_never_spans_two_tracks() {
        // t1 leaks but terminates; t2 has no terminal path but no leak.
        // Neither may claim the other track's problem.
        let def = routine_named(
            "r1",
            vec![
                track(
                    "t1",
                    vec![
                        branch("b1", "x", vec!["a1"], vec!["a2"]),
                        action("a1"),
                        action("a2"),
                        end("e1"),
                    ],
                ),
                track("t2", vec![action("s1")]),
            ],
        );
        let ctx = StaticContext::new();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        let leak = findings
            .iter()
            .find(|f| f.code == ARM_FALLTHROUGH_LEAK)
            .expect("ARM_FALLTHROUGH_LEAK on t1");
        let term = findings
            .iter()
            .find(|f| f.code == NO_TERMINAL_PATH)
            .expect("NO_TERMINAL_PATH on t2");
        assert!(!leak.message.contains(NO_TERMINAL_PATH), "{}", leak.message);
        assert!(!term.message.contains(ARM_FALLTHROUGH_LEAK), "{}", term.message);
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

    // --- ARM_FALLTHROUGH_LEAK (tuxlink-ilrav) -------------------------

    fn leak_findings(def: &RoutineDef) -> Vec<Finding> {
        let ctx = StaticContext::new();
        let mut findings = Vec::new();
        check(def, &ctx, &mut findings);
        findings
            .into_iter()
            .filter(|f| f.code == ARM_FALLTHROUGH_LEAK)
            .collect()
    }

    /// The battery S1 post-6epl8-1 qwen def verbatim in shape: then-arm
    /// success step placed directly before the else-arm's entry. The
    /// success path falls into the failure steps (a false APRS transmit in
    /// the real emission); exactly one warning, naming the branch.
    #[test]
    fn qwen_shape_then_path_leaks_into_else_arm() {
        let def = routine_named(
            "r1",
            vec![track(
                "t1",
                vec![
                    action("s1"),
                    branch("b1", "s1.ok", vec!["s4"], vec!["s5"]),
                    action("s4"),
                    action("s5"),
                    action("s6"),
                    end("e1"),
                ],
            )],
        );
        let leaks = leak_findings(&def);
        assert_eq!(leaks.len(), 1, "{leaks:?}");
        assert_eq!(leaks[0].step, Some(StepId("b1".into())));
        assert!(
            leaks[0].message.contains("\"then\" path falls through into \"s5\""),
            "{}",
            leaks[0].message
        );
    }

    /// The correct exclusive layout (glm/gpt S1 emissions): then steps
    /// terminated by an end before the else steps begin; else runs off the
    /// track end. No leak in either direction.
    #[test]
    fn exclusive_arm_layout_has_no_leak() {
        let def = routine_named(
            "r1",
            vec![track(
                "t1",
                vec![
                    action("s1"),
                    branch("b1", "s1.ok", vec!["s8"], vec!["s5"]),
                    action("s8"),
                    end("e1"),
                    action("s5"),
                    action("s7"),
                    end("e2"),
                ],
            )],
        );
        assert!(leak_findings(&def).is_empty());
    }

    /// The leak detector is direction-symmetric: an else-arm entry whose
    /// fall-through reaches the then-arm's entry is the same defect.
    #[test]
    fn else_path_leaking_into_then_arm_is_detected() {
        let def = routine_named(
            "r1",
            vec![track(
                "t1",
                vec![
                    action("s1"),
                    branch("b1", "s1.ok", vec!["t1s"], vec!["e1s"]),
                    action("e1s"),
                    action("t1s"),
                    end("x1"),
                ],
            )],
        );
        let leaks = leak_findings(&def);
        assert_eq!(leaks.len(), 1, "{leaks:?}");
        assert!(
            leaks[0].message.contains("\"else\" path falls through into \"t1s\""),
            "{}",
            leaks[0].message
        );
    }

    /// A second branch between the arms is a new decision point, not a
    /// leak: the walk stops there.
    #[test]
    fn walk_stops_at_an_intervening_branch() {
        let def = routine_named(
            "r1",
            vec![track(
                "t1",
                vec![
                    action("s1"),
                    branch("b1", "s1.ok", vec!["t1s"], vec!["e1s"]),
                    action("t1s"),
                    branch("b2", "s1.ok", vec!["e1s"], vec!["e1s"]),
                    action("e1s"),
                    end("x1"),
                ],
            )],
        );
        assert!(leak_findings(&def).is_empty(), "{:?}", leak_findings(&def));
    }

    /// An empty arm falls through to the step after the branch; when the
    /// other arm targets a later step past an end, there is no leak, and
    /// converged arms (same entry) claim no exclusivity at all.
    #[test]
    fn empty_and_converged_arms_do_not_leak() {
        let def = routine_named(
            "r1",
            vec![track(
                "t1",
                vec![
                    action("s1"),
                    branch("b1", "s1.ok", vec![], vec!["s5"]),
                    action("s2"),
                    end("e1"),
                    action("s5"),
                    end("e2"),
                ],
            )],
        );
        assert!(leak_findings(&def).is_empty());

        let converged = routine_named(
            "r2",
            vec![track(
                "t1",
                vec![
                    action("s1"),
                    branch("b1", "s1.ok", vec!["s2"], vec!["s2"]),
                    action("s2"),
                    end("e1"),
                ],
            )],
        );
        assert!(leak_findings(&converged).is_empty());
    }

    /// Codex 2026-07-22 P2: a Retry on one arm's path whose target is the
    /// other arm's entry executes that arm's first step - the same leak
    /// through a different door. Shape verbatim from the review: then jumps
    /// to a retry wrapping the else-arm's entry action.
    #[test]
    fn retry_targeting_the_other_arms_entry_is_a_leak() {
        let def = routine_named(
            "r1",
            vec![track(
                "t1",
                vec![
                    action("s1"),
                    branch("b1", "s1.ok", vec!["r1"], vec!["else_tx"]),
                    action("else_tx"),
                    end("e_else"),
                    retry("r1", "else_tx", 1),
                    end("e_then"),
                ],
            )],
        );
        let leaks = leak_findings(&def);
        assert_eq!(leaks.len(), 1, "{leaks:?}");
        assert!(
            leaks[0].message.contains("\"then\" path falls through into \"else_tx\""),
            "{}",
            leaks[0].message
        );

        // A retry wrapping a target on the SAME arm's path is not a leak.
        let benign = routine_named(
            "r2",
            vec![track(
                "t1",
                vec![
                    action("s1"),
                    branch("b1", "s1.ok", vec!["t1s"], vec!["e1s"]),
                    action("t1s"),
                    retry("r1", "t1s", 1),
                    end("e_then"),
                    action("e1s"),
                    end("e_else"),
                ],
            )],
        );
        assert!(leak_findings(&benign).is_empty(), "{:?}", leak_findings(&benign));
    }

    // --- BRANCH_OP_VALUE_PAIR / BRANCH_BOTH_ARMS_EMPTY (tuxlink-rrk51) ---

    fn cmp_branch(
        id: &str,
        on: &str,
        op: Option<crate::types::CmpOp>,
        value: Option<serde_json::Value>,
        then: Vec<&str>,
        r#else: Vec<&str>,
    ) -> Step {
        Step::Control(ControlStep {
            id: StepId(id.into()),
            control: Control::Branch {
                on: on.into(),
                op,
                value,
                then: then.into_iter().map(|s| StepId(s.into())).collect(),
                r#else: r#else.into_iter().map(|s| StepId(s.into())).collect(),
            },
        })
    }

    #[test]
    fn op_without_value_is_an_error_and_value_without_op_too() {
        use crate::types::CmpOp;
        let def = routine_named(
            "r1",
            vec![track(
                "t1",
                vec![
                    action("s1"),
                    cmp_branch("b1", "s1.k", Some(CmpOp::Gte), None, vec!["e1"], vec![]),
                    end("e1"),
                ],
            )],
        );
        let ctx = StaticContext::new();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        let hits: Vec<_> = findings
            .iter()
            .filter(|f| f.code == BRANCH_OP_VALUE_PAIR)
            .collect();
        assert_eq!(hits.len(), 1, "{findings:?}");
        assert_eq!(hits[0].severity, crate::validate::Severity::Error);
        assert!(hits[0].message.contains("op without value"), "{}", hits[0].message);

        let def2 = routine_named(
            "r2",
            vec![track(
                "t1",
                vec![
                    action("s1"),
                    cmp_branch("b1", "s1.k", None, Some(json!(4)), vec!["e1"], vec![]),
                    end("e1"),
                ],
            )],
        );
        let mut findings2 = Vec::new();
        check(&def2, &ctx, &mut findings2);
        assert!(
            findings2
                .iter()
                .any(|f| f.code == BRANCH_OP_VALUE_PAIR && f.message.contains("value without op")),
            "{findings2:?}"
        );
    }

    #[test]
    fn paired_op_value_and_strict_boolean_forms_are_clean() {
        use crate::types::CmpOp;
        let def = routine_named(
            "r1",
            vec![track(
                "t1",
                vec![
                    action("s1"),
                    cmp_branch("b1", "s1.k", Some(CmpOp::Gte), Some(json!(4)), vec!["e1"], vec![]),
                    branch("b2", "s1.ok", vec!["e1"], vec![]),
                    end("e1"),
                ],
            )],
        );
        let ctx = StaticContext::new();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().all(|f| f.code != BRANCH_OP_VALUE_PAIR), "{findings:?}");
    }

    #[test]
    fn a_branch_with_both_arms_empty_warns_it_decides_nothing() {
        let def = routine_named(
            "r1",
            vec![track(
                "t1",
                vec![action("s1"), branch("b1", "s1.ok", vec![], vec![]), end("e1")],
            )],
        );
        let ctx = StaticContext::new();
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        let hits: Vec<_> = findings
            .iter()
            .filter(|f| f.code == BRANCH_BOTH_ARMS_EMPTY)
            .collect();
        assert_eq!(hits.len(), 1, "{findings:?}");
        assert_eq!(hits[0].severity, crate::validate::Severity::Warning);
        assert_eq!(hits[0].step, Some(StepId("b1".into())));
        // One non-empty arm is a legitimate fall-through shape: no warning.
        let def2 = routine_named(
            "r2",
            vec![track(
                "t1",
                vec![action("s1"), branch("b1", "s1.ok", vec!["e1"], vec![]), end("e1")],
            )],
        );
        let mut findings2 = Vec::new();
        check(&def2, &ctx, &mut findings2);
        assert!(findings2.iter().all(|f| f.code != BRANCH_BOTH_ARMS_EMPTY), "{findings2:?}");
    }

    // --- TX_ONLY_ON_FAILURE_ARM (tuxlink-rrk51) -----------------------

    fn tx_descriptor(name: &'static str) -> crate::action::ActionDescriptor {
        crate::action::ActionDescriptor {
            name,
            label: "",
            description: "",
            needs_radio: true,
            transmits: true,
            writes_config: false,
            needs_internet: false,
            example_params: None,
            allowed_values: None,
            params: &[],
            outputs: &[],
            dry_run_shape: None,
        }
    }

    fn tx_action(id: &str, name: &str) -> Step {
        Step::Action(ActionStep {
            id: StepId(id.into()),
            action: name.into(),
            params: json!({}),
            timeout_s: None,
            on_radio_busy: BusyPolicy::Wait,
        })
    }

    #[test]
    fn a_transmit_step_reachable_only_via_the_else_arm_of_a_connected_branch_warns() {
        // s1 connect -> b1 on s1.connected: then -> log/end, else -> aprs/end.
        // The aprs "confirmation" runs only on failure — the lnctz S4 shape.
        let def = routine_named(
            "r1",
            vec![track(
                "t1",
                vec![
                    tx_action("s1", "radio.connect"),
                    branch("b1", "s1.connected", vec!["ok"], vec!["tx"]),
                    action("ok"),
                    end("e_ok"),
                    tx_action("tx", "radio.aprs_send"),
                    end("e_tx"),
                ],
            )],
        );
        let ctx = StaticContext::new()
            .with_action(tx_descriptor("radio.connect"))
            .with_action(tx_descriptor("radio.aprs_send"))
            .with_flushes_outbox("radio.connect");
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        let hits: Vec<_> = findings
            .iter()
            .filter(|f| f.code == TX_ONLY_ON_FAILURE_ARM)
            .collect();
        assert_eq!(hits.len(), 1, "{findings:?}");
        assert_eq!(hits[0].step, Some(StepId("tx".into())));
        assert!(hits[0].message.contains("failure alert"), "{}", hits[0].message);
    }

    #[test]
    fn a_connected_branch_over_a_status_read_is_not_a_failed_connect() {
        // Codex 2026-07-27 P2 regression: s1 is a modem-status READ exposing
        // `connected`; dialing in its else arm (modem idle) is exactly
        // right, so no warning may fire — the producer is not a connect.
        let def = routine_named(
            "r1",
            vec![track(
                "t1",
                vec![
                    action("s1"), // a status read, NOT classified as flushing
                    branch("b1", "s1.connected", vec!["done"], vec!["tx"]),
                    action("done"),
                    end("e_done"),
                    tx_action("tx", "radio.connect"),
                    end("e_tx"),
                ],
            )],
        );
        let ctx = StaticContext::new()
            .with_action(tx_descriptor("radio.connect"))
            .with_flushes_outbox("radio.connect");
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(
            findings.iter().all(|f| f.code != TX_ONLY_ON_FAILURE_ARM),
            "{findings:?}"
        );
    }

    #[test]
    fn a_transmit_step_on_the_success_arm_or_shared_tail_does_not_warn() {
        // Success-arm confirmation: the correct wiring stays silent.
        let def = routine_named(
            "r1",
            vec![track(
                "t1",
                vec![
                    tx_action("s1", "radio.connect"),
                    branch("b1", "s1.connected", vec!["tx"], vec!["fail"]),
                    tx_action("tx", "radio.aprs_send"),
                    end("e_ok"),
                    action("fail"),
                    end("e_fail"),
                ],
            )],
        );
        let ctx = StaticContext::new()
            .with_action(tx_descriptor("radio.connect"))
            .with_action(tx_descriptor("radio.aprs_send"))
            .with_flushes_outbox("radio.connect");
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().all(|f| f.code != TX_ONLY_ON_FAILURE_ARM), "{findings:?}");

        // A threshold (op) branch is out of scope even with a TX in its else.
        use crate::types::CmpOp;
        let def2 = routine_named(
            "r2",
            vec![track(
                "t1",
                vec![
                    action("s0"),
                    cmp_branch(
                        "b1",
                        "s0.k_index",
                        Some(CmpOp::Gte),
                        Some(json!(4)),
                        vec!["skip"],
                        vec!["tx"],
                    ),
                    action("skip"),
                    end("e_skip"),
                    tx_action("tx", "radio.connect"),
                    end("e_tx"),
                ],
            )],
        );
        let ctx2 = StaticContext::new().with_action(tx_descriptor("radio.connect"));
        let mut findings2 = Vec::new();
        check(&def2, &ctx2, &mut findings2);
        assert!(
            findings2.iter().all(|f| f.code != TX_ONLY_ON_FAILURE_ARM),
            "{findings2:?}"
        );
    }
}
