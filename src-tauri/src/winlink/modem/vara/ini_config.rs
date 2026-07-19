//! Agent-drivable VARA setup via `VARA.ini` edit + relaunch (tuxlink-iww9r).
//!
//! VARA persists ALL of its operator configuration to a plaintext `VARA.ini`
//! beside `VARA.exe` in its install dir under the WINE prefix. Editing that
//! file and relaunching VARA configures it deterministically — no GUI
//! automation. The pure INI layer (round-trip-preserving parse/set/render,
//! redaction) is the [`tuxlink_vara_ini`] leaf crate; this module owns
//! everything the leaf crate deliberately does not:
//!
//! - **Path resolution per WINE prefix + instance.** One prefix can hold two
//!   installs: the primary (`drive_c/VARA HF/` as provisioned by the vendored
//!   wine-vara-setup engine, or `drive_c/VARA/` as the stock installer lays
//!   down) and a second `drive_c/VARA2/` instance (differential/self-decode
//!   rig; both ship `VARA.exe` + `VARA.ini` under their own dir, verified
//!   against the live install on R2). The prefix itself is operator config —
//!   callers pass it; [`default_wine_prefix`] matches the engine's default.
//! - **The stop → edit → start lifecycle.** VARA rewrites `VARA.ini` on exit
//!   (it saves `[Position]` at minimum), so an edit made while VARA runs is
//!   clobbered. [`run_vara_ini_apply`] therefore stops any VARA it knows about
//!   (the [`VaraProcessSlot`] child, then the engine's `.vara.pid` daemon),
//!   verifies the cmd port actually went dark, waits for the INI's mtime to
//!   settle (absorbing the exit-time rewrite), and only then reads + edits.
//!   A listening cmd port that survives the stop chain means a VARA this app
//!   does not manage — the apply refuses rather than `pkill`ing anything.
//! - **Atomic write + timestamped backup.** The pre-edit file is copied to
//!   `VARA.ini.bak-<UTC>`; the new content lands via tmp-file + fsync +
//!   rename so a crash can never leave a half-written config.
//!
//! # RADIO-1
//!
//! Launching VARA opens its host TCP ports; it does not key a radio (ADR
//! 0018). Bouncing VARA mid-session WOULD kill a live ARQ link, so the apply
//! refuses while the app's VARA session is `Open`/`Connecting`.
//!
//! # Redaction
//!
//! `[Setup] Registration Code*` (paid license key) and `Password encryption`
//! flow through here in BOTH directions: reads surface only
//! [`VaraIni::redacted`] content, and [`VaraIniEdit`]'s manual `Debug` masks
//! sensitive values so a logged edit request can never leak the key.
//!
//! # Encoding
//!
//! VARA.ini is treated as UTF-8 and the edit path refuses a file that is not
//! (in practice the file is ASCII; a Windows-1252 device name with high bytes
//! would be corrupted by a lossy read, so refusing loudly beats guessing).

use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tuxlink_vara_ini::{is_sensitive_key, VaraIni};

use super::commands::{VaraSession, VaraState};
use super::transport::cmd_port_reachable;
use crate::winlink::modem::process::ManagedModem;

/// Tracing target for this module.
const LOG_TARGET: &str = "tuxlink::winlink::modem::vara::ini_config";

/// SIGINT → SIGKILL grace when stopping a VARA we spawned. Generous: a
/// graceful exit is when VARA writes its own INI, and we'd rather absorb that
/// write (via the settle wait) than SIGKILL mid-save.
const STOP_GRACE: Duration = Duration::from_secs(5);

/// How long to wait for the engine-daemonized pid to die after SIGTERM before
/// escalating to SIGKILL.
const PIDFILE_TERM_WAIT: Duration = Duration::from_secs(5);

/// After SIGKILL, how long to wait for the pid to vanish before giving up.
const PIDFILE_KILL_WAIT: Duration = Duration::from_secs(2);

/// After the stop chain, how long to allow the cmd port to finish going dark
/// before concluding an unmanaged VARA holds it.
const PORT_DOWN_WAIT: Duration = Duration::from_secs(3);

/// The INI is considered settled when its mtime has been unchanged this long.
const INI_SETTLE_STABLE: Duration = Duration::from_millis(500);

/// Upper bound on the settle wait (best-effort; a filesystem with coarse
/// mtime granularity must not stall the apply).
const INI_SETTLE_CAP: Duration = Duration::from_secs(5);

/// How long a relaunched VARA gets to open its cmd port. VB6 under WINE cold
/// starts slowly (OCX registration, soundcard probe); the engine's own
/// verify loop allows 30 s — give a little more headroom.
const LAUNCH_PORT_WAIT: Duration = Duration::from_secs(45);

/// Poll cadence for the launch port-wait and the port-down wait.
const PORT_POLL: Duration = Duration::from_millis(500);

/// Timeout for one TCP connect probe of the cmd port.
const PORT_PROBE_TIMEOUT: Duration = Duration::from_secs(1);

/// VARA's factory cmd port, used only when the INI does not carry
/// `[Setup] TCP Command Port` (e.g. first configure before first launch).
const DEFAULT_CMD_PORT: u16 = 8300;

// ─── Location resolution ────────────────────────────────────────────────────

/// Which VARA install inside the WINE prefix to target. One prefix can carry
/// both: the primary rig's VARA and a second `VARA2` install (differential /
/// self-decode rig). Serialized lowercase (`"primary"` / `"vara2"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VaraInstance {
    /// The primary VARA install: `drive_c/VARA HF/` (engine-provisioned) or
    /// `drive_c/VARA/` (stock installer default), probed in that order.
    #[default]
    Primary,
    /// The second install at `drive_c/VARA2/`.
    Vara2,
}

impl VaraInstance {
    /// Install-dir candidates under `drive_c/`, probed in order. The marker
    /// for "installed" is `VARA.exe` in the dir — the INI itself only appears
    /// after VARA's first run, and the apply path can create it.
    fn dir_candidates(self) -> &'static [&'static str] {
        match self {
            VaraInstance::Primary => &["VARA HF", "VARA"],
            VaraInstance::Vara2 => &["VARA2"],
        }
    }
}

/// The engine's default WINE prefix (`wv_prefix` in wine-vara-setup):
/// `$HOME/.local/share/wine-vara/prefix`. `None` when no home dir resolves.
pub fn default_wine_prefix() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".local/share/wine-vara/prefix"))
}

/// Resolve the VARA install dir (the one containing `VARA.exe`) for
/// `instance` under `prefix`. Errors enumerate every path probed so the
/// operator can see exactly what was looked for.
pub fn resolve_vara_dir(prefix: &Path, instance: VaraInstance) -> Result<PathBuf, String> {
    let mut probed: Vec<String> = Vec::new();
    for cand in instance.dir_candidates() {
        let dir = prefix.join("drive_c").join(cand);
        if dir.join("VARA.exe").is_file() {
            return Ok(dir);
        }
        probed.push(dir.join("VARA.exe").display().to_string());
    }
    Err(format!(
        "no {instance:?} VARA install under WINE prefix {} — probed: {}",
        prefix.display(),
        probed.join(", ")
    ))
}

// ─── Edit spec / report ─────────────────────────────────────────────────────

/// One `[section] key = value` assignment for the apply call.
#[derive(Clone, Serialize, Deserialize)]
pub struct VaraIniEdit {
    /// INI section name without brackets, e.g. `Soundcard`.
    pub section: String,
    /// Key exactly as VARA writes it, e.g. `Output Device Name`.
    pub key: String,
    /// New value, verbatim.
    pub value: String,
}

impl fmt::Debug for VaraIniEdit {
    /// Masks the value of sensitive keys (registration code / password) so a
    /// logged edit request can never leak them; other values (device names,
    /// ports) stay visible because they are what the log needs to show.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value: &str = if is_sensitive_key(&self.key) && !self.value.is_empty() {
            "<redacted>"
        } else {
            &self.value
        };
        f.debug_struct("VaraIniEdit")
            .field("section", &self.section)
            .field("key", &self.key)
            .field("value", &value)
            .finish()
    }
}

/// Outcome of one [`run_vara_ini_apply`] call.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaraIniApplyReport {
    /// Absolute path of the edited `VARA.ini`.
    pub ini_path: String,
    /// Path of the timestamped pre-edit backup; `None` when the INI did not
    /// exist yet (nothing to back up).
    pub backup_path: Option<String>,
    /// True when the INI was created fresh (install had never run / saved).
    pub created: bool,
    /// Number of edits applied.
    pub applied: usize,
    /// True when the apply relaunched VARA and its cmd port came up.
    pub relaunched: bool,
    /// The cmd port the (post-edit) config declares — the port a relaunch
    /// was verified against.
    pub cmd_port: u16,
}

// ─── Process slot ───────────────────────────────────────────────────────────

/// Holder for the VARA child this app spawned via the apply path. Managed as
/// Tauri state (`Arc<VaraProcessSlot>`); the whole apply runs under its lock
/// so two concurrent applies serialize instead of racing the stop/spawn.
/// Dropping the slot (app exit) reaps the child via [`ManagedModem`]'s Drop.
#[derive(Default)]
pub struct VaraProcessSlot {
    inner: Mutex<Option<ManagedModem>>,
}

// ─── Read path ──────────────────────────────────────────────────────────────

/// Read the resolved instance's `VARA.ini` and return its REDACTED content
/// (registration code / password masked). This is the only content-returning
/// read this module offers on purpose: agent-facing surfaces never need the
/// license key.
pub fn run_vara_ini_read(prefix: &Path, instance: VaraInstance) -> Result<String, String> {
    let dir = resolve_vara_dir(prefix, instance)?;
    let ini_path = dir.join("VARA.ini");
    match read_ini_strict(&ini_path)? {
        Some(ini) => Ok(ini.redacted()),
        None => Err(format!(
            "no VARA.ini at {} yet — VARA writes it on first run, or apply a config to create it",
            ini_path.display()
        )),
    }
}

// ─── Apply path ─────────────────────────────────────────────────────────────

/// Stop VARA, apply `edits` to its `VARA.ini` (atomic write + timestamped
/// backup), and — when `relaunch` — start VARA back up and wait for its cmd
/// port. See the module docs for the lifecycle rationale.
///
/// Refuses (never partially applies) when:
/// - the app's VARA session is `Open`/`Connecting` (a bounce would kill it),
/// - a VARA neither the slot nor the engine pidfile accounts for is still
///   listening after the stop chain (this module kills only processes it can
///   attribute; it never pattern-kills),
/// - the existing INI is not valid UTF-8,
/// - `edits` is empty and no relaunch was requested (nothing to do).
pub fn run_vara_ini_apply(
    slot: &VaraProcessSlot,
    session: Option<&VaraSession>,
    prefix: &Path,
    instance: VaraInstance,
    edits: &[VaraIniEdit],
    relaunch: bool,
) -> Result<VaraIniApplyReport, String> {
    run_vara_ini_apply_with(slot, session, prefix, instance, edits, relaunch, spawn_vara)
}

/// [`run_vara_ini_apply`] with an injectable launcher so tests can stand in a
/// harmless stub for `wine VARA.exe`. Production always passes [`spawn_vara`].
fn run_vara_ini_apply_with(
    slot: &VaraProcessSlot,
    session: Option<&VaraSession>,
    prefix: &Path,
    instance: VaraInstance,
    edits: &[VaraIniEdit],
    relaunch: bool,
    launcher: impl FnOnce(&Path, &Path) -> Result<ManagedModem, String>,
) -> Result<VaraIniApplyReport, String> {
    if edits.is_empty() && !relaunch {
        return Err("nothing to do: no edits given and no relaunch requested".to_string());
    }

    // A bounce would tear down a live ARQ link mid-exchange — refuse.
    if let Some(s) = session {
        let state = s.snapshot().state;
        if matches!(state, VaraState::Open | VaraState::Connecting) {
            return Err(format!(
                "a VARA session is {state:?} — close the VARA session before applying VARA.ini config (the apply bounces the modem)"
            ));
        }
    }

    let dir = resolve_vara_dir(prefix, instance)?;
    let ini_path = dir.join("VARA.ini");

    // Pre-read only to learn the currently-configured cmd port (the stop
    // chain needs it to verify VARA actually went dark). The authoritative
    // content read happens AFTER the stop + settle.
    let pre_port = read_ini_strict(&ini_path)?
        .as_ref()
        .and_then(configured_cmd_port)
        .unwrap_or(DEFAULT_CMD_PORT);

    // Serialize the whole apply on the slot lock: stop, edit, and relaunch
    // must not interleave with a concurrent apply.
    let mut slot_guard = slot
        .inner
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    // Stop chain, in attribution order: our own child first, then the
    // engine's daemonized pid. Anything still listening after both is a VARA
    // this app does not manage — refuse rather than guess at kills.
    if let Some(mut modem) = slot_guard.take() {
        tracing::info!(target: LOG_TARGET, "stopping slot-managed VARA for config apply");
        // Taken out of the slot first: even on a stop error the child is
        // dropped (ManagedModem's Drop escalates + reaps), never re-parked.
        modem
            .stop(STOP_GRACE)
            .map_err(|e| format!("failed to stop the managed VARA process: {e}"))?;
    }
    stop_engine_daemon(prefix)?;
    if !wait_port_down(pre_port, PORT_DOWN_WAIT) {
        return Err(format!(
            "a VARA instance is still listening on 127.0.0.1:{pre_port} but is not managed by \
             Tuxlink (no managed child, no {} pidfile) — close it manually, then retry",
            prefix.join(".vara.pid").display()
        ));
    }

    // VARA rewrites the INI on exit; wait for that write to land before
    // reading, or the edit would be based on (and back up) a stale snapshot.
    wait_for_stable_mtime(&ini_path, INI_SETTLE_STABLE, INI_SETTLE_CAP);

    let existing = read_ini_strict(&ini_path)?;
    let created = existing.is_none();
    let mut ini = existing.unwrap_or_else(|| VaraIni::parse(""));

    let backup_path = if created {
        None
    } else {
        let backup = ini_path.with_file_name(backup_file_name(&chrono::Utc::now()));
        fs::copy(&ini_path, &backup)
            .map_err(|e| format!("failed to back up {} to {}: {e}", ini_path.display(), backup.display()))?;
        Some(backup)
    };

    for edit in edits {
        tracing::info!(target: LOG_TARGET, edit = ?edit, "applying VARA.ini edit");
        ini.set(&edit.section, &edit.key, &edit.value);
    }

    // The edit itself may move the cmd port — the relaunch must be verified
    // against the port the NEW config declares.
    let cmd_port = configured_cmd_port(&ini).unwrap_or(pre_port);

    write_atomic(&ini_path, &ini.render())?;
    tracing::info!(
        target: LOG_TARGET,
        ini_path = %ini_path.display(),
        applied = edits.len(),
        created,
        "VARA.ini written",
    );

    let mut relaunched = false;
    if relaunch {
        let mut modem = launcher(&dir, prefix)?;
        let deadline = Instant::now() + LAUNCH_PORT_WAIT;
        loop {
            if cmd_port_reachable("127.0.0.1", cmd_port, PORT_PROBE_TIMEOUT) {
                break;
            }
            if !modem.is_running() {
                return Err(format!(
                    "VARA exited during startup (status: {:?}) — check the WINE prefix at {}",
                    modem.exit_status(),
                    prefix.display()
                ));
            }
            if Instant::now() >= deadline {
                let _ = modem.stop(Duration::from_secs(1));
                return Err(format!(
                    "VARA did not open cmd port {cmd_port} within {}s after relaunch",
                    LAUNCH_PORT_WAIT.as_secs()
                ));
            }
            std::thread::sleep(PORT_POLL);
        }
        *slot_guard = Some(modem);
        relaunched = true;
    }

    Ok(VaraIniApplyReport {
        ini_path: ini_path.display().to_string(),
        backup_path: backup_path.map(|p| p.display().to_string()),
        created,
        applied: edits.len(),
        relaunched,
        cmd_port,
    })
}

/// Spawn `wine VARA.exe` for the install at `dir` under `prefix`, mirroring
/// the engine's `wv_wineenv` + `wv_start_vara`: `WINEPREFIX` to the prefix,
/// `WINEDEBUG=-all` (VB6 under WINE is chatty), `WINEARCH=win64` (the wow64
/// prefix that carries syswow64, where VARA's VB6 runtime + OCX live), and
/// cwd = the install dir (VB6 resolves its `.dat` tables relative to cwd).
fn spawn_vara(dir: &Path, prefix: &Path) -> Result<ManagedModem, String> {
    let exe = dir.join("VARA.exe");
    let exe_str = exe.display().to_string();
    let prefix_str = prefix.display().to_string();
    let envs: [(&str, &str); 3] = [
        ("WINEPREFIX", prefix_str.as_str()),
        ("WINEDEBUG", "-all"),
        ("WINEARCH", "win64"),
    ];
    ManagedModem::spawn_configured("wine", &[exe_str.as_str()], &envs, Some(dir))
        .map_err(|e| format!("failed to launch VARA under WINE: {e}"))
}

// ─── Internals ──────────────────────────────────────────────────────────────

/// Read + parse the INI. `Ok(None)` when the file does not exist; an error
/// when it exists but is unreadable or not valid UTF-8 (refusing beats a
/// lossy read that would corrupt non-UTF-8 bytes on the round-trip).
fn read_ini_strict(path: &Path) -> Result<Option<VaraIni>, String> {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("failed to read {}: {e}", path.display())),
    };
    let content = String::from_utf8(bytes).map_err(|_| {
        format!(
            "{} contains non-UTF-8 bytes; refusing to edit it (a lossy rewrite would corrupt \
             the existing config)",
            path.display()
        )
    })?;
    Ok(Some(VaraIni::parse(&content)))
}

/// `[Setup] TCP Command Port` from the INI, when present and parseable.
fn configured_cmd_port(ini: &VaraIni) -> Option<u16> {
    ini.get("Setup", "TCP Command Port")
        .and_then(|v| v.trim().parse::<u16>().ok())
}

/// Backup filename for a given instant: `VARA.ini.bak-YYYYMMDDTHHMMSSZ`.
fn backup_file_name(now: &chrono::DateTime<chrono::Utc>) -> String {
    format!("VARA.ini.bak-{}", now.format("%Y%m%dT%H%M%SZ"))
}

/// Write `content` to `path` atomically: sibling tmp file, fsync, rename over
/// the target, best-effort fsync of the parent dir. A crash at any point
/// leaves either the old file or the new file — never a torn mix.
fn write_atomic(path: &Path, content: &str) -> Result<(), String> {
    let dir = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    let tmp = dir.join(".VARA.ini.tuxlink-tmp");
    {
        let mut f = fs::File::create(&tmp)
            .map_err(|e| format!("failed to create {}: {e}", tmp.display()))?;
        f.write_all(content.as_bytes())
            .map_err(|e| format!("failed to write {}: {e}", tmp.display()))?;
        f.sync_all()
            .map_err(|e| format!("failed to sync {}: {e}", tmp.display()))?;
    }
    fs::rename(&tmp, path)
        .map_err(|e| format!("failed to move {} into place: {e}", tmp.display()))?;
    // Make the rename itself durable; failure here only weakens crash
    // durability, it cannot tear the file, so best-effort.
    if let Ok(d) = fs::File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

/// Wait until `path`'s mtime has been stable for `stable_for`, bounded by
/// `cap`. Returns immediately when the file does not exist. Best-effort: on
/// a filesystem with coarse mtime granularity the cap bounds the stall.
fn wait_for_stable_mtime(path: &Path, stable_for: Duration, cap: Duration) {
    let start = Instant::now();
    let mut last_mtime = match fs::metadata(path).and_then(|m| m.modified()) {
        Ok(t) => t,
        Err(_) => return,
    };
    let mut unchanged_since = Instant::now();
    loop {
        if unchanged_since.elapsed() >= stable_for || start.elapsed() >= cap {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
        match fs::metadata(path).and_then(|m| m.modified()) {
            Ok(t) if t == last_mtime => {}
            Ok(t) => {
                last_mtime = t;
                unchanged_since = Instant::now();
            }
            Err(_) => return,
        }
    }
}

/// Stop the engine-daemonized VARA recorded in `<prefix>/.vara.pid`, if any.
/// Mirrors the engine's `wv_stop`: the pid is validated against its
/// `/proc/<pid>/cmdline` (must mention wine or VARA) before any signal, so a
/// stale or reused pid can never make us kill an unrelated process. Returns
/// `Ok(true)` when a daemon was stopped, `Ok(false)` when there was nothing
/// (or only a stale pidfile, which is removed).
fn stop_engine_daemon(prefix: &Path) -> Result<bool, String> {
    let pidfile = prefix.join(".vara.pid");
    let raw = match fs::read_to_string(&pidfile) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(format!("failed to read {}: {e}", pidfile.display())),
    };
    let pid: i32 = match raw.trim().parse() {
        Ok(p) => p,
        Err(_) => {
            tracing::warn!(target: LOG_TARGET, pidfile = %pidfile.display(), "removing malformed VARA pidfile");
            let _ = fs::remove_file(&pidfile);
            return Ok(false);
        }
    };
    let cmdline = fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
    let cmdline = String::from_utf8_lossy(&cmdline).to_lowercase();
    if !(cmdline.contains("wine") || cmdline.contains("vara")) {
        tracing::warn!(
            target: LOG_TARGET,
            pid,
            "VARA pidfile is stale (pid gone or not a wine/VARA process); removing it, not killing",
        );
        let _ = fs::remove_file(&pidfile);
        return Ok(false);
    }

    tracing::info!(target: LOG_TARGET, pid, "stopping engine-daemonized VARA");
    let nix_pid = nix::unistd::Pid::from_raw(pid);
    let _ = nix::sys::signal::kill(nix_pid, nix::sys::signal::Signal::SIGTERM);
    if !wait_pid_gone(pid, PIDFILE_TERM_WAIT) {
        let _ = nix::sys::signal::kill(nix_pid, nix::sys::signal::Signal::SIGKILL);
        if !wait_pid_gone(pid, PIDFILE_KILL_WAIT) {
            return Err(format!(
                "engine-daemonized VARA (pid {pid}) survived SIGTERM and SIGKILL"
            ));
        }
    }
    let _ = fs::remove_file(&pidfile);
    Ok(true)
}

/// Poll until `/proc/<pid>` disappears, bounded by `timeout`.
fn wait_pid_gone(pid: i32, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if !Path::new(&format!("/proc/{pid}")).exists() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Poll until nothing accepts on `127.0.0.1:port`, bounded by `timeout`.
/// True = the port went dark.
fn wait_port_down(port: u16, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if !cmd_port_reachable("127.0.0.1", port, PORT_PROBE_TIMEOUT) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(PORT_POLL);
    }
}

// ─── Tauri commands ─────────────────────────────────────────────────────────

/// Resolve the optional prefix argument: expand a leading `~/`, fall back to
/// [`default_wine_prefix`] when absent/blank.
fn resolve_prefix_arg(prefix: Option<String>) -> Result<PathBuf, String> {
    match prefix.map(|p| p.trim().to_string()).filter(|p| !p.is_empty()) {
        Some(p) => {
            if let Some(rest) = p.strip_prefix("~/") {
                let home = dirs::home_dir()
                    .ok_or_else(|| "cannot expand '~': no home directory".to_string())?;
                Ok(home.join(rest))
            } else {
                Ok(PathBuf::from(p))
            }
        }
        None => default_wine_prefix()
            .ok_or_else(|| "no WINE prefix given and no home directory to default under".to_string()),
    }
}

/// Redacted `VARA.ini` content for the resolved prefix + instance.
/// `prefix` `None` → the engine default prefix; `instance` `None` → primary.
#[tauri::command]
pub async fn vara_ini_read(
    prefix: Option<String>,
    instance: Option<VaraInstance>,
) -> Result<String, String> {
    let prefix = resolve_prefix_arg(prefix)?;
    let instance = instance.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || run_vara_ini_read(&prefix, instance))
        .await
        .map_err(|e| format!("join: {e}"))?
}

/// Stop-edit-start apply of `edits` to the resolved `VARA.ini`. `relaunch`
/// defaults to true (the point of the bounce is a configured, running VARA).
/// Blocking work (process stop, settle wait, port wait) runs off the async
/// runtime.
#[tauri::command]
pub async fn vara_ini_apply(
    slot: tauri::State<'_, std::sync::Arc<VaraProcessSlot>>,
    session: tauri::State<'_, std::sync::Arc<VaraSession>>,
    prefix: Option<String>,
    instance: Option<VaraInstance>,
    edits: Vec<VaraIniEdit>,
    relaunch: Option<bool>,
) -> Result<VaraIniApplyReport, String> {
    let slot = std::sync::Arc::clone(&slot);
    let session = std::sync::Arc::clone(&session);
    let prefix = resolve_prefix_arg(prefix)?;
    let instance = instance.unwrap_or_default();
    let relaunch = relaunch.unwrap_or(true);
    tauri::async_runtime::spawn_blocking(move || {
        run_vara_ini_apply(&slot, Some(&session), &prefix, instance, &edits, relaunch)
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// A structurally-real CRLF VARA.ini with a caller-chosen cmd port.
    fn sample_ini(port: u16) -> String {
        format!(
            "[Soundcard]\r\nInput Device Name=USB PnP Sound Device Mono\r\nOutput Device Name=USB PnP Sound Device Analog Ste\r\nALC Drive Level=-15\r\n[Setup]\r\nRegistration Code=FAKEFAKEFAKE1234\r\nTCP Command Port={port}\r\n[Position]\r\nTop Position=3060\r\n"
        )
    }

    /// Build `prefix/drive_c/<dir>/VARA.exe` (+ optional VARA.ini) in a
    /// tempdir and return (tempdir-guard, prefix, install-dir).
    fn fake_install(dir_name: &str, ini: Option<&str>) -> (tempfile::TempDir, PathBuf, PathBuf) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let prefix = tmp.path().join("prefix");
        let install = prefix.join("drive_c").join(dir_name);
        fs::create_dir_all(&install).expect("mkdir install");
        fs::write(install.join("VARA.exe"), b"not a real exe").expect("touch exe");
        if let Some(content) = ini {
            fs::write(install.join("VARA.ini"), content).expect("write ini");
        }
        (tmp, prefix, install)
    }

    /// A local port that was just free (bound then released).
    fn free_port() -> u16 {
        let l = TcpListener::bind(("127.0.0.1", 0)).expect("bind :0");
        l.local_addr().expect("addr").port()
    }

    // ── Resolution ────────────────────────────────────────────────────────

    #[test]
    fn resolver_prefers_vara_hf_then_vara_for_primary() {
        let (_g, prefix, install) = fake_install("VARA", None);
        assert_eq!(
            resolve_vara_dir(&prefix, VaraInstance::Primary).expect("resolve"),
            install,
            "with only drive_c/VARA present, primary resolves to it"
        );
        // Now add the engine layout — it must win.
        let hf = prefix.join("drive_c").join("VARA HF");
        fs::create_dir_all(&hf).expect("mkdir hf");
        fs::write(hf.join("VARA.exe"), b"x").expect("touch");
        assert_eq!(
            resolve_vara_dir(&prefix, VaraInstance::Primary).expect("resolve"),
            hf,
            "drive_c/VARA HF must be preferred over drive_c/VARA"
        );
    }

    #[test]
    fn resolver_finds_vara2_and_never_crosses_instances() {
        let (_g, prefix, install) = fake_install("VARA2", None);
        assert_eq!(
            resolve_vara_dir(&prefix, VaraInstance::Vara2).expect("resolve"),
            install
        );
        let err = resolve_vara_dir(&prefix, VaraInstance::Primary).expect_err("no primary");
        assert!(err.contains("VARA HF"), "error must enumerate probed paths: {err}");
        assert!(err.contains("Primary"), "error names the instance: {err}");
    }

    #[test]
    fn vara_instance_serde_shape_is_lowercase_tags() {
        // reference_serde_rename_all_enum_fields: prove the wire tags.
        assert_eq!(serde_json::to_string(&VaraInstance::Primary).unwrap(), "\"primary\"");
        assert_eq!(
            serde_json::from_str::<VaraInstance>("\"vara2\"").unwrap(),
            VaraInstance::Vara2
        );
    }

    // ── Pure helpers ──────────────────────────────────────────────────────

    #[test]
    fn backup_name_is_utc_timestamped() {
        let t = chrono::DateTime::parse_from_rfc3339("2026-07-18T01:02:03Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert_eq!(backup_file_name(&t), "VARA.ini.bak-20260718T010203Z");
    }

    #[test]
    fn write_atomic_replaces_and_leaves_no_tmp() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("VARA.ini");
        fs::write(&path, "old").expect("seed");
        write_atomic(&path, "new content").expect("atomic write");
        assert_eq!(fs::read_to_string(&path).unwrap(), "new content");
        assert!(
            !tmp.path().join(".VARA.ini.tuxlink-tmp").exists(),
            "tmp file must not survive the rename"
        );
    }

    #[test]
    fn edit_debug_redacts_registration_code_but_not_device_names() {
        let secret = VaraIniEdit {
            section: "Setup".into(),
            key: "Registration Code".into(),
            value: "FAKEFAKEFAKE1234".into(),
        };
        let dbg = format!("{secret:?}");
        assert!(!dbg.contains("FAKEFAKEFAKE1234"), "must not leak the key: {dbg}");
        assert!(dbg.contains("<redacted>"));

        let device = VaraIniEdit {
            section: "Soundcard".into(),
            key: "Output Device Name".into(),
            value: "USB Audio CODEC".into(),
        };
        assert!(format!("{device:?}").contains("USB Audio CODEC"), "device names stay visible");
    }

    #[test]
    fn stable_mtime_returns_quickly_for_static_or_missing_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("VARA.ini");
        // Missing file: immediate return.
        let t0 = Instant::now();
        wait_for_stable_mtime(&path, Duration::from_millis(200), Duration::from_secs(5));
        assert!(t0.elapsed() < Duration::from_millis(150), "missing file must not wait");
        // Static file: returns after ~stable_for, well under the cap.
        fs::write(&path, "x").unwrap();
        let t1 = Instant::now();
        wait_for_stable_mtime(&path, Duration::from_millis(200), Duration::from_secs(5));
        assert!(t1.elapsed() < Duration::from_secs(2), "static file must settle fast");
    }

    // ── Stop chain ────────────────────────────────────────────────────────

    #[test]
    fn stale_pidfile_is_removed_not_killed() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let prefix = tmp.path().to_path_buf();
        // Malformed content.
        fs::write(prefix.join(".vara.pid"), "not-a-pid").unwrap();
        assert_eq!(stop_engine_daemon(&prefix), Ok(false));
        assert!(!prefix.join(".vara.pid").exists(), "malformed pidfile removed");
        // A live pid whose cmdline mentions neither wine nor VARA (a `sleep`
        // child we own — NOT this test process, whose binary path may contain
        // "vara" in a vara-named worktree): validated, treated stale, and the
        // process must NOT be signalled.
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep child");
        fs::write(prefix.join(".vara.pid"), child.id().to_string()).unwrap();
        assert_eq!(stop_engine_daemon(&prefix), Ok(false));
        assert!(!prefix.join(".vara.pid").exists(), "non-VARA pidfile removed");
        assert!(
            child.try_wait().expect("try_wait").is_none(),
            "the innocent process must still be running"
        );
        child.kill().expect("cleanup kill");
        let _ = child.wait();
    }

    // ── Apply orchestration ───────────────────────────────────────────────

    #[test]
    fn apply_edits_backs_up_and_preserves_unknown_content() {
        let port = free_port();
        let (_g, prefix, install) = fake_install("VARA HF", Some(&sample_ini(port)));
        let slot = VaraProcessSlot::default();
        let edits = [VaraIniEdit {
            section: "Soundcard".into(),
            key: "Output Device Name".into(),
            value: "USB Audio CODEC Analog Stereo".into(),
        }];
        let report =
            run_vara_ini_apply(&slot, None, &prefix, VaraInstance::Primary, &edits, false)
                .expect("apply");

        assert_eq!(report.applied, 1);
        assert!(!report.created);
        assert!(!report.relaunched);
        assert_eq!(report.cmd_port, port);

        let written = fs::read_to_string(install.join("VARA.ini")).unwrap();
        assert!(written.contains("Output Device Name=USB Audio CODEC Analog Stereo"));
        assert!(written.contains("Top Position=3060"), "unknown sections preserved");
        assert!(written.contains("\r\n"), "CRLF preserved");

        let backup = report.backup_path.expect("backup must exist");
        assert_eq!(
            fs::read_to_string(&backup).unwrap(),
            sample_ini(port),
            "backup is the byte-exact pre-edit file"
        );
    }

    #[test]
    fn apply_creates_ini_when_absent_without_backup() {
        let (_g, prefix, install) = fake_install("VARA", None);
        let slot = VaraProcessSlot::default();
        let edits = [VaraIniEdit {
            section: "Soundcard".into(),
            key: "Output Device Name".into(),
            value: "USB Audio CODEC".into(),
        }];
        let report =
            run_vara_ini_apply(&slot, None, &prefix, VaraInstance::Primary, &edits, false)
                .expect("apply");
        assert!(report.created);
        assert!(report.backup_path.is_none());
        assert_eq!(report.cmd_port, DEFAULT_CMD_PORT);
        let written = fs::read_to_string(install.join("VARA.ini")).unwrap();
        assert!(written.contains("[Soundcard]"));
        assert!(written.contains("Output Device Name=USB Audio CODEC"));
    }

    #[test]
    fn apply_refuses_unmanaged_listener_on_the_cmd_port() {
        // Keep a listener alive on the INI's port for the whole apply.
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let port = listener.local_addr().unwrap().port();
        let (_g, prefix, _install) = fake_install("VARA HF", Some(&sample_ini(port)));
        let slot = VaraProcessSlot::default();
        let edits = [VaraIniEdit {
            section: "Soundcard".into(),
            key: "ALC Drive Level".into(),
            value: "-10".into(),
        }];
        let err = run_vara_ini_apply(&slot, None, &prefix, VaraInstance::Primary, &edits, false)
            .expect_err("must refuse");
        assert!(err.contains("not managed by"), "refusal names the cause: {err}");
        // And the INI was NOT touched.
        let content = fs::read_to_string(prefix.join("drive_c/VARA HF/VARA.ini")).unwrap();
        assert_eq!(content, sample_ini(port), "no partial apply on refusal");
        drop(listener);
    }

    #[test]
    fn apply_refuses_when_session_is_open() {
        let port = free_port();
        let (_g, prefix, _install) = fake_install("VARA HF", Some(&sample_ini(port)));
        let slot = VaraProcessSlot::default();
        let session = VaraSession::new();
        session.set_state_for_test(VaraState::Open);
        let edits = [VaraIniEdit {
            section: "Soundcard".into(),
            key: "ALC Drive Level".into(),
            value: "-10".into(),
        }];
        let err = run_vara_ini_apply(
            &slot,
            Some(&session),
            &prefix,
            VaraInstance::Primary,
            &edits,
            false,
        )
        .expect_err("must refuse while a session is open");
        assert!(err.contains("close the VARA session"), "{err}");
    }

    #[test]
    fn apply_with_no_edits_and_no_relaunch_is_an_error() {
        let (_g, prefix, _install) = fake_install("VARA HF", Some(&sample_ini(free_port())));
        let slot = VaraProcessSlot::default();
        let err = run_vara_ini_apply(&slot, None, &prefix, VaraInstance::Primary, &[], false)
            .expect_err("nothing to do");
        assert!(err.contains("nothing to do"), "{err}");
    }

    #[test]
    fn apply_refuses_non_utf8_ini() {
        let port = free_port();
        let (_g, prefix, install) = fake_install("VARA HF", None);
        let mut bytes = sample_ini(port).into_bytes();
        bytes.push(0xE9); // a lone Windows-1252 'é'
        fs::write(install.join("VARA.ini"), bytes).unwrap();
        let slot = VaraProcessSlot::default();
        let edits = [VaraIniEdit {
            section: "Soundcard".into(),
            key: "ALC Drive Level".into(),
            value: "-10".into(),
        }];
        let err = run_vara_ini_apply(&slot, None, &prefix, VaraInstance::Primary, &edits, false)
            .expect_err("must refuse non-UTF-8");
        assert!(err.contains("non-UTF-8"), "{err}");
    }

    // ── Relaunch (stub launcher) ──────────────────────────────────────────

    /// Stub launcher: a python3 child that binds the given port and idles,
    /// standing in for `wine VARA.exe` opening the cmd port.
    fn stub_launcher_binding(port: u16) -> impl FnOnce(&Path, &Path) -> Result<ManagedModem, String>
    {
        move |_dir: &Path, _prefix: &Path| {
            let script = format!(
                "import socket,time\ns=socket.socket()\ns.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1)\ns.bind(('127.0.0.1',{port}))\ns.listen(1)\ntime.sleep(60)"
            );
            ManagedModem::spawn("python3", &["-c", &script])
                .map_err(|e| format!("stub spawn: {e}"))
        }
    }

    #[test]
    fn apply_relaunch_waits_for_cmd_port_and_parks_child_in_slot() {
        let port = free_port();
        let (_g, prefix, _install) = fake_install("VARA HF", Some(&sample_ini(port)));
        let slot = VaraProcessSlot::default();
        let edits = [VaraIniEdit {
            section: "Soundcard".into(),
            key: "ALC Drive Level".into(),
            value: "-12".into(),
        }];
        let report = run_vara_ini_apply_with(
            &slot,
            None,
            &prefix,
            VaraInstance::Primary,
            &edits,
            true,
            stub_launcher_binding(port),
        )
        .expect("apply with relaunch");
        assert!(report.relaunched);
        assert_eq!(report.cmd_port, port);

        // The child is parked in the slot and still running; stop it cleanly.
        let mut guard = slot.inner.lock().unwrap();
        let modem = guard.as_mut().expect("slot must hold the relaunched child");
        assert!(modem.is_running());
        modem.stop(Duration::from_secs(2)).expect("stop stub");
        *guard = None;
    }

    #[test]
    fn apply_relaunch_reports_a_child_that_dies_on_startup() {
        let port = free_port();
        let (_g, prefix, _install) = fake_install("VARA HF", Some(&sample_ini(port)));
        let slot = VaraProcessSlot::default();
        let err = run_vara_ini_apply_with(
            &slot,
            None,
            &prefix,
            VaraInstance::Primary,
            &[],
            true,
            |_dir, _prefix| {
                ManagedModem::spawn("/bin/sh", &["-c", "exit 3"]).map_err(|e| format!("{e}"))
            },
        )
        .expect_err("dead child must surface");
        assert!(err.contains("exited during startup"), "{err}");
        assert!(slot.inner.lock().unwrap().is_none(), "slot stays empty on failure");
    }
}
