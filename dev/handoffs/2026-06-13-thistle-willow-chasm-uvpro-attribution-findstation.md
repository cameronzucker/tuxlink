# Handoff — thistle-willow-chasm (2026-06-13)

Build at handoff: **0.59.0**. All this session's work is **merged to main**.

## 🚨 NEXT SESSION PRIORITY: APRS tactical chat bug in 0.59.0 (tuxlink-iehg)

The operator discovered a **huge bug in the just-shipped APRS tactical chat**
(Phase 1a, PR #642 / tuxlink-2f2n, merged 2026-06-13 06:55, build 0.59.0) while
smoke-testing. **Symptoms were NOT captured this session.**

**First action for the next session (do not skip):**
1. **Ask the operator for the exact repro / symptom** before touching code.
2. Route through the **investigate** skill (root-cause-first; Iron Law: no fix
   without root cause). This is a regression in a just-shipped feature → the
   cause is almost certainly in the recently-merged APRS diff.

**Feature code:**
- Backend: `src-tauri/src/winlink/aprs/` — `engine.rs` (promiscuous RX listener),
  `message.rs` (APRS format), `dedupe.rs`, `framebuild.rs`, `tx.rs`
  (bounded-retransmit queue), `native_driver.rs`, `identity.rs`, `mod.rs`.
- Frontend: `src/aprs/` — `AprsChatPanel.tsx`, `AprsDockTabs.tsx`.
- Spec: `docs/design/2026-06-12-aprs-tactical-chat-design.md`.
- Build handoff (how it was built): `dev/handoffs/2026-06-12-sequoia-heron-jay-aprs-phase1a-built.md`.

**Coordinate:** there is in-flight native-GAIA-messaging work
(`docs/superpowers/plans/2026-06-13-aprs-native-gaia-messaging.md`) that may be a
separate concurrent session — check `get_tuxlink_sessions.py` before claiming.
RADIO-1: APRS transmits; the operator runs any on-air repro, the agent works
mocks/loopback.

## What this session shipped (all merged)

| PR | What | bd |
|----|------|----|
| **#647** | Native UV-Pro Benshi control backend (RFCOMM+GAIA codec, session driver, owner-lock, 7 `uvpro_*` commands + `uvpro:status`) | tuxlink-nx95 |
| **#651** | UV-Pro prior-art attribution + license-compliance review (benlink + HTCommander, both Apache-2.0; independent reimpl) | tuxlink-yh1d |
| **#658** | **fix(catalog): Find a Station reads the effective GPS grid** — the regression the operator reported | tuxlink-q1tm |

### Find a Station fix (#658) — the headline bug, fixed
- **Root cause:** `StationFinderPanel.tsx` resolved the operator grid ONLY from
  `config_read().grid` (= `config.identity.grid`, the *manual* grid). GPS
  operators (the default, `position_source=Gps`, `identity.grid=null`) had no
  grid there → no aiming/bearing header, HF prediction never fired, "set your
  location" shown despite a live GPS fix in the status bar.
- **Fix:** resolve the effective grid via `position_current_fix` (the GPS-aware
  `PositionArbiter`), falling back to `config_read().grid` — mirroring
  `CheckInForm`/`PositionFormV2`. Regression test added.
- **Live in the operator's k61j build:** the fix was HMR-applied to
  `worktrees/bd-tuxlink-k61j-tile-maxzoom-reactive/src/catalog/StationFinderPanel.tsx`
  as an **uncommitted working-tree change** for smoke. It's transient — once
  k61j rebases onto the new main it carries the real merged version; discard the
  transient copy then. (k61j is a SEPARATE session's tile-maxzoom WIP; do not
  commit on its branch.)

## Open follow-ups (filed, not started)
- **tuxlink-mn9y** (P3) — teach the KISS/packet path to consult the UV-Pro
  `UvproLinkLock` so a conflict from that side surfaces `LinkBusy`, not a raw
  socket error.
- **tuxlink-mjlh** (P3) — APRS messaging over the native UV-Pro `HT_SEND_DATA`
  path (premium tier; depends on Phase 1a). Overlaps the in-flight native-GAIA
  plan above — reconcile.
- **tuxlink-bv0b** (P2, non-gating per operator) — optional Codex cross-provider
  adrev on the nx95 control backend diff, if/when Codex quota returns.

## Worktree / repo state
- **Branch:** this handoff is on `bd-tuxlink-iehg/session-handoff` (off main).
  The main checkout is on `bd-tuxlink-xygm/recover-handoffs` and is held by
  other live sessions (hook blocked direct handoff commits there).
- **Merged-dead worktrees from this session, disposable** (ADR-0009 ritual):
  `bd-tuxlink-nx95-uvpro-benshi-control`, `bd-tuxlink-yh1d-uvpro-attribution`,
  `bd-tuxlink-q1tm-findstation-gps-grid`. All work merged; only regenerable
  `node_modules`/`target`/`dev/scratch` remain (q1tm's `dev/scratch` has the
  inventory scripts; nx95's `dev/scratch/benshi-re` has the benlink/HTCommander
  RE clones — keep if you want the protocol reference).
- **Worktree disposal this session:** 119 provably-dead worktrees removed
  (155→36), ~21 GB reclaimed; at-risk `dev/scratch` archived to
  `.claude/worktree-archives/dead-worktrees-devscratch-20260613.tar.gz`. Method:
  `rev-list origin/main..HEAD==0` + clean + no untracked + no embeddeddolt, with
  per-item re-verification. 7 GLOBAL stashes remain (shared `refs/stash`; old
  `task-amd-main-ui`/`fl6e`/`main` WIP snapshots) — review/`git stash drop` when
  convenient; they don't block anything.
- **Still-alive worktrees NOT to touch:** `2f2n` (APRS — now merged but worktree
  lingers), `k61j` (tile-maxzoom, another session, has uncommitted WIP + my
  transient Find-a-Station HMR edit), plus the closed/no-PR branches the disposal
  deliberately kept (`hblz`, `qxr3`, handoff branches, feature branches with
  unmerged commits).

## Gotchas worth carrying forward
- **Effective location = `position_current_fix`, not `config.identity.grid`.**
  Any surface that needs the operator's location must use the PositionArbiter
  (GPS-aware), not the manual config grid. This was the Find-a-Station bug; audit
  other consumers if similar symptoms appear.
- **The main checkout (recover-handoffs) is stale + operator-held.** Investigate
  against `origin/main` (a fresh worktree, or `git show origin/main:<path>`),
  never the main checkout source.
- **Bash cwd drifts** between worktrees mid-session; the main-checkout-race hook
  reads the persisted payload `.cwd`, so run a STANDALONE `cd <worktree>` before
  any git write op.
