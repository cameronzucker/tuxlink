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

fn main() -> ExitCode {
    // Invariant 2: must be invoked through pkexec (which sets PKEXEC_UID).
    let uid = match std::env::var("PKEXEC_UID") {
        Ok(u) if !u.is_empty() && u.chars().all(|c| c.is_ascii_digit()) => u,
        _ => return fail("PKEXEC_UID missing or invalid; run via pkexec"),
    };

    let args: Vec<String> = std::env::args().skip(1).collect();
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
    use super::is_safe_username;

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
}
