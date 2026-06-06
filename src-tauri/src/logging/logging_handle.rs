//! LoggingHandle — Tauri-managed state carrying the WorkerGuard + all the
//! runtime handles for the Logging Tauri commands (spec §2.6).
//!
//! `revert_cancel` (Amendment C): holds the cancellation sender for the
//! current Bounded auto-revert timer. Replacing it cancels the previous timer.

use crate::logging::event::LoggedEvent;
use crate::logging::export::FlushBarrier;
use crate::logging::settings::Settings;
use crate::session_log::SessionLogState;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tracing_appender::non_blocking::WorkerGuard;

pub struct LoggingHandle {
    /// Keeps the non-blocking appender's background writer thread alive.
    /// Must live for process lifetime — stored in Tauri-managed state.
    pub _appender_guard: WorkerGuard,
    /// Session log ring buffer shared with the UI command `session_log_snapshot`.
    pub session_log: Arc<SessionLogState>,
    /// Broadcast sender — UI consumers call `.subscribe()` for live tailing.
    pub broadcast_tx: broadcast::Sender<LoggedEvent>,
    /// Resolved on-disk log directory (canonical, validated by state_dir).
    pub log_dir: PathBuf,
    /// The currently-open JSONL file path (updated on hour rotation by
    /// disk_consumer). Starts as None; set on first event.
    /// Uses tokio::sync::Mutex because disk_consumer holds it across .await points.
    pub active_file_path: Arc<tokio::sync::Mutex<Option<PathBuf>>>,
    /// UUID v7 string minted at process start (matches `boot` field in LoggedEvent).
    pub boot_id: String,
    /// RFC3339 timestamp of process start (millisecond precision).
    pub boot_at: String,
    /// Persisted settings (detailed_mode + retention). Shared with
    /// bounded_timer so the revert task can update mode.
    pub settings: Arc<Mutex<Settings>>,
    /// tracing-subscriber reload handle for atomic filter swaps.
    pub filter_reload: tracing_subscriber::reload::Handle<
        tracing_subscriber::filter::EnvFilter,
        tracing_subscriber::Registry,
    >,
    /// FreeDiskGuard's paused flag — true when disk space is low.
    pub free_disk_paused: Arc<std::sync::atomic::AtomicBool>,
    /// Amendment C: cancellation sender for the active Bounded auto-revert
    /// timer. Replacing the Some value closes the previous sender, which
    /// causes the waiting timer task to exit (cancel semantics via channel-closed).
    pub revert_cancel: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
    /// EventId from `app.listen("first_paint_complete", ...)` so spawn_runner
    /// can unlisten the previous listener before registering a new one on
    /// re-init (dev-mode hot-reload listener leak prevention).
    pub probe_listener_id: Mutex<Option<tauri::EventId>>,
    /// Codex P2 #4: flush barrier wired into the production export path.
    /// `disk_consumer::spawn` holds the receiver side; `logging_export` and
    /// `report_issue_flow` send via this sender before reading log files, so
    /// all pending events are on disk before the reader opens files.
    pub flush_barrier: FlushBarrier,
}
