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
use crate::types::{ActionStep, Control, RoutineDef, Step, StepId};

use super::context::ValidationContext;
use super::findings::Finding;

pub const UNSATISFIABLE_VAR: &str = "UNSATISFIABLE_VAR";
pub const BRANCH_ON_UNKNOWN: &str = "BRANCH_ON_UNKNOWN";
pub const CROSS_TRACK_VAR: &str = "CROSS_TRACK_VAR";
/// A LITERAL (non-`$ref`) value for a descriptor's closed-vocabulary param
/// (today only `data.read`'s `source`) that is outside the allowed set — the
/// author typed a source name that will never match at run time. Error, since
/// the step is guaranteed to fail. A `$ref` value is chosen at run time and is
/// NOT flagged (the validator cannot know its resolved value).
pub const UNKNOWN_READ_SOURCE: &str = "UNKNOWN_READ_SOURCE";

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
/// finding for `def` into `findings`. The var/branch-order rules are pure over
/// `def`; the `UNKNOWN_READ_SOURCE` lint (D6) additionally consults `vctx` for
/// each action's descriptor `allowed_values`, so `check` now takes the
/// `ValidationContext` the rest of the validator already threads.
pub fn check(def: &RoutineDef, vctx: &dyn ValidationContext, findings: &mut Vec<Finding>) {
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
                Step::Action(a) => {
                    check_dollar_refs(&a.params, &ctx, step_idx, &a.id.0, findings);
                    check_allowed_values(a, vctx, &ctx, findings);
                }
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

/// How a `$` token sits in its string, which decides its failure class:
/// a WHOLE-value ref is a hard runtime error when unresolvable (Error /
/// Warning findings), while an EMBEDDED token interpolates and stays
/// VERBATIM text when unresolvable (Warning findings only). The routing
/// test is [`crate::refs::whole_ref`] - the exact one
/// `executor::resolve_string` uses, so validation cannot disagree with the
/// runtime (Codex 2026-07-22 P2 x2: "$ref plus trailing text" was refused
/// with whole-ref errors, and embedded forward references sailed through
/// with no order check at all).
enum TokenKind {
    Whole,
    Embedded,
}

fn collect_dollar_tokens(value: &serde_json::Value, out: &mut Vec<(String, TokenKind)>) {
    match value {
        serde_json::Value::String(s) => {
            if crate::refs::whole_ref(s) {
                out.push((s.clone(), TokenKind::Whole));
            } else {
                // Interpolation position: every DOTTED scannable token is a
                // step-output read at runtime. Dot-less tokens ("$50", a
                // bare input name inside text) are skipped - the dollar
                // amount stays verbatim and warning on it would be noise.
                for (_, path) in crate::refs::scan_embedded_refs(s) {
                    if path.contains('.') {
                        out.push((format!("${path}"), TokenKind::Embedded));
                    }
                }
            }
        }
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
    for (token, kind) in tokens {
        let raw = &token[1..]; // strip the leading '$'
        match kind {
            TokenKind::Whole => match classify(raw, ctx, step_idx) {
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
            },
            TokenKind::Embedded => {
                // Same lexical-order rule as whole refs, downgraded to the
                // embedded-ref warning class: an unresolvable embedded token
                // stays literal text at runtime (never a run failure), but
                // the author almost certainly expected a value. Nonexistent
                // step ids are params.rs's embedded check (declared
                // surfaces) - only order and track placement are ours.
                let Some(vp) = VarPath::parse(raw) else {
                    continue;
                };
                match ctx.locations.get(&vp.step) {
                    None => {}
                    Some(loc) if loc.track_idx == ctx.track_idx && loc.step_idx < step_idx => {}
                    Some(loc) if loc.track_idx == ctx.track_idx => findings.push(
                        Finding::warning(
                            super::params::EMBEDDED_REF_IGNORED,
                            ctx.def.routine.clone(),
                            format!(
                                "step \"{step_id}\" embeds \"{token}\", but step \"{}\" only \
                                 produces it LATER in track \"{}\" - an embedded ref that cannot \
                                 resolve stays literal text at runtime",
                                vp.step.0, ctx.track_name
                            ),
                        )
                        .with_track(ctx.track_name.to_string())
                        .with_step(StepId(step_id.to_string())),
                    ),
                    Some(_) => findings.push(
                        Finding::warning(
                            super::params::EMBEDDED_REF_IGNORED,
                            ctx.def.routine.clone(),
                            format!(
                                "step \"{step_id}\" embeds \"{token}\", which a step in a \
                                 different track produces - it interpolates only once that track \
                                 has run; until then it stays literal text"
                            ),
                        )
                        .with_track(ctx.track_name.to_string())
                        .with_step(StepId(step_id.to_string())),
                    ),
                }
            }
        }
    }
}

/// D6 `UNKNOWN_READ_SOURCE`: if this action's descriptor declares a closed
/// vocabulary (`allowed_values`) for one param, and the step supplies a LITERAL
/// string for that param outside the set, it can never match at run time. A
/// `$ref` value is resolved at run time (unknowable statically) and is skipped;
/// an unknown action (no descriptor) is another check's finding, not ours.
fn check_allowed_values(
    action: &ActionStep,
    vctx: &dyn ValidationContext,
    ctx: &TrackCtx,
    findings: &mut Vec<Finding>,
) {
    let Some(desc) = vctx.action_descriptor(&action.action) else {
        return;
    };
    let Some((key, allowed)) = desc.allowed_values else {
        return;
    };
    let Some(value) = action.params.get(key) else {
        return;
    };
    let Some(literal) = value.as_str() else {
        return; // non-string (e.g. object/number) — not this lint's concern
    };
    if literal.starts_with('$') {
        return; // runtime ref: value chosen when the run starts
    }
    if !allowed.contains(&literal) {
        findings.push(
            Finding::error(
                UNKNOWN_READ_SOURCE,
                ctx.def.routine.clone(),
                format!(
                    "step \"{}\" sets \"{key}\" to \"{literal}\", which is not a valid {key} for \
                     action \"{}\"",
                    action.id.0, action.action
                ),
            )
            .with_track(ctx.track_name.to_string())
            .with_step(action.id.clone()),
        );
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
    use crate::action::ActionDescriptor;
    use crate::validate::context::StaticContext;
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
                op: None,
                value: None,
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
            write_ack: None,
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
        check(&def, &StaticContext::new(), &mut findings);
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
        check(&def, &StaticContext::new(), &mut findings);
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
        check(&def, &StaticContext::new(), &mut findings);
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
        check(&def, &StaticContext::new(), &mut findings);
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
        check(&def, &StaticContext::new(), &mut findings);
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
        check(&def, &StaticContext::new(), &mut findings);
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
        check(&def, &StaticContext::new(), &mut findings);
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
        check(&def, &StaticContext::new(), &mut findings);
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
        check(&def, &StaticContext::new(), &mut findings);
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
        check(&def, &StaticContext::new(), &mut findings);
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
        check(&def, &StaticContext::new(), &mut findings);
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
        check(&def, &StaticContext::new(), &mut findings);
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
        check(&def, &StaticContext::new(), &mut findings);
        assert!(findings.is_empty());
    }

    // --- UNKNOWN_READ_SOURCE (D6) --------------------------------------

    const READ_SOURCES: &[&str] = &["grid", "modem_status", "config"];
    const DATA_READ_DESC: ActionDescriptor = ActionDescriptor {
        name: "data.read",
        label: "",
        description: "",
        needs_radio: false,
        transmits: false,
        writes_config: false,
        needs_internet: false,
        example_params: None,
        allowed_values: Some(("source", READ_SOURCES)),
        params: &[],
        outputs: &[],
        dry_run_shape: None,
    };

    fn read_step(id: &str, source: serde_json::Value) -> Step {
        Step::Action(ActionStep {
            id: StepId(id.into()),
            action: "data.read".into(),
            params: json!({ "source": source }),
            timeout_s: None,
            on_radio_busy: BusyPolicy::Wait,
        })
    }

    fn read_ctx() -> StaticContext {
        StaticContext::new().with_action(DATA_READ_DESC)
    }

    #[test]
    fn literal_unknown_read_source_fires_unknown_read_source() {
        let def = routine_with(
            vec![],
            vec![Track {
                name: "t1".into(),
                steps: vec![read_step("s1", json!("sorce"))],
            }],
        );
        let mut findings = Vec::new();
        check(&def, &read_ctx(), &mut findings);
        assert_eq!(findings.len(), 1, "{:?}", findings);
        assert_eq!(findings[0].code, UNKNOWN_READ_SOURCE);
        assert_eq!(findings[0].step, Some(StepId("s1".into())));
        assert!(findings[0].message.contains("sorce"), "{:?}", findings);
    }

    #[test]
    fn kebab_read_source_variant_is_unknown() {
        // The real vocabulary is snake_case; a kebab `modem-status` never matches.
        let def = routine_with(
            vec![],
            vec![Track {
                name: "t1".into(),
                steps: vec![read_step("s1", json!("modem-status"))],
            }],
        );
        let mut findings = Vec::new();
        check(&def, &read_ctx(), &mut findings);
        assert_eq!(findings.len(), 1, "{:?}", findings);
        assert_eq!(findings[0].code, UNKNOWN_READ_SOURCE);
    }

    #[test]
    fn known_read_source_is_silent() {
        let def = routine_with(
            vec![],
            vec![Track {
                name: "t1".into(),
                steps: vec![read_step("s1", json!("modem_status"))],
            }],
        );
        let mut findings = Vec::new();
        check(&def, &read_ctx(), &mut findings);
        assert!(findings.is_empty(), "{:?}", findings);
    }

    #[test]
    fn ref_read_source_is_not_flagged() {
        // A `$ref` source is chosen at run time — the validator cannot know its
        // value, so it MUST NOT fire UNKNOWN_READ_SOURCE. `$src` names a
        // declared input, so it is also var-satisfiable (no other finding).
        let def = routine_with(
            vec![InputDecl {
                name: "src".into(),
                required: true,
            }],
            vec![Track {
                name: "t1".into(),
                steps: vec![read_step("s1", json!("$src"))],
            }],
        );
        let mut findings = Vec::new();
        check(&def, &read_ctx(), &mut findings);
        assert!(
            !findings.iter().any(|f| f.code == UNKNOWN_READ_SOURCE),
            "a $ref source must not fire UNKNOWN_READ_SOURCE: {:?}",
            findings
        );
    }

    #[test]
    fn read_source_without_a_descriptor_is_silent() {
        // No descriptor seeded → no allowed_values → the lint cannot run (an
        // unknown action is UNKNOWN_ACTION's job, not this one's).
        let def = routine_with(
            vec![],
            vec![Track {
                name: "t1".into(),
                steps: vec![read_step("s1", json!("sorce"))],
            }],
        );
        let mut findings = Vec::new();
        check(&def, &StaticContext::new(), &mut findings);
        assert!(findings.is_empty(), "{:?}", findings);
    }

    // --- embedded tokens (Codex 2026-07-22 P2 x2) ----------------------

    /// A FORWARD embedded reference gets the same lexical-order rule as a
    /// whole ref, downgraded to the embedded warning class: "later=$s2.count"
    /// in s1's params, where s2 runs after s1, stays literal text at runtime.
    #[test]
    fn embedded_forward_reference_warns() {
        use crate::validate::findings::Severity;
        use crate::validate::params::EMBEDDED_REF_IGNORED;
        let def = routine_with(
            vec![],
            vec![Track {
                name: "t1".into(),
                steps: vec![
                    action("s1", json!({"message": "later=$s2.count"})),
                    action("s2", json!({})),
                ],
            }],
        );
        let mut findings = Vec::new();
        check(&def, &StaticContext::new(), &mut findings);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].code, EMBEDDED_REF_IGNORED);
        assert_eq!(findings[0].severity, Severity::Warning);
        assert_eq!(findings[0].step, Some(StepId("s1".into())));
        assert!(findings[0].message.contains("$s2.count"), "{}", findings[0].message);
        assert!(findings[0].message.contains("literal text"), "{}", findings[0].message);
    }

    /// Embedded cross-track reads warn (timing-dependent, like whole refs);
    /// a satisfied embedded ref, a dollar amount, and a nonexistent step id
    /// (params.rs's embedded check owns that one) are all silent here.
    #[test]
    fn embedded_cross_track_warns_and_benign_tokens_are_silent() {
        use crate::validate::params::EMBEDDED_REF_IGNORED;
        let def = routine_with(
            vec![],
            vec![
                Track {
                    name: "a".into(),
                    steps: vec![action(
                        "s1",
                        json!({"message": "peer said $s9.status, cost $50, ok=$s1.ok, gone=$s8.x"}),
                    )],
                },
                Track {
                    name: "b".into(),
                    steps: vec![action("s9", json!({}))],
                },
            ],
        );
        let mut findings = Vec::new();
        check(&def, &StaticContext::new(), &mut findings);
        // The cross-track $s9.status warns; $50 is dot-less (silent); $s8.x
        // names no step (silent here; params.rs owns nonexistent ids). The
        // self-read $s1.ok also warns (a step's own output does not exist
        // while it runs) - not asserted on, just present.
        assert!(
            findings
                .iter()
                .any(|f| f.code == EMBEDDED_REF_IGNORED && f.message.contains("$s9.status")),
            "{findings:?}"
        );
        assert!(
            !findings.iter().any(|f| f.message.contains("$50")),
            "dollar amounts are not refs: {findings:?}"
        );
        assert!(
            !findings.iter().any(|f| f.message.contains("$s8.x")),
            "nonexistent ids are params.rs's embedded check: {findings:?}"
        );
    }

    /// "$ref plus trailing text" is INTERPOLATION at runtime
    /// (executor::resolve_string), so it must not be refused with whole-ref
    /// errors: no UNSATISFIABLE_VAR for "$s9.connected fallback" when s9
    /// does not exist - the token stays literal text (params.rs warns on
    /// declared surfaces).
    #[test]
    fn dollar_prefixed_with_trailing_text_is_not_a_whole_ref_error() {
        let def = routine_with(
            vec![],
            vec![Track {
                name: "t1".into(),
                steps: vec![action("s1", json!({"message": "$s9.connected fallback"}))],
            }],
        );
        let mut findings = Vec::new();
        check(&def, &StaticContext::new(), &mut findings);
        assert!(
            !findings.iter().any(|f| f.code == UNSATISFIABLE_VAR),
            "interpolation strings must not hit whole-ref errors: {findings:?}"
        );
    }
}
