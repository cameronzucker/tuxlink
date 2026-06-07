//! UI consumer task — subscribes to Fanout broadcast, forwards each LoggedEvent
//! to SessionLogState::append_with_seq for the in-app log strip (Codex P2 #3).
//!
//! The disk_consumer and ui_consumer share the same broadcast — each gets its
//! own Receiver via broadcast_tx.subscribe(), so neither lags the other.

use crate::logging::event::LoggedEvent;
use crate::winlink_backend::{LogLevel, LogLine, LogSource};
use crate::session_log::SessionLogState;
use std::sync::Arc;
use tokio::sync::broadcast;

pub fn spawn(
    mut rx: broadcast::Receiver<LoggedEvent>,
    session_log: Arc<SessionLogState>,
) {
    tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    // Map tracing level string to LogLevel enum.
                    let level = match event.level.as_str() {
                        "trace" => LogLevel::Trace,
                        "debug" => LogLevel::Debug,
                        "warn"  => LogLevel::Warn,
                        "error" => LogLevel::Error,
                        _       => LogLevel::Info,
                    };
                    let line = LogLine {
                        seq: event.seq,
                        timestamp_iso: event.ts.clone(),
                        level,
                        source: LogSource::Backend,
                        // Compact one-line preview: "[target] msg"
                        message: format!("[{}] {}", event.target, event.msg),
                    };
                    session_log.append_with_seq(line);
                }
                Err(broadcast::error::RecvError::Closed) => break,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
            }
        }
    });
}
