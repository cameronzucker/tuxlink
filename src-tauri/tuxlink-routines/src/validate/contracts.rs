//! Variable/type contracts (spec §10 layer 1, plan-3 task 3): every `$var`
//! param and every branch `on` path must resolve to something that will
//! actually exist by the time the step runs.
//!
//! **v1 rule is lexical, not full path analysis.** A `$step.output` /
//! branch-`on` reference is satisfiable only when `step` names a step id
//! that appears EARLIER in the authored `tracks[].steps` array of the SAME
//! track as the referencing step — array position, not runtime jump order.
//! (Runtime jump order is a separate, later concern: `structure.rs`'s
//! reachability layer. A step that is lexically earlier but never actually
//! executes on a given run — e.g. skipped by a branch — still counts as
//! "earlier" here; full per-path liveness analysis is out of scope for
//! v1.) A bare (dot-less) reference is satisfiable only if it names a
//! declared routine input (`RoutineDef.inputs`).
//!
//! Three outcomes for a reference that fails the lexical-order rule:
//!
//! - The referenced step id does not exist earlier in the SAME track (it
//!   exists later in the same track, or does not exist at all) ->
//!   [`UNSATISFIABLE_VAR`] (Error).
//! - The referenced step id exists, but only in a DIFFERENT track ->
//!   [`CROSS_TRACK_VAR`] (Warning, not Error). This is runtime-legal:
//!   `executor.rs` shares one `RunVars` store across every concurrently-run
//!   track (`run_tracks`), so track B genuinely can read track A's step
//!   output once track A has actually produced it — the spec's "+5 min
//!   re-dial the last-heard gateway" pattern (see
//!   `executor.rs::tests::parallel_tracks_share_vars_and_join`, which reads
//!   `$s1.gateway` from track "a" inside track "b"). Flagging that pattern
//!   as an Error would reject a supported, load-bearing authoring shape.
//!   It still earns a Warning: nothing here statically proves the
//!   producing track runs first, so the read is timing-dependent.
//! - A bare (dot-less) reference that names neither a step-path shape nor
//!   a declared input. For a branch `on` field this gets its own code,
//!   [`BRANCH_ON_UNKNOWN`] (Error) — a branch condition that cannot even
//!   parse as "a thing with a value" is a distinct authoring mistake from
//!   an ordering violation, and deserves a message that says so. For a
//!   `$param` or `Control::Call` arg the same shape folds into
//!   [`UNSATISFIABLE_VAR`]: there is no reader-facing benefit to a second
//!   code for "unresolvable" vs. "unresolvable and also badly shaped."

use std::collections::{HashMap, HashSet};

use crate::refs::VarPath;
use crate::types::{Control, RoutineDef, Step, StepId};

use super::findings::Finding;

pub const UNSATISFIABLE_VAR: &str = "UNSATISFIABLE_VAR";
pub const BRANCH_ON_UNKNOWN: &str = "BRANCH_ON_UNKNOWN";
pub const CROSS_TRACK_VAR: &str = "CROSS_TRACK_VAR";

#[derive(Debug, Clone, Copy)]
struct StepLocation {
    track_idx: usize,
    step_idx: usize,
}

enum VarStatus {
    /// Earlier in the same track, or a declared input.
    Satisfied,
    /// Same track, but not earlier (later, or the id doesn't exist at all).
    UnsatisfiableOrder,
    /// Exists, but only in a different track — runtime-legal, timing-dependent.
    CrossTrack,
    /// Dot-less and not a declared input.
    UnknownBare,
}

/// Per-track context shared by every reference check in one track, so the
/// per-call signatures stay short (clippy's `too_many_arguments`).
struct TrackCtx<'a> {
    def: &'a RoutineDef,
    locations: &'a HashMap<StepId, StepLocation>,
    input_names: &'a HashSet<&'a str>,
    track_idx: usize,
    track_name: &'a str,
}

fn locate_steps(def: &RoutineDef) -> HashMap<StepId, StepLocation> {
    let mut out = HashMap::new();
    for (track_idx, track) in def.tracks.iter().enumerate() {
        for (step_idx, step) in track.steps.iter().enumerate() {
            out.insert(
                step.id().clone(),
                StepLocation {
                    track_idx,
                    step_idx,
                },
            );
        }
    }
    out
}

fn classify(raw: &str, ctx: &TrackCtx, step_idx: usize) -> VarStatus {
    if let Some(vp) = VarPath::parse(raw) {
        match ctx.locations.get(&vp.step) {
            Some(loc) if loc.track_idx == ctx.track_idx && loc.step_idx < step_idx => {
                VarStatus::Satisfied
            }
            Some(loc) if loc.track_idx == ctx.track_idx => VarStatus::UnsatisfiableOrder,
            Some(_) => VarStatus::CrossTrack,
            None => VarStatus::UnsatisfiableOrder,
        }
    } else if ctx.input_names.contains(raw) {
        VarStatus::Satisfied
    } else {
        VarStatus::UnknownBare
    }
}

/// Append every `UNSATISFIABLE_VAR` / `BRANCH_ON_UNKNOWN` / `CROSS_TRACK_VAR`
/// finding for `def` into `findings`. Pure over `def` — no `ValidationContext`
/// needed; the v1 rule never looks outside the routine being checked.
pub fn check(def: &RoutineDef, findings: &mut Vec<Finding>) {
    let locations = locate_steps(def);
    let input_names: HashSet<&str> = def.inputs.iter().map(|i| i.name.as_str()).collect();

    for (track_idx, track) in def.tracks.iter().enumerate() {
        let ctx = TrackCtx {
            def,
            locations: &locations,
            input_names: &input_names,
            track_idx,
            track_name: &track.name,
        };
        for (step_idx, step) in track.steps.iter().enumerate() {
            match step {
                Step::Action(a) => check_dollar_refs(&a.params, &ctx, step_idx, &a.id.0, findings),
                Step::Control(c) => match &c.control {
                    Control::Branch { on, .. } => {
                        check_branch_on(on, &ctx, step_idx, &c.id.0, findings)
                    }
                    Control::Call { args, .. } => {
                        check_dollar_refs(args, &ctx, step_idx, &c.id.0, findings)
                    }
                    Control::Retry { .. } | Control::Delay { .. } | Control::End { .. } => {}
                },
            }
        }
    }
}

fn collect_dollar_tokens(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(s) if s.starts_with('$') => out.push(s.clone()),
        serde_json::Value::Array(items) => items.iter().for_each(|v| collect_dollar_tokens(v, out)),
        serde_json::Value::Object(map) => map.values().for_each(|v| collect_dollar_tokens(v, out)),
        _ => {}
    }
}

fn check_dollar_refs(
    value: &serde_json::Value,
    ctx: &TrackCtx,
    step_idx: usize,
    step_id: &str,
    findings: &mut Vec<Finding>,
) {
    let mut tokens = Vec::new();
    collect_dollar_tokens(value, &mut tokens);
    for token in tokens {
        let raw = &token[1..]; // strip the leading '$'
        match classify(raw, ctx, step_idx) {
            VarStatus::Satisfied => {}
            VarStatus::CrossTrack => findings.push(
                Finding::warning(
                    CROSS_TRACK_VAR,
                    ctx.def.routine.clone(),
                    format!(
                        "step \"{step_id}\" references \"{token}\", which a step in a different \
                         track produces — only safe once that track has actually run"
                    ),
                )
                .with_track(ctx.track_name.to_string())
                .with_step(StepId(step_id.to_string())),
            ),
            VarStatus::UnsatisfiableOrder | VarStatus::UnknownBare => findings.push(
                Finding::error(
                    UNSATISFIABLE_VAR,
                    ctx.def.routine.clone(),
                    format!(
                        "step \"{step_id}\" references \"{token}\", which is not a step earlier \
                         in track \"{}\" and not a declared input",
                        ctx.track_name
                    ),
                )
                .with_track(ctx.track_name.to_string())
                .with_step(StepId(step_id.to_string())),
            ),
        }
    }
}

fn check_branch_on(
    on: &str,
    ctx: &TrackCtx,
    step_idx: usize,
    step_id: &str,
    findings: &mut Vec<Finding>,
) {
    match classify(on, ctx, step_idx) {
        VarStatus::Satisfied => {}
        VarStatus::CrossTrack => findings.push(
            Finding::warning(
                CROSS_TRACK_VAR,
                ctx.def.routine.clone(),
                format!(
                    "branch step \"{step_id}\" tests \"{on}\", which a step in a different track \
                     produces — only safe once that track has actually run"
                ),
            )
            .with_track(ctx.track_name.to_string())
            .with_step(StepId(step_id.to_string())),
        ),
        VarStatus::UnsatisfiableOrder => findings.push(
            Finding::error(
                UNSATISFIABLE_VAR,
                ctx.def.routine.clone(),
                format!(
                    "branch step \"{step_id}\" tests \"{on}\", which is not a step earlier in \
                     track \"{}\"",
                    ctx.track_name
                ),
            )
            .with_track(ctx.track_name.to_string())
            .with_step(StepId(step_id.to_string())),
        ),
        VarStatus::UnknownBare => findings.push(
            Finding::error(
                BRANCH_ON_UNKNOWN,
                ctx.def.routine.clone(),
                format!(
                    "branch step \"{step_id}\" condition \"{on}\" is neither a step output path \
                     nor a declared input"
                ),
            )
            .with_track(ctx.track_name.to_string())
            .with_step(StepId(step_id.to_string())),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        ActionStep, BusyPolicy, ControlStep, InputDecl, OnInterrupted, RoutineDef, Track,
        TransmitMode, Trigger,
    };
    use serde_json::json;

    fn action(id: &str, params: serde_json::Value) -> Step {
        Step::Action(ActionStep {
            id: StepId(id.into()),
            action: "local.note".into(),
            params,
            timeout_s: None,
            on_radio_busy: BusyPolicy::Wait,
        })
    }

    fn branch(id: &str, on: &str) -> Step {
        Step::Control(ControlStep {
            id: StepId(id.into()),
            control: Control::Branch {
                on: on.into(),
                then: vec![],
                r#else: vec![],
            },
        })
    }

    fn call(id: &str, routine: &str, args: serde_json::Value) -> Step {
        Step::Control(ControlStep {
            id: StepId(id.into()),
            control: Control::Call {
                routine: routine.into(),
                args,
                sync: true,
            },
        })
    }

    fn routine_with(inputs: Vec<InputDecl>, tracks: Vec<Track>) -> RoutineDef {
        RoutineDef {
            routine: "r1".into(),
            schema_version: crate::types::SUPPORTED_SCHEMA_VERSION,
            transmit_mode: TransmitMode::Attended,
            transmit_ack: None,
            on_interrupted: OnInterrupted::Stay,
            inputs,
            triggers: vec![Trigger::Manual],
            tracks,
        }
    }

    // --- UNSATISFIABLE_VAR ---------------------------------------------

    #[test]
    fn dollar_param_referencing_a_later_same_track_step_is_unsatisfiable() {
        let def = routine_with(
            vec![],
            vec![Track {
                name: "t1".into(),
                steps: vec![
                    action("s1", json!({"x": "$s2.out"})),
                    action("s2", json!({})),
                ],
            }],
        );
        let mut findings = Vec::new();
        check(&def, &mut findings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, UNSATISFIABLE_VAR);
        assert_eq!(findings[0].step, Some(StepId("s1".into())));
        assert!(findings[0].message.contains("$s2.out"), "{:?}", findings);
    }

    #[test]
    fn dollar_param_referencing_a_nonexistent_step_is_unsatisfiable() {
        let def = routine_with(
            vec![],
            vec![Track {
                name: "t1".into(),
                steps: vec![action("s1", json!({"x": "$nope.out"}))],
            }],
        );
        let mut findings = Vec::new();
        check(&def, &mut findings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, UNSATISFIABLE_VAR);
    }

    #[test]
    fn dollar_param_referencing_an_earlier_same_track_step_is_satisfied() {
        let def = routine_with(
            vec![],
            vec![Track {
                name: "t1".into(),
                steps: vec![
                    action("s1", json!({})),
                    action("s2", json!({"x": "$s1.out"})),
                ],
            }],
        );
        let mut findings = Vec::new();
        check(&def, &mut findings);
        assert!(findings.is_empty(), "{:?}", findings);
    }

    #[test]
    fn dollar_param_referencing_a_declared_input_is_satisfied() {
        let def = routine_with(
            vec![InputDecl {
                name: "band_plan".into(),
                required: true,
            }],
            vec![Track {
                name: "t1".into(),
                steps: vec![action("s1", json!({"x": "$band_plan"}))],
            }],
        );
        let mut findings = Vec::new();
        check(&def, &mut findings);
        assert!(findings.is_empty(), "{:?}", findings);
    }

    #[test]
    fn dollar_param_referencing_an_undeclared_bare_name_is_unsatisfiable_not_branch_on_unknown() {
        let def = routine_with(
            vec![],
            vec![Track {
                name: "t1".into(),
                steps: vec![action("s1", json!({"x": "$garbage"}))],
            }],
        );
        let mut findings = Vec::new();
        check(&def, &mut findings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, UNSATISFIABLE_VAR);
    }

    #[test]
    fn call_args_are_checked_the_same_as_action_params() {
        let def = routine_with(
            vec![],
            vec![Track {
                name: "t1".into(),
                steps: vec![call("c1", "callee", json!({"x": "$s9.out"}))],
            }],
        );
        let mut findings = Vec::new();
        check(&def, &mut findings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, UNSATISFIABLE_VAR);
        assert_eq!(findings[0].step, Some(StepId("c1".into())));
    }

    // --- BRANCH_ON_UNKNOWN ------------------------------------------------

    #[test]
    fn branch_on_a_bare_undeclared_name_is_branch_on_unknown() {
        let def = routine_with(
            vec![],
            vec![Track {
                name: "t1".into(),
                steps: vec![branch("b1", "garbage")],
            }],
        );
        let mut findings = Vec::new();
        check(&def, &mut findings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, BRANCH_ON_UNKNOWN);
        assert_eq!(findings[0].step, Some(StepId("b1".into())));
        assert!(findings[0].message.contains("garbage"), "{:?}", findings);
    }

    #[test]
    fn branch_on_a_declared_input_is_satisfied() {
        let def = routine_with(
            vec![InputDecl {
                name: "go".into(),
                required: false,
            }],
            vec![Track {
                name: "t1".into(),
                steps: vec![branch("b1", "go")],
            }],
        );
        let mut findings = Vec::new();
        check(&def, &mut findings);
        assert!(findings.is_empty(), "{:?}", findings);
    }

    #[test]
    fn branch_on_a_later_same_track_step_is_unsatisfiable_var_not_branch_on_unknown() {
        let def = routine_with(
            vec![],
            vec![Track {
                name: "t1".into(),
                steps: vec![branch("b1", "s2.connected"), action("s2", json!({}))],
            }],
        );
        let mut findings = Vec::new();
        check(&def, &mut findings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, UNSATISFIABLE_VAR);
        assert_eq!(findings[0].step, Some(StepId("b1".into())));
    }

    #[test]
    fn branch_on_an_earlier_same_track_step_is_satisfied() {
        let def = routine_with(
            vec![],
            vec![Track {
                name: "t1".into(),
                steps: vec![action("s1", json!({})), branch("b1", "s1.connected")],
            }],
        );
        let mut findings = Vec::new();
        check(&def, &mut findings);
        assert!(findings.is_empty(), "{:?}", findings);
    }

    // --- CROSS_TRACK_VAR ----------------------------------------------

    #[test]
    fn dollar_param_referencing_a_different_track_step_is_a_cross_track_warning() {
        let def = routine_with(
            vec![],
            vec![
                Track {
                    name: "a".into(),
                    steps: vec![action("s1", json!({}))],
                },
                Track {
                    name: "b".into(),
                    steps: vec![action("s2", json!({"station": "$s1.gateway"}))],
                },
            ],
        );
        let mut findings = Vec::new();
        check(&def, &mut findings);
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.code, CROSS_TRACK_VAR);
        assert_eq!(f.severity, super::super::Severity::Warning);
        assert_eq!(f.track, Some("b".to_string()));
        assert_eq!(f.step, Some(StepId("s2".into())));
    }

    #[test]
    fn branch_on_a_different_track_step_is_a_cross_track_warning() {
        let def = routine_with(
            vec![],
            vec![
                Track {
                    name: "a".into(),
                    steps: vec![action("s1", json!({}))],
                },
                Track {
                    name: "b".into(),
                    steps: vec![branch("b1", "s1.connected")],
                },
            ],
        );
        let mut findings = Vec::new();
        check(&def, &mut findings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, CROSS_TRACK_VAR);
        assert_eq!(findings[0].severity, super::super::Severity::Warning);
    }

    #[test]
    fn ref_free_routine_produces_no_findings() {
        let def = routine_with(
            vec![],
            vec![Track {
                name: "t1".into(),
                steps: vec![action("s1", json!({"plain": "value"}))],
            }],
        );
        let mut findings = Vec::new();
        check(&def, &mut findings);
        assert!(findings.is_empty());
    }
}
