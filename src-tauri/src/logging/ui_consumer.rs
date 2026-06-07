//! UI consumer task — subscribes to Fanout broadcast, forwarding only events
//! explicitly marked as session-log material into the operator connection log.
//!
//! The disk_consumer and ui_consumer share the same broadcast — each gets its
//! own Receiver via broadcast_tx.subscribe(), so neither lags the other.

use crate::logging::event::LoggedEvent;
use crate::session_log::SessionLogState;
use crate::winlink_backend::{LogLevel, LogLine, LogSource};
use std::sync::Arc;
use tokio::sync::broadcast;

pub fn spawn(mut rx: broadcast::Receiver<LoggedEvent>, session_log: Arc<SessionLogState>) {
    tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Some(line) = to_session_log_line(&event) {
                        session_log.append_with_seq(line);
                    }
                }
                Err(broadcast::error::RecvError::Closed) => break,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
            }
        }
    });
}

fn to_session_log_line(event: &LoggedEvent) -> Option<LogLine> {
    if !event
        .fields
        .get("session_log")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return None;
    }

    // Map tracing level string to LogLevel enum.
    let level = match event.level.as_str() {
        "trace" => LogLevel::Trace,
        "debug" => LogLevel::Debug,
        "warn" => LogLevel::Warn,
        "error" => LogLevel::Error,
        _ => LogLevel::Info,
    };
    Some(LogLine {
        seq: event.seq,
        timestamp_iso: event.ts.clone(),
        level,
        source: LogSource::Backend,
        // Compact one-line preview: "[target] msg"
        message: format!("[{}] {}", event.target, event.msg),
    })
}
