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
}
