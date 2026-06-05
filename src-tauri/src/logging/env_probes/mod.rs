//! Environment probes — read-only diagnostic snapshots (spec §9, RADIO-1 §9.1).
//!
//! Probes run AFTER first paint at startup AND on-error per their subsystem,
//! with debounce + single-flight (no probe storms).
//!
//! RADIO-1 contract: NO TX-touching APIs. Probe modules are compile-time
//! isolated from winlink::session, winlink::secure, winlink::handshake,
//! winlink::modem::*, winlink::transfer (see tests/probes_no_tx_apis.rs).

pub mod audio;
pub mod display;
pub mod keyring;
pub mod modem_process;
pub mod network;
pub mod serial;

use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

pub const ENV_ALLOWLIST: &[&str] = &[
    // XDG
    "XDG_RUNTIME_DIR", "XDG_STATE_HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME",
    "XDG_CACHE_HOME", "XDG_CURRENT_DESKTOP", "XDG_SESSION_TYPE", "XDG_SESSION_DESKTOP",
    // D-Bus
    "DBUS_SESSION_BUS_ADDRESS", "DBUS_SYSTEM_BUS_ADDRESS",
    // Desktop
    "DESKTOP_SESSION", "WAYLAND_DISPLAY", "DISPLAY", "WAYLAND_SOCKET",
    // User
    "HOME", "USER", "LOGNAME",
    // Locale
    "LANG", "LC_ALL", "LC_CTYPE", "LC_MESSAGES", "LC_COLLATE",
    // Diagnostic basics
    "PATH", "PWD", "SHELL", "TERM", "TERM_PROGRAM", "COLORTERM",
    // Tuxlink overrides
    "TUXLINK_CONFIG_DIR", "TUXLINK_CMS_HOST", "TUXLINK_CMS_PORT", "TUXLINK_CMS_PLAINTEXT",
    "TUXLINK_GPSD_ADDR", "TUXLINK_VARA_TCP_HOST", "TUXLINK_VARA_TCP_PORT",
    "TUXLINK_ARDOP_TCP_HOST", "TUXLINK_ARDOP_TCP_PORT",
];

static ENV_VALUE_EXCLUSION: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(password|token|secret|key|auth|bearer|credential)").unwrap()
});

const PATH_LIKE_CAP_BYTES: usize = 500;

/// Safely read an environment variable: must be allowlisted; value redacted
/// if name OR value matches the exclusion regex; PATH-like values truncated.
pub fn safe_env_value(name: &str) -> Option<String> {
    if !ENV_ALLOWLIST.contains(&name) { return None; }
    let val = std::env::var(name).ok()?;
    if ENV_VALUE_EXCLUSION.is_match(name) || ENV_VALUE_EXCLUSION.is_match(&val) {
        return Some("<redacted>".into());
    }
    if val.len() > PATH_LIKE_CAP_BYTES {
        return Some(format!("{}…[truncated {} bytes]", &val[..PATH_LIKE_CAP_BYTES], val.len() - PATH_LIKE_CAP_BYTES));
    }
    Some(val)
}

/// Per-probe atomic state for debounce + single-flight.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeState { Idle = 0, Pending = 1, Running = 2 }

pub struct ProbeGate {
    state: AtomicU8,
    cooldown: Duration,
    last_completed: std::sync::Mutex<Option<Instant>>,
}

impl ProbeGate {
    pub const fn new() -> Self {
        Self {
            state: AtomicU8::new(0),
            cooldown: Duration::from_secs(60),
            last_completed: std::sync::Mutex::new(None),
        }
    }

    /// Try to claim the probe. Returns true if claimed (probe should run);
    /// false if already running OR within cooldown window.
    pub fn try_claim(&self) -> bool {
        if let Ok(last) = self.last_completed.lock() {
            if let Some(t) = *last {
                if t.elapsed() < self.cooldown {
                    return false;
                }
            }
        }
        self.state.compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire).is_ok()
    }

    pub fn release(&self) {
        if let Ok(mut last) = self.last_completed.lock() {
            *last = Some(Instant::now());
        }
        self.state.store(0, Ordering::Release);
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProbeSnapshot {
    pub probe: String,
    pub timestamp: String,
    pub trigger: String,
    pub result: serde_json::Value,
}

/// Run a subprocess with a per-command deadline (500 ms).
/// Returns the stdout string if the command exits in time, None otherwise.
/// The helper itself uses Command::new; probe files call this helper instead
/// of Command::new directly, which allows probes_no_tx_apis.rs to assert
/// that probe source files contain no Command::new.
pub fn run_with_deadline(cmd: &str, args: &[&str]) -> Option<String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut child = Command::new(cmd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let start = Instant::now();
    let deadline = Duration::from_millis(500);
    loop {
        if start.elapsed() >= deadline {
            // Reap to avoid a zombie process accumulating on each deadline hit:
            // std::process::Child::drop does NOT wait(), so kill() alone leaves
            // the kernel process table entry alive until the parent exits.
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        match child.try_wait() {
            Ok(Some(_)) => {
                let mut out = String::new();
                child.stdout.take()?.read_to_string(&mut out).ok()?;
                return Some(out);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => {
                // try_wait failure is rare (waitpid surfaced EINTR/ECHILD). Kill
                // + reap so we don't leak an orphan child still running in the
                // background.
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// Amendment E.5.8 backend: subscribe to the `first_paint_complete` Tauri event
/// and run all probes once in a background task. Results are emitted as
/// structured `tracing::info!` events AND broadcast via
/// `logging://probes/snapshot-updated` for the Logging window's live display.
///
/// RADIO-1: probes are read-only; no TX-touching paths (enforced at compile
/// time by tests/probes_no_tx_apis.rs).
///
/// On-error subsystem trigger: deferred — the Fanout Layer would need to expose
/// an error-broadcast tap (spec §9.2). Tracked as a follow-up gap; only the
/// first-paint trigger is implemented here.
pub fn spawn_runner(
    app: tauri::AppHandle,
    _handle: std::sync::Arc<crate::logging::LoggingHandle>,
) {
    use tauri::Listener;
    let app2 = app.clone();
    app.listen("first_paint_complete", move |_| {
        let a = app2.clone();
        tokio::spawn(async move {
            let snaps = vec![
                keyring::run("first_paint"),
                audio::run("first_paint"),
                serial::run("first_paint"),
                modem_process::run("first_paint"),
                network::run("first_paint"),
                display::run("first_paint"),
            ];
            // Use per-probe static target: directives so the Fanout Layer's
            // target-based routing uses the correct per-cluster target
            // (spec §4.1 verbosity matrix). The tracing macro requires
            // `target:` to be a string literal — dynamic values via `target =`
            // (equals sign) would add `target` as a structured field while
            // leaving the macro's actual target as the module path.
            for s in &snaps {
                match s.probe.as_str() {
                    "keyring" => tracing::info!(
                        target: "tuxlink::logging::env_probes::keyring",
                        trigger = "first_paint",
                        probe = "keyring",
                        "probe snapshot"
                    ),
                    "audio" => tracing::info!(
                        target: "tuxlink::logging::env_probes::audio",
                        trigger = "first_paint",
                        probe = "audio",
                        "probe snapshot"
                    ),
                    "serial" => tracing::info!(
                        target: "tuxlink::logging::env_probes::serial",
                        trigger = "first_paint",
                        probe = "serial",
                        "probe snapshot"
                    ),
                    "modem_process" => tracing::info!(
                        target: "tuxlink::logging::env_probes::modem_process",
                        trigger = "first_paint",
                        probe = "modem_process",
                        "probe snapshot"
                    ),
                    "network" => tracing::info!(
                        target: "tuxlink::logging::env_probes::network",
                        trigger = "first_paint",
                        probe = "network",
                        "probe snapshot"
                    ),
                    "display" => tracing::info!(
                        target: "tuxlink::logging::env_probes::display",
                        trigger = "first_paint",
                        probe = "display",
                        "probe snapshot"
                    ),
                    _ => {} // unknown probe; no emission
                }
            }
            use tauri::Emitter;
            let _ = a.emit("logging://probes/snapshot-updated", &snaps);
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn safe_env_value_blocks_non_allowlisted() {
        std::env::set_var("WINLINK_PASSWORD", "hunter2");
        let result = safe_env_value("WINLINK_PASSWORD");
        std::env::remove_var("WINLINK_PASSWORD");
        assert!(result.is_none());
    }

    #[test]
    fn safe_env_value_redacts_credential_named() {
        // If somehow an allowlist entry matched exclusion, it would redact.
        // Allowlist contains no credential names; we verify the exclusion
        // logic via direct call.
        let regex = &*ENV_VALUE_EXCLUSION;
        assert!(regex.is_match("MY_API_KEY"));
        assert!(regex.is_match("SOMETHING_PASSWORD"));
    }

    #[test]
    fn probe_gate_serializes_concurrent_claims() {
        let gate = ProbeGate::new();
        assert!(gate.try_claim());
        assert!(!gate.try_claim(), "second claim must fail while first holds");
        gate.release();
        // Even after release, cooldown blocks
        assert!(!gate.try_claim(), "cooldown must prevent immediate re-claim");
    }
}
