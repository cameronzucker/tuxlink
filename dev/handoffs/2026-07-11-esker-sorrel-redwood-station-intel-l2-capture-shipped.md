# Handoff — 2026-07-11 (esker-sorrel-redwood): Station Intelligence L2 shipped — live capture + slot-timing decode service merged

One workstream: executed tuxlink-b026z.3 end-to-end from the kickoff gate
(spec → 5-round adrev → plan → 4-round plan review → 19-task SDD with six
review gates → 7 CI fix-forward rounds → final whole-branch review → merge).

## What shipped (PR #1072, branch `bd-tuxlink-b026z.3/station-intel-l2-capture`)

- New std-only leaf crate `src-tauri/tuxlink-capture`: bands (FT8 dial
  table), wavwrite (canonical slot WAV, preflight round-trip vs tuxlink-jt9),
  decimator (51-tap Kaiser polyphase 48k→12k, committed COEFFS + numeric
  response tests + sign-matched saturation KAT), slot (wall-clock-true
  assembler: two clock domains, surplus-drop, min-fill 2400, clock-anomaly
  abandonment, lost-frames drop), state (full machine: axes incl.
  needs-device-selection + capture-wedged, sweep element, N=5/k=20 counters).
- `src-tauri/src/ft8/`: traits + fakes (SampleSource/ClockProbe/EventSink/
  DecodeEngine + Ft8Platform), alsa_source (hw:<index>,0 open, S16 48k,
  wait+nonblock readi, wedge escalation), service (supervisor-first
  lifecycle, 8-step start sequence, capture/decode threads, rendezvous
  sync_channel(0) backpressure, WaterfallTap, 240-slot ring, snapshot),
  arbiter (pause/hold-latch/resume, rig-session serialization, FT8_ARBITER
  global), sweep (dwell scheduler, QSY provenance downgrade, FallbackHold),
  clock (timedatectl probe), commands (six ft8_* + config::update_config
  crate-wide RMW gate), events. Modem yield seams: spawn_ardop_with_yield
  choke wrapper (×4 sites), Dire Wolf spawn_inner hook, VARA spawn_blocking
  seam.
- L1 change: salvage-on-signal parity in tuxlink-jt9 (tuxlink-gujnz CLOSED
  with recorded decision; StderrEof-before-salvage pinned on all arms).
- E2E: committed fixture → ZOH×4 → real capture pipeline → real jt9 → ≥90%
  reference floor (sentinel excluded from the count). CI: libasound2-dev in
  all three workflows (salts v5/v3); deb/rpm gain libasound2 runtime Depends.
- CI green on the final head (fb700963): CI + Release + ECT, both arches,
  2,951 Rust tests.

## Process record

- Spec docs/superpowers/specs/2026-07-10-station-intel-l2-capture-design.md
  (v4 IMPLEMENTED; 5 adrev rounds, 16 P1 + 23 P2 + 20 P3 dispositioned;
  operator flow corrections: front-door zero-config, no device auto-pick,
  always-ask-once picker). Plan docs/superpowers/plans/2026-07-10-station-
  intel-l2-capture.md (Phase A/B execution-validated at authoring; 4 review
  rounds found 2 P1 incl. the rig-lock non-reentrancy deadlock).
- Six gates A–F (min 3 rounds each; findings in gitignored dev/scratch/
  b026z.3-gate-*-findings.md, archived in the worktree tarball). Notable
  catches: Gate A's saturation-KAT gap (whose first fix was itself a false
  green — fixed with a sign-matched overload burst); Gate B's u64-edge
  overflow panics (probe-confirmed); Gate E's stop-heals-wedged P1 + the
  spawn/stop handle race (tuxlink-qea6r filed then CLOSED by the fix).
- Seven CI fix-forward rounds: apt dep sequencing hole (T18 edits pulled
  forward), Config-literal sweeps (src/ then tests/ — 14 sites), dead-code
  cfg-erasure (the lesson: a cfg(test) reader does NOT cover the lib
  target), too_many_arguments bundling (SlotProvenance), fat-pointer casts /
  moved-value / unwrap_err-needs-Debug (the elmer trap recurring), clippy
  field_reassign round, and the held-Sender-across-stop() wedge (2950/2951
  green → the last red test's own bug).
- Final whole-branch review: READY AFTER FIXES (process only); five e2e
  traces clean; hygiene commit fb700963 (stale build-phase comments removed).
- Known blemish: commit c0ec1145 (T4) carries an invented subagent moniker
  trailer `Agent: moraine-ivy-larch` (amend banned; forward tasks enforced
  the exact-trailer directive).

## bd state

- CLOSED this session: tuxlink-gujnz (salvage decision recorded),
  tuxlink-b026z.8 (accepted bound + pipe-fd watermark), tuxlink-qea6r
  (spawn/stop race — fixed at Gate E), tuxlink-b026z.3 (at merge).
- FILED: tuxlink-kqp88 (config-writer migration to update_config, P3),
  QSY-retry-cadence spec-drift bug (P3, filed at final review).
- NEXT epic children: tuxlink-b026z.4 (L3 panel integration — the
  user-reachability layer; the WIRE-WALK GATE runs when L3/L4 land) and
  tuxlink-b026z.5 (L4 MCP tools). The snapshot/command/event surface they
  consume is pinned in the spec §Snapshot/§Commands and implemented.

## L3 kickoff notes (carry these in)

1. Ft8Snapshot is the L3 hydration contract (camelCase serde, 15 fields,
   completeness-tested). Slot phase computes from ring recency — never
   resets on panel reopen.
2. available_devices populates whenever config.ft8.device == None OR
   blocked(device-absent|needs-device-selection) — the picker must render
   even while wsjtx-blocked (one-visit dual-blocker flow).
3. Band chips: cat-absent chips are operator STATEMENTS (band_source
   provenance must render default-unconfirmed distinctly); cat-present chips
   QSY via ft8_set_band (persist-only when not listening).
4. Events: ft8-decodes:slot (SlotRecord per slot) + ft8-listening:change
   (axis/flags/phase/band/sweep). The ribbon badge hook belongs in src/shell/
   (cold-start bundle), four visual states (delta §L3).
5. WaterfallTap: 12kHz i16 blocks, 32×1200 lossy ring — L3 owns FFT +
   column cadence + the render budget (delta's L3 exit gate).
6. capture-wedged is restart-required: set_device/start return errors from
   it; the UI must say "restart Tuxlink".

## Worktree / branch state at handoff

- `worktrees/bd-tuxlink-b026z.3-station-intel-l2-capture`: disposed per ADR
  0009 after merge (inventory → tarball archive incl. gitignored
  .superpowers SDD ledger/briefs/reports + dev/scratch gate findings +
  dev/adversarial transcripts; node_modules + target EXCLUDED → rm -rf →
  prune). Archive at `.claude/worktree-archives/` on this Pi.
- Branch `bd-tuxlink-b026z.3/station-intel-l2-capture`: merged via PR #1072
  (merge-commit per ADR 0010), remote branch deleted, merged-dead.
- Main checkout: untouched all session (other sessions active on it).
- No stashes anywhere.
