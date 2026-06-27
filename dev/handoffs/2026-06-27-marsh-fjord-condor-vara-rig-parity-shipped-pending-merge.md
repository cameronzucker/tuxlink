# Handoff: VARA CAT-rig parity + 5 fixes — all gates green, PR #922 READY, pending operator merge

**Date:** 2026-06-27 · **Agent:** marsh-fjord-condor · **bd:** tuxlink-8fkkk
**Branch:** `bd-tuxlink-8fkkk/rig-control-single-pane` · **PR:** #922 (READY) · **HEAD:** `cf9fb4cd`
**Worktree:** `worktrees/bd-tuxlink-8fkkk-rig-control-single-pane`

## TL;DR
The ARDOP-only CAT-rig feature now has **full VARA parity**, the inert QSY toggle is
**wired**, and the **4 Codex findings + C1–C4** are fixed. **All four gates are green**
(final review, Codex adrev, wire-walk on 5 flows, CI both arches). PR #922 is marked
**ready**. The only remaining step is the **operator's no-squash merge** (ADR 0010) and
operator on-air validation (RADIO-1). Nothing is stranded.

## What shipped this session (8 commits, `0c9e5dea..cf9fb4cd`)
- **A1** `refactor(config)` `55dcc59a` — rig + CAT-serial config hoisted to a radio-level
  `Config.rig` (`RigUiConfig`), consumed by BOTH ARDOP and VARA. The 7 rig fields were
  unreleased (no migration); only `cat_serial_path`/`cat_baud` needed a lift, via a
  `serde(remote="Self")` post-deserialize migration (one-way, `skip_serializing`). **C1**
  folded in: `rigctld_port` default 4534 ≠ `cat_bridge_port` 4532. Schema 4→5. New
  `config_get_rig`/`config_set_rig`.
- **C23** `fix(rig)` `3d3cb985` — **C2** abort recheck after tune, before `connect_arq`
  (ARDOP); **C3** `ManagedRig::spawn` uses `connect_with_timeout` (RIG_READ_TIMEOUT 5s).
- **A2** `feat(vara)` `a772151a` — VARA pre-audio CAT tune + ordered-list QSY walk in
  `run_vara_b2f_with_transport`; reuses `tune_rig_for_connect`/`walk_candidates`. Winning
  candidate's rig held across the synchronous exchange (DRA-100), failed candidates
  released. VARA abort recheck included.
- **A1UI** `feat(ui)` `b9ac45a6` — shared `RigControlSection` rendered in BOTH panels,
  bound to `Config.rig` via the new commands. CAT serial editing moved there (radio-level);
  CatCommand PTT keeps key/unkey/bridge-port + points to Rig control for the serial.
- **A3+B** `feat(ui)` `def2799e` — VaraRadioPanel frequency element + Tune button + prefill;
  `rankedDialsFor` + prefill candidates + both panels send `qsyCandidates`; **C4** shared
  `freq.ts` normalizes kHz/MHz by magnitude + clears-on-empty.
- **Codex fixes** `1992e29d` — (P1) VARA post-tune abort recheck before CONNECT; (P1)
  clicked dial forced to `candidates[0]`; (P2) `vara_dial_disconnect` a failed VARA CONNECT
  before the next candidate; (P2) aborted ARDOP walk no longer clobbers Stopped with Error
  (`CONNECT_ABORTED_MSG` sentinel).
- **CI fixups** `0751455b` + `cf9fb4cd` — `too_many_arguments` allow on `modem_vara_b2f_exchange`;
  rig field added to integration-test `Config` literals; 9 redundant `..Default::default()`
  spreads removed; round-trip + abort tests corrected.

## Gate results (all green)
- **Final whole-branch review (opus):** READY TO MERGE, zero blocking findings. Verified the
  `remote="Self"` idiom, all 17 `Config` literals, every changed signature's callers, the VARA
  closure borrow-soundness, payload shapes, C4.
- **Codex cross-provider adrev:** `dev/adversarial/2026-06-27-vara-parity-codex.md` (gitignored,
  ~20k lines). 4 real findings — **all fixed** (Codex caught what the opus review missed;
  cross-provider value confirmed).
- **wire-walk:** ALL 5 operator-sourced flows ✅ WIRED, every hop `file:line`. F1 ARDOP connect,
  **F2 VARA connect (the prior-session gap — now closed)**, F3 QSY-on-fail (clicked-dial first,
  serde `freq_hz` matches, backend walks when `qsy_on_fail`), F4 manual Tune (both panels),
  F5 RigControlSection setup→`Config.rig`→tune-reads-same (+ cold-start no-rig graceful no-op).
- **CI:** green on `cf9fb4cd` — `verify` + `build-linux` + `deb-install` all pass, amd64 + arm64.

## Process notes
- **Pi CANNOT cold-build Rust**, but Codex's earlier `cargo check` left the target **warm**, so
  `cargo clippy --all-targets -D warnings` + `cargo test` finished locally (~5 min each) and
  batched the last CI failures instead of iterating one-per-6-min-round. When a slow remote gate
  keeps failing, check whether the target is warm enough to reproduce it locally.
- **clippy `--all-targets` compiles `src-tauri/tests/`** — a wide struct/signature change must grep
  `tests/` too (this bit once: integration-test `Config` literals).
- Subagents code+STOP-dirty; the parent commits (main-checkout hook). Verified frontend locally
  (tsc + vitest) every UI task.

## Branch / tree / push state
HEAD `cf9fb4cd`, clean, pushed. PR #922 READY. No stray worktrees beyond this one. The plan
extension is committed (`docs/superpowers/plans/2026-06-27-rig-control-vara-parity.md`); the SDD
ledger (`.superpowers/sdd/progress.md`, gitignored) has the full per-task + per-gate trail.

## What's left
1. **Operator merges PR #922** — no-squash merge-commit (ADR 0010). Agents never merge.
2. **Operator on-air validation** (RADIO-1) — ARDOP + VARA connect with a real rig: confirm the
   pre-audio CAT tune sets freq+mode, QSY walks on a failed connect, and abort/Stop is honored
   mid-tune. Agent work is done; only the licensee can run this.
