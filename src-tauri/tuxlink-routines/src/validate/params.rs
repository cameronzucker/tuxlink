//! Declared-param linting (tuxlink-3nvvl, spec §10): every action step's
//! `params` object is checked against its descriptor's [`ParamSpec`] list —
//! unknown keys, missing required keys, literal type/vocabulary mismatches,
//! and `$ref` TYPE checking against the referenced step's declared
//! [`OutputSpec`]s.
//!
//! ## Why `$ref` typing is the load-bearing lint
//!
//! Executor substitution (`executor::resolve_params`) replaces a whole
//! `"$path"` string with the referenced output value, and resolves array
//! ELEMENTS in place without flattening. So for a list-typed param:
//!
//! - `"stations": "$s1.callsigns"` (bare string, list-typed output) is the
//!   CORRECT idiom — the whole string becomes the list.
//! - `"stations": ["$s1.callsigns"]` becomes an array-of-arrays at runtime
//!   and dies in the action's `Vec<String>` deserialization — AFTER saving
//!   and validating clean, at whatever hour the trigger fires.
//!
//! Both shapes were observed in live transcripts on the same night
//! (2026-07-19: GLM-5.2 wrote the correct bare form; the shipped 122b wrote
//! the nested form). This module exists so the nested form dies at SAVE time
//! with a message that teaches the bare form.
//!
//! ## Shape-level compatibility
//!
//! The lint compares [`ValueType`]s at SHAPE level (string / number /
//! boolean / list / object): the finer list kinds (`BandList`,
//! `StationList`, `StringList`) exist for pickers and vocabularies, and all
//! deserialize as `Vec<String>` — a `StationList` output feeding a
//! `BandList`-declared param is not a runtime failure and is not flagged.
//!
//! ## Skips
//!
//! An action whose descriptor declares NO params (`params: &[]`) is skipped
//! entirely: the registry hasn't declared that surface yet, and firing
//! `UNKNOWN_PARAM` for every key of an undeclared surface would be noise,
//! not signal. Same for a step whose action has no descriptor —
//! `refs::check` already fires `UNKNOWN_ACTION` for it.

use serde_json::Value;

use crate::action::{ActionDescriptor, ValueType};
use crate::refs::VarPath;
use crate::types::{RoutineDef, Step, StepId};

use super::context::ValidationContext;
use super::findings::Finding;

pub const UNKNOWN_PARAM: &str = "UNKNOWN_PARAM";
pub const MISSING_REQUIRED_PARAM: &str = "MISSING_REQUIRED_PARAM";
pub const PARAM_TYPE_MISMATCH: &str = "PARAM_TYPE_MISMATCH";
pub const PARAM_VALUE_NOT_ALLOWED: &str = "PARAM_VALUE_NOT_ALLOWED";
pub const REF_TYPE_MISMATCH: &str = "REF_TYPE_MISMATCH";
pub const REF_UNKNOWN_STEP: &str = "REF_UNKNOWN_STEP";
pub const REF_UNKNOWN_OUTPUT: &str = "REF_UNKNOWN_OUTPUT";
pub const REF_NULLABLE_SOURCE: &str = "REF_NULLABLE_SOURCE";
pub const EMBEDDED_REF_IGNORED: &str = "EMBEDDED_REF_IGNORED";

/// Descriptor self-consistency check (tuxlink-3nvvl): lint `desc`'s own
/// `example_params` against `desc`'s declared [`ParamSpec`]s via a synthetic
/// one-step routine. App-side registry tests run this over every registered
/// action — a descriptor whose example fails its own declarations is a
/// backfill bug caught mechanically at test time. Returns every finding
/// (a consistent descriptor returns none).
pub fn example_self_check(desc: &ActionDescriptor) -> Vec<Finding> {
    use crate::types::{
        ActionStep, BusyPolicy, OnInterrupted, Track, TransmitMode, Trigger,
    };
    let params: Value = match desc.example_params {
        Some(s) => serde_json::from_str(s).unwrap_or(Value::Null),
        None => Value::Null,
    };
    let def = RoutineDef {
        routine: format!("self-check-{}", desc.name),
        schema_version: crate::types::SUPPORTED_SCHEMA_VERSION,
        transmit_mode: TransmitMode::Attended,
        transmit_ack: None,
        write_ack: None,
        on_interrupted: OnInterrupted::Stay,
        inputs: vec![],
        triggers: vec![Trigger::Manual],
        tracks: vec![Track {
            name: "main".into(),
            steps: vec![Step::Action(ActionStep {
                id: StepId("s1".into()),
                action: desc.name.to_string(),
                params,
                timeout_s: None,
                on_radio_busy: BusyPolicy::Wait,
            })],
        }],
    };
    let ctx = super::context::StaticContext::new().with_action(*desc);
    let mut findings = Vec::new();
    check(&def, &ctx, &mut findings);
    findings
}

/// Shape-level type classes — what actually matters to runtime
/// deserialization (see module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    Str,
    Num,
    Bool,
    List,
    ObjList,
    Obj,
}

fn shape(ty: ValueType) -> Shape {
    match ty {
        ValueType::String => Shape::Str,
        ValueType::Number => Shape::Num,
        ValueType::Boolean => Shape::Bool,
        ValueType::StringList | ValueType::BandList | ValueType::StationList => Shape::List,
        ValueType::ObjectList => Shape::ObjList,
        ValueType::Object => Shape::Obj,
    }
}

fn shape_name(s: Shape) -> &'static str {
    match s {
        Shape::Str => "a string",
        Shape::Num => "a number",
        Shape::Bool => "a boolean",
        Shape::List => "a list of strings",
        Shape::ObjList => "a list of objects",
        Shape::Obj => "an object",
    }
}

/// What a `$ref` resolves to, per the referenced step's declared outputs.
enum RefTarget {
    /// Step id not present in this routine.
    UnknownStep,
    /// Step exists but its action declares no output by that key.
    UnknownOutput,
    /// Referenced action declares no outputs at all, or the path walks into
    /// a container output — inner type unknown, nothing to lint.
    Unknowable,
    /// `(type, nullable)` of the declared output.
    Known(ValueType, bool),
}

fn resolve_ref(
    raw: &str,
    steps_by_id: &std::collections::HashMap<StepId, String>,
    ctx: &dyn ValidationContext,
) -> RefTarget {
    let Some(vp) = VarPath::parse(raw.trim_start_matches('$')) else {
        // Not a step-output path (e.g. a routine input name) — nothing to lint.
        return RefTarget::Unknowable;
    };
    let Some(action_name) = steps_by_id.get(&vp.step) else {
        return RefTarget::UnknownStep;
    };
    let Some(desc) = ctx.action_descriptor(action_name) else {
        // UNKNOWN_ACTION already fired in refs::check.
        return RefTarget::Unknowable;
    };
    if desc.outputs.is_empty() {
        return RefTarget::Unknowable;
    }
    // Exact key first (mirrors RunVars::resolve), then the first dot-segment
    // for nested walks. The executor's `walk_nested` traverses objects by key
    // AND arrays by numeric index, so a path through ANY container output
    // ($s1.gateways.0.callsign, $s1.callsigns.0) is real and its leaf type is
    // unknowable here (Codex adrev 2026-07-20 GPT-5.5 P2 #4).
    if let Some(o) = desc.outputs.iter().find(|o| o.key == vp.output) {
        return RefTarget::Known(o.ty, o.nullable);
    }
    let head = vp.output.split('.').next().unwrap_or(&vp.output);
    match desc.outputs.iter().find(|o| o.key == head) {
        Some(o) if matches!(shape(o.ty), Shape::Obj | Shape::ObjList | Shape::List) => {
            RefTarget::Unknowable
        }
        Some(_) => RefTarget::UnknownOutput,
        None => RefTarget::UnknownOutput,
    }
}

/// Expected value shape per `@entity:` kind, for the kinds whose resolved
/// shape is fixed by construction: a preset resolves to the preset OBJECT, a
/// station set to a callsign LIST. Unknown kinds return `None` (skip) —
/// `refs::check` still validates existence (Codex adrev 2026-07-20 GPT-5.6
/// P2 #1: an `@station-set:` ref into a `preset` param must not sail
/// through).
fn entity_kind_shape(kind: &str) -> Option<Shape> {
    match kind {
        "preset" => Some(Shape::Obj),
        "station-set" => Some(Shape::List),
        _ => None,
    }
}

/// Append every declared-param finding for `def` into `findings`.
pub fn check(def: &RoutineDef, ctx: &dyn ValidationContext, findings: &mut Vec<Finding>) {
    // Step-id → action-name index across ALL tracks (RunVars is shared
    // across concurrent tracks at runtime, so cross-track refs are legal).
    let mut steps_by_id: std::collections::HashMap<StepId, String> =
        std::collections::HashMap::new();
    for track in &def.tracks {
        for step in &track.steps {
            if let Step::Action(a) = step {
                steps_by_id.insert(a.id.clone(), a.action.clone());
            }
        }
    }

    for track in &def.tracks {
        for step in &track.steps {
            let Step::Action(a) = step else { continue };
            let Some(desc) = ctx.action_descriptor(&a.action) else {
                continue; // UNKNOWN_ACTION fires in refs::check
            };
            if desc.params.is_empty() {
                continue; // undeclared surface — nothing to lint against
            }
            let mut push = |mut f: Finding| {
                f.track = Some(track.name.clone());
                f.step = Some(a.id.clone());
                findings.push(f);
            };

            let empty = serde_json::Map::new();
            let obj = match &a.params {
                Value::Object(map) => map,
                Value::Null => &empty,
                other => {
                    push(Finding::error(
                        PARAM_TYPE_MISMATCH,
                        &def.routine,
                        format!(
                            "step \"{}\" (action \"{}\"): params must be a JSON object, got {}",
                            a.id.0,
                            a.action,
                            shape_name(value_shape(other))
                        ),
                    ));
                    continue;
                }
            };

            for key in obj.keys() {
                // `_`-prefixed keys are the engine-injected namespace
                // (`_radio_busy_policy`, `_step_timeout_s`, `_run_id`,
                // `_step_id` — see actions' `*_from_params` helpers) and are
                // never author-declared.
                if key.starts_with('_') {
                    continue;
                }
                if !desc.params.iter().any(|p| p.key == key) {
                    push(Finding::warning(
                        UNKNOWN_PARAM,
                        &def.routine,
                        format!(
                            "step \"{}\" (action \"{}\"): param \"{key}\" is not declared by this action and is ignored at runtime — declared params: {}",
                            a.id.0,
                            a.action,
                            keys_of(desc.params)
                        ),
                    ));
                }
            }

            for spec in desc.params {
                if spec.required && !obj.contains_key(spec.key) {
                    push(Finding::error(
                        MISSING_REQUIRED_PARAM,
                        &def.routine,
                        format!(
                            "step \"{}\" (action \"{}\"): required param \"{}\" is missing — it must be {} (example: {})",
                            a.id.0,
                            a.action,
                            spec.key,
                            shape_name(shape(spec.ty)),
                            spec.example
                        ),
                    ));
                }
            }

            for spec in desc.params {
                let Some(val) = obj.get(spec.key) else { continue };
                check_value(&def.routine, a, spec, val, &steps_by_id, ctx, &mut push);
            }
        }
    }
}

/// True when a literal (non-leading-`$`) string contains what reads like a
/// step ref (`$s<digit>` …) — the author almost certainly expected
/// interpolation that whole-value-only substitution will not perform.
fn looks_like_embedded_ref(s: &str) -> bool {
    if s.starts_with('$') {
        return false; // whole-value ref — handled by the ref arm
    }
    s.match_indices("$s").any(|(i, _)| {
        s[i + 2..]
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
    })
}

fn value_shape(v: &Value) -> Shape {
    match v {
        Value::String(_) => Shape::Str,
        Value::Number(_) => Shape::Num,
        Value::Bool(_) => Shape::Bool,
        Value::Array(_) => Shape::List,
        Value::Object(_) | Value::Null => Shape::Obj,
    }
}

fn keys_of(specs: &[crate::action::ParamSpec]) -> String {
    specs
        .iter()
        .map(|p| p.key)
        .collect::<Vec<_>>()
        .join(", ")
}

#[allow(clippy::too_many_arguments)]
fn check_value(
    routine: &str,
    a: &crate::types::ActionStep,
    spec: &crate::action::ParamSpec,
    val: &Value,
    steps_by_id: &std::collections::HashMap<StepId, String>,
    ctx: &dyn ValidationContext,
    push: &mut dyn FnMut(Finding),
) {
    let want = shape(spec.ty);
    match val {
        // Explicit null: legal serde input for an OPTIONAL param
        // (Option<T> fields deserialize null — GPT-5.6 adrev 2026-07-20),
        // a guaranteed runtime rejection for a REQUIRED one (GPT-5.5, same
        // round, opposite face of the same hole).
        Value::Null => {
            if spec.required {
                push(Finding::error(
                    PARAM_TYPE_MISMATCH,
                    routine,
                    format!(
                        "step \"{}\" (action \"{}\"): required param \"{}\" is null — provide {} (example: {})",
                        a.id.0,
                        a.action,
                        spec.key,
                        shape_name(want),
                        spec.example
                    ),
                ));
            }
        }
        // `@entity:name` refs resolve to their entity's shape before
        // execution. Existence is refs::check's job; the SHAPE is checkable
        // for the kinds whose resolved shape is fixed (preset → object,
        // station-set → list). Unknown kinds skip.
        Value::String(s) if s.starts_with('@') => {
            if let Some(entity) = crate::refs::EntityRef::parse(s) {
                if let Some(got) = entity_kind_shape(&entity.kind) {
                    if got != want {
                        push(Finding::error(
                            REF_TYPE_MISMATCH,
                            routine,
                            format!(
                                "step \"{}\" (action \"{}\"): param \"{}\" wants {} but \"{s}\" (an @{} ref) resolves to {}",
                                a.id.0,
                                a.action,
                                spec.key,
                                shape_name(want),
                                entity.kind,
                                shape_name(got)
                            ),
                        ));
                    }
                }
            }
        }
        // Whole-string `$ref`: the referenced output's type IS the value's
        // type after substitution.
        Value::String(s) if s.starts_with('$') => match resolve_ref(s, steps_by_id, ctx) {
            RefTarget::UnknownStep => push(Finding::error(
                REF_UNKNOWN_STEP,
                routine,
                format!(
                    "step \"{}\" (action \"{}\"): param \"{}\" references \"{s}\", but no step with that id exists — the run would fail at resolution",
                    a.id.0, a.action, spec.key
                ),
            )),
            RefTarget::UnknownOutput => push(Finding::warning(
                REF_UNKNOWN_OUTPUT,
                routine,
                format!(
                    "step \"{}\" (action \"{}\"): param \"{}\" references \"{s}\", but the referenced action declares no such output",
                    a.id.0, a.action, spec.key
                ),
            )),
            RefTarget::Unknowable => {}
            RefTarget::Known(out_ty, nullable) => {
                if shape(out_ty) != want {
                    push(Finding::error(
                        REF_TYPE_MISMATCH,
                        routine,
                        format!(
                            "step \"{}\" (action \"{}\"): param \"{}\" wants {} but \"{s}\" resolves to {}",
                            a.id.0,
                            a.action,
                            spec.key,
                            shape_name(want),
                            shape_name(shape(out_ty))
                        ),
                    ));
                } else if nullable && spec.required {
                    // Both adrev models, independently: a nullable output
                    // must not read as unconditionally type-safe.
                    push(Finding::warning(
                        REF_NULLABLE_SOURCE,
                        routine,
                        format!(
                            "step \"{}\" (action \"{}\"): required param \"{}\" takes \"{s}\", which can be null or absent depending on the referenced step's path — branch on it first or accept the runtime risk",
                            a.id.0, a.action, spec.key
                        ),
                    ));
                }
            }
        },
        Value::String(s) => {
            if want == Shape::Str {
                // Substitution is whole-value only: "$s2.status" substitutes,
                // "done: $s2.status" logs the literal text. Observed in the
                // wild (GLM battery 2026-07-19: every log line) — warn.
                if looks_like_embedded_ref(s) {
                    push(Finding::warning(
                        EMBEDDED_REF_IGNORED,
                        routine,
                        format!(
                            "step \"{}\" (action \"{}\"): param \"{}\" embeds a step ref inside longer text (\"{s}\") — refs substitute only when the value IS the ref; this will run/log as literal text",
                            a.id.0, a.action, spec.key
                        ),
                    ));
                }
                if let Some(allowed) = spec.allowed {
                    if !allowed_contains(spec, allowed, s) {
                        push(not_allowed(routine, a, spec, s, allowed));
                    }
                }
            } else if want == Shape::List {
                push(Finding::error(
                    PARAM_TYPE_MISMATCH,
                    routine,
                    format!(
                        "step \"{}\" (action \"{}\"): param \"{}\" wants a list of strings — wrap the value in an array: [\"{s}\"]",
                        a.id.0, a.action, spec.key
                    ),
                ));
            } else {
                push(mismatch(routine, a, spec, want, Shape::Str));
            }
        }
        Value::Array(items) if want == Shape::List => {
            for item in items {
                match item {
                    Value::String(s) if s.starts_with('$') => {
                        match resolve_ref(s, steps_by_id, ctx) {
                            RefTarget::UnknownStep => push(Finding::error(
                                REF_UNKNOWN_STEP,
                                routine,
                                format!(
                                    "step \"{}\" (action \"{}\"): param \"{}\" references \"{s}\", but no step with that id exists",
                                    a.id.0, a.action, spec.key
                                ),
                            )),
                            RefTarget::UnknownOutput => push(Finding::warning(
                                REF_UNKNOWN_OUTPUT,
                                routine,
                                format!(
                                    "step \"{}\" (action \"{}\"): param \"{}\" references \"{s}\", but the referenced action declares no such output",
                                    a.id.0, a.action, spec.key
                                ),
                            )),
                            RefTarget::Unknowable => {}
                            RefTarget::Known(out_ty, _) if shape(out_ty) == Shape::List => {
                                // THE motivating lint: element-wise
                                // substitution puts the referenced LIST
                                // inside this list — array-of-arrays,
                                // guaranteed runtime deserialization death.
                                push(Finding::error(
                                    REF_TYPE_MISMATCH,
                                    routine,
                                    format!(
                                        "step \"{}\" (action \"{}\"): [\"{s}\"] puts a list INSIDE the \"{}\" list — substitution replaces the element with the referenced list, producing an array of arrays that fails at runtime. Pass the reference as the whole value instead: \"{}\": \"{s}\"",
                                        a.id.0, a.action, spec.key, spec.key
                                    ),
                                ));
                            }
                            RefTarget::Known(out_ty, _) if shape(out_ty) != Shape::Str => {
                                push(Finding::error(
                                    REF_TYPE_MISMATCH,
                                    routine,
                                    format!(
                                        "step \"{}\" (action \"{}\"): param \"{}\" element \"{s}\" resolves to {}, but list elements must be strings",
                                        a.id.0,
                                        a.action,
                                        spec.key,
                                        shape_name(shape(out_ty))
                                    ),
                                ));
                            }
                            RefTarget::Known(..) => {}
                        }
                    }
                    Value::String(s) => {
                        if let Some(allowed) = spec.allowed {
                            if !allowed_contains(spec, allowed, s) {
                                push(not_allowed(routine, a, spec, s, allowed));
                            }
                        }
                    }
                    other => push(Finding::error(
                        PARAM_TYPE_MISMATCH,
                        routine,
                        format!(
                            "step \"{}\" (action \"{}\"): param \"{}\" has {} as a list element — elements must be strings",
                            a.id.0,
                            a.action,
                            spec.key,
                            shape_name(value_shape(other))
                        ),
                    )),
                }
            }
        }
        // Object-list params/outputs: presence + listness only (element
        // shape is not linted — see `ValueType::ObjectList`).
        Value::Array(_) if want == Shape::ObjList => {}
        other => {
            let got = value_shape(other);
            if got != want {
                push(mismatch(routine, a, spec, want, got));
            }
        }
    }
}

fn mismatch(
    routine: &str,
    a: &crate::types::ActionStep,
    spec: &crate::action::ParamSpec,
    want: Shape,
    got: Shape,
) -> Finding {
    Finding::error(
        PARAM_TYPE_MISMATCH,
        routine,
        format!(
            "step \"{}\" (action \"{}\"): param \"{}\" wants {} but got {} (example: {})",
            a.id.0,
            a.action,
            spec.key,
            shape_name(want),
            shape_name(got),
            spec.example
        ),
    )
}

/// Allowed-vocabulary membership. `BandList` compares case-insensitively
/// (tuxlink-fg0em adrev consensus): the runtime's `band_range` lookup is
/// `eq_ignore_ascii_case`, so `"20M"` has always executed — the validator
/// must not block on next save what the runtime accepts. Every other
/// allowed-list stays exact-match: enum-typed params (busy policies, listing
/// modes) deserialize case-SENSITIVELY downstream, and a case-insensitive
/// pass here would trade a teaching finding for a runtime serde error.
fn allowed_contains(spec: &ParamSpec, allowed: &[&str], s: &str) -> bool {
    if matches!(spec.ty, ValueType::BandList) {
        allowed.iter().any(|a| a.eq_ignore_ascii_case(s))
    } else {
        allowed.contains(&s)
    }
}

fn not_allowed(
    routine: &str,
    a: &crate::types::ActionStep,
    spec: &crate::action::ParamSpec,
    value: &str,
    allowed: &[&str],
) -> Finding {
    Finding::error(
        PARAM_VALUE_NOT_ALLOWED,
        routine,
        format!(
            "step \"{}\" (action \"{}\"): param \"{}\" value \"{value}\" is not in the allowed set: {}",
            a.id.0,
            a.action,
            spec.key,
            allowed.join(", ")
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{OutputSpec, ParamSpec};
    use crate::types::{
        ActionStep, BusyPolicy, OnInterrupted, RoutineDef, Track, TransmitMode, Trigger,
    };
    use crate::validate::context::StaticContext;
    use crate::validate::findings::Severity;
    use serde_json::json;

    const FIND_STATIONS: ActionDescriptor = ActionDescriptor {
        name: "data.find_stations",
        label: "",
        description: "",
        needs_radio: false,
        transmits: false,
        writes_config: false,
        needs_internet: false,
        example_params: None,
        allowed_values: None,
        params: &[ParamSpec {
            key: "bands",
            ty: ValueType::BandList,
            required: false,
            description: "Bands to search",
            allowed: Some(&["20m", "40m", "80m"]),
            example: "[\"20m\"]",
        }],
        outputs: &[
            OutputSpec {
                key: "callsigns",
                ty: ValueType::StationList,
                description: "Distance-sorted deduped callsigns",
                nullable: false,
            },
            OutputSpec {
                key: "count",
                ty: ValueType::Number,
                description: "How many stations matched",
                nullable: false,
            },
        ],
        dry_run_shape: None,
    };

    const RADIO_CONNECT: ActionDescriptor = ActionDescriptor {
        name: "radio.connect",
        label: "",
        description: "",
        needs_radio: true,
        transmits: true,
        writes_config: false,
        needs_internet: false,
        example_params: None,
        allowed_values: None,
        params: &[
            ParamSpec {
                key: "stations",
                ty: ValueType::StationList,
                required: true,
                description: "Callsigns to walk in order",
                allowed: None,
                example: "[\"N0DAJ\"]",
            },
            ParamSpec {
                key: "bands",
                ty: ValueType::BandList,
                required: false,
                description: "Bands to walk per station",
                allowed: Some(&["20m", "40m", "80m"]),
                example: "[\"20m\"]",
            },
            ParamSpec {
                key: "listen_before_tx_s",
                ty: ValueType::Number,
                required: false,
                description: "Clear-channel listen window",
                allowed: None,
                example: "5",
            },
        ],
        outputs: &[OutputSpec {
            key: "connected",
            ty: ValueType::Boolean,
            description: "Did any attempt connect",
            nullable: false,
        }],
        dry_run_shape: None,
    };

    /// An action that has NOT declared its params surface.
    const UNDECLARED: ActionDescriptor = ActionDescriptor {
        name: "local.legacy",
        label: "",
        description: "",
        needs_radio: false,
        transmits: false,
        writes_config: false,
        needs_internet: false,
        example_params: None,
        allowed_values: None,
        params: &[],
        outputs: &[],
        dry_run_shape: None,
    };

    fn action_step(id: &str, action: &str, params: Value) -> Step {
        Step::Action(ActionStep {
            id: StepId(id.into()),
            action: action.into(),
            params,
            timeout_s: None,
            on_radio_busy: BusyPolicy::Wait,
        })
    }

    fn routine(steps: Vec<Step>) -> RoutineDef {
        RoutineDef {
            routine: "r1".into(),
            schema_version: crate::types::SUPPORTED_SCHEMA_VERSION,
            transmit_mode: TransmitMode::Attended,
            transmit_ack: None,
            write_ack: None,
            on_interrupted: OnInterrupted::Stay,
            inputs: vec![],
            triggers: vec![Trigger::Manual],
            tracks: vec![Track {
                name: "main".into(),
                steps,
            }],
        }
    }

    fn ctx() -> StaticContext {
        StaticContext::new()
            .with_action(FIND_STATIONS)
            .with_action(RADIO_CONNECT)
            .with_action(UNDECLARED)
    }

    fn run(def: &RoutineDef) -> Vec<Finding> {
        let mut findings = Vec::new();
        check(def, &ctx(), &mut findings);
        findings
    }

    /// THE motivating lint: `["$s1.callsigns"]` where `callsigns` is a list
    /// output is an array-of-arrays after element-wise substitution. Save
    /// time error; the message teaches the bare-string form verbatim.
    #[test]
    fn nested_list_ref_to_list_output_is_flagged_with_the_fix() {
        let def = routine(vec![
            action_step("s1", "data.find_stations", json!({"bands": ["20m"]})),
            action_step(
                "s2",
                "radio.connect",
                json!({"stations": ["$s1.callsigns"], "bands": ["20m"]}),
            ),
        ]);
        let findings = run(&def);
        assert_eq!(findings.len(), 1, "exactly the nested-ref finding: {findings:?}");
        let f = &findings[0];
        assert_eq!(f.code, REF_TYPE_MISMATCH);
        assert_eq!(f.severity, Severity::Error);
        assert_eq!(f.step, Some(StepId("s2".into())));
        assert!(f.message.contains("$s1.callsigns"), "names the ref: {}", f.message);
        assert!(f.message.contains("stations"), "names the param: {}", f.message);
        assert!(
            f.message.contains("\"stations\": \"$s1.callsigns\""),
            "teaches the bare-string fix verbatim: {}",
            f.message
        );
    }

    /// The correct whole-string idiom is clean.
    #[test]
    fn bare_string_ref_to_list_output_is_clean() {
        let def = routine(vec![
            action_step("s1", "data.find_stations", json!({"bands": ["20m"]})),
            action_step(
                "s2",
                "radio.connect",
                json!({"stations": "$s1.callsigns", "bands": ["40m"]}),
            ),
        ]);
        assert!(run(&def).is_empty(), "{:?}", run(&def));
    }

    /// A scalar-output ref as a list ELEMENT is the valid element-wise form.
    /// A list-kind output feeding a different list-kind param (StationList
    /// output into BandList-ish positions) is shape-compatible — checked via
    /// bands taking the callsigns ref bare.
    #[test]
    fn scalar_element_ref_and_cross_list_kind_bare_ref_are_clean() {
        let def = routine(vec![
            action_step("s1", "data.find_stations", json!({})),
            action_step(
                "s2",
                "radio.connect",
                // count is Number — NOT valid as a list element…
                json!({"stations": "$s1.callsigns", "bands": "$s1.callsigns"}),
            ),
        ]);
        assert!(run(&def).is_empty(), "{:?}", run(&def));
    }

    #[test]
    fn missing_required_param_is_error() {
        let def = routine(vec![action_step("s2", "radio.connect", json!({"bands": ["20m"]}))]);
        let findings = run(&def);
        assert_eq!(findings.len(), 1, "{findings:?}");
        let f = &findings[0];
        assert_eq!(f.code, MISSING_REQUIRED_PARAM);
        assert_eq!(f.severity, Severity::Error);
        assert!(f.message.contains("stations"), "{}", f.message);
        assert!(f.message.contains("radio.connect"), "{}", f.message);
    }

    #[test]
    fn unknown_param_key_is_warning_naming_the_key() {
        let def = routine(vec![action_step(
            "s2",
            "radio.connect",
            json!({"stations": ["N0DAJ"], "bandz": ["20m"]}),
        )]);
        let findings = run(&def);
        assert_eq!(findings.len(), 1, "{findings:?}");
        let f = &findings[0];
        assert_eq!(f.code, UNKNOWN_PARAM);
        assert_eq!(f.severity, Severity::Warning);
        assert!(f.message.contains("bandz"), "{}", f.message);
    }

    #[test]
    fn literal_type_mismatch_is_error() {
        let def = routine(vec![action_step(
            "s2",
            "radio.connect",
            json!({"stations": ["N0DAJ"], "listen_before_tx_s": "five"}),
        )]);
        let findings = run(&def);
        assert_eq!(findings.len(), 1, "{findings:?}");
        let f = &findings[0];
        assert_eq!(f.code, PARAM_TYPE_MISMATCH);
        assert_eq!(f.severity, Severity::Error);
        assert!(f.message.contains("listen_before_tx_s"), "{}", f.message);
    }

    /// A bare literal string where a list is declared is the "wrap it in an
    /// array" mistake — type mismatch, not a ref problem.
    #[test]
    fn bare_literal_string_for_list_param_is_error() {
        let def = routine(vec![action_step(
            "s2",
            "radio.connect",
            json!({"stations": "N0DAJ"}),
        )]);
        let findings = run(&def);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].code, PARAM_TYPE_MISMATCH);
        assert!(findings[0].message.contains("[\"N0DAJ\"]"), "teaches the wrap: {}", findings[0].message);
    }

    #[test]
    fn literal_outside_allowed_vocabulary_is_error() {
        let def = routine(vec![action_step(
            "s2",
            "radio.connect",
            json!({"stations": ["N0DAJ"], "bands": ["99m"]}),
        )]);
        let findings = run(&def);
        assert_eq!(findings.len(), 1, "{findings:?}");
        let f = &findings[0];
        assert_eq!(f.code, PARAM_VALUE_NOT_ALLOWED);
        assert_eq!(f.severity, Severity::Error);
        assert!(f.message.contains("99m"), "{}", f.message);
        assert!(f.message.contains("20m"), "lists the vocabulary: {}", f.message);
    }

    // tuxlink-fg0em adrev consensus: the runtime's band lookup is
    // case-insensitive and always was — "20M" executed before the allowed
    // list existed, so the validator must accept it too (BandList only;
    // enum-typed allowed-lists stay exact so serde failures keep their
    // teaching finding).
    #[test]
    fn band_case_variants_stay_valid() {
        let def = routine(vec![action_step(
            "s2",
            "radio.connect",
            json!({"stations": ["N0DAJ"], "bands": ["20M", "40m"]}),
        )]);
        let findings = run(&def);
        assert!(
            findings.is_empty(),
            "case variants of a valid band must not flag: {findings:?}"
        );
    }

    #[test]
    fn ref_to_unknown_step_is_error_and_undeclared_output_is_warning() {
        let def = routine(vec![
            action_step("s1", "data.find_stations", json!({})),
            action_step(
                "s2",
                "radio.connect",
                json!({"stations": "$s9.callsigns"}),
            ),
            action_step(
                "s3",
                "radio.connect",
                json!({"stations": "$s1.mystery_field"}),
            ),
        ]);
        let findings = run(&def);
        assert_eq!(findings.len(), 2, "{findings:?}");
        let unknown_step = findings.iter().find(|f| f.code == REF_UNKNOWN_STEP).unwrap();
        assert_eq!(unknown_step.severity, Severity::Error);
        assert!(unknown_step.message.contains("$s9.callsigns"), "{}", unknown_step.message);
        let unknown_out = findings.iter().find(|f| f.code == REF_UNKNOWN_OUTPUT).unwrap();
        assert_eq!(unknown_out.severity, Severity::Warning);
        assert!(unknown_out.message.contains("mystery_field"), "{}", unknown_out.message);
    }

    /// Undeclared surface (params: &[]) is skipped wholesale — no
    /// UNKNOWN_PARAM storm from a registry that hasn't declared itself.
    #[test]
    fn undeclared_params_surface_is_skipped() {
        let def = routine(vec![action_step(
            "s1",
            "local.legacy",
            json!({"anything": {"nested": true}, "goes": 1}),
        )]);
        assert!(run(&def).is_empty(), "{:?}", run(&def));
    }

    /// An embedded `$sN.…` inside longer literal text never interpolates
    /// (whole-value substitution only) — warn, because the author almost
    /// certainly expected interpolation (GLM battery, every log line).
    #[test]
    fn embedded_ref_inside_text_warns() {
        // local.log-alike: declare a String param on the connect descriptor
        // via bands? Use a dedicated single-String-param action instead.
        const LOGGER: ActionDescriptor = ActionDescriptor {
            name: "local.log",
            label: "",
            description: "",
            needs_radio: false,
            transmits: false,
            writes_config: false,
            needs_internet: false,
            example_params: None,
            allowed_values: None,
            params: &[ParamSpec {
                key: "message",
                ty: ValueType::String,
                required: true,
                description: "",
                allowed: None,
                example: "\"x\"",
            }],
            outputs: &[],
            dry_run_shape: None,
        };
        let def = routine(vec![action_step(
            "s2",
            "local.log",
            json!({"message": "scan done: $s1.callsigns"}),
        )]);
        let mut findings = Vec::new();
        check(&def, &StaticContext::new().with_action(LOGGER), &mut findings);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].code, EMBEDDED_REF_IGNORED);
        assert_eq!(findings[0].severity, Severity::Warning);
        assert!(findings[0].message.contains("literal text"), "{}", findings[0].message);

        // A plain dollar amount is NOT a ref — no warning.
        let def = routine(vec![action_step(
            "s2",
            "local.log",
            json!({"message": "budget is $50 total"}),
        )]);
        let mut findings = Vec::new();
        check(&def, &StaticContext::new().with_action(LOGGER), &mut findings);
        assert!(findings.is_empty(), "{findings:?}");
    }

    /// `@entity:` refs are shape-checked by KIND where the resolved shape is
    /// fixed: a station set into a list param is clean; a station set into an
    /// object param is the runtime deserialization death GPT-5.6 flagged
    /// (adrev 2026-07-20). Unknown kinds skip.
    #[test]
    fn entity_ref_kinds_are_shape_checked() {
        // station-set → StationList param: clean.
        let def = routine(vec![action_step(
            "s2",
            "radio.connect",
            json!({"stations": "@station-set:or-gateways"}),
        )]);
        assert!(run(&def).is_empty(), "{:?}", run(&def));

        // station-set → Object param: error naming both shapes.
        const PRESET_TAKER: ActionDescriptor = ActionDescriptor {
            name: "rig.apply_preset",
            label: "",
            description: "",
            needs_radio: true,
            transmits: false,
            writes_config: false,
            needs_internet: false,
            example_params: None,
            allowed_values: None,
            params: &[ParamSpec {
                key: "preset",
                ty: ValueType::Object,
                required: true,
                description: "",
                allowed: None,
                example: "{}",
            }],
            outputs: &[],
            dry_run_shape: None,
        };
        let def = routine(vec![action_step(
            "s2",
            "rig.apply_preset",
            json!({"preset": "@station-set:or-gateways"}),
        )]);
        let mut findings = Vec::new();
        check(&def, &StaticContext::new().with_action(PRESET_TAKER), &mut findings);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].code, REF_TYPE_MISMATCH);
        assert!(findings[0].message.contains("station-set"), "{}", findings[0].message);

        // preset → Object param: clean.
        let def = routine(vec![action_step(
            "s2",
            "rig.apply_preset",
            json!({"preset": "@preset:vara-20m"}),
        )]);
        let mut findings = Vec::new();
        check(&def, &StaticContext::new().with_action(PRESET_TAKER), &mut findings);
        assert!(findings.is_empty(), "{findings:?}");
    }

    /// Explicit null: clean for an OPTIONAL param (Option<T> accepts it at
    /// runtime — GPT-5.6's false-positive), an error for a REQUIRED one
    /// (guaranteed runtime rejection — GPT-5.5's false-negative). Both faces
    /// of the same hole, adrev 2026-07-20.
    #[test]
    fn null_is_clean_for_optional_and_error_for_required() {
        // Optional bands: null → clean.
        let def = routine(vec![action_step(
            "s2",
            "radio.connect",
            json!({"stations": ["N0DAJ"], "bands": null}),
        )]);
        assert!(run(&def).is_empty(), "{:?}", run(&def));

        // Required stations: null → error naming the param.
        let def = routine(vec![action_step(
            "s2",
            "radio.connect",
            json!({"stations": null}),
        )]);
        let findings = run(&def);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].code, PARAM_TYPE_MISMATCH);
        assert!(findings[0].message.contains("is null"), "{}", findings[0].message);
    }

    /// Nested paths through ANY container output are executor-walkable
    /// ($s1.gateways.0.callsign — arrays traverse by numeric index), so they
    /// must be Unknowable, not a false REF_UNKNOWN_OUTPUT (GPT-5.5 #4).
    #[test]
    fn nested_path_through_list_outputs_is_unknowable_not_warned() {
        const WITH_GATEWAYS: ActionDescriptor = ActionDescriptor {
            name: "data.find_stations",
            label: "",
            description: "",
            needs_radio: false,
            transmits: false,
            writes_config: false,
            needs_internet: false,
            example_params: None,
            allowed_values: None,
            params: &[],
            outputs: &[
                OutputSpec {
                    key: "gateways",
                    ty: ValueType::ObjectList,
                    description: "",
                    nullable: false,
                },
                OutputSpec {
                    key: "callsigns",
                    ty: ValueType::StationList,
                    description: "",
                    nullable: false,
                },
            ],
            dry_run_shape: None,
        };
        const LOGGER2: ActionDescriptor = ActionDescriptor {
            name: "local.log",
            label: "",
            description: "",
            needs_radio: false,
            transmits: false,
            writes_config: false,
            needs_internet: false,
            example_params: None,
            allowed_values: None,
            params: &[ParamSpec {
                key: "message",
                ty: ValueType::String,
                required: true,
                description: "",
                allowed: None,
                example: "\"x\"",
            }],
            outputs: &[],
            dry_run_shape: None,
        };
        let def = routine(vec![
            action_step("s1", "data.find_stations", json!({})),
            action_step("s2", "local.log", json!({"message": "$s1.gateways.0.callsign"})),
            action_step("s3", "local.log", json!({"message": "$s1.callsigns.0"})),
        ]);
        let mut findings = Vec::new();
        check(
            &def,
            &StaticContext::new().with_action(WITH_GATEWAYS).with_action(LOGGER2),
            &mut findings,
        );
        assert!(findings.is_empty(), "{findings:?}");
    }

    /// A nullable output feeding a REQUIRED param warns (never errors): the
    /// shape matches when present, but both adrev models flagged the
    /// unconditional-safety illusion.
    #[test]
    fn nullable_output_into_required_param_warns() {
        const CONNECTOR: ActionDescriptor = ActionDescriptor {
            name: "radio.connect",
            label: "",
            description: "",
            needs_radio: true,
            transmits: true,
            writes_config: false,
            needs_internet: false,
            example_params: None,
            allowed_values: None,
            params: &[],
            outputs: &[OutputSpec {
                key: "band",
                ty: ValueType::String,
                description: "",
                nullable: true,
            }],
            dry_run_shape: None,
        };
        const LOGGER3: ActionDescriptor = ActionDescriptor {
            name: "local.log",
            label: "",
            description: "",
            needs_radio: false,
            transmits: false,
            writes_config: false,
            needs_internet: false,
            example_params: None,
            allowed_values: None,
            params: &[ParamSpec {
                key: "message",
                ty: ValueType::String,
                required: true,
                description: "",
                allowed: None,
                example: "\"x\"",
            }],
            outputs: &[],
            dry_run_shape: None,
        };
        let def = routine(vec![
            action_step("s1", "radio.connect", json!({})),
            action_step("s2", "local.log", json!({"message": "$s1.band"})),
        ]);
        let mut findings = Vec::new();
        check(
            &def,
            &StaticContext::new().with_action(CONNECTOR).with_action(LOGGER3),
            &mut findings,
        );
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].code, REF_NULLABLE_SOURCE);
        assert_eq!(findings[0].severity, Severity::Warning);
    }

    /// Engine-injected `_`-prefixed keys are reserved, not author params —
    /// never flagged as unknown.
    #[test]
    fn engine_reserved_underscore_keys_are_skipped() {
        let def = routine(vec![action_step(
            "s2",
            "radio.connect",
            json!({"stations": ["N0DAJ"], "_step_timeout_s": 30, "_run_id": "r"}),
        )]);
        assert!(run(&def).is_empty(), "{:?}", run(&def));
    }

    /// Params that aren't a JSON object at all (when a surface IS declared)
    /// can't deserialize — one error, no per-key cascade.
    #[test]
    fn non_object_params_is_single_type_error() {
        let def = routine(vec![action_step("s2", "radio.connect", json!(["N0DAJ"]))]);
        let findings = run(&def);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].code, PARAM_TYPE_MISMATCH);
    }
}
