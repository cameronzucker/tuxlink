//! Tauri commands for provisioning VARA HF under native WINE (tuxlink-w7212).
//!
//! These drive the vendored `wine-vara-setup` engine
//! (`resources/wine-vara-setup/`, MIT — upstream github.com/cameronzucker/
//! wine-vara-setup) which does the fragile VB6-under-WINE dance. Tuxlink's
//! posture is unchanged: it does NOT manage VARA at *runtime* (that stays a
//! third-party external process on 127.0.0.1:8300/8301); this only automates
//! the one-time, prep-time, online *install*.
//!
//! RADIO-1: provisioning never transmits. It runs `apt`/`winetricks`/`wine`
//! to install software; opening VARA's TCP ports (the `verify` checkpoint)
//! does not key a radio.
//!
//! ## Streaming contract
//!
//! `vara_install_start` spawns the engine with `--json` and forwards each
//! JSONL line verbatim (as [`EngineEvent`]) on the `vara_install:progress`
//! Tauri event. The field names mirror the engine's frozen contract
//! (see the vendored `docs/tuxlink-integration.md`): `hello`/`checkpoint`/
//! `summary` events, snake_case keys. The frontend renders friendly labels
//! from `id`/`index`/`total`; it must not parse `detail` for control flow.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

/// Tauri event carrying one line of the engine's `--json` progress stream.
pub const VARA_INSTALL_PROGRESS_EVENT: &str = "vara_install:progress";

/// Bundle-relative path to the vendored engine entrypoint.
const ENGINE_REL: &str = "resources/wine-vara-setup/bin/wine-vara-setup";

/// One parsed line of the engine's JSONL output. All fields beyond `event`
/// are optional because `hello`, `checkpoint`, and `summary` carry different
/// subsets. Field names match the engine contract verbatim (snake_case), so
/// this both deserializes the engine output and is the event payload sent to
/// the frontend — one shape, no re-casing drift.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EngineEvent {
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vara_version: Option<String>,
}

/// Result of a read-only `status --json` probe (offline; no network, no launch).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallStatus {
    /// True iff the engine's `status` reported the core checkpoints green
    /// (its process exited 0).
    pub ready: bool,
    /// Per-checkpoint events from the status stream, for display.
    pub checkpoints: Vec<EngineEvent>,
}

/// Parse a single JSONL line into an [`EngineEvent`]; non-JSON / non-object
/// lines yield `None` (the engine only emits objects, but be defensive).
fn parse_engine_line(line: &str) -> Option<EngineEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    serde_json::from_str::<EngineEvent>(trimmed).ok()
}

/// Build the argv for a headless install run.
fn build_install_args(installer_path: &str) -> Vec<String> {
    vec![
        "install".to_string(),
        "--installer".to_string(),
        installer_path.to_string(),
        "--yes".to_string(),
        "--json".to_string(),
        "--autostart".to_string(),
    ]
}

/// Resolve the vendored engine path inside the app bundle. Errors if the
/// resource is missing (e.g. an unbundled dev run).
fn resolve_engine(app: &AppHandle) -> Result<PathBuf, String> {
    let path = app
        .path()
        .resolve(ENGINE_REL, tauri::path::BaseDirectory::Resource)
        .map_err(|e| format!("could not resolve bundled engine path: {e}"))?;
    if path.exists() {
        Ok(path)
    } else {
        Err(format!(
            "VARA setup engine is not bundled (looked at {}). This build cannot provision VARA.",
            path.display()
        ))
    }
}

/// Base command that runs the engine via `bash` — avoids depending on the
/// bundled script's executable bit and lets the script resolve its own `lib/`
/// via `BASH_SOURCE`.
fn engine_command(engine: &Path) -> Command {
    let mut cmd = Command::new("bash");
    cmd.arg(engine);
    cmd
}

/// True iff the VARA provisioning engine is present in this build.
#[tauri::command]
pub fn vara_engine_available(app: AppHandle) -> bool {
    resolve_engine(&app).is_ok()
}

/// Read-only, offline readiness probe: runs `status --json` and reports whether
/// VARA is provisioned. Never launches VARA and never touches the network.
#[tauri::command]
pub fn vara_install_status(app: AppHandle) -> Result<InstallStatus, String> {
    let engine = resolve_engine(&app)?;
    let output = engine_command(&engine)
        .args(["status", "--json"])
        .output()
        .map_err(|e| format!("failed to run status probe: {e}"))?;
    let checkpoints = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_engine_line)
        .collect();
    Ok(InstallStatus {
        ready: output.status.success(),
        checkpoints,
    })
}

/// Provision VARA HF from a user-supplied installer `.exe`. Streams each
/// engine progress line on `vara_install:progress` and returns the final
/// `summary` event. Errors if the engine is missing, the installer path does
/// not exist, the child fails to spawn, or the run ends non-green.
///
/// `installer_path` is a filesystem path chosen by the user via the native
/// file dialog; it is passed as a process argument (not through a shell), so
/// it cannot inject additional commands.
#[tauri::command]
pub fn vara_install_start(app: AppHandle, installer_path: String) -> Result<EngineEvent, String> {
    let engine = resolve_engine(&app)?;

    if installer_path.trim().is_empty() {
        return Err("no installer selected".to_string());
    }
    if !PathBuf::from(&installer_path).exists() {
        return Err(format!("installer not found: {installer_path}"));
    }

    let mut child = engine_command(&engine)
        .args(build_install_args(&installer_path))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to start VARA setup: {e}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "failed to capture setup output".to_string())?;

    // Drain stderr on its own thread so a full stderr pipe can't deadlock the
    // stdout reader below.
    let stderr = child.stderr.take();
    let stderr_handle = std::thread::spawn(move || {
        let mut buf = String::new();
        if let Some(mut s) = stderr {
            let _ = s.read_to_string(&mut buf);
        }
        buf
    });

    // Stream stdout line-by-line, emitting each parsed event and remembering
    // the terminal summary.
    let mut summary: Option<EngineEvent> = None;
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if let Some(ev) = parse_engine_line(&line) {
            if ev.event == "summary" {
                summary = Some(ev.clone());
            }
            let _ = app.emit(VARA_INSTALL_PROGRESS_EVENT, &ev);
        }
    }

    let status = child
        .wait()
        .map_err(|e| format!("VARA setup did not exit cleanly: {e}"))?;
    let stderr_text = stderr_handle.join().unwrap_or_default();

    match summary {
        Some(ev) if status.success() && ev.ok == Some(true) => Ok(ev),
        Some(ev) => Err(ev
            .detail
            .filter(|d| !d.is_empty())
            .or_else(|| non_empty(stderr_text))
            .unwrap_or_else(|| "VARA setup did not complete".to_string())),
        None => Err(non_empty(stderr_text)
            .unwrap_or_else(|| "VARA setup produced no result".to_string())),
    }
}

/// Return `Some(s)` iff `s` is non-empty after trimming.
fn non_empty(s: String) -> Option<String> {
    if s.trim().is_empty() {
        None
    } else {
        Some(s.trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_checkpoint_line() {
        let ev = parse_engine_line(
            r#"{"event":"checkpoint","id":"ocx","index":5,"total":7,"state":"running","detail":"Registering"}"#,
        )
        .expect("should parse");
        assert_eq!(ev.event, "checkpoint");
        assert_eq!(ev.id.as_deref(), Some("ocx"));
        assert_eq!(ev.index, Some(5));
        assert_eq!(ev.total, Some(7));
        assert_eq!(ev.state.as_deref(), Some("running"));
    }

    #[test]
    fn parses_hello_and_summary() {
        let hello = parse_engine_line(r#"{"event":"hello","contract":1}"#).unwrap();
        assert_eq!(hello.event, "hello");
        assert_eq!(hello.contract, Some(1));

        let summary =
            parse_engine_line(r#"{"event":"summary","ok":true,"prefix":"/p","vara_version":"VARA HF"}"#)
                .unwrap();
        assert_eq!(summary.event, "summary");
        assert_eq!(summary.ok, Some(true));
        assert_eq!(summary.vara_version.as_deref(), Some("VARA HF"));
    }

    #[test]
    fn ignores_non_json_and_blank_lines() {
        assert!(parse_engine_line("").is_none());
        assert!(parse_engine_line("   ").is_none());
        assert!(parse_engine_line("not json at all").is_none());
    }

    #[test]
    fn install_args_are_headless_and_autostart() {
        let args = build_install_args("/home/ham/Downloads/VARA setup.exe");
        assert_eq!(
            args,
            vec![
                "install",
                "--installer",
                "/home/ham/Downloads/VARA setup.exe",
                "--yes",
                "--json",
                "--autostart",
            ]
        );
    }

    #[test]
    fn non_empty_trims_and_filters() {
        assert_eq!(non_empty("  ".to_string()), None);
        assert_eq!(non_empty(String::new()), None);
        assert_eq!(non_empty("  boom ".to_string()), Some("boom".to_string()));
    }
}
