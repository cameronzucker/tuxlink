//! Run-start snapshot resolution (spec §7): a run executes a fully-resolved
//! copy of its definition — library edits cannot mutate in-flight runs, and
//! exported run bundles are self-contained.

use async_trait::async_trait;

use crate::error::SnapshotError;
use crate::refs::EntityRef;
use crate::types::RoutineDef;

#[async_trait]
pub trait EntityResolver: Send + Sync {
    async fn resolve(&self, entity: &EntityRef) -> Result<serde_json::Value, SnapshotError>;
}

fn walk_refs(value: &serde_json::Value, out: &mut Vec<EntityRef>) {
    match value {
        serde_json::Value::String(s) => {
            if let Some(r) = EntityRef::parse(s) {
                out.push(r);
            }
        }
        serde_json::Value::Array(items) => items.iter().for_each(|v| walk_refs(v, out)),
        serde_json::Value::Object(map) => map.values().for_each(|v| walk_refs(v, out)),
        _ => {}
    }
}

/// Every `@`-token in the definition (plan 3's reference validator reuses this).
pub fn collect_refs(def: &RoutineDef) -> Vec<EntityRef> {
    let json = serde_json::to_value(def).expect("RoutineDef serializes");
    let mut out = Vec::new();
    walk_refs(&json, &mut out);
    out
}

async fn substitute(
    value: serde_json::Value,
    resolver: &dyn EntityResolver,
) -> Result<serde_json::Value, SnapshotError> {
    // Recursion via explicit stack-free style: JSON depth here is authoring
    // depth (small); Box::pin keeps the async recursion object-safe.
    match value {
        serde_json::Value::String(s) => match EntityRef::parse(&s) {
            Some(r) => resolver.resolve(&r).await.map_err(|e| match e {
                SnapshotError::UnresolvedRef(_) => SnapshotError::UnresolvedRef(s),
                other => other,
            }),
            None => Ok(serde_json::Value::String(s)),
        },
        serde_json::Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for v in items {
                out.push(Box::pin(substitute(v, resolver)).await?);
            }
            Ok(serde_json::Value::Array(out))
        }
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                out.insert(k, Box::pin(substitute(v, resolver)).await?);
            }
            Ok(serde_json::Value::Object(out))
        }
        other => Ok(other),
    }
}

/// Resolve every `@`-token in the definition; the result is what
/// `RunEvent::RunStarted.snapshot` records and what the executor runs.
pub async fn resolve_snapshot(
    def: &RoutineDef,
    resolver: &dyn EntityResolver,
) -> Result<serde_json::Value, SnapshotError> {
    let json = serde_json::to_value(def).expect("RoutineDef serializes");
    substitute(json, resolver).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fakes::FakeResolver;
    use crate::types::RoutineDef;
    use serde_json::json;

    const DEF: &str = r#"{
      "routine": "r", "schema_version": 1, "transmit_mode": "attended",
      "on_interrupted": "stay", "inputs": [], "triggers": [{"type": "manual"}],
      "tracks": [{ "name": "t", "steps": [
        { "id": "s1", "action": "radio.connect",
          "params": { "stations": "@station-set:or-gateways", "bands": ["40m"] } }
      ]}]
    }"#;

    #[tokio::test]
    async fn refs_are_replaced_with_resolved_values() {
        let def = RoutineDef::parse(DEF).unwrap();
        let resolver = FakeResolver::new()
            .entity("station-set", "or-gateways", json!(["W7DEF-10", "K7ABC-10"]));
        let snapshot = resolve_snapshot(&def, &resolver).await.unwrap();
        let stations = &snapshot["tracks"][0]["steps"][0]["params"]["stations"];
        assert_eq!(stations, &json!(["W7DEF-10", "K7ABC-10"]));
    }

    #[tokio::test]
    async fn unresolved_ref_names_the_token_verbatim() {
        let def = RoutineDef::parse(DEF).unwrap();
        let resolver = FakeResolver::new(); // knows nothing
        let err = resolve_snapshot(&def, &resolver).await.unwrap_err();
        assert!(matches!(err, SnapshotError::UnresolvedRef(t) if t == "@station-set:or-gateways"));
    }

    #[test]
    fn collect_refs_finds_every_token() {
        let def = RoutineDef::parse(DEF).unwrap();
        let refs = collect_refs(&def);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].to_string(), "@station-set:or-gateways");
    }
}
