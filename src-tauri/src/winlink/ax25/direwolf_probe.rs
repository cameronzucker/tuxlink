//! Pre-spawn safety probes for the managed-Dire-Wolf packet path
//! (Slice B, Phase 3 of the managed-modem on-air accessibility design,
//! `docs/design/2026-06-12-managed-modem-onair-accessibility-design.md`).
//!
//! Three capabilities, all decidable WITHOUT a real Dire Wolf binary, a real
//! ALSA stack, or a radio, by reasoning over an injected command runner /
//! injected inputs:
//!
//! 1. [`direwolf_presence`] — is Dire Wolf installed, and at what version? Plus
//!    a pure [`meets_min_version`] comparator (the policy minimum is decided in
//!    a later phase; this module only provides the comparator + parsing).
//! 2. [`validate_conf`] — a pre-spawn conf gate over an injected runner. See the
//!    **"Dire Wolf has no parse-only mode"** note below: this is intentionally a
//!    thin gate, not the load-bearing safety.
//! 3. [`device_busy_from_status`] / [`probe_device_busy`] — is the chosen ALSA
//!    device already held by another program? Returns a NAMED error
//!    (`"<device> is in use by another program"`), never a black-box failure.
//!
//! ## Pure-over-injection / thin-impure-shell split
//!
//! Mirroring [`super::devices`] and the `parse_alsa_devices` pattern in
//! `ui_commands.rs`, all the decision logic is pure: [`meets_min_version`],
//! [`parse_version_banner`], [`classify_validate_output`], and
//! [`device_busy_from_status`] take plain inputs and are fixture-tested. The
//! impure parts are isolated behind the [`CommandRunner`] seam (Dire Wolf
//! invocations) and the thin [`probe_device_busy`] shim (reads
//! `/proc/asound/.../status`). Tests inject a [`SystemCommandRunner`] stand-in
//! and hand the parsers fixture text — they never spawn a process or read
//! `/proc`.
//!
//! ## Dire Wolf has NO parse-only / config-check / dry-run mode (grounded 1.7)
//!
//! Dire Wolf 1.7's CLI (confirmed via `direwolf -v` / the usage banner — it has
//! no `--help`, only the option list) exposes **no flag that parses a config and
//! exits**. The full option set relevant here:
//!
//! - `-c fname`  — configuration file name. Running this **starts the modem and
//!   opens the audio device.** It is NOT a validation-only invocation.
//! - `-t n`      — **TEXT COLORS** (`0` disables color). The Phase-plan draft of
//!   `direwolf -t 0 -c <conf>` was based on a misreading of `-t` as a "test"
//!   flag; it is not. `-t 0 -c <conf>` would still open the sound card.
//! - `-u` / `-S` — the only "print-and-exit" flags (UTF-8 test string / symbol
//!   tables). Neither validates a config.
//!
//! **Consequence:** there is no invocation that validates a conf without
//! grabbing the sound card. Running `direwolf -c <conf>` as a "gate" would seize
//! the audio device before the real spawn — exactly the conflict ADR 0015 makes
//! tuxlink the arbiter of. So [`validate_conf`] is **deliberately not wired to
//! run Dire Wolf in production** (its real-binary path is left to the caller's
//! discretion and is documented as unsafe-as-a-gate). The load-bearing
//! pre-spawn safety is instead:
//!
//! - **Construction-correctness:** tuxlink GENERATES `direwolf.conf` from a fixed
//!   template (Phase 2, [`super::direwolf_conf`]) over a resolved device + PTT +
//!   port. A malformed conf is near-impossible by construction, so a parse gate
//!   buys little.
//! - **The device-busy probe** ([`probe_device_busy`]) — the genuine pre-spawn
//!   check that the sound card is free.
//!
//! [`validate_conf`] is retained as a pure classifier over an INJECTED runner so
//! that IF a future Dire Wolf gains a real `--check`-style flag, the wiring point
//! and its success/failure semantics already exist and are tested. The injected
//! seam means the unit test exercises the classify logic regardless of whether a
//! safe real invocation exists today.

use std::path::Path;
use std::process::Output;

// ============================================================================
// CommandRunner seam — the injection boundary for Dire Wolf invocations
// ============================================================================

/// Minimal injected command runner so the probes are testable without a real
/// `direwolf` binary. Production uses [`SystemCommandRunner`]; tests use a fake
/// that returns canned [`Output`]s. Intentionally tiny — one method.
pub trait CommandRunner {
    /// Run `program` with `args`, capturing exit status + stdout + stderr.
    ///
    /// Returns `Err` (typically [`std::io::ErrorKind::NotFound`]) when the
    /// program cannot be spawned at all — e.g. the binary is absent. The probes
    /// map that `Err` to a benign "absent" outcome rather than panicking,
    /// matching the `ardop_list_audio_devices` soft-failure posture.
    fn run(&self, program: &str, args: &[&str]) -> std::io::Result<Output>;
}

/// Production [`CommandRunner`]: shells out via [`std::process::Command`].
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> std::io::Result<Output> {
        std::process::Command::new(program).args(args).output()
    }
}

// ============================================================================
// Task 3.1 — presence + version
// ============================================================================

/// Whether Dire Wolf is installed, and (when present) its parsed `X.Y` version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DwPresence {
    /// The `direwolf` binary could not be spawned (not installed / not on PATH).
    Absent,
    /// Dire Wolf is present; `version` is the parsed `"X.Y"` string from its
    /// startup banner (e.g. `"1.7"`).
    Present {
        /// Parsed `"major.minor"` version string from the banner.
        version: String,
    },
}

/// Probe Dire Wolf presence + version via the injected runner.
///
/// Mechanism: Dire Wolf prints a `Dire Wolf version X.Y` banner to **stdout** on
/// startup (confirmed on 1.7 — even an unrecognized option like `-v` still emits
/// the banner before the usage text). We invoke `direwolf -v`: Dire Wolf treats
/// `-v` as an unknown option and prints the banner, which is all we need. If the
/// binary is absent, [`CommandRunner::run`] returns `Err` → [`DwPresence::Absent`].
///
/// The version is parsed leniently from the banner ([`parse_version_banner`]) so
/// surrounding "Includes optional support for: ..." noise and the duplicated
/// banner line do not break extraction. If the binary runs but no `X.Y` can be
/// found anywhere in stdout+stderr, we conservatively report [`DwPresence::Absent`]
/// (we cannot confirm a usable Dire Wolf).
pub fn direwolf_presence(exec: &impl CommandRunner) -> DwPresence {
    match exec.run("direwolf", &["-v"]) {
        Err(_) => DwPresence::Absent,
        Ok(output) => {
            // The banner lands on stdout on 1.7; scan both streams defensively
            // in case a future build routes it differently.
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            match parse_version_banner(&stdout).or_else(|| parse_version_banner(&stderr)) {
                Some(version) => DwPresence::Present { version },
                None => DwPresence::Absent,
            }
        }
    }
}

/// Extract a `"major.minor"` version from a Dire Wolf banner, tolerating noise.
///
/// The real 1.7 banner is, verbatim:
/// ```text
/// Dire Wolf version 1.7
/// Includes optional support for:  gpsd hamlib cm108-ptt
///
/// Dire Wolf version 1.7
/// ```
/// We look for the literal token `version` and parse the following whitespace-
/// separated word as `X.Y` (ignoring any trailing patch/qualifier). Falls back
/// to scanning for the first bare `X.Y` token if the `version` keyword is
/// absent, so a reformatted banner still yields something. Returns `None` if no
/// `digits.digits` pattern is present at all.
fn parse_version_banner(banner: &str) -> Option<String> {
    // Preferred: the word immediately after a "version" token.
    let lower = banner.to_ascii_lowercase();
    if let Some(idx) = lower.find("version") {
        let after = &banner[idx + "version".len()..];
        if let Some(v) = first_major_minor(after) {
            return Some(v);
        }
    }
    // Fallback: the first X.Y token anywhere in the banner.
    first_major_minor(banner)
}

/// Find the first `digits.digits` token in `text`, returning just the `"X.Y"`
/// (major.minor) portion — any trailing `.patch` or qualifier is dropped.
fn first_major_minor(text: &str) -> Option<String> {
    // Split on anything that is not a digit or a dot, then look for the first
    // token shaped like `X.Y[...]` and keep major.minor.
    for token in text.split(|c: char| !(c.is_ascii_digit() || c == '.')) {
        if token.is_empty() {
            continue;
        }
        let mut parts = token.split('.');
        let major = parts.next().filter(|s| !s.is_empty());
        let minor = parts.next().filter(|s| !s.is_empty());
        if let (Some(major), Some(minor)) = (major, minor) {
            if major.chars().all(|c| c.is_ascii_digit())
                && minor.chars().all(|c| c.is_ascii_digit())
            {
                return Some(format!("{major}.{minor}"));
            }
        }
    }
    None
}

/// Pure comparator: does the parsed `found` version (`"X.Y"`) meet a minimum of
/// `min_major.min_minor`?
///
/// **No policy minimum is hardcoded here** — the actual managed-Dire-Wolf
/// minimum is decided in a later phase. This is just the comparator + parser the
/// policy layer will call. An unparseable `found` returns `false` (conservative:
/// a version we cannot read does not meet any minimum).
pub fn meets_min_version(found: &str, min_major: u32, min_minor: u32) -> bool {
    let Some((major, minor)) = parse_major_minor(found) else {
        return false;
    };
    (major, minor) >= (min_major, min_minor)
}

/// Parse a bare `"X.Y"` (or `"X.Y.Z"`, keeping major.minor) into `(major, minor)`.
fn parse_major_minor(v: &str) -> Option<(u32, u32)> {
    let v = v.trim();
    let mut parts = v.split('.');
    let major = parts.next()?.parse::<u32>().ok()?;
    let minor = parts.next()?.parse::<u32>().ok()?;
    Some((major, minor))
}

// ============================================================================
// Task 3.2 — conf validation gate (over an injected runner)
// ============================================================================

/// A conf-validation failure carrying the surfaced Dire Wolf diagnostic — never
/// a black box. The `message` includes the binary's stderr so the operator sees
/// WHY the conf was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct ConfError {
    /// Human-facing message surfacing the underlying diagnostic.
    pub message: String,
}

/// Pre-spawn conf gate over an INJECTED runner.
///
/// **Read the module-level "Dire Wolf has NO parse-only mode" note first.** Dire
/// Wolf 1.7 has no invocation that validates a conf without opening the audio
/// device, so running the real binary here as a production gate is UNSAFE (it
/// would seize the sound card before the real spawn). The load-bearing pre-spawn
/// safety is construction-correctness (the conf is template-generated) plus
/// [`probe_device_busy`]. This function is retained as the typed wiring point +
/// failure classifier for a hypothetical future `direwolf --check` flag, and to
/// keep the success/failure contract tested.
///
/// Contract over the injected runner:
/// - runner `Err` (binary absent) → `Ok(())`: we cannot validate without a
///   binary, and a missing binary is surfaced by [`direwolf_presence`], not
///   here. Treating "no binary" as a conf failure would be a misleading error.
/// - runner returns success status → `Ok(())`.
/// - runner returns non-zero status → `Err(ConfError)` whose message surfaces the
///   captured stderr (or a fallback if stderr is empty).
///
/// The actual program/args used for the injected call are intentionally a single
/// place ([`VALIDATE_PROGRAM`] / [`validate_args`]) so a future real `--check`
/// flag only changes one line.
pub fn validate_conf(conf_path: &Path, exec: &impl CommandRunner) -> Result<(), ConfError> {
    let conf = conf_path.to_string_lossy();
    let args = validate_args(&conf);
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    match exec.run(VALIDATE_PROGRAM, &arg_refs) {
        // Binary absent: not a conf error — presence is reported elsewhere.
        Err(_) => Ok(()),
        Ok(output) => classify_validate_output(&conf, &output),
    }
}

/// The program a real conf-check would invoke. Single source of truth so a
/// future `--check`-style flag is a one-line change.
const VALIDATE_PROGRAM: &str = "direwolf";

/// Args for the conf-check invocation. NOTE: as documented at module level, Dire
/// Wolf 1.7 has no safe parse-only flag, so this invocation is NOT run as a
/// production gate today; it exists so the injected classifier has a defined
/// shape. `-c <conf>` is the only conf-bearing form Dire Wolf accepts.
fn validate_args(conf: &str) -> Vec<String> {
    vec!["-c".to_string(), conf.to_string()]
}

/// Pure classifier for a conf-check [`Output`]: success status → `Ok`, non-zero
/// → `Err(ConfError)` surfacing stderr (with a non-empty-message guarantee).
fn classify_validate_output(conf: &str, output: &Output) -> Result<(), ConfError> {
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = stderr.trim();
    let message = if detail.is_empty() {
        // Non-zero exit with nothing on stderr — still surface a usable message.
        let code = output
            .status
            .code()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "signal".to_string());
        format!("direwolf rejected config {conf} (exit {code})")
    } else {
        format!("direwolf rejected config {conf}: {detail}")
    };
    Err(ConfError { message })
}

// ============================================================================
// Task 3.3 — ALSA device-busy probe
// ============================================================================

/// Pure decision: does an ALSA PCM `sub` `status` file's text indicate the
/// device is currently in use (held by another program)?
///
/// The kernel writes one of two shapes to
/// `/proc/asound/card<N>/pcm<M>{c,p}/sub<K>/status`:
///
/// - A free substream: the single token `closed`.
/// - A held substream: a multi-line block beginning with `state: RUNNING`
///   (or `state: PREPARED` / `state: XRUN` — any non-closed state) plus
///   `owner_pid : <pid>` and stream parameters.
///
/// Rule: the device is busy IFF the status is NOT the `closed` sentinel. We
/// treat anything that is not exactly `closed` (trimmed) as held — this is the
/// conservative reading (an unexpected non-closed state means "something is
/// going on with this sub," which is exactly when we must not seize it).
pub fn device_busy_from_status(status_text: &str) -> bool {
    status_text.trim() != "closed"
}

/// Format the NAMED device-busy error. The device name is always present in the
/// message so the operator never sees a black-box failure.
pub fn device_busy_message(device: &str) -> String {
    format!("{device} is in use by another program")
}

/// IMPURE SHIM — read the ALSA PCM substream `status` files for the card backing
/// `plughw_name` and decide busy/free, returning the NAMED error string when
/// busy. Thin and untested by design (mirrors [`super::devices::read_sys_snapshot`]
/// and the `arecord -L` shim in `ui_commands.rs`).
///
/// Returns:
/// - `Ok(())` — no substream is held (every `status` reads `closed`, or none
///   were found, which on this soft-failure posture is "not provably busy").
/// - `Err(message)` — at least one substream is held; `message` is
///   [`device_busy_message`] naming `plughw_name`.
///
/// Soft-failure posture: a path that cannot be read is skipped (best-effort),
/// matching `confirm_audio_device_released`'s "lsof absent → assume released"
/// idiom — we do not fail-closed on a read error, because the genuine arbiter of
/// the one-card conflict is tuxlink's own managed lifecycle (ADR 0015), and a
/// transient `/proc` read hiccup should not block a spawn.
///
/// `card_index` maps the stable `plughw:CARD=<id>` name to the live `card<N>`
/// index — resolving that mapping is the caller's job (it has the snapshot); this
/// shim takes the already-resolved index so it stays a pure-ish file reader.
pub fn probe_device_busy(plughw_name: &str, card_index: u32) -> Result<(), String> {
    use std::fs;

    let glob_root = format!("/proc/asound/card{card_index}");
    // Walk pcm*/sub*/status under the card. We avoid a glob crate: enumerate
    // pcm dirs, then sub dirs, reading each status file. Any read error on an
    // individual entry is skipped (best-effort).
    let Ok(pcm_dirs) = fs::read_dir(&glob_root) else {
        // Card dir not present / unreadable — not provably busy.
        return Ok(());
    };
    for pcm in pcm_dirs.flatten() {
        let pcm_path = pcm.path();
        let name = pcm.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("pcm") {
            continue;
        }
        let Ok(sub_dirs) = fs::read_dir(&pcm_path) else {
            continue;
        };
        for sub in sub_dirs.flatten() {
            let sub_name = sub.file_name();
            let sub_name = sub_name.to_string_lossy();
            if !sub_name.starts_with("sub") {
                continue;
            }
            let status_path = sub.path().join("status");
            if let Ok(text) = fs::read_to_string(&status_path) {
                if device_busy_from_status(&text) {
                    return Err(device_busy_message(plughw_name));
                }
            }
        }
    }
    Ok(())
}

// ============================================================================
// Tests — pure over fixtures + a fake runner. No real direwolf, ALSA, or radio.
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;
    use std::process::{ExitStatus, Output};

    // ---- fake runner --------------------------------------------------------

    /// A [`CommandRunner`] that returns a single canned outcome — either an
    /// `Err` (simulating an absent binary) or a canned [`Output`].
    struct FakeRunner {
        result: std::io::Result<Output>,
    }

    impl FakeRunner {
        /// Simulate the binary being absent: `run` returns `NotFound`.
        fn absent() -> Self {
            FakeRunner {
                result: Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "no such file: direwolf",
                )),
            }
        }

        /// Simulate a successful run with the given stdout/stderr and exit code.
        fn ok(code: i32, stdout: &str, stderr: &str) -> Self {
            FakeRunner {
                result: Ok(Output {
                    status: ExitStatus::from_raw(code),
                    stdout: stdout.as_bytes().to_vec(),
                    stderr: stderr.as_bytes().to_vec(),
                }),
            }
        }
    }

    impl CommandRunner for FakeRunner {
        fn run(&self, _program: &str, _args: &[&str]) -> std::io::Result<Output> {
            // Re-create the canned result on each call (io::Error / Output are not
            // Clone, so we rebuild from the stored shape).
            match &self.result {
                Err(e) => Err(std::io::Error::new(e.kind(), e.to_string())),
                Ok(o) => Ok(Output {
                    status: o.status,
                    stdout: o.stdout.clone(),
                    stderr: o.stderr.clone(),
                }),
            }
        }
    }

    /// `ExitStatus::from_raw` takes a raw wait-status, not an exit code: on Unix
    /// a normal exit with code N is encoded as `N << 8`. This helper builds a
    /// success (0) or a non-zero exit status the way the kernel would.
    fn exit_status(code: i32) -> ExitStatus {
        ExitStatus::from_raw(code << 8)
    }

    // ---- Task 3.1: presence + version --------------------------------------

    /// Absent binary (runner Err NotFound) → DwPresence::Absent.
    #[test]
    fn presence_absent_when_binary_missing() {
        let exec = FakeRunner::absent();
        assert_eq!(direwolf_presence(&exec), DwPresence::Absent);
    }

    /// Present + parseable banner → Present{version}, parsed from the REAL 1.7
    /// banner shape (duplicated line + "Includes optional support" noise).
    #[test]
    fn presence_present_parses_real_1_7_banner() {
        let banner = "Dire Wolf version 1.7\n\
                      Includes optional support for:  gpsd hamlib cm108-ptt\n\
                      \n\
                      Dire Wolf version 1.7\n";
        let exec = FakeRunner::ok(0, banner, "");
        assert_eq!(
            direwolf_presence(&exec),
            DwPresence::Present {
                version: "1.7".to_string()
            }
        );
    }

    /// The banner can also arrive on stderr (defensive): still Present.
    #[test]
    fn presence_reads_banner_from_stderr_fallback() {
        let exec = FakeRunner::ok(0, "", "Dire Wolf version 1.6\n");
        assert_eq!(
            direwolf_presence(&exec),
            DwPresence::Present {
                version: "1.6".to_string()
            }
        );
    }

    /// Binary runs but emits no parseable version → conservatively Absent.
    #[test]
    fn presence_absent_when_no_version_token() {
        let exec = FakeRunner::ok(0, "some unrelated output\n", "");
        assert_eq!(direwolf_presence(&exec), DwPresence::Absent);
    }

    /// meets_min_version comparator: old version fails, ok version passes.
    #[test]
    fn meets_min_version_old_fails_new_passes() {
        // present-old: "1.4" does not meet a 1.6 minimum.
        assert!(!meets_min_version("1.4", 1, 6));
        // present-ok: "1.7" meets a 1.6 minimum.
        assert!(meets_min_version("1.7", 1, 6));
        // exact match meets.
        assert!(meets_min_version("1.6", 1, 6));
    }

    /// meets_min_version handles major-version bumps and unparseable input.
    #[test]
    fn meets_min_version_major_bump_and_garbage() {
        assert!(meets_min_version("2.0", 1, 9)); // 2.0 >= 1.9
        assert!(!meets_min_version("1.10", 1, 11)); // 1.10 < 1.11 (numeric, not lexical)
        assert!(meets_min_version("1.11", 1, 10)); // 1.11 >= 1.10
        assert!(!meets_min_version("garbage", 1, 0)); // unparseable → false
        assert!(!meets_min_version("", 0, 0)); // empty → false
    }

    /// The end-to-end present-old / present-ok scenario the plan calls out:
    /// "1.4" → Present but meets_min(1,6) == false; "1.7" → meets_min(1,6) == true.
    #[test]
    fn presence_then_min_version_gate() {
        let old = direwolf_presence(&FakeRunner::ok(0, "Dire Wolf version 1.4\n", ""));
        match old {
            DwPresence::Present { version } => {
                assert_eq!(version, "1.4");
                assert!(!meets_min_version(&version, 1, 6));
            }
            DwPresence::Absent => panic!("1.4 should be Present"),
        }

        let ok = direwolf_presence(&FakeRunner::ok(0, "Dire Wolf version 1.7\n", ""));
        match ok {
            DwPresence::Present { version } => {
                assert_eq!(version, "1.7");
                assert!(meets_min_version(&version, 1, 6));
            }
            DwPresence::Absent => panic!("1.7 should be Present"),
        }
    }

    // ---- Task 3.2: conf validation classifier ------------------------------

    /// Injected success Output → Ok(()).
    #[test]
    fn validate_conf_ok_on_success() {
        let exec = FakeRunner {
            result: Ok(Output {
                status: exit_status(0),
                stdout: b"ok".to_vec(),
                stderr: Vec::new(),
            }),
        };
        let res = validate_conf(Path::new("/tmp/direwolf.conf"), &exec);
        assert!(res.is_ok(), "success status must validate: {res:?}");
    }

    /// Injected failure Output (non-zero + stderr line) → Err(ConfError) whose
    /// message SURFACES the direwolf stderr (not a black box) AND names the conf.
    #[test]
    fn validate_conf_err_surfaces_stderr() {
        let exec = FakeRunner {
            result: Ok(Output {
                status: exit_status(1),
                stdout: Vec::new(),
                stderr: b"Config file line 12: Invalid PTT device.\n".to_vec(),
            }),
        };
        let err = validate_conf(Path::new("/etc/direwolf.conf"), &exec)
            .expect_err("non-zero status must be a ConfError");
        // The surfaced message includes the direwolf diagnostic verbatim...
        assert!(
            err.message.contains("Invalid PTT device"),
            "message must surface direwolf stderr, got: {}",
            err.message
        );
        // ...and names the offending conf path.
        assert!(
            err.message.contains("/etc/direwolf.conf"),
            "message must name the conf, got: {}",
            err.message
        );
    }

    /// Non-zero exit with EMPTY stderr still yields a usable (non-empty) message.
    #[test]
    fn validate_conf_err_nonempty_even_without_stderr() {
        let exec = FakeRunner {
            result: Ok(Output {
                status: exit_status(2),
                stdout: Vec::new(),
                stderr: Vec::new(),
            }),
        };
        let err = validate_conf(Path::new("/tmp/x.conf"), &exec)
            .expect_err("non-zero status must be a ConfError");
        assert!(!err.message.is_empty());
        assert!(err.message.contains("/tmp/x.conf"));
        assert!(err.message.contains("exit 2"));
    }

    /// Absent binary → Ok(()): a missing binary is NOT a conf error (presence is
    /// reported by direwolf_presence, not here).
    #[test]
    fn validate_conf_absent_binary_is_not_a_conf_error() {
        let exec = FakeRunner::absent();
        assert!(validate_conf(Path::new("/tmp/direwolf.conf"), &exec).is_ok());
    }

    // ---- Task 3.3: device-busy decision + named error ----------------------

    /// A free substream (`closed`) → not busy.
    #[test]
    fn device_not_busy_when_closed() {
        assert!(!device_busy_from_status("closed"));
        assert!(!device_busy_from_status("closed\n"));
        assert!(!device_busy_from_status("  closed  \n"));
    }

    /// A held substream (`state: RUNNING` + owner) → busy.
    #[test]
    fn device_busy_when_running_and_owned() {
        // The real kernel format for a held PLAYBACK substream.
        let held = "owner_pid   : 2412\n\
                    state       : RUNNING\n\
                    trigger_time: 1234.567890\n\
                    tstamp      : 0.000000\n\
                    delay       : 1024\n\
                    avail       : 0\n\
                    avail_max   : 2048\n\
                    -----\n\
                    hw_ptr      : 48000\n\
                    appl_ptr    : 49024\n";
        assert!(device_busy_from_status(held));
    }

    /// Any non-closed state (e.g. PREPARED) is treated as busy (conservative).
    #[test]
    fn device_busy_when_prepared() {
        let prepared = "owner_pid   : 999\nstate       : PREPARED\n";
        assert!(device_busy_from_status(prepared));
    }

    /// The named error string contains the device name — never a black box.
    #[test]
    fn busy_message_names_the_device() {
        let msg = device_busy_message("plughw:CARD=DRA,DEV=0");
        assert!(msg.contains("plughw:CARD=DRA,DEV=0"));
        assert!(msg.contains("in use by another program"));
    }
}
