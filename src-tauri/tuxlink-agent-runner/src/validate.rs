//! A minimal JSON-Schema validator for tool arguments (COR-3, T5).
//!
//! This is deliberately NOT a full draft-07 implementation — a new external
//! `jsonschema` dependency would add a cold-compile + lockfile surface for a
//! narrow need. It covers the keywords tool-argument schemas actually use:
//! `type` (object/array/string/number/integer/boolean/null), `required`,
//! `properties`, `items`, `enum`, `additionalProperties: false`, and numeric
//! `minimum`/`maximum`. Unknown keywords are ignored (permissive), so a schema
//! using a feature we don't model never *falsely rejects* a valid call — the
//! invoker's own validation (`ToolOutcome::InvalidArgs`) is the backstop.
//!
//! The validator returns the FIRST error found, with a JSON-pointer-ish path so
//! the message fed back to the model (COR-3 re-prompt) is actionable.

use serde_json::Value;

/// Validate `instance` against `schema`. `Ok(())` means valid; `Err(msg)` is a
/// human-readable, model-facing validation error.
pub fn validate(schema: &Value, instance: &Value) -> Result<(), String> {
    validate_at("", schema, instance)
}

fn validate_at(path: &str, schema: &Value, instance: &Value) -> Result<(), String> {
    // A non-object schema (e.g. `true`) accepts anything.
    let obj = match schema.as_object() {
        Some(o) => o,
        None => return Ok(()),
    };

    if let Some(ty) = obj.get("type") {
        check_type(path, ty, instance)?;
    }

    if let Some(enum_vals) = obj.get("enum").and_then(Value::as_array) {
        if !enum_vals.iter().any(|v| v == instance) {
            return Err(format!(
                "{}: value {} is not one of the allowed enum values",
                loc(path),
                compact(instance),
            ));
        }
    }

    // Numeric bounds (apply only to numbers).
    if let Some(n) = instance.as_f64() {
        if let Some(min) = obj.get("minimum").and_then(Value::as_f64) {
            if n < min {
                return Err(format!("{}: {n} is less than minimum {min}", loc(path)));
            }
        }
        if let Some(max) = obj.get("maximum").and_then(Value::as_f64) {
            if n > max {
                return Err(format!("{}: {n} is greater than maximum {max}", loc(path)));
            }
        }
    }

    // Object constraints.
    if let Some(map) = instance.as_object() {
        let properties = obj.get("properties").and_then(Value::as_object);

        if let Some(required) = obj.get("required").and_then(Value::as_array) {
            for req in required {
                if let Some(key) = req.as_str() {
                    if !map.contains_key(key) {
                        return Err(format!(
                            "{}: missing required property `{key}`",
                            loc(path)
                        ));
                    }
                }
            }
        }

        // additionalProperties: false rejects keys not named in `properties`.
        let additional_allowed = !matches!(obj.get("additionalProperties"), Some(Value::Bool(false)));

        for (key, child) in map {
            match properties.and_then(|p| p.get(key)) {
                Some(child_schema) => {
                    validate_at(&join(path, key), child_schema, child)?;
                }
                None if !additional_allowed => {
                    return Err(format!(
                        "{}: unexpected property `{key}` (additionalProperties is false)",
                        loc(path)
                    ));
                }
                None => {}
            }
        }
    }

    // Array constraints.
    if let (Some(arr), Some(items_schema)) = (instance.as_array(), obj.get("items")) {
        for (i, item) in arr.iter().enumerate() {
            validate_at(&join(path, &i.to_string()), items_schema, item)?;
        }
    }

    Ok(())
}

/// Validate the `type` keyword, which may be a single string or an array of
/// allowed types (draft-07 union).
fn check_type(path: &str, ty: &Value, instance: &Value) -> Result<(), String> {
    let matches_one = |name: &str| type_matches(name, instance);

    let ok = match ty {
        Value::String(name) => matches_one(name),
        Value::Array(names) => names
            .iter()
            .filter_map(Value::as_str)
            .any(matches_one),
        // Malformed `type` keyword: don't false-reject.
        _ => true,
    };

    if ok {
        Ok(())
    } else {
        Err(format!(
            "{}: expected type {}, got {}",
            loc(path),
            compact(ty),
            json_type_name(instance),
        ))
    }
}

fn type_matches(name: &str, instance: &Value) -> bool {
    match name {
        "object" => instance.is_object(),
        "array" => instance.is_array(),
        "string" => instance.is_string(),
        "boolean" => instance.is_boolean(),
        "null" => instance.is_null(),
        // JSON Schema: `integer` requires a whole number; `number` is any.
        "number" => instance.is_number(),
        "integer" => {
            if instance.as_i64().is_some() || instance.as_u64().is_some() {
                true
            } else if let Some(f) = instance.as_f64() {
                // JSON numbers like 3.0 are integral; 3.5 is not.
                f.fract() == 0.0
            } else {
                false
            }
        }
        // Unknown type name: be permissive.
        _ => true,
    }
}

fn json_type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Render a value compactly for error messages, truncating very long strings.
fn compact(v: &Value) -> String {
    let s = v.to_string();
    if s.len() > 80 {
        format!("{}…", &s[..80])
    } else {
        s
    }
}

fn loc(path: &str) -> String {
    if path.is_empty() {
        "(root)".to_string()
    } else {
        path.to_string()
    }
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        format!("/{key}")
    } else {
        format!("{path}/{key}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn accepts_valid_object() {
        let schema = json!({
            "type": "object",
            "required": ["station"],
            "properties": { "station": { "type": "string" } }
        });
        assert!(validate(&schema, &json!({"station": "W1AW"})).is_ok());
    }

    #[test]
    fn rejects_missing_required() {
        let schema = json!({
            "type": "object",
            "required": ["station"],
            "properties": { "station": { "type": "string" } }
        });
        let err = validate(&schema, &json!({})).unwrap_err();
        assert!(err.contains("station"), "msg was: {err}");
    }

    #[test]
    fn rejects_wrong_type() {
        let schema = json!({
            "type": "object",
            "properties": { "count": { "type": "integer" } }
        });
        let err = validate(&schema, &json!({"count": "not a number"})).unwrap_err();
        assert!(err.contains("count"), "msg was: {err}");
    }

    #[test]
    fn integer_rejects_fractional() {
        let schema = json!({ "type": "integer" });
        assert!(validate(&schema, &json!(3.5)).is_err());
        assert!(validate(&schema, &json!(3)).is_ok());
        assert!(validate(&schema, &json!(3.0)).is_ok());
    }

    #[test]
    fn enum_constraint() {
        let schema = json!({ "enum": ["a", "b"] });
        assert!(validate(&schema, &json!("a")).is_ok());
        assert!(validate(&schema, &json!("c")).is_err());
    }

    #[test]
    fn additional_properties_false() {
        let schema = json!({
            "type": "object",
            "properties": { "x": { "type": "string" } },
            "additionalProperties": false
        });
        assert!(validate(&schema, &json!({"x": "ok"})).is_ok());
        let err = validate(&schema, &json!({"x": "ok", "y": 1})).unwrap_err();
        assert!(err.contains("y"), "msg was: {err}");
    }

    #[test]
    fn numeric_bounds() {
        let schema = json!({ "type": "number", "minimum": 0, "maximum": 10 });
        assert!(validate(&schema, &json!(5)).is_ok());
        assert!(validate(&schema, &json!(-1)).is_err());
        assert!(validate(&schema, &json!(11)).is_err());
    }

    #[test]
    fn nested_and_array_items() {
        let schema = json!({
            "type": "object",
            "properties": {
                "tags": { "type": "array", "items": { "type": "string" } }
            }
        });
        assert!(validate(&schema, &json!({"tags": ["a", "b"]})).is_ok());
        let err = validate(&schema, &json!({"tags": ["a", 2]})).unwrap_err();
        assert!(err.contains("tags"), "msg was: {err}");
    }

    #[test]
    fn unknown_keyword_is_permissive() {
        // A schema using a keyword we don't model must not false-reject.
        let schema = json!({ "type": "string", "pattern": "^[A-Z]+$" });
        assert!(validate(&schema, &json!("anything")).is_ok());
    }

    #[test]
    fn non_object_schema_accepts_anything() {
        assert!(validate(&json!(true), &json!({"x": 1})).is_ok());
    }
}
