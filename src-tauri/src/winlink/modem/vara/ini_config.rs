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
//!   clobbered. [`run_vara_ini_apply`] therefore stops any VARA it can
//!   ATTRIBUTE to this prefix + instance (the [`VaraProcessSlot`] child, then
//!   the engine's `.vara.pid` daemon — validated against the resolved
//!   `VARA.exe` path / `WINEPREFIX`, never a bare "looks like wine" grep),
//!   verifies the cmd port actually went dark, waits for the INI's mtime to
//!   settle (absorbing the exit-time rewrite), and only then reads + edits.
//!   A listening cmd port that survives the stop chain means a VARA this app
//!   does not manage — the apply refuses rather than `pkill`ing anything.
//!   The stop+edit window runs under the session inner mutex
//!   ([`VaraSession::with_session_excluded`]) so a concurrent
//!   `vara_open_session` serializes against the bounce instead of racing it.
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

use super::commands::VaraSession;
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

    /// Factory-default cmd port when the INI does not declare one. Only the
    /// primary install has a safe default (8300). A second instance MUST run
    /// on a different port to coexist, and there is no trustworthy universal
    /// second-instance default — so `Vara2` has none and callers must get the
    /// port from the INI or the edits (Codex 2026-07-18 P2 #4).
    fn default_cmd_port(self) -> Option<u16> {
        match self {
            VaraInstance::Primary => Some(DEFAULT_CMD_PORT),
            VaraInstance::Vara2 => None,
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
    /// was verified against. `None` when it is unknowable: a second-instance
    /// (`vara2`) install whose INI carries no `[Setup] TCP Command Port` and
    /// whose edits did not set one (the primary's 8300 factory default is
    /// deliberately NOT assumed for VARA2; a relaunch refuses in that case).
    pub cmd_port: Option<u16>,
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

    let dir = resolve_vara_dir(prefix, instance)?;
    let ini_path = dir.join("VARA.ini");

    // Serialize the whole apply on the slot lock: stop, edit, and relaunch
    // must not interleave with a concurrent apply.
    let mut slot_guard = slot
        .inner
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    // The stop+edit window. When a session handle is present it runs under
    // the session's inner mutex ([`VaraSession::with_session_excluded`]) —
    // `vara_open_session` holds that same mutex across its connect, so an
    // open can never interleave with the bounce (Codex 2026-07-18 P1 #2).
    // The relaunch port-wait stays OUTSIDE the session mutex: WINE cold
    // starts take tens of seconds and `snapshot()` (the UI status poll)
    // must not block for that long.
    let slot_ref = &mut *slot_guard;
    let stop_edit = || -> Result<(Option<PathBuf>, bool, Option<u16>), String> {
        // Pre-read only to learn the currently-configured cmd port (the
        // stop chain needs it to verify VARA actually went dark). The
        // authoritative content read happens AFTER the stop + settle.
        let pre_port = read_ini_strict(&ini_path)?
            .as_ref()
            .and_then(configured_cmd_port)
            .or_else(|| instance.default_cmd_port());

        // Stop chain, in attribution order: our own child first, then the
        // engine's daemonized pid (only when attributable to THIS prefix +
        // instance). Anything still listening after both is a VARA this app
        // does not manage — refuse rather than guess at kills.
        if let Some(mut modem) = slot_ref.take() {
            tracing::info!(target: LOG_TARGET, "stopping slot-managed VARA for config apply");
            // Taken out of the slot first: even on a stop error the child is
            // dropped (ManagedModem's Drop escalates + reaps), never re-parked.
            modem
                .stop(STOP_GRACE)
                .map_err(|e| format!("failed to stop the managed VARA process: {e}"))?;
        }
        stop_engine_daemon(prefix, &dir)?;
        if let Some(port) = pre_port {
            if !wait_port_down(port, PORT_DOWN_WAIT) {
                return Err(format!(
                    "a VARA instance is still listening on 127.0.0.1:{port} but is not managed \
                     by Tuxlink (no managed child, no attributable {} pidfile) — close it \
                     manually, then retry",
                    prefix.join(".vara.pid").display()
                ));
            }
        }

        // VARA rewrites the INI on exit; wait for that write to land before
        // reading, or the edit would be based on (and back up) a stale snapshot.
        wait_for_stable_mtime(&ini_path, INI_SETTLE_STABLE, INI_SETTLE_CAP);

        let existing = read_ini_strict(&ini_path)?;
        let created = existing.is_none();
        let mut ini = existing.unwrap_or_else(|| VaraIni::parse(""));

        // The edits may move the cmd port; the LAST port edit wins, exactly
        // like the post-edit INI state. Resolved BEFORE any mutation so every
        // refusal below leaves the file untouched.
        let edited_port = edits
            .iter()
            .rev()
            .find(|e| e.section == "Setup" && e.key == "TCP Command Port")
            .and_then(|e| e.value.trim().parse::<u16>().ok());
        let cmd_port = edited_port.or_else(|| configured_cmd_port(&ini)).or(pre_port);

        if relaunch {
            let Some(port) = cmd_port else {
                return Err(
                    "cannot relaunch the second VARA instance without a known cmd port — set \
                     [Setup] TCP Command Port in the edits (the primary's 8300 factory default \
                     is not assumed for VARA2)"
                        .to_string(),
                );
            };
            // The stop chain only verified the PRE-edit port went dark. If
            // the target port differs, an unrelated listener there (e.g. the
            // other VARA instance) would fake the launch verification —
            // refuse before touching the file (Codex 2026-07-18 P2 #3).
            if cmd_port_reachable("127.0.0.1", port, PORT_PROBE_TIMEOUT) {
                return Err(format!(
                    "cmd port {port} is already in use by another process — VARA could not bind \
                     it; pick a free port or stop whatever holds it"
                ));
            }
        }

        let backup_path = if created { None } else { Some(create_backup(&ini_path)?) };

        for edit in edits {
            tracing::info!(target: LOG_TARGET, edit = ?edit, "applying VARA.ini edit");
            ini.set(&edit.section, &edit.key, &edit.value);
        }

        write_atomic(&ini_path, &ini.render())?;
        tracing::info!(
            target: LOG_TARGET,
            ini_path = %ini_path.display(),
            applied = edits.len(),
            created,
            "VARA.ini written",
        );
        Ok((backup_path, created, cmd_port))
    };
    let (backup_path, created, cmd_port) = match session {
        Some(s) => s.with_session_excluded(stop_edit)?,
        None => stop_edit()?,
    };

    let mut relaunched = false;
    if relaunch {
        let port = cmd_port.expect("relaunch requires a known cmd port (validated in stop_edit)");
        let mut modem = launcher(&dir, prefix)?;
        let deadline = Instant::now() + LAUNCH_PORT_WAIT;
        loop {
            if cmd_port_reachable("127.0.0.1", port, PORT_PROBE_TIMEOUT) {
                break;
            }
            if !modem.is_running() {
                return Err(format!(
                    "VARA exited during startup (status: {:?}) — the INI edit itself was applied \
                     and backed up; check the WINE prefix at {}",
                    modem.exit_status(),
                    prefix.display()
                ));
            }
            if Instant::now() >= deadline {
                let _ = modem.stop(Duration::from_secs(1));
                return Err(format!(
                    "VARA did not open cmd port {port} within {}s after relaunch — the INI edit \
                     itself was applied and backed up",
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

/// Copy the pre-edit `VARA.ini` to a timestamped sibling, collision-safe:
/// the timestamp has second resolution, so a same-second sibling gets a
/// `-1`/`-2`/… suffix via `create_new` (which can never overwrite an earlier
/// backup — Codex 2026-07-18 P2 #5). Returns the backup path.
fn create_backup(ini_path: &Path) -> Result<PathBuf, String> {
    let original = fs::read(ini_path)
        .map_err(|e| format!("failed to read {} for backup: {e}", ini_path.display()))?;
    let base = backup_file_name(&chrono::Utc::now());
    for attempt in 0..100u32 {
        let name = if attempt == 0 {
            base.clone()
        } else {
            format!("{base}-{attempt}")
        };
        let dest = ini_path.with_file_name(name);
        match fs::OpenOptions::new().write(true).create_new(true).open(&dest) {
            Ok(mut f) => {
                f.write_all(&original)
                    .map_err(|e| format!("failed to write backup {}: {e}", dest.display()))?;
                f.sync_all()
                    .map_err(|e| format!("failed to sync backup {}: {e}", dest.display()))?;
                return Ok(dest);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("failed to create backup {}: {e}", dest.display())),
        }
    }
    Err(format!(
        "could not find a free backup filename beside {} after 100 attempts",
        ini_path.display()
    ))
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

/// How a process cmdline relates to a specific VARA install. See
/// [`cmdline_matches_install`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstallMatch {
    /// argv carries the full unix exe path — prefix AND instance attributed.
    Full,
    /// argv carries only the instance-dir form (`…\VARA HF\VARA.exe` or a
    /// foreign-prefix unix path) — instance attributed, prefix NOT; the
    /// caller must confirm the prefix via the process environment.
    InstanceOnly,
    /// No relation to this install.
    No,
}

/// Classify a (lowercased, NUL→space) `/proc/<pid>/cmdline` against the VARA
/// install at `install_dir`. The engine spawns `wine <unix exe path>`, so a
/// fresh spawn matches `Full`; wine may rewrite argv to the windows form
/// (`C:\<dir>\VARA.exe`), which loses the prefix and only matches
/// `InstanceOnly`. The dir-name patterns require a leading separator so
/// `…\myvara\vara.exe` can never false-match `\vara\vara.exe`, and the
/// primary/`VARA2` dir names cannot cross-match each other.
fn cmdline_matches_install(cmdline_lower: &str, install_dir: &Path) -> InstallMatch {
    let exe_unix = install_dir.join("VARA.exe").display().to_string().to_lowercase();
    if cmdline_lower.contains(&exe_unix) {
        return InstallMatch::Full;
    }
    let dir_name = install_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if dir_name.is_empty() {
        return InstallMatch::No;
    }
    let win_form = format!("\\{dir_name}\\vara.exe");
    let unix_form = format!("/{dir_name}/vara.exe");
    if cmdline_lower.contains(&win_form) || cmdline_lower.contains(&unix_form) {
        InstallMatch::InstanceOnly
    } else {
        InstallMatch::No
    }
}

/// True iff `/proc/<pid>/environ` records `WINEPREFIX=<prefix>`. Unreadable
/// environ (process died, or not ours to read) → false: attribution fails
/// closed and the caller leaves the process alone.
fn environ_has_wineprefix(pid: i32, prefix: &Path) -> bool {
    let environ = fs::read(format!("/proc/{pid}/environ")).unwrap_or_default();
    let needle = format!("WINEPREFIX={}", prefix.display());
    environ
        .split(|b| *b == 0)
        .any(|entry| String::from_utf8_lossy(entry) == needle.as_str())
}

/// Stop the engine-daemonized VARA recorded in `<prefix>/.vara.pid`, if any.
/// Stricter than the engine's `wv_stop` grep: before any signal the pid must
/// be ATTRIBUTED to the install at `install_dir` under `prefix` — the full
/// unix exe path in argv, or the instance-dir form plus `WINEPREFIX` in the
/// process environment (Codex 2026-07-18 P1 #1). A wine/VARA-ish process
/// that fails attribution (another instance, another prefix) is left alone
/// AND its pidfile is left alone — it may be that other install's live
/// daemon. A pid that is dead, malformed, or reused by something that is
/// neither wine nor VARA is a stale record: pidfile removed, nothing
/// signalled. Returns `Ok(true)` iff a daemon was stopped.
fn stop_engine_daemon(prefix: &Path, install_dir: &Path) -> Result<bool, String> {
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
    let cmdline = String::from_utf8_lossy(&cmdline)
        .replace('\0', " ")
        .to_lowercase();
    if cmdline.trim().is_empty() || !(cmdline.contains("wine") || cmdline.contains("vara")) {
        tracing::warn!(
            target: LOG_TARGET,
            pid,
            "VARA pidfile is stale (pid gone or reused by a non-wine/VARA process); removing it, not killing",
        );
        let _ = fs::remove_file(&pidfile);
        return Ok(false);
    }
    let ours = match cmdline_matches_install(&cmdline, install_dir) {
        InstallMatch::Full => true,
        InstallMatch::InstanceOnly => environ_has_wineprefix(pid, prefix),
        InstallMatch::No => false,
    };
    if !ours {
        tracing::warn!(
            target: LOG_TARGET,
            pid,
            install_dir = %install_dir.display(),
            "pidfile records a wine/VARA process not attributable to this prefix+instance; \
             leaving process and pidfile alone",
        );
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
    use crate::winlink::modem::vara::commands::VaraState;
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

    /// Codex P1 #1: the attribution matcher. Full unix path → Full; wine's
    /// argv-rewritten windows form → InstanceOnly; other instances / lookalike
    /// dirs never match.
    #[test]
    fn cmdline_attribution_matches_exact_install_only() {
        let hf = Path::new("/home/op/.wine-vara/drive_c/VARA HF");
        let vara = Path::new("/home/op/.wine-vara/drive_c/VARA");
        let vara2 = Path::new("/home/op/.wine-vara/drive_c/VARA2");

        // Engine spawn form: full unix path in argv.
        let engine_argv = "wine /home/op/.wine-vara/drive_c/vara hf/vara.exe";
        assert_eq!(cmdline_matches_install(engine_argv, hf), InstallMatch::Full);
        assert_eq!(cmdline_matches_install(engine_argv, vara2), InstallMatch::No);

        // Wine-rewritten windows form: instance only.
        assert_eq!(
            cmdline_matches_install(r"c:\vara hf\vara.exe", hf),
            InstallMatch::InstanceOnly
        );
        assert_eq!(
            cmdline_matches_install(r"c:\vara2\vara.exe", vara2),
            InstallMatch::InstanceOnly
        );
        // VARA vs VARA2 can never cross-match, in either slash form.
        assert_eq!(cmdline_matches_install(r"c:\vara2\vara.exe", vara), InstallMatch::No);
        assert_eq!(cmdline_matches_install("/x/vara2/vara.exe", vara), InstallMatch::No);
        assert_eq!(cmdline_matches_install(r"c:\vara\vara.exe", vara2), InstallMatch::No);
        // Lookalike dir needs the leading separator to be rejected.
        assert_eq!(cmdline_matches_install(r"c:\myvara\vara.exe", vara), InstallMatch::No);
    }

    /// Codex P1 #1: a live wine/VARA-ish process that is NOT attributable to
    /// this prefix+instance is left alone — process untouched AND pidfile
    /// kept (it may be the other install's live daemon).
    #[test]
    fn unattributable_vara_process_is_left_alone() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let prefix = tmp.path().to_path_buf();
        let install_dir = prefix.join("drive_c").join("VARA HF");
        fs::create_dir_all(&install_dir).unwrap();
        // argv0 claims to be a VARA2 exe (instance mismatch) — and even the
        // instance-dir form would fail the WINEPREFIX environ check.
        let mut child = std::process::Command::new("bash")
            .args(["-c", "exec -a '/elsewhere/drive_c/VARA2/VARA.exe' sleep 30"])
            .spawn()
            .expect("spawn masquerading child");
        fs::write(prefix.join(".vara.pid"), child.id().to_string()).unwrap();

        assert_eq!(stop_engine_daemon(&prefix, &install_dir), Ok(false));
        assert!(
            prefix.join(".vara.pid").exists(),
            "an unattributable wine/VARA pidfile must be left in place"
        );
        assert!(
            child.try_wait().expect("try_wait").is_none(),
            "the unattributable process must not be signalled"
        );
        child.kill().expect("cleanup kill");
        let _ = child.wait();
    }

    #[test]
    fn stale_pidfile_is_removed_not_killed() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let prefix = tmp.path().to_path_buf();
        let install_dir = prefix.join("drive_c").join("VARA HF");
        fs::create_dir_all(&install_dir).unwrap();
        // Malformed content.
        fs::write(prefix.join(".vara.pid"), "not-a-pid").unwrap();
        assert_eq!(stop_engine_daemon(&prefix, &install_dir), Ok(false));
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
        assert_eq!(stop_engine_daemon(&prefix, &install_dir), Ok(false));
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
        assert_eq!(report.cmd_port, Some(port));

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
        assert_eq!(report.cmd_port, Some(DEFAULT_CMD_PORT));
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

    /// Codex P2 #5: two backups of the same INI in the same second must land
    /// in distinct files — the second may never overwrite the first.
    #[test]
    fn backups_in_the_same_second_do_not_collide() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let ini = tmp.path().join("VARA.ini");
        fs::write(&ini, "original").unwrap();
        let first = create_backup(&ini).expect("first backup");
        let second = create_backup(&ini).expect("second backup");
        assert_ne!(first, second, "same-second backups must get distinct names");
        assert_eq!(fs::read_to_string(&first).unwrap(), "original");
        assert_eq!(fs::read_to_string(&second).unwrap(), "original");
    }

    /// Codex P2 #4: a second-instance apply with no known cmd port must
    /// refuse a relaunch BEFORE touching anything — no INI created, no
    /// launcher call, no assumed 8300.
    #[test]
    fn vara2_relaunch_without_a_known_port_refuses_pre_mutation() {
        let (_g, prefix, install) = fake_install("VARA2", None);
        let slot = VaraProcessSlot::default();
        let err = run_vara_ini_apply_with(
            &slot,
            None,
            &prefix,
            VaraInstance::Vara2,
            &[VaraIniEdit {
                section: "Soundcard".into(),
                key: "Output Device Name".into(),
                value: "USB Audio CODEC".into(),
            }],
            true,
            |_dir, _prefix| -> Result<ManagedModem, String> {
                panic!("launcher must not be called when the port is unknown")
            },
        )
        .expect_err("must refuse");
        assert!(err.contains("TCP Command Port"), "{err}");
        assert!(
            !install.join("VARA.ini").exists(),
            "refusal must precede any file mutation"
        );
    }

    /// Codex P2 #3: when the edits move the cmd port onto one an unrelated
    /// process already holds, the apply refuses BEFORE mutation — otherwise
    /// that listener would fake the relaunch verification.
    #[test]
    fn relaunch_refuses_when_target_port_is_already_held() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let held_port = listener.local_addr().unwrap().port();
        let old_port = free_port();
        let (_g, prefix, install) = fake_install("VARA HF", Some(&sample_ini(old_port)));
        let slot = VaraProcessSlot::default();
        let err = run_vara_ini_apply_with(
            &slot,
            None,
            &prefix,
            VaraInstance::Primary,
            &[VaraIniEdit {
                section: "Setup".into(),
                key: "TCP Command Port".into(),
                value: held_port.to_string(),
            }],
            true,
            |_dir, _prefix| -> Result<ManagedModem, String> {
                panic!("launcher must not be called when the target port is held")
            },
        )
        .expect_err("must refuse");
        assert!(err.contains("already in use"), "{err}");
        assert_eq!(
            fs::read_to_string(install.join("VARA.ini")).unwrap(),
            sample_ini(old_port),
            "refusal must leave the INI untouched"
        );
        drop(listener);
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
        assert_eq!(report.cmd_port, Some(port));

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
