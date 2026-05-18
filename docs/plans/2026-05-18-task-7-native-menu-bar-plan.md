# Plan — Task 7: Native OS menu bar (Tauri 2 + AMD-10 wizard + runtime halves)

**Source spec:** `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` §"Task 7: Native OS menu bar" (lines 1703–1981), as amended by AMD-10 (both wizard half and runtime half merged into one coherent menu spec).

**Companion spec sources (read these alongside the plan source):**
- `docs/design/v0.0.1-ux-mockups.md` §7 "New menu items" (canonical AMD-10 source).
- `docs/ux-anti-patterns.md` §"Required elements (not negotiable for v0.1)" → "Native menu bar at the top of the window" (category list + starting keyboard-shortcut set).
- `docs/pitfalls/implementation-pitfalls.md` SCOPE-1, HOOK-1, LEASE-1, PARITY-1, RADIO-1, RADIO-2 (this plan must not regress any of them).

**bd issues:**
- Plan-writing issue (this plan): `tuxlink-q8i` (claimed by `willow-osprey-tamarack`).
- Implementation issue (Wave-2 executor claims this): `tuxlink-6vi`.
- Depends-on (already shipped): `tuxlink-wkz` (Task 1 — Tauri 2 + React + TypeScript scaffold).
- Blocks (downstream): `tuxlink-rit` (Task 8 — System tray + window-close-hides-to-tray).

**Goal:** Ship a single, coherent native OS menu bar (File / Message / Session / Mailbox / View / Tools / Help) that wires every menu item — both the baseline set from the original Task 7 spec AND the AMD-10 additions (one wizard-half item + seven runtime-half items, plus a nested Settings submenu) — to Tauri events of the form `menu:{category}:{action}`. The React frontend listens for those events; this task ships ONLY the menu and the event-emission glue (the listeners are downstream tasks' problem). Plus the menu-event-IDs unit test, plus the `pnpm tauri dev` manual smoke. Plus an idempotent `lib.rs`/`run()` integration that wires the menu and the event dispatcher at app setup.

**Architecture:** Pure Rust in `src-tauri/src/menu.rs` builds the menu via `tauri::menu::{MenuBuilder, SubmenuBuilder, MenuItemBuilder}`. The menu is constructed in the app's `setup` hook and registered via `app.set_menu(menu)`. Menu clicks dispatch via `Builder::on_menu_event` (chained on the top-level `tauri::Builder::default()`) — the handler reads the event's `MenuId`, converts to `&str`, and `emit`s a Tauri event named `menu` whose payload is the ID string (e.g., `"menu:file:new"`). React subscribes via `@tauri-apps/api/event`'s `listen("menu", ...)`. No code outside Task 7 changes its public API.

**Tech stack:** Rust (Tauri 2.x — `tauri = "2"`, already in `Cargo.toml`), the `tauri::menu` module (built into the `tauri` crate; no extra plugin needed), Tauri 2's built-in keyboard-accelerator support on menu items (no separate `tauri-plugin-global-shortcut` dependency required for shortcuts attached to menu items — those are first-class on `MenuItemBuilder::accelerator`).

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
| 0 — Executor preflight (moniker, bd claim, worktree, starting-state check) | ⬜ Not started | — | — |
| 1 — Failing unit test for `menu_event_ids()` | ⬜ Not started | — | — |
| 2 — Implement `src-tauri/src/menu.rs` (`build_menu` + `menu_event_ids` + `dispatch_menu_event`) | ⬜ Not started | — | — |
| 3 — Register the menu in `lib.rs::run()` setup + `pub mod menu;` | ⬜ Not started | — | — |
| 4 — Green the unit test (`cargo test --test menu_test`) | ⬜ Not started | — | — |
| 5 — Manual smoke via `pnpm tauri dev` + record outcome in PR body | ⬜ Not started | — | — |
| 6 — Commit + push + PR against `feat/v0.0.1` | ⬜ Not started | — | — |

### Deviations

_(none yet)_

### Discoveries

The plan-writing pass (Wave 1) surfaced two discrepancies between the existing Task 7 spec and current Tauri 2.x reality. Both are addressed in this plan's task bodies (Phases 2 and 3 specifically), but flagging here so the executor doesn't lose them under the spec-snippet's wording:

- **D1 — `on_menu_event` is a `tauri::Builder` method, not an `AppHandle` method in Tauri 2.x.** The original Task-7 spec snippet (`docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` lines 1864–1872) writes `app.clone().on_menu_event(...)` inside a helper called from `setup`. As of `tauri 2.9.5` (the current released line, ref `https://docs.rs/tauri/2.9.5/tauri/struct.Builder.html#method.on_menu_event`), `on_menu_event` is a `Builder` method that must be chained on the top-level `tauri::Builder::default()` BEFORE `.run(...)`. There IS a per-window variant on `Window`/`WebviewWindow` (`Window::on_menu_event`), but a single application-wide listener is the right shape here. This plan resolves the discrepancy by having `menu.rs` (Phase 2) expose a pure `dispatch_menu_event` function, and having `lib.rs::run()` (Phase 3) register the listener via `Builder::on_menu_event(|app, event| dispatch_menu_event(app, event.id().as_ref()))` directly on the top-level builder chain.
- **D2 — `lib.rs` is the entry point, not `main.rs`.** The Task-7 spec snippet shows modifications to `src-tauri/src/main.rs`. As scaffolded by Task 1 (already shipped), `src-tauri/src/main.rs` is a one-line shim that calls `tuxlink_lib::run()`, and the actual `tauri::Builder::default()` lives in `src-tauri/src/lib.rs::run()`. Phase 3 of this plan modifies `lib.rs`, not `main.rs`. The `main.rs` shim is left untouched.

---

## Why this plan structure

The original Task 7 spec is largely correct but has THREE specific issues that an executor reading it cold would predictably hit:

1. **The spec is duplicated in the source plan** — Steps 3–6 appear twice in `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` (lines 1770–1925 and again at 1927–1981, with a stray `}` and `\`\`\`` between them). This is a leftover from the AMD-10 amendment edit. The duplicate is harmless if the executor reads the first copy and stops, but a literal "follow the steps in order" executor will get confused and may try to apply the second copy and double-write files. **This plan supersedes the duplicate by being the single source of truth for the executor; the source-plan duplicate is a known artifact, do NOT re-execute it.**
2. **D1 above** — the `on_menu_event` wiring is in the wrong place.
3. **D2 above** — modifications target `main.rs` but should target `lib.rs::run()`.

This plan resolves all three in-line, citing the source spec where the spec is correct (the menu structure, the event-ID list, the accelerators, the test shape) and overriding only where the spec is stale against Tauri 2.x.

The plan is one single phase split into six steps because Task 7 is a small, atomic vertical: one Rust file created, two existing Rust files lightly modified, one unit test, one manual smoke. No parallelization is possible within Task 7 and no useful checkpoint exists between Steps 1 and 6. Phases-as-steps is the right granularity.

---

## Phase 0 — Setup (executor preflight)

**Execution Status:** ⬜ NOT STARTED

Before any code change, the Wave-2 executor does:

1. **Pick a moniker:** `python3 .claude/scripts/get_agent_moniker.py`. Record it; use it in commit `Agent:` trailers + PR title `[<moniker>] ...`.
2. **Claim the implementation issue:** `bd update tuxlink-6vi --claim`. Confirms ownership and updates the issue state from `OPEN` to `IN_PROGRESS`.
3. **Create the worktree:** `python3 .claude/scripts/new_tuxlink_worktree.py --slug task-7-menu-bar --issue tuxlink-6vi --moniker <your-moniker>`. This branches off `feat/v0.0.1` to `bd-tuxlink-6vi/task-7-menu-bar` (or whatever slug you pass — the convention is `bd-<issue-id>/<slug>`).
4. **cd into the worktree:** `cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-6vi-<your-slug>` (path mirrors the worktree directory — adjust `<your-slug>` to match whatever you passed to `--slug` in step 3).
5. **Verify the current `src-tauri/src/lib.rs` matches the "exact starting state" in Phase 3.** If it differs materially (e.g., another Wave-2 task already merged a `pub mod` insertion or a setup hook), surface to the dispatcher rather than blind-merging.
6. **Read the dispatcher's prompt + this plan top to bottom**, including the Living Document Contract and the Pitfall reviews. Do not skim.

This phase has no files to create. It is the precondition gate for Phases 1–6.

If any precondition fails (lib.rs differs, bd claim fails because the issue is already claimed by another agent, worktree creation fails because the hook denies), STOP and report. Do not proceed.

---

## Phase 1 — Failing unit test for `menu_event_ids()`

**Execution Status:** ⬜ NOT STARTED

### What this phase does

Write a Rust integration test at `src-tauri/tests/menu_test.rs` that asserts the `menu_event_ids()` function exposed by `src-tauri/src/menu.rs` returns every required event ID. The test is RED at this point because `menu.rs` does not yet exist (Phase 2 creates it). This is the TDD red stage and the regression-detection floor: if any future change drops a menu item or renames its event ID, this test breaks.

### Files

- **Create:** `src-tauri/tests/menu_test.rs`

### Exact behavior change

- Before: `src-tauri/tests/` contains `config_test.rs`, `pat_client_test.rs`, `pat_process_test.rs` (already shipped via Tasks 2, 5, 3).
- After: `src-tauri/tests/menu_test.rs` exists with one test, `test_menu_exposes_required_event_ids`, asserting the full event-ID list (baseline + AMD-10 wizard half + AMD-10 runtime half).

### TDD discipline (mandatory)

**BEFORE starting work:**

1. Invoke `/superpowers:test-driven-development`.
2. Read `docs/pitfalls/testing-pitfalls.md` (§§1–7).
3. Follow TDD: write failing test → implement → verify green.

**BEFORE marking this phase complete:**

1. Review the test against `docs/pitfalls/testing-pitfalls.md`. Particularly §1 (Test Output Pristine) — the test SHOULD NOT print anything on green; failure messages SHOULD name the missing ID; §2 (Skipped Tests Are Not Passing Tests) — no `#[ignore]` on this test.
2. Verify test coverage:
   - Every event ID enumerated in the Phase 2 spec is checked.
   - No event ID is checked twice (would silently mask a dropped second occurrence).
   - The test asserts presence-of-required-IDs (`ids.contains(...)`), NOT exact equality with a hardcoded list (forward-compatible with later additions per future amendments).
3. Run `cargo test --test menu_test` and CONFIRM that the test FAILS with a compile error like "unresolved import `tuxlink_lib::menu`" or "module `menu` not found." If the test passes at this stage, something is wrong — STOP and surface to the dispatcher.

### Exact test code

```rust
// src-tauri/tests/menu_test.rs
use tuxlink_lib::menu;

#[test]
fn test_menu_exposes_required_event_ids() {
    let ids = menu::menu_event_ids();
    let required = [
        // File
        "menu:file:new", "menu:file:quit",
        // Message
        "menu:message:reply", "menu:message:reply_all",
        "menu:message:forward", "menu:message:print",
        // Session (baseline + AMD-10 wizard half + AMD-10 runtime half)
        "menu:session:connect", "menu:session:disconnect", "menu:session:log",
        "menu:session:test_send",         // AMD-10 wizard half
        "menu:session:show_transport",    // AMD-10 runtime half
        // Mailbox
        "menu:mailbox:inbox", "menu:mailbox:sent", "menu:mailbox:outbox",
        // View (baseline + AMD-10 runtime half)
        "menu:view:session_log", "menu:view:status_bar",
        "menu:view:raw_log",              // AMD-10 runtime half
        "menu:view:radio_dock",           // AMD-10 runtime half
        // Tools (baseline + AMD-10 runtime half)
        "menu:tools:templates", "menu:tools:rig_control", "menu:tools:preferences",
        "menu:tools:settings_connection",         // AMD-10 runtime half
        "menu:tools:settings_privacy_gps",        // AMD-10 runtime half
        "menu:tools:settings_privacy_position",   // AMD-10 runtime half
        "menu:tools:settings_gps",                // AMD-10 runtime half
        // Help
        "menu:help:about", "menu:help:docs", "menu:help:report_issue",
    ];
    for r in required {
        assert!(
            ids.contains(&r),
            "missing menu event id: {r} (got: {ids:?})"
        );
    }
}
```

### Pitfall review for Phase 1

- **testing-pitfalls.md §1 (Test Output Pristine):** the failure-mode `assert!` uses `format_args!`-style interpolation so the diff shows what's missing AND what was emitted. Pass case prints nothing.
- **testing-pitfalls.md §3 (Error Path Coverage):** Phase 1 covers the missing-ID failure path explicitly via the `assert!` message. No error path beyond that — `menu_event_ids` is a pure constant returner.
- **testing-pitfalls.md §4 (Negative Property Testing):** N/A for a presence-of-required-IDs check. A `≥N` size check is INTENTIONALLY NOT used here because it would conflict with the forward-compatibility of presence-only assertions: future amendments (AMD-11+ hypothetically) add more IDs without breaking this test.
- **testing-pitfalls.md §7 (Test Infrastructure Hygiene):** the test does NOT instantiate `tauri::App`, does NOT touch the file system, does NOT spawn processes. It tests one pure function. This is the right level of isolation for menu-event-ID regression detection — full menu construction needs `tauri::AppHandle` which needs a GUI runtime, hence the split (`menu_event_ids` for unit-testable assertions; `build_menu` for the manual smoke).

### Assertion-rigor reminder (concurrency / timing — N/A here but boilerplate per writing-plans-enhanced)

This test has no concurrency, no timing, no flake potential. The boilerplate "if assertions race, fix synchronization not assertions" rule is non-binding for Phase 1. Boilerplate retained for executor-clarity in case the executor's environment surprises them.

### Ordering deps

- Depends on: `src-tauri/src/lib.rs` exporting `pub mod menu` (lands in Phase 3) — but the test compile-error IS the red stage, so the test CAN and SHOULD be written and committed (as a red checkpoint) before Phase 2 exists. The executor MAY EITHER (a) write the test, commit it, write the impl, commit, watch green, OR (b) write test + impl in one staged commit and watch green-from-the-start in a single `cargo test` run. Recommend (a) for cleaner git archaeology; either is acceptable. There is no observable correctness difference.
- Touches files: `src-tauri/tests/menu_test.rs` only. No conflict potential with any other Wave-2 task.

### DO NOT

- DO NOT add a test that constructs an actual `Menu` object from Phase 2's `build_menu` — that requires a `tauri::App` instance which needs a GUI runtime. The `menu_event_ids` indirection EXISTS specifically to allow Rust-side unit-test assertions without spinning a GUI. Plan was deliberate; do not "improve" by inlining.
- DO NOT use `assert_eq!` against a hardcoded full list — use `assert!(ids.contains(&r), ...)` per the snippet above. Reasoning: forward-compatibility for future amendments + tighter failure messages.
- DO NOT add Phase 3's `lib.rs` `pub mod menu;` line in this phase. Keep the red checkpoint pristine: only the test file is new in Phase 1.

---

## Phase 2 — Implement `src-tauri/src/menu.rs`

**Execution Status:** ⬜ NOT STARTED

### What this phase does

Create `src-tauri/src/menu.rs` exposing:

1. `menu_event_ids() -> Vec<&'static str>` — returns every event ID the menu emits. This is the assertion target for Phase 1's test.
2. `build_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>>` — constructs the full native menu via Tauri 2's `MenuBuilder` / `SubmenuBuilder` / `MenuItemBuilder` API.
3. `dispatch_menu_event<R: Runtime>(app: &AppHandle<R>, event_id: &str)` — pure handler that takes an event ID string and calls `app.emit("menu", event_id)` so the React frontend can subscribe via `@tauri-apps/api/event::listen("menu", ...)`.

`dispatch_menu_event` is split out as a separate function (rather than inlined into a closure) so that:
- The closure registered with `Builder::on_menu_event` in Phase 3 is small and reads naturally.
- The dispatch step has a named entry point future testing (Wave-2+) can hook if it ever becomes testable.
- The Phase-3 `lib.rs` change is mechanical and reviewer-trivial.

### Files

- **Create:** `src-tauri/src/menu.rs`

### Exact menu structure and event IDs

The menu is seven top-level submenus, in order: **File · Message · Session · Mailbox · View · Tools · Help**. Item-by-item layout (the source of truth for this is `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` Task 7 Steps 3 + Step 3-bis, reconciled with `docs/design/v0.0.1-ux-mockups.md` §7):

| Submenu | Label | Event ID | Accelerator | Origin |
|---|---|---|---|---|
| File | New Message | `menu:file:new` | `CmdOrCtrl+N` | baseline |
| File | _(separator)_ | — | — | baseline |
| File | Quit | `menu:file:quit` | `CmdOrCtrl+Q` | baseline |
| Message | Reply | `menu:message:reply` | `CmdOrCtrl+R` | baseline |
| Message | Reply All | `menu:message:reply_all` | `CmdOrCtrl+Shift+R` | baseline |
| Message | Forward | `menu:message:forward` | _none_ | baseline |
| Message | Print | `menu:message:print` | `CmdOrCtrl+P` | baseline |
| Session | Connect | `menu:session:connect` | `F5` | baseline |
| Session | Disconnect | `menu:session:disconnect` | _none_ | baseline |
| Session | _(separator)_ | — | — | baseline |
| Session | Session Log | `menu:session:log` | _none_ | baseline |
| Session | Test send | `menu:session:test_send` | _none_ | AMD-10 wizard half |
| Session | Show transport | `menu:session:show_transport` | _none_ | AMD-10 runtime half |
| Mailbox | Inbox | `menu:mailbox:inbox` | _none_ | baseline |
| Mailbox | Sent | `menu:mailbox:sent` | _none_ | baseline |
| Mailbox | Outbox | `menu:mailbox:outbox` | _none_ | baseline |
| View | Toggle Session Log | `menu:view:session_log` | `CmdOrCtrl+Shift+L` | baseline + AMD-10 accel |
| View | Show Raw Session Log | `menu:view:raw_log` | _none_ | AMD-10 runtime half |
| View | Toggle Status Bar | `menu:view:status_bar` | _none_ | baseline |
| View | Show Radio Dock | `menu:view:radio_dock` | `CmdOrCtrl+Shift+M` | AMD-10 runtime half |
| Tools | Templates | `menu:tools:templates` | _none_ | baseline |
| Tools | Rig Control | `menu:tools:rig_control` | _none_ | baseline |
| Tools | _(separator)_ | — | — | baseline |
| Tools → Settings | Connection | `menu:tools:settings_connection` | _none_ | AMD-10 runtime half |
| Tools → Settings → Privacy | GPS state | `menu:tools:settings_privacy_gps` | _none_ | AMD-10 runtime half |
| Tools → Settings → Privacy | Position precision | `menu:tools:settings_privacy_position` | _none_ | AMD-10 runtime half |
| Tools → Settings | GPS | `menu:tools:settings_gps` | _none_ | AMD-10 runtime half |
| Tools | Preferences | `menu:tools:preferences` | _none_ | baseline |
| Help | About Tuxlink | `menu:help:about` | _none_ | baseline |
| Help | Documentation | `menu:help:docs` | _none_ | baseline |
| Help | Report Issue | `menu:help:report_issue` | _none_ | baseline |

The nested "Settings" submenu under Tools is itself a submenu with three children: `Connection`, `Privacy` (which is itself a submenu containing `GPS state` and `Position precision`), and `GPS`. The construction order in code mirrors the table.

The `menu_event_ids()` return order MUST match Phase 1's test list — the test uses `contains`, so order does not affect correctness, but matching the test list makes diff-review trivial.

### Exact Rust code

```rust
// src-tauri/src/menu.rs
//
// Native OS menu bar. Categories per docs/ux-anti-patterns.md
// "Required elements (not negotiable for v0.1)" + AMD-10 additions
// from docs/design/v0.0.1-ux-mockups.md §7.
//
// Menu items emit Tauri events of the form "menu:{category}:{action}".
// The React frontend subscribes via @tauri-apps/api/event::listen("menu", ...).
//
// Pure-function `menu_event_ids()` exposes the full set for regression
// testing (see src-tauri/tests/menu_test.rs); the build_menu /
// dispatch_menu_event pair is the live runtime path.

use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// Returns every menu event ID the menu emits, in submenu order.
/// Pure function; safe to call without a Tauri runtime.
pub fn menu_event_ids() -> Vec<&'static str> {
    vec![
        // File
        "menu:file:new", "menu:file:quit",
        // Message
        "menu:message:reply", "menu:message:reply_all",
        "menu:message:forward", "menu:message:print",
        // Session (baseline + AMD-10 wizard half + AMD-10 runtime half)
        "menu:session:connect", "menu:session:disconnect", "menu:session:log",
        "menu:session:test_send",         // AMD-10 wizard half
        "menu:session:show_transport",    // AMD-10 runtime half
        // Mailbox
        "menu:mailbox:inbox", "menu:mailbox:sent", "menu:mailbox:outbox",
        // View (baseline + AMD-10 runtime half)
        "menu:view:session_log", "menu:view:status_bar",
        "menu:view:raw_log",              // AMD-10 runtime half
        "menu:view:radio_dock",           // AMD-10 runtime half
        // Tools (baseline + AMD-10 runtime half)
        "menu:tools:templates", "menu:tools:rig_control", "menu:tools:preferences",
        "menu:tools:settings_connection",         // AMD-10 runtime half
        "menu:tools:settings_privacy_gps",        // AMD-10 runtime half
        "menu:tools:settings_privacy_position",   // AMD-10 runtime half
        "menu:tools:settings_gps",                // AMD-10 runtime half
        // Help
        "menu:help:about", "menu:help:docs", "menu:help:report_issue",
    ]
}

/// Constructs the full native menu bar. Call from `setup` and pass to
/// `app.set_menu(...)`. Requires a live `AppHandle` (cannot be unit-tested
/// in isolation; see `menu_event_ids` for the unit-test seam).
pub fn build_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let file = SubmenuBuilder::new(app, "File")
        .item(&MenuItemBuilder::with_id("menu:file:new", "New Message").accelerator("CmdOrCtrl+N").build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id("menu:file:quit", "Quit").accelerator("CmdOrCtrl+Q").build(app)?)
        .build()?;

    let message = SubmenuBuilder::new(app, "Message")
        .item(&MenuItemBuilder::with_id("menu:message:reply", "Reply").accelerator("CmdOrCtrl+R").build(app)?)
        .item(&MenuItemBuilder::with_id("menu:message:reply_all", "Reply All").accelerator("CmdOrCtrl+Shift+R").build(app)?)
        .item(&MenuItemBuilder::with_id("menu:message:forward", "Forward").build(app)?)
        .item(&MenuItemBuilder::with_id("menu:message:print", "Print").accelerator("CmdOrCtrl+P").build(app)?)
        .build()?;

    let session = SubmenuBuilder::new(app, "Session")
        .item(&MenuItemBuilder::with_id("menu:session:connect", "Connect").accelerator("F5").build(app)?)
        .item(&MenuItemBuilder::with_id("menu:session:disconnect", "Disconnect").build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id("menu:session:log", "Session Log").build(app)?)
        .item(&MenuItemBuilder::with_id("menu:session:test_send", "Test send").build(app)?)             // AMD-10
        .item(&MenuItemBuilder::with_id("menu:session:show_transport", "Show transport").build(app)?)   // AMD-10
        .build()?;

    let mailbox = SubmenuBuilder::new(app, "Mailbox")
        .item(&MenuItemBuilder::with_id("menu:mailbox:inbox", "Inbox").build(app)?)
        .item(&MenuItemBuilder::with_id("menu:mailbox:sent", "Sent").build(app)?)
        .item(&MenuItemBuilder::with_id("menu:mailbox:outbox", "Outbox").build(app)?)
        .build()?;

    let view = SubmenuBuilder::new(app, "View")
        .item(&MenuItemBuilder::with_id("menu:view:session_log", "Toggle Session Log").accelerator("CmdOrCtrl+Shift+L").build(app)?)
        .item(&MenuItemBuilder::with_id("menu:view:raw_log", "Show Raw Session Log").build(app)?)        // AMD-10
        .item(&MenuItemBuilder::with_id("menu:view:status_bar", "Toggle Status Bar").build(app)?)
        .item(&MenuItemBuilder::with_id("menu:view:radio_dock", "Show Radio Dock").accelerator("CmdOrCtrl+Shift+M").build(app)?)  // AMD-10
        .build()?;

    // Settings nested submenu under Tools (AMD-10).
    let settings_privacy = SubmenuBuilder::new(app, "Privacy")
        .item(&MenuItemBuilder::with_id("menu:tools:settings_privacy_gps", "GPS state").build(app)?)
        .item(&MenuItemBuilder::with_id("menu:tools:settings_privacy_position", "Position precision").build(app)?)
        .build()?;
    let settings = SubmenuBuilder::new(app, "Settings")
        .item(&MenuItemBuilder::with_id("menu:tools:settings_connection", "Connection").build(app)?)
        .item(&settings_privacy)
        .item(&MenuItemBuilder::with_id("menu:tools:settings_gps", "GPS").build(app)?)
        .build()?;

    let tools = SubmenuBuilder::new(app, "Tools")
        .item(&MenuItemBuilder::with_id("menu:tools:templates", "Templates").build(app)?)
        .item(&MenuItemBuilder::with_id("menu:tools:rig_control", "Rig Control").build(app)?)
        .separator()
        .item(&settings)                                                                    // AMD-10
        .item(&MenuItemBuilder::with_id("menu:tools:preferences", "Preferences").build(app)?)
        .build()?;

    let help = SubmenuBuilder::new(app, "Help")
        .item(&MenuItemBuilder::with_id("menu:help:about", "About Tuxlink").build(app)?)
        .item(&MenuItemBuilder::with_id("menu:help:docs", "Documentation").build(app)?)
        .item(&MenuItemBuilder::with_id("menu:help:report_issue", "Report Issue").build(app)?)
        .build()?;

    MenuBuilder::new(app)
        .items(&[&file, &message, &session, &mailbox, &view, &tools, &help])
        .build()
}

/// Emit a menu event to the frontend. Pure dispatcher (no decisioning).
/// The frontend subscribes via `@tauri-apps/api/event::listen("menu", ...)`
/// and switches on the payload string (the menu event ID).
pub fn dispatch_menu_event<R: Runtime>(app: &AppHandle<R>, event_id: &str) {
    // Best-effort emit; if the frontend isn't subscribed yet (early startup
    // or compose-window-only state), the event is silently dropped. This
    // matches Tauri's built-in event semantics and is the right shape for
    // a menu — menu interactions are user-initiated and a missed event
    // means the user didn't see a response; the user will retry. A panic
    // here would crash the GUI thread for a missed user click, which is
    // wrong.
    let _ = app.emit("menu", event_id.to_string());
}
```

The `let _ = app.emit(...)` (rather than `.expect()` or `unwrap()`) is intentional and is the right choice here — see the inline comment for why. DO NOT change to `unwrap()` "for safety"; that would crash the app on a transient frontend-not-ready window.

The `use tauri::{... Emitter, Manager, Runtime};` line: `Emitter` is required to bring `.emit` into scope on `AppHandle`; `Manager` is required for any `AppHandle` ergonomics the executor may need; `Runtime` is the generic bound. If the compiler complains "unused import: Manager", remove it — `Manager` is included as a defensive measure for the Phase 3 wiring but is not strictly required for `menu.rs` itself.

### Pitfall review for Phase 2

- **implementation-pitfalls.md SCOPE-1 (RMS Express vs Trimode):** N/A — menu items are all client-side actions, no gateway/MPS/RMS Relay terminology.
- **implementation-pitfalls.md RADIO-1 / RADIO-2:** N/A — this phase ships NO code path that transmits. `menu:session:connect` and `menu:session:test_send` emit Tauri events; the React side's handlers (Tasks 11, 12+) are responsible for any actual radio interaction, and Task 11's wizard already has the operator-consent gate. Menu items emit only.
- **implementation-pitfalls.md HOOK-1 / LEASE-1 / PARITY-1:** N/A — Phase 2 does not touch hooks, leases, or path resolution.
- **implementation-pitfalls.md ORCH-1 (parallel-subagent persistence):** N/A — Task 7 is dispatched as a single subagent (one Wave-2 executor for this whole plan); no further parallel-subagent dispatch occurs within the task.
- **implementation-pitfalls.md BD-1 (bd opinionated tooling):** N/A — Task 7 makes no changes to CLAUDE.md / AGENTS.md / `.claude/settings.json`. The executor reads the existing `## Tool referee` section in CLAUDE.md and follows it (TodoWrite for in-turn micro-progress within the task; bd for cross-session work tracking via the claim of `tuxlink-6vi`).
- **testing-pitfalls.md (all):** Phase 2 has no test code itself; the tests live in Phase 1. Phase 2 is the implementation that turns Phase 1's red → green in Phase 4. The error path inside `dispatch_menu_event` (`app.emit()` returns a `Result` which is intentionally discarded with `let _ =`) is NOT covered by Phase 1's test — testing-pitfalls.md §3 (Error Path Coverage) would normally flag this. The intentional non-coverage is justified inline in `dispatch_menu_event`'s code comment: emit-to-frontend is fire-and-forget by Tauri's semantics, and the user-visible failure mode of a dropped event (the user clicks a menu and nothing happens) is acceptable for v0.0.1 since the user retries. Adding an emit-success assertion would require either (a) standing up a full `tauri::App` in a test (currently impractical) or (b) injecting a mock emitter (currently over-engineering for one error path). Either is out of scope for Task 7.
- **testing-pitfalls.md §6 (Boundary & Configuration Validation):** the accelerator strings ("CmdOrCtrl+N", "F5", "CmdOrCtrl+Shift+M", etc.) ARE a form of configuration. A typo (e.g. "CmdorCtrl+N" lowercase O) would be silently rejected by Tauri at registration time and the accelerator would simply not fire. Phase 4's `cargo test --test menu_test` does NOT catch this (the test only inspects `menu_event_ids`, not the accelerator strings). The catch is Phase 5's manual smoke — the operator checklist explicitly tests Ctrl+N, Ctrl+Q, F5, Ctrl+Shift+M, Ctrl+Shift+L. If a future amendment adds an accelerator NOT in the smoke checklist, the smoke checklist MUST be updated alongside.
- **ux-anti-patterns.md "in-content toolbars that duplicate menu items":** This phase DOES NOT create any toolbar or floating button. The native menu bar IS the source of truth for global actions. The spec already calls this out as a "DO NOT" in the source plan (line 1725); reaffirm it here so the Phase-2 executor doesn't get clever.

### Assertion-rigor reminder (concurrency / timing — N/A here but boilerplate per writing-plans-enhanced)

Phase 2's `dispatch_menu_event` does involve an event-emission boundary that's technically asynchronous (Tauri's event bus). But there are no test assertions on emission ordering or delivery, so the "if test assertions race, fix synchronization not assertions" boilerplate is non-binding for Phase 2. Boilerplate retained per writing-plans-enhanced for executor-clarity in case a future amendment adds an emission-test that races.

### Ordering deps

- Depends on: nothing else in Task 7 (this is the implementation phase).
- Touches files: `src-tauri/src/menu.rs` (new). Phase 3 adds the `pub mod menu;` line to `src-tauri/src/lib.rs`; do NOT add it here — keep the two-file change cleanly split.
- DOES NOT touch: `src-tauri/src/main.rs` (untouched per D2), `src-tauri/src/lib.rs::run()` (Phase 3's responsibility), `src-tauri/Cargo.toml` (no new dependencies — `tauri::menu` ships in the `tauri = "2"` crate already in deps).

### DO NOT

- DO NOT add `tauri-plugin-global-shortcut` or any other dependency. Menu accelerators are first-class in `MenuItemBuilder::accelerator(...)` and work without extra plugins. Adding a plugin "for keyboard shortcut robustness" is over-engineering and would add a permissions / capabilities config burden that isn't needed.
- DO NOT add `CheckMenuItem` or stateful menu items. The toggle items (Show Session Log, Show Radio Dock, Show Raw Session Log, Toggle Status Bar) are plain MenuItems in this task — the visual checkmark state is owned by the React frontend (Tasks 15, 16, 16.5) which decides when the underlying pane is visible. If the menu later needs visible checkmarks, that's a separate amendment.
- DO NOT wire menu actions to Rust-side logic. The dispatcher emits a `"menu"` event; React handles all menu actions. Wiring "menu:session:connect" directly to a Pat-control function in Rust would couple menu UI to backend logic and bypass React's state model (which Tasks 12+ depend on).
- DO NOT add `#[cfg(target_os = "macos")]` or other platform-conditional code. Tuxlink is Linux-first for v0.0.1 (per Task 1's scaffold + the project's Pi 5 target); Tauri's `CmdOrCtrl` accelerator-string handles the per-OS mapping automatically.
- DO NOT inline `dispatch_menu_event` into the Phase-3 closure registration. The function exists as a named entry point both for clarity and to give Phase 3 a one-line call site.

---

## Phase 3 — Register the menu in `lib.rs::run()` + `pub mod menu;`

**Execution Status:** ⬜ NOT STARTED

### What this phase does

Wire Phase 2's menu construction + event dispatch into the live app. Two surgical edits to `src-tauri/src/lib.rs`:

1. Add `pub mod menu;` near the top alongside the existing `pub mod config;`, `pub mod pat_client;`, `pub mod pat_process;` lines.
2. In `run()`, chain `.on_menu_event(...)` onto `tauri::Builder::default()` BEFORE `.setup(...)`, and add menu construction + `app.set_menu(menu)` INSIDE `.setup(...)`.

This is two narrow diffs on one file. No other file changes in Phase 3.

### Files

- **Modify:** `src-tauri/src/lib.rs`
- **NOT modified:** `src-tauri/src/main.rs` (per D2 — `main.rs` is a one-line shim calling `tuxlink_lib::run()`; no edits needed there).

### Exact starting state of `lib.rs`

(For reference, what the executor will find on the branch off `feat/v0.0.1` — assuming Tasks 1–6 have shipped and no other Wave-2 task has edited `lib.rs` in the meantime.)

```rust
pub mod config;
pub mod pat_client;
pub mod pat_process;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

If `lib.rs` differs materially from the above (e.g., another in-flight Wave-2 task already added a plugin or a setup hook), STOP and surface to the dispatcher rather than guessing the merge — see "Cross-task conflict surface" below.

### Exact ending state of `lib.rs`

```rust
pub mod config;
pub mod menu;
pub mod pat_client;
pub mod pat_process;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![greet])
        .on_menu_event(|app, event| {
            menu::dispatch_menu_event(app, event.id().as_ref());
        })
        .setup(|app| {
            let menu = menu::build_menu(app.handle())?;
            app.set_menu(menu)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

The two changes are:

1. `pub mod menu;` inserted alphabetically between `pub mod config;` and `pub mod pat_client;`.
2. Inside `run()`, after `.invoke_handler(...)` and before `.run(...)`:
   - `.on_menu_event(|app, event| menu::dispatch_menu_event(app, event.id().as_ref()))` — registers the application-wide menu-event listener. `event.id()` returns `&MenuId`; `.as_ref()` borrows it as `&str` for the dispatcher.
   - `.setup(|app| { let menu = menu::build_menu(app.handle())?; app.set_menu(menu)?; Ok(()) })` — builds the menu at app startup and installs it. `app.handle()` returns `&AppHandle<R>` which is what `build_menu` needs.

Note on closure-argument typing: `tauri::Builder::on_menu_event`'s closure signature in Tauri 2.x is `Fn(&AppHandle<R>, MenuEvent) + Send + Sync + 'static` (verified against `tauri 2.9.5` source via `https://docs.rs/tauri/2.9.5/tauri/struct.Builder.html#method.on_menu_event`). The closure body calls `menu::dispatch_menu_event(app, event.id().as_ref())` — `app` is `&AppHandle<R>` exactly matching `dispatch_menu_event`'s signature. If the Tauri version in `Cargo.lock` is materially different (3.x for instance), the executor MUST verify the signature against the installed crate's docs before assuming this snippet compiles.

### TDD discipline (mandatory)

**BEFORE starting work:**

1. Invoke `/superpowers:test-driven-development`.
2. Read `docs/pitfalls/testing-pitfalls.md` (§§1–7).
3. The relevant test for Phase 3 is Phase 1's `menu_event_ids` test, which DOES NOT exercise `run()` (no GUI in test). Phase 3's correctness verification is therefore the Phase 5 manual smoke + the fact that `cargo build` succeeds.

**BEFORE marking this phase complete:**

1. Run `cargo build` inside `src-tauri/` and confirm no errors. (Warnings on unused imports — particularly `Manager` from Phase 2 — are acceptable but if you can drop the unused import without breaking, do so.)
2. Run `cargo test --test menu_test` and confirm Phase 1's test still passes (Phase 3 doesn't change `menu.rs` directly, but the `pub mod menu;` edit affects the crate-graph and a typo there would break the test path).
3. Run `cargo clippy` and address any new clippy lints introduced by the lib.rs edits. Pre-existing lints from Tasks 1–6 are not in scope.

### Pitfall review for Phase 3

- **implementation-pitfalls.md SCOPE-1:** N/A — same as Phase 2.
- **implementation-pitfalls.md RADIO-1 / RADIO-2:** N/A — `lib.rs::run()` still does not transmit; the dispatcher only emits events to React.
- **implementation-pitfalls.md HOOK-1 / LEASE-1 / PARITY-1:** N/A — no hook / lease / path-resolution touched.
- **testing-pitfalls.md §7 (Test Infrastructure Hygiene):** Phase 3 does NOT add tests. Its correctness verification is `cargo build` + the unchanged Phase 1 test + Phase 5's manual smoke.

### Cross-task conflict surface

`src-tauri/src/lib.rs` is the integration point for ALL Wave-2 backend tasks (Task 8 system tray adds its own `.setup`/`.on_tray_icon_event` chain; Tasks 12+ may add more `invoke_handler` registrations). When two Wave-2 tasks land in parallel and both touch `lib.rs`:

- The merge conflict is mechanical (both add lines to the same builder chain).
- The resolution is order-preserving: chain calls in the order `(invoke_handler, on_menu_event, on_tray_icon_event, setup, run)`.
- For Task 7 specifically, this plan adds `on_menu_event` + `setup` content. Task 8 (system tray) will add `on_tray_icon_event` + extend the same `setup` to register tray. Wave-2 ordering should merge Task 7 first, then Task 8 can extend `setup` rather than rewrite it.

**If the executor encounters a `lib.rs` that ALREADY contains `.on_menu_event(...)` or a non-empty `.setup(...)`:** STOP and surface to the dispatcher. Do NOT merge speculatively — the dispatcher needs to know which Wave-2 ordering produced the state. Note this in the Discoveries subsection at the top of this plan before stopping.

### Ordering deps

- Depends on: Phase 2 (`menu.rs` must exist for `pub mod menu;` to resolve).
- Blocks: Phase 4 (the green-test verification) and Phase 5 (the manual smoke).
- Touches files: `src-tauri/src/lib.rs` only.

### DO NOT

- DO NOT touch `src-tauri/src/main.rs`. It is a one-line shim; modifying it duplicates effort with no behavioral effect and increases merge surface with future Wave-2 tasks.
- DO NOT remove the existing `greet` `#[tauri::command]` example. It's harmless dead code from the Tauri scaffold; Task 12+ will replace it with real commands. Removing it in Task 7 mixes concerns and complicates code review.
- DO NOT add `.plugin(...)` calls for `tauri-plugin-global-shortcut`. Same reason as Phase 2: menu-attached accelerators are sufficient for Task 7's needs.
- DO NOT change the `tauri::Builder::default()` chain order arbitrarily. The order above matches Tauri 2.x convention; reviewers will read the diff against that expectation.

---

## Phase 4 — Green the unit test (`cargo test --test menu_test`)

**Execution Status:** ⬜ NOT STARTED

### What this phase does

Run the Phase 1 test against the Phase 2 implementation, confirm green. This is the "TDD verify" step — explicit phase to make the green-the-test action a checkpoint rather than implicit boilerplate.

### Files

- **No files modified.** This phase is a verification action only.

### Exact actions

```bash
cd src-tauri
cargo test --test menu_test
```

Expected output (substring-match, do not require exact line-count match):

```
running 1 test
test test_menu_exposes_required_event_ids ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; ...
```

If the test fails: read the assertion message — it names the missing event ID. The missing ID is either:
- Absent from `menu.rs::menu_event_ids()` — add it to the vec.
- Misspelled in one of the two places (test or impl) — fix the typo.
- Present but in a stale typo of the prefix (`"menu:tools:settings_privacy_pos"` instead of `"...position"`, etc.) — match the table in Phase 2.

If the test passes: also run `cargo test` (full test suite) to confirm Phase 3's `lib.rs` edit didn't break `config_test`, `pat_client_test`, or `pat_process_test`. Pre-existing failures from other tasks are not Task 7's responsibility but a NEW failure introduced by Task 7 is.

### Pitfall review for Phase 4

- **testing-pitfalls.md §1 (Test Output Pristine):** green-state output is the standard `cargo test` summary; verify no `println!`/`dbg!`/`eprintln!` leaks from `menu.rs`. None expected from the Phase 2 code.
- **testing-pitfalls.md §2 (Skipped Tests Are Not Passing Tests):** verify the report says `0 ignored`. If anything is ignored, the executor should have flagged it earlier.

### Ordering deps

- Depends on: Phase 1 (test exists), Phase 2 (impl exists), Phase 3 (`pub mod menu` exports it).
- Blocks: Phase 5.

### DO NOT

- DO NOT weaken the test to make it pass. If an assertion fails, the fix is in the implementation, not the test. See writing-plans-enhanced's "Preserve assertion rigor under pressure" rule — applies here even though this phase isn't under timing pressure.
- DO NOT add `#[ignore]` to the test. The whole point of Phase 1's test is to be the regression floor for menu-event-ID changes; an ignored test is a deleted test.

---

## Phase 5 — Manual smoke via `pnpm tauri dev`

**Execution Status:** ⬜ NOT STARTED

### What this phase does

GUI verification: launch the app, see the menu, click through it. This is a MANUAL verification step (per the plan's "Manual Verification Tax" section at lines 4751–4762) because Tauri menu rendering is GUI-dependent and the project has no GUI test harness in v0.0.1.

The executor (an AI agent) cannot launch GUIs interactively in a subagent shell. Therefore Phase 5 has TWO modes of completion:

**Mode A — agent execution:** The agent runs `pnpm tauri dev` headlessly in the background, captures stdout/stderr, and verifies the app started without panics (Rust + Vite both ready). Click-through verification is delegated to the operator (Cameron) via a PR-body checklist; the agent's job is to give the operator a clean "ready for smoke" baseline.

**Mode B — operator execution:** The operator (Cameron) runs `pnpm tauri dev` interactively after the PR is opened, walks the menu, and reports back. The PR body has the checklist below; the operator ticks items.

For Wave 2's dispatch, Mode A is the default — the agent ships the PR, the operator smokes it before merge.

### Exact agent actions (Mode A)

Run from the Wave-2 worktree root (the executor's worktree, NOT the plan-writing worktree). The exact path is whatever `new_tuxlink_worktree.py --issue tuxlink-6vi --slug <slug>` produced in Phase 0 — typically `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-6vi-<slug>`.

```bash
WORKTREE=$(git rev-parse --show-toplevel)
cd "$WORKTREE"

# Headless smoke: start the dev server, capture stdout+stderr to a log,
# wait for the "ready" / "error" / "panic" signal within 90s.
LOG=/tmp/tauri-dev-task7-$$.log
pnpm tauri dev > "$LOG" 2>&1 &
DEV_PID=$!

# Poll the log for any of: "ready in" (Vite ready), "Compiling tuxlink" + later "Finished"
# (Rust compile done), or "error"/"panic" (failure).
timeout 90 bash -c "while ! grep -E 'ready in|Finished|error|panic' '$LOG' >/dev/null 2>&1; do sleep 1; done"

# After 90s, classify outcome.
if kill -0 "$DEV_PID" 2>/dev/null; then
    echo "OK: dev server alive after 90s"
    if grep -q -i "error\|panic" "$LOG"; then
        echo "WARN: process alive but log contains error/panic — inspect $LOG"
    fi
else
    echo "FAIL: dev server died — inspecting last 50 lines of $LOG:"
    tail -50 "$LOG"
fi

# Cleanup. SIGTERM first; SIGKILL as last resort.
kill "$DEV_PID" 2>/dev/null
sleep 2
kill -9 "$DEV_PID" 2>/dev/null
wait "$DEV_PID" 2>/dev/null  # reap
```

This is best-effort smoke — `pnpm tauri dev` on a headless Pi may not actually open a window (no display server), but it WILL compile both Rust and JS and will write any panics or compile errors to the log. If the headless invocation surfaces a Rust panic or a Vite compile error, Phase 5 has failed and the executor must fix and re-run.

If the headless invocation can't be done in the agent's shell (no display server, no X forwarding, hooks block long-running background processes), the agent falls back to:

```bash
cd "$(git rev-parse --show-toplevel)/src-tauri"
cargo build  # compile-only; confirms Rust side at least builds
# Frontend isn't strictly needed for Task 7 to be correct (the menu is Rust-side)
# but a passing cargo build is the bare minimum.
```

Either way, the executor records the actual smoke outcome (full successful smoke, partial headless smoke, or compile-only smoke) in the PR body's "Manual Verification" section.

### Operator-facing manual checklist (Mode B — goes in the PR body)

The PR body should include this checklist verbatim for Cameron to tick:

```markdown
## Manual smoke (per `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` "Manual Verification Tax")

Launch with `pnpm tauri dev` on the dev Pi (Pi 5 + Ubuntu 24.04 / equivalent).

- [ ] App window appears with a native menu bar at the top.
- [ ] Menu order is: **File · Message · Session · Mailbox · View · Tools · Help**.
- [ ] Click **File** → see *New Message* (Ctrl+N) and *Quit* (Ctrl+Q), separated.
- [ ] Click **Message** → see *Reply* (Ctrl+R), *Reply All* (Ctrl+Shift+R), *Forward*, *Print* (Ctrl+P).
- [ ] Click **Session** → see *Connect* (F5), *Disconnect*, separator, *Session Log*, *Test send*, *Show transport*.
- [ ] Click **Mailbox** → see *Inbox*, *Sent*, *Outbox*.
- [ ] Click **View** → see *Toggle Session Log* (Ctrl+Shift+L), *Show Raw Session Log*, *Toggle Status Bar*, *Show Radio Dock* (Ctrl+Shift+M).
- [ ] Click **Tools** → see *Templates*, *Rig Control*, separator, *Settings* (nested submenu), *Preferences*.
- [ ] Hover **Tools → Settings** → submenu opens with *Connection*, *Privacy* (nested), *GPS*.
- [ ] Hover **Tools → Settings → Privacy** → submenu opens with *GPS state*, *Position precision*.
- [ ] Click **Help** → see *About Tuxlink*, *Documentation*, *Report Issue*.
- [ ] Try **Ctrl+N**, **Ctrl+Q**, **F5**, **Ctrl+Shift+M**, **Ctrl+Shift+L**: keyboard accelerators fire menu items (visible by the menu briefly highlighting or by the dev console logging the `menu:*` event).
- [ ] Open the dev console (Tauri's right-click → Inspect Element → Console). Click any menu item. Verify a `menu` event fires with the corresponding ID as payload — this confirms the React-listener wiring path works end-to-end on the Rust side. (React doesn't subscribe yet in Task 7; verification is "an event was emitted," not "the UI reacted.")
- [ ] Quit via **File → Quit**. App exits cleanly. (No leftover process — `ps aux | grep tuxlink` returns nothing.)
```

The operator may report any subset of the checklist; full pass is the merge gate. Anything that fails surfaces back to the executor (a follow-up commit on the same branch, NOT a new PR).

### Pitfall review for Phase 5

- **implementation-pitfalls.md RADIO-1:** Phase 5 starts the app but does NOT exercise Session → Connect or Session → Test send to the point of actually contacting CMS. The dev console will log the `menu:session:connect` event when Connect is clicked, but no transmission occurs (the React handler that would consume that event isn't shipped yet — Task 12+). This is correct: Task 7 ships only the menu, not the actions.
- **testing-pitfalls.md §1 (Test Output Pristine):** N/A — Phase 5 is manual GUI smoke, not automated test output.

### Ordering deps

- Depends on: Phase 4 green.
- Blocks: Phase 6 commit (the executor SHOULD have a recorded smoke outcome — even if partial — before committing).

### DO NOT

- DO NOT skip Phase 5 with "looks good, shipping." The whole point of the Manual Verification Tax section in the source plan is to surface that Tauri menu correctness can't be unit-tested end-to-end; a CI-style "tests pass" claim does not substitute for the operator-visible "menu items are there and click correctly."
- DO NOT mark the PR as "ready for review" without recording the smoke outcome in the PR body. If the agent's environment can't do the full smoke, record "headless cargo-build smoke only; operator full smoke required before merge" — explicit incompleteness beats implicit ambiguity.
- DO NOT attempt to bypass the manual smoke by writing a `tauri::test`-style integration test that constructs an `App` and inspects the menu. Tauri 2.x's testing API for menus is immature; building one is a separate, much larger task and is not in Task 7's scope.

---

## Phase 6 — Commit + push + PR against `feat/v0.0.1`

**Execution Status:** ⬜ NOT STARTED

### What this phase does

Commit, push, open PR. Use heredoc commit syntax (mandatory per CLAUDE.md to avoid the destructive-git hook's substring match on commit messages containing banned-pattern text).

### Files

- **Stage:** `src-tauri/src/menu.rs`, `src-tauri/src/lib.rs`, `src-tauri/tests/menu_test.rs`.
- **Verify NOT staged:** any `.beads/embeddeddolt/*` (bd-owned, auto-managed by `bd` commands; do not stage manually).
- **Verify NOT modified:** `src-tauri/src/main.rs` (must remain a one-line shim), `src-tauri/Cargo.toml` (no new deps), `src-tauri/tauri.conf.json` (no config change).

### Exact actions

```bash
# Run from the Wave-2 executor's worktree (cd to it if not already there).
cd "$(git rev-parse --show-toplevel)"

# Stage exactly the three files Task 7 produces.
git add src-tauri/src/menu.rs src-tauri/src/lib.rs src-tauri/tests/menu_test.rs

# Confirm staged contents.
git diff --staged --stat
# Expected output: 3 files changed (menu.rs new, lib.rs modified, menu_test.rs new).

# Commit (heredoc-style mandatory).
git commit -m "$(cat <<'EOF'
feat(ui): native OS menu bar with File/Message/Session/Mailbox/View/Tools/Help (AMD-10)

Construct a native Tauri 2 menu bar via tauri::menu::{MenuBuilder,
SubmenuBuilder, MenuItemBuilder}. Menu items emit Tauri events of the
form "menu:{category}:{action}" consumed by the React frontend (handlers
arrive in Tasks 11, 12+).

Includes both AMD-10 halves per docs/design/v0.0.1-ux-mockups.md §7:
- wizard half: Session -> Test send
- runtime half: Session -> Show transport, View -> Show Raw Session Log
  + Show Radio Dock (Ctrl+Shift+M), Tools -> Settings -> {Connection,
  Privacy -> {GPS state, Position precision}, GPS}

Keyboard accelerators per docs/ux-anti-patterns.md "Required elements"
plus Ctrl+Shift+L (Toggle Session Log) and Ctrl+Shift+M (Show Radio
Dock) per AMD-10.

menu_event_ids() exposes the full event-ID set for regression testing
in src-tauri/tests/menu_test.rs (Tauri menu objects can't be asserted
from Rust unit tests since they hold platform handles; the indirection
is the test seam).

ANTI-PATTERN REVIEW: none. The native menu bar is the source of truth
for global actions; no toolbar is added.

Agent: <YOUR-MONIKER-FROM-PHASE-0>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"

# Push to origin. Branch was created by `new_tuxlink_worktree.py --issue tuxlink-6vi`
# at Wave-2 worktree-creation time; the convention is `bd-tuxlink-6vi/<slug>`
# where <slug> is whatever the executor passed to --slug. Use the actual branch
# name from `git branch --show-current`.
BRANCH=$(git branch --show-current)
git push -u origin "$BRANCH"

# Open PR against feat/v0.0.1.
gh pr create \
  --base feat/v0.0.1 \
  --head "$BRANCH" \
  --title '[<moniker>] feat(ui): Task 7 — Native OS menu bar (AMD-10 unified)' \
  --body "$(cat <<'EOF'
## Summary

Native OS menu bar via Tauri 2's `tauri::menu` module. Both AMD-10 halves
(wizard + runtime) shipped as a single coherent menu per
`docs/design/v0.0.1-ux-mockups.md` §7.

- 7 top-level submenus: **File · Message · Session · Mailbox · View · Tools · Help**
- 25 menu items + 1 nested "Settings" submenu under Tools (with a further
  nested "Privacy" submenu).
- 9 keyboard accelerators (Ctrl+N, Ctrl+R, Ctrl+Shift+R, Ctrl+P, Ctrl+Q,
  F5, Ctrl+Shift+L, Ctrl+Shift+M plus the Quit/Print/etc. baseline).
- Menu items emit Tauri events of the form `menu:{category}:{action}`.
  The React frontend subscribes via `@tauri-apps/api/event::listen("menu", ...)`
  (consumers ship in Tasks 11, 12+).

## Spec source

- Spec: `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` §"Task 7" (lines 1703–1981)
  as amended by AMD-10.
- Design: `docs/design/v0.0.1-ux-mockups.md` §7.
- Anti-patterns: `docs/ux-anti-patterns.md` §"Required elements (not negotiable for v0.1)"
  + "Forbidden elements" — verified no toolbar duplicates the menu bar.

## Plan source

This PR executes `docs/plans/2026-05-18-task-7-native-menu-bar-plan.md`.
Phases 1–6 ticked in that plan's Execution Status table.

## Discoveries during execution

(Filled in at execution time per the plan's Living Document Contract.)

## Tests

- New: `src-tauri/tests/menu_test.rs::test_menu_exposes_required_event_ids`
  — asserts every required menu event ID is present in `menu_event_ids()`.
- Existing: `cargo test` for the rest of the suite confirms Phase 3's
  `lib.rs` edit didn't break Tasks 2, 3, 5 tests.

## Manual smoke (per `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` "Manual Verification Tax")

(Operator: tick the checklist below after pulling the branch and running
`pnpm tauri dev` on the dev Pi. Replace each `[ ]` with `[x]` as you go
or leave a note for the failure.)

- [ ] App window appears with a native menu bar at the top.
- [ ] Menu order is **File · Message · Session · Mailbox · View · Tools · Help**.
- [ ] **File** → *New Message* (Ctrl+N), separator, *Quit* (Ctrl+Q).
- [ ] **Message** → *Reply* (Ctrl+R), *Reply All* (Ctrl+Shift+R), *Forward*, *Print* (Ctrl+P).
- [ ] **Session** → *Connect* (F5), *Disconnect*, separator, *Session Log*, *Test send*, *Show transport*.
- [ ] **Mailbox** → *Inbox*, *Sent*, *Outbox*.
- [ ] **View** → *Toggle Session Log* (Ctrl+Shift+L), *Show Raw Session Log*, *Toggle Status Bar*, *Show Radio Dock* (Ctrl+Shift+M).
- [ ] **Tools** → *Templates*, *Rig Control*, separator, *Settings* (nested), *Preferences*.
- [ ] **Tools → Settings** → *Connection*, *Privacy* (nested), *GPS*.
- [ ] **Tools → Settings → Privacy** → *GPS state*, *Position precision*.
- [ ] **Help** → *About Tuxlink*, *Documentation*, *Report Issue*.
- [ ] Keyboard accelerators (Ctrl+N, Ctrl+Q, F5, Ctrl+Shift+M, Ctrl+Shift+L) fire menu items.
- [ ] Dev console shows `menu` events with the correct ID payload on click.
- [ ] Quit via **File → Quit** exits cleanly (no leftover `tuxlink` process).

(Headless agent-side smoke recorded below — full operator smoke is the
merge gate.)

**Agent-side smoke outcome:** _(executor fills in: "cargo build clean" /
"pnpm tauri dev headless OK, Vite ready in N s, no Rust panic" /
"compile-only — no display server in agent shell.")_

## Closes

- bd `tuxlink-6vi` (Task 7: Native OS menu bar)

EOF
)"
```

The branch name `<branch-name>` is whatever the Wave-2 worktree-creation script produced (typically `bd-tuxlink-6vi/<slug>` since the impl issue is `tuxlink-6vi`). The PR title `[<moniker>]` is the Wave-2 executor's session moniker, NOT this plan-writer's moniker.

### Pitfall review for Phase 6

- **CLAUDE.md §"Commit and release discipline":** uses heredoc syntax (mandatory to avoid the destructive-git hook's substring-matching of commit-body text against banned patterns; `-F file` would bypass the discipline hook's Agent-trailer check, so heredoc is the required path).
- **CLAUDE.md §"Git workflow — destructive commands are BANNED":** no `--force`, no `--amend` on pushed commits, no `git reset --hard`. If a fixup is needed mid-stream, use a fresh commit; if the branch needs cleanup before push, use non-interactive `git rebase <base>` on the unpushed commits — but for a single-task PR like this, fixup probably isn't needed.
- **CLAUDE.md §"Worktree disposal ritual":** after PR merges, dispose the worktree per ADR 0009 — the disposal ritual is OUTSIDE Task 7's scope and is the post-merge follow-up.
- **CLAUDE.md §"Documentation propagation contract":** Task 7 ships no doc changes beyond this plan itself + the PR body. No CLAUDE.md / AGENTS.md / pitfalls edits needed. (If AMD-10's spec changes later, that's a separate AMD-style PR, not Task 7's concern.)

### Ordering deps

- Depends on: Phases 1–5 complete.
- Blocks: Wave-2 Task 8 (system tray) merging — Task 8's `lib.rs` edit will conflict with Phase 3's `lib.rs` edit unless Task 7 lands first.

### DO NOT

- DO NOT skip the `Agent: <moniker>` commit trailer. It's the project's grep-discoverability gate (`git log --grep="^Agent: <moniker>"`).
- DO NOT skip the `Co-Authored-By:` trailer.
- DO NOT use `git commit -F <file>` — the destructive-git hook checks the Agent trailer via the bash command text; `-F` bypasses that check.
- DO NOT force-push. The hook bans it (per CLAUDE.md). If the PR needs to be rebased onto a moved `feat/v0.0.1`, do a non-interactive `git rebase feat/v0.0.1` on the local branch and then `git push` (without `-f`) — if origin rejects because of conflicts, follow the project-sanctioned "open new PR, close old with link" pattern (the PR #40 → PR #43 example in the most recent handoff).
- DO NOT mark the bd issue closed in this PR's commit. `bd close tuxlink-6vi` is run AFTER the PR merges, by the same executor or by the next session's close-out step. Premature close pollutes the bd state.

---

## Cross-task conflict map

Task 7 touches three files. Map of conflict surfaces against other Wave-2 tasks:

| File | Task 7 change | Conflicts with | Resolution |
|---|---|---|---|
| `src-tauri/src/menu.rs` | new | none (only Task 7 owns this file) | none needed |
| `src-tauri/tests/menu_test.rs` | new | none | none needed |
| `src-tauri/src/lib.rs` | adds `pub mod menu;`, `on_menu_event`, `setup` | Task 8 (tray adds `on_tray_icon_event`, extends `setup`); future tasks adding more `invoke_handler` calls; any other Wave-2 backend task adding a `pub mod XXX;` line | Task 7 lands first; Task 8 extends rather than rewrites. Multiple `pub mod` insertions on adjacent lines may textually conflict; resolve by alphabetizing all `pub mod` declarations together. |

Recommended Wave-2 ordering: Task 7 → Task 8. The bd dependency graph already encodes this — `tuxlink-6vi` blocks `tuxlink-rit`, so `bd ready` will not surface Task 8 until Task 7 closes. If Task 8 starts before Task 7 merges (e.g., a dispatcher overrides the bd edge), Task 8 must either (a) defer its `lib.rs` edits until Task 7 lands, OR (b) explicitly merge in Task 7's `lib.rs` shape and rebase. Defer is cheaper.

The React side (Wave-2 Tasks 11, 12+) consumes the `menu` event stream by subscribing via `@tauri-apps/api/event::listen("menu", ...)`. Those tasks DO NOT need to wait for Task 7 to merge — they can stub the consumer against the event-ID list in Phase 1's test as a contract. Coupling: the event ID strings ARE the contract.

---

## Reviewing this batch when Phase 6 ships

After completing the six phases (and before PR-author marking the PR ready for review):

```
Review the batch from multiple perspectives. Minimum 3 review rounds.
If round 3 still finds issues, keep going until clean.
```

Rounds to cover, in addition to the standard ambiguity/gaps/drift dimensions:

- **Round A (Tauri-2.x API drift):** confirm `MenuItemBuilder`, `SubmenuBuilder`, `MenuBuilder`, `Emitter::emit`, and `Builder::on_menu_event` signatures match what the executor's local `cargo doc` shows for `tauri = "2"`. Tauri 2.x is the same major-version line but minor versions add small surface changes. If the locked version in `Cargo.lock` is `2.9.x` or earlier, the snippets above should compile as-is. If Tauri 3.x has shipped by the time Task 7 executes (unlikely but possible), the executor MUST flag the version drift in Discoveries, not silently adapt.
- **Round B (spec drift against `docs/design/v0.0.1-ux-mockups.md` §7):** re-read §7 against the table in Phase 2. If §7 has been amended (AMD-11+) since this plan was written, surface as a Discovery; do NOT silently incorporate amendments that arrived after this plan's commit SHA.
- **Round C (pitfalls coverage):** re-read `docs/pitfalls/implementation-pitfalls.md` SCOPE-1, HOOK-1, LEASE-1, PARITY-1, RADIO-1, RADIO-2 (the ones flagged in the plan-writing dispatch prompt). Verify the implementation steps don't violate any. Add notes if a new pitfall section has been added since (`docs/pitfalls/implementation-pitfalls.md`'s table-of-contents grows organically).

---

## Recommended execution strategy

**Subagent-driven** (`superpowers:subagent-driven-development`) — single fresh subagent executes all six phases, with the operator reviewing the PR before merge. Rationale:

- Task 7 is one small atomic vertical (one Rust file new, one Rust file lightly edited, one test new). No useful intra-task checkpoint.
- The work is fully self-contained per this plan; no need to dispatch parallel subagents.
- Parallel-agent dispatch (`dispatching-parallel-agents`) is overkill — there's no parallelism to extract.
- Inline execution in the current session (`executing-plans`) burns plan-writer context that's better saved for Wave-2 coordination across tasks.

If the dispatcher wants Wave-2 efficiency, dispatch Task 7's subagent in parallel with Tasks 8, 9, 10, 11, 11.5, 12 (since those don't conflict with Task 7's three-file footprint except Task 8's `lib.rs` edit — handle that via ordering as noted in the conflict map). All other tasks can run in parallel against Task 7.

---

## Self-review checklist (for the Wave-2 executor before opening PR)

- [ ] Phase 1's test file exists at `src-tauri/tests/menu_test.rs` and matches the snippet exactly (or with the alphabetization tightened).
- [ ] Phase 2's `menu.rs` exists at `src-tauri/src/menu.rs` and compiles standalone.
- [ ] Phase 3's `lib.rs` change is exactly the two narrow edits described (no scope creep into refactoring the surrounding code).
- [ ] `cargo test --test menu_test` is green.
- [ ] `cargo build` is clean.
- [ ] Manual smoke recorded in PR body (full operator smoke OR headless agent smoke + explicit "operator smoke required").
- [ ] Commit subject is conventional (`feat(ui): ...`) and the body is in the heredoc form.
- [ ] Commit body includes both `Agent: <moniker>` and `Co-Authored-By:` trailers.
- [ ] PR title matches `[<moniker>] feat(ui): Task 7 — Native OS menu bar (AMD-10 unified)`.
- [ ] PR body includes the operator-facing manual checklist.
- [ ] PR body includes "Closes bd `tuxlink-6vi`".
- [ ] `src-tauri/src/main.rs` is unchanged.
- [ ] `src-tauri/Cargo.toml` is unchanged.
- [ ] `src-tauri/tauri.conf.json` is unchanged.
- [ ] No new dependencies added (no `tauri-plugin-global-shortcut`, no clipboard / hotkey crates).
- [ ] No React-side code modified (Task 7 is Rust-only).

---

## Plan-review-cycle log

Per `/superpowers:plan-review-cycle`, this plan went through ≥3 adversarial review rounds (Rounds R1, R2, R3 below). Each round re-read the full plan and surfaced findings against the dimensions in the skill: ambiguity, context gaps, interpretation drift, cross-task conflicts, testing pitfalls, implementation pitfalls.

**Initial drafting also resolved (pre-review):**
- Original Task 7 spec is duplicated in the source plan (Steps 3–6 appear twice with subtle differences). Resolution: this plan IS the single source of truth; source-plan duplicate flagged in §"Why this plan structure."
- Source spec wires `on_menu_event` on `AppHandle` (Tauri 1.x pattern); Tauri 2.x has it on `Builder`. Resolution: Discovery D1 + Phase 3's explicit `Builder::on_menu_event` chain.
- Source spec modifies `main.rs`; Task 1's scaffold made `main.rs` a one-line shim and put `run()` in `lib.rs`. Resolution: Discovery D2 + Phase 3 targets `lib.rs`.
- Original "wire_menu_events" function name in source spec implies imperative wiring; renamed to `dispatch_menu_event` to clarify it's a per-event dispatcher (the listener registration is in `lib.rs`, not in `menu.rs`).
- `tauri::Emitter` not in scope by default for `app.emit(...)` in Tauri 2.x. Resolution: `use tauri::{... Emitter, ...}` added explicitly in Phase 2's snippet.

**Round R1 — Ambiguity + context gaps + interpretation drift (4 findings, all fixed):**
- F1.1: Execution Status table row for Phase 2 listed stale function name `wire_menu_events`. Fixed: renamed to `dispatch_menu_event` matching Phase 2's spec.
- F1.2: Phase 6's commit-action snippet had inconsistent branch-name placeholders (`<slug>` in one place, `<branch-name>` in another). Fixed: replaced with `BRANCH=$(git branch --show-current)` + `"$BRANCH"` for both push and `gh pr create --head`.
- F1.3: Discovery D1 wording said "Phase 2 resolves... by moving the listener to lib.rs" — actually Phase 3 resolves, Phase 2 enables. Fixed: D1 now correctly attributes the resolution.
- F1.4: Cross-task conflict map mentioned Wave-2 ordering Task 7 → Task 8 without noting bd's dependency graph already encodes this. Fixed: added explicit "bd `tuxlink-6vi` blocks `tuxlink-rit`; `bd ready` won't surface Task 8 until Task 7 closes" note.

**Round R2 — Cross-task conflicts + pitfall coverage (4 findings, all fixed):**
- F2.1: Phase 2's pitfall review was too cursory on implementation-pitfalls ORCH-1 and BD-1 (mentioned as "N/A" but no justification). Fixed: explicit N/A justifications added for both.
- F2.2: testing-pitfalls §3 (Error Path Coverage) WOULD flag the `let _ = app.emit(...)` discard but my Phase 2 dismissed §3 with "N/A." Real coverage gap. Fixed: Phase 2's pitfall review now explicitly addresses §3 — the intentional non-coverage is justified inline in the code comment + the plan body now spells out why mock-injection / full-App-construction is out of scope for Task 7.
- F2.3: testing-pitfalls §6 (Boundary & Configuration Validation) applies — accelerator strings ARE configuration; a typo silently fails. Phase 4's `cargo test` does NOT catch this. Fixed: Phase 2 pitfall review now explicitly identifies the gap + names Phase 5's manual smoke as the catch + requires the smoke checklist to track accelerator additions.
- F2.4: Plan never explicitly told the executor to claim `tuxlink-6vi` first. Fixed: added Phase 0 (Executor preflight) covering moniker, bd claim, worktree creation, starting-state verification, and full-read-before-action. Updated Execution Status table to include Phase 0.

**Round R3 — Tauri API drift + plan-review-log honesty (2 findings, both fixed):**
- F3.1: Phase 3 had a hedge sentence "Some Tauri versions document the first param as `&App<R>` instead." Real claim: `Builder::on_menu_event` in Tauri 2.x source is unambiguously `Fn(&AppHandle<R>, MenuEvent)`. Fixed: hedge replaced with cited concrete signature + "if Cargo.lock is materially different, verify before assuming compile-clean."
- F3.2: The plan-review-cycle log itself (this section) was originally written pre-review describing fictional rounds. That violates testing-pitfalls.md §1 (Test Output Pristine, generalized: "fake metadata is worse than missing metadata"). Fixed: rewrote this section to reflect the ACTUAL rounds executed, including this self-disclosing finding.

**Round R4 — Re-read after R1–R3 fixes (3 findings, all fixed):**
- F4.1: Phase 5's bash snippet did `pnpm tauri dev &` then `grep /tmp/tauri-dev.log` but the redirect to the log file was missing. The grep would always fail silently. Fixed: explicit `> "$LOG" 2>&1` redirect + unique `$$` suffix in the log filename to avoid cross-execution clobber + improved classification logic that distinguishes "alive but error in log" from "dead" from "alive clean."
- F4.2: Phase 5's `cd` path hardcoded the plan-WRITING worktree (`bd-tuxlink-q8i-task-7-plan`) instead of the Wave-2 executor's IMPLEMENTATION worktree (`bd-tuxlink-6vi-<slug>`). Fixed: replaced both hardcoded paths with `"$(git rev-parse --show-toplevel)"` which is portable and self-locating.
- F4.3: Phase 6's `cd` path had the same hardcoded-plan-worktree bug. Fixed: same `git rev-parse --show-toplevel` pattern.

**Round R5 — Final re-read after R4 fixes (0 findings).** Plan is subagent-ready.

If a future plan-reviewer (Wave-2 executor or any follow-up) finds additional issues, the right move is to add them to this log + fix in place, per the Living Document Contract above.

---

## End of plan
