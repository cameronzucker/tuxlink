# GPS "Fix it for me" — pkexec helper Implementation Plan (tuxlink-m9ej)

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or executing-plans. Steps use `- [ ]`.
> **Security-sensitive:** this ships a privileged helper run via `pkexec` as root. It MUST go through at least one Codex adversarial round before merge (build-robust-features discipline; the diff touches a root-executed binary). Findings + dispositions go in the PR body.

**Goal:** Activate the "Fix it for me" buttons (shipped disabled) so an operator can fix the two common Linux GPS blockers — `dialout` membership and a running ModemManager — with one click, authorized by a system PolicyKit dialog.

**Architecture:** A tiny std-only Rust helper binary `tuxlink-gps-fix` with a fixed action enum, installed at `/usr/libexec/`; a PolicyKit policy authorizing only that path (`auth_admin`); a Tauri command that invokes it via `pkexec` and maps exit codes; frontend wiring that enables the buttons, runs the fix, re-scans, and shows the dialout re-login notice. AppImage (no system polkit registration) degrades to "Show command" only.

**Tech Stack:** Rust (std only for the helper), `pkexec`/PolicyKit, Tauri command + events, `.deb`/`.rpm` bundle `files` overlay, React/Vitest.

**Stacks on:** `tuxlink-yy1m` (PR #678). The diagnostics + disabled buttons already exist (`GpsSourcePicker`).

**Canonical spec:** `bd show tuxlink-m9ej` + the parent design's bd-2 section ([2026-06-05-gps-setup-ux-design.md](../../design/2026-06-05-gps-setup-ux-design.md)). This plan operationalizes it; it does not restate the rationale.

---

## Security invariants (must hold; the Codex round attacks these)

1. The helper accepts ONLY the fixed actions `add-dialout` | `mask-modemmanager` | `unmask-modemmanager`. Any other argv → exit non-zero, no side effect.
2. The helper refuses to run without `$PKEXEC_UID` set (it must be invoked through pkexec, never directly as root by accident).
3. `add-dialout` resolves the target user from `$PKEXEC_UID` (the invoking user), NEVER from argv or `$USER` — so it cannot be steered to add an attacker-chosen account.
4. No shell, no string interpolation into a shell, no arbitrary path execution. The helper calls `usermod`/`systemctl` via absolute paths with fixed arg vectors (`Command::new` + `.arg`, never `sh -c`).
5. The PolicyKit policy authorizes exactly one exec path (`/usr/libexec/tuxlink-gps-fix`) with `auth_admin`; no wildcards.
6. The Tauri spawner passes the action as a single fixed argv token (from a Rust enum), never operator-supplied text.

## File Structure

- Create: `src-tauri/src/bin/tuxlink-gps-fix.rs` — the helper (std only; no tauri/serde).
- Create: `src-tauri/packaging/com.tuxlink.app.policy` — PolicyKit policy (static XML).
- Modify: `src-tauri/tauri.conf.json` — `linux.deb.files` (+ rpm) ships the helper binary and the policy.
- Create: `src-tauri/src/gps_fix.rs` — the Tauri command `gps_run_fix(action)` + exit-code mapping + `GpsFixOutcome` enum; registered in `lib.rs`.
- Modify: `src-tauri/src/lib.rs` — register `gps_run_fix`.
- Modify: `src/location/gpsProbes.ts` — binding `runGpsFix(action)` + `GpsFixOutcome` type + `pkexecAvailable()` probe.
- Modify: `src/location/GpsSourcePicker.tsx` — enable the fix buttons when fixable + pkexec available; run → outcome handling → rescan; dialout re-login notice. Add an `unmask-modemmanager` affordance.
- Modify: `src/location/GpsSourcePicker.test.tsx` — fix-button behavior + degradation.

---

## Task P1: Helper binary `tuxlink-gps-fix`

**Files:** Create `src-tauri/src/bin/tuxlink-gps-fix.rs`. (Cargo auto-discovers `src/bin/*.rs`, matching `native_cms_probe.rs` / `vara_tcp_probe.rs`.)

- [ ] **Step 1: Write the helper** (std only; the security invariants above are the spec)

```rust
//! tuxlink-gps-fix — privileged GPS-fix helper (tuxlink-m9ej). Run ONLY via
//! pkexec through the com.tuxlink.app.gps-fix PolicyKit action. Fixed action
//! enum; no shell; no arbitrary exec; target user resolved from $PKEXEC_UID.
//! Prints `ok` on success or `failed: <reason>` on stderr + nonzero exit.
use std::process::{Command, ExitCode};

fn fail(msg: &str) -> ExitCode {
    eprintln!("failed: {msg}");
    ExitCode::from(1)
}

fn main() -> ExitCode {
    // Invariant 2: must be invoked through pkexec.
    let uid = match std::env::var("PKEXEC_UID") {
        Ok(u) if u.chars().all(|c| c.is_ascii_digit()) && !u.is_empty() => u,
        _ => return fail("PKEXEC_UID missing or invalid; run via pkexec"),
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 1 {
        return fail("exactly one action argument required");
    }
    // Invariant 3: resolve the username from the numeric PKEXEC_UID, not argv.
    let username = match uid_to_username(&uid) {
        Some(n) => n,
        None => return fail("could not resolve invoking user from PKEXEC_UID"),
    };
    // Invariant 1 + 4: fixed actions, absolute binaries, fixed argv, no shell.
    let status = match args[0].as_str() {
        "add-dialout" => Command::new("/usr/sbin/usermod").arg("-aG").arg("dialout").arg(&username).status(),
        "mask-modemmanager" => Command::new("/usr/bin/systemctl").arg("mask").arg("ModemManager").status(),
        "unmask-modemmanager" => Command::new("/usr/bin/systemctl").arg("unmask").arg("ModemManager").status(),
        other => return fail(&format!("unknown action: {other}")),
    };
    match status {
        Ok(s) if s.success() => { println!("ok"); ExitCode::SUCCESS }
        Ok(s) => fail(&format!("command exited with {s}")),
        Err(e) => fail(&format!("could not spawn command: {e}")),
    }
}

/// Resolve a numeric uid string to a username by reading /etc/passwd (no libc
/// dep). Returns None if not found. Format: name:passwd:uid:gid:...
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
```

> Note: `/usr/sbin/usermod` is the Debian/Ubuntu path; some distros use `/usr/bin/usermod` (usrmerge). The Codex round should flag this — mitigation: probe both paths (try `/usr/sbin/usermod` then `/usr/bin/usermod`) rather than hard-coding one. Apply that hardening when implementing (search a small fixed allowlist of absolute paths, first existing wins; still no PATH lookup).

- [ ] **Step 2: Can't cold-compile locally — CI gates it.** Confirm it builds in CI (the helper is a new bin target). Locally, only `cargo check` if a warm target exists; otherwise rely on CI.

- [ ] **Step 3: Commit** (`feat(gps): privileged tuxlink-gps-fix helper binary (tuxlink-m9ej)` + trailers).

## Task P2: PolicyKit policy

**Files:** Create `src-tauri/packaging/com.tuxlink.app.policy`.

- [ ] **Step 1: Write the policy** (verbatim from m9ej spec)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE policyconfig PUBLIC "-//freedesktop//DTD PolicyKit Policy Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/PolicyKit/1.0/policyconfig.dtd">
<policyconfig>
  <action id="com.tuxlink.app.gps-fix">
    <description>Adjust GPS-related system configuration</description>
    <message>Tuxlink would like to fix a GPS configuration issue. This requires your password. You can reverse the change from Settings → Location → Troubleshoot.</message>
    <icon_name>com.tuxlink.app</icon_name>
    <defaults>
      <allow_any>auth_admin</allow_any>
      <allow_inactive>auth_admin</allow_inactive>
      <allow_active>auth_admin</allow_active>
    </defaults>
    <annotate key="org.freedesktop.policykit.exec.path">/usr/libexec/tuxlink-gps-fix</annotate>
  </action>
</policyconfig>
```

- [ ] **Step 2: Validate syntax** where polkit is present: `pkaction --action-id com.tuxlink.app.gps-fix --verbose` (operator/CI-with-polkit). Locally, validate it is well-formed XML: `xmllint --noout src-tauri/packaging/com.tuxlink.app.policy`.

- [ ] **Step 3: Commit.**

## Task P3: Bundle wiring (.deb/.rpm ship binary + policy; AppImage degrades)

**Files:** Modify `src-tauri/tauri.conf.json` `linux.deb.files` (and `rpm.files` if present).

- [ ] **Step 1:** Add to `linux.deb.files` (paths: install-target → source):

```json
"/usr/libexec/tuxlink-gps-fix": "target/release/tuxlink-gps-fix",
"/usr/share/polkit-1/actions/com.tuxlink.app.policy": "packaging/com.tuxlink.app.policy"
```

> Source paths in `deb.files` are resolved relative to `src-tauri/`. The helper binary is a cargo build artifact present at bundle time (cargo builds all `src/bin/*` for the release profile). Confirm the artifact name is `tuxlink-gps-fix` (matches the file stem). The Codex round should check whether tauri's bundler builds extra bins before `deb.files` copy — if not, the helper must be listed as an explicit bin to build, or copied via a `beforeBundleCommand`.

- [ ] **Step 2:** Mirror under `rpm.files` if the rpm target ships.

- [ ] **Step 3: Commit.** (Packaged-install behavior is operator-verified on a real `.deb` — note in the PR per Verification-provenance.)

## Task P4: Tauri command `gps_run_fix` + pkexec availability

**Files:** Create `src-tauri/src/gps_fix.rs`; register in `lib.rs`.

- [ ] **Step 1: Write the failing test** (in `gps_fix.rs`) — exit-code → outcome mapping is the pure, testable core:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maps_exit_codes_to_outcomes() {
        assert_eq!(classify_exit(Some(0)), GpsFixOutcome::Ok);
        assert_eq!(classify_exit(Some(126)), GpsFixOutcome::AuthDismissed);
        assert_eq!(classify_exit(Some(127)), GpsFixOutcome::PkexecMissing);
        assert_eq!(classify_exit(Some(1)), GpsFixOutcome::Failed);
        assert_eq!(classify_exit(None), GpsFixOutcome::Failed);
    }
}
```

- [ ] **Step 2: Implement** the command + pure `classify_exit` + a fixed action allowlist (mirrors the helper enum so operator text never reaches argv):

```rust
use serde::Serialize;

#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum GpsFixOutcome { Ok, AuthDismissed, PkexecMissing, Failed }

pub fn classify_exit(code: Option<i32>) -> GpsFixOutcome {
    match code {
        Some(0) => GpsFixOutcome::Ok,
        Some(126) => GpsFixOutcome::AuthDismissed,
        Some(127) => GpsFixOutcome::PkexecMissing,
        _ => GpsFixOutcome::Failed,
    }
}

fn action_token(action: &str) -> Option<&'static str> {
    match action {
        "add-dialout" => Some("add-dialout"),
        "mask-modemmanager" => Some("mask-modemmanager"),
        "unmask-modemmanager" => Some("unmask-modemmanager"),
        _ => None, // operator text can never pass through
    }
}

#[tauri::command]
pub async fn gps_run_fix(action: String) -> Result<GpsFixOutcome, crate::ui_commands::UiError> {
    let token = action_token(&action)
        .ok_or_else(|| crate::ui_commands::UiError::Internal { detail: "unknown gps fix action".into() })?;
    let status = std::process::Command::new("pkexec")
        .arg("/usr/libexec/tuxlink-gps-fix")
        .arg(token)
        .status();
    match status {
        Ok(s) => Ok(classify_exit(s.code())),
        Err(_) => Ok(GpsFixOutcome::PkexecMissing), // pkexec not installed
    }
}

/// Whether pkexec exists at all (drives button visibility — AppImage / minimal
/// installs hide "Fix it for me").
#[tauri::command]
pub async fn gps_pkexec_available() -> Result<bool, crate::ui_commands::UiError> {
    Ok(which_pkexec())
}
fn which_pkexec() -> bool {
    ["/usr/bin/pkexec", "/usr/local/bin/pkexec", "/bin/pkexec"].iter().any(|p| std::path::Path::new(p).exists())
}
```

Register both in `lib.rs`'s `invoke_handler` generate list (alongside the existing `gps_probe_*`).

- [ ] **Step 3: Run** `cargo test --manifest-path src-tauri/Cargo.toml classify` if warm; else CI. **Commit.**

## Task P5: Frontend bindings + button activation

**Files:** Modify `src/location/gpsProbes.ts`, `src/location/GpsSourcePicker.tsx` (+ test).

- [ ] **Step 1: Write the failing test** (GpsSourcePicker.test.tsx): when `pkexecAvailable` and the triage is fixable, the fix button is enabled and clicking it invokes `gps_run_fix` then rescans; the dialout success shows the re-login notice.

```tsx
it('runs the dialout fix via pkexec and shows the re-login notice', async () => {
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case 'gps_pkexec_available': return true as unknown as never;
      case 'gps_probe_gpsd': return { reachable: false } as unknown as never;
      case 'gps_probe_serial_devices': return { devices: [] } as unknown as never;
      case 'gps_probe_dialout': return { member: false, groupExists: true } as unknown as never;
      case 'gps_probe_modemmanager': return { active: false } as unknown as never;
      case 'gps_run_fix': return 'ok' as unknown as never;
      default: return undefined as unknown as never;
    }
  });
  renderPicker();
  const fix = await screen.findByTestId('gps-fix-dialout');
  expect(fix).not.toBeDisabled();
  fireEvent.click(fix);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('gps_run_fix', { action: 'add-dialout' }));
  expect(await screen.findByTestId('gps-relogin-notice')).toBeInTheDocument();
});
```

- [ ] **Step 2: Implement.** Add bindings in `gpsProbes.ts`:

```typescript
export type GpsFixOutcome = 'ok' | 'auth_dismissed' | 'pkexec_missing' | 'failed';
export const pkexecAvailable = () => invoke<boolean>('gps_pkexec_available');
export const runGpsFix = (action: 'add-dialout' | 'mask-modemmanager' | 'unmask-modemmanager') =>
  invoke<GpsFixOutcome>('gps_run_fix', { action });
```

In `GpsSourcePicker.tsx`: probe `pkexecAvailable` once; map each triage `kind` → its action (`dialout`→`add-dialout`, `modemmanager`→`mask-modemmanager`); enable the existing `gps-fix-<kind>` button when `t.fixable && pkexec`; on click call `runGpsFix`, then on `ok` re-run `rescan()`; for `dialout` `ok` show a `gps-relogin-notice` ("Log out and back in for this to take effect — a Linux rule we can't bypass."). When pkexec is unavailable, keep the buttons hidden/disabled with the "use Show command" explanation (current disabled state is the floor). Add an unmask affordance for ModemManager in the triage card once masked.

- [ ] **Step 3: Run** `pnpm exec vitest run src/location/GpsSourcePicker.test.tsx` + `tsc --noEmit`. **Commit.**

## Task P6: Codex adversarial round (REQUIRED — security)

- [ ] Run a directed Codex review on the diff, attacking the security invariants above (argv injection, `$PKEXEC_UID` trust, path hijack, policy scope, bundler-builds-the-bin assumption). Use the stdin custom-prompt pattern from CLAUDE.md; tee to `dev/adversarial/2026-06-13-gps-pkexec-codex.md` (gitignored). Triage findings; apply P0/P1 fixes; record dispositions in the PR body. If Codex quota is hit, defer the round (capacity-defer, not skip) — do NOT merge the privileged helper without it.

## Task P7: Wire-walk + finalize

- [ ] Wire-walk the fix-it flow (operator-greenfield flows → trace to code). Mark PR #678 ready once CI green + Codex round done. Operator smoke on a packaged `.deb`: click "Fix it for me" → polkit dialog → password → fix runs → rescan/notice. Packaged-install behavior is operator-verified (Verification-provenance).

## Out of scope
Native NMEA (`tuxlink-ley0`), live monitoring (`tuxlink-gnws`), Bluetooth NMEA, the `loginctl terminate-session` auto-logout button (offer the notice; auto-logout is a follow-up if the operator wants it).
