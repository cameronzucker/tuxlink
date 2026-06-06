//! Fanout Layer — formats each event ONCE through the RedactingVisitor,
//! allocates the monotonic `seq` once, and broadcasts the redacted
//! `LoggedEvent` to UI + disk consumers (spec §2.2).

use crate::logging::event::{LoggedEvent, SpanInfo, ThreadInfo};
use crate::logging::visit::RedactingVisitor;
use crate::session_log::SessionLogState;
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing_subscriber::{
    layer::{Context, Layer},
    registry::LookupSpan,
};

/// The broadcast capacity. Choose generously — broadcasts to slow consumers
/// drop oldest-first, which is acceptable for UI subscribers but tracked as
/// `events_dropped_lagged` via the broadcast's lag count.
pub const BROADCAST_CAPACITY: usize = 4096;

/// Per-event-size cap (post-encoding); events larger are dropped + replaced
/// with a synthetic dropped marker.
pub const EVENT_SIZE_CAP_BYTES: usize = 32 * 1024;

pub struct FanoutLayer {
    pub session_log: Arc<SessionLogState>,
    pub broadcast_tx: broadcast::Sender<LoggedEvent>,
    pub boot_id: String,
    pub pid: u32,
}

impl FanoutLayer {
    pub fn create(session_log: Arc<SessionLogState>) -> (FanoutLayerHandle, broadcast::Receiver<LoggedEvent>) {
        let (tx, rx) = broadcast::channel(BROADCAST_CAPACITY);
        let inner = Arc::new(Self {
            session_log,
            broadcast_tx: tx,
            boot_id: uuid::Uuid::now_v7().to_string(),
            pid: std::process::id(),
        });
        (FanoutLayerHandle(inner), rx)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<LoggedEvent> {
        self.broadcast_tx.subscribe()
    }
}

/// Newtype wrapper so we can `impl Layer<S>` (local type — coherence-friendly)
/// for an `Arc<FanoutLayer>`. Per plan-adrev v2 §1 Finding "FanoutLayer Layer
/// impl is the wrong Rust/tracing-subscriber shape": `Arc<T>` is a foreign type
/// (defined in std::sync), so `impl Layer<S> for Arc<FanoutLayer>` falls under
/// Rust's orphan-rule restriction for foreign-trait-on-foreign-type. The newtype
/// wrapper makes the impl target local.
#[derive(Clone)]
pub struct FanoutLayerHandle(pub Arc<FanoutLayer>);

impl std::ops::Deref for FanoutLayerHandle {
    type Target = FanoutLayer;
    fn deref(&self) -> &FanoutLayer { &self.0 }
}

impl<S> Layer<S> for FanoutLayerHandle
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
        let mut visitor = RedactingVisitor::new();
        event.record(&mut visitor);

        let meta = event.metadata();
        let spans: Vec<SpanInfo> = ctx
            .event_scope(event)
            .into_iter()
            .flat_map(|scope| scope.from_root())
            .map(|span_ref| {
                let attempt_id = span_ref
                    .extensions()
                    .get::<crate::logging::AttemptIdExt>()
                    .map(|ext| ext.0.clone());
                SpanInfo {
                    name: span_ref.name().to_string(),
                    id: format!("{:#x}", span_ref.id().into_u64()),
                    attempt_id,
                }
            })
            .collect();

        let attempt_id = spans
            .iter()
            .rev()
            .find_map(|s| s.attempt_id.clone());

        let thread = std::thread::current();
        let thread_info = ThreadInfo {
            id: thread_id_u64(),
            name: thread.name().map(|n| n.to_string()).unwrap_or_else(|| "unnamed".into()),
        };

        let seq = self.session_log.allocate_seq();

        let logged = LoggedEvent {
            v: 1,
            ts: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
            boot: self.boot_id.clone(),
            seq,
            level: meta.level().to_string().to_lowercase(),
            target: meta.target().to_string(),
            module: meta.module_path().map(String::from),
            file: meta.file().map(String::from),
            line: meta.line(),
            pid: Some(self.pid),
            thread: Some(thread_info),
            attempt_id,
            spans,
            msg: visitor.msg.unwrap_or_default(),
            fields: visitor.fields,
        };

        // Size-cap enforcement
        let line_size = logged.to_jsonl().len();
        let to_send = if line_size > EVENT_SIZE_CAP_BYTES {
            LoggedEvent {
                v: 1,
                ts: logged.ts.clone(),
                boot: logged.boot.clone(),
                seq: logged.seq,
                level: "warn".into(),
                target: "tuxlink::logging::fanout".into(),
                module: None,
                file: None,
                line: None,
                pid: logged.pid,
                thread: logged.thread.clone(),
                attempt_id: None,
                spans: vec![],
                msg: "event_dropped_oversize".into(),
                fields: {
                    let mut m = std::collections::BTreeMap::new();
                    m.insert("original_target".into(), serde_json::json!(logged.target));
                    m.insert("original_size_bytes".into(), serde_json::json!(line_size));
                    m
                },
            }
        } else {
            logged
        };

        // Best-effort broadcast — subscribers may have dropped due to lag.
        let _ = self.broadcast_tx.send(to_send);
    }

    /// Per plan-adrev v2 §3 Finding "AttemptIdExt is read but never written":
    /// extract any `attempt_id` field from span values on span creation and
    /// store it in the span's extensions map. The on_event handler above reads
    /// these extensions when it constructs `SpanInfo.attempt_id` and the
    /// top-level `attempt_id` promotion. Without this hook, `attempt_id` would
    /// always be `None` even when spans declared the field.
    fn on_new_span(&self, attrs: &tracing::span::Attributes<'_>, id: &tracing::span::Id, ctx: Context<'_, S>) {
        let mut visitor = AttemptIdFieldVisitor(None);
        attrs.record(&mut visitor);
        if let Some(attempt_id) = visitor.0 {
            if let Some(span) = ctx.span(id) {
                span.extensions_mut().insert(AttemptIdExt(attempt_id));
            }
        }
    }
}

/// Single-purpose visitor that captures the `attempt_id` field if present.
/// Used by FanoutLayerHandle::on_new_span. Not part of the redacting pipeline.
struct AttemptIdFieldVisitor(Option<String>);

impl tracing::field::Visit for AttemptIdFieldVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "attempt_id" {
            self.0 = Some(value.to_string());
        }
    }
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "attempt_id" {
            let s = format!("{value:?}");
            // Trim surrounding quotes that Debug adds for &str
            let trimmed = s.trim_matches('"').to_string();
            self.0 = Some(trimmed);
        }
    }
}

/// Span extension holding the `attempt_id` string when a span carries one.
pub struct AttemptIdExt(pub String);

fn thread_id_u64() -> u64 {
    // std::thread::ThreadId::as_u64 is nightly; on stable we hash the Debug repr.
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let id = std::thread::current().id();
    let mut h = DefaultHasher::new();
    format!("{id:?}").hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_log::SessionLogState;
    use std::sync::Arc;
    use tracing_subscriber::{Registry, layer::SubscriberExt};

    #[test]
    fn broadcasts_emitted_events() {
        let session_log = Arc::new(SessionLogState::new(100));
        let (layer, mut rx) = FanoutLayer::create(session_log);
        let subscriber = Registry::default().with(layer.clone());

        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(test_field = 42, "smoke event");
        });

        let event = rx.try_recv().expect("event should be broadcast");
        assert_eq!(event.level, "info");
        assert_eq!(event.msg, "smoke event");
        assert_eq!(event.fields.get("test_field"), Some(&serde_json::json!(42)));
        assert_eq!(event.seq, 1);
    }

    #[test]
    fn password_field_is_redacted_in_broadcast() {
        let session_log = Arc::new(SessionLogState::new(100));
        let (layer, mut rx) = FanoutLayer::create(session_log);
        let subscriber = Registry::default().with(layer.clone());

        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(password = "hunter2hunter2", "auth event");
        });

        let event = rx.try_recv().expect("event should be broadcast");
        assert_eq!(event.fields.get("password"), Some(&serde_json::json!("<redacted>")));
        let line = event.to_jsonl();
        assert!(!line.contains("hunter2hunter2"), "JSONL must not contain real password");
    }

    #[test]
    fn attempt_id_in_span_is_promoted_to_logged_event() {
        let session_log = Arc::new(SessionLogState::new(100));
        let (layer, mut rx) = FanoutLayer::create(session_log);
        let subscriber = Registry::default().with(layer.clone());

        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!("dial", attempt_id = "att-abc1");
            let _g = span.enter();
            tracing::info!("dialing");
        });

        let event = rx.try_recv().expect("event must broadcast");
        assert_eq!(event.attempt_id.as_deref(), Some("att-abc1"));
    }
}
