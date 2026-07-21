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
//! ([`string_coerced_params`] → the sink's `arg_shape` field) preserves each
//! raw stringified emission as the fine-tune target and regression metric:
//! the string-coercion rate per run trending to zero.

use serde_json::Value;

/// The composite-typed (object- or array-valued) params of each routines
/// verb tool — the boundary's type knowledge, consumed by the port adapter's
/// coercion and the transcript sink's `arg_shape` marker. Scalar params
/// never coerce. Params whose CONTRACT is JSON-in-a-string
/// (`routines_save.def_json`) are deliberately absent: a string there is the
/// documented shape, not a model artifact.
pub fn composite_params(tool: &str) -> &'static [&'static str] {
    match tool {
        "routines_save" => &["def"],
        "routines_step_add" => &["step"],
        "routines_step_update" => &["patch"],
        "routines_trigger_set" => &["triggers"],
        "routines_meta_set" => &["patch"],
        _ => &[],
    }
}

/// True when `v` is a string whose content parses as one JSON composite
/// (object or array) — the model-emission shape the boundary coerces.
pub fn is_stringified_composite(v: &Value) -> bool {
    match v {
        Value::String(s) => matches!(
            serde_json::from_str::<Value>(s),
            Ok(Value::Object(_)) | Ok(Value::Array(_))
        ),
        _ => false,
    }
}

/// The one coercion: a string parsing to a JSON composite becomes that
/// composite; everything else — genuine composites, scalars, strings that do
/// not parse or parse to a scalar — passes through untouched, so downstream
/// validation sees exactly what it saw before this rule existed.
pub fn parse_if_string(v: Value) -> Value {
    match v {
        Value::String(s) => match serde_json::from_str::<Value>(&s) {
            Ok(parsed) if parsed.is_object() || parsed.is_array() => parsed,
            _ => Value::String(s),
        },
        other => other,
    }
}

/// The params of `tool` that arrived string-coerced in `args` — the content
/// of the transcript's per-call `arg_shape` marker. Empty for well-shaped
/// calls, unknown tools, and tools with no composite params.
pub fn string_coerced_params(tool: &str, args: &Value) -> Vec<&'static str> {
    composite_params(tool)
        .iter()
        .filter(|p| args.get(**p).is_some_and(is_stringified_composite))
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Exam transcript 1784598978430-0 seq 5 — the exact `patch` shape
    /// `routines_step_update` rejected 11 times.
    const TRANSCRIPT_STEP_PATCH: &str =
        r#"{"params": {"message": "Finding closest 20m VARA CMS gateways"}}"#;

    /// Seq 33 — the exact `patch` shape `routines_meta_set` rejected.
    const TRANSCRIPT_META_PATCH: &str = r#"{"transmit_mode": "automatic"}"#;

    #[test]
    fn transcript_shapes_coerce_to_objects() {
        let got = parse_if_string(Value::String(TRANSCRIPT_STEP_PATCH.into()));
        assert_eq!(
            got,
            json!({"params": {"message": "Finding closest 20m VARA CMS gateways"}})
        );
        let got = parse_if_string(Value::String(TRANSCRIPT_META_PATCH.into()));
        assert_eq!(got, json!({"transmit_mode": "automatic"}));
    }

    #[test]
    fn stringified_array_coerces_for_triggers() {
        let got = parse_if_string(Value::String(r#"[{"type": "manual"}]"#.into()));
        assert_eq!(got, json!([{"type": "manual"}]));
    }

    #[test]
    fn non_json_and_scalar_json_strings_pass_through() {
        // Not JSON at all: the downstream instructive error must see the
        // original string.
        for s in ["nope", "", "  ", "{broken", "null", "42", "true", "\"quoted\""] {
            let v = Value::String(s.into());
            assert_eq!(parse_if_string(v.clone()), v, "must not coerce {s:?}");
            assert!(!is_stringified_composite(&v), "must not flag {s:?}");
        }
    }

    #[test]
    fn genuine_composites_and_scalars_untouched() {
        for v in [
            json!({"a": 1}),
            json!([1, 2]),
            json!(7),
            json!(true),
            Value::Null,
        ] {
            assert_eq!(parse_if_string(v.clone()), v);
            assert!(!is_stringified_composite(&v));
        }
    }

    #[test]
    fn string_coerced_params_flags_exact_transcript_calls() {
        // Seq 5: scalar siblings arrive correctly typed; only patch flags.
        let args = json!({
            "patch": TRANSCRIPT_STEP_PATCH,
            "routine": "hourly-20m-vara-cms",
            "step_id": "s1"
        });
        assert_eq!(
            string_coerced_params("routines_step_update", &args),
            vec!["patch"]
        );

        // Seq 33.
        let args = json!({"patch": TRANSCRIPT_META_PATCH, "routine": "hourly-20m-vara-cms"});
        assert_eq!(string_coerced_params("routines_meta_set", &args), vec!["patch"]);

        // Seq 3: a stringified def IS a coerced emission (the #1205 class).
        let args = json!({"def": "{\"routine\": \"hourly-20m-vara-cms\", \"tracks\": []}"});
        assert_eq!(string_coerced_params("routines_save", &args), vec!["def"]);
    }

    #[test]
    fn def_json_contract_string_is_never_flagged() {
        // def_json is JSON-in-a-string BY CONTRACT — per-contract usage is
        // not a coerced emission and must not inflate the metric.
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
}
