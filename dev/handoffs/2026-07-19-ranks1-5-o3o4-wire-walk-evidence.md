# Wire-walk evidence: ranks 1-5 + O3/O4 (bd tuxlink-iizmk)

Code-level reachability trace of the operator flow author -> dry-run -> run ->
History, on the merged `main` (groups A-F). This is the always-runnable half of
the wire-walk gate (grep + read, no build). The operator-supplied greenfield
flows (per the CLAUDE.md wire-walk rule, the operator names them, not the
agent) plus the live converge-build capture are the F2 next-session steps
recorded in the session handoff.

## The chain, stage by stage (file:line on the merged base)

1. **AUTHOR (palette).** The designer palette is registry-driven: no hand-kept
   action list. `list_actions` projects the live `ActionRegistry` descriptors
   (`src-tauri/src/routines/commands.rs:732`); the five new capabilities are
   registered in `build_registry` (`src-tauri/src/routines/actions/mod.rs:481`)
   -> `FindStations` (:521), `DocsSearch` (:524), `ConfigSetArdop` (:527), plus
   the rank-1/3 status+config sources as `DataService` methods (:279, :315).
   So each new action/source appears in the palette the moment it is
   registered, with its label, flags (incl. the WRITES badge), description, and
   seeded `example_params` (groups D6/E3).

2. **DRY-RUN.** The dry-run registry consults the descriptor's `dry_run_shape`
   against the resolved params (`src-tauri/tuxlink-routines/src/dryrun.rs:132`),
   so `data.find_stations` -> `$s1.callsigns` -> `radio.connect` and branches on
   read outputs complete green in a dry-run (the marquee e2e, group D6). Author
   -> dry-run is a closed loop.

3. **RUN (consent).** A `writes_config` step parks in attended mode via the
   executor predicate (`src-tauri/tuxlink-routines/src/executor.rs:316`), keyed
   on the descriptor flag `config.set_ardop` carries (group C2/D5); automatic
   mode requires the digest-bound `write_ack` (group C3). The consent dialog
   renders write copy, never Part 97 transmit language (group E1).

4. **HISTORY.** Run journals render call/end/park rows with child navigation:
   `call_child` (`src/routines/designer/RunsTab.tsx:363`), `end_reached` (:384),
   and `parkKind` on park rows (:313) — the O3/O4 UI (group B4), validated
   against the 2026-07-18 real-run fixture via the WebKitGTK harness.

## Coverage

Compat-tree §3 recount: **24 of 24 cells human-actionable** (was 0/24 at
analysis time) — the four read families (ranks 1-4) + the first config write
(rank 5) map every corpus step to an action. ADR 0024 dual-actionability met
for the corpus at this revision.

## What the gate still needs (F2, operator/next-session)

- The operator supplies the key user flows greenfield (author each new
  action in the live designer, dry-run, run, read History) — not drafted here,
  per the wire-walk rule against anchoring.
- The live converge-build capture: rebuild converge on R2 (restarts the
  operator's app), execute a probe exercising branch + skip + a sync call
  (child) + End-with-reason, capture the journal to a fresh
  `dev/render-harness/real-run-<date>.json` (`&real=2`), render History. The
  attended `config.set_ardop` write-park needs the operator's own click (an
  operator act), so it is validated by vitest + the E4 harness render, with the
  live click named as an operator step.
