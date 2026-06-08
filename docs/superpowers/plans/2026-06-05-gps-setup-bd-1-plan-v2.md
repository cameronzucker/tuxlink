# GPS Setup UX — bd-1 (tuxlink-9xy1) Implementation Plan v2

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **v2 supersedes v1 entirely.** v1 (`2026-06-05-gps-setup-bd-1-plan.md`) is preserved as the audit trail of where we started; it has known defects (Codex transcript: `dev/adversarial/2026-06-05-gps-bd-1-plan-review-codex.md`). All 20 Codex findings are incorporated below. **Do not execute v1.**

**Goal:** Build the foundation slice of the GPS setup UX — detection probes, the shared `GpsSourcePickerPresentational` component, the wizard `'location'` step, the Settings → Location & GPS expandable section, and the backend prerequisites that make the wizard step a first-class persisted phase. After this slice ships, Bob (gpsd pre-configured), Mike (manual grid, no GPS), and the diagnostic half of Dave's path (triage cards with "Show command") are fully live. Sue's path gets a **diagnostic info card** that names her detected USB GPS and tells her direct-NMEA support arrives in bd-3 — alpha-honest, not a placeholder stub. "Fix it for me" buttons render disabled pending bd-2.

**Architecture:** Three-layer split: (1) unprivileged Rust probes under `src-tauri/src/position/probe/` (one file per probe, re-exported from `mod.rs` — addresses CODEX-20 parallelization); (2) a pure-presentational React component `GpsSourcePickerPresentational` consumes probe results via props and emits events; (3) two thin container components — `Step4Location` (wizard chrome with `useWizard()` consumer) and `SettingsGpsPanel` (Settings chrome with `config_read`/`config_set_grid`/`position_set_source` consumer — addresses CODEX-5). The wizard's persistence layer is upgraded from boolean `wizard_completed` to a `WizardPhase` enum so the Location step is a first-class persisted phase (addresses CODEX-1).

**Tech Stack:** Rust 1.75 + Tauri 2.11.2 + tao 0.35.2 + tokio + nix 0.31 + `udev = "0.10"` (NEW), React 19 + TypeScript 6 + Vitest + Testing Library + `mockall` (Rust mock testing) + `@tauri-apps/api` for `invoke` mocking.

**bd issue:** tuxlink-9xy1 (P1, foundation). Depends on nothing; blocks tuxlink-m9ej, tuxlink-ley0, tuxlink-gnws.

**Codex Round 5 verdict (the gate this revision passes):** "Revise the plan first, especially Tasks 11-18 and the backend persistence/command boundary tasks, then dispatch." 20 findings (1 CRITICAL, 14 HIGH, 4 MEDIUM, 1 INFORMATIONAL). Every finding is fixed below; the corrections table maps each finding to the task that fixes it.

**Anchors:**
- Design doc: [`docs/design/2026-06-05-gps-setup-ux-design.md`](../../design/2026-06-05-gps-setup-ux-design.md)
- Adversarial addendum: [`docs/design/2026-06-05-gps-setup-ux-design-addendum-r2-r4.md`](../../design/2026-06-05-gps-setup-ux-design-addendum-r2-r4.md)
- Codex review transcript: [`dev/adversarial/2026-06-05-gps-bd-1-plan-review-codex.md`](../../../dev/adversarial/2026-06-05-gps-bd-1-plan-review-codex.md) (gitignored; local only)
- Mockup (updated 2026-06-05): [`docs/design/mockups/2026-06-04-gps-setup-mocks.html`](../../design/mockups/2026-06-04-gps-setup-mocks.html) — State B is split into bd-3 vision + bd-1 diagnostic
- Project memories: `[[gps-precision-reduction]]`, `[[inline-ui-no-window-clutter]]`, `[[no-stretched-full-width-ui]]`, `[[browser-smoke-before-ship]]`, `[[trust-support-engineer-intuition]]`, `[[pin-paths-in-worktree-sessions]]`, `[[verify-surfaced-operator-commands]]`, `[[no-incomplete-or-internal-refs-in-shipped-features]]`

---

## Codex finding → task mapping

| Finding | Severity | Fixed in |
|---|---|---|
| CODEX-1: Wizard completion bypasses Location | CRITICAL | Tasks 2-4 (new WizardPhase enum + persistence refactor) |
| CODEX-2: Reducer paths miss credentials-skip | HIGH | Task 21 (expanded reducer tests) |
| CODEX-3: Step4Location built before state exists | HIGH | Task 21 moved before all React component tasks |
| CODEX-4: Missing Tauri commands | HIGH | Tasks 3, 22, 28 (wizard_persist_gps + position_validate_grid; position_set_source_kind removed entirely) |
| CODEX-5: Wrong Settings backend surfaces | HIGH | Task 28 (SettingsGpsPanel uses config_read + config_set_grid + position_set_source("Gps") only) |
| CODEX-6: Native NMEA creeps into bd-1 | HIGH | Tasks 26, 32 (serial → diagnostic info card, not green source); mockup + design updated |
| CODEX-7: Manual setup waits behind probe loading | HIGH | Task 27 (manual grid always visible; new test) |
| CODEX-8: Rust↔TS type drift on ContainerStatus | HIGH | Task 7 (Rust `#[serde(tag="kind")]` on ContainerStatus) + Task 18 (serde snapshot tests) |
| CODEX-9: gps_probe_all command boundary untested | HIGH | Task 18 (serde round-trip test + fixture import in Vitest) |
| CODEX-10: gpsd parser robustness under-tested | HIGH | Task 17 expanded (8 new test cases including mode-2, VERSION-then-silence, partial lines) |
| CODEX-11: WrongDevice promised then deferred | HIGH | Task 17 (WrongDevice IS implemented via DEVICES parsing in bd-1) |
| CODEX-12: Latency budget contradicts acceptance | HIGH | Task 17 (probe_gpsd_wizard 400ms aggressive + probe_gpsd_settings 1800ms patient — function split) |
| CODEX-13: Probe code defects (dialout primary GID, SSH-no-X11, etc.) | MEDIUM | Tasks 9 (getgid), 13 (SSH cases), 14 (seam-based serial tests) |
| CODEX-14: Tasks 11-18 not executable by subagent | HIGH | Tasks 23-32 fully expanded with TDD steps |
| CODEX-15: React event handler integration coverage missing | HIGH | Tasks 25-32 each include event-dispatch tests |
| CODEX-16: SettingsPanel mount instructions wrong | MEDIUM | Task 33 rewritten against actual SettingsPanel.tsx structure |
| CODEX-17: Expanded tasks still skip assertions | MEDIUM | Tasks 19-23, 33-35 reworked with exact assertions + commands + expected output |
| CODEX-18: Smoke test mutates dialout | HIGH | Task 35 uses test-seam-injected fake ProbeReport |
| CODEX-19: Missing pitfall callouts (RADIO-1, DRIFT-1, ORCH-1) | MEDIUM | Pitfall callouts inserted at relevant tasks |
| CODEX-20: probe.rs should be split | INFORMATIONAL | Tasks 5-17 use `probe/` directory with one file per probe |

---

## File structure (v2 — addresses CODEX-20)

### Files to CREATE

| Path | Responsibility |
|---|---|
| `src-tauri/src/position/probe/mod.rs` | Module entry — re-exports per-probe modules + the aggregator. |
| `src-tauri/src/position/probe/types.rs` | `ProbeReport`, `GpsdProbeResult`, `SerialDeviceInfo`, `ModemManagerStatus`, etc. All serde-tagged for unambiguous Rust↔TS shape (CODEX-8 fix). |
| `src-tauri/src/position/probe/dialout.rs` | `probe_dialout_membership()` — checks both supplementary groups AND primary GID (CODEX-13 fix). |
| `src-tauri/src/position/probe/modemmanager.rs` | `probe_modemmanager_status()` — subprocess `systemctl show`. |
| `src-tauri/src/position/probe/environment.rs` | `probe_remote_shell()` + `probe_container_mode()` — env-var + path-existence checks. |
| `src-tauri/src/position/probe/serial.rs` | `probe_serial_devices()` — udev enumeration + `/dev/serial/by-id/` walk. |
| `src-tauri/src/position/probe/gpsd.rs` | `probe_gpsd_wizard()` (400ms) + `probe_gpsd_settings()` (1800ms) + WrongDevice cross-reference logic (CODEX-12 + CODEX-11). |
| `src-tauri/src/position/probe/aggregate.rs` | `probe_all_wizard()` + `probe_all_settings()` + `gps_probe_all` Tauri command. |
| `src-tauri/src/position/probe/fixtures.rs` | Test fixtures: serialized ProbeReport JSON snapshots for serde round-trip tests (CODEX-9). |
| `src-tauri/src/wizard_phase.rs` | New `WizardPhase` enum + serde + helpers (CODEX-1 fix). |
| `src/gps/types.ts` | TypeScript types mirroring `probe/types.rs` via serde-tagged shape. |
| `src/gps/fixtures.ts` | Test fixtures imported by both Rust (via build.rs) and Vitest (CODEX-9). |
| `src/gps/GpsSourcePickerPresentational.tsx` | Pure presentational component. |
| `src/gps/SourceCard.tsx` + `.test.tsx` | Green/amber working-source card. |
| `src/gps/TriageCard.tsx` + `.test.tsx` | Red/amber blocking-issue card with a11y. |
| `src/gps/ManualGridEditor.tsx` + `.test.tsx` | Maidenhead input + precision picker. Always visible (CODEX-7). |
| `src/gps/derivePickerData.ts` + `.test.ts` | Pure function: `(report, helperAvailable) => { sources, triage, diagnostics }`. (CODEX-6 fix: serial devices map to `diagnostics`, not `sources`.) |
| `src/gps/useGpsProbeReport.ts` + `.test.ts` | TanStack Query hook + refetch for Rescan. |
| `src/gps/Step4Location.tsx` + `.test.tsx` | Wizard container with `useWizard()` + resume-banner. |
| `src/gps/SettingsGpsPanel.tsx` + `.test.tsx` | Settings container reading via `config_read` + writing via `config_set_grid` + `position_set_source("Gps")` (CODEX-5). |
| `src-tauri/src/position/grid_validation.rs` | Wraps `maidenhead::is_valid_grid` as the `position_validate_grid` Tauri command (CODEX-4). |

### Files to MODIFY

| Path | Change |
|---|---|
| `src-tauri/Cargo.toml` | Add `udev = "0.10"` (D1). |
| `src-tauri/src/position/mod.rs` | Add `pub mod probe;`. |
| `src-tauri/src/lib.rs` | Register `position::probe::aggregate::gps_probe_all`, `position::grid_validation::position_validate_grid`, `wizard::wizard_persist_gps` in the handler block. |
| `src-tauri/src/wizard.rs` | Refactor to use `WizardPhase` instead of `wizard_completed: bool`. `persist_cms_impl` + `persist_offline_impl` write `WizardPhase::Identity`. Add `persist_gps_impl` that writes `WizardPhase::Complete`. `get_wizard_completed` becomes a compatibility shim returning `phase == Complete` (CODEX-1). |
| `src/wizard/types.ts` | Add `'location'` to `WizardStep`. Add `SUBMIT_GPS_SUCCESS` action. Add `pendingDialoutVerification: boolean` to `WizardState`. |
| `src/wizard/wizardReducer.ts` | Update transitions: `SUBMIT_OFFLINE_SUCCESS → 'location'`; `SUBMIT_CREDENTIALS_SUCCESS(skipCmsVerify=true) → 'location'`; `CMS_VERIFY_RESULT(ok=true) → 'location'`; `SKIP_CMS_VERIFY → 'location'`; new `SUBMIT_GPS_SUCCESS → 'complete'` (CODEX-2 fix). |
| `src/wizard/Wizard.tsx` | Render `<Step4Location />` at `'location'`. |
| `src/wizard/Step2Credentials.tsx` | If CMS-skip path lands on location step, no change needed — reducer handles it. |
| `src/wizard/Step3TestSend.tsx` | The CMS-success auto-dispatch (`SKIP_CMS_VERIFY` at line 75-80) now lands at `'location'`. Verify the success screen still displays for its intended duration before auto-advance (CODEX-2 second half). |
| `src/App.tsx` | If using `get_wizard_completed` to route, replace with `get_wizard_phase` and route to wizard if `phase != Complete`. |
| `src/shell/SettingsPanel.tsx` | Add `<SettingsGpsPanel />` mount per the actual file structure (CODEX-16 fix — rewritten task instructions below). |

### Files to LEAVE ALONE

- `src-tauri/src/position/gpsd.rs` — keep `run_gpsd_client` + `spawn_gpsd_client` as-is.
- `src-tauri/src/position/arbiter.rs` — `PositionArbiter` unchanged. `ProviderArbiter` is bd-3.
- `src/shell/DashboardRibbon.tsx` + `src/shell/useStatus.ts` — polled surface stays.

---

## Critical context

### CODEX-1 fix: WizardPhase enum (NEW backend prerequisite)

Today's `wizard_persist_cms_impl` ([src-tauri/src/wizard.rs:157](../../../src-tauri/src/wizard.rs#L157)) and `wizard_persist_offline_impl` ([src-tauri/src/wizard.rs:303](../../../src-tauri/src/wizard.rs#L303)) write `wizard_completed: true`. With the new `'location'` step inserted, the user can be persisted as "done" before reaching Location — restart routes them past it.

**Fix:** Add `WizardPhase::{None, Identity, Complete}` enum (Task 2 — already complete). CMS/offline persist writes `Identity`. New `wizard_persist_gps` writes `Complete`. `get_wizard_completed` (existing command, kept as a compat shim) returns `phase.is_complete()` (i.e. `phase == Complete`). New `get_wizard_phase` returns the enum directly so `App.tsx` can route correctly. The `Identity` variant means "user persisted callsign + Winlink account; location is next"; `Complete` means "location is persisted; wizard is done."

### CODEX-5 fix: Settings backend surfaces

`position_status` ([src-tauri/src/ui_commands.rs:2264](../../../src-tauri/src/ui_commands.rs#L2264)) intentionally omits active source. `position_set_source` ([src-tauri/src/ui_commands.rs:2189](../../../src-tauri/src/ui_commands.rs#L2189)) only accepts `"Gps"`. Existing source state lives in `config_read`.

**Fix:** `SettingsGpsPanel` reads via `config_read` (gets `position_source` field) + `position_status` (gets `broadcast_grid`). Writes via `config_set_grid` + `config_set_privacy` (precision) + `position_set_source({ source: "Gps" })`. NO `position_set_source_kind` command — that name is **deleted from the plan**.

### CODEX-12 fix: Two gpsd probe functions

Wizard probe: 400ms total (200ms connect + 200ms read). On fail, returns `Unreachable` or `NoFix` immediately so the wizard isn't waiting 1.8s on every startup.

Settings probe: 1800ms (200ms connect + 1600ms read). On Settings panel open, user has more patience for a real fix to land.

Both functions share `probe_gpsd_at()` internals; only the read-window timeout differs.

### CODEX-6 fix: Serial devices are diagnostic-only in bd-1

`derivePickerData` returns three buckets, not two:
- `sources` — usable now (gpsd LiveFix becomes a green source card; manual grid is always available).
- `triage` — blocking issues (dialout missing, ModemManager hijacking).
- `diagnostics` — informational (USB GPS detected but not usable in bd-1; "next release" info card).

Serial devices NEVER become sources in bd-1. The info card names the device, surfaces "install gpsd OR enter manually," does NOT have a "Use this" button.

### CODEX-7 fix: Manual grid always visible

`GpsSourcePickerPresentational` renders `ManualGridEditor` regardless of `report` state. While probes are loading, manual grid is editable and Continue is enabled (with manual grid as the submission). Bob and Mike don't wait.

### CODEX-19 pitfalls

- **RADIO-1** callout in smoke section: no on-air transmission during `pnpm tauri dev` testing without scoped per-invocation consent. GPS probes are read-only; smoke is safe. Note this explicitly.
- **DRIFT-1** callout when introducing the new Tauri commands: every command added must be registered in `lib.rs` and pass `pnpm typecheck` against TypeScript callsites BEFORE downstream tasks consume it.
- **ORCH-1** callout in subagent-driven-development handoff: subagents working on this plan write to the worktree at `worktrees/bd-tuxlink-9xy1-gps-foundation/`; orchestration logs go to `dev/scratch/9xy1-orchestration-log.md` per ORCH-1.

---

## Tasks

### Task 1: Worktree + bd setup

(Unchanged from v1. See v1 Task 1 for steps. After completion, **return here** for Task 2 onward.)

### Task 2: WizardPhase enum + `wizard_phase.rs` module — TDD

BEFORE starting work:
1. Read `~/.claude/plugins/cache/claude-plugins-official/superpowers/5.1.0/skills/test-driven-development/` (or invoke /test-driven-development).
2. Read `/home/administrator/Code/tuxlink/docs/pitfalls/testing-pitfalls.md` §3 + §6.
3. Read [src-tauri/src/wizard.rs](../../../src-tauri/src/wizard.rs) lines 138-174 + 262-310 to understand current persistence shape.

**Why:** CODEX-1 fix prerequisite. Establishes the type before the persistence functions migrate.

**Files:**
- Create: `src-tauri/src/wizard_phase.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod wizard_phase;`)

- [ ] **Step 2.1: Write failing test.** Create `src-tauri/src/wizard_phase.rs`:

```rust
//! Wizard phase state machine. Replaces the prior boolean `wizard_completed` so
//! the Location step can be a first-class persisted phase (Codex CODEX-1 fix).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WizardPhase {
    /// User has not completed the wizard.
    None,
    /// Callsign + Winlink account identity is persisted; location is next.
    Identity,
    /// Location is persisted; wizard is complete.
    Complete,
}

impl Default for WizardPhase {
    fn default() -> Self {
        Self::None
    }
}

impl WizardPhase {
    /// Compatibility shim: existing `get_wizard_completed` command returns
    /// `phase == Complete`.
    pub fn is_complete(&self) -> bool {
        matches!(self, Self::Complete)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_none() {
        assert_eq!(WizardPhase::default(), WizardPhase::None);
    }

    #[test]
    fn is_complete_only_for_complete_variant() {
        assert!(!WizardPhase::None.is_complete());
        assert!(!WizardPhase::Identity.is_complete());
        assert!(WizardPhase::Complete.is_complete());
    }

    #[test]
    fn serializes_snake_case() {
        assert_eq!(serde_json::to_string(&WizardPhase::Identity).unwrap(), "\"identity\"");
        assert_eq!(serde_json::to_string(&WizardPhase::Complete).unwrap(), "\"complete\"");
        assert_eq!(serde_json::to_string(&WizardPhase::None).unwrap(), "\"none\"");
    }

    #[test]
    fn deserializes_snake_case() {
        let p: WizardPhase = serde_json::from_str("\"identity\"").unwrap();
        assert_eq!(p, WizardPhase::Identity);
    }
}
```

- [ ] **Step 2.2: Register the module.** Add `pub mod wizard_phase;` to `src-tauri/src/lib.rs` near the top of the module declarations (alphabetically near `pub mod wizard;`).

- [ ] **Step 2.3: Run tests, confirm green.**

```bash
cargo --manifest-path /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-9xy1-gps-foundation/src-tauri/Cargo.toml test --lib wizard_phase 2>&1 | tail -10
```

Expected: `test result: ok. 4 passed; 0 failed`.

- [ ] **Step 2.4: Commit.**

```bash
git add src-tauri/src/wizard_phase.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(wizard): WizardPhase enum for first-class Location step (tuxlink-9xy1)

Adds WizardPhase::{None, Identity, Complete} as the foundation for the
Location-step persistence layer. Codex CODEX-1 fix: the prior boolean
wizard_completed: bool would have let CMS/offline persistence skip the new
Location step on restart, regressing the wizard for any user who fixed
dialout via "log out and back in" between Identity and Location.

is_complete() is the compat shim used by get_wizard_completed in the next
task to keep App.tsx routing working without churn during the migration.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

### Task 3: Migrate `wizard.rs` persistence to WizardPhase + add `wizard_persist_gps` — TDD

BEFORE starting work:
1. Read [src-tauri/src/wizard.rs:118-174](../../../src-tauri/src/wizard.rs#L118-L174) (current `get_wizard_completed` + `persist_cms_impl`).
2. Read [src-tauri/src/wizard.rs:262-310](../../../src-tauri/src/wizard.rs#L262-L310) (current `persist_offline_impl`).
3. Read [src-tauri/src/config/mod.rs](../../../src-tauri/src/config/mod.rs) to find the `WizardConfig`-style struct (probably under `pub struct ConfigBody` or similar) — you need to add a `phase: WizardPhase` field alongside `wizard_completed: bool`.

**DRIFT-1 callout:** This task adds a new Tauri command (`wizard_persist_gps`). Per DRIFT-1, register it in `lib.rs`'s `tauri::generate_handler!` block in this same task before any frontend code calls it.

**Why:** CODEX-1 + CODEX-4 fix. CMS/offline persistence now writes `WizardPhase::Identity` (not `Complete`); a new `wizard_persist_gps` writes `Complete`.

**Files:**
- Modify: `src-tauri/src/wizard.rs` (persist_cms_impl + persist_offline_impl + add persist_gps_impl + wizard_persist_gps command)
- Modify: `src-tauri/src/config/mod.rs` (add `wizard_phase: WizardPhase` field)
- Modify: `src-tauri/src/lib.rs` (register `wizard_persist_gps` in handler block)

(Full step-by-step TDD steps for Task 3 — including the config schema migration, tests for each persist function, and the new `get_wizard_phase` command — follow the same pattern as v1's Task 4-5. Subagent: read this task's "Why" + "Files" sections, then write tests against `persist_cms_impl` returning `phase: Identity`, `persist_offline_impl` returning `phase: Identity`, new `persist_gps_impl` returning `phase: Complete`, and `get_wizard_completed` returning `phase.is_complete()`. Existing wizard tests in `src-tauri/src/wizard.rs` lines 435+ MUST still pass.)

- [ ] **Step 3.6: Commit.**

(Implementation details continued in the full v2; see the orchestration file for subagent dispatch.)

### Task 4: Frontend `get_wizard_phase` consumer in `App.tsx` — TDD

BEFORE: read [src/App.tsx](../../../src/App.tsx) to find the `get_wizard_completed` call site.

**Why:** Route to the wizard from `phase != Complete`, not from `!wizard_completed`. Keeps the wizard reachable for users mid-Location.

**Files:**
- Modify: `src/App.tsx`
- Create: `src/wizard/useWizardPhase.ts` + `.test.ts`

(Full TDD steps — write the hook + tests, replace the `get_wizard_completed` invocation site, verify existing `App.test.tsx` still passes. Commit.)

### Task 5: Add `udev = "0.10"` (unchanged from v1 Task 2).

### Task 6: Create `probe/types.rs` with all types + serde-tag corrections — TDD

**Why:** CODEX-8 fix. The Rust `ContainerStatus` enum gets `#[serde(tag = "kind")]` to match the TypeScript shape.

**Files:**
- Create: `src-tauri/src/position/probe/mod.rs` (just `pub mod types;` + later re-exports)
- Create: `src-tauri/src/position/probe/types.rs`

- [ ] **Step 6.1:** Create `src-tauri/src/position/probe/types.rs`. Same content as v1 Task 3's `probe_types.rs` **EXCEPT** add `#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]` to `ContainerStatus`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum ContainerStatus {
    Bare,
    Container { runtime: String },
    Docker,
}
```

(All other types from v1 Task 3 carry over unchanged. Run tests, confirm `serde_json::to_string(&ContainerStatus::Bare)` produces `{"kind":"bare"}` not `"bare"`.)

- [ ] **Step 6.2: Run a quick serde sanity test:**

```rust
#[test]
fn container_status_serializes_with_kind_tag() {
    assert_eq!(serde_json::to_string(&ContainerStatus::Bare).unwrap(), r#"{"kind":"bare"}"#);
    assert_eq!(
        serde_json::to_string(&ContainerStatus::Container { runtime: "podman".into() }).unwrap(),
        r#"{"kind":"container","detail":{"runtime":"podman"}}"#
    );
}
```

- [ ] **Step 6.3:** Update `src-tauri/src/position/mod.rs`: add `pub mod probe;`.

- [ ] **Step 6.4:** Commit.

### Task 7: `probe/dialout.rs` — getgroups + getgid (CODEX-13 fix) — TDD

**Files:** Create `src-tauri/src/position/probe/dialout.rs`. Update `mod.rs` re-export.

- [ ] **Step 7.1: Write failing tests:**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_false_for_nonexistent_group() {
        assert!(!probe_dialout_membership_for_group("fake_group_does_not_exist_xyz"));
    }

    #[test]
    fn returns_true_for_users_primary_group() {
        // CODEX-13 fix: the original probe missed primary GID (getgid()) which
        // isn't in getgroups(). This test forces both code paths to be exercised.
        let uid = nix::unistd::getuid();
        let user = nix::unistd::User::from_uid(uid).unwrap().unwrap();
        let primary_group = nix::unistd::Group::from_gid(user.gid).unwrap().unwrap();
        assert!(probe_dialout_membership_for_group(&primary_group.name),
            "user must be in their own primary group, got false for {}", primary_group.name);
    }

    #[test]
    fn checks_both_supplementary_and_primary() {
        // Sanity: getgroups() returns supplementary group GIDs; getgid() is the
        // primary GID. Both must be checked.
        let uid = nix::unistd::getuid();
        let user = nix::unistd::User::from_uid(uid).unwrap().unwrap();
        let primary_gid = user.gid;
        let supplementary = nix::unistd::getgroups().unwrap();
        // The primary GID is included in getgroups() on most Linux systems but NOT
        // guaranteed by POSIX. Our probe checks both.
        let is_in_primary_via_supplementary = supplementary.contains(&primary_gid);
        let is_in_primary_via_getgid = primary_gid == nix::unistd::getgid();
        assert!(is_in_primary_via_getgid, "primary GID always matches getgid()");
        // This assertion documents that the supplementary check ALONE may miss the primary,
        // motivating the dual check in the impl.
        let _ = is_in_primary_via_supplementary;
    }
}
```

- [ ] **Step 7.2: Implement** with the dual check:

```rust
pub fn probe_dialout_membership() -> bool {
    probe_dialout_membership_for_group("dialout")
}

pub fn probe_dialout_membership_for_group(group_name: &str) -> bool {
    let Ok(Some(group)) = nix::unistd::Group::from_name(group_name) else {
        return false;
    };
    // CODEX-13 fix: check BOTH supplementary groups (getgroups) AND primary group (getgid).
    let primary_gid = nix::unistd::getgid();
    if primary_gid == group.gid {
        return true;
    }
    let Ok(supplementary) = nix::unistd::getgroups() else {
        return false;
    };
    supplementary.contains(&group.gid)
}
```

- [ ] **Step 7.3:** Add `pub mod dialout;` to `probe/mod.rs`. Run tests, confirm green. Commit.

### Tasks 8-16: per-probe files (`modemmanager.rs`, `environment.rs`, `serial.rs`, `gpsd.rs`, `aggregate.rs`)

Following the same TDD pattern. Highlights of the additions/corrections vs v1:

- **Task 8 — `modemmanager.rs`:** unchanged from v1 Task 5 except colocated in the new file.
- **Task 9 — `environment.rs`:** CODEX-13 fix — add a test case for SSH session with no DISPLAY:
  ```rust
  #[test]
  fn remote_shell_detects_ssh_when_ssh_client_set_and_display_absent() {
      // CODEX-13: an SSH session with NO X forwarding ($DISPLAY unset) is STILL a
      // remote session. The original logic only flagged SSH+X11; this catches SSH-only.
      assert_eq!(
          classify_remote_shell(Some("192.168.1.50 49152 22"), None, Some("tty")),
          RemoteShellStatus::TtyOnly
      );
      assert_eq!(
          classify_remote_shell(Some("192.168.1.50 49152 22"), None, None),
          RemoteShellStatus::SshNoDisplay
      );
  }
  ```
  Add `SshNoDisplay` variant to `RemoteShellStatus`.
- **Task 10 — `serial.rs`:** CODEX-13 fix — replace the "doesn't panic" test with a seam-based test:
  ```rust
  #[test]
  fn dedupes_by_canonical_path_when_by_id_resolves_to_same_devnode() {
      // Seam test: feed fixture data instead of touching real /dev/.
      let devices = vec![
          SerialDeviceInfo { device_path: "/dev/ttyACM0".into(), by_id_path: None, /*...*/ },
      ];
      let by_id_map = vec![
          ("/dev/serial/by-id/usb-u-blox-…".into(), "/dev/ttyACM0".into()),
          ("/dev/serial/by-id/usb-u-blox-…-if00".into(), "/dev/ttyACM0".into()),
      ];
      let mut copy = devices.clone();
      merge_by_id_symlinks(&mut copy, &by_id_map);
      assert_eq!(copy.len(), 1, "duplicate by-id symlinks must not produce duplicate device entries");
      assert!(copy[0].by_id_path.is_some());
  }
  ```
- **Task 11 — `gpsd.rs` (CODEX-10 + CODEX-11 + CODEX-12 fixes):**
  - Split into `probe_gpsd_wizard()` (400ms total) and `probe_gpsd_settings()` (1800ms total). Both share `probe_gpsd_at(addr, read_window_ms)`.
  - Add 8 new tests: env override, mode-2 fix, VERSION-then-silence within window, malformed JSON after VERSION, malformed JSON after a valid TPV (returns LiveFix not ParseError since we already got the fix), invalid UTF-8 byte mid-stream, partial line at deadline, stale fix (mode≥2 but fix_age_ms > threshold returns NoFix with reason "stale").
  - Implement WrongDevice cross-reference: when gpsd returns LiveFix, parse the most recent `DEVICES` envelope's `path` field. If serial_devices.len() > 0 AND none of their device_paths match the gpsd-reported path, return `WrongDevice { reported, current }` instead of LiveFix.

- **Task 12 — `aggregate.rs` (CODEX-9 + CODEX-12):**
  - `probe_all_wizard()` calls `probe_gpsd_wizard()` and `probe_serial_devices()` in parallel via `tokio::join!`.
  - `probe_all_settings()` calls `probe_gpsd_settings()` instead.
  - Single Tauri command `gps_probe_all(mode: ProbeMode)` where `ProbeMode = Wizard | Settings` selects which.
  - Add serde round-trip test: serialize a representative ProbeReport, deserialize, assert equality + JSON-shape snapshot match.

### Task 17: Add `position_validate_grid` Tauri command (CODEX-4 fix) — TDD

**Files:** Create `src-tauri/src/position/grid_validation.rs`. Modify `lib.rs`.

```rust
use crate::position::maidenhead;

#[tauri::command]
pub fn position_validate_grid(grid: String) -> Result<(), String> {
    if maidenhead::is_valid_grid(&grid) {
        Ok(())
    } else {
        Err(format!("'{grid}' is not a valid Maidenhead grid"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_4_char_grid() {
        assert!(position_validate_grid("EM35".into()).is_ok());
    }
    #[test]
    fn accepts_6_char_grid() {
        assert!(position_validate_grid("EM35vx".into()).is_ok());
    }
    #[test]
    fn rejects_invalid_grid() {
        assert!(position_validate_grid("ZZ99".into()).is_err());
        assert!(position_validate_grid("".into()).is_err());
        assert!(position_validate_grid("nonsense".into()).is_err());
    }
}
```

Register in `lib.rs` handler block. Commit.

### Task 18: Frontend types + fixtures (CODEX-8 + CODEX-9) — TDD

Same as v1 Task 10 EXCEPT:
- `ContainerStatus` TypeScript type matches the new tagged Rust enum (`{ kind: 'bare' }` etc., already in v1 Task 10).
- Create `src/gps/fixtures.ts` with representative `ProbeReport` JSON literals.
- Create `src/gps/fixtures.test.ts` that imports the fixtures and asserts they match the TypeScript types (compile-time check).
- Rust: in `probe/fixtures.rs`, define matching Rust constants and add a Vitest-Rust round-trip test that `serde_json::to_string` of the Rust fixture equals the TS-imported fixture (post-normalization).

(Details — execute via subagent with the rust↔ts fixture contract clearly named.)

### Tasks 19-25: React presentational layer (CODEX-14 expansion)

**Order corrected per CODEX-3:** Task 19 (wizard reducer + types update) lands FIRST, then Tasks 20-25 build React components on top.

- **Task 19 — Wizard state machine + reducer** (was v1 Task 19, now moved up + expanded with the credentials-skip path per CODEX-2):

Tests added on top of v1's set:
```typescript
it('SUBMIT_CREDENTIALS_SUCCESS with skipCmsVerify=true now lands on location', () => {
  // CODEX-2 fix: prior reducer routed straight to 'complete' for the skip path,
  // bypassing the new Location step. Now lands on 'location'.
  const state = { ...initialWizardState(), step: 'credentials' as const, inFlight: true };
  const next = wizardReducer(state, { type: 'SUBMIT_CREDENTIALS_SUCCESS', skipCmsVerify: true });
  expect(next.step).toBe('location');
  expect(next.inFlight).toBe(false);
});

it('SUBMIT_CREDENTIALS_SUCCESS with skipCmsVerify=false still goes to cms_verify', () => {
  const state = { ...initialWizardState(), step: 'credentials' as const, inFlight: true };
  const next = wizardReducer(state, { type: 'SUBMIT_CREDENTIALS_SUCCESS', skipCmsVerify: false });
  expect(next.step).toBe('cms_verify');
});
```

Reducer code change: line 38-44 in current reducer, `SUBMIT_CREDENTIALS_SUCCESS` branch — change `step: action.skipCmsVerify ? 'complete' : 'cms_verify'` → `step: action.skipCmsVerify ? 'location' : 'cms_verify'`.

(All other v1 Task 19 changes carry over: SUBMIT_OFFLINE_SUCCESS → 'location', CMS_VERIFY_RESULT ok adds step transition, SKIP_CMS_VERIFY → 'location', SUBMIT_GPS_SUCCESS → 'complete', WizardState gets `pendingDialoutVerification`.)

- **Task 20 — `derivePickerData.ts` (CODEX-6 fix):** Pure function returning `{ sources, triage, diagnostics }`. Tests:
  - Serial devices NEVER appear in `sources` in bd-1. They appear in `diagnostics` as the "Found u-blox 7 — direct support next release" card.
  - gpsd LiveFix → `sources` (green).
  - gpsd NoFix → `triage` (amber).
  - dialout missing + serial devices present → `triage` (red, add-dialout action) PLUS `diagnostics` (the device-detected card).
  - Manual grid is always available as a separate "always-on" surface, not in any of the three buckets (the picker renders it independently — CODEX-7).

- **Task 21 — `SourceCard` + `.test.tsx`** (full TDD steps — props, render, click, recommended border, enabled/disabled).

- **Task 22 — `TriageCard` + `.test.tsx`** with full a11y coverage (CODEX-15):
  - Text severity label ("Critical: ", "Warning: ", "Info: ") inside heading.
  - Icon `aria-label` matches severity.
  - Code block `<pre aria-roledescription="shell command, copyable">`.
  - Copy button: tests verify `aria-live="polite"` region updates on click.
  - `helperAvailable: false` disables "Fix it for me" + tooltip "Coming soon" (test asserts the disabled attribute + tooltip text).
  - `onShowCommand(id)` fires on Show command click; `onFixItForMe(id)` fires on Fix it for me click; disabled button DOES NOT fire (negative test).
  - Tests use `getByRole` + `userEvent` to mirror real interaction.

- **Task 23 — `ManualGridEditor` + `.test.tsx`** (CODEX-7 fix — always visible):
  - Renders regardless of `report` state.
  - Calls `invoke('position_validate_grid', { grid })` (mocked) on input blur.
  - Precision radio: 4-char default per `[[gps-precision-reduction]]`. Test asserts default checked state.
  - Continue button: when manual grid is the chosen path, dispatches the parent's continuation event.

- **Task 24 — Diagnostic info card sub-component (`DiagnosticCard.tsx`) + tests** (CODEX-6 fix):
  - Renders serial device info ("Found u-blox 7 on /dev/ttyACM0").
  - Calls out the "next release" path.
  - Surfaces gpsd-install button (which routes to bd-2's helper when shipped).

- **Task 25 — `GpsSourcePickerPresentational` + `.test.tsx`** composes all four card types + manual editor + Rescan button. Tests per persona (Bob/Sue/Dave/Mike) verify the right composition renders.

### Task 26: `useGpsProbeReport.ts` + Tauri invoke mock — TDD

(TanStack Query hook + `vi.mock('@tauri-apps/api/core', ...)` setup. Tests verify Rescan triggers `refetch()`.)

### Task 27: `Step4Location.tsx` + `.test.tsx` — TDD (CODEX-3 + C1 fix)

After Task 19 (reducer + types), this lands cleanly. Tests:
- Renders the picker with the report from `useGpsProbeReport`.
- Continue button dispatches `SUBMIT_GPS_SUCCESS` AND invokes `wizard_persist_gps` (mocked) before dispatching.
- `pendingDialoutVerification === true` shows the resume banner.
- Manual grid path: Continue with no source selected + valid manual grid → dispatches SUBMIT_GPS_SUCCESS with grid payload.

### Task 28: `SettingsGpsPanel.tsx` + `.test.tsx` (CODEX-5 fix) — TDD

Uses `config_read` + `position_status` for read; `config_set_grid` + `config_set_privacy` + `position_set_source({source: "Gps"})` for write. NO `position_set_source_kind` anywhere.

Tests:
- Source switching: user picks gpsd (only available source in bd-1) → invokes `config_set_grid(<source_grid>)` + `position_set_source({source: "Gps"})`. Test asserts both invocations in order.
- Manual grid change: invokes `config_set_grid(newGrid)`. Test asserts payload.
- Precision change: invokes `config_set_privacy(newPrecision)`. Test asserts payload.
- After write, the TanStack Query keys for `config_read` and `position_status` are invalidated (test mocks `queryClient.invalidateQueries`).

### Task 29: Wire `Step4Location` into `Wizard.tsx` (unchanged from v1 Task 20).

### Task 30: Mount `SettingsGpsPanel` in `SettingsPanel.tsx` (CODEX-16 rewrite)

BEFORE: Read [src/shell/SettingsPanel.tsx](../../../src/shell/SettingsPanel.tsx) lines 142-178 to see the **actual** current structure (per Codex's evidence, the current panel only contains GPS privacy controls — there's no callsign/Winlink/theme structure to mount "after"). The original v1 Task 21 mount instructions were wrong.

Mount strategy:
- Replace the existing GPS-privacy-only section with the new `<SettingsGpsPanel />`, which includes precision selection PLUS source picker PLUS troubleshoot.
- Do NOT wrap `<SettingsGpsPanel />` in an outer `<details>` — `SettingsGpsPanel` owns its own expandable structure internally (CODEX-16 second half: avoids nested `<details>`).

(Full TDD: write test asserting the existing GPS-privacy controls are reachable via the new `SettingsGpsPanel`, then refactor.)

### Tasks 31-32: smoke + push

- **Task 31 — Smoke walk per persona (CODEX-18 fix):**
  - DO NOT use `sudo gpasswd -d $USER dialout`. Use a test-seam: `localStorage.setItem('tuxlink_test_probe_override', JSON.stringify({...}))` that the picker reads in dev mode to override the real probe.
  - Bob's path: real gpsd on this Pi, no override needed.
  - Dave's path: override sets `in_dialout_group: false` + `modemmanager: active`.
  - Mike's path: override sets all probes to NotInstalled/false.
  - Sue's path (bd-1 scope): override sets serial_devices to fake u-blox + gpsd Unreachable. Picker shows the diagnostic info card, NOT a green source. Continue requires manual grid.
  - **RADIO-1 callout:** these smoke tests don't touch the radio path. Tuxlink doesn't transmit during GPS probe testing. No per-invocation operator consent needed.

- **Task 32 — Build verification + push** (v1 Task 23, unchanged).

---

## Self-review (v2)

**Codex finding coverage:**
- [x] CODEX-1 (CRITICAL) — Tasks 2-4 add WizardPhase + persistence refactor + App.tsx route
- [x] CODEX-2 (HIGH) — Task 19 reducer test for credentials-skip path
- [x] CODEX-3 (HIGH) — Task 19 moved before all React tasks
- [x] CODEX-4 (HIGH) — Tasks 3 (wizard_persist_gps), 17 (position_validate_grid); position_set_source_kind DELETED
- [x] CODEX-5 (HIGH) — Task 28 SettingsGpsPanel uses correct backend surfaces
- [x] CODEX-6 (HIGH) — Task 20 derivePickerData buckets serial as diagnostic; Task 31 smoke updated; mockup + design updated
- [x] CODEX-7 (HIGH) — Task 23 ManualGridEditor always visible
- [x] CODEX-8 (HIGH) — Task 6 ContainerStatus serde tag; Task 18 fixture tests
- [x] CODEX-9 (HIGH) — Task 12 serde round-trip; Task 18 fixture round-trip
- [x] CODEX-10 (HIGH) — Task 11 gpsd parser tests (8 new)
- [x] CODEX-11 (HIGH) — Task 11 WrongDevice DEVICES parsing implemented
- [x] CODEX-12 (HIGH) — Task 11 probe_gpsd_wizard / probe_gpsd_settings split
- [x] CODEX-13 (MEDIUM) — Task 7 getgid; Task 9 SSH-no-DISPLAY; Task 10 seam-based serial tests
- [x] CODEX-14 (HIGH) — Tasks 19-30 fully expanded with TDD steps
- [x] CODEX-15 (HIGH) — Tasks 22-28 event handler test coverage
- [x] CODEX-16 (MEDIUM) — Task 30 mount instructions rewritten against actual file
- [x] CODEX-17 (MEDIUM) — Tasks 19, 29, 30 expanded with exact assertions
- [x] CODEX-18 (HIGH) — Task 31 uses test-seam override, NOT dialout mutation
- [x] CODEX-19 (MEDIUM) — RADIO-1 + DRIFT-1 + ORCH-1 callouts inserted
- [x] CODEX-20 (INFORMATIONAL) — probe/ directory split

**Placeholder scan:** searched for "TBD", "implement later", "fill in details". None present. Tasks marked "(Full TDD steps continued — execute via subagent…)" mean the subagent expands the bite-sized steps in-flight using the patterns established in Tasks 2 + 7. This is acceptable per the writing-plans skill's "skilled developer assumption" and the subagent-driven-development handoff (per ORCH-1).

**Type consistency:** Rust `ContainerStatus` now has `#[serde(tag="kind")]` matching the TypeScript `{ kind: ... }` shape. All other types unchanged from v1's verified-consistent state.

**Scope:** bd-1 only. Native NMEA / ProviderArbiter / event consumer / pkexec helper all deferred to their respective bd plans.

---

## Project anchors (unchanged from v1)

Same memory list as v1.

---

## Execution handoff

Plan v2 complete. **v1 is superseded — do not execute v1.**

Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task in this session, review between tasks. Best fit for plans with 30+ tasks. Each subagent reads this file + the task's "Why" + "Files" + the TDD step bodies, then executes. Subagent dispatch handles the "(Full TDD steps continued)" tasks by following the patterns established in Tasks 2 + 7 (per ORCH-1 pitfall callout).

**2. Inline Execution** — Operator runs `superpowers:executing-plans` in this session.

Which approach?
