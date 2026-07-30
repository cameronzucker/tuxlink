# laguna1-t07: Laguna underperforms qwen overall but wins two cells outright, and brings a new failure class

Date: 2026-07-30. Agent: chasm-wren-crag. Main pass 20:36Z to 22:35Z on the
dual-Spark cluster (chains pinned per box), plus a 10-attempt redo pass
22:37Z to 22:48Z (below). Driver `ladder3-cluster.sh`, judge sonnet-5
live-daemon (laguna1-judge workdir, fingerprint-keyed). Binary main
bc9bc648: the SAME generation as control1-base, so this is the cross-model
arm of the n=10 regime. Model laguna-s21-nvfp4 (poolside Laguna-S 2.1
NVFP4), temp 0.7, width 16, TURN_TIMEOUT 2700s (raised from control1's
1800s per the width-16 censoring lever; duration comparisons must note it).

Comparability caveat, stated up front: model AND temperature differ from
control1 (0.7 vs 0.2, deliberate operator config for this arm). Cell-level
contrasts below are directional model characterization, not a controlled
single-variable experiment.

## Topline

180/180 bundles judged, 0 missing, 0 stale fingerprints. 9 harness-invalid
(aymi7 class: C2 5, EU2 3, EU3 1). Of 171 valid: 35 PASS / 29 PARTIAL /
107 FAIL. Attempt-level pass rate 20.5 percent vs qwen's 31 percent on the
identical binary. Laguna is NOT a drop-in improvement over qwen at this
task family; it is differently shaped.

## Where Laguna wins outright

- **S2: 7/10 PASS vs qwen 0/10** (qwen: 0 PASS / 7 PARTIAL / 3 FAIL).
  The largest single-cell gap in either direction across the program.
- **P2: 8/10 PASS vs qwen 4/10.** The cell qwen resolved to a 40 percent
  rate at n=10, Laguna nearly saturates.
- C1 shows first-ever movement on a wall cell: 2/10 PARTIAL where qwen was
  0/30 across all runs (still 0 PASS).

## Where Laguna collapses

- **S3 (the validator-depth lever cell): 0 PASS / 1 PARTIAL / 9 FAIL vs
  qwen 5/4/1.** The absorption lint that measurably lifted qwen did not
  rescue Laguna; its S3 failures are upstream of the lint's teaching.
- **A2: 2/10 vs qwen 8/10. P1: 5/10 vs 9/10. C3: 1/10 vs 6/10.**
- E1: 0 PASS / 10 FAIL vs qwen 3 PASS / 7 PARTIAL. P3: 10/10 FAIL vs
  qwen 10/10 PARTIAL. E2: 0/2/8 vs qwen 0/10/0. The near-miss cells where
  qwen reliably landed PARTIAL, Laguna lands FAIL.
- EU3 (honest-diagnosis control): 8/10 PASS vs qwen 10/10. Still strong
  but the first FAIL ever recorded on the control cell.
- Walls hold on both models: A1 0/10, EU1 0/10 (see below), C1 0 PASS.

## The new failure class: provider-turn-cap exhaustion

38 of 180 bundles (21 percent) ended `cancelled` with cancel_reason
"provider-turn cap reached (40/40)". Control1 had 3 cancellations in 180,
all slow-turn wall-clock timeouts; Laguna's are the opposite phenomenon:
fast churn. One A-family attempt burned 43 tool calls and 1.26M prompt
tokens in 144 seconds. EU1 is the extreme case (9/10 attempts cap out):
where qwen stalls quietly on EU1, Laguna loops actively until the budget
dies. Judged outcomes of capped bundles: 1 PASS / 8 PARTIAL / 29 FAIL,
so the cap mostly truncates work that was not converging anyway, but a
40-turn cap is now known to bind ~1/5 of Laguna bundles and any future
Laguna arm should either raise it deliberately or treat 40 as part of the
measured envelope.

## Denial handling differs from qwen

Only 9 aymi7 harness-invalid bundles vs qwen's 13 on the same binary, and
C2 produced 5 valid (FAIL) bundles where qwen produced zero valid C2 data:
Laguna more often keeps working after a denial in ways that do not trip
the one-shot post-denial kill. The aymi7 fix (10efaa25, next generation)
remains required for C2 to be fully measurable on both models.

## Harness and infrastructure events (all resolved, data clean)

1. **Node 1 vllm silent wedge (20:57Z to 21:42Z):** the engine stalled with
   7 requests held "running", /health green and utilization.gpu reporting
   96 percent while power draw sat at 17W and generation_tokens_total was
   flat. Detected by the operator from hardware activity, not by any
   software signal. Recovery: chains and in-flight attempts SIGSTOPped,
   container restarted, SIGCONT after a verified real completion; zero
   fresh attempts hit the down endpoint. Lesson recorded: on GB10, power
   draw and token-counter deltas are the liveness signals; util percent
   and /health both lie during an engine hang. Note the nginx balancer
   fronting the Elmer endpoint would also route to a wedged backend; if
   this recurs a token-delta health probe is the fix.
2. **Egress-guard assert, E3 attempt-6:** the battery's post-cell
   ASSERT-NO-EGRESS fail-safe aborted one attempt (tainted=true at cell
   end; nothing armed, nothing sent). One occurrence in 180 bundles; did
   not recur on redo. Safety behaved as designed: refuse to certify, abort
   unscored.
3. **Redo pass:** the 9 wedge-damaged attempts plus E3#6 were archived
   (wedge-archive-20260730T2240Z.tar.gz in the run dir), deleted, and
   re-run single-box at conc=6; all completed with real outcomes, giving
   the full 180. Redo rows carry conc=6 in latency.jsonl.

## Durations

conc=16 median 394s / p90 785s (qwen at width 16: median 968s / p90
1854s). Laguna is roughly 2.5x faster per bundle despite the higher turn
count, consistent with its churn profile: many short turns rather than few
long ones. The raised 2700s turn timeout was never the binding constraint;
the 40-turn cap was. Per-box: twin-bramble 100 rows, inference2 90
(includes the 10 redo rows on twin-bramble).

## Program implications

- The lever-round generation now has both arms measured at n=10 on one
  binary: qwen 31 percent, Laguna 20.5 percent, with strongly
  complementary cell profiles (Laguna: S2/P2; qwen: A2/P1/C3/S3/E-family).
  Neither dominates; model choice per task family is a real decision.
- S3 confirms absorption levers are model-relative: a lint that closes
  qwen's loop does not close Laguna's. Lever design should be validated
  per model, not assumed transferable.
- Turn-cap policy is now a measured model parameter: 40 binds 21 percent
  of Laguna bundles and ~0 percent of qwen bundles.
- Next: generation guard lifts. Rebuild at current main (aymi7 10efaa25),
  strings-gate, fold in the grc1j fix, then the next control run makes C2
  measurable for the first time on both models.

Data: R2 `~/6i8jz-run/battery-results/laguna1-t07/` (PROVENANCE.md with
wedge/redo provenance, laguna1_joined.json, latency.jsonl with box/conc
per row, judgments.jsonl, wedge-archive tarball); judge workdir
`dev/scratch/laguna1-judge/` (local).

Agent: chasm-wren-crag
