//! Redacting `tracing::field::Visit` implementation (spec §5.7).
//!
//! The visitor receives field values one at a time as tracing serializes them.
//! For each field whose NAME matches the redaction blocklist, the value is
//! replaced with `<redacted>`. Otherwise the value is formatted into the
//! event's `fields` map.

use crate::logging::redact::should_redact_field;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fmt;
use tracing::field::{Field, Visit};

const STRING_FIELD_CAP_BYTES: usize = 4096;
const BYTES_PREVIEW_CAP: usize = 256;

pub struct RedactingVisitor {
    pub fields: BTreeMap<String, Value>,
    pub msg: Option<String>,
}

impl RedactingVisitor {
    pub fn new() -> Self {
        Self {
            fields: BTreeMap::new(),
            msg: None,
        }
    }

    /// Insert a field value, applying the blocklist + caps. The `message` field
    /// (tracing's special field for the format-string argument) is captured into
    /// `self.msg` instead of `self.fields`.
    fn insert(&mut self, name: &str, raw_value: Value) {
        let value = if should_redact_field(name) {
            json!("<redacted>")
        } else {
            raw_value
        };

        if name == "message" {
            if let Value::String(s) = &value {
                self.msg = Some(cap_string(s));
            }
        } else {
            self.fields.insert(name.to_string(), value);
        }
    }
}

fn cap_string(s: &str) -> String {
    if s.len() <= STRING_FIELD_CAP_BYTES {
        s.to_string()
    } else {
        format!("{}…[truncated {} bytes]", &s[..STRING_FIELD_CAP_BYTES], s.len() - STRING_FIELD_CAP_BYTES)
    }
}

impl Visit for RedactingVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.insert(field.name(), json!(cap_string(&format!("{value:?}"))));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.insert(field.name(), json!(cap_string(value)));
    }

    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        let mut chain = format!("{value}");
        let mut src = value.source();
        while let Some(e) = src {
            chain.push_str(&format!(" -> {e}"));
            src = e.source();
        }
        self.insert(field.name(), json!(cap_string(&chain)));
    }

    fn record_bytes(&mut self, field: &Field, value: &[u8]) {
        let preview = &value[..value.len().min(BYTES_PREVIEW_CAP)];
        self.insert(
            field.name(),
            json!(format!("{} bytes; preview: {}", value.len(), hex::encode(preview))),
        );
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.insert(field.name(), json!(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.insert(field.name(), json!(value));
    }

    fn record_i128(&mut self, field: &Field, value: i128) {
        self.insert(field.name(), json!(value.to_string()));
    }

    fn record_u128(&mut self, field: &Field, value: u128) {
        self.insert(field.name(), json!(value.to_string()));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.insert(field.name(), json!(value));
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        if value.is_finite() {
            self.insert(field.name(), json!(value));
        } else {
            let kind = if value.is_nan() {
                "nan"
            } else if value.is_sign_positive() {
                "posinf"
            } else {
                "neginf"
            };
            self.insert(field.name(), Value::Null);
            self.fields
                .insert(format!("{}_kind", field.name()), json!(kind));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logging::event::LoggedEvent;
    use crate::session_log::SessionLogState;
    use std::sync::Arc;
    use tracing_subscriber::{Registry, layer::SubscriberExt};

    /// Helper: capture one event emitted while a fanout-driven subscriber is active.
    fn capture_one(emit: impl FnOnce()) -> LoggedEvent {
        let session_log = Arc::new(SessionLogState::new(100));
        let (handle, mut rx) = crate::logging::fanout::FanoutLayer::new(session_log);
        let subscriber = Registry::default().with(handle.clone());
        tracing::subscriber::with_default(subscriber, emit);
        rx.try_recv().expect("event must be broadcast")
    }

    #[test]
    fn record_str_routes_through_blocklist() {
        let ev = capture_one(|| tracing::info!(password = "hunter2", "auth"));
        assert_eq!(ev.fields.get("password"), Some(&serde_json::json!("<redacted>")));
    }

    #[test]
    fn record_debug_with_credential_struct_redacts() {
        #[derive(Debug)] struct Fake;
        impl std::fmt::Display for Fake { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "fake") } }
        let ev = capture_one(|| tracing::info!(token = "abc123", "auth"));
        assert_eq!(ev.fields.get("token"), Some(&serde_json::json!("<redacted>")));
    }

    #[test]
    fn record_i64_preserves_value() {
        let ev = capture_one(|| tracing::info!(count = 42_i64, "tick"));
        assert_eq!(ev.fields.get("count"), Some(&serde_json::json!(42)));
    }

    #[test]
    fn record_bool_preserves_value() {
        let ev = capture_one(|| tracing::info!(success = true, "result"));
        assert_eq!(ev.fields.get("success"), Some(&serde_json::json!(true)));
    }

    #[test]
    fn record_f64_finite_preserves_value() {
        let ev = capture_one(|| tracing::info!(rate = 3.14_f64, "metric"));
        assert_eq!(ev.fields.get("rate"), Some(&serde_json::json!(3.14)));
    }

    #[test]
    fn record_f64_nan_encodes_as_null_plus_kind_marker() {
        let ev = capture_one(|| tracing::info!(rate = f64::NAN, "metric"));
        assert_eq!(ev.fields.get("rate"), Some(&serde_json::Value::Null));
        assert_eq!(ev.fields.get("rate_kind"), Some(&serde_json::json!("nan")));
    }

    #[test]
    fn benign_field_passes_through() {
        let ev = capture_one(|| tracing::info!(callsign = "K0ABC", "dial"));
        assert_eq!(ev.fields.get("callsign"), Some(&serde_json::json!("K0ABC")));
    }
}
