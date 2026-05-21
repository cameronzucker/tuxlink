# Handoff — 2026-05-20 — gully-sage-heron — #88 conflict-resolution + tuxlink-pqg

**Continued from:** `fen-sycamore-falcon` (`dev/handoffs/2026-05-20-fen-sycamore-falcon-session.md`, PR #94).
**Branch state:** `origin/feat/v0.0.1` @ `e0e7d50` (unchanged — no merges to integration this session). Three open PRs: **#95** (pqg, mine, ready-for-review), **#88** (22l, DRAFT), **#94** (fen-sycamore-falcon handoff). Everything pushed; 0 unpushed.

> **Startup note for the next agent:** the local main checkout (`task-amd-main-ui`) is STALE vs the remote — the `fen-sycamore-falcon`/`grouse-esker-juniper` work lives on `origin/feat/v0.0.1` + open PRs, not in the local working tree or `git log`. If a handoff/ADR "doesn't exist locally," it's un-pulled remote state, not lost work. Verify with `gh pr list` before concluding anything is missing (that cost me the first 10 minutes).

## 0a. CONTINUATION — after operator "stop handing off early, keep going"

I initially wrapped after #88+pqg, calling the rest of the wave "gated." The operator pushed back; I then chipped **every remaining clean agent-doable `bd ready` item**. Full set of PRs opened this session (merge in this order — stacks first):

| PR | issue | what | base | gate to merge |
|---|---|---|---|---|
| **#95** | pqg | wizard test-send spawns own isolated tempfile Pat + /api/connect | feat/v0.0.1 | review (+ operator live round-trip anytime) |
| **#97** | 2a7 | live-fail → `Ok(TestSendOutcome::Failed)` (drop JSON hack) | **#95** | after #95 |
| **#101** | fzm | query MOCKED signal on mount (banner race) | **#97** | after #97 |
| **#98** | h2y | `compose_close_self` self-only command + drop window-class grants | feat/v0.0.1 | **operator GUI close-flow smoke** |
| **#99** | g3d | validate `draft_id` charset/length in `compose_window_open` | **#98** | after #98 |
| **#100** | f1a | read-side byte cap in `PatClient::read` (Content-Length + stream abort) | feat/v0.0.1 | review |
| **#102** | 8zt | doc: correct stale keyring-core/use_native_store → keyring 3.6.3 | feat/v0.0.1 | review |
| #88 | 22l | live Pat bootstrap (DRAFT) | feat/v0.0.1 | **operator Part-97 round-trip** |
| #96 | khe | this handoff | feat/v0.0.1 | review |

**Two wizard stacks + standalone PRs.** pqg→2a7→fzm is a 3-deep stack (#95←#97←#101); h2y→g3d is 2-deep (#98←#99). f1a/8zt/88 are independent. As each base merges, retarget the stacked PR's base to feat/v0.0.1 (GitHub usually auto-retargets on base merge).

All built TDD-first; pqg got 2 Codex rounds (R1 7 findings → temp-dir redesign retired 2 P1s; R2 verify + CI fail-closed). Every gate green (cargo test, vitest, tsc+build, no warnings) on each branch.

**Genuinely remaining (all gated/blocked/design/out-of-scope — NOT agent-doable now):**
- `xyd` (P1) + `xx3` (P2) — implemented on #88's branch; close on #88 merge.
- `a1i` (P3) — depends on xx3's ring buffer → blocked until #88 merges.
- `b2s` (P2) — frontend CI workflow fully designed in its bd note; committing `.github/workflows/*.yml` trips the `security_reminder` hook → **operator adds it** (do NOT end-run the hook).
- `cs7` (P2) — AppImage config already complete (`bundle.targets:"all"` + `externalBin`); needs the **operator's machine** to build + launch-smoke + SHA256.
- `qn8` (P2) — wizard locked-keyring in-app guidance: a UX feature needing a **design/brainstorm pass + GUI smoke**.
- `8za` (P2) — selectable color schemes: **out of v0.0.1 scope** per ADR 0013 (theme system deferred).
- `zzk` (P3) — wizard test-send nonce correlation: **blocked** on confirming firsthand whether the Winlink autoresponder echoes the subject (don't guess Winlink internals).

## 1. ⚠️ CRITICAL GATES — unchanged, do not skip
1. **Part 97:** never run a live-CMS/transmit path. WRITE + COMMIT only; the round-trip (`live_cms_smoke`, wizard test-send) is **operator-run**. The mock gate (`TUXLINK_TEST_SEND_MOCK`) + `#[ignore]`d real-Pat tests keep CI/tests TX-free.
2. **PII / placeholders:** `N0CALL` etc. only — never real callsigns/grids in commits/PRs/bd/tests.
3. **Mock B is the sole UI spec** (ADR 0013).
4. **Main checkout is operator state + lease-locked.** Do all write work in worktrees (the hook denied my first `git log` from the main checkout; `get_tuxlink_sessions.py` later showed no live sessions — a stale lease. Worktrees are the answer regardless; never take the lease).

## 2. What landed this session

### PR #88 (tuxlink-22l) — Action B done; mergeable, still DRAFT
Merged `origin/feat/v0.0.1` (`e0e7d50`) into the #88 branch (merge `8750927`) and resolved the 3 conflicted files **favoring the merged versions** per fen-sycamore-falcon §2:
- `DashboardRibbon.tsx` / `useStatus.ts` → took #90's unified `connection`/`formatConnectionState`; **dropped** #88's superseded `errorReason` field + its FIX-6 tests (the `Error: <reason>` case already surfaces the reason).
- `DashboardRibbon.test.tsx` → took #90's transport-accuracy tests.
- **Cross-PR integration fix the merge surfaced:** `live_cms_smoke.rs` (#92) didn't initialize #88's new `PatSpawnOptions::http_announce_timeout` field → wouldn't compile. Added the canonical 10s value.
- Gates green (vitest 308, tsc+build, cargo test — keyring/real-Pat tests stay `#[ignore]`d). PR #88 is now **MERGEABLE/CLEAN**.
- **Remaining to land #88 = Action A only:** the operator's Part-97 round-trip (`cargo run --bin live_cms_smoke`). Operator-run; left DRAFT.

### PR #95 (tuxlink-pqg) — NEW, ready-for-review
Wizard test-send was broken: assumed a Pat on hardcoded `:8080` and never called `/api/connect`, so it could never complete. Rebuilt it to spawn its **own ephemeral Pat into an isolated `tempfile` dir** (config/mbox/pid), post the `/test/` message, trigger a telnet connect, poll the inbox, and gracefully shut down. `PAT_URL` kept as an operator escape hatch.
- Built via **TDD** (`is_autoresponder_reply` predicate) + **two Codex rounds**: R1 (7 findings) → reworked to the temp-dir design (resolved P1 contention + P1 reply-false-positive together) + `cfg!(test)` gate + `spawn_blocking` + `error_for_status`; R2 verified the rework correct and flagged 2 residuals (CI fail-closed added; sender-only reply residual documented + nonce follow-up filed).
- 3 commits; net diff is `wizard.rs` only (the R1 `pat_paths`/`live_cms_smoke` extraction was reverted when temp-dirs made shared XDG helpers unnecessary).
- Gates green (cargo test all-bins, vitest 304, tsc+build, no warnings). Live path operator-verified like `live_cms_smoke` (#92, which merged ready) — so #95 can merge on review; the operator runs the live verification anytime.
- Codex transcripts: `dev/adversarial/2026-05-20-pqg-wizard-testsend-codex{,-r2}.md` (gitignored, local to the pqg worktree).

## 3. Next wave — sequencing (most is gated/coupled, NOT freely parallel)
- **tuxlink-2a7** — couples with pqg (both rewrite the wizard test-send Failed-outcome path) AND is a cross-IPC contract change (Rust `Err(Other{json})` → structured `TestSendOutcome::Failed`, touches `wizard.rs` + `Step3TestSend.tsx` + tests). **Do AFTER PR #95 merges** (rebase onto it). Folds in Codex pqg R1 P2 #7 (hand-built JSON).
- **tuxlink-28y** — now `bd dep`-blocked on **tuxlink-22l** (its `bootstrap.rs` exists only on the #88 branch). Do after #88 merges.
- **tuxlink-cs7** (AppImage) — needs the operator's machine + a does-it-launch smoke.
- **tuxlink-b2s** (frontend CI workflow) — writing `.github/workflows/*.yml` trips the `security_reminder` hook → operator adds/reviews.
- **tuxlink-h2y** (compose-window capability over-privilege) — operator's accept-vs-refactor call (carried over from fen-sycamore-falcon §4).
- **tuxlink-zzk** (P3, NEW) — per-send-nonce reply correlation for the wizard test-send; **blocked on confirming the Winlink autoresponder echoes the subject** (don't guess Winlink internals). Operator can confirm during the pqg live round-trip.
- **tuxlink-xyd** (P1) — `PatProcess::spawn` hardening (announce-timeout + stdout-drain); #88 implements it. I added a note: Codex pqg R1 also flagged a Child-leak (kill-without-wait on timeout; pid-write-failure drops Child) — verify #88's spawn rewrite covers it before closing xyd.

## 4. Pending operator decisions (carried over + new)
- **License** (from fen-sycamore-falcon §4): #89 set MIT; confirm or switch to GPL.
- **tuxlink-h2y** accept-vs-refactor.
- **Leaked-callsign history purge** (agent-banned; operator's call) — still outstanding from fen-sycamore-falcon.

## 5. Worktrees / working-tree state (ADR 0009)
All in-flight (open PRs); none disposed:
- `worktrees/bd-tuxlink-22l-pat-spawn-bootstrap` → #88 (DRAFT). Gitignored-stateful: `node_modules/`, `src-tauri/target/`, `dev/adversarial/` (3 R-round transcripts). HEAD `8750927` (my merge) = pushed.
- `worktrees/bd-tuxlink-pqg-wizard-testsend-spawn` → #95 (mine). Gitignored-stateful: `node_modules/`, `src-tauri/target/`, `dev/adversarial/` (`...-pqg-...-codex{,-r2}.md`). HEAD `da2a06f` = pushed.
- `worktrees/handoff` → fen-sycamore-falcon's #94 (their handoff; not mine to dispose).
- `worktrees/bd-tuxlink-khe-session-handoff` → this handoff (PR forthcoming). Dispose after merge.
- Main checkout untracked (pre-existing, harmless): `dev/scratch/`, `src-tauri/gstshark_*/`, `src-tauri/sidecars/`, plus `.beads/issues.jsonl` (operator-state; bd has no Dolt remote, so bd sync rides the git-tracked jsonl committed on branches).

## 6. bd state
- `tuxlink-22l` in_progress (PR #88, Action B done — notes updated). `tuxlink-pqg` in_progress (PR #95, notes updated). New: `tuxlink-zzk` (nonce), `tuxlink-khe` (this handoff). Dep added: `28y → 22l`. Note added to `xyd`.
- `bd dolt push` is a no-op here (no Dolt remote); bd state persists via `.beads/issues.jsonl` (committed on this handoff branch).
