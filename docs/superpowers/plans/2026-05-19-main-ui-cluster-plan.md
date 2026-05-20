# Main-UI Cluster Implementation Plan (v0.0.1 Tasks 8 / 12 / 13 / 14 / 15 / 16)

> **For agentic workers:** REQUIRED SUB-SKILL — use `superpowers:test-driven-development` per task (failing test → red → implement → green → commit). Spec of record: [`docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md`](../specs/2026-05-19-main-ui-cluster-design.md). **This plan is the executable layer; the spec is the contract.** Where this plan says "per spec §X," read the spec — it is authoritative for data model, IPC signatures, layout, and behavior. Do not restate or re-decide spec contracts.
>
> **Orchestration of record:** [`dev/plans/2026-05-19-autonomous-ui-buildout.md`](../../../dev/plans/2026-05-19-autonomous-ui-buildout.md). Per-task dispatch via `superpowers:subagent-driven-development`; review = orchestrator + Codex (not reviewer-subagents); auto-merge low-risk render tasks after green gates + both reviews; M2 operator smoke is the runtime gate.

## LDC Execution Status

> Living-document banner. The implementing subagent for a task IS authorized to flip its own task's marker ⬜→✅ as a final commit (`docs(plan): flip main-UI LDC <task>`). Do not edit other tasks' markers.

- ✅ Task 8 — System tray + window-close-to-tray (`tuxlink-rit`)
- ✅ Task 12 — Folder sidebar + message list + model + backend bootstrap (`tuxlink-zsm`) [ROOT] — model/trait/IPC/sidebar/list/AppShell DONE; live Pat bootstrap STUBBED (follow-up, see PR)
- ⬜ Task 13 — Message reading pane (`tuxlink-y5c`)
- ⬜ Task 14 — Compose window (separate Tauri window) (`tuxlink-dm8`)
- ✅ Task 15 — Session log pane (`tuxlink-69z`)
- ✅ Task 16 — Dashboard ribbon + minimal status bar (`tuxlink-hvv`)

## Prerequisites — every subagent reads before starting

1. [`CLAUDE.md`](../../../CLAUDE.md) — ethos, git safety rails (destructive git HOOK-BANNED), moniker discipline, Part 97, commit discipline.
2. **Spec of record:** [`docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md`](../specs/2026-05-19-main-ui-cluster-design.md) — read the architecture thesis (§1), your task's §5.x, the IPC contract (§3), the data model (§2), and the ownership map (§7).
3. [`docs/design/v0.0.1-ux-mockups.md`](../../design/v0.0.1-ux-mockups.md) §5.5–§5.9 + the synthesis mocks (`mockups/images/synthesis-dock-off.png`, `synthesis-dock-on.png`) — the canonical UX.
4. [`docs/pitfalls/implementation-pitfalls.md`](../../pitfalls/implementation-pitfalls.md) §1 (SCOPE-1 — tuxlink is a Winlink **client**, never gateway functionality).
5. [`docs/pitfalls/testing-pitfalls.md`](../../pitfalls/testing-pitfalls.md) §9 (native-menu/rendering — static tests verify model + pure logic, NOT rendered widgets; the M2 operator smoke is the only runtime gate).
6. The shipped backend: `src-tauri/src/winlink_backend.rs` + `src-tauri/src/pat_client.rs` (the trait surface your task consumes).

## Mandatory Per-Task Preamble (implicit in every task below)

1. Read the prerequisites above.
2. Create your worktree from the repo root: `python3 .claude/scripts/new_tuxlink_worktree.py --slug <slug> --issue <bd-id> --moniker <moniker>` (off `origin/feat/v0.0.1`). Set your working directory into it (EnterWorktree or `git -C`). **Verify `git branch --show-current` shows `bd-<bd-id>/<slug>` before any commit** — never write in the main checkout.
3. Invoke `superpowers:test-driven-development`. Failing test FIRST, confirm red, implement minimal, confirm green. No implementation before a failing test.
4. Run all gates (below) green before opening the PR. Paste real output.
5. Conventional commits with trailers `Agent: <moniker>` + `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>`.

## Subagent Guardrails (read first; ignore at your peril)

- **SCOPE-1:** client only. No gateway/RMS functionality. No live transmission from any automated context.
- **Route through the trait (spec §1):** backend access is via the `WinlinkBackend` trait held in `AppBackend` managed state. Do NOT construct raw `PatClient` per command; do NOT invent a parallel API. The v0.0.1 plan's old per-task code snippets are **stale** (spec §1.2) — follow the spec, not those snippets.
- **NO full-view-swap (spec §4.2):** selecting a message updates ONLY the reader; selecting a folder updates ONLY the list. The whole view never swaps; no router-driven page navigation for selection. (Compose is the one permitted separate window.)
- **New deps:** only those authorized in spec §8 (`@tanstack/react-query`, `react-virtuoso`, `tauri-plugin-window-state`, `mail-parser`). Adding any other dependency → STOP and report NEEDS_CONTEXT.
- **You cannot smoke (headless).** Ship green automated gates (vitest + tsc + cargo test + cargo build) + the PR. NEVER claim a UI "renders correctly" — claim "gates green, smoke pending." Visual verification is the operator's M2 job.
- **Stay in your file set (spec §7).** Do NOT edit `AppShell.tsx`, `App.tsx`, or `lib.rs`'s `invoke_handler` — those land in the orchestrator's single integration commit (spec §4.3). You APPEND your `#[tauri::command]` fn to `ui_commands.rs` (append-only) and create your own component files. (Task 8 is self-contained: `tray.rs` + its own tray wiring in `lib.rs`'s `run()`.)

## Gates (all green before PR; per task)

- `pnpm vitest run` (frontend unit tests) — green
- `pnpm exec tsc --noEmit` — no TS errors
- `cd src-tauri && cargo test` — Rust tests green; `cargo build` succeeds
- (Tasks touching Rust commands) the relevant `cargo test --test <name>` green

## Dependency / wave overview (per orchestration playbook §Phase 2)

```
Wave 2A (parallel, independent build): Task 8, Task 15, Task 16
Wave 2B (parallel start; ROOT — gates 13/14): Task 12   (owns types.ts + AppShell + bootstrap)
Wave 2C (after Task 12 merges; rebase on it): Task 13, Task 14
INTEGRATION (orchestrator, after ALL component PRs merge): AppShell wiring + App.tsx menu listener + lib.rs invoke_handler → then M2
```
Build-hard-dependency = the message model (`src/mailbox/types.ts`, Task 12) imported by 13/14. Everything else (8/15/16 components, 13/14 components) builds standalone with unit tests; **all AppShell wiring + `invoke_handler` registration is deferred to one orchestrator-owned integration commit after the component PRs merge** (spec §4.3, see "Integration commit" below). 8 is fully standalone (Rust).

---

## Task 8 — System tray + window-close-to-tray (`tuxlink-rit`) — Wave 2A, standalone

**Files** (per spec §5.1): create `src-tauri/src/tray.rs`, `src-tauri/tests/tray_test.rs`, `src-tauri/icons/tray-icon.png`; modify `src-tauri/src/lib.rs` (`pub mod tray;` + `tray::install` in `.setup()` + `on_window_event` CloseRequested → `window.hide()` + `api.prevent_close()`, all inside `tuxlink_lib::run()` — NOT `main.rs`, which is just the shim), `src-tauri/tauri.conf.json` (trayIcon).

**TDD recipe:**
1. Failing test (`tray_test.rs`): `tray::tray_event_ids()` contains `tray:show_hide`/`tray:new_message`/`tray:quit`. Run `cargo test --test tray_test` → red (module not found).
2. Implement `tray.rs` per v0.0.1 plan Task 8 Step 3 — **Quit uses `MenuItemBuilder` + `on_menu_event` → `app.exit(0)`** (NOT `PredefinedMenuItem::quit`, which is Linux-unsupported — this is the PR #71 fix). "New Message" emits `menu` event `menu:file:new`. `on_tray_icon_event` Click → show + focus.
3. Wire `lib.rs`'s `run()` Builder (close→hide; only File:Quit/tray:Quit/Ctrl+Q exit — load-bearing: close mid-ARQ must not kill Pat). Provide a 32×32 monochrome "T" PNG.
4. Gates green. Note in PR body: runtime close-hides-to-tray verified at M2 smoke (test asserts event IDs only).

**Completion check:** `cargo test --test tray_test` passes; `cargo build` succeeds; tray icon path resolves in `tauri.conf.json`.

**Risk class:** low (render/Rust-wiring) → auto-merge eligible after orchestrator + Codex + green gates.

---

## Task 12 — Folder sidebar + message list + model + backend bootstrap (`tuxlink-zsm`) — Wave 2B, ROOT

The root. Owns the message model (TS + Rust trait additions), the IPC foundation (`ui_commands.rs` + `UiError`), the `AppShell`, and the `AppBackend` bootstrap. **Blocks 13/14.**

**Files** (per spec §5.2 + §7): create `src/mailbox/types.ts`, `src/mailbox/FolderSidebar.tsx`, `src/mailbox/MessageList.tsx`, `src/mailbox/useMailbox.ts`, `src/shell/AppShell.tsx`, `src-tauri/src/app_backend.rs`, `src-tauri/src/ui_commands.rs`, + tests; modify `winlink_backend.rs` + `pat_client.rs` (MessageMeta `to`+`has_attachments`; `read_message_in(folder,id)` per spec §2.1), `lib.rs` (managed state `.manage()` + `mailbox_list` registration + app-start bootstrap inside `run()`: spawn Pat → construct `PatBackend` → store in `AppBackend` (RwLock) → drain `stream_log` → emit `session_log:line` per spec §3.3; `main.rs` stays the shim), `App.tsx` (wizard→shell routing).

**TDD recipe:**
1. **Rust trait additions first:** failing test in `ui_commands_test.rs` / `winlink_backend` tests — `mailbox_list` maps a Pat DTO (mockito fixture) → DTO with `to` + `hasAttachments`; `read_message_in(Inbox,id)` == prior `read_message(id)`. Red → implement the `MessageMeta` fields + `read_message_in` + `mailbox_list` command + `UiError` projection (spec §2.1, §3.1, §3.2) → green. **If Pat 1.0.0's list JSON lacks To/attachment metadata, degrade `to`/`hasAttachments` and report DONE_WITH_CONCERNS** (spec §9 item 7).
2. **Frontend model + list:** failing `MessageList.test.tsx` (renders subject/from/to/size; empty-state copy; unread class; `onSelect` fires without remounting shell) → implement `types.ts`, `MessageList.tsx` (`react-virtuoso`), `useMailbox.ts` (TanStack, 10s refetch), `FolderSidebar.tsx` (Inbox/Outbox/Sent/Drafts functional; Deleted/Templates disabled) → green.
3. **AppShell + routing:** `AppShell.tsx` CSS-grid with regions per spec §4.1; owns `selectedFolder` + `selectedMessage:{folder,id}` (spec §4.2 — selection carries the folder); inline placeholder divs for ribbon/sessionlog/statusbar/reader. `App.tsx` routes wizard→shell on `get_wizard_completed`.
4. **Bootstrap:** app-start Pat spawn + `AppBackend` managed state + `session_log:line` event drain. `None` → `NotConfigured` (empty state, not error).
5. Gates green (vitest + tsc + cargo).

**Completion check:** all gates green; `mailbox_list` returns mapped metas against a mockito Pat; AppShell renders post-wizard with functional Inbox/Outbox/Sent; selection updates only the relevant pane.

**Risk class:** medium (touches the trait + bootstrap + shared shell) — orchestrator + Codex review; auto-merge eligible (not keyring/Part-97) but scrutinize the trait diff + invoke_handler registration.

---

## Task 13 — Message reading pane (`tuxlink-y5c`) — Wave 2C (after 12 merges; rebase on it)

**Files** (spec §5.3): create `src/mailbox/MessageView.tsx`, `src/mailbox/useMessage.ts`, + tests; modify `ui_commands.rs` (APPEND the `message_read` command fn; do NOT register it in `invoke_handler` or edit `AppShell.tsx` — the integration commit does both, spec §4.3).

**TDD recipe:**
1. Failing Rust test (`ui_commands_test.rs`): `message_read(folder,id)` parses an RFC5322 fixture → `ParsedMessageDto` (subject/from/to/cc/date/body/attachments/isForm). Red → add `mail-parser` dep (spec §8) → implement parse at the command boundary over `read_message_in` bytes (spec §5.3) → green. Cover: multipart (attachment names + text/plain), form payload (`<?xml` → isForm), non-UTF-8 (lossy, no panic), missing MID → `NotFound`.
2. Failing `MessageView.test.tsx`: header strip (sender + grid + UTC sent/received + routing; omit routing when null); body `pre` wrap; form → v0.1 placeholder; attachment strip lists names+sizes; no-selection empty state. Implement → green.
3. `useMessage(folder, id)` query, key `[folder, id]`, `enabled: !!selectedMessage` — folder from `selectedMessage.folder` (spec §4.2), never assume Inbox. (Wiring `MessageView` into AppShell's `reader` region is the integration commit's job, not this PR.)
4. Gates green.

**Completion check:** parse tests green; reader renders selected message; form placeholder shows; no attachment download (v0.1).

**Risk class:** medium (untrusted-byte parsing) — Codex scrutinizes the parser choice + form heuristic. Auto-merge eligible.

---

## Task 14 — Compose window (separate Tauri window) (`tuxlink-dm8`) — Wave 2C (after 12 merges; rebase on it)

Separate Tauri window per AMD-6 + spec §5.4. **The v0.0.1 plan's Radix Dialog snippet is void** (spec §1.2).

**Files** (spec §5.4): create `src/compose/Compose.tsx`, `src/compose/useDraft.ts`, `src/compose/draft.test.ts`, `src-tauri/src/compose_window.rs`; modify `ui_commands.rs` (APPEND `message_send` command fn), `lib.rs` (`compose_window_open` command + `tauri-plugin-window-state`, inside `run()` — NOT `main.rs`). Do NOT edit `App.tsx` — the main-window-only `menu:file:new` listener lands in the integration commit (spec §5.4).

**TDD recipe:**
1. Failing `draft.test.ts`: localStorage round-trip / clear / unknown→null; To/Cc split on `;` trim empties. Implement `useDraft.ts` (the Drafts-folder source for Task 12) → green.
2. `Compose.tsx` mounted at `/compose/:draftId` in a `compose-<draftId>` window (`WebviewWindowBuilder` in `compose_window.rs`; per-window geometry via plugin). Fields per spec §5.4 (disabled From/Send-as/Select-Template w/ v0.1 tooltips; To/Cc/Subject/Body; Attachments list; Request-ack; Post-to-Outbox; Save-Draft). Autosave 2s; restore on reopen; clear on send success. Close-with-changes → Save/Discard/Cancel. Ctrl+S / Ctrl+Enter.
3. `message_send` maps `OutboundDraftDto` → `OutboundMessage` (date = now RFC3339) → `send_message`. **`Ok(None)` → "Posted to Outbox" success** (Pat returns no MID — spec §3.2). **`cc` (Codex finding 5): Pat currently drops it — either wire `cc` through `PatClient::send` after verifying Pat 1.0.0 accepts the field, or disable the Cc input with a v0.1 tooltip; NEVER silently drop.** Failing Rust test asserts the None-success mapping.
4. Gates green. PR body notes: draft-survives-window-hide verified at M2 (test covers persistence functions).

**Completion check:** draft tests green; send maps correctly incl. None-success; window opens independently.

**Risk class:** medium (multi-window + draft-loss anti-pattern) — auto-merge eligible.

---

## Task 15 — Session log pane (`tuxlink-69z`) — Wave 2A (independent build; wiring rebases on 12)

Human/Raw projections of ONE structured stream (AMD-7 + spec §5.5). Backend event emission is part of Task 12's bootstrap (spec §3.3); Task 15's frontend is unit-tested against synthetic `LogLineDto[]` (mock IPC) so it builds independently.

**Files** (spec §5.5): create `src/session/SessionLog.tsx`, `src/session/logProjection.ts`, + tests. No shared-file edits — the `sessionlog` region wiring lands in the integration commit (spec §4.3); Task 15 builds + unit-tests standalone (mock the `session_log:line` events).

**TDD recipe:**
1. Failing `logProjection.test.ts` (the prime target — pure function): Human projection keeps `***`/Backend/Transport lines + a per-session summary, drops Wire/`;PQ`/`;PR`/`[WL2K-...]`/`FF`/`FQ`; Raw keeps all; both read the same input array (no dual stream); empty→empty. Implement `logProjection.ts` → green.
2. `SessionLog.tsx`: resizable bottom strip (default 120px, persisted); header (session-state + `[Human|Raw]` + Copy); listens `session_log:line` + seeds from `session_log_snapshot`; live-tail auto-scroll with scroll-up pause. Unit-test the scroll/projection-toggle logic; mock IPC.
3. Wire into AppShell `sessionlog` region.
4. Gates green.

**Completion check:** projection tests green (Human suppresses raw B2F; Raw shows all); pane renders + toggles.

**Risk class:** low (pure-logic + render) → auto-merge eligible.

---

## Task 16 — Dashboard ribbon + minimal status bar (`tuxlink-hvv`) — Wave 2A (independent; wiring rebases on 12)

TWO surfaces (AMD-8 + spec §5.6). Independent of `AppBackend` — reads config via its own `config_read` command (spec §3.2).

**Files** (spec §5.6): create `src/shell/DashboardRibbon.tsx`, `src/shell/StatusBar.tsx`, `src/shell/useStatus.ts`, + tests; modify `ui_commands.rs` (APPEND `config_read` + `backend_status` command fns — `config_read` reads `config.rs`, no AppBackend). Do NOT edit `AppShell.tsx`/`lib.rs` invoke_handler — the integration commit wires the regions + registers the commands (spec §4.3).

**TDD recipe:**
1. Failing `status.test.ts` (pure formatters): `formatStatus` idle; connection-state names the configured transport (CMS-SSL/Telnet); ribbon callsign from `identity.callsign` → falls back to `identity.identifier` offline; grid 4-char + 6-char tooltip only when `SixCharGrid`; GPS status maps each `gps_state`. Implement `useStatus.ts` formatters → green.
2. `config_read` Rust command reads the `Config` struct (spec §3.2); failing Rust test parses the `ConfigViewDto`.
3. `DashboardRibbon.tsx` (top, always visible) + `StatusBar.tsx` (bottom, toggleable). Connection-state consumes live `backend_status` (→ `status()`) when the backend exists, falls back to config-derived "Idle · <transport>" otherwise (spec §5.6). Formatters are pure-fn unit tests against synthetic `BackendStatus`.
4. Wire into AppShell `ribbon` + `statusbar` regions. Gates green.

**Completion check:** formatter tests green; ribbon shows real callsign/grid/GPS/time + transport-named connection state; status bar toggles.

**Risk class:** low (render + config-read) → auto-merge eligible.

---

## Integration commit (orchestrator-owned — after the component PRs merge, before M2)

Per spec §4.3 + Codex verdict 5, the shared-file wiring is concentrated into ONE commit the orchestrator authors on a short-lived integration branch off `feat/v0.0.1`, after Tasks 8/12/13/14/15/16 have merged:

1. `AppShell.tsx` — swap Task 12's inline placeholders for real imports: `DashboardRibbon` (ribbon), `MessageView` (reader), `SessionLog` (sessionlog), `StatusBar` (statusbar).
2. `App.tsx` — add the `menu:file:new` listener **gated to the MAIN window only** → `compose_window_open` (spec §5.4; compose windows must NOT listen, or they spawn nested compose windows).
3. `lib.rs` `invoke_handler![...]` — register `message_read`, `message_send`, `backend_status`, `config_read` (the command fns each task appended to `ui_commands.rs`).
4. Gates green (vitest + tsc + cargo) on the integrated tree; a final Codex round on the integration branch; then surface M2.

This is the only place those four shared files are edited after Task 12 — eliminating cross-PR conflicts rather than resolving them at each rebase. Small, mechanical, orchestrator-owned (not a subagent dispatch).

## Review model (per task — orchestrator + Codex, not reviewer-subagents)

Per the orchestration playbook §"Review model":
1. Subagent does TDD, runs all gates, opens PR with a test-plan checklist. Does NOT merge.
2. **Orchestrator review:** diff vs spec — behavior implemented? test list covered? no scope creep? no SCOPE-1 violation? IPC matches the trait? stayed in file set?
3. **Codex review** (`[[codex-post-subagent-review]]`): `npx --yes @openai/codex exec "Review commit <SHA> on branch <branch> vs docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §<task>. Attack spec-contract drift, missing error states, state-ownership bugs, React anti-patterns (full-view-swap, effect races), Rust async/Tauri-command correctness. VERIFIED vs INFERRED." 2>&1 | tee dev/adversarial/2026-05-19-<task>-codex.md`
4. **Decision:** low-risk render task + both reviews pass + gates green → auto-merge (`gh pr merge --merge --delete-branch`), dispose worktree (ADR 0009), `bd close`. Substantive issue → re-dispatch with findings (don't fix silently — `receiving-code-review` discipline). 13/14 wait for 12's merge then rebase.

## References

- Spec of record: `docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md`
- Orchestration: `dev/plans/2026-05-19-autonomous-ui-buildout.md`
- Canonical UX: `docs/design/v0.0.1-ux-mockups.md` §5.5–§5.9
- v0.0.1 plan (task structure + acceptance criteria; code snippets stale per spec §1.2): `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` Tasks 8/12–16
- Backend trait: `src-tauri/src/winlink_backend.rs`, `src-tauri/src/pat_client.rs`

## Plan-Review Disposition (post-cycle)

_Codex round was run on the SPEC (spec §end disposition table). This plan is the executable layer over the reviewed spec; no separate Codex round on the plan doc itself per the orchestration playbook Phase 0 step 5 (docs PR, self-merge after orchestrator review). Per-task impl PRs each get their own post-commit Codex round (Review model above)._
