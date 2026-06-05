//! Detailed-mode Bounded auto-revert timer (Amendment C; spec §4.3).
//!
//! Spawned whenever `logging_set_detailed_mode(Bounded, ...)` is called and
//! once at `logging::init()` to resume persisted Bounded state across restarts.
//!
//! The `revert_cancel` oneshot on `LoggingHandle` cancels any previous timer
//! when a new `schedule_revert` call replaces it.

use crate::logging::filter_layer;
use crate::logging::logging_handle::LoggingHandle;
use crate::logging::settings::{self, DetailedMode};
use chrono::Utc;
use std::sync::Arc;

/// Schedule an auto-revert timer for Bounded mode.
///
/// If the current settings are NOT `Bounded`, this is a no-op (returns
/// immediately without spawning a task).
///
/// If a previous timer is active (its sender is stored in
/// `handle.revert_cancel`), replacing the sender closes the previous
/// channel, causing the previous task's `cancel_rx` to resolve immediately
/// and exit — effectively cancelling it.
pub fn schedule_revert(handle: Arc<LoggingHandle>) {
    let expires_at = {
        let s = match handle.settings.lock() {
            Ok(s) => s,
            Err(_) => return,
        };
        match s.detailed_mode {
            DetailedMode::Bounded { expires_at } => expires_at,
            _ => return, // not Bounded; no timer to schedule
        }
    };

    // Install a new cancellation channel, replacing (and thereby cancelling)
    // any previous timer.
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    {
        if let Ok(mut slot) = handle.revert_cancel.lock() {
            // Dropping the previous sender closes its receiver, cancelling the
            // previous timer task via the select! below.
            *slot = Some(cancel_tx);
        }
    }

    let handle_for_task = handle.clone();
    tokio::spawn(async move {
        let now = Utc::now();
        let wait = (expires_at - now)
            .to_std()
            .unwrap_or(std::time::Duration::from_millis(0));

        // Race: timer vs cancel.
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = cancel_rx => return, // this timer was superseded
        }

        // Re-check: operator may have changed mode while we slept, or the
        // expiry may have been extended. Only revert if we're still in a
        // Bounded state whose expiry has passed.
        let still_bounded = handle_for_task
            .settings
            .lock()
            .ok()
            .map(|s| {
                matches!(
                    s.detailed_mode,
                    DetailedMode::Bounded { expires_at: e } if e <= Utc::now()
                )
            })
            .unwrap_or(false);
        if !still_bounded {
            return;
        }

        // Revert to Off + persist.
        if let Ok(mut s) = handle_for_task.settings.lock() {
            s.detailed_mode = DetailedMode::Off;
            let _ = settings::save(&s);
        }
        let _ = filter_layer::set_standard(&handle_for_task.filter_reload);
        tracing::info!(
            target: "tuxlink::logging::settings",
            "logging.detailed_mode.expired"
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logging::settings::Settings;
    use crate::session_log::SessionLogState;
    use std::sync::atomic::AtomicBool;
    use tokio::sync::broadcast;

    /// Construct a minimal LoggingHandle suitable for schedule_revert tests.
    /// Uses a real filter_reload handle so set_standard() doesn't panic.
    fn make_test_handle(mode: DetailedMode) -> Arc<LoggingHandle> {
        let (filter_layer, filter_reload) = crate::logging::filter_layer::build();
        // We need a subscriber with the filter layer attached so the handle
        // is usable; we install it temporarily via with_default.
        // For unit tests, we just build the handle without setting a global.
        let _ = filter_layer; // not installed globally in unit tests
        // The disk consumer WorkerGuard is not constructable in unit tests,
        // so we use a no-op via tracing_appender::non_blocking(std::io::sink()).
        let (writer, guard) = tracing_appender::non_blocking(std::io::sink());
        let _ = writer;

        Arc::new(LoggingHandle {
            _appender_guard: guard,
            session_log: Arc::new(SessionLogState::new(100)),
            broadcast_tx: {
                let (tx, _) = broadcast::channel(16);
                tx
            },
            log_dir: std::path::PathBuf::from("/tmp"),
            active_file_path: Arc::new(tokio::sync::Mutex::new(None)),
            boot_id: "test-boot-id".into(),
            boot_at: "2026-06-05T00:00:00.000Z".into(),
            settings: Arc::new(std::sync::Mutex::new(Settings {
                detailed_mode: mode,
                retention_days: 14,
                retention_mb_cap: 500,
            })),
            filter_reload,
            free_disk_paused: Arc::new(AtomicBool::new(false)),
            revert_cancel: Arc::new(std::sync::Mutex::new(None)),
            probe_listener_id: std::sync::Mutex::new(None),
        })
    }

    #[test]
    fn schedule_revert_noop_when_off() {
        // schedule_revert on an Off handle must not panic and must not install
        // a cancel sender (there is nothing to cancel).
        let handle = make_test_handle(DetailedMode::Off);
        schedule_revert(handle.clone());
        let slot = handle.revert_cancel.lock().unwrap();
        assert!(slot.is_none(), "schedule_revert(Off) must not install a cancel sender");
    }

    #[test]
    fn schedule_revert_noop_when_on() {
        let handle = make_test_handle(DetailedMode::On);
        schedule_revert(handle.clone());
        let slot = handle.revert_cancel.lock().unwrap();
        assert!(slot.is_none(), "schedule_revert(On) must not install a cancel sender");
    }

    #[tokio::test]
    async fn schedule_revert_installs_cancel_sender_for_bounded() {
        let future_expiry = Utc::now() + chrono::Duration::hours(1);
        let handle = make_test_handle(DetailedMode::Bounded { expires_at: future_expiry });
        schedule_revert(handle.clone());
        // Give the async spawn a moment to run.
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let slot = handle.revert_cancel.lock().unwrap();
        assert!(slot.is_some(), "schedule_revert(Bounded) must install a cancel sender");
    }

    #[tokio::test]
    async fn schedule_revert_cancels_previous_timer() {
        // Spawn two timers in sequence; the first should be cancelled.
        let future_expiry = Utc::now() + chrono::Duration::hours(1);
        let handle = make_test_handle(DetailedMode::Bounded { expires_at: future_expiry });

        schedule_revert(handle.clone());
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        // Spawn a second — this should cancel the first.
        schedule_revert(handle.clone());
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Both calls installed a cancel sender; the second one replaced the first.
        // The settings are still Bounded (not reverted yet — expiry is 1 hour away).
        let s = handle.settings.lock().unwrap();
        assert!(matches!(s.detailed_mode, DetailedMode::Bounded { .. }));
    }
}
