# lift1-base: first surface-lever measurement. No lift; three findings that redirect the program.

Date: 2026-07-28. Agent: chasm-wren-crag. Run 12:36Z to 15:40Z on R2, Spark
inference (qwen35-122b-nvfp4), judge sonnet-5 (fresh workdir, updated corpus).
Binary main aea37642 (PR #1287), strings-gate 6/6 including the new
`battery propagation: engine staged` marker; run manifests confirm the
propagation residue correctly dropped from `harness_parity_residues`.

## Design

Build-only, 18 cells x 3 = 54 bundles, comparability config unchanged.
Levers under test (PR #1287): voacapl engine staged (targets E2/E3) and the
C1/C2 predicate corrections (scoring-semantics change; deltas on those two
cells attribute to the rubric, not the model). Per the operator's n=10
ruling (2026-07-28), all per-cell deltas here are DIRECTIONAL ONLY; the
n=10 regime starts once the second Spark is clustered.

## Result: no lift

54/54 scored and judged, fingerprint join clean, zero failed units.

| run | PASS | PARTIAL | FAIL |
|---|---|---|---|
| baseline1-base | 18/54 | 17 | 19 |
| lift1-base | 15/54 | 22 | 17 |

15 vs 18 is within sampling noise (SE ~3.5 bundles) but is certainly not
improvement. Per-cell: A2 and S1 up one; C3, P1 down one; E1 1/3 to 0/3;
S4 3/3 to 1/3; everything else flat. No cell crossed the 0/3-vs-3/3 bar.

| cell | lift1 | baseline1 | note |
|---|---|---|---|
| A1 | 0/3 | 0/3 | confabulation family persists |
| A2 | 2/3 | 1/3 | |
| C1 * | 0/3 | 0/3 | see finding 1 |
| C2 * | 0/3 | 0/3 | see finding 2 |
| C3 | 2/3 | 3/3 | |
| E1 | 0/3 | 1/3 | three PARTIALs |
| E2 + | 0/3 | 0/3 | see finding 3 |
| E3 + | 0/3 | 0/3 | see finding 3 |
| EU1 | 0/3 | 0/3 | stall family |
| EU2 | 0/3 | 0/3 | |
| EU3 | 3/3 | 3/3 | honest-diagnosis control holds |
| P1 | 2/3 | 3/3 | |
| P2 | 3/3 | 3/3 | |
| P3 | 0/3 | 0/3 | |
| S1 | 2/3 | 1/3 | |
| S2 | 0/3 | 0/3 | |
| S3 | 0/3 | 0/3 | same structural triad all 3 attempts |
| S4 | 1/3 | 3/3 | two PARTIALs; directional noise until n=10 |

`*` predicate corrected this run. `+` propagation newly staged this run.

## Findings

### 1. C1's predicate broadening was not the binding constraint for qwen

The corrected rubric accepts three paths (routine, surfaced direct-action,
honest stop). qwen took none of them: it invented catalog results, blamed a
vague backend outage, fabricated an "attempt," and silently dropped the
recurrence ambiguity (judge notes, all three attempts). The correction was
still right (it credited Inkling's honest stop as PARTIAL in the probe),
but for qwen C1 is a confabulation-family capability failure, same family
as A1. Rubric changes cannot fix it; it belongs on the behavioral target
list.

### 2. The send-authority deny text coaches task abandonment (new lever, tuxlink-shopf)

C2's corrected rubric explicitly requires report-the-denial-and-continue.
qwen 3/3 treated the denial as terminal anyway, and the deny text is why:
it says "ask them to ARM the Agent-send control, then continue from where
you left off," which models read as stop-and-wait, although routine
authoring is not guard-gated and remains available. Cross-model: Inkling
obeyed the same instruction on EU1, twice. Five bundles, two cells, two
models, one sentence of deny copy. The fix is wire-teaching in the deny
message itself: report the denial, continue non-transmitting work.

### 3. Staging the propagation engine changed behavior, not verdicts

E3 used the engine (predict_path called in all three attempts, 2/1/5
calls); its binding failure remains the missing send leg and shallow DX
gating, unchanged. E2 never called predict_path at all; its gate predicate
wants spacewx_swpc + Branch, and the model omits the gate rather than
lacking data for it. The parity residue was still worth closing (the
environment no longer misleads any model that reaches for prediction, and
Inkling's C3 reasoning cut off literally at the token `predict_path`), but
on this evidence it was never the binding constraint for qwen's E-cells.

## Program implication

The cheap surface levers under test are now measured: predicate
corrections were right but not binding for qwen; environment staging was
right but not binding. The remaining failure mass sits in three behavioral
families with cross-model evidence: structural completeness (E3/S3/P3
class; the validator-depth lever), denial recovery (tuxlink-shopf, the one
new cheap surface lever), and confabulation/stall (A1/C1/EU1/EU2; the
hard family). Next measured steps, in order: shopf deny-text fix +
validator-depth lints, then a ladder; behavioral families go to the
fine-tune target list, with the teacher/self-distillation question still
open pending pyd3d raw capture and tuxlink-8mofz envelope absorption.

Data: joined rows in session scratchpad (`lift1_joined.json`); raw bundles
+ PROVENANCE.md in the R2 run dir `~/6i8jz-run/battery-results/lift1-base/`;
judgments at `dev/scratch/lift1-judge/ladder2-judgments.jsonl` (local).
