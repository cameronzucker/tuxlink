# Task 16 — Dashboard Ribbon + Minimal Status Bar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement v0.0.1 Task 16 as TWO surfaces per AMD-8: a **dashboard ribbon** above the main panes (callsign · grid · GPS · UTC+local · connection-with-transport) and a **minimal status bar** at the bottom (app-chrome only). Together they satisfy Principles 3 ("promote the dashboard information legacy Winlink hides") and 4 ("persistent radio-connection-state pane"), and operationalize §5.9 of the canonical UX design doc.

**Architecture:** Frontend = two React components (`DashboardRibbon.tsx`, `StatusBar.tsx`) plus shared hooks/derivation modules. Backend = two Tauri commands (`dashboard_read`, `status_read`) returning stub snapshots in v0.0.1 (no real Pat-session-state tracking yet; v0.1 promotes). State derivation is a pure function (`derive*` helpers) so it's unit-testable without GUI. GPS field renders **precision-reduced (4-char Maidenhead) by default** per Principle 7 + RADIO-2 — full 6-char only when `privacy.position_precision = SixCharGrid` is configured. Connection-state field always names the transport (CMS-SSL or Telnet) per the transport-visibility anti-pattern + §4.1 finding.

**Tech Stack:** Rust 1.75+ (Tauri 2 commands), React 18 + TypeScript 5 (components), Vite (dev server), TanStack Query 5 (polling), vitest (TS unit tests), `cargo test` (Rust unit tests), Tauri event bus (menu event consumption for status-bar toggle).

---

## Living Document Contract

This plan is a living document. Every executing agent MUST update it as
execution progresses, not only at completion.

- **On phase claim:** the executor MUST flip the banner to 🚧 IN PROGRESS
  with a claim timestamp (ISO 8601 UTC) and the active branch name. The
  banner MUST NOT include an expected-completion estimate — agents cannot
  reliably estimate their own wall-clock, and a fabricated duration
  becomes a stale anchor that misleads future readers. Followers
  encountering a 🚧 banner determine liveness by observable signals (PR
  existence, recent branch commits), not by arithmetic on expected times.
  See Step 5's stale-claim reclaim protocol.
- **On phase ship:** the executor MUST update that phase's **Execution
  Status** banner with the shipped commit SHA(s) and date. If a PR is
  open, the PR number and URL MUST appear in the top-of-plan Execution
  Status table.
- **On phase defer:** the executor MUST update the banner with ⏸ status
  AND a prose description of the unblock condition + a link to the
  likely-unblocker artifact (plan page, task, or PR whose own Execution
  Status banner will signal completion). Prose + link is durable across
  paraphrases and scope edits; exact-string coordination between agents
  is not.
- **On PR merge:** the executor MUST record the merge SHA in the banner
  + the top-of-plan Execution Status table.
- **On deviation from the written plan** (scope edits, structural
  refactors, dropped tasks, reordered phases): the executor MUST
  inline-document the deviation in the affected task AND summarize it
  in the top-of-plan Execution Status as a "Deviations" subsection.
  Deviation state MUST NOT live only in PR notes or status reports.
- **On discovery** (pre-existing drift surfaced during execution, new
  bugs found, architectural issues noted): the executor MUST add a
  "Discoveries" subsection at the top of the plan with pointers to the
  files/lines affected. Follow-up dispatches read this subsection to
  avoid duplicate discovery work.

The plan SHOULD reflect reality at the end of every session that touches
it. Anything worth putting in a status report to the user is worth
putting in the plan.

Rationale: `/writing-plans-enhanced` Step 5. Writing at ship time is
cheap; reconstruction by downstream readers is expensive, compounds
across dispatches, and fails silently when state is split across PR
notes and commit messages.

---

## Execution Status

**Overall:** Not started.

| Phase | Status | Ship SHA(s) | Notes |
|---|---|---|---|
| 1 — Backend stubs (Tauri commands + types) | ⬜ Not started | — | — |
| 2 — Frontend derivation helpers (pure TS, unit-tested) | ⬜ Not started | — | — |
| 3 — DashboardRibbon component + integration | ⬜ Not started | — | — |
| 4 — Minimal StatusBar component + integration | ⬜ Not started | — | — |
| 5 — App.tsx layout wiring + menu event handler | ⬜ Not started | — | — |
| 6 — Manual verification + commit + 3-round review gate | ⬜ Not started | — | REVIEW GATE per plan §Task 16 |

### Deviations

- **Plan-author-level deviation (cypress-lupine-moss, 2026-05-18):** the design doc §5.9 example uses `↔` (U+2194 LEFT RIGHT ARROW) in the connection-state label: `"In session via CMS-SSL (W4PHS↔cms.winlink.org)"`. This plan uses the ASCII fallback `<->` instead in all code + tests. Rationale: (a) Tauri's webview renders Unicode fine, but the test assertions are easier to author and review without Unicode escape mangling, (b) the Pat session-log format itself uses ASCII (per §4.4), (c) the connection-detail string is operator-facing diagnostic info, not branded UX copy. If Cameron prefers the `↔` character at review time, the swap is a 5-line change (the literal in `deriveConnectionField` + the 3 matching test assertions). Surface for review in the PR body's "Open decisions" section.

### Discoveries
_(none yet)_

---

## Prerequisites — read these BEFORE any task

**Every subagent executing a phase below MUST read these files first.** Plan-section-only reads are insufficient — the load-bearing context lives in the source docs.

1. [`CLAUDE.md`](../../CLAUDE.md) — project ethos, destructive-git ban (no `--force`, no `--amend` on pushed commits, no `git rebase -i`), agent-moniker discipline (set yours via `python3 .claude/scripts/get_agent_moniker.py` BEFORE any git op; include `Agent: <moniker>` trailer in every commit), worktrees-mandatory rule (when the `block-main-checkout-race.sh` hook denies a write, route to a worktree via `bd create` + `python3 .claude/scripts/new_tuxlink_worktree.py` per ADR 0008 + HOOK-1).
2. [`docs/design/v0.0.1-ux-mockups.md`](../design/v0.0.1-ux-mockups.md) — §5.9 (canonical spec for Task 16's TWO surfaces), §4.1 (transport-visibility anti-pattern; the connection-state field must NAME the transport), §4.7 (GPS auto-grid-update + 3-state privacy model), §6 (config schema including `IdentityConfig`, `PrivacyConfig`, `CmsTransport`, `GpsState`, `PositionPrecision`), §7 (new menu items — relevant: `menu:view:status_bar` for status-bar toggle is in Task 7 baseline).
3. [`docs/design/v0.0.1-ux-principles.md`](../design/v0.0.1-ux-principles.md) — Principle 3 (promote dashboard info), Principle 4 (persistent radio-connection-state pane), **Principle 7 (position privacy via precision reduction, not opt-out — the dashboard ribbon's grid field MUST default to 4-char broadcast precision; full 6-char only with explicit opt-in)**.
4. [`docs/pitfalls/implementation-pitfalls.md`](../pitfalls/implementation-pitfalls.md):
    - **RADIO-1** (no agent-autonomous transmission — this task only renders state; it does NOT initiate connections. If you find yourself adding code that opens a CMS session, STOP and escalate. You are not authorized to transmit.).
    - **RADIO-2** (encryption decisions require operator approval — the dashboard's connection-state label MUST surface CMS-SSL vs Telnet transparently, never hide; this is the operator-visibility primitive RADIO-2 + §4.1 prescribe).
    - **SCOPE-1** (tuxlink is the CLIENT, not the GATEWAY — the connection-state label refers to OUTBOUND client sessions to the CMS, never inbound gateway-style listening).
    - **HOOK-1 / LEASE-1 / PARITY-1** (operational discipline; if a hook denies a write, route to a worktree and do not argue).
5. [`docs/pitfalls/testing-pitfalls.md`](../pitfalls/testing-pitfalls.md) — §1 (test output pristine), §3 (error path coverage — render with malformed snapshots), §4 (negative property testing — empty/null/zero inputs for callsign/grid/identifier; oversized inputs), §6 (default values are tested — what does the ribbon render before backend snapshot arrives? Empty? Loading?), §7 (test infrastructure hygiene — no shared mutable state, no hardcoded time-of-day).
6. [`docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md`](2026-04-22-tuxlink-v0.0.1-plan.md):
    - "Subagent Guardrails" — no extra dependencies, no extra crates, no destructive git, branch-naming, commit cadence.
    - **Task 2 (AMD-1 schema)** lines ~274-467 — the canonical shape of `IdentityConfig`, `PrivacyConfig`, `CmsTransport`, `GpsState`, `PositionPrecision`. The ribbon's data sources MUST match these field names exactly.
    - **Task 7 (AMD-10)** lines ~1703-1862 — the menu spec. `menu:view:status_bar` event is already defined for status-bar toggle. The runtime-half items (`menu:view:radio_dock`, `menu:view:raw_log`, `menu:tools:settings_*`) are NOT this task's concern — they're consumed by Tasks 15, 16.5, and Settings dialogs.
    - **Task 15 (AMD-7)** lines ~3766-3935 — the session log pane shares "current session state" terminology (Idle / Connecting / In-session / Disconnecting) with the dashboard ribbon's connection-state field. Both surfaces consume the SAME backend snapshot; do not duplicate the derivation logic.

**You SHALL NOT** modify any of these prerequisite docs as part of this task. If you find a real spec gap during execution, STOP and surface it for review per the Living Document Contract's "On discovery" rule — do NOT silently amend the spec.

---

## Mandatory Per-Phase Preamble

**Every phase below starts with this work, implicitly. Do it even though it is not repeated verbatim per phase:**

1. Verify your agent moniker is set (commit trailers MUST include `Agent: <moniker>` on its own line per CLAUDE.md). If not set, run `python3 .claude/scripts/get_agent_moniker.py` and use the result for all forward commits this session.
2. Read the 6 prerequisite files above.
3. Invoke the `superpowers:test-driven-development` skill (or read the equivalent `SKILL.md` under `~/.claude/plugins/cache/claude-plugins-official/superpowers/<version>/skills/test-driven-development/` — pick the highest-numbered version directory).
4. Follow TDD: write the failing test first, run to confirm it fails, implement the minimal code to pass, run to confirm green. **No implementation code before a failing test.**

## Mandatory Per-Phase Completion Check

**Before marking any phase complete:**

1. Re-read `docs/pitfalls/testing-pitfalls.md` with your just-written tests in mind. Specifically check: test output pristine, no skipped-is-passing, error paths covered, boundary values validated (empty callsign, missing grid, malformed transport), no shared-state test flakes, no hardcoded time-of-day in tests that render UTC/local.
2. Run the phase's test command and confirm green.
3. If any test assertion races, flakes, or fails nondeterministically, the fix is deterministic synchronization (a `waitFor` with explicit predicate, a `vi.useFakeTimers()` shim with explicit advance, a `tokio::time::pause`/`advance` in Rust async) — NOT assertion removal or weakening. If synchronization cannot make the assertion pass reliably, STOP and raise to the dispatching agent. Do not ship a weaker test. Weakened assertions rationalized as "CI stability fixes" are the exact pattern this rule prevents.
4. Update the Living Document Contract's Execution Status banner for that phase to ✅ SHIPPED with the commit SHA and date.

## Phase grouping for review

Phases 1-5 ship as separate logical units inside the same task-branch but **MUST NOT be merged piecemeal** — Task 16 is a single review-gate per the plan's overview table. After Phase 5 ships, Phase 6 runs the REVIEW GATE protocol (3-round review over Tasks 12-16 per Task 16's existing review-gate semantics — but the implementing agent's responsibility ENDS at "Task 16 is review-ready and PR is open"; the actual cross-task review gate is operator-scheduled).

After completing the entire 6-phase group:

```
After completing this group:
Review the batch from multiple perspectives. Minimum 3 review rounds.
If round 3 still finds issues, keep going until clean.
```

---

## File structure

Before defining tasks, the locked-in decomposition:

| File | Responsibility | Phase |
|---|---|---|
| `src-tauri/src/lib.rs` | Wire two new Tauri commands (`dashboard_read`, `status_read`) into the existing handler list. ~10 lines added. | Phase 1 |
| `src-tauri/src/dashboard.rs` | NEW. Defines `DashboardSnapshot` struct, `status_read()` + `dashboard_read()` command implementations (stub for v0.0.1, returning `Idle` connection state + config-derived callsign/grid/GPS values). Pure module — no Pat-process integration in v0.0.1. | Phase 1 |
| `src-tauri/tests/dashboard_test.rs` | NEW. Unit tests for the Tauri command return shapes (JSON serialization matches the TS-side schema). | Phase 1 |
| `src/shell/types.ts` | NEW. TypeScript type definitions: `DashboardSnapshot`, `StatusSnapshot`, `ConnectionState`, `GpsState`, `PositionPrecision`, `CmsTransport`. Mirrors the Rust types in `dashboard.rs` and the config types in `src-tauri/src/config.rs`. | Phase 2 |
| `src/shell/dashboard.ts` | NEW. Pure functions: `deriveCallsignField`, `deriveGridField` (PRECISION-REDUCED BY DEFAULT per Principle 7), `deriveGpsField`, `deriveTimeFields` (UTC + local), `deriveConnectionField` (always names transport per §4.1). All synchronous, no DOM, no Tauri imports — unit-testable. | Phase 2 |
| `src/shell/dashboard.test.ts` | NEW. Vitest unit tests for every derivation helper — happy path + empty/null/oversized + the GPS precision-reduction default (critical: 6-char input MUST render 4-char in default config) + transport-naming invariant (connection label MUST contain the transport name). | Phase 2 |
| `src/shell/status.ts` | NEW. Pure functions for the minimal status bar: `deriveActivityLabel` (left side), `deriveWindowInfoLabel` (right side). Mail.app-style minimal. | Phase 2 |
| `src/shell/status.test.ts` | NEW. Vitest unit tests for the status derivation helpers. | Phase 2 |
| `src/shell/useDashboard.ts` | NEW. TanStack Query hook polling `dashboard_read` every 5s. Returns `DashboardSnapshot` + loading + error states. | Phase 3 |
| `src/shell/DashboardRibbon.tsx` | NEW. React component rendering the 5 fields (callsign · grid · GPS · UTC+local · connection). Top of the main window, ~40px tall, always visible in v0.0.1. Reads via `useDashboard`. | Phase 3 |
| `src/shell/DashboardRibbon.test.tsx` | NEW. Vitest + React Testing Library: render with various snapshots, assert each field renders with the expected text + accessibility attributes (`role="status"`, `aria-live="polite"` on the connection field). | Phase 3 |
| `src/shell/useStatus.ts` | NEW. TanStack Query hook polling `status_read` every 5s. Returns `StatusSnapshot` + loading + error states. | Phase 4 |
| `src/shell/StatusBar.tsx` | NEW. React component rendering app-chrome only (left: last action timestamp; right: pane focus / window info). ~24px tall, toggleable via `menu:view:status_bar`. | Phase 4 |
| `src/shell/StatusBar.test.tsx` | NEW. Vitest + React Testing Library: render hidden vs visible, render with snapshots, assert minimal Mail.app-style output. | Phase 4 |
| `src/App.tsx` | MODIFY. Mount `<DashboardRibbon />` at the top of the main layout; mount `<StatusBar visible={statusBarVisible} />` at the bottom; wire `menu:view:status_bar` Tauri event to toggle `statusBarVisible` (default `true`). Layout uses CSS grid `grid-template-rows: var(--ribbon-h) 1fr var(--status-h)` so the ribbon and status bar don't push other panes' contents. | Phase 5 |
| `src/App.css` | MODIFY. CSS variables: `--ribbon-h: 40px`, `--status-h: 24px`. Class styles for `.dashboard-ribbon` and `.status-bar` (grid layout, font sizing, colors). Do NOT introduce a CSS framework; plain CSS only per the existing scaffold. | Phase 5 |

**Files this task MUST NOT touch:**
- Any file under `src/wizard/` (Tasks 9-11.5 own those — separate PRs).
- Any file under `src/mailbox/`, `src/compose/`, `src/session/`, `src/dock/` (other tasks own those).
- `src-tauri/src/config.rs` (Task 2 owns it; this task READS the config but does not modify the schema).
- `src-tauri/src/menu.rs` (Task 7 owns it; this task LISTENS to menu events but does not add new ones — `menu:view:status_bar` is already in the Task 7 baseline).
- `src-tauri/src/session_log.rs` or `src-tauri/src/pat_process.rs` (Task 15 owns those; the v0.0.1 dashboard snapshot is a STUB returning Idle — no Pat-state integration yet).
- `docs/design/*.md`, `docs/pitfalls/*.md`, `docs/plans/2026-04-22-*.md` (specs; see "Prerequisites").

---

## One-time setup before Phase 1

**Done once at the start of Wave-2 implementation, NOT per phase:**

1. **Claim the Task 16 bd issue:**

   ```bash
   bd update tuxlink-hvv --claim
   ```

   (Note: `tuxlink-3dz` is the plan-writing bd issue — already closed by the Wave-1 plan-writing agent. `tuxlink-hvv` is the implementation bd issue that this plan executes.)

2. **Create the per-task branch + worktree (if running outside the main checkout):**

   ```bash
   # Option A: worktree (REQUIRED if block-main-checkout-race.sh denies a write — per HOOK-1)
   python3 .claude/scripts/new_tuxlink_worktree.py \
     --slug task-16-status-surfaces \
     --issue tuxlink-hvv \
     --moniker <your-moniker>
   cd worktrees/bd-tuxlink-hvv-task-16-status-surfaces

   # Option B: main checkout (only if the hook does NOT deny)
   git checkout feat/v0.0.1
   git pull --ff-only
   git checkout -b bd-tuxlink-hvv/task-16-status-surfaces
   ```

   If you went with Option A, the script creates the branch `bd-tuxlink-hvv/task-16-status-surfaces` for you and cd's into the worktree.

3. **Set your agent moniker** (CLAUDE.md requirement; trailer-required in every commit):

   ```bash
   python3 .claude/scripts/get_agent_moniker.py
   ```

   Record the result; substitute it for the `<SESSION-MONIKER>` placeholder appearing in every commit heredoc throughout the phases below.

---

## Phase 1 — Backend Tauri commands (stubs)

**Execution Status:** ⬜ NOT STARTED

**Goal of this phase:** Provide the two Tauri commands the frontend will call. v0.0.1 returns stubbed values derived from the config file; real Pat-session-state tracking is v0.1+. The contract (JSON shape) is what matters here — frontend phases depend on it.

**Files:**
- Create: `src-tauri/src/dashboard.rs`
- Create: `src-tauri/tests/dashboard_test.rs`
- Modify: `src-tauri/src/lib.rs` (register the module + commands)

**Ordering dependencies:** Phase 1 must complete BEFORE Phase 3 (DashboardRibbon component consumes `dashboard_read`) and Phase 4 (StatusBar consumes `status_read`). Phase 2 (frontend pure helpers) is parallelizable with Phase 1 because it has no backend dependency.

**DO NOT, in this phase:**

- DO NOT wire `dashboard_read` to actually read from `crate::config::load()` — the v0.0.1 scope is **stubbed values only**. Hooking up the config-loader is a separate follow-up task (see "Tasks NOT in scope" in Phase 6's PR body).
- DO NOT wire `dashboard_read` to any session-state tracker, `LogRing`, or Pat process — that's v0.1+. Touching `src-tauri/src/session_log.rs` or `src-tauri/src/pat_process.rs` from this task is out of scope (Task 15 owns those).
- DO NOT add `chrono` or any other date-time crate. The plan uses an inline `unix_seconds_to_ymdhms` helper specifically to avoid adding a dependency. The plan's `## Subagent Guardrails` block forbids unlisted crates.

### Task 1.1 — Write the failing test for `DashboardSnapshot` shape

- [ ] **Step 1.1.1: Create `src-tauri/tests/dashboard_test.rs`**

```rust
use serde_json::json;
use tuxlink_lib::dashboard::{DashboardSnapshot, ConnectionStateKind, GpsStateKind, CmsTransportKind};

#[test]
fn test_dashboard_snapshot_serializes_with_expected_keys() {
    let snap = DashboardSnapshot {
        callsign: Some("W4PHS".to_string()),
        identifier: None,
        grid_local: Some("EM75xx".to_string()),       // full precision stored locally
        grid_broadcast: Some("EM75".to_string()),     // precision-reduced for broadcast
        gps_state: GpsStateKind::BroadcastAtPrecision,
        utc_iso: "2026-05-18T14:32:00Z".to_string(),
        local_iso: "2026-05-18T09:32:00-05:00".to_string(),
        local_tz_abbrev: "CDT".to_string(),
        connection_state: ConnectionStateKind::Idle,
        transport: CmsTransportKind::CmsSsl,
        connection_detail: None,
    };
    let v = serde_json::to_value(&snap).expect("serializes");
    assert_eq!(v["callsign"], json!("W4PHS"));
    assert_eq!(v["grid_local"], json!("EM75xx"));
    assert_eq!(v["grid_broadcast"], json!("EM75"));
    assert_eq!(v["gps_state"], json!("BroadcastAtPrecision"));
    assert_eq!(v["connection_state"], json!("Idle"));
    assert_eq!(v["transport"], json!("CmsSsl"));
    assert!(v["utc_iso"].as_str().unwrap().ends_with("Z"));
}

#[test]
fn test_dashboard_snapshot_offline_path_uses_identifier_not_callsign() {
    let snap = DashboardSnapshot {
        callsign: None,
        identifier: Some("EOC-1".to_string()),
        grid_local: Some("EM75".to_string()),
        grid_broadcast: Some("EM75".to_string()),
        gps_state: GpsStateKind::Off,
        utc_iso: "2026-05-18T14:32:00Z".to_string(),
        local_iso: "2026-05-18T09:32:00-05:00".to_string(),
        local_tz_abbrev: "CDT".to_string(),
        connection_state: ConnectionStateKind::Idle,
        transport: CmsTransportKind::CmsSsl,
        connection_detail: None,
    };
    let v = serde_json::to_value(&snap).expect("serializes");
    assert_eq!(v["callsign"], json!(null));
    assert_eq!(v["identifier"], json!("EOC-1"));
    assert_eq!(v["gps_state"], json!("Off"));
}

#[test]
fn test_dashboard_snapshot_in_session_includes_connection_detail() {
    let snap = DashboardSnapshot {
        callsign: Some("W4PHS".to_string()),
        identifier: None,
        grid_local: Some("EM75xx".to_string()),
        grid_broadcast: Some("EM75".to_string()),
        gps_state: GpsStateKind::BroadcastAtPrecision,
        utc_iso: "2026-05-18T14:32:00Z".to_string(),
        local_iso: "2026-05-18T09:32:00-05:00".to_string(),
        local_tz_abbrev: "CDT".to_string(),
        connection_state: ConnectionStateKind::InSession,
        transport: CmsTransportKind::CmsSsl,
        connection_detail: Some("W4PHS<->cms.winlink.org".to_string()),
    };
    let v = serde_json::to_value(&snap).expect("serializes");
    assert_eq!(v["connection_state"], json!("InSession"));
    assert_eq!(v["transport"], json!("CmsSsl"));
    assert_eq!(v["connection_detail"], json!("W4PHS<->cms.winlink.org"));
}

#[test]
fn test_status_snapshot_serializes_with_expected_keys() {
    use tuxlink_lib::dashboard::StatusSnapshot;
    let s = StatusSnapshot {
        last_action: Some("2026-05-18T14:32:00Z".to_string()),
        last_action_label: Some("Toggled session log".to_string()),
        pane_focus: Some("inbox".to_string()),
    };
    let v = serde_json::to_value(&s).expect("serializes");
    assert_eq!(v["last_action"], json!("2026-05-18T14:32:00Z"));
    assert_eq!(v["last_action_label"], json!("Toggled session log"));
    assert_eq!(v["pane_focus"], json!("inbox"));
}
```

- [ ] **Step 1.1.2: Run the test to confirm failure**

```bash
cd src-tauri
cargo test --test dashboard_test
```

Expected: compile error "module `dashboard` not found" or "could not find `DashboardSnapshot` in `tuxlink_lib`". Red stage.

### Task 1.2 — Implement `src-tauri/src/dashboard.rs`

- [ ] **Step 1.2.1: Create `src-tauri/src/dashboard.rs` with the snapshot types and stub commands**

```rust
//! Dashboard ribbon + minimal status bar backend (v0.0.1 stubs).
//!
//! Per AMD-8 (Task 16) + docs/design/v0.0.1-ux-mockups.md §5.9:
//! - DashboardSnapshot feeds the top ribbon (callsign · grid · GPS · UTC+local · connection).
//! - StatusSnapshot feeds the bottom status bar (app activity only — Mail.app-style).
//!
//! v0.0.1 ships STUB implementations: connection_state is always Idle, status fields
//! return reasonable initial defaults. Real Pat-session-state tracking lands in v0.1+.
//!
//! The GPS field is precision-reduced for broadcast BY DEFAULT per Principle 7 +
//! RADIO-2: the 6-character grid is stored locally (config.identity.grid) but only
//! the 4-character prefix is broadcast unless the operator opts into SixCharGrid.
//! See docs/design/v0.0.1-ux-mockups.md §5.9 + §6 and Principle 7.

use serde::{Deserialize, Serialize};

/// Top-of-window dashboard ribbon data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardSnapshot {
    /// Active Winlink callsign for CMS-mode installs (config.identity.callsign).
    /// None when connect.connect_to_cms = false (offline path).
    pub callsign: Option<String>,
    /// Free-form station identifier for offline installs (config.identity.identifier).
    /// None when connect.connect_to_cms = true.
    pub identifier: Option<String>,
    /// Full-precision Maidenhead grid as stored in config (config.identity.grid).
    /// Tooltip-only when broadcast precision is 4-char.
    pub grid_local: Option<String>,
    /// Precision-reduced grid for ribbon display (4-char by default).
    /// Per Principle 7: 4-char unless privacy.position_precision = SixCharGrid.
    pub grid_broadcast: Option<String>,
    /// Operator's GPS state (3-state per Principle 7).
    pub gps_state: GpsStateKind,
    /// UTC time in ISO 8601 (e.g., "2026-05-18T14:32:00Z").
    pub utc_iso: String,
    /// Local time in ISO 8601 with offset (e.g., "2026-05-18T09:32:00-05:00").
    pub local_iso: String,
    /// Locale-aware abbrev for local TZ (e.g., "CDT", "PST"). Best-effort.
    pub local_tz_abbrev: String,
    /// Current outbound connection state.
    pub connection_state: ConnectionStateKind,
    /// Operator-chosen transport (always named per §4.1 transport-visibility anti-pattern).
    pub transport: CmsTransportKind,
    /// Optional human-readable detail for in-session state (e.g., "W4PHS<->cms.winlink.org").
    /// None when connection_state = Idle or Connecting.
    pub connection_detail: Option<String>,
}

/// Bottom status bar data — app-chrome only (Mail.app-style).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusSnapshot {
    /// ISO 8601 UTC timestamp of last user action (menu click, pane focus change).
    pub last_action: Option<String>,
    /// Human-readable label for the last action (e.g., "Toggled session log").
    pub last_action_label: Option<String>,
    /// Currently-focused pane key (e.g., "inbox", "reading", "compose").
    pub pane_focus: Option<String>,
}

/// GPS dashboard-display state. Combines the 3-state operator-intent setting
/// from `config.privacy.gps_state` (AMD-1 / Principle 7: `Off`, `LocalUiOnly`,
/// `BroadcastAtPrecision`) with 2 runtime-only states (`Searching`, `Manual`)
/// that describe the device acquisition phase. The config schema persists ONLY
/// the 3 intent variants; the runtime variants are derived from device state.
///
/// In v0.0.1 the stub returns `Manual` (no GPS device integration yet); the
/// runtime variants exist in this enum so v0.1+ can promote without changing
/// the wire format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GpsStateKind {
    /// No GPS device read at all (matches config.privacy.gps_state = Off).
    Off,
    /// GPS read locally; never broadcast (matches config = LocalUiOnly).
    LocalUiOnly,
    /// GPS read and broadcast at chosen precision (matches config = BroadcastAtPrecision).
    BroadcastAtPrecision,
    /// Runtime: GPS configured but device not yet acquired (yellow indicator). v0.1+.
    Searching,
    /// Runtime: Operator entered grid manually; no GPS device involvement. v0.0.1 default.
    Manual,
}

/// Outbound connection lifecycle states. v0.0.1 stub: always Idle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionStateKind {
    Idle,
    Connecting,
    InSession,
    Disconnecting,
    Disconnected,
}

/// Mirrors config.connect.transport (CmsTransport). Always named in the ribbon
/// per §4.1 transport-visibility anti-pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CmsTransportKind {
    CmsSsl,
    Telnet,
}

/// Tauri command: return the dashboard snapshot.
///
/// v0.0.1 stub: returns a snapshot reflecting the current config + a frozen
/// "Idle" connection state. v0.1+ will integrate with the Pat session-log
/// projection (Task 15's LogRing) and the background-polling state to derive
/// the real connection_state and connection_detail.
///
/// SCOPE-1 reminder: this command's connection_state refers to OUTBOUND
/// client sessions to the CMS. It does NOT report any inbound gateway-style
/// listening — tuxlink is the client side only.
#[tauri::command]
pub async fn dashboard_read() -> Result<DashboardSnapshot, String> {
    // v0.0.1 stub. The real implementation would:
    //   1. Read config via crate::config::load() (Task 2).
    //   2. Derive grid_broadcast from grid_local + privacy.position_precision.
    //   3. Pull connection_state from a session-state tracker (v0.1+).
    //
    // For v0.0.1 we return safe placeholder values. The FRONTEND derivation
    // layer (Phase 2) handles the precision-reduction default; the backend
    // returns both grid_local and grid_broadcast as-is so the frontend can
    // re-derive deterministically when config changes mid-session.

    // Generate timestamps deterministically here so the command is testable;
    // the real implementation can use chrono::Utc::now(). For v0.0.1 stub,
    // we use a fixed-format placeholder.
    let utc = chrono_like_utc_now();
    Ok(DashboardSnapshot {
        callsign: Some("W4PHS".to_string()),       // STUB: real impl reads config
        identifier: None,
        grid_local: Some("EM75xx".to_string()),     // STUB
        grid_broadcast: Some("EM75".to_string()),   // STUB precision-reduced
        gps_state: GpsStateKind::Manual,            // STUB
        utc_iso: utc.clone(),
        local_iso: utc,                              // STUB — no TZ math in v0.0.1 backend
        local_tz_abbrev: "UTC".to_string(),         // STUB
        connection_state: ConnectionStateKind::Idle,
        transport: CmsTransportKind::CmsSsl,
        connection_detail: None,
    })
}

/// Tauri command: return the minimal status snapshot.
#[tauri::command]
pub async fn status_read() -> Result<StatusSnapshot, String> {
    Ok(StatusSnapshot {
        last_action: None,
        last_action_label: None,
        pane_focus: None,
    })
}

/// Helper: produces an ISO-8601 UTC timestamp string.
///
/// Uses std::time::SystemTime to avoid adding a new dependency (chrono is
/// NOT in v0.0.1's allowed list per plan §"Subagent Guardrails"). The format
/// is RFC 3339-compatible: "YYYY-MM-DDTHH:MM:SSZ".
fn chrono_like_utc_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Convert Unix seconds to "YYYY-MM-DDTHH:MM:SSZ" via a small inline formatter.
    // This avoids pulling in chrono. Algorithm: standard civil-from-days.
    let (year, month, day, hour, min, sec) = unix_seconds_to_ymdhms(secs);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hour, min, sec
    )
}

/// Convert Unix epoch seconds (UTC) to (year, month, day, hour, min, sec).
/// Algorithm: Howard Hinnant's date library "civil_from_days" (public domain).
fn unix_seconds_to_ymdhms(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (secs / 86_400) as i64;
    let secs_of_day = (secs % 86_400) as u32;
    let hour = secs_of_day / 3600;
    let min = (secs_of_day % 3600) / 60;
    let sec = secs_of_day % 60;

    // civil_from_days: returns (year, month, day) for the given days since 1970-01-01.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d, hour, min, sec)
}

#[cfg(test)]
mod local_tests {
    use super::*;

    #[test]
    fn test_unix_seconds_to_ymdhms_epoch() {
        let (y, mo, d, h, mi, s) = unix_seconds_to_ymdhms(0);
        assert_eq!((y, mo, d, h, mi, s), (1970, 1, 1, 0, 0, 0));
    }

    #[test]
    fn test_unix_seconds_to_ymdhms_known_values() {
        // Verifiable via `date -u -d @<seconds>`. Pinned values:
        //   1_700_000_000 → 2023-11-14T22:13:20Z
        //   1_750_000_000 → 2025-06-15T16:53:20Z
        // If a subagent updates these, verify with: `date -u -d @1700000000`.
        let (y, mo, d, h, mi, s) = unix_seconds_to_ymdhms(1_700_000_000);
        assert_eq!((y, mo, d, h, mi, s), (2023, 11, 14, 22, 13, 20));
        let (y, mo, d, h, mi, s) = unix_seconds_to_ymdhms(1_750_000_000);
        assert_eq!((y, mo, d, h, mi, s), (2025, 6, 15, 16, 53, 20));
    }

    #[test]
    fn test_unix_seconds_to_ymdhms_handles_leap_year() {
        // 2024-02-29T00:00:00Z = 1709164800. Leap-year boundary;
        // catches off-by-one in civil_from_days.
        let (y, mo, d, h, mi, s) = unix_seconds_to_ymdhms(1_709_164_800);
        assert_eq!((y, mo, d, h, mi, s), (2024, 2, 29, 0, 0, 0));
    }
}
```

- [ ] **Step 1.2.2: Register the module in `src-tauri/src/lib.rs`**

Locate the existing module declarations in `src-tauri/src/lib.rs` and add:

```rust
pub mod dashboard;
```

Then in the `tauri::Builder::default().invoke_handler(tauri::generate_handler![...])` registration block (look for the existing handler list — Task 2 / Task 3 / Task 5 entries should already be there), append:

```rust
dashboard::dashboard_read,
dashboard::status_read,
```

If `lib.rs` does NOT yet have an `invoke_handler` block (i.e., the existing tasks haven't landed yet — verify via `git log feat/v0.0.1 --oneline | head -20`), add the registration via the standard Tauri builder pattern in whatever entry point currently exists (`main.rs` for the binary, or `run()` in `lib.rs`). Do NOT invent a new entry point; use the existing one.

- [ ] **Step 1.2.3: Run the tests to verify green**

```bash
cd src-tauri
cargo test --test dashboard_test
cargo test --lib dashboard::local_tests
```

Expected: 4 tests pass in `dashboard_test`, 2 tests pass in `local_tests`. If `cargo` complains about a missing handler registration (Tauri macro), confirm the `invoke_handler` block has both new commands.

- [ ] **Step 1.2.4: Verify the build still compiles end-to-end**

```bash
cd src-tauri
cargo build
```

Expected: clean build, no warnings about unused imports.

### Task 1.3 — Commit Phase 1

- [ ] **Step 1.3.1: Stage + commit**

```bash
git add src-tauri/src/dashboard.rs src-tauri/src/lib.rs src-tauri/tests/dashboard_test.rs
git commit -m "$(cat <<'EOF'
feat(shell): Task 16 Phase 1 — dashboard_read + status_read Tauri commands

v0.0.1 stub backend for the dashboard ribbon + minimal status bar
per AMD-8 / docs/design/v0.0.1-ux-mockups.md §5.9.

DashboardSnapshot ships both grid_local (full 6-char) and grid_broadcast
(4-char default) per Principle 7 + RADIO-2; precision-reduction is the
default. Real Pat-session-state tracking is v0.1+; v0.0.1 returns
Idle and stubbed identity fields.

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

Substitute `<SESSION-MONIKER>` with the moniker from `python3 .claude/scripts/get_agent_moniker.py`. Confirm `git log -1` shows the trailer.

### Phase 1 completion check

**Before marking Phase 1 ✅ SHIPPED:**

1. Re-read `docs/pitfalls/testing-pitfalls.md` §3 (error paths) + §6 (defaults). Confirm: snapshot tests cover the offline-path-identifier case and the in-session-with-detail case (error-adjacent shapes). The `unix_seconds_to_ymdhms` helper is tested with the epoch (boundary) + a known value (sanity).
2. `cd src-tauri && cargo test --test dashboard_test && cargo test --lib dashboard::local_tests` → all green.
3. `cargo build` → clean.
4. Update the Living Document Contract banner: Phase 1 → ✅ SHIPPED at `<SHA>` on `<YYYY-MM-DD>`.

---

## Phase 2 — Frontend pure derivation helpers (TS + vitest)

**Execution Status:** ⬜ NOT STARTED

**Goal of this phase:** Pure, unit-testable TypeScript helpers that turn a `DashboardSnapshot` / `StatusSnapshot` into the user-visible field strings. No DOM, no Tauri imports — `vitest` tests run in node. This isolates the **precision-reduction default** and the **transport-naming invariant** into testable functions, so reviewers can verify Principle 7 + §4.1 compliance without running the GUI.

**Files:**
- Create: `src/shell/types.ts`
- Create: `src/shell/dashboard.ts`
- Create: `src/shell/dashboard.test.ts`
- Create: `src/shell/status.ts`
- Create: `src/shell/status.test.ts`

**Ordering dependencies:** Independent of Phase 1 (no backend call). Can be developed in parallel. MUST complete before Phase 3 (component consumes these helpers).

### Task 2.1 — Set up vitest if not already present

- [ ] **Step 2.1.1: Verify vitest is in `devDependencies`**

```bash
grep -E '"vitest"|"@testing-library/react"|"@testing-library/jest-dom"|"jsdom"' package.json
```

If vitest is absent, install:

```bash
pnpm add -D vitest @testing-library/react @testing-library/jest-dom jsdom @types/node
```

**Cross-task race note:** Tasks 12, 13, 15, 16.5 will also need vitest if they ship tests. Two parallel implementers may both try to add it to `package.json`. The first to merge wins; the second will find vitest already present at rebase time and SHOULD verify the merged config is compatible (the configs above are minimal and conflict-free with what other tasks will need). If the merged config diverges (e.g., different `environment` value, missing `setupFiles`), DO NOT overwrite — `git merge`/rebase will surface the conflict; resolve by keeping the union of settings.

Add to `package.json` `scripts`:

```json
"test": "vitest run",
"test:watch": "vitest"
```

Add `vitest.config.ts` at the repo root if not present:

```ts
import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./vitest.setup.ts'],
  },
});
```

Add `vitest.setup.ts`:

```ts
import '@testing-library/jest-dom/vitest';
```

If `vitest.config.ts` and the scripts already exist (a prior task may have added them), do NOT overwrite — just verify they include the above settings.

- [ ] **Step 2.1.2: Verify the test runner works on an empty suite**

```bash
pnpm test
```

Expected: "No test files found" or 0 tests passed. If vitest fails to start, fix that BEFORE writing tests.

### Task 2.2 — Define shared TypeScript types

- [ ] **Step 2.2.1: Create `src/shell/types.ts`**

```ts
// src/shell/types.ts
//
// Mirror of src-tauri/src/dashboard.rs types. Keep field names + enum
// variants in EXACT sync — these cross the Tauri IPC boundary as JSON.

export type ConnectionState =
  | 'Idle'
  | 'Connecting'
  | 'InSession'
  | 'Disconnecting'
  | 'Disconnected';

export type GpsState =
  | 'Off'
  | 'LocalUiOnly'
  | 'BroadcastAtPrecision'
  | 'Searching'
  | 'Manual';

export type PositionPrecision = 'FourCharGrid' | 'SixCharGrid';

export type CmsTransport = 'CmsSsl' | 'Telnet';

export interface DashboardSnapshot {
  callsign: string | null;
  identifier: string | null;
  grid_local: string | null;
  grid_broadcast: string | null;
  gps_state: GpsState;
  utc_iso: string;
  local_iso: string;
  local_tz_abbrev: string;
  connection_state: ConnectionState;
  transport: CmsTransport;
  connection_detail: string | null;
}

export interface StatusSnapshot {
  last_action: string | null;
  last_action_label: string | null;
  pane_focus: string | null;
}

/// Subset of the operator's privacy config the dashboard cares about.
/// In v0.0.1, this is read once at app boot from the config file.
export interface DashboardPrivacyConfig {
  position_precision: PositionPrecision;
}
```

### Task 2.3 — Write failing tests for dashboard derivation helpers

- [ ] **Step 2.3.1: Create `src/shell/dashboard.test.ts`**

```ts
// src/shell/dashboard.test.ts
import { describe, it, expect } from 'vitest';
import {
  deriveCallsignField,
  deriveGridField,
  deriveGpsField,
  deriveTimeFields,
  deriveConnectionField,
} from './dashboard';
import type { DashboardSnapshot, DashboardPrivacyConfig } from './types';

const baseSnap: DashboardSnapshot = {
  callsign: 'W4PHS',
  identifier: null,
  grid_local: 'EM75xx',
  grid_broadcast: 'EM75',
  gps_state: 'Manual',
  utc_iso: '2026-05-18T14:32:00Z',
  local_iso: '2026-05-18T09:32:00-05:00',
  local_tz_abbrev: 'CDT',
  connection_state: 'Idle',
  transport: 'CmsSsl',
  connection_detail: null,
};

describe('deriveCallsignField', () => {
  it('prefers callsign over identifier when present', () => {
    expect(deriveCallsignField(baseSnap)).toBe('W4PHS');
  });

  it('falls back to identifier for offline-mode installs', () => {
    const s = { ...baseSnap, callsign: null, identifier: 'EOC-1' };
    expect(deriveCallsignField(s)).toBe('EOC-1');
  });

  it('renders an empty-state placeholder when neither is set', () => {
    const s = { ...baseSnap, callsign: null, identifier: null };
    // Empty state: do NOT render "null" or "undefined"; render a stable placeholder.
    expect(deriveCallsignField(s)).toBe('—');
  });

  it('trims surrounding whitespace defensively', () => {
    const s = { ...baseSnap, callsign: '  W4PHS  ' };
    expect(deriveCallsignField(s)).toBe('W4PHS');
  });
});

describe('deriveGridField', () => {
  const defaultPrivacy: DashboardPrivacyConfig = { position_precision: 'FourCharGrid' };
  const sixCharPrivacy: DashboardPrivacyConfig = { position_precision: 'SixCharGrid' };

  it('renders 4-char broadcast precision BY DEFAULT (Principle 7 + RADIO-2)', () => {
    const result = deriveGridField(baseSnap, defaultPrivacy);
    expect(result.display).toBe('EM75');
    expect(result.tooltip).toBe('Local: EM75xx · Broadcast: EM75 (reduced)');
  });

  it('renders 6-char full precision only when operator opts in', () => {
    const result = deriveGridField(baseSnap, sixCharPrivacy);
    expect(result.display).toBe('EM75xx');
    expect(result.tooltip).toBe('Local: EM75xx · Broadcast: EM75xx (high precision)');
  });

  it('falls back to broadcast string when local is null (defensive)', () => {
    const s = { ...baseSnap, grid_local: null, grid_broadcast: 'EM75' };
    const result = deriveGridField(s, defaultPrivacy);
    expect(result.display).toBe('EM75');
  });

  it('renders empty-state placeholder when grid is unset', () => {
    const s = { ...baseSnap, grid_local: null, grid_broadcast: null };
    const result = deriveGridField(s, defaultPrivacy);
    expect(result.display).toBe('—');
    expect(result.tooltip).toBe('No grid configured');
  });

  it('CRITICAL: a 6-char grid_local MUST render as 4-char under default privacy', () => {
    // This is the load-bearing Principle 7 assertion. If this regresses,
    // the operator is broadcasting higher precision than intended.
    const s = { ...baseSnap, grid_local: 'EM75ab', grid_broadcast: 'EM75' };
    const result = deriveGridField(s, defaultPrivacy);
    expect(result.display).toBe('EM75');
    expect(result.display.length).toBe(4);
  });

  it('rejects malformed grid_broadcast longer than grid_local (defensive)', () => {
    // grid_broadcast SHOULD be <= grid_local in length. If backend
    // somehow ships a longer broadcast, clamp to grid_local to avoid
    // accidentally widening precision.
    const s = { ...baseSnap, grid_local: 'EM75', grid_broadcast: 'EM75xx' };
    const result = deriveGridField(s, defaultPrivacy);
    expect(result.display).toBe('EM75');
  });
});

describe('deriveGpsField', () => {
  it('renders Manual state', () => {
    const r = deriveGpsField({ ...baseSnap, gps_state: 'Manual' });
    expect(r.label).toBe('GPS: manual');
    expect(r.indicator).toBe('neutral');
  });

  it('renders Off state', () => {
    const r = deriveGpsField({ ...baseSnap, gps_state: 'Off' });
    expect(r.label).toBe('GPS: off');
    expect(r.indicator).toBe('off');
  });

  it('renders Searching state with yellow indicator', () => {
    const r = deriveGpsField({ ...baseSnap, gps_state: 'Searching' });
    expect(r.label).toBe('GPS: searching');
    expect(r.indicator).toBe('searching');
  });

  it('renders BroadcastAtPrecision as "GPS: on" with green indicator', () => {
    const r = deriveGpsField({ ...baseSnap, gps_state: 'BroadcastAtPrecision' });
    expect(r.label).toBe('GPS: on');
    expect(r.indicator).toBe('on');
  });

  it('renders LocalUiOnly with explicit "local only" wording', () => {
    const r = deriveGpsField({ ...baseSnap, gps_state: 'LocalUiOnly' });
    expect(r.label).toBe('GPS: local only');
    expect(r.indicator).toBe('on');
  });
});

describe('deriveTimeFields', () => {
  it('formats UTC + local with abbrev', () => {
    const r = deriveTimeFields(baseSnap);
    expect(r.utc).toBe('14:32 UTC');
    expect(r.local).toBe('09:32 CDT');
    expect(r.combined).toBe('14:32 UTC · 09:32 CDT');
  });

  it('handles missing tz abbrev gracefully', () => {
    const s = { ...baseSnap, local_tz_abbrev: '' };
    const r = deriveTimeFields(s);
    expect(r.local).toBe('09:32');
  });

  it('handles missing seconds-precision in iso (HH:MM only)', () => {
    const s = {
      ...baseSnap,
      utc_iso: '2026-05-18T14:32Z',
      local_iso: '2026-05-18T09:32-05:00',
    };
    const r = deriveTimeFields(s);
    expect(r.utc).toBe('14:32 UTC');
    expect(r.local).toBe('09:32 CDT');
  });

  it('renders placeholders for malformed iso (defensive)', () => {
    const s = { ...baseSnap, utc_iso: 'not-a-date', local_iso: 'also-not' };
    const r = deriveTimeFields(s);
    expect(r.utc).toBe('—');
    expect(r.local).toBe('—');
  });
});

describe('deriveConnectionField', () => {
  it('renders Idle state', () => {
    const r = deriveConnectionField({ ...baseSnap, connection_state: 'Idle' });
    expect(r.label).toBe('Idle');
  });

  it('CRITICAL: Connecting state names the transport (§4.1)', () => {
    const r = deriveConnectionField({
      ...baseSnap,
      connection_state: 'Connecting',
      transport: 'CmsSsl',
    });
    expect(r.label).toBe('Connecting via CMS-SSL...');
    expect(r.label).toContain('CMS-SSL');
  });

  it('CRITICAL: Connecting via Telnet names Telnet explicitly', () => {
    const r = deriveConnectionField({
      ...baseSnap,
      connection_state: 'Connecting',
      transport: 'Telnet',
    });
    expect(r.label).toBe('Connecting via Telnet...');
    expect(r.label).toContain('Telnet');
  });

  it('CRITICAL: InSession includes transport + detail', () => {
    const r = deriveConnectionField({
      ...baseSnap,
      connection_state: 'InSession',
      transport: 'CmsSsl',
      connection_detail: 'W4PHS<->cms.winlink.org',
    });
    expect(r.label).toBe('In session via CMS-SSL (W4PHS<->cms.winlink.org)');
    expect(r.label).toContain('CMS-SSL');
  });

  it('InSession without detail still names transport', () => {
    const r = deriveConnectionField({
      ...baseSnap,
      connection_state: 'InSession',
      transport: 'Telnet',
      connection_detail: null,
    });
    expect(r.label).toBe('In session via Telnet');
    expect(r.label).toContain('Telnet');
  });

  it('Disconnected state includes "at <utc-time>"', () => {
    const r = deriveConnectionField({
      ...baseSnap,
      connection_state: 'Disconnected',
    });
    expect(r.label).toBe('Disconnected at 14:32 UTC');
  });

  it('CRITICAL: connection label NEVER omits the transport (transport-visibility invariant)', () => {
    // Exhaustive check: every non-Idle/Disconnected state must include the transport.
    const states: Array<DashboardSnapshot['connection_state']> = [
      'Connecting', 'InSession', 'Disconnecting',
    ];
    for (const state of states) {
      for (const t of ['CmsSsl', 'Telnet'] as const) {
        const r = deriveConnectionField({
          ...baseSnap,
          connection_state: state,
          transport: t,
        });
        const transportName = t === 'CmsSsl' ? 'CMS-SSL' : 'Telnet';
        expect(r.label, `${state}/${t}`).toContain(transportName);
      }
    }
  });
});
```

- [ ] **Step 2.3.2: Run the test to confirm failure**

```bash
pnpm test
```

Expected: import errors — the `./dashboard` module doesn't exist yet. Red.

### Task 2.4 — Implement `src/shell/dashboard.ts`

- [ ] **Step 2.4.1: Create `src/shell/dashboard.ts`**

```ts
// src/shell/dashboard.ts
//
// Pure derivation helpers for the dashboard ribbon. No DOM, no Tauri imports.
// Tests live in dashboard.test.ts; the component (Phase 3) consumes these.
//
// Principle 7 + RADIO-2 invariant: the grid field renders PRECISION-REDUCED
// (4-char Maidenhead) by default. The 6-character display is opt-in via
// privacy.position_precision = 'SixCharGrid'. If you find yourself widening
// this default, STOP — Principle 7 is load-bearing for operator privacy.
//
// Tension resolution: Principle 7 says "Local UI always shows full GPS
// precision when available." Design doc §5.9 (canonical per-task spec for
// Task 16) says the ribbon shows 4-char broadcast precision with a tooltip
// exposing the 6-char local value. §5.9 wins (per the design doc's
// "If a Task's plan section disagrees with the specs here, this doc wins"
// authority statement). The operator's full-precision value is exposed
// in the tooltip, satisfying the spirit of Principle 7 without making the
// ribbon look different from what gets broadcast — a deliberate UX choice
// to reinforce "what you see is what your peers see."
//
// §4.1 transport-visibility invariant: the connection field ALWAYS names
// the transport (CMS-SSL or Telnet) in non-Idle states. Hiding the
// transport reproduces the Express anti-pattern this project exists to fix.

import type {
  DashboardSnapshot,
  DashboardPrivacyConfig,
  ConnectionState,
  GpsState,
} from './types';

const EMPTY = '—';

export function deriveCallsignField(snap: DashboardSnapshot): string {
  const cs = snap.callsign?.trim();
  if (cs && cs.length > 0) return cs;
  const id = snap.identifier?.trim();
  if (id && id.length > 0) return id;
  return EMPTY;
}

export interface GridFieldResult {
  display: string;
  tooltip: string;
}

export function deriveGridField(
  snap: DashboardSnapshot,
  privacy: DashboardPrivacyConfig,
): GridFieldResult {
  const local = snap.grid_local?.trim() ?? '';
  const broadcast = snap.grid_broadcast?.trim() ?? '';

  // Empty state.
  if (local.length === 0 && broadcast.length === 0) {
    return { display: EMPTY, tooltip: 'No grid configured' };
  }

  if (privacy.position_precision === 'SixCharGrid') {
    // Operator opted in to high precision; display whatever the local grid is
    // (typically 6-char). The backend's grid_broadcast field will mirror
    // grid_local in this mode but we use local to avoid double-truncation bugs.
    const display = local.length > 0 ? local : broadcast;
    return {
      display,
      tooltip: `Local: ${local || display} · Broadcast: ${display} (high precision)`,
    };
  }

  // DEFAULT (FourCharGrid): precision-reduced. Use broadcast field directly,
  // but defensively clamp to the SHORTER of (broadcast, local) — broadcast
  // should never be longer than local; if the backend ships a longer
  // broadcast, treat it as a bug and use local to avoid widening precision.
  let display: string;
  if (broadcast.length > 0 && local.length > 0) {
    display = broadcast.length <= local.length ? broadcast : local;
  } else if (broadcast.length > 0) {
    display = broadcast;
  } else {
    // local-only: derive 4-char from local.
    display = local.slice(0, 4);
  }

  return {
    display,
    tooltip: `Local: ${local || display} · Broadcast: ${display} (reduced)`,
  };
}

export type GpsIndicator = 'on' | 'off' | 'searching' | 'neutral';

export interface GpsFieldResult {
  label: string;
  indicator: GpsIndicator;
}

export function deriveGpsField(snap: DashboardSnapshot): GpsFieldResult {
  const state: GpsState = snap.gps_state;
  switch (state) {
    case 'Off':
      return { label: 'GPS: off', indicator: 'off' };
    case 'Manual':
      return { label: 'GPS: manual', indicator: 'neutral' };
    case 'Searching':
      return { label: 'GPS: searching', indicator: 'searching' };
    case 'LocalUiOnly':
      return { label: 'GPS: local only', indicator: 'on' };
    case 'BroadcastAtPrecision':
      return { label: 'GPS: on', indicator: 'on' };
    default: {
      // Exhaustiveness check — if a new GpsState is added without updating
      // this switch, TypeScript will flag the `_unreachable` line.
      const _unreachable: never = state;
      return { label: 'GPS: ?', indicator: 'neutral' };
    }
  }
}

export interface TimeFieldsResult {
  utc: string;
  local: string;
  combined: string;
}

/**
 * Extract HH:MM from an ISO-8601 timestamp. Returns null on parse failure.
 * Accepts both "HH:MM:SSZ" and "HH:MMZ" forms.
 */
function isoHhMm(iso: string): string | null {
  // Match "T" followed by HH:MM
  const m = iso.match(/T(\d{2}):(\d{2})/);
  if (!m) return null;
  return `${m[1]}:${m[2]}`;
}

export function deriveTimeFields(snap: DashboardSnapshot): TimeFieldsResult {
  const utcHm = isoHhMm(snap.utc_iso);
  const localHm = isoHhMm(snap.local_iso);
  const utc = utcHm ? `${utcHm} UTC` : EMPTY;
  const tz = snap.local_tz_abbrev?.trim() ?? '';
  const local = localHm ? (tz.length > 0 ? `${localHm} ${tz}` : localHm) : EMPTY;
  const combined = utc === EMPTY && local === EMPTY ? EMPTY : `${utc} · ${local}`;
  return { utc, local, combined };
}

export interface ConnectionFieldResult {
  label: string;
}

export function deriveConnectionField(snap: DashboardSnapshot): ConnectionFieldResult {
  const transportName = snap.transport === 'CmsSsl' ? 'CMS-SSL' : 'Telnet';
  const state: ConnectionState = snap.connection_state;

  switch (state) {
    case 'Idle':
      return { label: 'Idle' };
    case 'Connecting':
      return { label: `Connecting via ${transportName}...` };
    case 'InSession': {
      const detail = snap.connection_detail?.trim();
      if (detail && detail.length > 0) {
        return { label: `In session via ${transportName} (${detail})` };
      }
      return { label: `In session via ${transportName}` };
    }
    case 'Disconnecting':
      return { label: `Disconnecting via ${transportName}...` };
    case 'Disconnected': {
      const utcHm = isoHhMm(snap.utc_iso);
      return { label: utcHm ? `Disconnected at ${utcHm} UTC` : 'Disconnected' };
    }
    default: {
      const _unreachable: never = state;
      return { label: 'Unknown' };
    }
  }
}
```

- [ ] **Step 2.4.2: Run the tests to verify green**

```bash
pnpm test src/shell/dashboard.test.ts
```

Expected: all 25+ tests pass. If a test fails, fix the helper — do NOT weaken the test. The Principle-7 + transport-visibility tests are load-bearing assertions.

### Task 2.5 — Status derivation helpers

- [ ] **Step 2.5.1: Create `src/shell/status.test.ts`**

```ts
// src/shell/status.test.ts
import { describe, it, expect } from 'vitest';
import { deriveActivityLabel, deriveWindowInfoLabel } from './status';
import type { StatusSnapshot } from './types';

const empty: StatusSnapshot = {
  last_action: null,
  last_action_label: null,
  pane_focus: null,
};

describe('deriveActivityLabel', () => {
  it('renders empty-state placeholder when no last action', () => {
    expect(deriveActivityLabel(empty)).toBe('Ready');
  });

  it('renders last action with HH:MM UTC + label', () => {
    const s: StatusSnapshot = {
      last_action: '2026-05-18T14:32:00Z',
      last_action_label: 'Toggled session log',
      pane_focus: null,
    };
    expect(deriveActivityLabel(s)).toBe('14:32 UTC · Toggled session log');
  });

  it('renders just the timestamp when label is null', () => {
    const s: StatusSnapshot = {
      last_action: '2026-05-18T14:32:00Z',
      last_action_label: null,
      pane_focus: null,
    };
    expect(deriveActivityLabel(s)).toBe('14:32 UTC');
  });

  it('renders just the label when timestamp is malformed', () => {
    const s: StatusSnapshot = {
      last_action: 'not-a-date',
      last_action_label: 'Toggled session log',
      pane_focus: null,
    };
    expect(deriveActivityLabel(s)).toBe('Toggled session log');
  });
});

describe('deriveWindowInfoLabel', () => {
  it('renders empty when no pane focus', () => {
    expect(deriveWindowInfoLabel(empty)).toBe('');
  });

  it('renders the pane focus label capitalized', () => {
    const s: StatusSnapshot = {
      last_action: null,
      last_action_label: null,
      pane_focus: 'inbox',
    };
    expect(deriveWindowInfoLabel(s)).toBe('Inbox');
  });

  it('handles multi-word pane focus', () => {
    const s: StatusSnapshot = {
      last_action: null,
      last_action_label: null,
      pane_focus: 'reading-pane',
    };
    expect(deriveWindowInfoLabel(s)).toBe('Reading pane');
  });
});
```

- [ ] **Step 2.5.2: Run to confirm failure**

```bash
pnpm test src/shell/status.test.ts
```

Expected: import error — module not found. Red.

- [ ] **Step 2.5.3: Create `src/shell/status.ts`**

```ts
// src/shell/status.ts
//
// Pure derivation for the bottom status bar (Mail.app-style minimal,
// app-chrome only per AMD-8). NO operator-relevant state lives here —
// callsign/grid/GPS/connection live in the dashboard ribbon above.

import type { StatusSnapshot } from './types';

function isoHhMm(iso: string): string | null {
  const m = iso.match(/T(\d{2}):(\d{2})/);
  return m ? `${m[1]}:${m[2]}` : null;
}

function capitalize(s: string): string {
  if (s.length === 0) return s;
  // Convert kebab-case or snake_case to "First word rest words" style.
  const normalized = s.replace(/[-_]/g, ' ').toLowerCase();
  return normalized.charAt(0).toUpperCase() + normalized.slice(1);
}

export function deriveActivityLabel(snap: StatusSnapshot): string {
  const ts = snap.last_action ? isoHhMm(snap.last_action) : null;
  const label = snap.last_action_label?.trim() ?? '';

  if (ts && label.length > 0) return `${ts} UTC · ${label}`;
  if (ts) return `${ts} UTC`;
  if (label.length > 0) return label;
  return 'Ready';
}

export function deriveWindowInfoLabel(snap: StatusSnapshot): string {
  const focus = snap.pane_focus?.trim() ?? '';
  if (focus.length === 0) return '';
  return capitalize(focus);
}
```

- [ ] **Step 2.5.4: Run to verify green**

```bash
pnpm test src/shell/
```

Expected: all dashboard.test.ts + status.test.ts tests pass.

### Task 2.6 — Commit Phase 2

- [ ] **Step 2.6.1: Stage + commit**

```bash
git add src/shell/types.ts src/shell/dashboard.ts src/shell/dashboard.test.ts src/shell/status.ts src/shell/status.test.ts package.json vitest.config.ts vitest.setup.ts
git commit -m "$(cat <<'EOF'
feat(shell): Task 16 Phase 2 — pure TS derivation helpers + vitest

dashboard.ts + status.ts as pure unit-testable helpers per AMD-8.
The Principle-7 grid precision-reduction default and the §4.1
transport-visibility invariant are isolated in deriveGridField and
deriveConnectionField; their tests are load-bearing — never weaken.

types.ts mirrors src-tauri/src/dashboard.rs over the JSON IPC.

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

If `package.json` / `vitest.config.ts` / `vitest.setup.ts` already existed (you only verified, didn't modify), drop them from the `git add` list.

### Phase 2 completion check

**Before marking Phase 2 ✅ SHIPPED:**

1. Re-read `docs/pitfalls/testing-pitfalls.md` §4 (negative property testing) — confirm: empty-string and null inputs covered for callsign, grid, identifier; oversized inputs (long callsign) handled gracefully (current code trims; oversized doesn't crash).
2. **Critical Principle-7 verification:** `pnpm test src/shell/dashboard.test.ts -t "CRITICAL: a 6-char grid_local MUST render as 4-char"` → passes. This is the load-bearing assertion for Principle 7. If this fails or is removed, the operator broadcasts higher precision than configured.
3. **Critical transport-visibility verification:** `pnpm test src/shell/dashboard.test.ts -t "CRITICAL: connection label NEVER omits the transport"` → passes. This is the §4.1 invariant.
4. `pnpm test src/shell/` → all green.
5. Update Living Document Contract: Phase 2 → ✅ SHIPPED at `<SHA>` on `<YYYY-MM-DD>`.

---

## Phase 3 — DashboardRibbon component

**Execution Status:** ⬜ NOT STARTED

**Goal of this phase:** React component rendering the ribbon at the top of the main window. Polls the backend every 5s via TanStack Query, reads privacy config once at mount, composes the field strings via the Phase-2 derivation helpers, renders them as a horizontal 5-segment strip.

**Files:**
- Create: `src/shell/useDashboard.ts`
- Create: `src/shell/DashboardRibbon.tsx`
- Create: `src/shell/DashboardRibbon.test.tsx`

**Ordering dependencies:** Requires Phase 1 (backend command) + Phase 2 (derivation helpers).

### Task 3.1 — TanStack Query hook for `dashboard_read`

- [ ] **Step 3.1.1: Create `src/shell/useDashboard.ts`**

```ts
// src/shell/useDashboard.ts
import { useQuery } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import type { DashboardSnapshot } from './types';

export function useDashboard() {
  return useQuery<DashboardSnapshot>({
    queryKey: ['dashboard'],
    queryFn: () => invoke<DashboardSnapshot>('dashboard_read'),
    refetchInterval: 5000,
    // Keep showing stale data while refetching to avoid flicker:
    placeholderData: (prev) => prev,
  });
}
```

There is no test file for this hook by itself — TanStack Query hooks are tested via the consuming component (Phase 3.3). Direct hook tests would require a QueryClientProvider wrapper which adds boilerplate without catching real bugs that the component-level tests don't already catch.

### Task 3.2 — Write failing tests for `DashboardRibbon`

- [ ] **Step 3.2.1: Create `src/shell/DashboardRibbon.test.tsx`**

```tsx
// src/shell/DashboardRibbon.test.tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { DashboardRibbon } from './DashboardRibbon';
import type { DashboardSnapshot, DashboardPrivacyConfig } from './types';

// Mock the Tauri core invoke so the component can run in jsdom without a backend.
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

import { invoke } from '@tauri-apps/api/core';
const mockInvoke = invoke as unknown as ReturnType<typeof vi.fn>;

function renderWithClient(privacy: DashboardPrivacyConfig) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <DashboardRibbon privacy={privacy} />
    </QueryClientProvider>,
  );
}

const baseSnap: DashboardSnapshot = {
  callsign: 'W4PHS',
  identifier: null,
  grid_local: 'EM75xx',
  grid_broadcast: 'EM75',
  gps_state: 'BroadcastAtPrecision',
  utc_iso: '2026-05-18T14:32:00Z',
  local_iso: '2026-05-18T09:32:00-05:00',
  local_tz_abbrev: 'CDT',
  connection_state: 'Idle',
  transport: 'CmsSsl',
  connection_detail: null,
};

beforeEach(() => {
  mockInvoke.mockReset();
});

describe('DashboardRibbon', () => {
  it('renders the loading state before the first snapshot arrives', () => {
    mockInvoke.mockReturnValue(new Promise(() => { /* never resolves */ }));
    renderWithClient({ position_precision: 'FourCharGrid' });
    // The ribbon should render placeholders for unknown fields, not blank space.
    // Use a data-testid on the ribbon root so we can assert it mounted.
    expect(screen.getByTestId('dashboard-ribbon')).toBeInTheDocument();
  });

  it('renders all 5 fields when snapshot arrives', async () => {
    mockInvoke.mockResolvedValue(baseSnap);
    renderWithClient({ position_precision: 'FourCharGrid' });

    await waitFor(() => {
      expect(screen.getByTestId('field-callsign')).toHaveTextContent('W4PHS');
    });
    expect(screen.getByTestId('field-grid')).toHaveTextContent('EM75');   // 4-char default
    expect(screen.getByTestId('field-gps')).toHaveTextContent('GPS: on');
    expect(screen.getByTestId('field-time')).toHaveTextContent('14:32 UTC · 09:32 CDT');
    expect(screen.getByTestId('field-connection')).toHaveTextContent('Idle');
  });

  it('CRITICAL: renders 4-char grid by default even when local grid is 6-char (Principle 7)', async () => {
    mockInvoke.mockResolvedValue({ ...baseSnap, grid_local: 'EM75ab', grid_broadcast: 'EM75' });
    renderWithClient({ position_precision: 'FourCharGrid' });

    await waitFor(() => {
      expect(screen.getByTestId('field-grid')).toHaveTextContent('EM75');
    });
    expect(screen.getByTestId('field-grid')).not.toHaveTextContent('EM75ab');
  });

  it('renders 6-char grid only when operator opted in', async () => {
    mockInvoke.mockResolvedValue({ ...baseSnap, grid_local: 'EM75ab', grid_broadcast: 'EM75ab' });
    renderWithClient({ position_precision: 'SixCharGrid' });

    await waitFor(() => {
      expect(screen.getByTestId('field-grid')).toHaveTextContent('EM75ab');
    });
  });

  it('CRITICAL: connection field names the transport in non-Idle states (§4.1)', async () => {
    mockInvoke.mockResolvedValue({
      ...baseSnap,
      connection_state: 'InSession',
      transport: 'CmsSsl',
      connection_detail: 'W4PHS<->cms.winlink.org',
    });
    renderWithClient({ position_precision: 'FourCharGrid' });

    await waitFor(() => {
      expect(screen.getByTestId('field-connection')).toHaveTextContent('In session via CMS-SSL');
    });
    expect(screen.getByTestId('field-connection')).toHaveTextContent('W4PHS<->cms.winlink.org');
  });

  it('CRITICAL: Telnet transport is named explicitly (no hiding)', async () => {
    mockInvoke.mockResolvedValue({
      ...baseSnap,
      connection_state: 'Connecting',
      transport: 'Telnet',
    });
    renderWithClient({ position_precision: 'FourCharGrid' });

    await waitFor(() => {
      expect(screen.getByTestId('field-connection')).toHaveTextContent('Telnet');
    });
  });

  it('falls back to identifier in offline-mode installs (no callsign)', async () => {
    mockInvoke.mockResolvedValue({ ...baseSnap, callsign: null, identifier: 'EOC-1' });
    renderWithClient({ position_precision: 'FourCharGrid' });

    await waitFor(() => {
      expect(screen.getByTestId('field-callsign')).toHaveTextContent('EOC-1');
    });
  });

  it('connection field has aria-live for screen-reader announcements', async () => {
    mockInvoke.mockResolvedValue(baseSnap);
    renderWithClient({ position_precision: 'FourCharGrid' });

    await waitFor(() => {
      const conn = screen.getByTestId('field-connection');
      expect(conn).toHaveAttribute('aria-live', 'polite');
    });
  });

  it('grid field tooltip exposes local + broadcast precision', async () => {
    mockInvoke.mockResolvedValue(baseSnap);
    renderWithClient({ position_precision: 'FourCharGrid' });

    await waitFor(() => {
      const grid = screen.getByTestId('field-grid');
      expect(grid).toHaveAttribute('title');
      expect(grid.getAttribute('title')).toContain('EM75xx');     // local
      expect(grid.getAttribute('title')).toContain('reduced');
    });
  });

  it('renders placeholder dashes when backend returns empty fields', async () => {
    mockInvoke.mockResolvedValue({
      callsign: null,
      identifier: null,
      grid_local: null,
      grid_broadcast: null,
      gps_state: 'Off',
      utc_iso: '',
      local_iso: '',
      local_tz_abbrev: '',
      connection_state: 'Idle',
      transport: 'CmsSsl',
      connection_detail: null,
    });
    renderWithClient({ position_precision: 'FourCharGrid' });

    await waitFor(() => {
      expect(screen.getByTestId('field-callsign')).toHaveTextContent('—');
    });
    expect(screen.getByTestId('field-grid')).toHaveTextContent('—');
  });
});
```

- [ ] **Step 3.2.2: Run to confirm failure**

```bash
pnpm test src/shell/DashboardRibbon.test.tsx
```

Expected: import error — component doesn't exist. Red.

### Task 3.3 — Implement `DashboardRibbon.tsx`

- [ ] **Step 3.3.1: Create `src/shell/DashboardRibbon.tsx`**

```tsx
// src/shell/DashboardRibbon.tsx
//
// Top-of-window dashboard ribbon per AMD-8 / design doc §5.9.
// 5 fields: callsign · grid · GPS · UTC+local · connection.
//
// Per Principle 7: the grid field renders precision-reduced (4-char) BY
// DEFAULT; full 6-char only when privacy.position_precision = 'SixCharGrid'.
// Per §4.1: the connection field always names the transport in non-Idle
// states.

import {
  deriveCallsignField,
  deriveGridField,
  deriveGpsField,
  deriveTimeFields,
  deriveConnectionField,
} from './dashboard';
import { useDashboard } from './useDashboard';
import type { DashboardPrivacyConfig } from './types';

export interface DashboardRibbonProps {
  privacy: DashboardPrivacyConfig;
}

export function DashboardRibbon({ privacy }: DashboardRibbonProps) {
  const { data: snap } = useDashboard();

  // Render placeholder skeleton when snap is undefined (first load).
  if (!snap) {
    return (
      <header
        className="dashboard-ribbon"
        data-testid="dashboard-ribbon"
        role="banner"
      >
        <span data-testid="field-callsign" className="ribbon-field">—</span>
        <span data-testid="field-grid" className="ribbon-field">—</span>
        <span data-testid="field-gps" className="ribbon-field">GPS: …</span>
        <span data-testid="field-time" className="ribbon-field">—</span>
        <span data-testid="field-connection" className="ribbon-field" aria-live="polite">
          —
        </span>
      </header>
    );
  }

  const callsign = deriveCallsignField(snap);
  const grid = deriveGridField(snap, privacy);
  const gps = deriveGpsField(snap);
  const time = deriveTimeFields(snap);
  const connection = deriveConnectionField(snap);

  return (
    <header
      className="dashboard-ribbon"
      data-testid="dashboard-ribbon"
      role="banner"
    >
      <span data-testid="field-callsign" className="ribbon-field ribbon-callsign">
        {callsign}
      </span>
      <span
        data-testid="field-grid"
        className="ribbon-field ribbon-grid"
        title={grid.tooltip}
      >
        {grid.display}
      </span>
      <span
        data-testid="field-gps"
        className={`ribbon-field ribbon-gps ribbon-gps-${gps.indicator}`}
      >
        {gps.label}
      </span>
      <span data-testid="field-time" className="ribbon-field ribbon-time">
        {time.combined}
      </span>
      <span
        data-testid="field-connection"
        className="ribbon-field ribbon-connection"
        aria-live="polite"
      >
        {connection.label}
      </span>
    </header>
  );
}
```

- [ ] **Step 3.3.2: Run to verify green**

```bash
pnpm test src/shell/DashboardRibbon.test.tsx
```

Expected: all 10 tests pass. If any fail, fix the component — do NOT weaken tests. The two "CRITICAL: …" tests are the Principle-7 + §4.1 load-bearing invariants.

### Task 3.4 — Commit Phase 3

- [ ] **Step 3.4.1: Stage + commit**

```bash
git add src/shell/useDashboard.ts src/shell/DashboardRibbon.tsx src/shell/DashboardRibbon.test.tsx
git commit -m "$(cat <<'EOF'
feat(shell): Task 16 Phase 3 — DashboardRibbon component

Top-of-window ribbon per AMD-8: callsign · grid · GPS · UTC+local ·
connection. Polls dashboard_read every 5s via TanStack Query. Grid
renders precision-reduced (4-char) by default per Principle 7;
connection field names the transport (CMS-SSL / Telnet) per §4.1.

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

### Phase 3 completion check

1. `pnpm test src/shell/DashboardRibbon.test.tsx` → all green.
2. Re-verify CRITICAL tests: grid-precision-default + transport-naming-invariant.
3. Re-read `docs/pitfalls/testing-pitfalls.md` §7 (test infrastructure) — confirm: no shared mutable state across tests (`beforeEach` resets `mockInvoke`); no network calls (Tauri invoke is mocked).
4. Update Living Document Contract: Phase 3 → ✅ SHIPPED at `<SHA>` on `<YYYY-MM-DD>`.

---

## Phase 4 — Minimal StatusBar component

**Execution Status:** ⬜ NOT STARTED

**Goal of this phase:** The bottom status bar. Per AMD-8 this is **app-chrome only** — last user action timestamp on the left, current pane focus on the right. Mail.app-style minimal. Default visible; togglable via `menu:view:status_bar` (already defined in Task 7's menu spec, wired in Phase 5).

**Files:**
- Create: `src/shell/useStatus.ts`
- Create: `src/shell/StatusBar.tsx`
- Create: `src/shell/StatusBar.test.tsx`

**Ordering dependencies:** Requires Phase 1 (backend command) + Phase 2 (derivation helpers).

### Task 4.1 — TanStack Query hook for `status_read`

- [ ] **Step 4.1.1: Create `src/shell/useStatus.ts`**

```ts
// src/shell/useStatus.ts
import { useQuery } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import type { StatusSnapshot } from './types';

export function useStatus() {
  return useQuery<StatusSnapshot>({
    queryKey: ['status'],
    queryFn: () => invoke<StatusSnapshot>('status_read'),
    refetchInterval: 5000,
    placeholderData: (prev) => prev,
  });
}
```

### Task 4.2 — Write failing tests for `StatusBar`

- [ ] **Step 4.2.1: Create `src/shell/StatusBar.test.tsx`**

```tsx
// src/shell/StatusBar.test.tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { StatusBar } from './StatusBar';
import type { StatusSnapshot } from './types';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
const mockInvoke = invoke as unknown as ReturnType<typeof vi.fn>;

function renderWithClient(visible: boolean) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <StatusBar visible={visible} />
    </QueryClientProvider>,
  );
}

beforeEach(() => { mockInvoke.mockReset(); });

describe('StatusBar', () => {
  it('renders nothing when visible=false', () => {
    mockInvoke.mockResolvedValue({ last_action: null, last_action_label: null, pane_focus: null });
    renderWithClient(false);
    expect(screen.queryByTestId('status-bar')).not.toBeInTheDocument();
  });

  it('renders "Ready" left + empty right on first paint', async () => {
    mockInvoke.mockResolvedValue({ last_action: null, last_action_label: null, pane_focus: null });
    renderWithClient(true);
    await waitFor(() => {
      expect(screen.getByTestId('status-activity')).toHaveTextContent('Ready');
    });
    expect(screen.getByTestId('status-windowinfo')).toHaveTextContent('');
  });

  it('renders activity + pane focus when both present', async () => {
    const snap: StatusSnapshot = {
      last_action: '2026-05-18T14:32:00Z',
      last_action_label: 'Toggled session log',
      pane_focus: 'inbox',
    };
    mockInvoke.mockResolvedValue(snap);
    renderWithClient(true);
    await waitFor(() => {
      expect(screen.getByTestId('status-activity')).toHaveTextContent('14:32 UTC · Toggled session log');
    });
    expect(screen.getByTestId('status-windowinfo')).toHaveTextContent('Inbox');
  });

  it('has role="status" + aria-live="polite" for a11y', async () => {
    mockInvoke.mockResolvedValue({ last_action: null, last_action_label: null, pane_focus: null });
    renderWithClient(true);
    await waitFor(() => {
      const bar = screen.getByTestId('status-bar');
      expect(bar).toHaveAttribute('role', 'status');
      expect(bar).toHaveAttribute('aria-live', 'polite');
    });
  });
});
```

- [ ] **Step 4.2.2: Run to confirm failure**

```bash
pnpm test src/shell/StatusBar.test.tsx
```

Expected: import error — component doesn't exist. Red.

### Task 4.3 — Implement `StatusBar.tsx`

- [ ] **Step 4.3.1: Create `src/shell/StatusBar.tsx`**

```tsx
// src/shell/StatusBar.tsx
//
// Bottom status bar per AMD-8: app-chrome only (last action · pane focus).
// Operator-relevant signals (callsign, grid, GPS, connection) live in
// the DashboardRibbon at the top, NOT here. Do not promote signals from
// the ribbon into the status bar — that re-introduces the Express
// thin-strip-with-cryptic-abbreviations anti-pattern AMD-8 fixes.

import { useStatus } from './useStatus';
import { deriveActivityLabel, deriveWindowInfoLabel } from './status';

export interface StatusBarProps {
  visible: boolean;
}

export function StatusBar({ visible }: StatusBarProps) {
  const { data: snap } = useStatus();
  if (!visible) return null;

  const fallback = { last_action: null, last_action_label: null, pane_focus: null };
  const effective = snap ?? fallback;
  const activity = deriveActivityLabel(effective);
  const windowInfo = deriveWindowInfoLabel(effective);

  return (
    <footer
      className="status-bar"
      data-testid="status-bar"
      role="status"
      aria-live="polite"
    >
      <span data-testid="status-activity" className="status-left">{activity}</span>
      <span data-testid="status-windowinfo" className="status-right">{windowInfo}</span>
    </footer>
  );
}
```

- [ ] **Step 4.3.2: Run to verify green**

```bash
pnpm test src/shell/StatusBar.test.tsx
```

Expected: 4 tests pass.

### Task 4.4 — Commit Phase 4

- [ ] **Step 4.4.1: Stage + commit**

```bash
git add src/shell/useStatus.ts src/shell/StatusBar.tsx src/shell/StatusBar.test.tsx
git commit -m "$(cat <<'EOF'
feat(shell): Task 16 Phase 4 — minimal StatusBar component

Bottom status bar per AMD-8: app-chrome only (left = last action,
right = pane focus). Operator-relevant signals (callsign, grid, GPS,
connection) live in DashboardRibbon at the top — DO NOT promote them
into the status bar; the AMD-8 split exists to prevent the Express
thin-strip anti-pattern.

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

### Phase 4 completion check

1. `pnpm test src/shell/StatusBar.test.tsx` → all 4 tests pass.
2. Update Living Document Contract: Phase 4 → ✅ SHIPPED at `<SHA>` on `<YYYY-MM-DD>`.

---

## Phase 5 — App.tsx layout wiring + menu event handler

**Execution Status:** ⬜ NOT STARTED

**Goal of this phase:** Mount both components in `App.tsx`, set up the CSS grid that gives the ribbon and status bar their own rows (so they don't overlap or push other panes' content), and wire the `menu:view:status_bar` Tauri event to toggle status-bar visibility. The status bar defaults to visible per AMD-8 ("Default true" — see plan §Task 16 Step 4).

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/App.css`

**Ordering dependencies:** Requires Phases 1-4. This is the integration step.

### Task 5.1 — Wire the components into App.tsx

- [ ] **Step 5.1.1: Inspect the current `src/App.tsx` and `src/App.css`**

```bash
cat src/App.tsx
cat src/App.css
```

If `App.tsx` already wires other components (mailbox, session log, radio dock — from other tasks landed earlier), preserve them. Insert `<DashboardRibbon />` at the top of the main layout and `<StatusBar />` at the bottom. If `App.tsx` is still close to the Tauri scaffold default (just a "Welcome to Tauri" greeting), replace the body with the new layout while keeping the QueryClientProvider wrapper if one exists.

- [ ] **Step 5.1.2: Update `src/App.tsx` to mount the ribbon + status bar**

Target structure (preserve any existing siblings; this shows the wiring points only):

```tsx
// src/App.tsx
import { useEffect, useState } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { listen } from '@tauri-apps/api/event';
import { DashboardRibbon } from './shell/DashboardRibbon';
import { StatusBar } from './shell/StatusBar';
import type { DashboardPrivacyConfig } from './shell/types';
import './App.css';

const queryClient = new QueryClient();

// v0.0.1: hardcode the privacy default until Task 2's config-loader hook
// is wired into the frontend. Per Principle 7 the default is FourCharGrid
// (precision-reduced broadcast). Once config-loading is wired (separate
// follow-up task), replace this with a hook that reads
// privacy.position_precision from $XDG_CONFIG_HOME/tuxlink/config.json.
const DEFAULT_PRIVACY: DashboardPrivacyConfig = {
  position_precision: 'FourCharGrid',
};

function AppShell() {
  const [statusBarVisible, setStatusBarVisible] = useState(true);

  useEffect(() => {
    // Wire the menu:view:status_bar Tauri event (defined in Task 7 menu spec).
    // React 18 StrictMode runs effects twice in dev; the cleanup MUST unlisten
    // the prior listener or we get double-toggle bugs. The pattern below is
    // correct: listen returns a Promise<UnlistenFn>; cleanup awaits + calls it.
    const unlistenPromise = listen<string>('menu', (event) => {
      if (event.payload === 'menu:view:status_bar') {
        setStatusBarVisible((v) => !v);
      }
    });
    return () => {
      unlistenPromise.then((fn) => fn()).catch(() => { /* harness teardown */ });
    };
  }, []);

  return (
    <div className="app-shell" data-status-bar-visible={statusBarVisible ? 'true' : 'false'}>
      <DashboardRibbon privacy={DEFAULT_PRIVACY} />
      <main className="app-main">
        {/* Other tasks mount their panes here: Mailbox (Task 12), MessageView
            (Task 13), SessionLog (Task 15), RadioDock (Task 16.5). Preserve
            existing siblings if they were added by prior task PRs. */}
      </main>
      <StatusBar visible={statusBarVisible} />
    </div>
  );
}

export default function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <AppShell />
    </QueryClientProvider>
  );
}
```

**If a `QueryClientProvider` already wraps the app at a higher level** (e.g., a prior task added one in `main.tsx`): drop both the `import { QueryClient, QueryClientProvider } from '@tanstack/react-query';` line and the outer `<QueryClientProvider client={queryClient}>` wrapper from the `App` export above. Export `AppShell` directly: `export default AppShell;` (and also delete the `const queryClient = new QueryClient();` line — leaving it as an unused variable is dead code).

Verify the decision by `grep -rn "QueryClientProvider" src/`. If you find exactly one site, that's where the provider lives. The provider MUST exist exactly once for TanStack Query to work; zero or two is a bug.

- [ ] **Step 5.1.3: Update `src/App.css` with the layout variables + base styles**

Append (or merge if already present — do NOT overwrite existing rules):

```css
:root {
  --ribbon-h: 40px;
  --status-h: 24px;
}

.app-shell {
  display: grid;
  grid-template-rows: var(--ribbon-h) 1fr var(--status-h);
  height: 100vh;
}

.app-shell[data-status-bar-visible="false"] {
  grid-template-rows: var(--ribbon-h) 1fr 0;
}

.app-main {
  overflow: hidden;
  /* Other tasks' panes live here; their internal grids handle their own layouts. */
}

/* Dashboard ribbon — top */
.dashboard-ribbon {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 0 12px;
  border-bottom: 1px solid var(--border, #333);
  font-size: 13px;
  background: var(--ribbon-bg, #1e1e1e);
  color: var(--ribbon-fg, #d4d4d4);
  height: var(--ribbon-h);
}

.ribbon-field {
  white-space: nowrap;
}

.ribbon-callsign {
  font-weight: 600;
  font-size: 14px;
}

.ribbon-gps-on { color: #4ec9b0; }
.ribbon-gps-off { color: #888; }
.ribbon-gps-searching { color: #d7ba7d; }
.ribbon-gps-neutral { color: var(--ribbon-fg, #d4d4d4); }

.ribbon-connection {
  margin-left: auto;
}

/* Status bar — bottom (minimal, app-chrome only) */
.status-bar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0 12px;
  font-size: 11px;
  color: var(--statusbar-fg, #888);
  background: var(--statusbar-bg, #181818);
  border-top: 1px solid var(--border, #333);
  height: var(--status-h);
}

.status-left { text-align: left; }
.status-right { text-align: right; }
```

If the project has not yet adopted a CSS variable convention for theme colors, use the literal hex values shown (dark-mode defaults consistent with the mockup gallery). Other tasks may introduce theme variables later — those will override the literals here without modification.

### Task 5.2 — Verify the integrated build

- [ ] **Step 5.2.1: Run the full TS + Rust test suite**

```bash
pnpm test                              # vitest, all suites
cd src-tauri && cargo test && cd ..
```

Expected: all tests pass. No new failures introduced.

- [ ] **Step 5.2.2: Confirm TypeScript compiles cleanly**

```bash
pnpm tsc --noEmit
```

Expected: zero errors. If `tsc` is not configured to run via pnpm, run via `npx tsc --noEmit` instead.

- [ ] **Step 5.2.3: Commit Phase 5**

```bash
git add src/App.tsx src/App.css
git commit -m "$(cat <<'EOF'
feat(shell): Task 16 Phase 5 — mount DashboardRibbon + StatusBar in App

Three-row CSS grid: ribbon at top (40px) · main panes (1fr) · status
bar at bottom (24px). Status bar defaults to visible; menu:view:status_bar
event toggles. Privacy default hardcoded to FourCharGrid per Principle 7
until Task 2's config loader is wired into the frontend (separate follow-up).

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

### Phase 5 completion check

1. `pnpm test && cd src-tauri && cargo test && cd ..` → all green.
2. `pnpm tsc --noEmit` → zero errors.
3. Update Living Document Contract: Phase 5 → ✅ SHIPPED at `<SHA>` on `<YYYY-MM-DD>`.

---

## Phase 6 — Manual verification + REVIEW GATE + PR

**Execution Status:** ⬜ NOT STARTED

**Goal of this phase:** Run the app end-to-end in `pnpm tauri dev` and walk the user flow per the `feedback_browser_smoke_before_ship` memory entry. Then open the PR. Phase 6 does NOT include the cross-task 3-round review gate over Tasks 12-16 — that's an operator-scheduled gate that runs after THIS task ships AND the other main-UI tasks ship. The implementing agent's responsibility ends at "PR is open, browser smoke passed, all tests green."

**Files:** none modified in this phase (verification + commit + PR only).

### Task 6.1 — Browser smoke (mandatory per memory `feedback_browser_smoke_before_ship`)

- [ ] **Step 6.1.1: Run the dev build**

```bash
pnpm tauri dev
```

Expected: Tauri window opens. The dashboard ribbon appears at the top showing the v0.0.1 stub values (callsign "W4PHS", grid "EM75" — the 4-char broadcast precision per Principle 7, GPS "GPS: manual", time in UTC + local, connection "Idle"). Status bar at the bottom shows "Ready" on the left, empty on the right.

- [ ] **Step 6.1.2: Verify the menu toggle (CONDITIONAL on Task 7 shipped)**

Click View → Toggle Status Bar (the menu item defined in Task 7's `menu:view:status_bar`). The bottom status bar disappears; the dashboard ribbon stays in place. Click again; the status bar reappears.

**If Task 7 (native OS menu bar) has NOT yet merged**, the menu won't exist in the app and this step is N/A. Verify via:

```bash
grep -n "pub mod menu" src-tauri/src/lib.rs
```

Empty result = Task 7 not yet shipped. In that case:

- The `listen('menu', ...)` effect in `App.tsx` is still correct — it just won't ever fire until Task 7's `wire_menu_events` is wired. The status bar will be visible by default (`statusBarVisible = true` initial state). That's the correct fallback.
- Note the dependency in the PR body's "Open decisions" section: "Toggle behavior verifiable post-Task-7 merge."
- Do NOT add a stand-in shortcut handler (e.g., a `useEffect` keyboard listener for Ctrl+Shift+B) to make the toggle testable now — that's scope creep into Task 7's territory and creates a code-removal task once Task 7 lands.

**If Task 7 has merged**, the toggle should work. If it doesn't:
- The Task 7 menu was built — `cargo test --test menu_test` should pass.
- The `wire_menu_events` Rust handler emits a Tauri event named `menu` with the event id as payload — this matches `App.tsx`'s `listen<string>('menu', ...)`.
- The browser dev-tools console (open via right-click → Inspect Element in Tauri dev mode) shows no React errors.
- If the event isn't being fired by Task 7's implementation (e.g., the wire-up is incomplete in another PR), STOP and surface it per the Living Document Contract's "On discovery" rule — do NOT silently rewrite Task 7.

- [ ] **Step 6.1.3: Verify ribbon updates every 5s**

Watch the time field on the ribbon. It should update on the next 5-second poll boundary (the `refetchInterval` setting). If the time field is frozen, check the network panel for `dashboard_read` invocations.

- [ ] **Step 6.1.4: Verify CRITICAL invariants visually (within stub-state limits)**

- **Grid field shows 4-char** (`EM75`, NOT `EM75xx`) — per Principle 7. This IS observable from the stub.
- **Connection field shows "Idle"** — the only state the v0.0.1 stub produces. The "names the transport" invariant CANNOT be observed visually until the stub is replaced with a real session-state tracker in v0.1+ — that's why the load-bearing assertion lives in the unit tests (`deriveConnectionField` exhaustive cross-product) rather than in the visual smoke. Confirm the test ran green in Phase 5.2.1.

If the grid invariant is violated visually, STOP. The corresponding "CRITICAL" unit tests should have caught it; if they passed but the UI is wrong, there's a wiring bug between component and helpers. Trace it back; do NOT ship.

**Optional: temporary stub override for visual verification.** If you want to visually confirm the `In session via CMS-SSL` rendering before merging, you MAY edit `dashboard_read` in `src-tauri/src/dashboard.rs` to return `ConnectionStateKind::InSession` + a sample `connection_detail`, run `pnpm tauri dev`, observe, then REVERT the change before committing. Do NOT ship the override.

### Task 6.2 — Stage + push + PR

- [ ] **Step 6.2.1: Verify branch state**

```bash
git status
git log --oneline -10
```

Expected: working tree clean (all 5 phase commits committed); on branch `bd-tuxlink-hvv/task-16-status-surfaces` (or the equivalent per-task branch name); 5 commits ahead of `feat/v0.0.1`.

If the branch name differs (because the implementing agent picked a different slug), that's fine — the convention is `bd-<id>/<slug>` per CLAUDE.md / ADR 0004; the slug is the implementer's choice.

- [ ] **Step 6.2.2: Pull + push the branch**

```bash
git fetch origin
git pull --rebase origin bd-tuxlink-hvv/task-16-status-surfaces 2>/dev/null || true
# (Pull only succeeds if the remote branch exists; first push to a new branch
#  has nothing to pull. The `|| true` keeps the script flowing for first push.)
git push -u origin bd-tuxlink-hvv/task-16-status-surfaces
```

(Substitute your actual branch name if it differs from `bd-tuxlink-hvv/task-16-status-surfaces`.)

If push fails because of stale remote (someone else pushed to the branch), STOP — do NOT force-push (banned by `block-destructive-git.sh`). Investigate via `git fetch && git log origin/<branch> --oneline -5`. If a prior partial push exists, integrate via `git pull --rebase origin <branch>` then `git push` (no `--force-with-lease` — also banned).

If `git pull --rebase` produces merge conflicts that can't be resolved trivially, STOP and surface to the operator — do NOT attempt `git reset --hard` or `git rebase --abort` followed by destructive cleanup.

- [ ] **Step 6.2.3: Open the PR**

```bash
gh pr create \
  --base feat/v0.0.1 \
  --head bd-tuxlink-hvv/task-16-status-surfaces \
  --title "[<your-moniker>] feat(shell): Task 16 — Dashboard ribbon + minimal status bar (AMD-8)" \
  --body "$(cat <<'EOF'
## Summary

Implements Task 16 per AMD-8 / docs/design/v0.0.1-ux-mockups.md §5.9
as TWO surfaces:

- **Dashboard ribbon** (top, ~40px): callsign · grid · GPS · UTC+local ·
  connection-with-transport. Always visible in v0.0.1.
- **Minimal status bar** (bottom, ~24px): app-chrome only (last action ·
  pane focus). Toggleable via View → Toggle Status Bar.

Backend (`src-tauri/src/dashboard.rs`) ships STUB Tauri commands —
real Pat-session-state tracking is v0.1+. The contract (JSON shape)
is the load-bearing artifact this PR establishes.

Frontend (`src/shell/`) isolates derivation logic in pure TS helpers
(`dashboard.ts`, `status.ts`) so the load-bearing invariants are
unit-testable without the GUI.

## Load-bearing invariants

1. **Principle 7 (position privacy):** grid field renders precision-
   reduced (4-char Maidenhead) BY DEFAULT; 6-char only when
   `privacy.position_precision = SixCharGrid` is configured. Covered
   by `deriveGridField` tests + the "CRITICAL: a 6-char grid_local
   MUST render as 4-char" assertion.
2. **§4.1 transport-visibility (RADIO-2):** connection field ALWAYS
   names the transport (CMS-SSL or Telnet) in non-Idle states. Covered
   by `deriveConnectionField` exhaustive cross-product test.
3. **SCOPE-1:** connection state refers to OUTBOUND CLIENT sessions
   only; no inbound/gateway semantics introduced.

## Test plan

- [ ] `cd src-tauri && cargo test --test dashboard_test` — 4 tests pass
- [ ] `cd src-tauri && cargo test --lib dashboard::local_tests` — 2 tests pass
- [ ] `pnpm test src/shell/` — all helper + component tests pass
- [ ] `pnpm tsc --noEmit` — zero TS errors
- [ ] `pnpm tauri dev` — ribbon + status bar render; View → Toggle Status
      Bar hides/shows the status bar
- [ ] Visual check: grid field shows 4-char (`EM75`, NOT `EM75xx`)
- [ ] Visual check: connection field shows "Idle" (or "In session via
      CMS-SSL" once stub returns InSession)

## Cross-task ordering

This task is part of the main-UI cluster (Tasks 12-16) and is the
**review gate** for that cluster per the plan's Task Overview table.
After this PR merges, a 3-round review over Tasks 12-16 is the
operator's next gate before Task 17.

## Tasks NOT in scope

- Task 16.5 (Radio Dock pane) — separate task per AMD-9.
- Real Pat-session-state integration — v0.1+.
- GPS device integration (serial/TCP/IP/HAT) — v0.1+ per design doc §8.
- Frontend config-loader for privacy.position_precision — placeholder
  default (`FourCharGrid`) hardcoded in `App.tsx`; follow-up task wires
  the Task 2 config loader through to the frontend.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 6.2.4: Update the Living Document Contract**

After the PR is open, edit this plan file:
- Top-of-plan Execution Status: Phase 6 → 🚧 IN PROGRESS — claimed `<YYYY-MM-DD HH:MMZ>` (branch `<branch>`, PR #N).
- Top-of-plan table: add the PR # to the Notes column.

Commit the plan update:

```bash
git add docs/plans/2026-05-18-task-16-status-surfaces-plan.md
git commit -m "$(cat <<'EOF'
docs(plan): Task 16 plan — Phase 6 IN PROGRESS, PR #N opened

Per Living Document Contract: banner flipped to 🚧 IN PROGRESS for
Phase 6 with claim timestamp + branch + PR pointer.

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
git push
```

### Phase 6 completion check (post-merge — by the merging agent or by THIS agent if Cameron merges promptly)

1. After the PR merges, update Phase 6's banner to ✅ SHIPPED at `<merge-SHA>` on `<YYYY-MM-DD>` (PR #N merged at `<merge-SHA>`).
2. Update all 6 phase rows in the top-of-plan table.
3. `bd close tuxlink-hvv` (the Task 16 bd issue).
4. Dispose the worktree per ADR 0009 ritual (inventory → cd back → archive if needed → `rm -rf` → `git worktree prune`).

---

## Cross-task conflict minimization

This task touches:

- **NEW files only** under `src/shell/` (DashboardRibbon, StatusBar, types, dashboard, status, useDashboard, useStatus).
- **NEW files only** under `src-tauri/src/dashboard.rs` + `src-tauri/tests/dashboard_test.rs`.
- **MODIFY** `src-tauri/src/lib.rs` (add `pub mod dashboard;` + invoke_handler entries — append-only).
- **MODIFY** `src/App.tsx` (mount the new components — additive, preserve existing siblings).
- **MODIFY** `src/App.css` (add new layout variables + classes — additive, do NOT overwrite existing rules).

**Potential conflicts with parallel Wave-2 tasks:**

- **Task 7 (menu bar)** owns `src-tauri/src/menu.rs` — this task does NOT modify it. The `menu:view:status_bar` event is consumed via the Tauri event bus, not by importing from menu.rs.
- **Task 12 (Mailbox)**, **Task 13 (MessageView)**, **Task 15 (SessionLog)**, **Task 16.5 (RadioDock)** also modify `src/App.tsx` and `src/App.css` as the integration sites for their panes. **Resolution strategy:** when this PR rebases on `feat/v0.0.1` post-merge of other panes, the `<DashboardRibbon />` mount point at the top of the layout and `<StatusBar />` mount point at the bottom are stable; the `<main>` block in between accepts any siblings the other tasks added. CSS variables `--ribbon-h` and `--status-h` are unique to this task; no conflict.
- **Task 2 (config schema)** is already shipped per the plan (AMD-1 landed). This task READS the config shape (`PrivacyConfig.position_precision`) but does NOT modify it. The frontend `DashboardPrivacyConfig` type in `src/shell/types.ts` is a subset mirror.

**If two parallel implementers both edit `src/App.tsx`:** the safe resolution is to add the new component as a sibling inside the existing layout (additive). Git conflict-marker resolution should preserve both insertions. If the conflict is non-trivial (e.g., layout restructuring needed), STOP and surface it for human resolution — do NOT silently drop another task's mount.

---

## Self-review (done by the plan author at write time, not by executors)

This section records the plan-author's self-review. Executors do not need to redo it.

1. **Spec coverage:**
   - Design doc §5.9 dashboard ribbon — 5 fields covered (callsign, grid, GPS, UTC+local, connection-with-transport). ✓
   - Design doc §5.9 minimal status bar — 2 zones covered (left activity, right window info). ✓
   - Principle 7 (precision reduction by default) — enforced in `deriveGridField` + ribbon component + CRITICAL test. ✓
   - §4.1 transport-visibility — enforced in `deriveConnectionField` + CRITICAL test. ✓
   - AMD-8 acceptance criteria all addressed (dashboard ribbon NEW component, explicit transport, GPS 3-state semantics). ✓
   - AMD-8 explicitly excludes Radio Dock (Task 16.5) — confirmed not in scope. ✓
   - Backend stub + frontend separation matches AMD-8 amendment note "implementing agent must … split the work into two components." ✓

2. **Placeholder scan:** no "TBD", "TODO", "fill in details", "similar to Task N" markers. Every code block is complete.

3. **Type consistency:**
   - Rust `GpsStateKind` enum variants match TS `GpsState` literals (`Off`, `LocalUiOnly`, `BroadcastAtPrecision`, `Searching`, `Manual`) ✓
   - Rust `ConnectionStateKind` matches TS `ConnectionState` (`Idle`, `Connecting`, `InSession`, `Disconnecting`, `Disconnected`) ✓
   - Rust `CmsTransportKind` matches TS `CmsTransport` (`CmsSsl`, `Telnet`) — same casing as AMD-1 schema ✓
   - Field names in `DashboardSnapshot` Rust struct match TS interface keys (snake_case) ✓
   - Derivation helper signatures match their consumer calls in `DashboardRibbon.tsx` ✓

4. **Pitfall coverage:**
   - RADIO-1 (no transmission) — task is render-only; explicit STOP-and-escalate note added.
   - RADIO-2 (encryption visibility) — transport-naming invariant tested.
   - SCOPE-1 (client vs gateway) — explicit doc comment in `dashboard.rs` + plan note.
   - HOOK-1 — worktree creation flow already implied by the Wave-1 dispatch process; no in-plan note needed.
   - Testing pitfalls §1-§7 — covered by the "Mandatory Per-Phase Completion Check."

5. **No banned dependencies:** the plan uses `serde`/`serde_json` (existing Task 1 deps), `tauri` (existing), `@tanstack/react-query` (existing), `@testing-library/react` + `vitest` + `jsdom` (new dev-only test deps — these are NOT on the banned list in plan §"Subagent Guardrails"; the ban is for runtime crates like `tracing`/`anyhow`/`thiserror`/`log`). NO chrono added; UTC formatting done inline.

---

**End of plan.** Plan author: `cypress-lupine-moss` (2026-05-18). See top-of-plan Execution Status for live state.
