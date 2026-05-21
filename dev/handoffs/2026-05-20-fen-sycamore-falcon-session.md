# Handoff — 2026-05-20 — fen-sycamore-falcon — v0.0.1 push (22l + ship-surface + PII remediation)

**Branch state:** `feat/v0.0.1` @ `e0e7d50` (PR #93 merge). **One open PR: #88** (tuxlink-22l, DRAFT). All other session PRs merged. All branches pushed; 0 unpushed.

## 0. TL;DR
Drove v0.0.1 forward. Merged: **#86** (Mock B UI), **#87** (P1 keyring-test isolation), **#90** (ribbon transport fix), **#89** (real README + install docs), **#91** (callsign PII scrub), **#92** (`live_cms_smoke` round-trip validator), **#93** (wizard `sending` watchdog). The operational core — **#88 (tuxlink-22l live Pat bootstrap)** — is built + gates-green but **DRAFT**, pending (A) the operator's Part-97 round-trip smoke and (B) conflict-resolution against the now-merged `feat/v0.0.1`.

## 1. ⚠️ CRITICAL GATES — do not skip
1. **Part 97:** never run a live-CMS / transmit path; **WRITE + COMMIT only**; the round-trip (`live_cms_smoke` / wizard test-send) is **operator-run**. Spawning Pat in `http` mode is safe (no TX); a CMS `connect`/send transmits.
2. **PII / placeholders:** NEVER propagate real callsigns/grids/values read from the operator's config or keyring into commits, PRs, bd, or test fixtures — use placeholders (`N0CALL`). A real callsign leaked this session (scrubbed from live files via #91; **it remains in git history** — a purge needs an operator-run history rewrite, which is hook-banned for agents). See memory `feedback_decisive_autonomous_execution`.
3. **Mock B is the SOLE approved UI spec** (ADR 0013). Do not reintroduce Mock D.
4. **Work decisively:** chip the specced/phased backlog end-to-end; do NOT stall with option-menus, "what next" check-ins, or incident-theater. Surface only Part-97 gates, hard-to-undo *shape* decisions, or genuine blockers. (Memory `feedback_decisive_autonomous_execution` — written after repeated operator pushback this session.)

## 2. #88 (tuxlink-22l) — the one open PR; TWO actions to land it
- **What:** app-start bootstrap spawns Pat → live mailbox / session-log / three-state status (not-connected / error / connected). Built via build-robust-features (spec + 3 Codex rounds + subagent TDD). Implements **tuxlink-xx3** (durable session-log ring buffer) + **tuxlink-xyd** (PatProcess announce-timeout/stdout hardening) — both close on merge. Worktree: `worktrees/bd-tuxlink-22l-pat-spawn-bootstrap`.
- **Action A — operator round-trip (Part 97):** the operational proof. Easiest path: the merged **`live_cms_smoke`** binary — `cargo run --bin live_cms_smoke` (requires a keyring entry first: complete the wizard, or `secret-tool store --label='tuxlink-pat WL2K' service tuxlink-pat account <CALLSIGN>`). It spawns Pat → posts a `/test/` msg → `POST /api/connect?url=telnet` → polls Inbox for the SERVICE reply. **Note:** the in-app compose path likely does NOT trigger the connect — `PatBackend::connect` is a v0.0.1 stub (real `/api/connect` integration deferred to v0.5); `live_cms_smoke` is the reliable round-trip for v0.0.1.
- **Action B — resolve #88's conflicts (8 markers) vs current `feat/v0.0.1`:** #88 predates the merged #90/#93/#91. Merge `feat/v0.0.1` into the #88 branch and resolve **favoring the MERGED versions**: the ribbon connection = #90's unified `connection`/`formatConnectionState` (so #88's separate `errorReason` ribbon edit + its tests are **superseded — drop them**); keep #91's `N0CALL` test-fixture scrub. Re-run `cargo test` + `pnpm vitest run`/`tsc`/`build`. Then #88 is cleanly mergeable once the round-trip passes.

## 3. Next wave — now unblocked by the merges (chip in this order)
- **tuxlink-pqg** — wizard test-send hardcodes `http://127.0.0.1:8080` + never triggers `/api/connect`, so it's broken post-refactor. #92's round-trip flow is merged → mirror/share `live_cms_smoke`'s spawn→post→`/api/connect`→poll inside `wizard_run_test_send` (spawn its own ephemeral Pat from the wizard's in-progress config; keyring cred is written by wizard Step 2). Part-97: write+commit, operator runs the live test; the `TUXLINK_TEST_SEND_MOCK` gate stays for tests/CI. Touches the wizard — sequence vs other wizard work.
- **tuxlink-2a7** (P3) — wizard live-path failure serialization → structured `TestSendOutcome::Failed` (vs `WizardError::Other{detail:json}`). Clean refactor; #93 merged.
- **tuxlink-28y** (mid-spawn Pat supervisor) + the **dropped-lines marker** (xx3 follow-up, low-pri) — both touch #88's `bootstrap.rs`; do AFTER #88 merges.
- **tuxlink-cs7** (Task 17 AppImage packaging) — needs the operator's machine + a does-it-launch smoke.
- **tuxlink-b2s** (frontend CI workflow) — the `frontend-ci.yml` is designed (in the bd note); writing a workflow file trips the `security_reminder` hook → operator adds/reviews it.
- **tuxlink-h2y** (P3 compose-window capability over-privilege) — disposition recorded: accept as bounded v0.0.1 risk (option b) OR refactor to a self-only-close command (option a) — operator's call.

## 4. Pending operator decisions
- **License:** #89 set **MIT** (matching the `LICENSE` file); the old README claimed GPL. Confirm MIT, or switch to GPL (+ update the `LICENSE` file).
- **tuxlink-h2y** accept-vs-refactor (above).
- **History rewrite** to purge the leaked callsign from merged history (agent-banned; your call). Broader sample-callsign scrub (`W4PHS` ×32 files, mockup senders `K0SWE`/`WX4MTL`/etc.) is separate.

## 5. Worktree / working-tree state (ADR 0009)
- `feat/v0.0.1` @ `e0e7d50`. Only open PR: **#88**. Everything pushed.
- **In-flight worktree:** `worktrees/bd-tuxlink-22l-pat-spawn-bootstrap` (#88). Gitignored-stateful content there: `node_modules/`, `src-tauri/target/`, and `dev/adversarial/2026-05-20-pat-spawn-bootstrap-{,impl-,impl-fixes-}codex.md` (the 3 Codex transcripts — local-only reference, summarized in the spec §10 + PR #88 body).
- The 4 merged-PR worktrees (5pk, 9w8, gkn, nk7) were disposed this session. `bd` state current + pushed. New memory: `feedback_decisive_autonomous_execution`.

## 6. Next-session paste-ready prompt
(see the fenced block surfaced as the session's final message)
