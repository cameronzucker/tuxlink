# Handoff: ARDOP CAT rig control built+reviewed; VARA parity + 5 fixes are next

**Date:** 2026-06-27 · **Agent:** butte-crag-marten · **bd:** tuxlink-8fkkk
**Branch:** `bd-tuxlink-8fkkk/rig-control-single-pane` · **PR:** #922 (draft) · **Worktree:** `worktrees/bd-tuxlink-8fkkk-rig-control-single-pane`

## TL;DR
The WLE-parity CAT rig-control feature is **fully built, reviewed, and CI-green for the
ARDOP path** (20 commits). The mandatory **wire-walk gate failed Flow 2 (VARA)**: the
feature is ARDOP-only — VARA has no CAT tune. Operator decided (2026-06-27) to **build
VARA parity now** (most-popular mode; don't forward the dev-Pi's limitation to the
userbase — see memory `feedback_dont_forward_env_limits_to_userbase`). Final review +
cross-provider Codex adrev also surfaced **5 real fixes**. None merged yet — PR stays
draft until VARA + the 5 fixes land and the wire-walk passes both flows.

## What shipped this session (ARDOP, complete + reviewed, CI green)
Backend `tux-rig` crate (rigctld client + managed lifecycle + close-serial), config +
TS DTO, pre-audio CAT tune step, operator-gated QSY-on-fail (backend), the frequency
element + Tune button + Rig control expander, and the live-VFO poll thread. Every task
passed two-stage review; real bugs caught+fixed (rigctld two-line desync; orphan-on-
spawn-timeout; 3 clippy issues). **`verify` is green on both arches at HEAD 5f6c7d59.**
Per-task trail: `.superpowers/sdd/progress.md` (the SDD ledger).

**Wire-walk Flow 1 (ARDOP) traced ✅ end-to-end:** Find a Station "Use" →
`StationRail.onUse` → `channelToDial` `freq: mhz(khz)`=`(khz/1000).toFixed(3)`="7.102"
([channelGrouping.ts:39-48]) → `handlePrefill` → `freqHz`=7102000 →
`modem_ardop_connect({target,freqHz})` (ArdopRadioPanel.tsx:779) →
`tune_rig_for_connect` → `ManagedRig::spawn` → `tune(hz,PKTUSB)`.

## GATE FINDINGS — the next session's work list

### A. Wire-walk Flow 2 (VARA) BROKEN — build parity (operator: do now)
Evidence: `VaraRadioPanel.tsx` has 0 frequency-element refs; rig fields live only on
`ArdopUiConfig` (config.rs:908), `VaraUiConfig` (config.rs:1195) has none; `winlink/
modem/vara/` has no `tux_rig`/tune step. **CAT control is radio-level (one radio, all
modes)** — so the fix is shared config, not per-mode duplication.

1. **Hoist rig config to a shared `RigUiConfig`** at the top-level `Config` (e.g.
   `Config.rig`), consumed by BOTH ARDOP and VARA connect flows. Move the 7 rig fields
   off `ArdopUiConfig` → `RigUiConfig` (`rig_hamlib_model`, `rigctld_host/port/binary`,
   `close_serial_sequencing`, `live_vfo_poll`, `qsy_on_fail`; CAT serial `cat_serial_path`
   is also radio-level — consider moving it too). Update: the hand-written
   `Deserialize`/`Shadow`/`Default` for whatever struct holds them, the TS DTO + the
   `config_get/set` command(s), `rig_config_from` (read from the shared source), and the
   Rig control expander bindings (it should write the shared config, and be reachable
   from VARA too — or live in a shared Settings location). **Fold the Codex-P1 port
   fix here** (see C1).
2. **VARA connect tune step** — insert `tune_rig_for_connect` into the VARA connect
   flow (the VARA equivalent of `dial_one_candidate`, pre-audio). `tux-rig` is mode-
   agnostic; reuse it. Mirror close-serial release / DRA-100 keep + live-VFO poll.
3. **`VaraRadioPanel` frequency element + Tune + prefill-freq** — mirror Tasks 10/11
   (the ArdopRadioPanel work): freq input → `freqHz` sent on VARA connect; `handlePrefill`
   sets freq from `dial.freq`; a Rig control affordance (or point to the shared one).

### B. QSY-on-fail is INERT (final review I1 + Codex P2) — wire it
No frontend sends `qsyCandidates`, so `walk_candidates` always gets 1 element and
`qsy_on_fail` gates nothing (ArdopRadioPanel.tsx:777-780). Operator earlier chose QSY
"user-selectable" → it must WORK. Wire an ordered `qsyCandidates` list from Find a
Station's ranked channels (the handoff currently carries one dial; extend it to carry
the ranked top-N for the destination), for BOTH ARDOP + VARA. Reconcile the
`qsy_on_fail` doc-comment (config.rs:1020-1023 describes pre-connect QSY, not walk-on-
fail). If wiring is deferred, hide/disable the toggle — do not ship a dead control.

### C. Real bugs from Codex cross-provider adrev (corroborated where noted)
- **C1 [P1] rigctld/CAT-bridge port collision** — `default_rigctld_port`==`default_cat_
  bridge_port`==4532 (config.rs:890-891). Internal-codec + CAT-PTT: bridge binds 4532 at
  spawn, then rigctld tune fails to bind 4532. Fix: give rigctld a distinct default
  (e.g. 4534) — it's the new field, so no existing config depends on it. (Do in A1.)
- **C2 [P1] abort-before-ARQ** — the walk doesn't re-check the abort generation right
  before `connect_arq` (modem_commands.rs:634-638); the tune step's added latency widens
  the abort-miss window. Check `close_generation` after tune, before `connect_arq`.
- **C3 [P2] bound the tune reads** — the tune path uses the plain `RigctldClient::connect`
  (no read timeout); a hung rigctld blocks the connect. Use the `connect_with_timeout`
  variant (already added for the poll thread, tux-rig client.rs) in the tune path
  (modem_commands.rs:1780-1783).
- **C4 [P2] freq normalize + clear-on-empty** (= final review I2) — `favorite.freq` is a
  freeform user string; saved favorites store kHz like "14105.0" → Task 11 regex →
  ×1e6 = 14.105 GHz (1000× wrong; Find-a-Station path is fine, only saved-favorite path
  bugs). Fix: carry numeric kHz/Hz through the prefill DTO (don't re-parse a display
  string), or normalize by magnitude; AND clear `freqMhz` when `dial.freq` is absent so
  a new gateway doesn't inherit the previous freq (ArdopRadioPanel.tsx:444-447).

### D. Minors (fine as follow-up; triaged in the final review)
- live-VFO connect-time stale ≤1 write (self-healing ≤2s) — benign.
- `start_rig_poll` `.expect` on thread-spawn (modem_status.rs:680) — Tauri-caught;
  optional log-and-return.
- CAT-port shown in both PTT + Rig sections via shared state — cosmetic.

## After A–C: re-run the gates, then mark ready
Final whole-branch review → wire-walk (trace BOTH ARDOP + VARA flows) → Codex adrev →
CI green → mark PR ready → operator merge (no-squash, ADR 0010). On-air validation is
operator-only (RADIO-1).

## Process notes for the next session
- **Pi can't compile Rust** — push the draft PR; CI is the compile/clippy/test gate.
  **TS verifies locally** (`pnpm vitest run`, `pnpm exec tsc --noEmit`) — use it.
- **Subagents can't commit in the worktree** (main-checkout hook). They code+test+STOP
  dirty; the PARENT commits (standalone `cd worktree` then commit, since cwd resets
  per turn).
- **Clippy `-D warnings`** surfaced 3 issues only at CI (snake_case, field_reassign_
  with_default, type_complexity) — author Rust tests with struct-update syntax + snake
  case + type aliases for complex closure returns to avoid the round-trips.
- Codex adrev transcript: `dev/adversarial/2026-06-27-rig-control-codex.md` (gitignored).
- SDD ledger: `.superpowers/sdd/progress.md`.

## Branch / tree / push state
HEAD `5f6c7d59`, clean, pushed. `verify` green both arches; build-linux finishing.
No stray worktrees created beyond this one. Design spec + plan are committed on the
branch (`docs/superpowers/specs/2026-06-26-rig-control-single-pane-design.md`,
`docs/superpowers/plans/2026-06-26-rig-control-single-pane.md`) — extend the plan with
the A–C tasks at session start.
