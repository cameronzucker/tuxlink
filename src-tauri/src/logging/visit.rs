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

impl Default for RedactingVisitor {
    fn default() -> Self {
        Self::new()
    }
}

fn cap_string(s: &str) -> String {
    if s.len() <= STRING_FIELD_CAP_BYTES {
        s.to_string()
    } else {
        // Find a UTF-8 boundary at or below the byte cap. This preserves the
        // 4 KB field budget without slicing inside a multi-byte codepoint.
        let mut end = 0;
        for (idx, ch) in s.char_indices() {
            let next = idx + ch.len_utf8();
            if next > STRING_FIELD_CAP_BYTES {
                break;
            }
            end = next;
        }
        let truncated = &s[..end];
        format!("{truncated}…[truncated {} bytes]", s.len() - end)
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
            json!(format!(
                "{} bytes; preview: {}",
                value.len(),
                hex::encode(preview)
            )),
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
            if !crate::logging::redact::should_redact_field(field.name()) {
                self.fields
                    .insert(format!("{}_kind", field.name()), json!(kind));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::STRING_FIELD_CAP_BYTES;
    use crate::logging::event::LoggedEvent;
    use crate::logging::fanout::FanoutLayer;
    use tracing_subscriber::{layer::SubscriberExt, Registry};

    /// Helper: capture one event emitted while a fanout-driven subscriber is active.
    fn capture_one(emit: impl FnOnce()) -> LoggedEvent {
        let (handle, mut rx) = FanoutLayer::create();
        let subscriber = Registry::default().with(handle.clone());
        tracing::subscriber::with_default(subscriber, emit);
        rx.try_recv().expect("event must be broadcast")
    }

    #[test]
    fn record_str_routes_through_blocklist() {
        let ev = capture_one(|| tracing::info!(password = "hunter2", "auth"));
        assert_eq!(
            ev.fields.get("password"),
            Some(&serde_json::json!("<redacted>"))
        );
    }

    #[test]
    fn record_str_token_field_is_redacted() {
        let ev = capture_one(|| tracing::info!(token = "abc123", "auth"));
        assert_eq!(
            ev.fields.get("token"),
            Some(&serde_json::json!("<redacted>"))
        );
    }

    #[test]
    fn record_debug_via_question_mark_routes_field_value_through_blocklist() {
        let ev = capture_one(|| {
            let token_value = "abc123";
            tracing::info!(token = ?token_value, "auth");
        });
        assert_eq!(
            ev.fields.get("token"),
            Some(&serde_json::json!("<redacted>"))
        );
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
        let ev = capture_one(|| tracing::info!(rate = 1.25_f64, "metric"));
        assert_eq!(ev.fields.get("rate"), Some(&serde_json::json!(1.25)));
    }

    #[test]
    fn record_f64_nan_encodes_as_null_plus_kind_marker() {
        let ev = capture_one(|| tracing::info!(rate = f64::NAN, "metric"));
        assert_eq!(ev.fields.get("rate"), Some(&serde_json::Value::Null));
        assert_eq!(ev.fields.get("rate_kind"), Some(&serde_json::json!("nan")));
    }

    #[test]
    fn record_f64_nan_with_blocklisted_name_suppresses_kind_marker() {
        let ev = capture_one(|| tracing::info!(nonce = f64::NAN, "auth"));
        // Blocklisted field: value must be redacted (insert() routes through blocklist first)
        assert_eq!(
            ev.fields.get("nonce"),
            Some(&serde_json::json!("<redacted>"))
        );
        // The _kind marker must NOT leak for a blocklisted field name
        assert!(
            !ev.fields.contains_key("nonce_kind"),
            "blocklisted field's _kind marker must not leak"
        );
    }

    #[test]
    fn benign_field_passes_through() {
        let ev = capture_one(|| tracing::info!(callsign = "K0ABC", "dial"));
        assert_eq!(ev.fields.get("callsign"), Some(&serde_json::json!("K0ABC")));
    }

    /// Regression test: a traced string >4096 bytes where the cap falls inside
    /// a multi-byte UTF-8 sequence must NOT panic inside the subscriber.
    ///
    /// Codex impl-adrev P2 #5: the original `&s[..STRING_FIELD_CAP_BYTES]` byte-
    /// index slice panics when byte 4096 is not a char boundary. "é" is 2 bytes
    /// in UTF-8 (U+00E9, encoded as 0xC3 0xA9); 5000 repetitions = 10 000 bytes;
    /// the 4096-byte boundary falls inside the second byte of some "é", causing
    /// the slice to panic. The fix uses char-count truncation, which is always
    /// safe regardless of multi-byte sequences.
    #[test]
    fn cap_string_does_not_panic_on_multibyte_utf8_at_boundary() {
        // "é" is 2 bytes; 5000 repetitions = 10 000 bytes — well past 4096.
        // The byte boundary at 4096 falls inside a 2-byte sequence for many
        // offsets, which would panic with the old byte-index slice.
        let long_unicode: String = "é".repeat(5000);
        assert!(long_unicode.len() > STRING_FIELD_CAP_BYTES, "pre-condition");

        // This must not panic.
        let ev = capture_one(|| tracing::info!(field_value = %long_unicode, "unicode test"));

        // The captured value must be truncated, not the full string.
        let value = ev
            .fields
            .get("field_value")
            .expect("field_value must be present")
            .as_str()
            .expect("field_value must be a string");
        assert!(
            value.contains("…[truncated"),
            "long string must be truncated; got len={}",
            value.len()
        );
        // The truncated prefix must be valid UTF-8 (no broken sequences).
        assert!(
            std::str::from_utf8(value.as_bytes()).is_ok(),
            "truncated string must be valid UTF-8"
        );
    }
}
