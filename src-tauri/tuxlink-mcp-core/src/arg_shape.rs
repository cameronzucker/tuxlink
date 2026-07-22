//! ONE parse-if-string rule at the MCP argument-decode boundary
//! (tuxlink-sq72z). Small models systematically emit composite-typed tool
//! params (JSON objects/arrays) as STRINGS containing the JSON — exam
//! transcript 1784598978430-0 (2026-07-20): `routines_step_update.patch`
//! rejected 11x and `routines_meta_set.patch` 6x with the model unable to
//! perceive the object/string-of-object difference in its own emission,
//! while `routines_save.def` (taught the same tolerance per-tool by #1205)
//! accepted first try. Wire origin is the model's own emission, not an
//! OpenAI-compat double-decode: the outer `function.arguments` string parses
//! once in the provider adapter, and sibling scalar params arrive correctly
//! typed while composite params arrive as JSON-in-a-string.
//!
//! The rule: a composite-typed param arriving as a string of valid JSON gets
//! exactly ONE parse, then flows into the normal validation — never a second
//! parse, never per-tool acceptance patches. Strings that do NOT parse to a
//! composite pass through untouched so every existing instructive error
//! still fires verbatim. The transcript telemetry marker
//! ([`string_coerced_params`] → the sink's `arg_shape` field) makes each
//! string-coerced call countable while the transcript keeps the redacted,
//! shape-preserved emission as the fine-tune target and regression metric:
//! the string-coercion rate per run trending to zero.
//!
//! This file also owns the SIBLING rule at the same boundary: branch-dialect
//! absorption ([`absorb_branch_dialect`], tuxlink-6epl8) — see its docs for
//! the battery evidence and the exact observed dialect inventory. Same
//! architecture: one deterministic rewrite where step objects enter, honest
//! refusals for everything outside the observed set, kind-precise transcript
//! markers ([`branch_dialect_params`] → the sink's `branch_dialect` field).

use serde_json::Value;

/// The declared composite kind of a coercible param — drives kind-exact
/// coercion and the transcript marker's vocabulary (tuxlink-hq3e2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositeKind {
    Object,
    Array,
}

/// `schema_with` target: an honest `{"type": "object"}` schema for the
/// composite params that were previously accept-anything `Value` schemas.
/// The advertised contract is the strict shape — the string tolerance is
/// deliberately NOT advertised (adrev pair-12 consensus); it lives at the
/// decode boundaries instead (runner validate.rs + [`parse_if_string`]).
pub fn object_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({ "type": "object" })
}

/// `schema_with` target: honest `{"type": "array"}` (see [`object_schema`]).
pub fn array_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({ "type": "array" })
}

/// The composite-typed (object- or array-valued) params of each routines
/// verb tool — the boundary's type knowledge, consumed by the port adapter's
/// coercion and the transcript sink's `arg_shape` marker. Scalar params
/// never coerce. Params whose CONTRACT is JSON-in-a-string
/// (`routines_save.def_json`) are deliberately absent: a string there is the
/// documented shape, not a model artifact.
pub fn composite_params(tool: &str) -> &'static [(&'static str, CompositeKind)] {
    use CompositeKind::{Array, Object};
    match tool {
        "routines_save" => &[("def", Object)],
        "routines_step_add" => &[("step", Object)],
        "routines_step_update" => &[("patch", Object)],
        "routines_trigger_set" => &[("triggers", Array)],
        "routines_meta_set" => &[("patch", Object)],
        _ => &[],
    }
}

/// The composite kind `v`'s string content parses to, when `v` is a string
/// of JSON — the model-emission shape the boundary coerces. `None` for
/// non-strings, non-JSON strings, and scalar-JSON strings.
pub fn stringified_kind(v: &Value) -> Option<CompositeKind> {
    match v {
        Value::String(s) => match serde_json::from_str::<Value>(s) {
            Ok(Value::Object(_)) => Some(CompositeKind::Object),
            Ok(Value::Array(_)) => Some(CompositeKind::Array),
            _ => None,
        },
        _ => None,
    }
}

/// The one coercion, kind-exact (tuxlink-hq3e2 tightening): a string
/// parsing to exactly the DECLARED composite kind becomes that composite;
/// everything else — genuine composites, scalars, strings that do not parse,
/// parse to a scalar, or parse to the WRONG kind — passes through untouched,
/// so downstream validation sees exactly what it saw before this rule
/// existed and its instructive errors fire verbatim.
pub fn parse_if_string(v: Value, declared: CompositeKind) -> Value {
    match v {
        Value::String(s) => match (serde_json::from_str::<Value>(&s), declared) {
            (Ok(parsed @ Value::Object(_)), CompositeKind::Object) => parsed,
            (Ok(parsed @ Value::Array(_)), CompositeKind::Array) => parsed,
            _ => Value::String(s),
        },
        other => other,
    }
}

/// The params of `tool` that arrived string-coerced in `args`, with the kind
/// their content parses to — the transcript's per-call `arg_shape` marker
/// (`string-to-object` / `string-to-array`). A string parsing to the WRONG
/// kind is still reported (the emission happened; the boundary will not
/// coerce it and the tool will reject it — both facts belong in telemetry).
/// Empty for well-shaped calls, unknown tools, and tools with no composite
/// params.
pub fn string_coerced_params(tool: &str, args: &Value) -> Vec<(&'static str, CompositeKind)> {
    composite_params(tool)
        .iter()
        .filter_map(|(p, _declared)| {
            args.get(*p).and_then(stringified_kind).map(|k| (*p, k))
        })
        .collect()
}

// ─── Branch-dialect absorption (tuxlink-6epl8) ──────────────────────────────

/// The condition-carrier keys the cross-model battery observed for
/// `Control::Branch` (bd tuxlink-hwgdi stage S1, 2026-07-21): 4/4 models
/// failed to author the real flat shape; glm-5.2 and sonnet-5 thrashed 7-11
/// invented dialects built from these carriers, at the step's top level and
/// nested inside `params`. This list is the CLOSED observed set — extending
/// it takes new battery evidence, not intuition. `tuxlink-routines`'
/// `edit.rs` mirrors it for its teaching refusal (that leaf crate cannot
/// depend on this boundary crate).
pub const BRANCH_CONDITION_CARRIERS: &[&str] = &["condition", "if", "when", "expr", "test"];

/// `CmpOp`'s wire names (`tuxlink-routines::types::CmpOp`), needed to
/// classify op-keyed conditions without a routines-crate dependency.
const CMP_OP_NAMES: &[&str] = &["eq", "ne", "lt", "lte", "gt", "gte"];

/// Every non-carrier key a branch step (or branch patch) may carry. A key in
/// neither list means the emission is outside the observed dialect set — the
/// absorber leaves the whole step alone and validation refuses honestly.
const BRANCH_KNOWN_KEYS: &[&str] = &["id", "control", "on", "op", "value", "then", "else", "params"];

/// Kind-precise transcript markers, one per observed condition shape plus
/// the `$`-strip, the `control: "if"` remap, and the inline-arm hoist - the
/// `branch_dialect` sibling of `arg_shape`'s `string-to-object` vocabulary.
pub const BRANCH_CONDITION_STRING: &str = "branch-condition-string";
pub const BRANCH_CONDITION_OBJECT: &str = "branch-condition-object";
pub const BRANCH_CONDITION_OPKEYED: &str = "branch-condition-opkeyed";
pub const BRANCH_CONDITION_REF: &str = "branch-condition-ref";
pub const BRANCH_REF_DOLLAR_STRIPPED: &str = "branch-ref-dollar-stripped";
pub const BRANCH_CONTROL_IF_MAPPED: &str = "branch-control-if-mapped";
pub const BRANCH_ARMS_HOISTED: &str = "branch-arms-hoisted";

/// Where a branch value entered: a WHOLE step object (`routines_save` defs,
/// `routines_step_add.step`) or a shallow-merge PATCH
/// (`routines_step_update.patch`). Patch is the one place a strict-boolean
/// absorption must write explicit `op: null` / `value: null`: null clears an
/// optional through the merge, where omission would leave a stale comparison
/// half on the stored step and silently flip the branch's semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchShape {
    WholeStep,
    Patch,
}

/// Strip exactly ONE leading `$` from a ref path. `None` when there is no
/// single-`$` prefix to strip: bare paths, a lone `"$"`, and `"$$…"` (a
/// second application would strip again, breaking idempotency) all decline.
fn strip_one_dollar(s: &str) -> Option<&str> {
    match s.strip_prefix('$') {
        Some(rest) if !rest.is_empty() && !rest.starts_with('$') => Some(rest),
        _ => None,
    }
}

/// Classify one observed condition-carrier VALUE into
/// `(path, op, value, marker)`. `None` = not an observed shape — the caller
/// leaves the step untouched. The four shapes are exactly the battery's
/// inventory: (a) bare string ref (strict-boolean), (b) `{field, op,
/// value}`, (c) op-keyed `{"eq": [ref, value]}` (JSONLogic-ish; op key ∈
/// eq/ne/lt/lte/gt/gte), (d) `{"ref": "$path"}` (gpt-5.5 seq 14,
/// strict-boolean).
#[allow(clippy::type_complexity)] // one internal classifier, named by its doc
fn classify_condition(
    cond: &Value,
) -> Option<(String, Option<String>, Option<Value>, &'static str)> {
    match cond {
        Value::String(s) if !s.is_empty() => {
            Some((s.clone(), None, None, BRANCH_CONDITION_STRING))
        }
        Value::Object(m) if m.len() == 3 => {
            let field = m.get("field")?.as_str()?;
            let op = m.get("op")?.as_str()?;
            let value = m.get("value")?;
            if field.is_empty() || !CMP_OP_NAMES.contains(&op) {
                return None;
            }
            Some((
                field.to_string(),
                Some(op.to_string()),
                Some(value.clone()),
                BRANCH_CONDITION_OBJECT,
            ))
        }
        Value::Object(m) if m.len() == 1 => {
            let (op, rhs) = m.iter().next()?;
            if op == "ref" {
                // gpt-5.5's `{"ref": "$s3.connected"}` wrapper: strict-bool
                // on the wrapped path.
                let path = rhs.as_str()?;
                if path.is_empty() {
                    return None;
                }
                return Some((path.to_string(), None, None, BRANCH_CONDITION_REF));
            }
            if !CMP_OP_NAMES.contains(&op.as_str()) {
                return None;
            }
            let pair = rhs.as_array()?;
            if pair.len() != 2 {
                return None;
            }
            let path = pair[0].as_str()?;
            if path.is_empty() {
                return None;
            }
            Some((
                path.to_string(),
                Some(op.clone()),
                Some(pair[1].clone()),
                BRANCH_CONDITION_OPKEYED,
            ))
        }
        _ => None,
    }
}

/// The ONE branch-dialect rule (tuxlink-6epl8, sibling of [`parse_if_string`]):
/// a step that EXPLICITLY says `control: "branch"` (or the invented
/// `control: "if"`, 2026-07-19 probe, remapped to `branch`) and carries an
/// observed condition dialect is rewritten to the real flat shape - carrier
/// keys (`condition`/`if`/`when`/`expr`/`test`, top level or inside
/// `params`) become `on`/`op`/`value`, `params`-nested `then`/`else` lift to
/// the step's top level, and a single leading `$` is stripped from the ref
/// path (also when `on` came directly). Returns the kind-precise markers
/// applied; empty = untouched.
///
/// Constraints, all load-bearing:
/// - A carrier key WITHOUT an explicit `control: "branch"` / `"if"` never
///   invents a branch - only the explicit discriminator activates the rule.
/// - Ambiguity refuses to guess: a carrier alongside `on`/`op`/`value`,
///   multiple carriers, keys beyond the observed set, a `params` holding
///   anything beyond one carrier plus `then`/`else`, arms present BOTH in
///   `params` and at top level, an unclassifiable carrier value, or an
///   unstrippable `$`-path all leave the step byte-identical so
///   validation's instructive refusal fires on the original emission.
/// - Idempotent: absorbed output re-enters and leaves untouched (`"if"` is
///   never a valid control kind, so the remap can never touch valid input).
/// - INLINE STEP OBJECTS inside arms are NOT absorbed here - a lone step
///   has no surrounding track to hoist them into. The whole-def walk
///   ([`absorb_branch_dialects_in_def`]) owns that absorption (glm-5.2
///   battery S1 seq 16-18 evidence).
pub fn absorb_branch_dialect(step: &mut Value, shape: BranchShape) -> Vec<&'static str> {
    let Some(obj) = step.as_object() else {
        return Vec::new();
    };
    let map_if = match obj.get("control").and_then(Value::as_str) {
        Some("branch") => false,
        Some("if") => true,
        _ => return Vec::new(),
    };
    if obj.keys().any(|k| {
        !BRANCH_KNOWN_KEYS.contains(&k.as_str()) && !BRANCH_CONDITION_CARRIERS.contains(&k.as_str())
    }) {
        return Vec::new();
    }
    // Locate the carrier: top level, or inside `params` (glm emitted
    // `params: {"if": "$s3.connected"}`; the 2026-07-19 probe put the whole
    // condition+then+else payload under `params`). `params` is not a branch
    // field, so a `params` holding anything beyond exactly one carrier plus
    // `then`/`else` is outside the observed set - as are arms present both
    // in `params` and at top level (ambiguous, no guessing).
    let top_carriers: Vec<&'static str> = BRANCH_CONDITION_CARRIERS
        .iter()
        .copied()
        .filter(|c| obj.contains_key(*c))
        .collect();
    let mut params_arms: Vec<(&'static str, Value)> = Vec::new();
    let params_carrier: Option<&'static str> = match obj.get("params") {
        None => None,
        Some(Value::Object(p)) => {
            let carriers: Vec<&'static str> = BRANCH_CONDITION_CARRIERS
                .iter()
                .copied()
                .filter(|c| p.contains_key(*c))
                .collect();
            let only_known = p.keys().all(|k| {
                k == "then"
                    || k == "else"
                    || BRANCH_CONDITION_CARRIERS.contains(&k.as_str())
            });
            match (carriers.as_slice(), only_known) {
                ([c], true) => {
                    for arm in ["then", "else"] {
                        if let Some(v) = p.get(arm) {
                            if obj.contains_key(arm) {
                                return Vec::new(); // arm in params AND top level
                            }
                            params_arms.push((arm, v.clone()));
                        }
                    }
                    Some(*c)
                }
                _ => return Vec::new(),
            }
        }
        Some(_) => return Vec::new(),
    };
    let carrier = match (top_carriers.as_slice(), params_carrier) {
        ([], None) => None,
        ([c], None) => Some((*c, false)),
        ([], Some(c)) => Some((c, true)),
        _ => return Vec::new(), // multiple carriers — ambiguous
    };

    let mut markers = Vec::new();
    match carrier {
        Some((key, in_params)) => {
            // A carrier alongside any flat condition field is a mixed,
            // never-observed emission — no guessing.
            if obj.contains_key("on") || obj.contains_key("op") || obj.contains_key("value") {
                return Vec::new();
            }
            let cond = if in_params {
                obj.get("params").and_then(|p| p.get(key))
            } else {
                obj.get(key)
            };
            let Some(cond) = cond else {
                return Vec::new();
            };
            let Some((raw_path, op, value, marker)) = classify_condition(cond) else {
                return Vec::new();
            };
            let (path, stripped) = match strip_one_dollar(&raw_path) {
                Some(rest) => (rest.to_string(), true),
                None if raw_path.starts_with('$') => return Vec::new(), // "$" / "$$…"
                None => (raw_path, false),
            };
            let obj = step.as_object_mut().expect("checked as_object above");
            if in_params {
                obj.remove("params");
                for (arm, v) in params_arms {
                    obj.insert(arm.into(), v);
                }
            } else {
                obj.remove(key);
            }
            obj.insert("on".into(), Value::String(path));
            match (op, value) {
                (Some(op), Some(value)) => {
                    obj.insert("op".into(), Value::String(op));
                    obj.insert("value".into(), value);
                }
                _ => {
                    // Strict-boolean: a PATCH must actively CLEAR any stored
                    // comparison halves; a whole step simply omits them.
                    if shape == BranchShape::Patch {
                        obj.insert("op".into(), Value::Null);
                        obj.insert("value".into(), Value::Null);
                    }
                }
            }
            if map_if {
                obj.insert("control".into(), Value::String("branch".into()));
                markers.push(BRANCH_CONTROL_IF_MAPPED);
            }
            markers.push(marker);
            if stripped {
                markers.push(BRANCH_REF_DOLLAR_STRIPPED);
            }
        }
        None => {
            // No carrier: the flat shape may still carry a `$`-prefixed `on`
            // (models emit the REAL field with the ref sigil), and the
            // control kind may still be the invented `"if"`.
            if let Some(on) = obj.get("on").and_then(Value::as_str) {
                if on.starts_with('$') && strip_one_dollar(on).is_none() {
                    return Vec::new(); // "$" / "$$…" - unstrippable, untouched
                }
            }
            let stripped = obj
                .get("on")
                .and_then(Value::as_str)
                .and_then(strip_one_dollar)
                .map(str::to_string);
            let obj = step.as_object_mut().expect("checked as_object above");
            if map_if {
                obj.insert("control".into(), Value::String("branch".into()));
                markers.push(BRANCH_CONTROL_IF_MAPPED);
            }
            if let Some(path) = stripped {
                obj.insert("on".into(), Value::String(path));
                markers.push(BRANCH_REF_DOLLAR_STRIPPED);
            }
        }
    }
    markers
}

/// What [`hoist_inline_arms`] decided for one (already-absorbed) step.
enum HoistOutcome {
    /// Arms hold no inline step objects (or this is not a flat branch) -
    /// nothing to do.
    NotApplicable,
    /// Arms held inline step objects; they were replaced by their id lists
    /// and the extracted steps (then-arm first, in emission order) must be
    /// spliced in immediately after the branch.
    Hoisted(Vec<Value>),
    /// Arms hold inline objects the rule refuses to guess at (an object
    /// without an `id`, a colliding id, a non-string non-object element) -
    /// the caller reverts the WHOLE step to its original bytes.
    Refused,
}

/// glm-5.2's inline-arm dialect (battery S1 seq 16-18): full step objects
/// inside a branch's `then`/`else` where the real shape wants step-id LISTS.
/// Only a surrounding track gives the objects somewhere to live, so this
/// runs exclusively from [`absorb_branch_dialects_in_def`]. The step must
/// already be in the flat branch vocabulary (any leftover carrier or
/// `params` means the condition absorption refused - do not hoist arms of a
/// step that will be refused anyway). Strings mixed among inline objects
/// stay as the ids they are.
fn hoist_inline_arms(
    step: &mut Value,
    used_ids: &mut std::collections::HashSet<String>,
) -> HoistOutcome {
    let Some(obj) = step.as_object() else {
        return HoistOutcome::NotApplicable;
    };
    if obj.get("control").and_then(Value::as_str) != Some("branch")
        || !obj.keys().all(|k| {
            matches!(k.as_str(), "id" | "control" | "on" | "op" | "value" | "then" | "else")
        })
    {
        return HoistOutcome::NotApplicable;
    }
    let has_inline = ["then", "else"].iter().any(|arm| {
        obj.get(*arm)
            .and_then(Value::as_array)
            .is_some_and(|a| a.iter().any(Value::is_object))
    });
    if !has_inline {
        return HoistOutcome::NotApplicable;
    }
    // Dry pass: every arm element must be a string (kept as an id) or an
    // object with a fresh, unique, non-empty string id.
    let mut hoisted_ids: Vec<String> = Vec::new();
    for arm in ["then", "else"] {
        let Some(items) = obj.get(arm).and_then(Value::as_array) else {
            continue;
        };
        for item in items {
            match item {
                Value::String(_) => {}
                Value::Object(m) => match m.get("id").and_then(Value::as_str) {
                    Some(id)
                        if !id.is_empty()
                            && !used_ids.contains(id)
                            && !hoisted_ids.iter().any(|h| h == id) =>
                    {
                        hoisted_ids.push(id.to_string());
                    }
                    _ => return HoistOutcome::Refused,
                },
                _ => return HoistOutcome::Refused,
            }
        }
    }
    // Commit: rewrite both arms to id lists, extract the objects in order.
    let mut extracted: Vec<Value> = Vec::new();
    let obj = step.as_object_mut().expect("checked as_object above");
    for arm in ["then", "else"] {
        let Some(items) = obj.get_mut(arm).and_then(Value::as_array_mut) else {
            continue;
        };
        for item in items.iter_mut() {
            if item.is_object() {
                let id = item
                    .get("id")
                    .and_then(Value::as_str)
                    .expect("dry pass verified the id")
                    .to_string();
                extracted.push(std::mem::replace(item, Value::String(id)));
            }
        }
    }
    used_ids.extend(hoisted_ids);
    HoistOutcome::Hoisted(extracted)
}

/// Apply [`absorb_branch_dialect`] to every step of a WHOLE routine
/// definition (`routines_save`), then hoist inline-arm step objects
/// ([`hoist_inline_arms`], glm-5.2 seq 16-18): extracted steps land in the
/// surrounding track immediately after their branch (then-arm first,
/// emission order preserved) and the arms become the id lists the real
/// shape wants. Hoisted steps re-enter the walk, so a hoisted step that is
/// itself a dialect branch absorbs too. Markers aggregate in document
/// order. No-guessing survives whole-step-wise: when an arm holds an
/// id-less or id-colliding object, the ENTIRE step reverts to its original
/// bytes (condition absorption included) so validation refuses the verbatim
/// emission. A def that is not object-shaped (or has no tracks) returns
/// empty and stays untouched - the parser's refusal fires.
pub fn absorb_branch_dialects_in_def(def: &mut Value) -> Vec<&'static str> {
    let mut markers = Vec::new();
    let Some(tracks) = def.get_mut("tracks").and_then(Value::as_array_mut) else {
        return markers;
    };
    // Def-wide id inventory for the hoist collision check (arms may
    // reference steps in the same track; ids must be unique per routine).
    let mut used_ids: std::collections::HashSet<String> = tracks
        .iter()
        .filter_map(|t| t.get("steps").and_then(Value::as_array))
        .flatten()
        .filter_map(|s| s.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    for track in tracks {
        let Some(steps) = track.get_mut("steps").and_then(Value::as_array_mut) else {
            continue;
        };
        let mut i = 0;
        while i < steps.len() {
            let original = steps[i].clone();
            let mut step_markers = absorb_branch_dialect(&mut steps[i], BranchShape::WholeStep);
            match hoist_inline_arms(&mut steps[i], &mut used_ids) {
                HoistOutcome::NotApplicable => {}
                HoistOutcome::Hoisted(extracted) => {
                    step_markers.push(BRANCH_ARMS_HOISTED);
                    for (offset, hoisted) in extracted.into_iter().enumerate() {
                        steps.insert(i + 1 + offset, hoisted);
                    }
                }
                HoistOutcome::Refused => {
                    steps[i] = original;
                    step_markers.clear();
                }
            }
            markers.append(&mut step_markers);
            i += 1;
        }
    }
    markers
}

/// The branch-dialect markers `tool`'s call absorbs, per param — the
/// transcript's per-call `branch_dialect` marker. Pure: clones and runs the
/// absorber exactly as the port boundary does (including the ONE
/// parse-if-string on a stringified composite first), so the marker cannot
/// drift from the behavior. Empty for well-shaped calls and other tools.
pub fn branch_dialect_params(tool: &str, args: &Value) -> Vec<(&'static str, Vec<&'static str>)> {
    fn step_markers(v: &Value, shape: BranchShape) -> Vec<&'static str> {
        let mut owned = parse_if_string(v.clone(), CompositeKind::Object);
        absorb_branch_dialect(&mut owned, shape)
    }
    fn def_markers(v: &Value) -> Vec<&'static str> {
        let mut owned = parse_if_string(v.clone(), CompositeKind::Object);
        absorb_branch_dialects_in_def(&mut owned)
    }
    let mut out = Vec::new();
    match tool {
        "routines_step_add" => {
            if let Some(step) = args.get("step") {
                let m = step_markers(step, BranchShape::WholeStep);
                if !m.is_empty() {
                    out.push(("step", m));
                }
            }
        }
        "routines_step_update" => {
            if let Some(patch) = args.get("patch") {
                let m = step_markers(patch, BranchShape::Patch);
                if !m.is_empty() {
                    out.push(("patch", m));
                }
            }
        }
        "routines_save" => {
            if let Some(def) = args.get("def") {
                let m = def_markers(def);
                if !m.is_empty() {
                    out.push(("def", m));
                }
            }
            if let Some(def_json) = args.get("def_json") {
                let m = def_markers(def_json);
                if !m.is_empty() {
                    out.push(("def_json", m));
                }
            }
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use CompositeKind::{Array, Object};

    /// Exam transcript 1784598978430-0 seq 5 — the exact `patch` shape
    /// `routines_step_update` rejected 11 times.
    const TRANSCRIPT_STEP_PATCH: &str =
        r#"{"params": {"message": "Finding closest 20m VARA CMS gateways"}}"#;

    /// Seq 33 — the exact `patch` shape `routines_meta_set` rejected.
    const TRANSCRIPT_META_PATCH: &str = r#"{"transmit_mode": "automatic"}"#;

    #[test]
    fn transcript_shapes_coerce_to_objects() {
        let got = parse_if_string(Value::String(TRANSCRIPT_STEP_PATCH.into()), Object);
        assert_eq!(
            got,
            json!({"params": {"message": "Finding closest 20m VARA CMS gateways"}})
        );
        let got = parse_if_string(Value::String(TRANSCRIPT_META_PATCH.into()), Object);
        assert_eq!(got, json!({"transmit_mode": "automatic"}));
    }

    #[test]
    fn stringified_array_coerces_for_triggers() {
        let got = parse_if_string(Value::String(r#"[{"type": "manual"}]"#.into()), Array);
        assert_eq!(got, json!([{"type": "manual"}]));
    }

    #[test]
    fn coercion_is_kind_exact() {
        // A string parsing to the WRONG declared kind passes through
        // untouched — the downstream typed error stays instructive.
        let arr_str = Value::String("[1, 2]".into());
        assert_eq!(parse_if_string(arr_str.clone(), Object), arr_str);
        let obj_str = Value::String(r#"{"a": 1}"#.into());
        assert_eq!(parse_if_string(obj_str.clone(), Array), obj_str);
    }

    #[test]
    fn non_json_and_scalar_json_strings_pass_through() {
        for s in ["nope", "", "  ", "{broken", "null", "42", "true", "\"quoted\""] {
            let v = Value::String(s.into());
            assert_eq!(parse_if_string(v.clone(), Object), v, "must not coerce {s:?}");
            assert_eq!(parse_if_string(v.clone(), Array), v, "must not coerce {s:?}");
            assert_eq!(stringified_kind(&v), None, "must not flag {s:?}");
        }
    }

    #[test]
    fn genuine_composites_and_scalars_untouched() {
        for v in [json!({"a": 1}), json!([1, 2]), json!(7), json!(true), Value::Null] {
            assert_eq!(parse_if_string(v.clone(), Object), v);
            assert_eq!(parse_if_string(v.clone(), Array), v);
            assert_eq!(stringified_kind(&v), None);
        }
        // stringified_kind reports genuine string-composites by their kind.
        assert_eq!(stringified_kind(&json!(r#"{"a": 1}"#)), Some(Object));
        assert_eq!(stringified_kind(&json!("[1]")), Some(Array));
    }

    #[test]
    fn string_coerced_params_flags_exact_transcript_calls_with_kind() {
        let args = json!({
            "patch": TRANSCRIPT_STEP_PATCH,
            "routine": "hourly-20m-vara-cms",
            "step_id": "s1"
        });
        assert_eq!(
            string_coerced_params("routines_step_update", &args),
            vec![("patch", Object)]
        );

        // A stringified ARRAY sent where triggers is declared: reported as
        // string-to-array (telemetry counts the emission even though the
        // declared kind matches here).
        let args = json!({"triggers": "[{\"type\": \"manual\"}]", "routine": "r"});
        assert_eq!(
            string_coerced_params("routines_trigger_set", &args),
            vec![("triggers", Array)]
        );

        // Wrong-kind emission still reported, with the PARSED kind.
        let args = json!({"patch": "[1, 2]", "routine": "r"});
        assert_eq!(
            string_coerced_params("routines_meta_set", &args),
            vec![("patch", Array)]
        );
    }

    #[test]
    fn def_json_contract_string_is_never_flagged() {
        let args = json!({"def_json": "{\"routine\": \"am-capture\"}"});
        assert!(string_coerced_params("routines_save", &args).is_empty());
    }

    #[test]
    fn well_shaped_calls_and_unknown_tools_flag_nothing() {
        let args = json!({"patch": {"transmit_mode": "automatic"}, "routine": "r"});
        assert!(string_coerced_params("routines_meta_set", &args).is_empty());
        assert!(string_coerced_params("routines_get", &args).is_empty());
        assert!(string_coerced_params("cms_connect", &json!({"x": "{\"y\":1}"})).is_empty());
    }

    // ── tuxlink-hq3e2: the table and the ADVERTISED schemas cannot drift ──
    // The coercion table's declared kinds are pinned against the actual
    // schemars-generated schemas of the param structs. A new composite param
    // (or a kind change) that touches one side without the other fails here.
    #[test]
    fn table_matches_generated_schemas() {
        fn declared_type(schema: &serde_json::Value, field: &str) -> String {
            schema["properties"][field]["type"]
                .as_str()
                .unwrap_or_else(|| panic!("{field} schema has no type: {schema}"))
                .to_string()
        }
        let save = serde_json::to_value(schemars::schema_for!(crate::router::RoutineSaveParams))
            .unwrap();
        let step_add =
            serde_json::to_value(schemars::schema_for!(crate::router::RoutineStepAddParams))
                .unwrap();
        let step_update =
            serde_json::to_value(schemars::schema_for!(crate::router::RoutineStepUpdateParams))
                .unwrap();
        let trigger_set =
            serde_json::to_value(schemars::schema_for!(crate::router::RoutineTriggerSetParams))
                .unwrap();
        let meta_set =
            serde_json::to_value(schemars::schema_for!(crate::router::RoutineMetaSetParams))
                .unwrap();

        let expected: &[(&str, &serde_json::Value, &str)] = &[
            ("routines_save", &save, "def"),
            ("routines_step_add", &step_add, "step"),
            ("routines_step_update", &step_update, "patch"),
            ("routines_trigger_set", &trigger_set, "triggers"),
            ("routines_meta_set", &meta_set, "patch"),
        ];
        for (tool, schema, field) in expected {
            let table = composite_params(tool);
            assert_eq!(table.len(), 1, "{tool} table entry count");
            let (name, kind) = table[0];
            assert_eq!(&name, field, "{tool} param name");
            let want = match kind {
                Object => "object",
                Array => "array",
            };
            assert_eq!(
                declared_type(schema, field),
                want,
                "{tool}.{field}: schema type must match the coercion table"
            );
        }
    }

    // ── tuxlink-6epl8: branch-dialect absorption (battery S1, 2026-07-21) ──

    /// Every observed top-level carrier key (glm-5.2's thrash inventory)
    /// rewrites to the flat strict-boolean shape with the `$` stripped.
    #[test]
    fn battery_carrier_keys_absorb_to_flat_on() {
        for carrier in ["condition", "if", "when", "expr", "test"] {
            let mut step = json!({
                "id": "s4", "control": "branch", "then": ["s5"], "else": ["s6"]
            });
            step.as_object_mut()
                .unwrap()
                .insert(carrier.into(), json!("$s3.connected"));
            let markers = absorb_branch_dialect(&mut step, BranchShape::WholeStep);
            assert_eq!(
                markers,
                vec![BRANCH_CONDITION_STRING, BRANCH_REF_DOLLAR_STRIPPED],
                "{carrier}"
            );
            assert_eq!(
                step,
                json!({
                    "id": "s4", "control": "branch", "on": "s3.connected",
                    "then": ["s5"], "else": ["s6"]
                }),
                "{carrier}"
            );
        }
    }

    /// glm-5.2 also nested the carrier inside `params` — absorbed only when
    /// `params` holds EXACTLY the carrier; the empty shell goes with it.
    #[test]
    fn glm_params_nested_if_absorbs() {
        let mut step = json!({
            "id": "s4", "control": "branch",
            "params": {"if": "$s3.connected"},
            "then": ["s5"], "else": ["s6"]
        });
        let markers = absorb_branch_dialect(&mut step, BranchShape::WholeStep);
        assert_eq!(
            markers,
            vec![BRANCH_CONDITION_STRING, BRANCH_REF_DOLLAR_STRIPPED]
        );
        assert!(step.get("params").is_none(), "{step}");
        assert_eq!(step["on"], "s3.connected");
    }

    /// Sonnet-5's condition-object dialect flattens to on/op/value. The
    /// empty arms are ALSO from the battery (sonnet emitted them empty in
    /// most probes, intending to populate after) — they must survive.
    #[test]
    fn sonnet_condition_object_absorbs() {
        let mut step = json!({
            "id": "s4", "control": "branch",
            "condition": {"field": "$s3.connected", "op": "eq", "value": true},
            "then": [], "else": []
        });
        let markers = absorb_branch_dialect(&mut step, BranchShape::WholeStep);
        assert_eq!(
            markers,
            vec![BRANCH_CONDITION_OBJECT, BRANCH_REF_DOLLAR_STRIPPED]
        );
        assert_eq!(
            step,
            json!({
                "id": "s4", "control": "branch", "on": "s3.connected",
                "op": "eq", "value": true, "then": [], "else": []
            })
        );
    }

    /// The JSONLogic-ish op-keyed form: `{"eq": [ref, value]}`.
    #[test]
    fn jsonlogic_opkeyed_condition_absorbs() {
        let mut step = json!({
            "id": "s4", "control": "branch",
            "condition": {"eq": ["$s3.connected", true]},
            "then": ["s5"], "else": []
        });
        let markers = absorb_branch_dialect(&mut step, BranchShape::WholeStep);
        assert_eq!(
            markers,
            vec![BRANCH_CONDITION_OPKEYED, BRANCH_REF_DOLLAR_STRIPPED]
        );
        assert_eq!(step["on"], "s3.connected");
        assert_eq!(step["op"], "eq");
        assert_eq!(step["value"], json!(true));
        assert!(step.get("condition").is_none());
    }

    /// Models also emit the REAL `on` field with the `$` sigil — strip it
    /// there too. Exactly one `$`: a lone "$" and "$$…" pass through (a
    /// second strip would break idempotency); bare paths are never touched.
    #[test]
    fn direct_on_dollar_prefix_strips() {
        let mut step = json!({
            "id": "s2", "control": "branch", "on": "$s1.connected",
            "then": ["s3"], "else": []
        });
        let markers = absorb_branch_dialect(&mut step, BranchShape::WholeStep);
        assert_eq!(markers, vec![BRANCH_REF_DOLLAR_STRIPPED]);
        assert_eq!(step["on"], "s1.connected");

        for odd in ["$", "$$s1.connected"] {
            let mut step = json!({
                "id": "s2", "control": "branch", "on": odd,
                "then": [], "else": []
            });
            assert!(absorb_branch_dialect(&mut step, BranchShape::WholeStep).is_empty());
            assert_eq!(step["on"], odd);
        }
    }

    /// Already-flat valid shapes are untouched, and the rule is IDEMPOTENT:
    /// absorbing an absorbed step is a marker-free no-op, in both contexts.
    #[test]
    fn already_flat_untouched_and_absorption_is_idempotent() {
        let flat = json!({
            "id": "s2", "control": "branch", "on": "s1.connected",
            "op": "gte", "value": 4, "then": ["s3"], "else": ["s4"]
        });
        let mut v = flat.clone();
        assert!(absorb_branch_dialect(&mut v, BranchShape::WholeStep).is_empty());
        assert_eq!(v, flat);

        let dialects = [
            (
                json!({"control": "branch", "if": "$s3.connected", "then": [], "else": []}),
                BranchShape::WholeStep,
            ),
            (
                json!({"control": "branch",
                       "condition": {"field": "$s3.connected", "op": "eq", "value": true},
                       "then": [], "else": []}),
                BranchShape::WholeStep,
            ),
            (
                json!({"control": "branch", "condition": {"lt": ["$s1.count", 3]},
                       "then": [], "else": []}),
                BranchShape::WholeStep,
            ),
            (
                json!({"control": "branch", "when": "$s3.connected"}),
                BranchShape::Patch,
            ),
            (
                json!({"control": "if", "on": "s1.ok", "then": [], "else": []}),
                BranchShape::WholeStep,
            ),
            (
                json!({"control": "branch", "condition": {"ref": "$s3.connected"},
                       "then": [], "else": []}),
                BranchShape::WholeStep,
            ),
        ];
        for (dialect, shape) in dialects {
            let mut once = dialect.clone();
            assert!(
                !absorb_branch_dialect(&mut once, shape).is_empty(),
                "first pass must absorb: {dialect}"
            );
            let after_once = once.clone();
            assert!(
                absorb_branch_dialect(&mut once, shape).is_empty(),
                "second pass must be a no-op: {after_once}"
            );
            assert_eq!(once, after_once, "second pass must not mutate");
        }
    }

    /// A carrier key with NO explicit `control: "branch"` never invents a
    /// branch — carrier-bearing patches and action steps pass through.
    #[test]
    fn carrier_without_explicit_branch_control_never_invents_a_branch() {
        let cases = [
            json!({"if": "$s3.connected", "then": ["s5"], "else": []}),
            json!({"id": "s1", "action": "local.log", "params": {"if": "$s3.connected"}}),
        ];
        for case in cases {
            let mut v = case.clone();
            assert!(
                absorb_branch_dialect(&mut v, BranchShape::Patch).is_empty(),
                "{case}"
            );
            assert_eq!(v, case);
        }
    }

    /// Ambiguous or out-of-inventory shapes stay byte-identical so
    /// validation's refusal fires on the original emission: mixed
    /// carrier+flat fields, multiple carriers, unknown keys, params with
    /// extra content, unclassifiable carrier values, wrong-arity pairs,
    /// unknown ops.
    #[test]
    fn ambiguous_and_unknown_shapes_pass_through_untouched() {
        let cases = [
            json!({"control": "branch", "on": "s1.x", "if": "$s2.y", "then": [], "else": []}),
            json!({"control": "branch", "if": "$s2.y", "op": "eq", "value": 1,
                   "then": [], "else": []}),
            json!({"control": "branch", "if": "$s2.y", "when": "$s2.z", "then": [], "else": []}),
            json!({"control": "branch", "if": "$s2.y", "note": "?", "then": [], "else": []}),
            json!({"control": "branch", "params": {"if": "$s2.y", "x": 1},
                   "then": [], "else": []}),
            json!({"control": "branch", "if": 7, "then": [], "else": []}),
            json!({"control": "branch", "if": "", "then": [], "else": []}),
            json!({"control": "branch", "if": {"field": "$s2.y"}, "then": [], "else": []}),
            json!({"control": "branch", "condition": {"eq": ["$s2.y"]}, "then": [], "else": []}),
            json!({"control": "branch", "condition": {"foo": ["$s2.y", 1]},
                   "then": [], "else": []}),
            json!({"control": "branch",
                   "condition": {"field": "$s2.y", "op": "approx", "value": 1},
                   "then": [], "else": []}),
        ];
        for case in cases {
            let mut v = case.clone();
            assert!(
                absorb_branch_dialect(&mut v, BranchShape::WholeStep).is_empty(),
                "{case}"
            );
            assert_eq!(v, case, "{case}");
        }
    }

    /// PATCH context: a strict-boolean carrier writes explicit `op`/`value`
    /// NULLS so the shallow merge CLEARS a stored comparison — omission
    /// would leave a stale half and silently flip the branch's semantics.
    /// WholeStep context omits the halves instead (serde defaults apply).
    #[test]
    fn patch_context_strict_boolean_clears_stale_op_value_with_nulls() {
        let mut patch = json!({"control": "branch", "condition": "$s3.connected"});
        let markers = absorb_branch_dialect(&mut patch, BranchShape::Patch);
        assert_eq!(
            markers,
            vec![BRANCH_CONDITION_STRING, BRANCH_REF_DOLLAR_STRIPPED]
        );
        assert_eq!(
            patch,
            json!({"control": "branch", "on": "s3.connected", "op": null, "value": null})
        );

        let mut step =
            json!({"control": "branch", "condition": "$s3.connected", "then": [], "else": []});
        absorb_branch_dialect(&mut step, BranchShape::WholeStep);
        assert!(step.get("op").is_none());
        assert!(step.get("value").is_none());
    }

    /// `routines_save` whole defs: the walk reaches every track's steps.
    #[test]
    fn save_def_walk_absorbs_branch_steps_in_tracks() {
        let mut def = json!({
            "routine": "s1-cycle", "schema_version": 1, "transmit_mode": "attended",
            "triggers": [{"type": "manual"}],
            "tracks": [{"name": "main", "steps": [
                {"id": "s1", "action": "radio.connect", "params": {}},
                {"id": "s2", "control": "branch", "when": "$s1.connected",
                 "then": ["s3"], "else": []}
            ]}]
        });
        let markers = absorb_branch_dialects_in_def(&mut def);
        assert_eq!(
            markers,
            vec![BRANCH_CONDITION_STRING, BRANCH_REF_DOLLAR_STRIPPED]
        );
        assert_eq!(def["tracks"][0]["steps"][1]["on"], "s1.connected");
        assert!(def["tracks"][0]["steps"][1].get("when").is_none());

        let mut not_a_def = json!("nope");
        assert!(absorb_branch_dialects_in_def(&mut not_a_def).is_empty());
        assert_eq!(not_a_def, json!("nope"));
    }

    /// The transcript's `branch_dialect` marker: kind-precise, per param,
    /// stacking with the parse-if-string rule on stringified emissions.
    #[test]
    fn branch_dialect_params_reports_kind_precise_markers() {
        // step_add with the condition-object dialect, STRINGIFIED — both
        // boundary rules apply: the ONE parse, then the absorption markers.
        let args = json!({
            "routine": "r",
            "track": "main",
            "step": "{\"control\": \"branch\", \"condition\": {\"field\": \"$s3.connected\", \"op\": \"eq\", \"value\": true}, \"then\": [], \"else\": []}"
        });
        assert_eq!(
            branch_dialect_params("routines_step_add", &args),
            vec![(
                "step",
                vec![BRANCH_CONDITION_OBJECT, BRANCH_REF_DOLLAR_STRIPPED]
            )]
        );

        let args = json!({
            "routine": "r", "step_id": "s2",
            "patch": {"control": "branch", "if": "$s3.connected"}
        });
        assert_eq!(
            branch_dialect_params("routines_step_update", &args),
            vec![(
                "patch",
                vec![BRANCH_CONDITION_STRING, BRANCH_REF_DOLLAR_STRIPPED]
            )]
        );

        let args = json!({"def": {"routine": "r", "schema_version": 1, "tracks": [
            {"name": "m", "steps": [
                {"id": "s2", "control": "branch", "test": "$s1.ok", "then": [], "else": []}
            ]}
        ]}});
        assert_eq!(
            branch_dialect_params("routines_save", &args),
            vec![(
                "def",
                vec![BRANCH_CONDITION_STRING, BRANCH_REF_DOLLAR_STRIPPED]
            )]
        );

        // Clean calls, other tools: nothing.
        let clean =
            json!({"step": {"id": "s2", "control": "branch", "on": "s1.ok", "then": [], "else": []}});
        assert!(branch_dialect_params("routines_step_add", &clean).is_empty());
        assert!(branch_dialect_params("cms_connect", &args).is_empty());
    }

    /// gpt-5.5 seq 14 verbatim: the `{"ref": "$path"}` wrapper is a
    /// strict-boolean condition; a non-string or empty `ref` refuses.
    #[test]
    fn gpt_ref_wrapper_condition_absorbs() {
        let mut step = json!({
            "condition": {"ref": "$s3.connected"}, "control": "branch",
            "else": [], "then": []
        });
        let markers = absorb_branch_dialect(&mut step, BranchShape::WholeStep);
        assert_eq!(markers, vec![BRANCH_CONDITION_REF, BRANCH_REF_DOLLAR_STRIPPED]);
        assert_eq!(
            step,
            json!({"control": "branch", "on": "s3.connected", "then": [], "else": []})
        );

        for bad in [json!({"ref": 7}), json!({"ref": ""}), json!({"ref": ["$s3.x"]})] {
            let mut step = json!({"control": "branch", "condition": bad, "then": [], "else": []});
            let before = step.clone();
            assert!(absorb_branch_dialect(&mut step, BranchShape::WholeStep).is_empty());
            assert_eq!(step, before);
        }
    }

    /// Sonnet-5 seq 27 verbatim: `"condition": true` is NOT mappable - the
    /// step passes through byte-identical so validation refuses the honest
    /// original (there is no path to branch on).
    #[test]
    fn sonnet_literal_condition_true_passes_through_verbatim() {
        let mut step = json!({
            "condition": true, "control": "branch", "else": [], "id": "s4", "then": []
        });
        let before = step.clone();
        assert!(absorb_branch_dialect(&mut step, BranchShape::WholeStep).is_empty());
        assert_eq!(step, before, "must stay byte-identical");
    }

    /// The 2026-07-19 probe's invented `control: "if"` maps to `"branch"`:
    /// bare, with a flat `$`-prefixed `on`, and with the whole
    /// condition+then+else payload nested under `params` (the probe's exact
    /// carrier layout, with id-list arms). A carrier WITHOUT the explicit
    /// `branch`/`if` discriminator still never invents a branch.
    #[test]
    fn control_if_maps_to_branch() {
        let mut step = json!({"control": "if", "on": "s1.ok", "then": ["s2"], "else": []});
        assert_eq!(
            absorb_branch_dialect(&mut step, BranchShape::WholeStep),
            vec![BRANCH_CONTROL_IF_MAPPED]
        );
        assert_eq!(
            step,
            json!({"control": "branch", "on": "s1.ok", "then": ["s2"], "else": []})
        );

        let mut step = json!({"control": "if", "on": "$s1.ok", "then": [], "else": []});
        assert_eq!(
            absorb_branch_dialect(&mut step, BranchShape::WholeStep),
            vec![BRANCH_CONTROL_IF_MAPPED, BRANCH_REF_DOLLAR_STRIPPED]
        );
        assert_eq!(step["control"], "branch");
        assert_eq!(step["on"], "s1.ok");

        let mut step = json!({
            "control": "if", "id": "s4",
            "params": {"condition": "$s2.success", "then": ["s5"], "else": ["s6"]}
        });
        assert_eq!(
            absorb_branch_dialect(&mut step, BranchShape::WholeStep),
            vec![
                BRANCH_CONTROL_IF_MAPPED,
                BRANCH_CONDITION_STRING,
                BRANCH_REF_DOLLAR_STRIPPED
            ]
        );
        assert_eq!(
            step,
            json!({
                "control": "branch", "id": "s4", "on": "s2.success",
                "then": ["s5"], "else": ["s6"]
            })
        );

        // Arms in params AND at top level: ambiguous, untouched.
        let mut step = json!({
            "control": "if", "then": ["s9"],
            "params": {"condition": "$s2.success", "then": ["s5"]}
        });
        let before = step.clone();
        assert!(absorb_branch_dialect(&mut step, BranchShape::WholeStep).is_empty());
        assert_eq!(step, before);

        // No discriminator: no invented branch.
        let mut step = json!({"if": "$s2.success", "then": [], "else": []});
        let before = step.clone();
        assert!(absorb_branch_dialect(&mut step, BranchShape::Patch).is_empty());
        assert_eq!(step, before);
    }

    /// glm-5.2 seq 16 structural: the whole `routines_save` def with a
    /// carrier condition AND inline step objects in both arms. The def walk
    /// flattens the condition, hoists the inline steps into the track right
    /// after the branch (then-arm first, order preserved), and rewrites the
    /// arms as id lists. Second pass: no-op (idempotent).
    #[test]
    fn glm_inline_arm_def_absorbs_and_hoists() {
        let mut def = json!({
            "routine": "gateway-check-4h", "schema_version": 1,
            "transmit_mode": "attended", "triggers": [{"type": "manual"}],
            "tracks": [{"name": "track-1", "steps": [
                {"action": "data.find_stations", "id": "s1", "on_radio_busy": "wait",
                 "params": {"bands": ["20m"], "limit": 3, "modes": ["vara-hf"]}},
                {"action": "radio.connect", "id": "s3", "on_radio_busy": "wait",
                 "params": {"bands": ["20m"], "stations": "$s1.callsigns"}},
                {"condition": "$s3.connected", "control": "branch",
                 "else": [
                    {"action": "radio.aprs_send", "id": "s6",
                     "params": {"text": "No gateway was reachable this cycle"}},
                    {"action": "local.log", "id": "s7",
                     "params": {"message": "no gateway reachable, APRS alert sent"}}
                 ],
                 "id": "s4",
                 "then": [
                    {"action": "local.log", "id": "s5",
                     "params": {"message": "connected to a 20m VARA gateway"}}
                 ]},
                {"control": "end", "failed": false, "id": "s2"}
            ]}]
        });
        let markers = absorb_branch_dialects_in_def(&mut def);
        assert_eq!(
            markers,
            vec![
                BRANCH_CONDITION_STRING,
                BRANCH_REF_DOLLAR_STRIPPED,
                BRANCH_ARMS_HOISTED
            ]
        );
        let steps = def["tracks"][0]["steps"].as_array().unwrap();
        let ids: Vec<&str> = steps.iter().map(|s| s["id"].as_str().unwrap()).collect();
        assert_eq!(
            ids,
            vec!["s1", "s3", "s4", "s5", "s6", "s7", "s2"],
            "then-arm steps first, right after the branch: {def}"
        );
        let branch = &steps[2];
        assert_eq!(branch["on"], "s3.connected");
        assert!(branch.get("condition").is_none());
        assert_eq!(branch["then"], json!(["s5"]));
        assert_eq!(branch["else"], json!(["s6", "s7"]));
        assert_eq!(steps[3]["action"], "local.log", "hoisted step kept whole");
        assert_eq!(steps[4]["action"], "radio.aprs_send");

        // Idempotent: the absorbed def re-enters and leaves untouched.
        let after_once = def.clone();
        assert!(absorb_branch_dialects_in_def(&mut def).is_empty());
        assert_eq!(def, after_once);
    }

    /// No-guessing on arms: an inline object without an `id`, or one whose
    /// id collides with an existing step, reverts the WHOLE step - the
    /// condition absorption included - so validation refuses the verbatim
    /// emission. A lone step (step_add/step_update) never hoists: there is
    /// no track to hoist into.
    #[test]
    fn inline_arm_hoisting_refuses_without_ids_and_never_runs_on_lone_steps() {
        let idless = json!({
            "routine": "r", "schema_version": 1, "transmit_mode": "attended",
            "triggers": [{"type": "manual"}],
            "tracks": [{"name": "main", "steps": [
                {"id": "s1", "action": "radio.connect", "params": {}},
                {"id": "s2", "control": "branch", "when": "$s1.connected",
                 "then": [{"action": "local.log", "params": {"message": "hi"}}],
                 "else": []}
            ]}]
        });
        let mut def = idless.clone();
        assert!(
            absorb_branch_dialects_in_def(&mut def).is_empty(),
            "id-less inline arm reverts the whole step"
        );
        assert_eq!(def, idless);

        let colliding = json!({
            "routine": "r", "schema_version": 1, "transmit_mode": "attended",
            "triggers": [{"type": "manual"}],
            "tracks": [{"name": "main", "steps": [
                {"id": "s1", "action": "radio.connect", "params": {}},
                {"id": "s2", "control": "branch", "on": "s1.connected",
                 "then": [{"id": "s1", "action": "local.log", "params": {}}],
                 "else": []}
            ]}]
        });
        let mut def = colliding.clone();
        assert!(absorb_branch_dialects_in_def(&mut def).is_empty());
        assert_eq!(def, colliding);

        // Lone step through the fragment path: inline arms pass through
        // verbatim (the condition still absorbs).
        let mut step = json!({
            "control": "branch", "condition": "$s1.connected",
            "then": [{"id": "s5", "action": "local.log", "params": {}}], "else": []
        });
        let markers = absorb_branch_dialect(&mut step, BranchShape::WholeStep);
        assert_eq!(
            markers,
            vec![BRANCH_CONDITION_STRING, BRANCH_REF_DOLLAR_STRIPPED]
        );
        assert_eq!(
            step["then"],
            json!([{"id": "s5", "action": "local.log", "params": {}}]),
            "no track, no hoist"
        );
    }

    /// Mixed arms (id strings alongside inline objects) hoist only the
    /// objects and keep the strings in place.
    #[test]
    fn mixed_inline_and_id_arms_hoist_only_the_objects() {
        let mut def = json!({
            "routine": "r", "schema_version": 1, "transmit_mode": "attended",
            "triggers": [{"type": "manual"}],
            "tracks": [{"name": "main", "steps": [
                {"id": "s1", "action": "radio.connect", "params": {}},
                {"id": "s2", "control": "branch", "on": "s1.connected",
                 "then": ["s9", {"id": "s5", "action": "local.log", "params": {}}],
                 "else": []},
                {"id": "s9", "action": "local.log", "params": {}}
            ]}]
        });
        let markers = absorb_branch_dialects_in_def(&mut def);
        assert_eq!(markers, vec![BRANCH_ARMS_HOISTED]);
        let steps = def["tracks"][0]["steps"].as_array().unwrap();
        let ids: Vec<&str> = steps.iter().map(|s| s["id"].as_str().unwrap()).collect();
        assert_eq!(ids, vec!["s1", "s2", "s5", "s9"]);
        assert_eq!(steps[1]["then"], json!(["s9", "s5"]));
    }
}
