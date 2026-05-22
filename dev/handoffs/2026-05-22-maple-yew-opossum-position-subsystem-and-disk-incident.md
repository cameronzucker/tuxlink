# Handoff — 2026-05-22 — maple-yew-opossum — position subsystem (Tasks 5–12) + disk incident

## TL;DR
Finished the **position subsystem** (`tuxlink-686`) — Tasks 5–12 of 12, each with per-task
two-stage review + fixes, then a **cross-provider Codex gate that caught 2 P1 privacy/
transparency bugs (both fixed)**. All 9 commits pushed; **PR #113** is open against `main`,
ready for operator review/merge after the owed smokes. Mid-session, diagnosed + partially
remediated an **egregious disk-consumption issue** (worktree `target/` duplication) — reclaimed
~86 GB and put a recurrence guard in place; some disk follow-ups are operator-owned before the
1.5-week absence.

## 🚨 CRITICAL for the 1.5-week absence (disk / env stability)
**Root cause:** every git worktree was cold-building its own full Rust `target/` (5–16 GB each;
~100 GB across ~20 worktrees) — no shared target dir. A second concurrent Claude session
building fresh worktrees (`7fr`, `10o`) was the live "~1 GB/few-min" growth.

**Done this session:**
- Reclaimed **~86 GB** by deleting the regenerable `target/` caches of 10 stale worktrees
  (`0ic, 22l, 7fr, h2y, pqg, lbg, 882, f1a, 2a7, g3d`). **Only `target/` caches** — NOT the
  worktrees, source, uncommitted work, or `.beads` state. Disk: 57% → **47% used (450 GB free)**.
  Kept `686` (this session's warm target) + the main checkout's target.
- **Recurrence guard:** the 2nd agent was redirected to a shared target dir
  `/home/administrator/.cache/tuxlink-cargo-target` (confirmed building there). Future worktree
  builds reuse shared dependency artifacts instead of duplicating ~100s of crates each.

**Operator owes BEFORE leaving (only you can — they're other sessions):**
1. **Stop or redirect the other live sessions** so they don't re-grow per-worktree targets while
   you're away: there are idle/orphaned shells in worktrees `0ic` (incl. a **stuck ~24 h orphaned
   wait-loop, PID 879890** — safe to `kill`), `22l`, and `arv`. Point any live agent at
   `CARGO_TARGET_DIR=/home/administrator/.cache/tuxlink-cargo-target` or stop it.
2. (Optional, durable) Adopt the shared target dir repo-wide and dispose the stale worktrees via
   the ADR 0009 ritual when convenient — that reclaims the rest (the worktree source trees, ~hundreds
   of MB each, still on disk; their `target/` caches are already gone).

## Position subsystem (tuxlink-686) — DONE + in PR #113
Branch `bd-tuxlink-686/position-subsystem` @ `48187b3`, pushed, in sync. PR **#113** → `main`
(continues merged #109 design/plan + #111 Tasks 1–4).

**Commits (this session, 9):** `abafcc8` config_set_grid+arbiter · `cf3bb02` CMS locator from
arbiter · `f197368` position_source DTO · `ff01e5c` GridEdit inline-edit+chip · `40f56ab` gpsd
TPV parse · `e117f25` gpsd watch+backoff · `be4a992` position_set_source+spawn · `9bb85e9`
gpsfake e2e · `48187b3` **Codex P1 fix** (gps_state privacy + live broadcast grid).

**Review:** per-task spec+quality reviews (fixed: `broadcast_grid()` single-lock atomicity on the
privacy boundary; persist-before-mutate ordering; GridEdit a11y + Tauri-error extraction; panic-safe
grid validation). **Codex cross-provider gate found 2 P1s, both fixed in `48187b3`:**
- **P1 privacy leak:** the on-air locator ignored `gps_state`. Now GPS goes on air **only** under
  `BroadcastAtPrecision`; `Off`/`LocalUiOnly` fall back to the config grid (GPS never broadcast), and
  the gpsd reader isn't spawned when `Off`. A single shared `position::effective_broadcast_locator()`
  is used by **both** `native_connect` (TX) and `position_status` (display) → they can't diverge.
- **P1 transparency:** `position_status` now surfaces the live `broadcast_grid` so the ribbon shows
  exactly what is transmitted (was showing the stale config grid).
- Raw Codex transcript: `dev/adversarial/2026-05-22-position-subsystem-tasks5-12-codex.md` (gitignored, local-only).

**Gates:** 212 Rust lib + 41 vitest green; `tsc --noEmit` clean; gpsfake e2e validated against real
gpsd (`JN58td` → `JN58`), skips cleanly without the opt-in env flag / gpsfake.

**OWED before merging #113 (operator):**
- Browser smoke of GridEdit inline-edit + source chip in the real WebKitGTK window (`pnpm tauri dev`)
  — unit tests can't catch CSS specificity / focus-blur.
- Live LC29C no-fix-path smoke (per the spec's operator validation): ribbon shows GPS·no-fix +
  manual fallback; a pinned Manual grid is NOT overridden by a fix.
- **Decide the `LocalUiOnly` refinement** (deferred): the ribbon currently shows the *broadcast*
  grid (config) under LocalUiOnly; a future option could show live GPS *locally* while broadcasting
  the config grid. Privacy-transparent default chosen for now.
- Optional: a Codex *re-confirm* of `48187b3` (verified-by-inspection + Claude-tested here; the fix
  is exactly what Codex prescribed).

## Branch / worktree / working-tree state
- `bd-tuxlink-686/position-subsystem` worktree: tracked tree **clean**, in sync with origin.
  Gitignored-on-disk (NOT pushed): `src-tauri/target/` (warm, ~5.5 GB — kept), `node_modules/`,
  `dev/adversarial/` (the Codex transcript), `dev/scratch/`, `.superpowers/`.
- bd `tuxlink-686`: left **in_progress** (NOT closed) pending the operator smokes; checkpoint note
  recorded. `bd dolt push` is a no-op here (no Dolt remote configured).
- The 10 disposed `target/` caches will cold-rebuild on next use of those worktrees (now cheap via
  the shared dir).

## Decisions made this session
- Disk: delete only `target/` caches (safe/regenerable), not worktrees; shared `CARGO_TARGET_DIR`
  as the structural fix; defer full worktree disposal to the operator.
- `gps_ready` is live arbiter state → its own `position_status` command, NOT `ConfigViewDto`.
- On-air locator + ribbon display share one `effective_broadcast_locator` (TX == what's shown).
- `position_set_source` persists-before-mutate (matches `config_set_grid`).
