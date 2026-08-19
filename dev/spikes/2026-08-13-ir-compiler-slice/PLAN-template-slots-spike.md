# Template+slots IR spike — PLAN (tuxlink-3gaz7)

**Companion to `SPEC-template-slots-spike.md` (the what); this is the how
and the sequence. Both are the anchor pair per ADR 0031.**

## Session 1 — harness

1. Real routines port in the testserver: new port impl backed by
   `tuxlink-routines` (registry catalog, validator, temp-dir store);
   `routines_run` inert with an honest "execution out of scope" error.
2. Intake-tool plumbing: `routine_template_compile` registered in the
   testserver router only; schema + description written as the SHIPPED
   instruction surface (this replaces the sheet-as-user-message — the tool
   description carries the contract).
3. Compiler skeleton + the `scheduled-connect-with-fallback` expansion +
   first golden tests (the wa-gateways ask end-to-end).
4. Local gates: `cargo test` on the touched crates (Pi-buildable), then PR
   → CI → steward merge on green.

## Session 2 — compiler complete

1. Full expansion mapping (window→Trigger.window, bands lowering,
   log→step-output refs, fail_reason→end), named positioned refusals.
2. Distractor templates (drafted for review in this PR):
   `beacon-schedule` (periodic position announce — deliberately transmit-
   adjacent so selection errors are visible), `log-rotation` (housekeeping,
   no radio). Registry stays a closed enum of 3–4 ids.
3. Golden tests for every matrix cell's expected verdict, including the
   refusal cells and both distractor asks.
4. Codex adversarial round on the compiler (correctness attack: lowering
   fidelity, refusal coverage, id determinism) BEFORE merge. PR → steward.

## Session 3 — the evaluation

1. Matrix v2 runner (adapt `run-insitu.sh`): serving pre-flight; fresh
   testserver per run; cells N1/N2/E1/E2/C1/S1/S2 ×2 + CTRL ×3 ≈ 17 runs;
   emission via the intake tool; verbatim capture as before.
2. Grade: compiler verdicts + per-cell semantic checks + trace assertions;
   mandatory eyeball pass; results doc with raw appendix.
3. The ruling brief for the operator: PASS/FAIL against the SPEC's
   pre-registered criteria, findings, and — if FAIL — the failure shape
   analysis for the challenger decision.
4. PR → steward; bd tuxlink-3gaz7 closes on the operator's ruling, not on
   the merge.

## Standing constraints (inherited, not new)

One git write per call; worktree = this branch's
(`worktrees/bd-tuxlink-3gaz7-template-slots-spike`); merges on green via
steward; no sampling params; no bench; no product wiring; thin handoff
pointing at the SPEC/PLAN pair; any mid-build discovery that the SPEC is
wrong = stop and surface, never quietly adapt (the falsified-premise rule).
