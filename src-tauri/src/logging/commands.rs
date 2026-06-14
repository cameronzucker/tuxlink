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
use crate::logging::env_probes::{
    audio, display, keyring, modem_process, network, serial, ProbeSnapshot,
};
use crate::logging::export::{build_archive, ExportInputs, ExportResult};
use crate::logging::filter_layer;
use crate::logging::logging_handle::LoggingHandle;
use crate::logging::retention::{self, RetentionConfig};
use crate::logging::settings::{self, DetailedMode};
use crate::logging::DegradedHandle;
use chrono::{Duration, Utc};
use std::path::Path;
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
        event_rate_per_hour: 0,     // TODO: populate from sliding-window counter
        last_export: None,          // TODO: persist across sessions
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
pub fn logging_set_retention(app: tauri::AppHandle, days: u32, mb_cap: u32) -> Result<(), String> {
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
    let active = handle
        .active_file_path
        .try_lock()
        .ok()
        .and_then(|g| g.clone());
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
pub fn logging_export(app: tauri::AppHandle, output_path: String) -> Result<ExportResult, String> {
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
    let active = handle
        .active_file_path
        .try_lock()
        .ok()
        .and_then(|g| g.clone());
    build_archive(ExportInputs {
        log_dir: &handle.log_dir,
        active_file_path: active.as_deref(),
        output_path: std::path::Path::new(&output_path),
        session_log: handle.session_log.as_ref(),
        correlation_id: None,
        boot_id: &handle.boot_id,
        boot_at: &handle.boot_at,
        detailed_mode: detailed_label,
        retention_days: settings.retention_days,
        retention_mb_cap: settings.retention_mb_cap,
        // Codex P2 #4: pass the live flush barrier so pending disk-consumer
        // events are flushed before the archive reader opens files.
        flush_barrier: Some(&handle.flush_barrier),
    })
    .map_err(|e| format!("export failed: {e}"))
}

/// Open the log directory in the system file manager.
#[tauri::command]
pub fn logging_open_directory(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    let handle = app
        .try_state::<Arc<LoggingHandle>>()
        .ok_or_else(|| "logging not available (degraded or not initialized)".to_string())?;

    // tauri_plugin_shell::Shell::open is deprecated upstream in favor of
    // tauri-plugin-opener (Tauri 2.1+). Migrating to the new plugin requires
    // adding tauri-plugin-opener to Cargo.toml + capabilities/*.json + lib.rs
    // .plugin() registration — out of scope for the alpha-logging PR. Suppress
    // the deprecation locally; the project-wide migration is a follow-up before
    // the Tauri 3 upgrade.
    #[allow(deprecated)]
    tauri_plugin_shell::ShellExt::shell(&app)
        .open(handle.log_dir.to_string_lossy().to_string(), None)
        .map_err(|e| format!("shell open: {e}"))
}

/// Clear the in-memory session log ring buffer, delete closed log files, and
/// truncate the currently-open active log file so visible disk usage resets.
#[tauri::command]
pub fn logging_clear_history(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    let handle = app
        .try_state::<Arc<LoggingHandle>>()
        .ok_or_else(|| "logging not available (degraded or not initialized)".to_string())?;

    handle.session_log.clear();
    handle
        .flush_barrier
        .flush_and_wait(std::time::Duration::from_millis(500))
        .map_err(|e| format!("flush before clear failed: {e}"))?;
    let active = handle
        .active_file_path
        .try_lock()
        .map_err(|_| "active log path busy; retry clear history".to_string())?
        .clone();
    clear_history_files(&handle.log_dir, active.as_deref())
}

fn clear_history_files(log_dir: &Path, active_file_path: Option<&Path>) -> Result<(), String> {
    let Ok(entries) = std::fs::read_dir(log_dir) else {
        return Ok(());
    };

    for e in entries.flatten() {
        let path = e.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if !(name.starts_with("tuxlink.") && name.ends_with(".jsonl")) {
                continue;
            }
        } else {
            continue;
        }

        if Some(path.as_path()) == active_file_path {
            std::fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&path)
                .map_err(|e| format!("truncate active log file {}: {e}", path.display()))?;
        } else {
            std::fs::remove_file(&path)
                .map_err(|e| format!("remove log file {}: {e}", path.display()))?;
        }
    }

    Ok(())
}

/// Return a snapshot of all environment probes (read-only; RADIO-1 safe).
#[tauri::command]
pub fn logging_env_probes_snapshot(_app: tauri::AppHandle) -> Result<Vec<ProbeSnapshot>, String> {
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
///
/// Gate-guarded: each probe's ProbeGate prevents double-runs on rapid
/// double-click. Only probes whose gate is successfully claimed are re-run;
/// skipped probes are omitted from the result set. The gate does NOT apply to
/// `logging_env_probes_snapshot` — that is a read-only status display where
/// gating would confusingly return empty data.
#[tauri::command]
pub fn logging_env_probes_rerun(app: tauri::AppHandle) -> Result<Vec<ProbeSnapshot>, String> {
    let mut snaps = Vec::with_capacity(6);

    if keyring::GATE.try_claim() {
        let s = keyring::run("rerun");
        keyring::GATE.release();
        snaps.push(s);
    }
    if audio::GATE.try_claim() {
        let s = audio::run("rerun");
        audio::GATE.release();
        snaps.push(s);
    }
    if serial::GATE.try_claim() {
        let s = serial::run("rerun");
        serial::GATE.release();
        snaps.push(s);
    }
    if modem_process::GATE.try_claim() {
        let s = modem_process::run("rerun");
        modem_process::GATE.release();
        snaps.push(s);
    }
    if network::GATE.try_claim() {
        let s = network::run("rerun");
        network::GATE.release();
        snaps.push(s);
    }
    if display::GATE.try_claim() {
        let s = display::run("rerun");
        display::GATE.release();
        snaps.push(s);
    }

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

/// Forward a frontend (webview) error into the structured log (tuxlink-4b96).
///
/// A React ErrorBoundary capture, a `window.onerror`, or an `unhandledrejection`
/// otherwise reaches ONLY the WebKitGTK devtools console / `tauri dev` stdout —
/// invisible to the robust logs (`tuxlink.<hour>.jsonl` + the Logging window),
/// so a webview crash cannot be diagnosed from logs the way alpha expects.
/// Emitting it as a `tracing::error!` runs it through the FanoutLayer like any
/// backend event. Infallible + cheap: the caller is already on an error path and
/// its logging must never throw.
#[tauri::command]
pub fn log_frontend_error(source: String, message: String, stack: Option<String>) {
    tracing::error!(
        target: "tuxlink::frontend",
        source = %source,
        stack = stack.as_deref().unwrap_or(""),
        "frontend error: {message}",
    );
}

// ─── Report Issue flow ────────────────────────────────────────────────────────

/// Result returned to the frontend after a successful `report_issue_flow` call.
#[derive(serde::Serialize)]
pub struct ReportIssueResult {
    pub archive_path: String,
    pub archive_size_bytes: u64,
    pub github_url: String,
    pub browser_opened: bool,
    pub correlation_id: Option<String>,
    /// Pasteable build/environment summary for the bug-report Logs field.
    /// Surfaced as a "Copy diagnostics" affordance because the github_url no
    /// longer carries the context (see `report_issue_chooser_url`).
    pub diagnostics: String,
}

/// The GitHub Issues template-chooser URL. Static, no query params.
///
/// uhpn: the prior `?labels=…&body=…` form opened GitHub's BLANK issue editor
/// and prefilled its body — but this repo sets `blank_issues_enabled: false`
/// with a structured `bug_report.yml`, so GitHub bounced the blank-form URL to
/// the chooser and DROPPED the body, landing the operator on an empty issue.
/// `?template=bug_report.yml` does not help: GitHub's template query-prefill is
/// unreliable for YAML issue forms and also falls back to a blank issue
/// (operator-verified 2026-06-10). The chooser reliably presents the Bug report
/// form; the diagnostics the operator pastes into it come from
/// `build_diagnostics` + the attached log archive, not from dropped URL params.
fn report_issue_chooser_url() -> &'static str {
    "https://github.com/cameronzucker/tuxlink/issues/new/choose"
}

/// Build the pasteable diagnostics block for the bug-report Logs field. Each
/// runtime field is markdown-escaped (values are not operator-controlled, but a
/// path or kernel string can carry backticks / ANSI); lines join with real
/// newlines so the block pastes as multi-line Markdown.
#[allow(clippy::too_many_arguments)]
fn build_diagnostics(
    version: &str,
    git_sha: &str,
    profile: &str,
    os: &str,
    kernel: &str,
    correlation_id: &str,
    exported_at: &str,
    archive_path: &str,
    archive_size: &str,
) -> String {
    format!(
        "Build: tuxlink {} (git {}, {})\n\
         Platform: {} · {}\n\
         Correlation ID: {}\n\
         Exported at: {}\n\
         Log archive: `{}` ({}) — drag this file into the issue to attach it.",
        markdown_escape(version),
        markdown_escape(git_sha),
        markdown_escape(profile),
        markdown_escape(os),
        markdown_escape(kernel),
        markdown_escape(correlation_id),
        markdown_escape(exported_at),
        markdown_escape(archive_path),
        markdown_escape(archive_size),
    )
}

/// Auto-export the logs archive, build the pasteable diagnostics summary + the
/// GitHub Issues template-chooser URL, and attempt to open it in the operator's
/// default browser.
///
/// Called from the frontend's ReportIssueModal after the operator confirms the
/// Save As path. Returns `ReportIssueResult` on success; the frontend handles
/// each failure path (no browser, path copy, URL copy).
///
/// Signature mirrors `logging_export`: uses `app.try_state` for full/degraded
/// handling so Tauri does not panic when logging is degraded.
#[tauri::command]
pub fn report_issue_flow(
    app: tauri::AppHandle,
    output_path: String,
) -> Result<ReportIssueResult, String> {
    use tauri::Manager;

    // 1. Export the archive. Reuses the same build_archive path as logging_export
    //    (spec §8.7 single-source-of-truth requirement).
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
    let active = handle
        .active_file_path
        .try_lock()
        .ok()
        .and_then(|g| g.clone());
    let export = build_archive(ExportInputs {
        log_dir: &handle.log_dir,
        active_file_path: active.as_deref(),
        output_path: std::path::Path::new(&output_path),
        session_log: handle.session_log.as_ref(),
        correlation_id: None,
        boot_id: &handle.boot_id,
        boot_at: &handle.boot_at,
        detailed_mode: detailed_label,
        retention_days: settings.retention_days,
        retention_mb_cap: settings.retention_mb_cap,
        // Codex P2 #4: pass the live flush barrier so pending disk-consumer
        // events are flushed before the archive reader opens files.
        flush_barrier: Some(&handle.flush_barrier),
    })
    .map_err(|e| format!("export failed: {e}"))?;
    // Drop the settings lock before doing I/O.
    drop(settings);

    // 2. Build the pasteable diagnostics + the template-chooser URL.
    let build = crate::logging::manifest::build_info();
    let platform = crate::logging::manifest::platform_info();
    let exported_at = chrono::Utc::now().to_rfc3339();
    let correlation_id = export.correlation_id.as_deref().unwrap_or("(none)");
    let archive_size = format_bytes(export.archive_size_bytes);

    let diagnostics = build_diagnostics(
        &build.version,
        &build.git_sha,
        &build.profile,
        &platform.os,
        &platform.kernel,
        correlation_id,
        &exported_at,
        &export.output_path.display().to_string(),
        &archive_size,
    );

    // Route to the template chooser (not a prefilled blank issue) — see
    // `report_issue_chooser_url`. The diagnostics above are surfaced to the
    // operator for paste into the Bug report form; they are not carried in the
    // URL (GitHub drops them, landing the operator on a blank issue).
    let url = report_issue_chooser_url().to_string();

    // 3. Attempt to open the URL in the operator's default browser.
    //    tauri_plugin_shell::Shell::open is deprecated upstream in favor of
    //    tauri-plugin-opener (Tauri 2.1+). Suppress locally; project-wide
    //    migration is a follow-up before the Tauri 3 upgrade. Same pattern
    //    as logging_open_directory (Task 6 fix I4).
    #[allow(deprecated)]
    let browser_opened = tauri_plugin_shell::ShellExt::shell(&app)
        .open(url.clone(), None)
        .is_ok();

    Ok(ReportIssueResult {
        archive_path: export.output_path.display().to_string(),
        archive_size_bytes: export.archive_size_bytes,
        github_url: url,
        browser_opened,
        correlation_id: export.correlation_id,
        diagnostics,
    })
}

/// Escape a runtime string for safe embedding in the GitHub Issues Markdown body.
///
/// Backticks are replaced with the HTML entity `&#96;` (renders as a backtick
/// in GitHub but does not break the surrounding code-span). Newlines become
/// the literal sequence `\n`. Carriage returns are stripped. ANSI escape
/// sequences (ESC + `[` + params + letter) are stripped.
fn markdown_escape(s: &str) -> String {
    // Strip ANSI escape sequences first.
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Consume the CSI sequence: `[` + optional param bytes + final byte.
            if chars.peek() == Some(&'[') {
                chars.next(); // consume `[`
                              // Consume params (0x30–0x3F) and intermediates (0x20–0x2F).
                while let Some(&p) = chars.peek() {
                    if ('\x30'..='\x3f').contains(&p) || ('\x20'..='\x2f').contains(&p) {
                        chars.next();
                    } else {
                        break;
                    }
                }
                // Consume final byte (0x40–0x7E).
                if let Some(&f) = chars.peek() {
                    if ('\x40'..='\x7e').contains(&f) {
                        chars.next();
                    }
                }
            }
            // Other ESC sequences (rare in log data) — skip the ESC itself.
            continue;
        }
        match c {
            '`' => out.push_str("&#96;"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            other => out.push(other),
        }
    }
    out
}

/// Human-readable byte size (B / KB / MB).
fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let kb = bytes as f64 / 1024.0;
    if kb < 1024.0 {
        return format!("{kb:.1} KB");
    }
    format!("{:.1} MB", kb / 1024.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

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

    #[test]
    fn clear_history_truncates_active_file_and_is_idempotent() {
        let tmp = tempdir().unwrap();
        let active = tmp.path().join("tuxlink.2026-06-07-08.jsonl");
        let closed = tmp.path().join("tuxlink.2026-06-07-07.jsonl");
        let unrelated = tmp.path().join("not-a-tuxlink-log.txt");
        std::fs::write(&active, b"{\"msg\":\"active backlog\"}\n").unwrap();
        std::fs::write(&closed, b"{\"msg\":\"closed backlog\"}\n").unwrap();
        std::fs::write(&unrelated, b"keep me").unwrap();

        clear_history_files(tmp.path(), Some(active.as_path())).unwrap();

        assert!(active.exists(), "active appender path remains present");
        assert_eq!(
            std::fs::metadata(&active).unwrap().len(),
            0,
            "active file should be truncated so disk usage can reset"
        );
        assert!(!closed.exists(), "closed diagnostic files are deleted");
        assert!(
            unrelated.exists(),
            "non-log files in the directory are preserved"
        );

        clear_history_files(tmp.path(), Some(active.as_path())).unwrap();
        assert_eq!(
            std::fs::metadata(&active).unwrap().len(),
            0,
            "repeated clears should not grow the active diagnostic log"
        );
    }

    // ── markdown_escape tests ──────────────────────────────────────────────

    #[test]
    fn markdown_escape_passes_through_plain_text() {
        assert_eq!(markdown_escape("hello world"), "hello world");
    }

    #[test]
    fn markdown_escape_replaces_backtick() {
        let input = "path/to/`file`";
        let out = markdown_escape(input);
        assert!(!out.contains('`'), "backtick should be gone: {out}");
        assert!(out.contains("&#96;"), "entity should be present: {out}");
    }

    #[test]
    fn markdown_escape_replaces_newline_with_literal_backslash_n() {
        let out = markdown_escape("line1\nline2");
        assert_eq!(out, r"line1\nline2");
    }

    #[test]
    fn markdown_escape_strips_carriage_return() {
        let out = markdown_escape("a\r\nb");
        assert_eq!(out, r"a\nb");
    }

    #[test]
    fn markdown_escape_strips_ansi_color_sequence() {
        // ESC[32m = green; ESC[0m = reset
        let input = "\x1b[32mgreen\x1b[0m text";
        let out = markdown_escape(input);
        assert_eq!(out, "green text");
    }

    #[test]
    fn markdown_escape_strips_ansi_csi_multi_param() {
        // ESC[1;31m = bold + red
        let input = "\x1b[1;31merror\x1b[0m";
        let out = markdown_escape(input);
        assert_eq!(out, "error");
    }

    #[test]
    fn markdown_escape_no_injection_via_archive_path() {
        // Operator-chosen path that tries to break out of backtick span.
        let path = "/home/user/my`archive`.tar.zst";
        let out = markdown_escape(path);
        assert!(!out.contains('`'), "no backtick in escaped output: {out}");
    }

    // ── format_bytes tests ────────────────────────────────────────────────

    #[test]
    fn format_bytes_bytes() {
        assert_eq!(format_bytes(512), "512 B");
    }

    #[test]
    fn format_bytes_kilobytes() {
        assert_eq!(format_bytes(2048), "2.0 KB");
    }

    #[test]
    fn format_bytes_megabytes() {
        assert_eq!(format_bytes(2 * 1024 * 1024), "2.0 MB");
    }

    #[test]
    fn format_bytes_boundary_1023() {
        assert_eq!(format_bytes(1023), "1023 B");
    }

    #[test]
    fn format_bytes_boundary_1024() {
        assert_eq!(format_bytes(1024), "1.0 KB");
    }

    // ── report-issue URL + diagnostics (uhpn) ─────────────────────────────

    #[test]
    fn report_issue_url_is_the_template_chooser_not_a_blank_issue() {
        let url = report_issue_chooser_url();
        // The chooser presents bug_report.yml reliably; the prior forms landed
        // the operator on a blank issue.
        assert!(
            url.ends_with("/issues/new/choose"),
            "must route to the template chooser: {url}"
        );
        assert!(
            !url.contains("?body="),
            "must not use the blank-issue body form (dropped under blank_issues_enabled:false): {url}"
        );
        assert!(
            !url.contains("?template="),
            "?template= falls back to a blank issue for YAML forms (operator-verified): {url}"
        );
    }

    #[test]
    fn diagnostics_carry_build_and_archive_context_multiline() {
        let d = build_diagnostics(
            "v0.41.1",
            "abc1234",
            "release",
            "Ubuntu 24.04",
            "6.8.0-generic",
            "corr-1",
            "2026-06-10T00:00:00Z",
            "/home/op/tuxlink-logs.tar.zst",
            "1.2 MB",
        );
        assert!(d.contains("v0.41.1"), "carries the build version: {d}");
        assert!(d.contains("Ubuntu 24.04"), "carries the platform: {d}");
        assert!(
            d.contains("/home/op/tuxlink-logs.tar.zst"),
            "carries the archive path: {d}"
        );
        assert!(
            d.contains('\n'),
            "diagnostics must be multi-line so it pastes cleanly: {d}"
        );
    }
}
