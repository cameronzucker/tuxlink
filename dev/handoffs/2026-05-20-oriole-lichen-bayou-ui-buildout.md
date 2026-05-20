# Handoff — 2026-05-20 — Autonomous UI Build-Out (orchestrator session, mid-flight)

**From agent:** `oriole-lichen-bayou` (continued `willow-cypress-heron`'s `dev/plans/2026-05-19-autonomous-ui-buildout.md`).
**Why handing off:** context wall mid-execution. Main-UI components are merged; the wizard stack is code-complete and held for M1; Task 13 + two Codex rounds are unresolved. No operator milestone (M1/M2) has been surfaced yet — both are close.
**State:** `feat/v0.0.1` @ `9f0ceb2`. Main checkout on `task-amd-main-ui`. 9 worktrees live (enumerated below). Nothing lost; everything in-flight is committed + pushed to its branch.

---

## What landed this session (MERGED to `feat/v0.0.1`)

| Item | PR | Notes |
|---|---|---|
| **Phase 0** — main-UI spec + plan | #74 | `docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md` + `…/plans/…-plan.md`. ONE Codex round applied (disposition table in spec §11). The spec re-grounds all 6 main-UI tasks on the SHIPPED `WinlinkBackend` trait (the v0.0.1 plan's per-task code snippets are stale). |
| Task 16 — ribbon + status bar | #76 | clean (Codex P2/P3 deferred to integration) |
| Task 15 — session log | #77 | needed a Codex-fix cycle (P1 listener leak + snapshot race) |
| Task 12 — ROOT (model + IPC + AppShell + bootstrap) | #79 | opus; parent Codex VERIFIED all contracts. **Live Pat bootstrap STUBBED** → `mailbox_list` returns `NotConfigured`/empty (follow-up = `tuxlink-22l`). |
| Task 8 — system tray | #78 | needed a Codex-fix cycle (double-tray, window-label gate, menu guard) |
| Task 14 — compose window | #80 | needed a Codex-fix cycle (3 P1: URL-mount [deferred to integration], autosave-resurrect, native-close prompt) |

**Process truth worth carrying:** Codex post-subagent rounds caught real P0/P1s in 4 of 6 tasks **despite green gates** (Task 10 P0 mock-keyring; Task 15 P1 listener leak; Task 8 HIGH double-tray; Task 14 3×P1). The review layer is load-bearing — do NOT skip Codex on the remaining high-risk items (Task 11 Part-97, the integration commit).

---

## Wizard stack — CODE-COMPLETE, HELD for M1 (NOT merged)

Linear stack off Task 10's branch: **Task 10 (`#75`, `bd-tuxlink-1r5/wizard-credentials` @ `5f269d9`) ◄ Task 11.5 (`#82`, `bd-tuxlink-d76/offline-identity` @ `ce59f57`) ◄ Task 11 (`#83`, `bd-tuxlink-e4x/test-send` @ `0549bbc`)**.

- **Task 10** keyring fix is Codex-VERIFIED (no new P0/P1): `keyring` now uses `sync-secret-service` (real D-Bus Secret Service, not mock); the integration test is a HARD cross-process `secret-tool` read-back; rollback distinguishes `NoEntry`. Cross-process attribute key is **`username`** (not `account`) — verified against both the Rust keyring crate AND Pat's go-keyring source. Held for M1 (keyring-class).
- **Task 11.5** offline path: config-only persist (`connect_to_cms=false`, no keyring), orchestrator-reviewed clean.
- **Task 11** test-send: MOCK gate VERIFIED zero-transmission; the `tokio::sync::Mutex` is correctly held across `.await` (blocks *concurrent* transmission). **⚠⚠ But its Codex Part-97 round (`bsotyvs3g`) RETURNED with 2 P0 + 1 P1 — Task 11 is NOT shippable; M1 is BLOCKED until fixed.** The implementer's self-flagged Retry path was NOT safe:
  - **P0a — double-transmission under one consent gesture:** failed-`[Retry]` bypasses the reducer (`BEGIN_TEST_SEND` allowed only from `idle`, not `failed`), directly `invoke('wizard_run_test_send')`s while state stays `failed`, with the Retry button LIVE in the failed DOM and no in-flight flag. A fast live completion releases the mutex before React leaves `failed` → a second Retry/Enter transmits AGAIN.
  - **P0b — corrupted consent state:** `Busy` is caught as a generic failure → reducer sets `failed` from any substate → a double-click can show `failed` (Retry live) WHILE the first live send is still in flight.
  - **P1 — no source-enforced consent gate** before `client.send` (spec §3.8 wants a "type go" gate). The `docs/live-cms-testing-policy.md` click-exception may supersede §3.8 — **PENDING OPERATOR/LICENSEE DECISION** (the Part-97 live-TX consent mechanism is Cameron's call as operator-of-record).
  - **FIX (next session — re-dispatch on `bd-tuxlink-e4x/test-send`, then a Codex Part-97 re-round):** route Retry through a reducer-supported `failed→sending` transition; remove/disable Retry during the in-flight call (add an in-flight flag); treat `Busy` as "still sending" (ignore/preserve), do NOT convert to `failed`. P2s can ride along (no `test_send:log` events → mock banner unreachable; live failures serialized as `WizardError::Other` instead of `TestSendOutcome::Failed`).

**M1 mechanics:** operator smokes the full first-run wizard by running `pnpm tauri dev` from the **`bd-tuxlink-e4x-test-send` worktree** (top of the stack = has 10+11.5+11) with `TUXLINK_TEST_SEND_MOCK=1`. On approval, merge the stack to `feat/v0.0.1` up the chain (11→11.5→10, or merge each PR base-up). Then close `tuxlink-1r5` / `tuxlink-d76` / `tuxlink-e4x`.

---

## IN-FLIGHT at handoff (RESOLVE FIRST in the next session)

1. **Two Codex rounds were still running — verdicts unknown.** Check their output files (gitignored, in main checkout `dev/adversarial/`); `tail` each per `[[codex-quota-gotcha]]` (thin + "usage limit" = quota-defer, NOT a skip):
   - `dev/adversarial/2026-05-19-task13-postsubagent-codex.md` (`b3b6n8xaq`) — tail showed "Reading additional input from stdin…", **may be HUNG** — likely re-run it.
   - `dev/adversarial/2026-05-19-task11-postsubagent-codex.md` (`bsotyvs3g`) — was mid-review (Part-97 dedup analysis). If incomplete, re-run.
   - If a re-run is needed and Codex is quota-exhausted: defer + note (do NOT substitute Claude — the cross-provider value is the point).

2. **Task 13 (`#81`, `bd-tuxlink-y5c/reading-pane` @ `b504ccf`) — NOT merged.** Reading pane + RFC5322 parse (`mail-parser 0.9`, 2 MiB cap, parse-error state, folder-carried `useMessage([folder,id])`). Orchestrator-reviewed against spec (clean) + report says all guardrails honored; **awaiting its Codex round**. When clean → **merge-resolve** (it will conflict vs `feat`: `ui_commands.rs` `message_read` append vs #80's `message_send` append; `Cargo.toml` `mail-parser` vs #80's `chrono`/`tauri-plugin-window-state` — both mechanical, keep-both). This completes the main-UI component set.

---

## PENDING (the path to M2, after #81 merges)

**INTEGRATION COMMIT** (orchestrator-owned, single commit per spec §4.3 — this is the deliberate concentration of all shared-file wiring; do NOT let subagents do it piecemeal). After #81 is on `feat`, in a fresh worktree off `feat`:
- `src/shell/AppShell.tsx`: replace Task 12's inline placeholder divs with real imports — `DashboardRibbon` (ribbon), `MessageView` (reader), `SessionLog` (sessionlog), `StatusBar` (statusbar).
- `src/App.tsx`: add the **main-window-only** `menu:file:new` listener → `compose_window_open` (Codex Task-8/14 F7 — compose windows must NOT listen) **AND the `/compose/:draftId` route** that renders `Compose` in the compose window (Codex Task-14 P1 — currently the window would render the shell, not Compose).
- `lib.rs` `invoke_handler`: register `message_read`, `message_send`, `backend_status`, `config_read` (the command fns are appended in `ui_commands.rs`; only `mailbox_list` is registered today). Register the `tauri-plugin-window-state` plugin in `run()`.
- **Implement `config_read` (nested `config.rs` → flat `ConfigViewDto`)** + `backend_status` (Codex Task-16 INFERRED + Task-12 finding) — Task 16's frontend expects a flat DTO; the Rust commands were deferred to here.
- **Wire `FolderSidebar` counts** (Codex Task-12 finding 2 — `AppShell` renders `<FolderSidebar>` without the `counts` prop) and the Drafts source (`useDraft.listDraftIds` replaces Task 12's `draftIds.ts` stub).
- Gates (vitest + tsc + cargo) + a Codex round on the integration → merge → **surface M2** (operator smokes inbox/reading/compose/log/status/tray; expect EMPTY mailbox — live Pat bootstrap is `tuxlink-22l`, operator-gated).

---

## bd state

- **Closed this session:** `tuxlink-wbo` (Phase 0), `rit` (8), `hvv` (16), `69z` (15), `zsm` (12), `dm8` (14).
- **in_progress (held for M1 / pending merge):** `tuxlink-1r5` (10), `tuxlink-e4x` (11), `tuxlink-y5c` (13). (`tuxlink-d76`/11.5 — PR #82 open, work done, held for M1; verify its bd status.)
- **Filed follow-ups:** `tuxlink-22l` (P2 — `PatBackend::spawn` + live bootstrap; operator-gated keyring/Part-97-adjacent), `tuxlink-8zt` (P3 — cred-handling spec says stale `use_native_store(true)`; the real Secret Service attr is `username`).
- **Excluded from this run:** Task 6 (`nk7`, live-CMS), 16.5 (Radio Dock — no bd issue), 17/18/19 (`cs7`/`gkn`/`n65`).

## Worktrees live (ADR 0009 — enumerate before disposal)

- **KEEP (wizard stack, held for M1):** `bd-tuxlink-1r5-wizard-credentials`, `bd-tuxlink-d76-offline-identity`, `bd-tuxlink-e4x-test-send`.
- **Disposable (PRs merged):** `bd-tuxlink-hvv-…` (16), `bd-tuxlink-69z-…` (15), `bd-tuxlink-zsm-…` (12), `bd-tuxlink-rit-tray` (8), `bd-tuxlink-dm8-compose-window` (14). Their bd issues are closed (clear provenance — not orphans). Dispose per ADR 0009 (each has regenerable `node_modules`/`target` + possibly `.beads/embeddeddolt`; canonical bd state is in `issues.jsonl`).
- **Keep until #81 merges:** `bd-tuxlink-y5c-reading-pane` (13).
- Codex transcripts live in main-checkout `dev/adversarial/` (gitignored, local-only).

## Notes / gotchas

- **No repo auto-merge** (`gh pr merge --auto` errors). Protocol: push → wait `build-linux` CI → plain `gh pr merge <#> --merge`. The `UNKNOWN`-mergeStateStatus right after a push is just GitHub recompute; re-check `mergedAt` to confirm.
- **Per-task LDC-banner flips caused merge conflicts** (resolved each). The integration commit + any future parallel PRs should NOT per-task-flip the LDC — concentrate LDC edits in the integration commit.
- **Conflict-resolution discipline:** when resolving via Edits (not a merge tool), `grep -nE '^(<<<<<<<|=======|>>>>>>>)'` the files before `git add` — caught a dangling marker once.
- Main checkout working tree: `.beads/issues.jsonl` (bd auto-export — committed with this handoff) + untracked `dev/scratch/`, `src-tauri/gstshark_*/`, `src-tauri/sidecars/` (build byproducts; harmless).
