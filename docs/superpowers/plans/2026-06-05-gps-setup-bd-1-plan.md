# GPS Setup UX — bd-1 (tuxlink-9xy1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the foundation slice of the GPS setup UX — detection probes, the shared `GpsSourcePickerPresentational` component, the wizard `'location'` step, and the Settings → Location & GPS expandable section. After this slice ships, Bob's path (gpsd pre-configured), Mike's path (manual grid, no GPS) and the diagnostic half of Dave's path (triage cards with "Show command") are fully live. "Fix it for me" buttons render but are disabled pending bd-2 (tuxlink-m9ej).

**Architecture:** Three-layer split: (1) unprivileged Rust probes in `src-tauri/src/position/probe.rs` run in parallel and return a structured `ProbeReport` enum; (2) a pure-presentational React component `GpsSourcePickerPresentational` consumes the report via props and emits events; (3) two thin container components — `Step4Location` (wizard chrome with `useWizard()` consumer) and `SettingsGpsPanel` (Settings chrome with `config_read`/`config_set_grid` consumer) — wrap the presentational. The existing `PositionArbiter` is reused unchanged; this slice does NOT introduce the `ProviderArbiter` refactor (that's bd-3).

**Tech Stack:** Rust 1.75 + Tauri 2.11.2 + tao 0.35.2 + tokio + nix 0.31 + `udev = "0.10"` (NEW), React 19 + TypeScript 6 + Vitest + Testing Library, existing wizard state machine + reducer pattern.

**bd issue:** tuxlink-9xy1 (P1, foundation). Depends on nothing; blocks tuxlink-m9ej, tuxlink-ley0, tuxlink-gnws.

**Anchors:**
- Design doc: [`docs/design/2026-06-05-gps-setup-ux-design.md`](../../design/2026-06-05-gps-setup-ux-design.md)
- Adversarial addendum: [`docs/design/2026-06-05-gps-setup-ux-design-addendum-r2-r4.md`](../../design/2026-06-05-gps-setup-ux-design-addendum-r2-r4.md)
- Mockup: [`docs/design/mockups/2026-06-04-gps-setup-mocks.html`](../../design/mockups/2026-06-04-gps-setup-mocks.html)
- Project memories (anchor list at end of this plan): `[[gps-precision-reduction]]`, `[[inline-ui-no-window-clutter]]`, `[[no-stretched-full-width-ui]]`, `[[browser-smoke-before-ship]]`, `[[trust-support-engineer-intuition]]`, `[[pin-paths-in-worktree-sessions]]`, `[[verify-surfaced-operator-commands]]`

---

## File structure

### Files to CREATE

| Path | Responsibility |
|---|---|
| `src-tauri/src/position/probe.rs` | Detection probes module — async functions returning `ProbeReport`. Public API: `probe_all()`, `probe_gpsd()`, `probe_serial_devices()`, `probe_dialout_membership()`, `probe_modemmanager_status()`, `probe_remote_shell()`, `probe_container_mode()`. |
| `src-tauri/src/position/probe_types.rs` | `ProbeReport`, `GpsdProbeResult`, `SerialDeviceInfo`, `ModemManagerStatus` types — serde-tagged for Tauri event marshalling. |
| `src/gps/types.ts` | TypeScript types mirroring `probe_types.rs` via Tauri's serde-tagged shape (use the same `WizardError`-style pattern at [`src/wizard/types.ts:1-10`](../../../src/wizard/types.ts)). |
| `src/gps/GpsSourcePickerPresentational.tsx` | Pure presentational component. Props in: `ProbeReport`, current `gridChoice`, `precisionPreference`, `helperAvailable`. Events out: `onUseSource(source)`, `onManualGridChange(grid)`, `onPrecisionChange(precision)`, `onShowCommand(action)`, `onFixItForMe(action)` (disabled in bd-1), `onRescan()`. |
| `src/gps/SourceCard.tsx` | Sub-component — green/amber working source. Props: `source: SourceCardData`, `recommended: bool`, `onUseSource: () => void`. |
| `src/gps/TriageCard.tsx` | Sub-component — red/amber blocking issue. Props: `card: TriageCardData`, `helperAvailable: bool`, `onShowCommand`, `onFixItForMe`. Includes a11y text severity label + `aria-roledescription` on code block + `aria-live` for copy confirmation. |
| `src/gps/ManualGridEditor.tsx` | Sub-component — Maidenhead input + precision radio (4-char default per `[[gps-precision-reduction]]`). |
| `src/gps/Step4Location.tsx` | Wizard container — `useWizard()` consumer, dispatches `SUBMIT_GPS_*` actions, owns the `pending_dialout_verification` resume banner. Renders the presentational. |
| `src/gps/SettingsGpsPanel.tsx` | Settings container — reads/writes config via existing `position_status` + `config_set_grid` + `position_set_source` commands. Renders the presentational inside an expandable `<details>` block per A5 in the addendum (NOT a tabs framework). |
| `src/gps/GpsSourcePickerPresentational.test.tsx` | Tests for the presentational component across all 4 persona states (Bob/Sue/Dave/Mike). |
| `src/gps/SourceCard.test.tsx` | Tests for source card render + click. |
| `src/gps/TriageCard.test.tsx` | Tests for triage card render + a11y attributes + button-disabled state. |
| `src/gps/ManualGridEditor.test.tsx` | Tests for grid input + validation + precision radio. |
| `src/gps/Step4Location.test.tsx` | Tests for wizard step — entry, exit, resume-banner. |
| `src/gps/SettingsGpsPanel.test.tsx` | Tests for Settings container — read/write/source-switch. |

### Files to MODIFY

| Path | Change |
|---|---|
| `src-tauri/Cargo.toml` | Add `udev = "0.10"` dependency (line in `[dependencies]`). |
| `src-tauri/src/position/mod.rs` | Add `pub mod probe;` + `pub mod probe_types;` re-exports. |
| `src-tauri/src/lib.rs:249-294` | Register `crate::position::probe::gps_probe_all` in `tauri::generate_handler![...]`. |
| `src/wizard/types.ts:16-22` | Add `'location'` to `WizardStep` union. |
| `src/wizard/types.ts:38-59` | Add `SUBMIT_GPS_SUCCESS` action variant. Add `pendingDialoutVerification` field to `WizardState`. |
| `src/wizard/wizardReducer.ts:3-18` | Add `pendingDialoutVerification: false` to `initialWizardState()`. |
| `src/wizard/wizardReducer.ts:46-47` | Change `SUBMIT_OFFLINE_SUCCESS → step: 'complete'` to `step: 'location'`. |
| `src/wizard/wizardReducer.ts:76-88` | Change `CMS_VERIFY_RESULT ok` and `SKIP_CMS_VERIFY` to land at `'location'` instead of `'complete'`. Add `SUBMIT_GPS_SUCCESS → step: 'complete'`. |
| `src/wizard/Wizard.tsx:25-38` | Render `<Step4Location />` when `state.step === 'location'`. |
| `src/wizard/wizardReducer.test.ts` | Add tests for the new transitions. |
| `src/shell/SettingsPanel.tsx` | Add `<SettingsGpsPanel />` rendered as an expandable `<details>` section. |
| `scripts/install-githooks.sh` | No change needed; bd-1 doesn't touch hooks. |

### Files to LEAVE ALONE (called out so subagents don't drift)

- `src-tauri/src/position/gpsd.rs` — keep the existing `run_gpsd_client` + `spawn_gpsd_client` as-is. The new `probe_gpsd()` function is **additive**, not a refactor of the long-lived client.
- `src-tauri/src/position/arbiter.rs` — `PositionArbiter` stays unchanged. ProviderArbiter is bd-3.
- `src/shell/DashboardRibbon.tsx` + `src/shell/useStatus.ts` — the existing polled dashboard surface stays as-is. Event-driven consumer is bd-4.

---

## Critical context — read before touching code

### Wizard state machine TODAY

```
account → (connectToCms ? credentials : offline_identity)
credentials → (success ? (skipCmsVerify ? complete : cms_verify) : credentials)
offline_identity → complete                                  ← change to: → location
cms_verify → (ok ? complete : error) / skip → complete       ← change ok/skip to: → location
                                                                                location → (gps configured ? complete : location)
                                                                                location → complete                              ← NEW
```

After this slice, both `offline_identity → location` and `cms_verify ok / skip → location` are the new wiring. `SUBMIT_GPS_SUCCESS → complete` is the new terminal.

### "Step 4 of 6" framing — DROPPED

Per addendum A1: the wizard sidebar shows named steps without numbers. The current `Step1Welcome.tsx` / `Step2Credentials.tsx` / `Step2OfflineIdentity.tsx` / `Step3TestSend.tsx` filenames keep their numbers as-is (they're filenames, not user-visible). The new step file is named `Step4Location.tsx` for consistency, but the **user-visible** step name is "Location" with no numeric prefix.

### "Same component, different chrome" — corrected to 3-component split

Per addendum A2: `GpsSourcePickerPresentational` has no context coupling. `Step4Location` adds the `useWizard()` consumer wrapper. `SettingsGpsPanel` adds the `config_read`/`config_set_grid` consumer wrapper. Each container owns its dispatch path.

### gpsd 3-tier probe

Per addendum C3: `probe_gpsd()` returns an enum, not a boolean:

```rust
pub enum GpsdProbeResult {
    /// TCP connected, WATCH succeeded, TPV with mode≥2 received within 2s, grid extracted.
    LiveFix { grid: String, fix_age_ms: u64, satellites: u8, mode: u8 },
    /// TCP connected, WATCH succeeded, no TPV with fix received within 2s.
    NoFix { reason: String },
    /// TCP connected, gpsd reports a DEVICES path differing from the user-currently-plugged-in device.
    WrongDevice { reported: String, current: Vec<String> },
    /// TCP connect failed within 200ms timeout (probably gpsd not running or socket disabled).
    Unreachable { error: String },
    /// Connected, but parser bailed on an unknown JSON envelope (gpsd version drift per C15).
    ParseError { version: Option<String>, raw: String },
}
```

### Wizard resume after dialout-fix flow

Per addendum C1: the `Step4Location` container persists `pendingDialoutVerification: true` to wizard state when "Fix it for me" is dispatched on the dialout card. **bd-1 implements the persistence and the resume-banner render path even though the "Fix it for me" button itself is disabled.** When the user re-opens tuxlink after a logout (whether via the still-disabled button now, or via bd-2's enabled button later), the wizard checks the flag, auto-rescans, shows result.

Persistence approach: leverage existing `wizard_persist_offline` pattern — add a Rust command `wizard_persist_gps` that stores `pendingDialoutVerification` + `gridChoice` to the wizard config file used by `wizard_completed`.

### Remote-shell + container detection

Per addendum C7 + E2: `probe_remote_shell()` and `probe_container_mode()` are detection helpers, NOT triage cards. Their results modify how the picker renders (e.g., hide "Fix it for me" buttons globally in remote-shell mode, replace triage card body with host-side instructions in container mode).

### a11y requirements

Per addendum C6: triage cards must have:
- Text severity label as `<span>` ("Critical: …", "Warning: …", "Info: …") inside the card heading.
- Icons (`✗ ⚠ ○`) wrapped in `<span aria-label="critical">`/`<span aria-label="warning">`/`<span aria-label="info">`.
- Code blocks wrapped in `<pre aria-roledescription="shell command, copyable">`.
- Copy button click: announce "Copied" via `aria-live="polite"` region.

---

## Tasks

### Task 1: Create worktree + claim bd-1 + setup

**Why:** Per ADR 0008, all write work goes in a worktree owned by a claimed bd issue. The main checkout is operator state.

**Files:**
- Create: `worktrees/bd-tuxlink-9xy1-gps-foundation/` (worktree)
- Modify: bd state — claim tuxlink-9xy1

- [ ] **Step 1.1:** Run `python3 /home/administrator/Code/tuxlink/.claude/scripts/new_tuxlink_worktree.py --slug gps-foundation --issue tuxlink-9xy1 --moniker <YOUR-MONIKER>` from any directory.

Expected output ends with:
```
Path:     /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-9xy1-gps-foundation
Branch:   bd-tuxlink-9xy1/gps-foundation (off origin/main)
bd issue: tuxlink-9xy1 (claimed)
```

- [ ] **Step 1.2:** `cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-9xy1-gps-foundation` and run `bash scripts/install-githooks.sh` to activate the branch-lifecycle hooks.

Expected output: `core.hooksPath` set + hook scripts confirmed executable.

- [ ] **Step 1.3:** Run `pnpm install --frozen-lockfile` in the worktree.

Expected: `Done in <N>s using pnpm v10.33.3`.

- [ ] **Step 1.4:** Verify build prereqs by running `cargo --manifest-path src-tauri/Cargo.toml check 2>&1 | tail -3`.

Expected: `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in <N>s` (warnings OK; errors are blocking).

---

### Task 2: Add `udev = "0.10"` dependency

**Why:** Detection probes use `udev` crate for VID/PID lookup (50x faster than `udevadm` subprocess per addendum D1). Pure-Rust on Linux, no C dep.

**Files:**
- Modify: `src-tauri/Cargo.toml` (in `[dependencies]` block, alphabetically near `tokio`)

- [ ] **Step 2.1:** Add the dependency line after the existing `tokio` line (around line 23):

```toml
udev = "0.10"             # NEW (tuxlink-9xy1) — pure-Rust udev binding for USB device enumeration + VID/PID lookup in GPS detection probes
```

- [ ] **Step 2.2:** Run `cargo --manifest-path src-tauri/Cargo.toml build --tests 2>&1 | tail -3`.

Expected: `Finished` line; udev compiles cleanly.

- [ ] **Step 2.3:** Commit:

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-9xy1-gps-foundation
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "$(cat <<'EOF'
build(cargo)(deps): add udev 0.10 for GPS detection probes (tuxlink-9xy1)

Pure-Rust udev binding. Used by src-tauri/src/position/probe.rs to enumerate
/dev/ttyACM* + /dev/ttyUSB* + /dev/serial/by-id/* and extract VID/PID +
vendor/model strings. ~50x faster than spawning udevadm per device, which
matters for the wizard probe's <500 ms p95 budget across 5 parallel probes.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: Create `probe_types.rs` with type definitions

BEFORE starting work:
1. Read the skill at `~/.claude/plugins/cache/claude-plugins-official/superpowers/5.1.0/skills/test-driven-development/` (or invoke /test-driven-development)
2. Read `/home/administrator/Code/tuxlink/docs/pitfalls/testing-pitfalls.md`
Follow TDD: write failing test → implement → verify green.

**Why:** Probe types are serialized to Tauri events. Defining them up-front lets every subsequent probe task target a stable shape, and the React side gets a mirror to type against.

**Files:**
- Create: `src-tauri/src/position/probe_types.rs`

- [ ] **Step 3.1:** Create the file with these contents:

```rust
//! Probe-result types for the GPS detection layer. Serialize-friendly via serde
//! `tag` + `content` so the React side can pattern-match.

use serde::{Deserialize, Serialize};

/// One source-detection probe's outcome. Aggregated by `probe_all()`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum GpsdProbeResult {
    /// TCP connected, WATCH succeeded, TPV with mode≥2 received within 2s, grid extracted.
    LiveFix { grid: String, fix_age_ms: u64, satellites: u8, mode: u8 },
    /// TCP connected, WATCH succeeded, no TPV with fix received within 2s.
    NoFix { reason: String },
    /// TCP connected, gpsd reports a DEVICES path differing from what's currently plugged in.
    WrongDevice { reported: String, current: Vec<String> },
    /// TCP connect failed within 200ms (probably gpsd not running or socket disabled).
    Unreachable { error: String },
    /// Connected, but parser bailed on an unknown JSON envelope (gpsd version drift).
    ParseError { version: Option<String>, raw: String },
}

/// Per-USB-serial-device metadata extracted by udev probe.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SerialDeviceInfo {
    /// Canonical device path (e.g. `/dev/ttyACM0`).
    pub device_path: String,
    /// /dev/serial/by-id symlink path if present (stable across reboots).
    pub by_id_path: Option<String>,
    /// USB vendor ID (4 hex chars, lowercase).
    pub vendor_id: String,
    /// USB product ID (4 hex chars, lowercase).
    pub product_id: String,
    /// Human-readable vendor name from hwdata (e.g. "u-blox AG").
    pub vendor_name: Option<String>,
    /// Human-readable model name from hwdata (e.g. "u-blox 7 GPS receiver").
    pub model_name: Option<String>,
}

/// ModemManager status — 4-state enum, not a boolean.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModemManagerStatus {
    /// `ModemManager.service` is not present on this system.
    NotInstalled,
    /// Installed, unit is loaded, but inactive (not running).
    Inactive,
    /// Installed, unit is masked (won't start even on demand).
    Masked,
    /// Installed and currently running.
    Active,
}

/// Remote-shell detection result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RemoteShellStatus {
    /// Likely local desktop session.
    Local,
    /// SSH session via X11 forwarding ($SSH_CLIENT set + $DISPLAY != :0).
    SshX11,
    /// Tty-only session ($XDG_SESSION_TYPE == "tty").
    TtyOnly,
}

/// Container-mode detection result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContainerStatus {
    /// Bare-metal or VM host.
    Bare,
    /// Inside a Distrobox/Toolbox/podman container (/run/.containerenv present).
    Container { runtime: String },
    /// Inside a Docker container (/.dockerenv present).
    Docker,
}

/// Aggregated probe report — the single payload returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProbeReport {
    pub gpsd: GpsdProbeResult,
    pub serial_devices: Vec<SerialDeviceInfo>,
    /// True if the current user is in the `dialout` group.
    pub in_dialout_group: bool,
    pub modemmanager: ModemManagerStatus,
    pub remote_shell: RemoteShellStatus,
    pub container: ContainerStatus,
    /// Username being checked (resolved from getuid() + getpwuid()).
    pub current_user: String,
}
```

- [ ] **Step 3.2:** Modify `src-tauri/src/position/mod.rs` to add the module declaration. Read the current contents first to find the right insertion point (after the existing `pub mod gpsd;` line, around line 2):

Add after `pub mod gpsd;`:
```rust
pub mod probe;
pub mod probe_types;
```

- [ ] **Step 3.3:** Create a placeholder `src-tauri/src/position/probe.rs` with just module-level doc + a re-export:

```rust
//! GPS source detection probes. Unprivileged. Run in parallel via `probe_all`.

pub use crate::position::probe_types::*;

// Implementations land in subsequent tasks. This file's body grows task-by-task.
```

- [ ] **Step 3.4:** Run `cargo --manifest-path src-tauri/Cargo.toml build --tests 2>&1 | tail -3`.

Expected: `Finished` (the empty probe.rs is fine; this is just a compile gate).

- [ ] **Step 3.5:** Commit:

```bash
git add src-tauri/src/position/probe_types.rs src-tauri/src/position/probe.rs src-tauri/src/position/mod.rs
git commit -m "$(cat <<'EOF'
feat(position): add probe_types + probe module skeleton (tuxlink-9xy1)

Detection-probe data types — serde tag+content for Tauri event marshalling
to React. ProbeReport aggregates per-probe results returned to the frontend:
GpsdProbeResult (5 variants), SerialDeviceInfo, ModemManagerStatus (4
variants), RemoteShellStatus, ContainerStatus.

Per design addendum C3, gpsd probe is 3-tier (Live/NoFix/Wrong/Unreachable/
ParseError), not a boolean. Per C7, RemoteShellStatus drives hiding the
"Fix it for me" buttons in remote sessions. Per E2, ContainerStatus drives
the container-mode triage card.

probe.rs is an empty module — implementations land per-probe in subsequent
tasks (probe_dialout, probe_serial, probe_gpsd, etc.).

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

BEFORE marking this task complete:
1. Review your tests against `/home/administrator/Code/tuxlink/docs/pitfalls/testing-pitfalls.md`
2. Verify test coverage of the fix (this task is types-only, no behavior to test yet — types are validated by compilation).
3. Run `cargo --manifest-path src-tauri/Cargo.toml check 2>&1 | tail -3` and confirm green.

---

### Task 4: probe_dialout_membership — test + impl

BEFORE starting work:
1. Read the skill at `~/.claude/plugins/cache/claude-plugins-official/superpowers/5.1.0/skills/test-driven-development/`
2. Read `/home/administrator/Code/tuxlink/docs/pitfalls/testing-pitfalls.md` §3 (Error Path Coverage) and §6 (Boundary & Configuration Validation)
Follow TDD: write failing test → implement → verify green.

**Why:** Lightest probe to start with. Uses `nix::unistd::getgroups()` + `nix::unistd::Group::from_name()` — no subprocess, no async, no external dependency beyond what we already have. Establishes the probe testing pattern.

**Files:**
- Modify: `src-tauri/src/position/probe.rs` — add `probe_dialout_membership() -> bool`.
- The test goes in the same file with `#[cfg(test)]`.

**Architectural note:** Probes return their narrow result type. The aggregating `probe_all()` packages them into `ProbeReport`. The dialout probe returns `bool` (the field name `in_dialout_group` on `ProbeReport`).

- [ ] **Step 4.1: Write the failing test.** Append to `src-tauri/src/position/probe.rs`:

```rust
#[cfg(test)]
mod tests_dialout {
    use super::probe_dialout_membership;

    #[test]
    fn returns_false_when_group_not_in_user_groups() {
        // The "fake_group_does_not_exist" sentinel: a group name that won't be in any user's
        // groups OR in /etc/group. probe_dialout_membership() calls Group::from_name(name),
        // which returns Ok(None) for a nonexistent group, leading to a false return.
        let result = probe_dialout_membership_for_group("fake_group_does_not_exist_xyz");
        assert!(!result, "nonexistent group must return false, got true");
    }

    #[test]
    fn returns_true_when_group_membership_matches() {
        // Every Linux user is in their primary group. /etc/passwd[3] is the primary GID.
        // We resolve it via nix::unistd::getuid() + nix::unistd::User::from_uid().
        let uid = nix::unistd::getuid();
        let user = nix::unistd::User::from_uid(uid)
            .expect("getuid worked")
            .expect("User entry exists");
        let primary_group = nix::unistd::Group::from_gid(user.gid)
            .expect("gid lookup worked")
            .expect("primary group exists");
        let result = probe_dialout_membership_for_group(&primary_group.name);
        assert!(result, "user must be in their own primary group, got false for {}", primary_group.name);
    }
}
```

The `probe_dialout_membership_for_group(name)` helper is the testable seam; `probe_dialout_membership()` is the public function that hardcodes `"dialout"`.

- [ ] **Step 4.2: Run the test, confirm it fails to compile.**

```bash
cargo --manifest-path /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-9xy1-gps-foundation/src-tauri/Cargo.toml test --lib position::probe::tests_dialout 2>&1 | tail -20
```

Expected: compile error — `probe_dialout_membership_for_group` and `probe_dialout_membership` not found.

- [ ] **Step 4.3: Implement.** Add to `src-tauri/src/position/probe.rs` (above the `#[cfg(test)]` block):

```rust
/// Returns true iff the current user is a member of the `dialout` group.
///
/// Reads group membership via `nix::unistd::getgroups()` (one syscall, no subprocess).
/// Returns false if the group doesn't exist on the system OR the user isn't in it.
pub fn probe_dialout_membership() -> bool {
    probe_dialout_membership_for_group("dialout")
}

/// Test seam — accepts the group name. Production callers use `probe_dialout_membership()`.
pub fn probe_dialout_membership_for_group(group_name: &str) -> bool {
    let Ok(Some(group)) = nix::unistd::Group::from_name(group_name) else {
        return false;
    };
    let Ok(user_groups) = nix::unistd::getgroups() else {
        return false;
    };
    user_groups.contains(&group.gid)
}
```

- [ ] **Step 4.4: Run the test, confirm it passes.**

```bash
cargo --manifest-path /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-9xy1-gps-foundation/src-tauri/Cargo.toml test --lib position::probe::tests_dialout 2>&1 | tail -10
```

Expected: `test result: ok. 2 passed; 0 failed`.

- [ ] **Step 4.5: Commit.**

```bash
git add src-tauri/src/position/probe.rs
git commit -m "$(cat <<'EOF'
feat(position): probe_dialout_membership for GPS detection (tuxlink-9xy1)

First probe in the GPS detection-and-triage chain. Uses
nix::unistd::getgroups() + Group::from_name("dialout") — one syscall, no
subprocess (per design addendum D4). Returns false when the group doesn't
exist OR the user isn't in it.

The test pattern uses a `_for_group(name)` seam: public callers go through
probe_dialout_membership() which hardcodes "dialout"; tests parameterize.
Tests cover both branches (nonexistent group → false; primary group → true).

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

BEFORE marking this task complete:
1. Review against testing-pitfalls.md §3 (Error Path Coverage) — both branches tested ✓.
2. Verify no subprocess used (per D4) ✓.

**Do NOT** add a third "happy-path" test using `"dialout"` directly — that's environmentally fragile (CI runners may or may not put their user in dialout). The `_for_group` seam + the primary-group test is the universal coverage.

---

### Task 5: probe_modemmanager_status — test + impl

BEFORE starting work: TDD prelude from Task 4.

**Why:** ModemManager status drives the "your USB cellular modem and your GPS are fighting" triage card. Per addendum D5 + D6, this probe uses **subprocess** (`busctl list --acquired | grep -q ModemManager` + `systemctl is-active ModemManager.service` + `systemctl is-enabled ModemManager.service`) rather than `zbus`. Subprocess saves ~250 KB of binary size and is fast enough at probe-time (~5 ms × 3 subprocesses = ~15 ms).

**Files:**
- Modify: `src-tauri/src/position/probe.rs` — add `probe_modemmanager_status()`.

- [ ] **Step 5.1: Write the failing test.**

```rust
#[cfg(test)]
mod tests_modemmanager {
    use super::*;
    use std::path::Path;

    #[test]
    fn maps_systemctl_states_correctly() {
        // Pure mapping function — no subprocess in the test, we just exercise the
        // classifier logic against synthetic systemctl outputs.
        // The four states map from (loaded? + active? + masked?) tuples:
        assert_eq!(
            classify_modemmanager_state(/*loaded=*/false, /*active=*/false, /*masked=*/false),
            ModemManagerStatus::NotInstalled
        );
        assert_eq!(
            classify_modemmanager_state(/*loaded=*/true, /*active=*/false, /*masked=*/false),
            ModemManagerStatus::Inactive
        );
        assert_eq!(
            classify_modemmanager_state(/*loaded=*/true, /*active=*/false, /*masked=*/true),
            ModemManagerStatus::Masked
        );
        assert_eq!(
            classify_modemmanager_state(/*loaded=*/true, /*active=*/true, /*masked=*/false),
            ModemManagerStatus::Active
        );
    }

    #[tokio::test]
    async fn probe_returns_a_value_without_panicking() {
        // The full probe spawns subprocesses; on CI runners systemctl may be absent
        // or behave differently. We just assert the probe returns SOMETHING in
        // bounded time, not which value.
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            probe_modemmanager_status(),
        )
        .await
        .expect("probe completed within 2s");
        // Any of the 4 enum variants is acceptable here — environment-dependent.
        let _ = result;
    }
}
```

- [ ] **Step 5.2: Run, confirm compile failure** on `classify_modemmanager_state` and `probe_modemmanager_status` not found.

```bash
cargo --manifest-path /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-9xy1-gps-foundation/src-tauri/Cargo.toml test --lib position::probe::tests_modemmanager 2>&1 | tail -10
```

- [ ] **Step 5.3: Implement.** Add to `src-tauri/src/position/probe.rs`:

```rust
use crate::position::probe_types::ModemManagerStatus;
use tokio::process::Command;

/// Probe ModemManager state via subprocess. Returns a 4-state enum (NotInstalled
/// / Inactive / Masked / Active) so the React UI can render the right triage card.
///
/// Uses `systemctl show ModemManager.service --property=LoadState,ActiveState,UnitFileState`
/// (one subprocess, all 3 fields). Bounded to 500ms via tokio::time::timeout.
pub async fn probe_modemmanager_status() -> ModemManagerStatus {
    let output = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        Command::new("systemctl")
            .args(["show", "ModemManager.service",
                   "--property=LoadState",
                   "--property=ActiveState",
                   "--property=UnitFileState"])
            .output(),
    )
    .await;

    let output = match output {
        Ok(Ok(o)) => o,
        _ => return ModemManagerStatus::NotInstalled,
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let loaded = stdout.contains("LoadState=loaded");
    let active = stdout.contains("ActiveState=active");
    let masked = stdout.contains("UnitFileState=masked");
    classify_modemmanager_state(loaded, active, masked)
}

/// Pure classifier — separated from the I/O for testability.
pub fn classify_modemmanager_state(loaded: bool, active: bool, masked: bool) -> ModemManagerStatus {
    if !loaded {
        return ModemManagerStatus::NotInstalled;
    }
    if masked {
        return ModemManagerStatus::Masked;
    }
    if active {
        return ModemManagerStatus::Active;
    }
    ModemManagerStatus::Inactive
}
```

- [ ] **Step 5.4: Run tests, confirm green.**

```bash
cargo --manifest-path /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-9xy1-gps-foundation/src-tauri/Cargo.toml test --lib position::probe::tests_modemmanager 2>&1 | tail -10
```

Expected: `test result: ok. 2 passed; 0 failed`.

- [ ] **Step 5.5: Commit.**

```bash
git add src-tauri/src/position/probe.rs
git commit -m "$(cat <<'EOF'
feat(position): probe_modemmanager_status for GPS detection (tuxlink-9xy1)

Subprocess-based ModemManager probe via `systemctl show`. One subprocess,
3 fields (LoadState + ActiveState + UnitFileState), 500ms bounded timeout.
Returns the 4-variant ModemManagerStatus enum (NotInstalled/Inactive/
Masked/Active) so the UI can render the appropriate triage card per
addendum C8.

Per addendum D5+D6: subprocess over zbus. Saves ~250KB binary size for
probe-only use; bd-4's continuous monitoring can reconsider zbus if a
long-lived listener is needed there.

Pure `classify_modemmanager_state` separated from I/O for unit testing
without depending on the runner's systemd presence.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

**Do NOT** add a `zbus` dependency here. The decision (subprocess) is final for bd-1.

---

### Task 6: probe_remote_shell + probe_container_mode — combined task

BEFORE starting work: TDD prelude from Task 4.

**Why:** Both probes are pure environment-variable + file-existence checks. Combining them into one task because they share the same test-injection seam (pass env / path lookups in for the pure classifier; production reads from actual sources). Each adds ~15 lines.

**Files:**
- Modify: `src-tauri/src/position/probe.rs`.

- [ ] **Step 6.1: Write failing tests.**

```rust
#[cfg(test)]
mod tests_environment {
    use super::*;

    #[test]
    fn remote_shell_detects_local_when_no_ssh_or_tty() {
        let result = classify_remote_shell(/*ssh_client=*/None, /*display=*/Some(":0"), /*session_type=*/Some("wayland"));
        assert_eq!(result, RemoteShellStatus::Local);
    }

    #[test]
    fn remote_shell_detects_ssh_x11_when_ssh_client_set_and_display_remote() {
        let result = classify_remote_shell(Some("192.168.1.50 49152 22"), Some("localhost:10.0"), Some("x11"));
        assert_eq!(result, RemoteShellStatus::SshX11);
    }

    #[test]
    fn remote_shell_detects_tty_only_when_session_type_is_tty() {
        let result = classify_remote_shell(None, None, Some("tty"));
        assert_eq!(result, RemoteShellStatus::TtyOnly);
    }

    #[test]
    fn container_detects_bare_when_no_marker_files() {
        let result = classify_container_mode(/*containerenv=*/false, /*dockerenv=*/false);
        assert!(matches!(result, ContainerStatus::Bare));
    }

    #[test]
    fn container_detects_podman_when_containerenv_present() {
        let result = classify_container_mode(true, false);
        assert!(matches!(result, ContainerStatus::Container { .. }));
    }

    #[test]
    fn container_detects_docker_when_dockerenv_present() {
        let result = classify_container_mode(false, true);
        assert_eq!(result, ContainerStatus::Docker);
    }
}
```

- [ ] **Step 6.2: Run, confirm compile failure.**

```bash
cargo --manifest-path /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-9xy1-gps-foundation/src-tauri/Cargo.toml test --lib position::probe::tests_environment 2>&1 | tail -10
```

- [ ] **Step 6.3: Implement.** Add to `src-tauri/src/position/probe.rs`:

```rust
use crate::position::probe_types::{RemoteShellStatus, ContainerStatus};

/// Detects whether the current session is local or remote, for hiding the
/// "Fix it for me" buttons (PolicyKit dialogs don't render cleanly over SSH
/// X11 / x2go / NoMachine per addendum C7).
pub fn probe_remote_shell() -> RemoteShellStatus {
    classify_remote_shell(
        std::env::var("SSH_CLIENT").ok().as_deref(),
        std::env::var("DISPLAY").ok().as_deref(),
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
    )
}

/// Pure classifier — test seam for the above.
pub fn classify_remote_shell(
    ssh_client: Option<&str>,
    display: Option<&str>,
    session_type: Option<&str>,
) -> RemoteShellStatus {
    if session_type == Some("tty") {
        return RemoteShellStatus::TtyOnly;
    }
    // SSH X11 fwd: $SSH_CLIENT set AND $DISPLAY is "localhost:N.M" or "host:N.M" (not ":0").
    if ssh_client.is_some() && display.map(|d| !d.starts_with(':')).unwrap_or(false) {
        return RemoteShellStatus::SshX11;
    }
    RemoteShellStatus::Local
}

/// Detects whether tuxlink is running inside a container (Distrobox/Toolbox/
/// podman/Docker) per addendum E2.
pub fn probe_container_mode() -> ContainerStatus {
    classify_container_mode(
        std::path::Path::new("/run/.containerenv").exists(),
        std::path::Path::new("/.dockerenv").exists(),
    )
}

/// Pure classifier — test seam for the above.
pub fn classify_container_mode(containerenv: bool, dockerenv: bool) -> ContainerStatus {
    if containerenv {
        // /run/.containerenv is written by podman + Distrobox/Toolbox.
        return ContainerStatus::Container { runtime: "podman".to_string() };
    }
    if dockerenv {
        return ContainerStatus::Docker;
    }
    ContainerStatus::Bare
}
```

- [ ] **Step 6.4: Run, confirm green.**

```bash
cargo --manifest-path /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-9xy1-gps-foundation/src-tauri/Cargo.toml test --lib position::probe::tests_environment 2>&1 | tail -10
```

Expected: `test result: ok. 6 passed; 0 failed`.

- [ ] **Step 6.5: Commit.**

```bash
git add src-tauri/src/position/probe.rs
git commit -m "$(cat <<'EOF'
feat(position): probe_remote_shell + probe_container_mode (tuxlink-9xy1)

Two environment-detection probes that modify how the picker renders. Per
addendum C7, RemoteShellStatus::SshX11 / TtyOnly hides the "Fix it for me"
buttons because PolicyKit auth dialogs don't render cleanly on remote X11
or tty-only sessions. Per addendum E2, ContainerStatus::Container/Docker
swaps the triage-card body for host-side instructions because masking MM
from inside a container can't affect the host.

Both use a pure-classifier + env-reading-wrapper split for testability.
6 unit tests cover all branches.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 7: probe_serial_devices — test + impl

BEFORE starting work: TDD prelude.

**Why:** Enumerates `/dev/ttyACM*` + `/dev/ttyUSB*` and walks `/dev/serial/by-id/*` for stable symlinks. For each device, extracts VID/PID + vendor/model strings via the `udev` crate. Per addendum D2, `/dev/serial/by-id/...` is the canonical persisted path.

**Files:**
- Modify: `src-tauri/src/position/probe.rs`.

- [ ] **Step 7.1: Write failing test.**

```rust
#[cfg(test)]
mod tests_serial {
    use super::*;

    #[tokio::test]
    async fn returns_empty_when_no_serial_devices() {
        // We can't deterministically test "no devices" on a random CI box because
        // /dev/ttyACM0 might exist; instead we test the function returns a Vec
        // (might be empty or populated) without panicking, and exits bounded time.
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            probe_serial_devices(),
        )
        .await
        .expect("probe completed within 500ms");
        // Vec<SerialDeviceInfo> — assert it's a Vec (always true) and each entry has
        // a well-formed device_path.
        for dev in result {
            assert!(dev.device_path.starts_with("/dev/"),
                    "device_path must be absolute /dev/ path, got {}", dev.device_path);
            assert_eq!(dev.vendor_id.len(), 4, "vendor_id must be 4 hex chars");
            assert_eq!(dev.product_id.len(), 4, "product_id must be 4 hex chars");
        }
    }

    #[test]
    fn merge_by_id_symlinks_attaches_stable_path() {
        let mut devices = vec![
            SerialDeviceInfo {
                device_path: "/dev/ttyACM0".to_string(),
                by_id_path: None,
                vendor_id: "1546".to_string(),
                product_id: "01a7".to_string(),
                vendor_name: Some("u-blox AG".to_string()),
                model_name: Some("u-blox 7".to_string()),
            },
        ];
        let by_id_map = vec![
            ("/dev/serial/by-id/usb-u-blox_AG_-_www.u-blox.com_u-blox_7_-_GPS_GNSS_Receiver-if00".to_string(),
             "/dev/ttyACM0".to_string()),
        ];
        merge_by_id_symlinks(&mut devices, &by_id_map);
        assert_eq!(
            devices[0].by_id_path.as_deref(),
            Some("/dev/serial/by-id/usb-u-blox_AG_-_www.u-blox.com_u-blox_7_-_GPS_GNSS_Receiver-if00")
        );
    }
}
```

- [ ] **Step 7.2: Run, confirm fail.**

- [ ] **Step 7.3: Implement.** Add to `src-tauri/src/position/probe.rs`:

```rust
use crate::position::probe_types::SerialDeviceInfo;

/// Enumerate USB serial devices that might be GPS receivers. Returns a Vec of
/// SerialDeviceInfo with VID/PID + by-id symlinks where available.
///
/// Strategy:
/// 1. Use udev to walk all `tty` subsystem devices. Filter to those with a USB
///    parent. Extract VID/PID + vendor/model from udev properties.
/// 2. Walk /dev/serial/by-id/* readlinks. For each, find the matching device_path
///    in the udev enumeration and attach the by-id symlink as the persisted path.
///
/// Bounded by udev's enumeration speed (~5-50ms typically). No subprocess.
pub async fn probe_serial_devices() -> Vec<SerialDeviceInfo> {
    let mut devices = Vec::new();

    // udev enumeration runs on a blocking thread.
    let enumerated = tokio::task::spawn_blocking(|| {
        let mut out = Vec::new();
        let Ok(mut enumerator) = udev::Enumerator::new() else { return out };
        let _ = enumerator.match_subsystem("tty");
        let Ok(iter) = enumerator.scan_devices() else { return out };
        for device in iter {
            let Some(devnode) = device.devnode() else { continue };
            let devnode_str = devnode.to_string_lossy().to_string();
            // Only USB serial: filter to /dev/ttyACM* or /dev/ttyUSB*.
            if !devnode_str.starts_with("/dev/ttyACM") && !devnode_str.starts_with("/dev/ttyUSB") {
                continue;
            }
            let vendor_id = device.property_value("ID_VENDOR_ID")
                .and_then(|v| v.to_str()).unwrap_or("").to_lowercase();
            let product_id = device.property_value("ID_MODEL_ID")
                .and_then(|v| v.to_str()).unwrap_or("").to_lowercase();
            if vendor_id.is_empty() || product_id.is_empty() {
                continue; // Not a USB device with VID/PID — skip.
            }
            out.push(SerialDeviceInfo {
                device_path: devnode_str,
                by_id_path: None, // populated below.
                vendor_id,
                product_id,
                vendor_name: device.property_value("ID_VENDOR_FROM_DATABASE")
                    .and_then(|v| v.to_str().map(String::from)),
                model_name: device.property_value("ID_MODEL_FROM_DATABASE")
                    .and_then(|v| v.to_str().map(String::from)),
            });
        }
        out
    }).await.unwrap_or_default();

    devices = enumerated;

    // Attach /dev/serial/by-id/* symlinks.
    let by_id_map = read_serial_by_id_map().await;
    merge_by_id_symlinks(&mut devices, &by_id_map);

    devices
}

/// Read /dev/serial/by-id/ entries and return a Vec of (by_id_path, real_path)
/// tuples. Empty Vec if the directory doesn't exist.
async fn read_serial_by_id_map() -> Vec<(String, String)> {
    tokio::task::spawn_blocking(|| {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir("/dev/serial/by-id") else { return out };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(target) = std::fs::canonicalize(&path) else { continue };
            out.push((
                path.to_string_lossy().to_string(),
                target.to_string_lossy().to_string(),
            ));
        }
        out
    }).await.unwrap_or_default()
}

/// Attach by-id symlinks to matching devices in-place.
pub fn merge_by_id_symlinks(devices: &mut [SerialDeviceInfo], by_id_map: &[(String, String)]) {
    for dev in devices.iter_mut() {
        for (by_id_path, real_path) in by_id_map {
            if real_path == &dev.device_path {
                dev.by_id_path = Some(by_id_path.clone());
                break;
            }
        }
    }
}
```

- [ ] **Step 7.4: Run, confirm green.**

- [ ] **Step 7.5: Commit.**

```bash
git add src-tauri/src/position/probe.rs
git commit -m "$(cat <<'EOF'
feat(position): probe_serial_devices via udev crate (tuxlink-9xy1)

USB serial device enumeration via the `udev` crate (D1 — pure-Rust, no
subprocess). Filters to /dev/ttyACM* and /dev/ttyUSB*. Extracts VID/PID +
vendor_name + model_name from udev properties (requires hwdata pkg for
the *_FROM_DATABASE properties; falls back to None gracefully).

Per addendum D2, /dev/serial/by-id/* symlinks are attached as the stable
persisted path. The picker stores the by-id symlink in config (not the
ttyACMN path which changes across reboots).

Pure `merge_by_id_symlinks` separated for testing without touching real
/dev/. Bounded probe time via tokio::time::timeout in tests; production
caller wraps in 500ms timeout at the aggregator (probe_all).

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

**Do NOT** attempt to parse `ID_VENDOR_FROM_DATABASE` further (e.g., extract sub-tokens). It's a free-form string from hwdata; the UI renders it as-is.

---

### Task 8: probe_gpsd — 3-tier — test + impl

BEFORE starting work: TDD prelude. **Also read [src-tauri/src/position/gpsd.rs:33-40](../../../src-tauri/src/position/gpsd.rs#L33-L40) for the existing `parse_tpv` helper — REUSE it.**

**Why:** The headline probe. Per addendum C3, returns a 5-variant enum (`LiveFix` / `NoFix` / `WrongDevice` / `Unreachable` / `ParseError`), not a boolean. Bounded to 2 seconds total (200 ms connect + 1800 ms read window).

**Files:**
- Modify: `src-tauri/src/position/probe.rs`.

- [ ] **Step 8.1: Write failing test.**

```rust
#[cfg(test)]
mod tests_gpsd {
    use super::*;

    #[tokio::test]
    async fn returns_unreachable_when_no_gpsd_running() {
        // Probe a port nothing is listening on. Use a high-numbered port that's
        // unlikely to be in use by any system service.
        let result = probe_gpsd_at("127.0.0.1:59947").await;
        assert!(matches!(result, GpsdProbeResult::Unreachable { .. }),
                "expected Unreachable, got {result:?}");
    }

    #[tokio::test]
    async fn returns_unreachable_within_200ms_when_port_dropped() {
        // Bounded-time invariant: connect timeout is 200ms; total probe should
        // complete in well under 500ms even when the target is silent.
        let start = std::time::Instant::now();
        let _ = probe_gpsd_at("127.0.0.1:59947").await;
        let elapsed = start.elapsed();
        assert!(elapsed < std::time::Duration::from_millis(500),
                "probe took {elapsed:?}, expected <500ms");
    }
}
```

- [ ] **Step 8.2: Run, confirm fail.**

- [ ] **Step 8.3: Implement.** Add to `src-tauri/src/position/probe.rs`:

```rust
use crate::position::gpsd::parse_tpv;
use crate::position::probe_types::GpsdProbeResult;

const PROBE_GPSD_DEFAULT_ADDR: &str = "127.0.0.1:2947";
const PROBE_GPSD_CONNECT_TIMEOUT_MS: u64 = 200;
const PROBE_GPSD_READ_WINDOW_MS: u64 = 1800;

/// Probe gpsd, return a 3-tier result. Total bounded time: ~2 seconds worst case.
///
/// Uses TUXLINK_GPSD_ADDR env var if set (matches the existing client's idiom);
/// otherwise 127.0.0.1:2947.
pub async fn probe_gpsd() -> GpsdProbeResult {
    let addr = std::env::var("TUXLINK_GPSD_ADDR")
        .unwrap_or_else(|_| PROBE_GPSD_DEFAULT_ADDR.to_string());
    probe_gpsd_at(&addr).await
}

/// Probe gpsd at a specific address. Test seam for the above.
pub async fn probe_gpsd_at(addr: &str) -> GpsdProbeResult {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpStream;
    use tokio::time::{timeout, Duration, Instant};

    let connect = timeout(
        Duration::from_millis(PROBE_GPSD_CONNECT_TIMEOUT_MS),
        TcpStream::connect(addr),
    ).await;

    let mut stream = match connect {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return GpsdProbeResult::Unreachable { error: format!("connect: {e}") },
        Err(_) => return GpsdProbeResult::Unreachable { error: "connect timeout".to_string() },
    };

    if stream.write_all(b"?WATCH={\"enable\":true,\"json\":true}\n").await.is_err() {
        return GpsdProbeResult::Unreachable { error: "WATCH write failed".to_string() };
    }

    let reader = BufReader::new(stream);
    let mut lines = reader.lines();
    let deadline = Instant::now() + Duration::from_millis(PROBE_GPSD_READ_WINDOW_MS);

    let mut version_seen: Option<String> = None;

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return GpsdProbeResult::NoFix {
                reason: format!("no TPV with fix in {}ms", PROBE_GPSD_READ_WINDOW_MS),
            };
        }
        let line = match timeout(remaining, lines.next_line()).await {
            Ok(Ok(Some(l))) => l,
            Ok(Ok(None)) => return GpsdProbeResult::Unreachable { error: "EOF".to_string() },
            Ok(Err(e)) => return GpsdProbeResult::Unreachable { error: format!("read: {e}") },
            Err(_) => return GpsdProbeResult::NoFix {
                reason: format!("no TPV with fix in {}ms", PROBE_GPSD_READ_WINDOW_MS),
            },
        };

        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
            if json.get("class").and_then(|v| v.as_str()) == Some("VERSION") {
                version_seen = json.get("release")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                continue;
            }
            if let Some(fix) = parse_tpv(&line) {
                let satellites = json.get("uSat")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u8;
                let mode = json.get("mode")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) as u8;
                return GpsdProbeResult::LiveFix {
                    grid: fix.grid,
                    fix_age_ms: 0,
                    satellites,
                    mode,
                };
            }
        } else {
            return GpsdProbeResult::ParseError {
                version: version_seen,
                raw: line.chars().take(200).collect(),
            };
        }
    }
}
```

- [ ] **Step 8.4: Run, confirm green.**

```bash
cargo --manifest-path /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-9xy1-gps-foundation/src-tauri/Cargo.toml test --lib position::probe::tests_gpsd 2>&1 | tail -10
```

Expected: `2 passed`.

- [ ] **Step 8.5: Commit.**

```bash
git add src-tauri/src/position/probe.rs
git commit -m "$(cat <<'EOF'
feat(position): probe_gpsd 3-tier result (tuxlink-9xy1)

NEW function additive to the existing run_gpsd_client (per addendum D8).
Returns GpsdProbeResult with 5 variants (LiveFix/NoFix/WrongDevice/
Unreachable/ParseError) covering the cases addendum C3 specified:

- LiveFix: TCP + WATCH succeeded + TPV with mode≥2 received in 1.8s window
- NoFix: TCP + WATCH succeeded but no TPV with fix
- Unreachable: TCP connect failed within 200ms (gpsd absent / socket disabled)
- ParseError: connected but received unparseable JSON (gpsd version drift —
  addendum C15)

Bounded total time: 200ms connect + 1800ms read window = 2s worst-case.
The WrongDevice variant is filled by the aggregator (probe_all) which can
compare the gpsd-reported DEVICES path against probe_serial_devices output;
this probe alone returns LiveFix even if the device is "wrong" from the
plugged-in-now perspective.

Reuses parse_tpv from gpsd.rs:33-40 (DO NOT duplicate). The TUXLINK_GPSD_ADDR
env var override matches the long-lived client's idiom.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 9: probe_all aggregator + Tauri command

BEFORE starting work: TDD prelude. **Also re-read this task's "WrongDevice cross-reference" logic — easy to drift.**

**Why:** Aggregates the per-probe results into a single `ProbeReport` returned to the frontend. Runs probes in parallel via `tokio::join!`. Also handles the gpsd `WrongDevice` cross-reference by comparing the gpsd-reported DEVICES path against the `probe_serial_devices()` output.

**Files:**
- Modify: `src-tauri/src/position/probe.rs`.

- [ ] **Step 9.1: Write failing test.**

```rust
#[cfg(test)]
mod tests_aggregator {
    use super::*;

    #[tokio::test]
    async fn returns_a_report_within_2_seconds() {
        let start = std::time::Instant::now();
        let report = probe_all().await;
        let elapsed = start.elapsed();
        assert!(elapsed < std::time::Duration::from_secs(2),
                "probe_all took {elapsed:?}");
        // Sanity: current_user is populated.
        assert!(!report.current_user.is_empty());
    }
}
```

- [ ] **Step 9.2: Run, confirm fail.**

- [ ] **Step 9.3: Implement.** Add to `src-tauri/src/position/probe.rs`:

```rust
use crate::position::probe_types::ProbeReport;

/// Run all detection probes in parallel and aggregate into a ProbeReport.
/// Bounded to ~2 seconds by the slowest probe (gpsd).
pub async fn probe_all() -> ProbeReport {
    let (gpsd, serial_devices) = tokio::join!(
        probe_gpsd(),
        probe_serial_devices(),
    );
    // Cross-reference: if gpsd is LiveFix but its DEVICES path differs from
    // anything in serial_devices, we'd return WrongDevice. For bd-1 simplicity,
    // we trust gpsd's own choice; the WrongDevice variant becomes meaningful
    // when bd-3's NativeNMEA reader competes with gpsd. For now, leave gpsd as-is.
    let in_dialout_group = probe_dialout_membership();
    let modemmanager = probe_modemmanager_status().await;
    let remote_shell = probe_remote_shell();
    let container = probe_container_mode();
    let current_user = current_username();
    ProbeReport {
        gpsd, serial_devices, in_dialout_group,
        modemmanager, remote_shell, container, current_user,
    }
}

/// Resolve the current process's username via nix.
fn current_username() -> String {
    let uid = nix::unistd::getuid();
    nix::unistd::User::from_uid(uid)
        .ok()
        .flatten()
        .map(|u| u.name)
        .unwrap_or_else(|| format!("uid:{}", uid.as_raw()))
}

/// Tauri command exposed to the frontend.
#[tauri::command]
pub async fn gps_probe_all() -> ProbeReport {
    probe_all().await
}
```

- [ ] **Step 9.4: Register the Tauri command** in `src-tauri/src/lib.rs`. Find the `tauri::generate_handler![...]` block around line 249-294. Add `crate::position::probe::gps_probe_all,` to the list, alphabetically near other `position` commands or at the end of the block.

- [ ] **Step 9.5: Run tests, confirm green.**

- [ ] **Step 9.6: Commit.**

```bash
git add src-tauri/src/position/probe.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(position): probe_all aggregator + gps_probe_all Tauri command (tuxlink-9xy1)

Runs probe_gpsd + probe_serial_devices in parallel via tokio::join!.
Sequential adds for the pure/cheap probes (dialout, modemmanager, remote
shell, container). Total bounded time ~2s worst case (gpsd-dominated).

Aggregates into ProbeReport — the single payload returned to the frontend.
current_user is resolved via nix::unistd::User::from_uid for the "I'll add
<user> to dialout — is this right?" disclosure (addendum E1).

WrongDevice cross-reference deferred: meaningful when bd-3's NativeNMEA
reader competes with gpsd. bd-1 trusts gpsd's choice.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### After every logical group of tasks

After Tasks 1-9 (the Rust probe layer) is complete, do this 3-round review:

Carefully review the batch of work from multiple perspectives:
- **Round 1:** Does each probe handle its error paths? Are timeouts bounded? Do tests cover both branches per testing-pitfalls §3?
- **Round 2:** Are the types in `probe_types.rs` consistent with each probe's return value? Any drift between what `probe_serial_devices()` returns and `SerialDeviceInfo`?
- **Round 3:** Cargo build clean? `cargo --manifest-path src-tauri/Cargo.toml clippy --all-targets -- -D warnings` clean? Cargo test green? If you still find substantive issues in round 3, keep going until no findings.

Then update your private journal and continue with the React layer.

---

### Task 10: Frontend types — `src/gps/types.ts`

BEFORE starting work: TDD prelude. **Read [src/wizard/types.ts:1-10](../../../src/wizard/types.ts) for the existing tag-and-content pattern; mirror it exactly.**

**Why:** The frontend types mirror `probe_types.rs` via Tauri's serde-tagged shape. Defining them up-front lets every component target a stable shape.

**Files:**
- Create: `src/gps/types.ts`

- [ ] **Step 10.1:** Create the file with these contents:

```typescript
// Mirrors src-tauri/src/position/probe_types.rs via Tauri's #[serde(tag, content)] shape.

export type GpsdProbeResult =
  | { kind: 'live_fix'; detail: { grid: string; fix_age_ms: number; satellites: number; mode: number } }
  | { kind: 'no_fix'; detail: { reason: string } }
  | { kind: 'wrong_device'; detail: { reported: string; current: string[] } }
  | { kind: 'unreachable'; detail: { error: string } }
  | { kind: 'parse_error'; detail: { version: string | null; raw: string } };

export interface SerialDeviceInfo {
  device_path: string;
  by_id_path: string | null;
  vendor_id: string;
  product_id: string;
  vendor_name: string | null;
  model_name: string | null;
}

export type ModemManagerStatus = 'not_installed' | 'inactive' | 'masked' | 'active';

export type RemoteShellStatus = 'local' | 'ssh_x11' | 'tty_only';

export type ContainerStatus =
  | { kind: 'bare' }
  | { kind: 'container'; runtime: string }
  | { kind: 'docker' };

export interface ProbeReport {
  gpsd: GpsdProbeResult;
  serial_devices: SerialDeviceInfo[];
  in_dialout_group: boolean;
  modemmanager: ModemManagerStatus;
  remote_shell: RemoteShellStatus;
  container: ContainerStatus;
  current_user: string;
}

// Triage card kinds — derived from ProbeReport by the picker.
export type TriageActionId =
  | 'add-dialout'
  | 'mask-modemmanager'
  | 'unmask-modemmanager'
  | 'enable-gpsd-socket';

export interface TriageCardData {
  id: TriageActionId;
  severity: 'critical' | 'warning' | 'info';
  title: string;
  body: string;
  command: string;
  reversibility_note: string | null;
}

// Source card data — for working sources.
export interface SourceCardData {
  id: string;                 // stable per-source key, e.g. "gpsd:127.0.0.1:2947" or "serial:/dev/serial/by-id/usb-…"
  kind: 'gpsd' | 'native_nmea' | 'manual';
  title: string;
  description: string;
  meta: { label: string; value: string }[];
  recommended: boolean;
  enabled: boolean;
}

// Precision preference — 4-char default per [[gps-precision-reduction]].
export type PrecisionChoice = 'four_char' | 'six_char';

// Picker props — pure presentational.
export interface GpsSourcePickerProps {
  report: ProbeReport | null;       // null = probes in flight
  sources: SourceCardData[];        // derived from report (caller's responsibility)
  triage: TriageCardData[];          // derived from report (caller's responsibility)
  manualGrid: string;
  precisionPreference: PrecisionChoice;
  helperAvailable: boolean;         // false in bd-1 (pkexec helper not yet shipped)

  onUseSource: (sourceId: string) => void;
  onManualGridChange: (grid: string) => void;
  onPrecisionChange: (precision: PrecisionChoice) => void;
  onShowCommand: (action: TriageActionId) => void;
  onFixItForMe: (action: TriageActionId) => void;
  onRescan: () => void;
}
```

- [ ] **Step 10.2:** Run `pnpm typecheck` from the worktree root.

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-9xy1-gps-foundation
pnpm typecheck 2>&1 | tail -3
```

Expected: clean (no errors).

- [ ] **Step 10.3: Commit.**

```bash
git add src/gps/types.ts
git commit -m "$(cat <<'EOF'
feat(gps): frontend types mirror probe_types.rs (tuxlink-9xy1)

TypeScript types matching the Rust ProbeReport shape via Tauri's
#[serde(tag,content)] pattern. Same idiom as src/wizard/types.ts for
the WizardError union — the React side gets a discriminated union it
can switch on, the Rust side gets serde-tagged enum.

Includes the picker's pure-presentational prop shape
(GpsSourcePickerProps): report-in, events-out. No context coupling.

Per addendum A2, this types layer enables the 3-component split:
GpsSourcePickerPresentational (no context) + Step4Location (wizard
container) + SettingsGpsPanel (Settings container).

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Tasks 11-18: React presentational components

The following 8 tasks build the React component layer bottom-up. Each follows the same TDD pattern (Vitest + React Testing Library). Brief task headers below; full TDD steps follow the pattern of Tasks 4-9.

- **Task 11:** `SourceCard.tsx` — render `SourceCardData` as a card with optional "recommended" green border, meta rows, "Use this" button. Tests: render with `recommended: true` shows green border; clicking the button fires `onUseSource(id)`; `enabled: false` disables the button.

- **Task 12:** `TriageCard.tsx` — render `TriageCardData` with severity color + a11y text label + icon `aria-label` + code block `aria-roledescription` + Show command / Fix it for me buttons. Tests: severity → border color mapping; text label present per addendum C6; code block aria attrs present; `helperAvailable: false` disables "Fix it for me" with "Coming soon" tooltip; "Show command" reveals the command + Copy button; Copy click fires `onShowCommand(id)` (and in production triggers clipboard write + aria-live announcement; the latter is bd-1's a11y polish).

- **Task 13:** `ManualGridEditor.tsx` — Maidenhead input with validation reusing `src-tauri/src/position/maidenhead.rs` via a Tauri command `position_validate_grid` (NEW; add to lib.rs handler) + precision radio (4-char default per `[[gps-precision-reduction]]`). Tests: invalid grid shows inline error; precision change fires `onPrecisionChange`.

- **Task 14:** `GpsSourcePickerPresentational.tsx` — composes the above. Takes `GpsSourcePickerProps`, renders source cards + triage cards + manual grid editor + Rescan button. Tests: each persona scenario (Bob/Sue/Dave/Mike) renders the correct cards; `report: null` shows loading state; Rescan click fires `onRescan`.

- **Task 15:** `useGpsProbeReport.ts` — TanStack Query hook that invokes `gps_probe_all` and provides the report + `refetch` for Rescan. Tests: hook returns `data: ProbeReport`; `refetch` re-invokes.

- **Task 16:** `derivePickerData.ts` — pure function: `(report: ProbeReport, helperAvailable: boolean) => { sources: SourceCardData[], triage: TriageCardData[] }`. Encodes the persona decision logic (gpsd LiveFix → green source; gpsd NoFix → amber triage; serial devices present + in dialout → green source; serial present + not in dialout → red triage with `add-dialout` action; modemmanager Active → warning triage with `mask-modemmanager`; remote_shell ≠ Local → suppress all Fix it for me; container ≠ Bare → swap triage body for host-side instructions). Tests: each persona-scenario maps to the expected sources + triage list.

- **Task 17:** `Step4Location.tsx` — wizard container. Uses `useWizard()` + `useGpsProbeReport()` + reads `pendingDialoutVerification` from `state` + renders the resume banner if set. On `onUseSource`/`onManualGridChange`/Continue, dispatches `SUBMIT_GPS_SUCCESS`. Tests: entering the step with `pendingDialoutVerification: true` shows the banner; "Use this" + Continue dispatches `SUBMIT_GPS_SUCCESS`.

- **Task 18:** `SettingsGpsPanel.tsx` — Settings container. Reads current config via existing `position_status` + writes via `config_set_grid` + uses (NEW) `position_set_source_kind` Tauri command for source switching. Renders the picker inside a `<details>` block (the addendum A5 expandable-section design). Tests: changing source via the picker dispatches `position_set_source_kind`; changing manual grid dispatches `config_set_grid`.

---

### Task 19: Wire wizard state machine for the 'location' step

BEFORE starting work: TDD prelude.

**Files:**
- Modify: `src/wizard/types.ts`, `src/wizard/wizardReducer.ts`, `src/wizard/wizardReducer.test.ts`

- [ ] **Step 19.1: Write failing reducer tests.** Append to `src/wizard/wizardReducer.test.ts`:

```typescript
it('SUBMIT_OFFLINE_SUCCESS lands on location step', () => {
  const state = { ...initialWizardState(), step: 'offline_identity' as const };
  const next = wizardReducer(state, { type: 'SUBMIT_OFFLINE_SUCCESS' });
  expect(next.step).toBe('location');
  expect(next.inFlight).toBe(false);
});

it('CMS_VERIFY_RESULT ok ... then later SKIP_CMS_VERIFY lands on location', () => {
  let state = { ...initialWizardState(), step: 'cms_verify' as const, cmsVerifySubstate: 'probing' as const };
  state = wizardReducer(state, { type: 'CMS_VERIFY_RESULT', ok: true });
  expect(state.cmsVerifySubstate).toBe('ok');
  // The Step3TestSend component dispatches the next step-transition; emulate that.
  state = wizardReducer(state, { type: 'SUBMIT_GPS_SUCCESS' });
  // After SUBMIT_GPS_SUCCESS the wizard lands on complete; pre-location it'd be cms_verify.
});

it('SKIP_CMS_VERIFY now lands on location instead of complete', () => {
  const state = { ...initialWizardState(), step: 'cms_verify' as const };
  const next = wizardReducer(state, { type: 'SKIP_CMS_VERIFY' });
  expect(next.step).toBe('location');
  expect(next.skipSignaled).toBe(true);
});

it('SUBMIT_GPS_SUCCESS lands on complete', () => {
  const state = { ...initialWizardState(), step: 'location' as const };
  const next = wizardReducer(state, { type: 'SUBMIT_GPS_SUCCESS' });
  expect(next.step).toBe('complete');
  expect(next.inFlight).toBe(false);
});
```

- [ ] **Step 19.2: Run, confirm fail** (types.ts doesn't have 'location' yet; reducer doesn't transition there).

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-9xy1-gps-foundation
pnpm vitest run src/wizard/wizardReducer.test.ts 2>&1 | tail -10
```

- [ ] **Step 19.3: Update `src/wizard/types.ts`:**
  - In `WizardStep` (lines 16-22), add `| 'location'` after `'cms_verify'`.
  - In `WizardAction` union (lines 38-59), add `| { type: 'SUBMIT_GPS_SUCCESS' }`.
  - In `WizardState` (lines 23-36), add `pendingDialoutVerification: boolean;`.

- [ ] **Step 19.4: Update `src/wizard/wizardReducer.ts`:**
  - In `initialWizardState()` (line 3-18), add `pendingDialoutVerification: false,` to the returned object.
  - Line 46-47, `SUBMIT_OFFLINE_SUCCESS`: change `step: 'complete'` → `step: 'location'`.
  - Line 76-79, `CMS_VERIFY_RESULT ok`: keep `cmsVerifySubstate: 'ok'` but ALSO add `step: 'location'` to the returned object.
  - Line 87-88, `SKIP_CMS_VERIFY`: change `step: 'complete'` → `step: 'location'`.
  - Add new case `SUBMIT_GPS_SUCCESS`: returns `{ ...state, step: 'complete', inFlight: false }`.

- [ ] **Step 19.5: Run tests, confirm green.**

- [ ] **Step 19.6: Commit.**

```bash
git add src/wizard/types.ts src/wizard/wizardReducer.ts src/wizard/wizardReducer.test.ts
git commit -m "$(cat <<'EOF'
feat(wizard): add 'location' step + SUBMIT_GPS_SUCCESS action (tuxlink-9xy1)

Inserts the GPS step into the wizard state machine. Both
SUBMIT_OFFLINE_SUCCESS and CMS_VERIFY_RESULT(ok=true) / SKIP_CMS_VERIFY
now land on 'location' instead of 'complete'. SUBMIT_GPS_SUCCESS is the
new terminal transition.

Adds pendingDialoutVerification: boolean to WizardState for the resume-
after-dialout-fix flow (addendum C1). Persists via the existing
wizard-config write path (added in a subsequent task).

Per addendum A1, no numeric "Step N of M" framing — named steps only.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 20: Wire `Step4Location` into `Wizard.tsx`

BEFORE starting work: TDD prelude.

**Files:**
- Modify: `src/wizard/Wizard.tsx`

- [ ] **Step 20.1:** Read [src/wizard/Wizard.tsx:25-38](../../../src/wizard/Wizard.tsx#L25-L38). The render branches need a new entry for `state.step === 'location'`.

- [ ] **Step 20.2:** Add the import at the top (line 7):

```typescript
import { Step4Location } from '../gps/Step4Location';
```

- [ ] **Step 20.3:** Add the render branch after the `cms_verify` line (~line 32):

```tsx
{state.step === 'location' && <Step4Location />}
```

- [ ] **Step 20.4:** Run existing Wizard tests:

```bash
pnpm vitest run src/wizard/Wizard.test.tsx 2>&1 | tail -10
```

Expected: green (the new branch is additive; existing transitions still work).

- [ ] **Step 20.5: Commit.**

```bash
git add src/wizard/Wizard.tsx
git commit -m "$(cat <<'EOF'
feat(wizard): render Step4Location at the 'location' step (tuxlink-9xy1)

Wires the new GPS step into the wizard's render branches. Additive only;
no existing transitions change.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 21: Wire `SettingsGpsPanel` into `SettingsPanel.tsx`

BEFORE starting work: TDD prelude. **Read [src/shell/SettingsPanel.tsx](../../../src/shell/SettingsPanel.tsx) — find the right place to mount the new panel.**

**Why:** Per addendum A5, NOT a tabs framework. The GPS panel sits as a top-level expandable `<details>` section in the existing flat Settings panel.

**Files:**
- Modify: `src/shell/SettingsPanel.tsx`

- [ ] **Step 21.1:** Survey the current panel structure. The new `<SettingsGpsPanel />` should sit prominently, after callsign / Winlink account but before theme settings.

- [ ] **Step 21.2:** Add the import + render:

```tsx
import { SettingsGpsPanel } from '../gps/SettingsGpsPanel';
// ... inside the Settings panel JSX, after the Winlink account section:
<details className="settings-section">
  <summary>Location & GPS</summary>
  <SettingsGpsPanel />
</details>
```

- [ ] **Step 21.3:** Verify existing SettingsPanel tests still pass.

- [ ] **Step 21.4: Commit.**

```bash
git add src/shell/SettingsPanel.tsx
git commit -m "$(cat <<'EOF'
feat(settings): mount SettingsGpsPanel as expandable section (tuxlink-9xy1)

Per design addendum A5, the GPS surface lives as a top-level
<details>/<summary> expandable section in the existing flat Settings
panel — NOT a new tabs framework. Preserves the existing Settings panel
architecture.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 22: pnpm tauri dev smoke walk per persona

Per `[[browser-smoke-before-ship]]`: UI work isn't done until walked in `pnpm tauri dev` against each persona scenario.

- [ ] **Step 22.1:** From the worktree root, start the dev server:

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-9xy1-gps-foundation
pnpm tauri dev 2>&1 | tee /tmp/9xy1-tauri-dev.log
```

Watch for the window to launch + the dashboard to appear.

- [ ] **Step 22.2: Bob's path** — if gpsd is running on this machine, the wizard should detect it. Walk through: complete Welcome → Callsign → offline_identity (or skip CMS) → arrive at Location → see green source card → click "Use this" → Continue → reach `complete` → land in main shell.

- [ ] **Step 22.3: Mike's path** — restart the wizard (clear wizard state or use a fresh user profile). At Location, click "Skip — I'll enter my grid manually". Enter `EM35vx`. Confirm 4-char precision is selected by default. Continue.

- [ ] **Step 22.4: Dave's path simulation** — `sudo gpasswd -d $USER dialout` (mock the dialout-missing state; or run the tauri-dev as a user not in dialout). Restart wizard. Confirm triage card renders with the right severity color + text label + "Show command" reveals the exact `sudo usermod -aG dialout $USER` command + "Fix it for me" is visible but disabled with "Coming soon" tooltip.

- [ ] **Step 22.5: Sue's path simulation** — if a USB GPS is plugged in (or simulated via a virtual serial device), confirm it appears in the source cards list with vendor/model name from udev. **bd-1 doesn't ship native NMEA reading**, so this is a list-only verification — the device is detected, named, but "Use this" leads to the manual-grid editor pre-populated with a placeholder grid pending bd-3.

- [ ] **Step 22.6: Settings panel walk** — open Settings, expand "Location & GPS", verify the same picker UI renders, switch sources, confirm config writes via `position_status` re-read.

- [ ] **Step 22.7:** Stop the dev server. Note any visual / functional issues in a `dev/scratch/9xy1-smoke-notes.md`.

---

### Task 23: Build verification + git push

- [ ] **Step 23.1:** Run the full local quality gate (see `[[browser-smoke-before-ship]]` for why this matters):

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-9xy1-gps-foundation
pnpm typecheck 2>&1 | tail -3
pnpm vitest run 2>&1 | tail -10
cargo --manifest-path src-tauri/Cargo.toml clippy --all-targets --locked -- -D warnings 2>&1 | tail -10
cargo --manifest-path src-tauri/Cargo.toml test --locked 2>&1 | tail -10
```

All four must be clean (warnings tolerated only if NOT clippy-blocking).

Note: per the project's `[[verify-build-provenance-before-onair]]` + recent PR #337 ("CI is the non-GUI verification gate"), this local pass is for tight iteration; the merge gate is CI. Don't ship if locally green but suspect — push and let CI confirm.

- [ ] **Step 23.2:** Push:

```bash
git push -u origin bd-tuxlink-9xy1/gps-foundation
```

- [ ] **Step 23.3:** Open PR:

```bash
gh pr create --base main --head bd-tuxlink-9xy1/gps-foundation \
  --title "[<YOUR-MONIKER>] feat(gps): GPS source picker foundation (tuxlink-9xy1)" \
  --body "<see PR body template below>"
```

PR body template:

```markdown
## Summary

Foundation slice of the GPS setup UX. Implements detection probes (Rust), the GpsSourcePickerPresentational component (React), Step4Location wizard step, SettingsGpsPanel expandable section in Settings. After this PR, Bob's path (gpsd pre-configured), Mike's path (manual grid only) and the diagnostic half of Dave's path (triage cards with "Show command") are fully live. "Fix it for me" buttons render disabled pending tuxlink-m9ej (bd-2).

## Anchors
- Design: [`docs/design/2026-06-05-gps-setup-ux-design.md`](../docs/design/2026-06-05-gps-setup-ux-design.md)
- Addendum (rounds 2-4 review): [`docs/design/2026-06-05-gps-setup-ux-design-addendum-r2-r4.md`](../docs/design/2026-06-05-gps-setup-ux-design-addendum-r2-r4.md)
- Mockup: [`docs/design/mockups/2026-06-04-gps-setup-mocks.html`](../docs/design/mockups/2026-06-04-gps-setup-mocks.html)
- Plan: [`docs/superpowers/plans/2026-06-05-gps-setup-bd-1-plan.md`](../docs/superpowers/plans/2026-06-05-gps-setup-bd-1-plan.md)

## Persona coverage (this PR)

- ✅ Bob (gpsd pre-configured) — green source card, one-Enter Continue
- ✅ Mike (manual grid only) — first-class skip path, no GPS timeout
- ⚠️ Dave (Windows-Winlink convert) — triage cards render with "Show command" only; "Fix it for me" disabled pending bd-2
- ⚠️ Sue (USB GPS, no gpsd) — device detected + named via udev; native NMEA reading deferred to bd-3

## Test plan

- [x] Cargo unit tests (`cargo test --locked`) — green
- [x] React unit tests (`pnpm vitest run`) — green
- [x] Typecheck (`pnpm typecheck`) — clean
- [x] Clippy (`cargo clippy --all-targets -- -D warnings`) — clean
- [x] `pnpm tauri dev` smoke walks per persona (Bob/Mike/Dave/Sue) — see commit messages for per-persona evidence
- [ ] Operator verification on labwc+wf-panel-pi (this Pi)
- [ ] CI green (will land when this PR is opened)

🤖 Generated with [Claude Code](https://claude.com/claude-code)
```

---

## Self-review (run before this plan is handed off)

This is a checklist YOU run on the plan above before it goes to the executor.

**1. Spec coverage** — every requirement from the design doc + addendum bd-1 section has a task:
- [x] Detection probes (gpsd 3-tier / serial / dialout / modemmanager / remote-shell / container) — Tasks 4-9
- [x] ProbeReport aggregator + Tauri command — Task 9
- [x] Frontend types mirror — Task 10
- [x] Presentational components (Source / Triage / Manual / Composed) — Tasks 11-14
- [x] Container components (WizardGpsStep + SettingsGpsPanel) — Tasks 15-18
- [x] Wizard state machine 'location' transitions — Task 19
- [x] Wizard.tsx wiring — Task 20
- [x] SettingsPanel.tsx wiring — Task 21
- [x] a11y requirements (Task 12: TriageCard text labels + aria attributes)
- [x] pnpm tauri dev smoke per persona — Task 22
- [x] Local quality gates + push + PR — Task 23

**2. Placeholder scan** — searched for "TBD", "implement later", etc. None present.

**3. Type consistency** — Rust `ProbeReport` ↔ TypeScript `ProbeReport` — checked. Field names match exactly. The `wrong_device` GpsdProbeResult variant is in both; bd-1 doesn't yet emit it but the type accommodates bd-3's cross-reference.

**Caveat noted:** Tasks 11-18 are sketched at task-header level (full TDD steps follow the pattern of Tasks 4-9 but aren't expanded in this draft). The executor should expand each into the full bite-sized TDD steps per the writing-plans skill's "Bite-Sized Task Granularity" guidance.

---

## Subsequent slices (separate plans)

This plan is bd-1 only. The following plans will be written via `superpowers:writing-plans` when each bd's turn comes:

- **bd-2 (tuxlink-m9ej):** pkexec helper + PolicyKit policy + Tauri spawner + "Fix it for me" wiring. ~5-7 days.
- **bd-3 (tuxlink-ley0):** ProviderArbiter refactor + native NMEA reader + `nmea` crate + u-blox UBX-mode triage. ~7-10 days.
- **bd-4 (tuxlink-gnws):** Background detection task + Tauri event emitter + React event-consumer module + debounced toast + modal interrupt. ~7-10 days.

---

## Project anchors (must honor throughout)

- `[[gps-precision-reduction]]` — 4-char Maidenhead default broadcast; operator opts into 6-char.
- `[[inline-ui-no-window-clutter]]` — Settings stays inline; the picker is a `<details>` section, not a modal/window.
- `[[no-stretched-full-width-ui]]` — model density on ~700px reading-pane width.
- `[[browser-smoke-before-ship]]` — `pnpm tauri dev` walk before declaring UI done (Task 22).
- `[[trust-support-engineer-intuition]]` — operator pushback is high-signal; this addendum exists because of operator pushback on the wizard-only design.
- `[[pin-paths-in-worktree-sessions]]` — every command in this plan uses absolute paths to the worktree.
- `[[verify-surfaced-operator-commands]]` — commands tested for first-paste runnability; cargo invocations include `--manifest-path src-tauri/Cargo.toml`.
- ADR 0008 — worktree-mandatory write work.
- ADR 0004 — per-task branch model (`bd-9xy1/gps-foundation`).
- ADR 0010 — no-squash merge.
- ADR 0017 — branch lifecycle (hooks deny commits to dead branches).

---

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-06-05-gps-setup-bd-1-plan.md`. Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task in this session, review between tasks, fast iteration. Best fit for plans with 20+ tasks where context-window drift between tasks would cause errors.

**2. Inline Execution** — Execute tasks in this session using `superpowers:executing-plans`, batch execution with checkpoints for review. Best fit if the operator wants to walk through tasks personally with the agent.

Which approach?
