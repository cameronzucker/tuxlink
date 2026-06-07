//! Regression coverage for tuxlink-xvqy: logging startup/listener helpers must
//! not call bare `tokio::spawn`.
//!
//! The release panic happened because `FreeDiskGuard::spawn` was called from
//! synchronous Tauri setup and used bare `tokio::spawn`, which requires a
//! currently-entered Tokio reactor. Unit tests under `#[tokio::test]` hide that
//! bug, so these tests scan the known startup helper files for the forbidden
//! spawn form and construct the startup helpers from plain `#[test]` contexts.

use std::sync::Arc;

use tuxlink_lib::logging::{
    bounded_timer, disk_consumer, filter_layer,
    free_disk_guard::FreeDiskGuard,
    logging_handle::LoggingHandle,
    retention::RetentionConfig,
    settings::{DetailedMode, Settings},
    ui_consumer,
};
use tuxlink_lib::session_log::SessionLogState;

const STARTUP_LOGGING_FILES: &[&str] = &[
    "src/logging/free_disk_guard.rs",
    "src/logging/disk_consumer.rs",
    "src/logging/ui_consumer.rs",
    "src/logging/bounded_timer.rs",
    "src/logging/env_probes/mod.rs",
];

#[test]
fn logging_startup_helpers_do_not_call_bare_tokio_spawn() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for relative in STARTUP_LOGGING_FILES {
        let path = manifest_dir.join(relative);
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        assert!(
            !src.contains("tokio::spawn"),
            "{} must spawn through tauri::async_runtime::spawn or a logging-local wrapper, not bare tokio::spawn",
            relative
        );
    }
}

fn make_logging_handle_for_revert(initial: DetailedMode) -> Arc<LoggingHandle> {
    use std::sync::atomic::AtomicBool;
    use tokio::sync::broadcast;

    let (_, filter_reload) = filter_layer::build();
    let (writer, guard) = tracing_appender::non_blocking(std::io::sink());
    drop(writer);

    Arc::new(LoggingHandle {
        _appender_guard: guard,
        session_log: Arc::new(SessionLogState::new(16)),
        broadcast_tx: {
            let (tx, _) = broadcast::channel(16);
            tx
        },
        log_dir: std::env::temp_dir(),
        active_file_path: Arc::new(tokio::sync::Mutex::new(None::<std::path::PathBuf>)),
        boot_id: "test-boot".to_string(),
        boot_at: "2026-06-06T00:00:00.000Z".to_string(),
        settings: Arc::new(std::sync::Mutex::new(Settings {
            detailed_mode: initial,
            retention_days: 14,
            retention_mb_cap: 500,
        })),
        filter_reload,
        free_disk_paused: Arc::new(AtomicBool::new(false)),
        revert_cancel: Arc::new(std::sync::Mutex::new(None)),
        probe_listener_id: std::sync::Mutex::new(None),
        flush_barrier: {
            let (barrier, _rx) = tuxlink_lib::logging::export::FlushBarrier::new();
            barrier
        },
    })
}

#[test]
fn free_disk_guard_spawn_does_not_require_current_tokio_reactor() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let _guard = FreeDiskGuard::spawn(tmp.path().to_path_buf());
}

#[test]
fn disk_consumer_spawn_does_not_require_current_tokio_reactor() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let log_dir = tmp.path().join("logs");
    std::fs::create_dir_all(&log_dir).expect("create log dir");
    let (_tx, rx) = tokio::sync::broadcast::channel(16);
    let active_file_path = Arc::new(tokio::sync::Mutex::new(None::<std::path::PathBuf>));
    let paused = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (_barrier, flush_rx) = tuxlink_lib::logging::export::FlushBarrier::new();

    let _guard = disk_consumer::spawn(
        rx,
        log_dir,
        active_file_path,
        paused,
        RetentionConfig {
            days: 14,
            mb_cap: 500,
        },
        flush_rx,
    )
    .expect("disk consumer appender should initialize");
}

#[test]
fn ui_consumer_spawn_does_not_require_current_tokio_reactor() {
    let (_tx, rx) = tokio::sync::broadcast::channel(16);
    ui_consumer::spawn(rx, Arc::new(SessionLogState::new(16)));
}

#[test]
fn bounded_timer_spawn_does_not_require_current_tokio_reactor() {
    let expires_at = chrono::Utc::now() + chrono::Duration::hours(1);
    let handle = make_logging_handle_for_revert(DetailedMode::Bounded { expires_at });
    bounded_timer::schedule_revert(handle);
}

#[test]
fn logging_init_degrades_when_global_subscriber_install_is_not_handled() {
    // Do not mutate the process-global subscriber in a shared Rust test binary;
    // it is one-way and can contaminate sibling tests. This source-level guard
    // catches the regression shape without installing a global subscriber.
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src = std::fs::read_to_string(manifest_dir.join("src/logging/mod.rs"))
        .expect("read logging/mod.rs");
    assert!(
        src.contains("tracing::subscriber::set_global_default(subscriber)")
            && src.contains("InitOutcome::Degraded")
            && !src.contains("let _ = tracing::subscriber::set_global_default(subscriber);"),
        "logging::init must handle set_global_default failure explicitly"
    );
}
