# Pre-compaction state, chasm-wren-crag (2026-07-28 ~09:15Z): wire-compat shipped, teacher bake-off decided-ish, baseline1-base in flight

Same session continues after compaction. This doc is the authoritative state.

## Shipped since the baseline-zero handoff (all merged to main)

1. **PR #1281** (eefln wire-teaching + refinement cap 9): merged.
2. **PR #1284** (ewqiy wire-compat pass): merged by the operator. Boundary
   stringify normalizer in tuxlink-agent-runner/validate.rs (schema-driven,
   kind-exact, oneOf-aware; RAW args recorded / COERCED twin dispatched per
   the GPT-5.5 adrev round), routines_get `edit_protocol` wire-teaching
   (incl. routines_rename), class 3/4 audit dispositions in the PR body.
   bd ewqiy + ryyhi closed. Adrev transcript in dev/adversarial/ (local).
3. ADR 0029 PR #1267 merged on operator instruction; sccrg closed + disposed.
4. Baseline-zero deep analysis delivered in-chat (paired arm reads, predicate
   taxonomy, 43-of-74 one-away PARTIALs); drove the operator directive.

## Operator directives in force

- **Base arm is the play**; skills + reviewer arms RETIRED (wash at baseline
  zero). See battery-methodology memory.
- Wire-compat program sequence was: merge -> frontier retest -> branch. GLM
  retest was NOT clean (2 loops + 1 garbage emission out of 3) -> operator
  redirected: "Test Inkling. GLM was only selected because cheap + benchmarks."
- m71mu (explore returns rows by default) stays PARKED: Inkling's 3/3 weakens
  urgency; decide after the qaq54 probe set.

## Teacher bake-off (P1, wire-compat surface, recorded on bd qaq54)

- GLM-5.2@Novita: 1 clean / 6 attempts total; ~$1.21 spent; failure modes are
  emission quality (loops despite next_call present, invented snapshot ids,
  1-char unparseable def — normalizer behaved correctly).
- **Inkling@BaseTen: 3/3 clean, 7-10 turns, ~30s, ~$0.02/attempt (~60x
  cheaper per success than GLM).** Probe runs are outcome-level (no judge).
- qwen35-122b local: 3/3 (12-13 turns) on the merged surface = no regression.
- OPEN OPERATOR CALLS: teacher flip to Inkling; qaq54 probe-set go (~$1-3
  with Inkling primary over the 0/12 cell list from baseline zero).

## IN FLIGHT: baseline1-base

- First base-only ladder: 18 cells x (build + rev_off) x 3 = 108 bundles,
  conc=8, launched 09:01Z, driver PID 482350 on R2. ETA ~6h (~15:00Z).
- Binary: `~/eefln-ab/src-tauri/target/debug/elmer_battery` @ main c25ed259,
  all five strings-gate markers verified (see methodology memory for list).
  Run dir `~/6i8jz-run/battery-results/baseline1-base/` + PROVENANCE.md.
- Observation: dashboard PID 482790 (LADDER2_ROOT=baseline1-base, :8899);
  judge daemon PID 2983133 on Pi, workdir `dev/scratch/baseline1-judge/`
  (FRESH store); persistent run monitor armed in-session (if compaction or a
  restart kills it, re-arm: poll last-START segment of run.log for
  UNIT FAILED/COMPLETE + driver PID 482350 liveness).
- End-of-run: sweep any failed units (kill whole process trees), verify 108
  scored + judged, aggregate with the fingerprint join (per-file sha256 + NUL,
  aggregator pattern in baseline0 scratchpad script — REWRITE from the recipe
  if scratchpad is gone: walk arm/cell/cond/attempt dirs, join judgments on
  fp, table per cell), report to dev/battery/ comparing base-arm absolutes vs
  baseline0's base arm (SAME arm, different surface — the first legitimate
  before/after since both are parity harnesses; label surface delta clearly).

## Standing cautions

- elmer_battery needs OPENROUTER_API_KEY in env even for Spark runs.
- pgrep self-match illusion (remote ssh pattern greps match their own shell).
- Kill whole process trees (wrapper + binary + Xvfb child).
- ssh+nohup fd-hangs: verify from a fresh connection, TaskStop the wrapper.
- OpenRouter account ~$157 of $200 used.

## Open queue (unchanged unless noted)

tuxlink-qaq54 (probe set, awaiting go), m71mu (parked), l02v0 (out-of-band
deadline), 3cal1, voacapl staging (E2/E3 residue may be load-bearing — see
baseline0 report finding), EU1 output-truncation check (report finding),
C1-class predicate review, Spark boot-autostart (sudo), spark_hwmon MOK,
old worktree disposal set, second Spark arrival prep (qealk).

Agent: chasm-wren-crag
