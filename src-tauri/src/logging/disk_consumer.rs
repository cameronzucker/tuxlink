//! Disk consumer task — subscribes to the Fanout broadcast and writes JSONL
//! to the tracing-appender non-blocking writer (spec §6.2).
//!
//! Amendment A: respects the `paused_flag` AtomicBool set by `FreeDiskGuard`
//! — skips writes when free space is low (spec §6.4).
//!
//! Amendment B: tracks the current hour from event timestamps; on hour
//! rotation, updates `active_file_tracker`, runs a retention sweep, and emits
//! a structured `tracing::info!` event (spec §6.3).
//!
//! Amendment C (spec §6.4): polls the NonBlocking appender's dropped-lines
//! error counter every 30 seconds; on any increment, emits a structured
//! `tracing::warn!` event at target "tuxlink::logging::disk" so the frontend
//! event bridge can surface a "disk-write-error" notification (spec §10.4 #23).

use crate::logging::event::LoggedEvent;
use crate::logging::retention::{self, RetentionConfig};
use chrono::{DateTime, Datelike, Timelike, Utc};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::sync::Mutex;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{InitError, RollingFileAppender, Rotation};

/// Spawn the disk consumer task. Returns the WorkerGuard (must live for
/// process lifetime — store it in Tauri-managed state).
///
/// Returns `Err(InitError)` if the log directory is not writable at spawn
/// time (e.g., permissions race, read-only remount after `state_dir::resolve()`).
/// Callers fold this into `InitOutcome::Degraded` per Amendment D.
///
/// Arguments:
/// - `rx`: broadcast receiver from FanoutLayer.
/// - `log_dir`: resolved log directory (from `state_dir::resolve()`).
/// - `active_file_tracker`: shared mutex tracking the currently-open log file.
/// - `paused_flag`: `FreeDiskGuard` flips this to true when free space is low.
/// - `retention_config`: days + MB cap for the retention sweep.
/// - `flush_barrier_rx`: Codex P2 #4 — mpsc receiver for flush requests from
///   `FlushBarrier::flush_and_wait`. The disk consumer services these in
///   `tokio::select!` so exports get an ack after pending events are on disk.
pub fn spawn(
    mut rx: broadcast::Receiver<LoggedEvent>,
    log_dir: PathBuf,
    active_file_tracker: Arc<Mutex<Option<PathBuf>>>,
    paused_flag: Arc<AtomicBool>,
    retention_config: RetentionConfig,
    mut flush_barrier_rx: tokio::sync::mpsc::UnboundedReceiver<tokio::sync::oneshot::Sender<()>>,
) -> Result<WorkerGuard, InitError> {
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::HOURLY)
        .filename_prefix("tuxlink")
        .filename_suffix("jsonl")
        .build(&log_dir)?;

    let (writer, guard) = tracing_appender::non_blocking(appender);

    // Capture the error counter BEFORE wrapping the writer in Arc+Mutex.
    // ErrorCounter is a cheap Arc clone — the poller holds a direct reference
    // and never needs the writer lock (spec §6.4 / Amendment C).
    let error_counter = writer.error_counter();

    let writer = Arc::new(Mutex::new(writer));

    // Amendment C: spawn a second task that polls the appender's dropped-lines
    // counter every 30 seconds and emits a warn event on any increment.
    // Required for spec §10.4 #23 (ENOSPC acceptance test).
    //
    // Codex P2 #11: also flip paused_flag on any drop increment so the
    // broadcast stops enqueuing events that the appender can't write. The
    // free-disk guard's recover threshold clears the flag when space returns.
    let paused_flag_for_poll = paused_flag.clone();
    tauri::async_runtime::spawn(async move {
        let mut last_count: usize = 0;
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            let current = error_counter.dropped_lines();
            if current > last_count {
                let new_dropped = current - last_count;
                tracing::warn!(
                    target: "tuxlink::logging::disk",
                    new_dropped_lines = new_dropped,
                    total_dropped_lines = current,
                    "disk-write-error: appender dropped log lines"
                );
                // Codex P2 #11: flip the pause flag so the broadcast doesn't
                // keep enqueueing events the appender can't write. The free-disk
                // guard's recover threshold clears the flag when space returns
                // above the 200 MB threshold.
                paused_flag_for_poll.store(true, Ordering::Release);
                last_count = current;
            }
        }
    });

    let log_dir_for_task = log_dir.clone();
    let active_tracker_for_task = active_file_tracker.clone();

    tauri::async_runtime::spawn(async move {
        // Amendment B: track the hour we're currently writing to.
        // None = first event; triggers active-file init without rotation log.
        let mut current_hour: Option<(i32, u32, u32, u32)> = None; // (year, month, day, hour)

        loop {
            tokio::select! {
                // Codex P2 #4: service flush barrier requests with priority.
                // biased; ensures flush requests are not starved by a high
                // event rate.
                biased;
                Some(ack_tx) = flush_barrier_rx.recv() => {
                    // Drain any events currently buffered in the broadcast
                    // before flushing, so the export reader sees everything.
                    {
                        let mut w = writer.lock().await;
                        while let Ok(event) = rx.try_recv() {
                            if !paused_flag.load(Ordering::Acquire) {
                                let line = event.to_jsonl();
                                let _ = w.write_all(line.as_bytes());
                            }
                        }
                        let _ = w.flush();
                    }
                    let _ = ack_tx.send(());
                }
                result = rx.recv() => {
                    match result {
                        Ok(event) => {
                            // Amendment A: skip writes if paused (free-disk guard active).
                            if paused_flag.load(Ordering::Acquire) {
                                continue;
                            }

                            // Amendment B: detect hour rotation from event timestamp.
                            // LoggedEvent.ts is an RFC3339 string (e.g., "2026-06-04T12:34:56.789012Z").
                            let hour_key: Option<(i32, u32, u32, u32)> =
                                event.ts.parse::<DateTime<Utc>>().ok().map(|dt| {
                                    (dt.year(), dt.month(), dt.day(), dt.hour())
                                });

                            if let Some(hk) = hour_key {
                                if current_hour != Some(hk) {
                                    let new_active_name = format!(
                                        "tuxlink.{:04}-{:02}-{:02}-{:02}.jsonl",
                                        hk.0, hk.1, hk.2, hk.3
                                    );
                                    let new_active_path = log_dir_for_task.join(&new_active_name);

                                    // Update the active-file tracker.
                                    {
                                        let mut tracker = active_tracker_for_task.lock().await;
                                        *tracker = Some(new_active_path.clone());
                                    }

                                    // Run retention sweep, preserving the new active file.
                                    let sweep_result = retention::sweep(
                                        &log_dir_for_task,
                                        &retention_config,
                                        Some(&new_active_path),
                                    );

                                    // Only emit the rotation info event if this isn't first-event init.
                                    if current_hour.is_some() {
                                        tracing::info!(
                                            target: "tuxlink::logging::disk",
                                            new_active = %new_active_path.display(),
                                            deleted_count = sweep_result.deleted_count,
                                            deleted_bytes = sweep_result.deleted_bytes,
                                            retained_count = sweep_result.retained_count,
                                            clock_grace_skips = sweep_result.clock_grace_skips,
                                            "log file hour rotation + retention sweep"
                                        );
                                    }

                                    current_hour = Some(hk);
                                }
                            }

                            let line = event.to_jsonl();
                            let mut w = writer.lock().await;
                            let _ = w.write_all(line.as_bytes());
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    }
                }
            }
        }
    });

    Ok(guard)
}
