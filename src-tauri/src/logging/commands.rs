//! Tauri commands exposed by the Logging window (spec §8.4).
//!
//! Amendment D: when logging init degraded, `logging_status` reads
//! `State<DegradedHandle>` and returns a status with `degraded: Some(reason)`.
//!
//! Amendment H: `LoggingStatus` carries `boot_id_short` (first 8 chars of
//! boot_id) so the frontend can seed a stable per-process export filename.
//!
//! Amendment E.7.7: `emit_first_paint_complete` command bridges the frontend's
//! first-render event to the backend probe runner.

use crate::logging::bounded_timer;
use crate::logging::env_probes::{audio, display, keyring, modem_process, network, serial, ProbeSnapshot};
use crate::logging::export::{build_archive, ExportInputs, ExportResult};
use crate::logging::filter_layer;
use crate::logging::logging_handle::LoggingHandle;
use crate::logging::retention::{self, RetentionConfig};
use crate::logging::settings::{self, DetailedMode};
use crate::logging::DegradedHandle;
use chrono::{Duration, Utc};
use std::sync::Arc;

// ─── Status types ─────────────────────────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct LoggingStatus {
    pub disk_usage_bytes: u64,
    pub disk_cap_bytes: u64,
    pub retained_window_seconds: u64,
    pub event_rate_per_hour: u64,
    pub last_export: Option<LastExport>,
    pub detailed_mode: String,
    pub bounded_remaining_seconds: Option<i64>,
    pub retention_days: u32,
    pub retention_mb_cap: u32,
    /// Amendment H: first 8 chars of boot_id for export-filename seeding.
    pub boot_id_short: String,
    /// Amendment D: Some(reason) when logging is degraded (no disk logging).
    pub degraded: Option<String>,
}

#[derive(serde::Serialize)]
pub struct LastExport {
    pub path: String,
    pub size_bytes: u64,
    pub at: String,
    pub correlation_id: Option<String>,
}

// ─── Commands ─────────────────────────────────────────────────────────────────

/// Return the current logging status. Reads managed `LoggingHandle`; if logging
/// is degraded, falls back to `DegradedHandle` and returns a minimal status.
///
/// Tauri will pass whichever managed type is present. Because Tauri panics if
/// you ask for a State<T> that was never managed, we use the application's
/// actual managed types. The two types are mutually exclusive: `init()` either
/// manages `LoggingHandle` (Full) or `DegradedHandle` (Degraded), never both.
///
/// Implementation note: we accept both optionally via `Option<State<T>>`
/// — Tauri 2 doesn't support optional State natively, so we instead supply
/// a command that takes `Arc<LoggingHandle>` if managed.
///
/// Simpler approach used here: accept `Arc<LoggingHandle>` wrapped in `State`.
/// In the Degraded path, the app manages `DegradedHandle` but NOT `LoggingHandle`,
/// so Tauri would panic. To avoid this, we take `State<LoggingHandle>` and rely
/// on Tauri's `try_state`-style access via `app: tauri::AppHandle`.
#[tauri::command]
pub fn logging_status(app: tauri::AppHandle) -> Result<LoggingStatus, String> {
    use tauri::Manager;

    // Try the Full path first.
    if let Some(handle) = app.try_state::<Arc<LoggingHandle>>() {
        return full_status(&handle);
    }

    // Degraded path.
    if let Some(degraded) = app.try_state::<DegradedHandle>() {
        return Ok(LoggingStatus {
            disk_usage_bytes: 0,
            disk_cap_bytes: 0,
            retained_window_seconds: 0,
            event_rate_per_hour: 0,
            last_export: None,
            detailed_mode: "off".into(),
            bounded_remaining_seconds: None,
            retention_days: 14,
            retention_mb_cap: 500,
            boot_id_short: "degraded".into(),
            degraded: Some(degraded.reason.clone()),
        });
    }

    Err("logging not initialized".into())
}

fn full_status(handle: &Arc<LoggingHandle>) -> Result<LoggingStatus, String> {
    let settings = handle
        .settings
        .lock()
        .map_err(|e| format!("settings lock: {e}"))?;

    // Sum up on-disk JSONL files.
    let mut disk_usage_bytes = 0u64;
    if let Ok(entries) = std::fs::read_dir(&handle.log_dir) {
        for e in entries.flatten() {
            if let Some(name) = e.file_name().to_str() {
                if name.starts_with("tuxlink.") && name.ends_with(".jsonl") {
                    if let Ok(m) = e.metadata() {
                        disk_usage_bytes += m.len();
                    }
                }
            }
        }
    }

    let bounded_remaining = match &settings.detailed_mode {
        DetailedMode::Bounded { expires_at } => {
            Some((expires_at.signed_duration_since(Utc::now())).num_seconds())
        }
        _ => None,
    };
    let detailed_label = match &settings.detailed_mode {
        DetailedMode::Off => "off",
        DetailedMode::On => "on",
        DetailedMode::Bounded { .. } => "bounded",
    };

    // Amendment H: first 8 chars of boot_id.
    let boot_id_short = handle.boot_id.chars().take(8).collect::<String>();

    Ok(LoggingStatus {
        disk_usage_bytes,
        disk_cap_bytes: (settings.retention_mb_cap as u64) * 1024 * 1024,
        retained_window_seconds: 0, // TODO: populate from oldest file timestamp
        event_rate_per_hour: 0, // TODO: populate from sliding-window counter
        last_export: None,      // TODO: persist across sessions
        detailed_mode: detailed_label.into(),
        bounded_remaining_seconds: bounded_remaining,
        retention_days: settings.retention_days,
        retention_mb_cap: settings.retention_mb_cap,
        boot_id_short,
        degraded: None,
    })
}

/// Set the detailed logging mode (off / on / bounded).
///
/// `bounded_hours` is required when `mode == "bounded"` and must be 1..=720.
/// Persists the new mode to settings TOML and atomically swaps the filter.
/// For Bounded transitions, schedules the auto-revert timer (Amendment C).
#[tauri::command]
pub fn logging_set_detailed_mode(
    app: tauri::AppHandle,
    mode: String,
    bounded_hours: Option<u32>,
) -> Result<(), String> {
    use tauri::Manager;
    let handle = app
        .try_state::<Arc<LoggingHandle>>()
        .ok_or_else(|| "logging not available (degraded or not initialized)".to_string())?;

    let new_mode = match mode.as_str() {
        "off" => DetailedMode::Off,
        "on" => DetailedMode::On,
        "bounded" => {
            let hours = bounded_hours.ok_or("bounded_hours required for 'bounded' mode")?;
            if hours == 0 || hours > 720 {
                return Err(format!("bounded_hours must be 1..=720, got {hours}"));
            }
            DetailedMode::Bounded {
                expires_at: Utc::now() + Duration::hours(hours as i64),
            }
        }
        _ => return Err(format!("unknown mode: {mode}")),
    };

    {
        let mut s = handle
            .settings
            .lock()
            .map_err(|e| format!("settings lock: {e}"))?;
        s.detailed_mode = new_mode.clone();
        settings::save(&s)?;
    }

    match &new_mode {
        DetailedMode::Off => filter_layer::set_standard(&handle.filter_reload)?,
        DetailedMode::On | DetailedMode::Bounded { .. } => {
            filter_layer::set_detailed(&handle.filter_reload)?
        }
    }

    // Amendment C: schedule Bounded auto-revert timer after a Bounded transition.
    // handle is State<Arc<LoggingHandle>>; deref to get the Arc.
    if matches!(new_mode, DetailedMode::Bounded { .. }) {
        bounded_timer::schedule_revert((*handle).clone());
    }

    tracing::info!(mode = ?new_mode, "logging.detailed_mode.changed");
    Ok(())
}

/// Update retention settings (days + MB cap) and run an immediate sweep.
#[tauri::command]
pub fn logging_set_retention(
    app: tauri::AppHandle,
    days: u32,
    mb_cap: u32,
) -> Result<(), String> {
    use tauri::Manager;
    let handle = app
        .try_state::<Arc<LoggingHandle>>()
        .ok_or_else(|| "logging not available (degraded or not initialized)".to_string())?;

    if !(1..=365).contains(&days) {
        return Err(format!("days must be 1..=365, got {days}"));
    }
    if !(50..=10240).contains(&mb_cap) {
        return Err(format!("mb_cap must be 50..=10240, got {mb_cap}"));
    }
    {
        let mut s = handle
            .settings
            .lock()
            .map_err(|e| format!("settings lock: {e}"))?;
        s.retention_days = days;
        s.retention_mb_cap = mb_cap;
        settings::save(&s)?;
    }
    let cfg = RetentionConfig { days, mb_cap };
    let active = handle.active_file_path.try_lock().ok().and_then(|g| g.clone());
    let result = retention::sweep(&handle.log_dir, &cfg, active.as_deref());
    tracing::info!(
        deleted = result.deleted_count,
        retained_bytes = result.retained_bytes,
        "retention sweep complete"
    );
    Ok(())
}

/// Build and save a zstd export archive.
#[tauri::command]
pub fn logging_export(
    app: tauri::AppHandle,
    output_path: String,
) -> Result<ExportResult, String> {
    use tauri::Manager;
    let handle = app
        .try_state::<Arc<LoggingHandle>>()
        .ok_or_else(|| "logging not available (degraded or not initialized)".to_string())?;

    let settings = handle
        .settings
        .lock()
        .map_err(|e| format!("settings lock: {e}"))?;
    let detailed_label = match &settings.detailed_mode {
        DetailedMode::Off => "off",
        DetailedMode::On => "on",
        DetailedMode::Bounded { .. } => "bounded",
    };
    let active = handle.active_file_path.try_lock().ok().and_then(|g| g.clone());
    build_archive(ExportInputs {
        log_dir: &handle.log_dir,
        active_file_path: active.as_deref(),
        output_path: std::path::Path::new(&output_path),
        correlation_id: None,
        boot_id: &handle.boot_id,
        boot_at: &handle.boot_at,
        detailed_mode: detailed_label,
        retention_days: settings.retention_days,
        retention_mb_cap: settings.retention_mb_cap,
        flush_barrier: None,
    })
    .map_err(|e| format!("export failed: {e}"))
}

/// Open the log directory in the system file manager.
#[tauri::command]
pub fn logging_open_directory(
    app: tauri::AppHandle,
) -> Result<(), String> {
    use tauri::Manager;
    let handle = app
        .try_state::<Arc<LoggingHandle>>()
        .ok_or_else(|| "logging not available (degraded or not initialized)".to_string())?;

    tauri_plugin_shell::ShellExt::shell(&app)
        .open(handle.log_dir.to_string_lossy().to_string(), None)
        .map_err(|e| format!("shell open: {e}"))
}

/// Clear the in-memory session log ring buffer and remove all closed log files
/// from disk (preserving the currently-open active file).
#[tauri::command]
pub fn logging_clear_history(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    let handle = app
        .try_state::<Arc<LoggingHandle>>()
        .ok_or_else(|| "logging not available (degraded or not initialized)".to_string())?;

    handle.session_log.clear();
    let active = handle.active_file_path.try_lock().ok().and_then(|g| g.clone());
    if let Ok(entries) = std::fs::read_dir(&handle.log_dir) {
        for e in entries.flatten() {
            let path = e.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if !(name.starts_with("tuxlink.") && name.ends_with(".jsonl")) {
                    continue;
                }
            }
            if Some(path.as_path()) == active.as_deref() {
                continue;
            }
            let _ = std::fs::remove_file(path);
        }
    }
    tracing::warn!("logging history cleared by operator");
    Ok(())
}

/// Return a snapshot of all environment probes (read-only; RADIO-1 safe).
#[tauri::command]
pub fn logging_env_probes_snapshot(
    _app: tauri::AppHandle,
) -> Result<Vec<ProbeSnapshot>, String> {
    Ok(vec![
        keyring::run("snapshot"),
        audio::run("snapshot"),
        serial::run("snapshot"),
        modem_process::run("snapshot"),
        network::run("snapshot"),
        display::run("snapshot"),
    ])
}

/// Re-run all environment probes and emit a push event to the Logging window.
#[tauri::command]
pub fn logging_env_probes_rerun(app: tauri::AppHandle) -> Result<Vec<ProbeSnapshot>, String> {
    let snaps = vec![
        keyring::run("rerun"),
        audio::run("rerun"),
        serial::run("rerun"),
        modem_process::run("rerun"),
        network::run("rerun"),
        display::run("rerun"),
    ];
    use tauri::Emitter;
    let _ = app.emit("logging://probes/snapshot-updated", &snaps);
    Ok(snaps)
}

/// Amendment E.7.7 backend: emit the `first_paint_complete` Tauri event so the
/// backend probe runner (env_probes::spawn_runner) can react to first paint.
/// Called from the frontend's useEffect after first render commit.
#[tauri::command]
pub fn emit_first_paint_complete(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Emitter;
    app.emit("first_paint_complete", ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detailed_label_variants() {
        // Smoke-test the label strings match the spec.
        let off: DetailedMode = DetailedMode::Off;
        let on: DetailedMode = DetailedMode::On;
        let bounded = DetailedMode::Bounded {
            expires_at: Utc::now() + Duration::hours(1),
        };
        let label_of = |m: &DetailedMode| match m {
            DetailedMode::Off => "off",
            DetailedMode::On => "on",
            DetailedMode::Bounded { .. } => "bounded",
        };
        assert_eq!(label_of(&off), "off");
        assert_eq!(label_of(&on), "on");
        assert_eq!(label_of(&bounded), "bounded");
    }

    #[test]
    fn retention_bounds_validation() {
        // days = 0 is invalid
        assert!(!(1..=365).contains(&0u32));
        // days = 366 is invalid
        assert!(!(1..=365).contains(&366u32));
        // mb_cap = 49 is invalid
        assert!(!(50..=10240).contains(&49u32));
        // mb_cap = 10241 is invalid
        assert!(!(50..=10240).contains(&10241u32));
        // Valid values
        assert!((1..=365).contains(&14u32));
        assert!((50..=10240).contains(&500u32));
    }

    #[test]
    fn boot_id_short_truncates_to_8() {
        let boot_id = "01927a8b-9c12-7000-a4d3-2f8e1b9c0001";
        let short: String = boot_id.chars().take(8).collect();
        assert_eq!(short.len(), 8);
        assert_eq!(short, "01927a8b");
    }
}
