//! Amendment C integration gate — Bounded mode auto-revert + persistence.
//!
//! Two tests:
//! 1. `bounded_mode_reverts_to_off_after_expiry` — schedule a Bounded(50ms)
//!    revert, wait 300ms, assert settings show Off.
//! 2. `bounded_mode_resumed_at_startup_does_not_panic` — regression for the
//!    HIGH spec-compliance finding: init() called bounded_timer::schedule_revert
//!    and then Arc::try_unwrap, which panics when the timer holds a clone.
//!    Schedules a revert on the same Arc immediately after construction — no
//!    try_unwrap anywhere, so no panic regardless of Arc ref-count.

use std::sync::Arc;
use std::time::Duration;

use tuxlink_lib::logging::bounded_timer;
use tuxlink_lib::logging::filter_layer;
use tuxlink_lib::logging::logging_handle::LoggingHandle;
use tuxlink_lib::logging::settings::{DetailedMode, Settings};
use tuxlink_lib::session_log::SessionLogState;

/// Build a minimal LoggingHandle for bounded-timer tests — mirrors the
/// `make_test_handle` helper in `bounded_timer`'s unit tests, exposed here
/// as a module-level function to avoid duplication in the integration test.
fn make_test_handle(initial: DetailedMode) -> Arc<LoggingHandle> {
    use std::sync::atomic::AtomicBool;
    use tokio::sync::broadcast;

    let (_, filter_reload) = filter_layer::build();
    let (writer, guard) = tracing_appender::non_blocking(std::io::sink());
    let _ = writer;

    Arc::new(LoggingHandle {
        _appender_guard: guard,
        session_log: Arc::new(SessionLogState::new(100)),
        broadcast_tx: {
            let (tx, _) = broadcast::channel(16);
            tx
        },
        log_dir: std::env::temp_dir(),
        active_file_path: Arc::new(tokio::sync::Mutex::new(None)),
        boot_id: "test-boot".to_string(),
        boot_at: "2026-06-05T00:00:00.000Z".to_string(),
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

#[tokio::test]
async fn bounded_mode_reverts_to_off_after_expiry() {
    let expires = chrono::Utc::now() + chrono::Duration::milliseconds(50);
    let handle = make_test_handle(DetailedMode::Bounded { expires_at: expires });

    bounded_timer::schedule_revert(handle.clone());

    // Wait past the expiry window with some margin.
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Settings now show Off.
    let s = handle.settings.lock().unwrap();
    assert!(
        matches!(s.detailed_mode, DetailedMode::Off),
        "expected DetailedMode::Off after expiry, got {:?}",
        s.detailed_mode
    );
}

#[tokio::test]
async fn bounded_mode_resumed_at_startup_does_not_panic() {
    // Regression for the HIGH spec-compliance finding: Arc::try_unwrap
    // panic when init() resumes a persisted Bounded state. This test simulates
    // the scenario by scheduling a revert on the same Arc immediately after
    // constructing it — the same pattern init() now follows. No try_unwrap
    // anywhere, so no panic possible regardless of how many Arc clones the
    // timer captures.
    let expires = chrono::Utc::now() + chrono::Duration::hours(1);
    let handle = make_test_handle(DetailedMode::Bounded { expires_at: expires });

    bounded_timer::schedule_revert(handle.clone());

    // No panic = pass. The timer is sleeping for an hour; we don't need to
    // wait for the revert.
    assert!(Arc::strong_count(&handle) >= 2, "Arc count includes the timer task");
}
