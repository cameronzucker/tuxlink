//! The post-redaction event representation broadcast through the Fanout Layer
//! (spec §3.1 schema).

// Both Serialize + Deserialize per plan-adrev v2 §1 Finding "Export
// deserialization / Tauri command serialization derives are missing": Task 4.7's
// build_archive reads JSONL files back via serde_json::from_str::<LoggedEvent>,
// so the type MUST be Deserialize as well. ThreadInfo and SpanInfo are nested
// fields and need both derives too.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggedEvent {
    /// Schema version. Always 1 for v0.
    pub v: u32,
    /// UTC RFC3339 with microsecond precision.
    pub ts: String,
    /// UUID v7 minted at process start.
    pub boot: String,
    /// Monotonic seq allocated by the Fanout Layer (single allocator).
    pub seq: u64,
    /// `trace` | `debug` | `info` | `warn` | `error`.
    pub level: String,
    /// Tracing target.
    pub target: String,
    /// `module_path!()` from emission site (may equal `target`).
    pub module: Option<String>,
    /// `file!()` repo-relative.
    pub file: Option<String>,
    /// `line!()`.
    pub line: Option<u32>,
    /// Process ID.
    pub pid: Option<u32>,
    /// Thread {id, name}.
    pub thread: Option<ThreadInfo>,
    /// Promoted from innermost span carrying an `attempt_id`; `None` if no span has one.
    pub attempt_id: Option<String>,
    /// Full span stack, outermost-first. Always present; `[]` when outside any span.
    pub spans: Vec<SpanInfo>,
    /// Post-wire-sanitizer message.
    pub msg: String,
    /// Post-redaction structured fields.
    pub fields: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadInfo {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanInfo {
    pub name: String,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempt_id: Option<String>,
}

impl LoggedEvent {
    /// Render as a single JSONL line (terminating `\n` included).
    pub fn to_jsonl(&self) -> String {
        let mut s = serde_json::to_string(self).unwrap_or_else(|e| {
            format!(
                r#"{{"v":1,"level":"error","target":"tuxlink::logging::event","msg":"failed to serialize event: {}"}}"#,
                e
            )
        });
        s.push('\n');
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_event() -> LoggedEvent {
        LoggedEvent {
            v: 1,
            ts: "2026-06-04T12:34:56.789012Z".into(),
            boot: "01927a8b-9c12-7000-a4d3-2f8e1b9c0001".into(),
            seq: 42891,
            level: "info".into(),
            target: "tuxlink::winlink::session".into(),
            module: Some("tuxlink::winlink::session".into()),
            file: Some("src-tauri/src/winlink/session.rs".into()),
            line: Some(412),
            pid: Some(12345),
            thread: Some(ThreadInfo { id: 7, name: "tokio-runtime-worker".into() }),
            attempt_id: Some("att-xyz1".into()),
            spans: vec![
                SpanInfo { name: "dial_attempt".into(), id: "0x7f3a".into(), attempt_id: Some("att-xyz1".into()) },
                SpanInfo { name: "b2f_exchange".into(), id: "0x812c".into(), attempt_id: None },
            ],
            msg: "dial start".into(),
            fields: {
                let mut m = std::collections::BTreeMap::new();
                m.insert("transport".into(), json!("vara"));
                m.insert("gateway".into(), json!("K6XXX-10"));
                m
            },
        }
    }

    #[test]
    fn jsonl_roundtrips_through_serde() {
        let e = sample_event();
        let line = e.to_jsonl();
        assert!(line.ends_with('\n'));
        let parsed: serde_json::Value = serde_json::from_str(line.trim_end()).unwrap();
        assert_eq!(parsed["v"], 1);
        assert_eq!(parsed["seq"], 42891);
        assert_eq!(parsed["attempt_id"], "att-xyz1");
        assert_eq!(parsed["spans"].as_array().unwrap().len(), 2);
        assert_eq!(parsed["fields"]["transport"], "vara");
    }

    #[test]
    fn empty_spans_serialize_as_array_not_null() {
        let mut e = sample_event();
        e.spans = vec![];
        e.attempt_id = None;
        let line = e.to_jsonl();
        let parsed: serde_json::Value = serde_json::from_str(line.trim_end()).unwrap();
        assert!(parsed["spans"].is_array());
        assert_eq!(parsed["spans"].as_array().unwrap().len(), 0);
        assert!(parsed["attempt_id"].is_null());
    }
}
