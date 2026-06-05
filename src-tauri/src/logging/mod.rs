//! Diagnostic logging — alpha-logging spec §2.
//!
//! Wiring is exposed via `init(session_log) -> InitOutcome` (Task 6.2) and the
//! Tauri command handlers in `commands` (Task 6.4). The Subscriber composition
//! lives in `subscriber`; the Fanout Layer + redacting Visit live in `fanout`
//! + `visit`; redaction policy in `redact` + `wire_sanitize`.

pub mod bounded_timer;
pub mod commands;
pub mod dict;
pub mod disk_consumer;
pub mod env_probes;
pub mod event;
pub mod export;
pub mod fanout;
pub mod filter_layer;
pub mod free_disk_guard;
pub mod logging_handle;
pub mod manifest;
pub mod redact;
pub mod retention;
pub mod settings;
pub mod state_dir;
pub mod subscriber;
pub mod summary;
pub mod visit;
pub mod wire_sanitize;

pub use fanout::AttemptIdExt;
pub use logging_handle::LoggingHandle;

use crate::session_log::SessionLogState;
use chrono::Utc;
use std::sync::{Arc, Mutex};

/// Outcome of `init()` — either a full pipeline or degraded (no disk logging).
/// Per Amendment D (spec §6.1 fail-soft).
pub enum InitOutcome {
    Full(LoggingHandle),
    Degraded { reason: String },
}

/// Managed state installed when `init()` returns `Degraded`. The Logging
/// window's `logging_status` command checks for this type and surfaces the
/// reason to the operator.
pub struct DegradedHandle {
    pub reason: String,
}

/// Initialize the logging pipeline. Single owner: called once from
/// `lib.rs::run().setup(...)`. Returns `InitOutcome::Full(handle)` on success
/// or `InitOutcome::Degraded { reason }` if the state dir is unavailable.
///
/// On success, the returned `LoggingHandle` MUST be stored via `app.manage()`
/// so the `WorkerGuard` lives for process lifetime.
pub fn init(session_log: Arc<SessionLogState>) -> InitOutcome {
    // Amendment D: fail-soft on state_dir failure.
    let log_dir = match state_dir::resolve() {
        Ok(d) => d,
        Err(e) => {
            // Install a temporary stderr-only subscriber so warn/error still surface.
            let stderr_sub = tracing_subscriber::FmtSubscriber::builder()
                .with_writer(std::io::stderr)
                .with_max_level(tracing::Level::WARN)
                .finish();
            let _ = tracing::subscriber::set_global_default(stderr_sub);
            tracing::warn!(error = %e, "logging:init degraded: state_dir unavailable");
            return InitOutcome::Degraded { reason: e.to_string() };
        }
    };

    let settings_loaded = settings::load();

    // Amendment B startup-sweep: clean leftover files from previous runs before
    // opening the appender, so the retention window is current from first write.
    let startup_cfg = retention::RetentionConfig {
        days: settings_loaded.retention_days,
        mb_cap: settings_loaded.retention_mb_cap,
    };
    retention::sweep(&log_dir, &startup_cfg, None);

    let settings = Arc::new(Mutex::new(settings_loaded));

    let (subscriber, handles) = subscriber::build(session_log.clone());
    let _ = tracing::subscriber::set_global_default(subscriber);

    let active_file_path = Arc::new(tokio::sync::Mutex::new(None::<std::path::PathBuf>));
    let free_disk_guard = free_disk_guard::FreeDiskGuard::spawn(log_dir.clone());

    let retention_cfg = {
        let s = settings.lock().expect("settings lock");
        retention::RetentionConfig { days: s.retention_days, mb_cap: s.retention_mb_cap }
    };

    let appender_guard = match disk_consumer::spawn(
        handles.broadcast_rx,
        log_dir.clone(),
        active_file_path.clone(),
        free_disk_guard.paused.clone(),
        retention_cfg,
    ) {
        Ok(g) => g,
        Err(e) => {
            return InitOutcome::Degraded {
                reason: format!("disk consumer spawn failed: {e}"),
            };
        }
    };

    let boot_id = handles.fanout.boot_id.clone();
    let boot_at = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let filter_reload = handles.filter_reload;

    // Apply persisted detailed mode filter on startup.
    {
        let s = settings.lock().expect("settings lock");
        match &s.detailed_mode {
            settings::DetailedMode::Off => {}
            settings::DetailedMode::On | settings::DetailedMode::Bounded { .. } => {
                let _ = filter_layer::set_detailed(&filter_reload);
            }
        }
    }

    let handle = LoggingHandle {
        _appender_guard: appender_guard,
        session_log,
        broadcast_tx: handles.fanout.broadcast_tx.clone(),
        log_dir,
        active_file_path,
        boot_id,
        boot_at,
        settings,
        filter_reload,
        free_disk_paused: free_disk_guard.paused,
        revert_cancel: Arc::new(Mutex::new(None)),
    };

    // Amendment C: schedule Bounded auto-revert timer if settings persisted
    // a Bounded state across a restart.
    let handle_arc = Arc::new(handle);
    bounded_timer::schedule_revert(handle_arc.clone());

    InitOutcome::Full(
        Arc::try_unwrap(handle_arc)
            .unwrap_or_else(|_| panic!("logging::init: handle_arc had unexpected extra references"))
    )
}
