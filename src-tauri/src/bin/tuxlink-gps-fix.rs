//! tuxlink-gps-fix — privileged GPS-fix helper (tuxlink-m9ej).
//!
//! Runs ONLY via `pkexec` through the `com.tuxlink.app.gps-fix` PolicyKit action
//! (installed at /usr/libexec/tuxlink-gps-fix). std-only, auditable, ~one screen.
//!
//! Security invariants (see docs/superpowers/plans/2026-06-13-gps-fix-it-pkexec.md):
//!
//! - Only the fixed actions add-dialout / mask-modemmanager / unmask-modemmanager run.
//! - Refuses to run without a valid numeric `$PKEXEC_UID` (must come through pkexec).
//! - add-dialout resolves the target user from `$PKEXEC_UID`, never argv/`$USER`.
//! - No shell, no interpolation: absolute binaries via Command + fixed argv, plus a
//!   `--` end-of-options guard and a username sanity check on the dialout add.
//! - The action token comes from a fixed match; operator text never reaches argv.
//!
//! Prints `ok` on success; `failed: <reason>` on stderr + nonzero exit otherwise.

use std::process::{Command, ExitCode};

fn fail(msg: &str) -> ExitCode {
    eprintln!("failed: {msg}");
    ExitCode::from(1)
}

/// First existing path from a fixed allowlist (handles usrmerge distros where
/// usermod is /usr/bin vs /usr/sbin). NO $PATH lookup — only these absolutes.
fn resolve_bin(candidates: &[&'static str]) -> Option<&'static str> {
    candidates.iter().copied().find(|p| std::path::Path::new(p).exists())
}

const USERMOD: [&str; 2] = ["/usr/sbin/usermod", "/usr/bin/usermod"];
const SYSTEMCTL: [&str; 2] = ["/usr/bin/systemctl", "/bin/systemctl"];
const APT_GET: [&str; 2] = ["/usr/bin/apt-get", "/bin/apt-get"];

const GPSD_DEFAULTS_PATH: &str = "/etc/default/gpsd";

/// A safe GPS device path: an absolute `/dev/...` node matching the serial
/// device shapes we detect, OR a `/dev/serial/by-id/...` stable symlink. No `..`,
/// no spaces/newlines/shell metacharacters — this string is written into
/// /etc/default/gpsd, so it must be inert.
fn is_safe_device_path(p: &str) -> bool {
    if p.is_empty() || p.len() > 256 || !p.starts_with("/dev/") || p.contains("..") {
        return false;
    }
    // Conservative charset for device nodes / by-id symlinks.
    if !p.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.' | ':')) {
        return false;
    }
    p.starts_with("/dev/ttyACM")
        || p.starts_with("/dev/ttyUSB")
        || p.starts_with("/dev/ttyAMA")
        || p.starts_with("/dev/ttyS")
        || p.starts_with("/dev/serial/by-id/")
        || p.starts_with("/dev/gps")
}

/// Render the /etc/default/gpsd contents. `device` is pre-validated (or None for
/// USB-hotplug-only). Plain key=value — this is exactly what dpkg-reconfigure
/// writes, so we skip its ncurses frontend entirely.
fn gpsd_defaults(device: Option<&str>) -> String {
    let devices = device.unwrap_or("");
    format!(
        "# Managed by Tuxlink (tuxlink-n399). Default settings for the gpsd init system.\n\
         START_DAEMON=\"true\"\n\
         USBAUTO=\"true\"\n\
         DEVICES=\"{devices}\"\n\
         GPSD_OPTIONS=\"-n\"\n"
    )
}

/// Write /etc/default/gpsd safely (adrev P1/P2):
///  - back up a pre-existing, non-Tuxlink-managed REGULAR file so a hand-tuned
///    config isn't silently destroyed;
///  - write a temp file in the same (root-owned) dir, then `rename` over the
///    target — atomic, and it REPLACES a symlink at the path rather than
///    following it to clobber an arbitrary file.
fn write_gpsd_defaults(device: Option<&str>) -> Result<(), String> {
    if let Ok(meta) = std::fs::symlink_metadata(GPSD_DEFAULTS_PATH) {
        // Only back up a real file we didn't author; a symlink is intentionally
        // not followed (the rename below replaces it with our regular file).
        if meta.file_type().is_file() {
            if let Ok(existing) = std::fs::read_to_string(GPSD_DEFAULTS_PATH) {
                if !existing.contains("# Managed by Tuxlink") {
                    let _ = std::fs::write(format!("{GPSD_DEFAULTS_PATH}.tuxlink.bak"), &existing);
                }
            }
        }
    }
    let tmp = format!("{GPSD_DEFAULTS_PATH}.tuxlink.tmp");
    std::fs::write(&tmp, gpsd_defaults(device))
        .map_err(|e| format!("could not write {tmp}: {e}"))?;
    std::fs::rename(&tmp, GPSD_DEFAULTS_PATH)
        .map_err(|e| format!("could not install {GPSD_DEFAULTS_PATH}: {e}"))?;
    Ok(())
}

/// Full gpsd setup in ONE privileged run (one pkexec prompt): apt-get update +
/// install, write /etc/default/gpsd, then enable + (re)start the socket/service.
/// Each step must succeed; the first failure aborts with a specific reason.
fn setup_gpsd(device: Option<&str>) -> ExitCode {
    if let Some(d) = device {
        if !is_safe_device_path(d) {
            return fail("device path failed the safety check");
        }
    }

    // 1. Install (Debian-family). Fixed package names — no operator input.
    let Some(apt) = resolve_bin(&APT_GET) else {
        return fail("apt-get not found (non-Debian system); use the shown commands");
    };
    // `apt-get update` first: a fresh Pi/Debian image often has a stale/pruned
    // index, so `install` would 404 with a misleading "check network" (adrev P0).
    // Best-effort — if the index can't refresh, install may still work from cache.
    let _ = Command::new(apt).arg("update").env("DEBIAN_FRONTEND", "noninteractive").status();
    let install = Command::new(apt)
        .arg("install")
        .arg("-y")
        .arg("gpsd")
        .arg("gpsd-clients")
        .env("DEBIAN_FRONTEND", "noninteractive")
        .status();
    match install {
        Ok(s) if s.success() => {}
        Ok(s) => return fail(&format!(
            "apt-get install failed ({s}) — check your network / package index"
        )),
        Err(e) => return fail(&format!("could not run apt-get: {e}")),
    }

    // 2. Configure the device (direct file write; no ncurses dpkg-reconfigure).
    if let Err(e) = write_gpsd_defaults(device) {
        return fail(&e);
    }

    // 3. Enable + (re)start so it reads the device now and on boot.
    let Some(systemctl) = resolve_bin(&SYSTEMCTL) else {
        return fail("systemctl not found");
    };
    let enable = Command::new(systemctl)
        .arg("enable")
        .arg("--now")
        .arg("gpsd.socket")
        .arg("gpsd.service")
        .status();
    match enable {
        Ok(s) if s.success() => {}
        Ok(s) => return fail(&format!("systemctl enable gpsd failed ({s})")),
        Err(e) => return fail(&format!("could not run systemctl: {e}")),
    }
    // Restart the SERVICE (not just the socket) so it re-reads the freshly written
    // DEVICES — on a re-run gpsd.service may already be active and `enable --now`
    // is then a no-op, so restarting only the socket would never load the new
    // config (adrev P1). Best-effort; gpsd -n opens the device on (re)start.
    let _ = Command::new(systemctl).arg("restart").arg("gpsd.socket").arg("gpsd.service").status();

    println!("ok");
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    // Invariant 2: must be invoked through pkexec (which sets PKEXEC_UID).
    let uid = match std::env::var("PKEXEC_UID") {
        Ok(u) if !u.is_empty() && u.chars().all(|c| c.is_ascii_digit()) => u,
        _ => return fail("PKEXEC_UID missing or invalid; run via pkexec"),
    };

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        return fail("an action argument is required");
    }

    // setup-gpsd is multi-step (install + configure + enable) and takes an
    // OPTIONAL device path; handle it separately from the single-command actions.
    if args[0] == "setup-gpsd" {
        if args.len() > 2 {
            return fail("setup-gpsd takes at most one device argument");
        }
        return setup_gpsd(args.get(1).map(String::as_str));
    }

    if args.len() != 1 {
        return fail("exactly one action argument required");
    }

    let status = match args[0].as_str() {
        "add-dialout" => {
            // Invariant 3: target user resolved from the numeric PKEXEC_UID only.
            let username = match uid_to_username(&uid) {
                Some(n) => n,
                None => return fail("could not resolve invoking user from PKEXEC_UID"),
            };
            // Defense-in-depth (adrev P2): reject anything that could be parsed as
            // a usermod option or is otherwise not a plausible username, even
            // though /etc/passwd is root-owned and the uid is the caller's own.
            if !is_safe_username(&username) {
                return fail("resolved username failed the safety check");
            }
            let bin = match resolve_bin(&USERMOD) {
                Some(b) => b,
                None => return fail("usermod not found"),
            };
            // `--` ends option parsing so a username can never be read as a flag.
            Command::new(bin).arg("-aG").arg("dialout").arg("--").arg(&username).status()
        }
        "mask-modemmanager" => {
            let bin = match resolve_bin(&SYSTEMCTL) {
                Some(b) => b,
                None => return fail("systemctl not found"),
            };
            Command::new(bin).arg("mask").arg("ModemManager").status()
        }
        "unmask-modemmanager" => {
            let bin = match resolve_bin(&SYSTEMCTL) {
                Some(b) => b,
                None => return fail("systemctl not found"),
            };
            Command::new(bin).arg("unmask").arg("ModemManager").status()
        }
        other => return fail(&format!("unknown action: {other}")),
    };

    match status {
        Ok(s) if s.success() => {
            println!("ok");
            ExitCode::SUCCESS
        }
        Ok(s) => fail(&format!("command exited with {s}")),
        Err(e) => fail(&format!("could not spawn command: {e}")),
    }
}

/// A plausible POSIX username that cannot be misread as a `usermod` option.
/// Non-empty, no leading `-`, and limited to `[A-Za-z0-9._-]`.
fn is_safe_username(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// Resolve a numeric uid string to a username by reading /etc/passwd (no libc
/// dependency). Lines are `name:passwd:uid:gid:...`. Returns None if not found.
fn uid_to_username(uid: &str) -> Option<String> {
    let passwd = std::fs::read_to_string("/etc/passwd").ok()?;
    for line in passwd.lines() {
        let mut f = line.split(':');
        let name = f.next()?;
        let _pw = f.next()?;
        let u = f.next()?;
        if u == uid {
            return Some(name.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{gpsd_defaults, is_safe_device_path, is_safe_username};

    #[test]
    fn accepts_plausible_usernames() {
        for n in ["alice", "w4phs", "user_1", "a.b-c", "svc-gps"] {
            assert!(is_safe_username(n), "{n} should be accepted");
        }
    }

    #[test]
    fn rejects_option_like_and_malformed_usernames() {
        for n in ["", "-G", "--badname", "a b", "name;reboot", "na/me", "x\n"] {
            assert!(!is_safe_username(n), "{n:?} should be rejected");
        }
    }

    #[test]
    fn accepts_real_gps_device_paths() {
        for d in [
            "/dev/ttyACM0",
            "/dev/ttyUSB0",
            "/dev/ttyAMA0",
            "/dev/ttyS0",
            "/dev/serial/by-id/usb-u-blox_AG_u-blox_GNSS_receiver-if00",
            "/dev/gps0",
        ] {
            assert!(is_safe_device_path(d), "{d} should be accepted");
        }
    }

    #[test]
    fn rejects_injection_and_off_tree_device_paths() {
        for d in [
            "",
            "/etc/passwd",
            "/dev/ttyACM0; reboot",
            "/dev/../etc/shadow",
            "/dev/ttyACM0\nDEVICES=evil",
            "ttyACM0",
            "/dev/ttyACM0 ",
            "/home/x",
        ] {
            assert!(!is_safe_device_path(d), "{d:?} should be rejected");
        }
    }

    #[test]
    fn gpsd_defaults_embeds_validated_device_and_is_inert() {
        let out = gpsd_defaults(Some("/dev/ttyACM0"));
        assert!(out.contains("DEVICES=\"/dev/ttyACM0\""));
        assert!(out.contains("USBAUTO=\"true\""));
        // No-device variant leaves DEVICES empty (USB hotplug only).
        assert!(gpsd_defaults(None).contains("DEVICES=\"\""));
    }
}
