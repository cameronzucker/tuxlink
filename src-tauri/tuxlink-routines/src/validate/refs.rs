//! Reference + action-name resolution checks (spec §10 layer 1, plan-3
//! task 2): every `@`-token in a step's params/args must resolve to a real
//! configured entity, and every `ActionStep.action` must be a name the
//! action registry actually declares.
//!
//! Per-step attribution reuses [`crate::snapshot::walk_refs`] (the same
//! walk `snapshot::collect_refs` runs over the whole definition) scoped to
//! one step's JSON blob at a time, so a `Finding` can name the exact
//! offending track/step rather than just "somewhere in this routine."
//!
//! `$var` strings (e.g. `"$s1.gateway"`, spec §14 — resolved by the
//! executor through `RunVars`, see `executor.rs`) are never mistaken for
//! `@`-refs: `EntityRef::parse` only matches a leading `@`, so `walk_refs`
//! already skips them structurally. `unresolved_ref_ignores_dollar_var_strings`
//! below is the explicit regression test the plan-1 ledger asked for.

use crate::refs::EntityRef;
use crate::snapshot::walk_refs;
use crate::types::{Control, RoutineDef, Step};

use super::context::ValidationContext;
use super::findings::Finding;

pub const UNRESOLVED_REF: &str = "UNRESOLVED_REF";
pub const UNKNOWN_ACTION: &str = "UNKNOWN_ACTION";

/// Append every `UNRESOLVED_REF` / `UNKNOWN_ACTION` finding for `def` into
/// `findings`. Called by `validate()` (task 2 wiring) alongside
/// `capability::check`.
pub fn check(def: &RoutineDef, ctx: &dyn ValidationContext, findings: &mut Vec<Finding>) {
    for track in &def.tracks {
        for step in &track.steps {
            match step {
                Step::Action(action_step) => {
                    check_refs_in(&action_step.params, def, ctx, findings, &track.name, &action_step.id.0);

                    if ctx.action_descriptor(&action_step.action).is_none() {
                        findings.push(
                            Finding::error(
                                UNKNOWN_ACTION,
                                def.routine.clone(),
                                format!(
                                    "step \"{}\" uses action \"{}\", which is not a known action",
                                    action_step.id.0, action_step.action
                                ),
                            )
                            .with_track(track.name.clone())
                            .with_step(action_step.id.clone()),
                        );
                    }
                }
                Step::Control(control_step) => {
                    if let Control::Call { args, .. } = &control_step.control {
                        check_refs_in(args, def, ctx, findings, &track.name, &control_step.id.0);
                    }
                }
            }
        }
    }
}

fn check_refs_in(
    value: &serde_json::Value,
    def: &RoutineDef,
    ctx: &dyn ValidationContext,
    findings: &mut Vec<Finding>,
    track_name: &str,
    step_id: &str,
) {
    let mut refs: Vec<EntityRef> = Vec::new();
    walk_refs(value, &mut refs);
    for r in refs {
        if !ctx.entity_exists(&r) {
            findings.push(
                Finding::error(
                    UNRESOLVED_REF,
                    def.routine.clone(),
                    format!("step \"{step_id}\" references {r}, which does not resolve to a configured entity"),
                )
                .with_track(track_name.to_string())
                .with_step(crate::types::StepId(step_id.to_string())),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::ActionDescriptor;
    use crate::types::{
        ActionStep, BusyPolicy, Control, ControlStep, OnInterrupted, RoutineDef, StepId, Track, TransmitMode,
        Trigger,
    };
    use crate::validate::context::StaticContext;
    use serde_json::json;

    const RADIO_CONNECT: ActionDescriptor =
        ActionDescriptor { name: "radio.connect", needs_radio: true, transmits: true, needs_internet: false };

    fn routine_with_action_step(action: &str, params: serde_json::Value) -> RoutineDef {
        RoutineDef {
            routine: "r1".into(),
            schema_version: crate::types::SUPPORTED_SCHEMA_VERSION,
            transmit_mode: TransmitMode::Attended,
            transmit_ack: None,
            on_interrupted: OnInterrupted::Stay,
            inputs: vec![],
            triggers: vec![Trigger::Manual],
            tracks: vec![Track {
                name: "t1".into(),
                steps: vec![Step::Action(ActionStep {
                    id: StepId("s1".into()),
                    action: action.into(),
                    params,
                    timeout_s: None,
                    on_radio_busy: BusyPolicy::Wait,
                })],
            }],
        }
    }

    #[test]
    fn unresolved_ref_names_the_entity_and_step_verbatim() {
        let def = routine_with_action_step(
            "radio.connect",
            json!({ "stations": "@station-set:or-gateways", "bands": ["40m"] }),
        );
        let ctx = StaticContext::new().with_action(RADIO_CONNECT); // entity NOT seeded
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);

        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.code, UNRESOLVED_REF);
        assert_eq!(f.routine, "r1");
        assert_eq!(f.track, Some("t1".to_string()));
        assert_eq!(f.step, Some(StepId("s1".into())));
        assert!(f.message.contains("@station-set:or-gateways"), "message: {}", f.message);
        assert!(f.message.contains("s1"), "message: {}", f.message);
    }

    #[test]
    fn resolved_ref_produces_no_finding() {
        let def = routine_with_action_step(
            "radio.connect",
            json!({ "stations": "@station-set:or-gateways", "bands": ["40m"] }),
        );
        let ctx =
            StaticContext::new().with_action(RADIO_CONNECT).with_entity("station-set", "or-gateways");
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.is_empty());
    }

    #[test]
    fn unresolved_ref_ignores_dollar_var_strings() {
        // Plan-1 ledger carry-in: "$s1.gateway" is a RunVars path (executor.rs),
        // never an @-entity ref. Must not fire UNRESOLVED_REF.
        let def = routine_with_action_step("radio.connect", json!({ "station": "$s1.gateway" }));
        let ctx = StaticContext::new().with_action(RADIO_CONNECT); // nothing seeded at all
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.is_empty(), "expected no findings for $var param, got {findings:?}");
    }

    #[test]
    fn unknown_action_names_the_action_verbatim() {
        let def = routine_with_action_step("radio.nonexistent", json!({}));
        let ctx = StaticContext::new(); // no actions seeded
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);

        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.code, UNKNOWN_ACTION);
        assert_eq!(f.track, Some("t1".to_string()));
        assert_eq!(f.step, Some(StepId("s1".into())));
        assert!(f.message.contains("radio.nonexistent"), "message: {}", f.message);
    }

    #[test]
    fn known_action_produces_no_unknown_action_finding() {
        let def = routine_with_action_step("radio.connect", json!({}));
        let ctx = StaticContext::new().with_action(RADIO_CONNECT);
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.is_empty());
    }

    #[test]
    fn unresolved_ref_inside_call_step_args_is_flagged() {
        let def = RoutineDef {
            routine: "caller".into(),
            schema_version: crate::types::SUPPORTED_SCHEMA_VERSION,
            transmit_mode: TransmitMode::Attended,
            transmit_ack: None,
            on_interrupted: OnInterrupted::Stay,
            inputs: vec![],
            triggers: vec![Trigger::Manual],
            tracks: vec![Track {
                name: "t1".into(),
                steps: vec![Step::Control(ControlStep {
                    id: StepId("c1".into()),
                    control: Control::Call {
                        routine: "callee".into(),
                        args: json!({ "preset": "@preset:winlink-40m" }),
                        sync: true,
                    },
                })],
            }],
        };
        let ctx = StaticContext::new(); // preset not seeded
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, UNRESOLVED_REF);
        assert_eq!(findings[0].step, Some(StepId("c1".into())));
        assert!(findings[0].message.contains("@preset:winlink-40m"));
    }
}
