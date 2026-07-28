# baseline1-base: base-arm regression test of the wire-compat surface

Date: 2026-07-28. Agent: chasm-wren-crag. Run window 09:01Z to 11:28Z on R2,
Spark inference (qwen35-122b-nvfp4), judge sonnet-5 on the Pi.

## What this run is

The first build-only ladder under the base-arm-only regime: 18 cells x build
x 3 attempts = 54 bundles, wire-compat binary at main c25ed259 (PR #1281
find_stations wire-teaching + PR #1284 stringify normalizer and
routines_get edit_protocol). Purpose, per operator directive: verify the
wire-compat surface changes did not regress base Qwen before implementing
the Qwen-lift queue.

Mid-run correction (recorded in run.log and PROVENANCE.md): the ladder was
initially launched with the driver's stale `REVCONDS="off"` default, adding
a retired reviewer column (108-bundle shape). The operator caught it; the
run was stopped, the driver set to `REVCONDS=""`, rev_off output moved to
`_extraneous-rev_off/` (excluded from all analysis), and the ladder
relaunched build-only. Banked build bundles were preserved by the
score.json idempotency skip; nothing scored was lost.

## Result: no degradation. Gate CLEAR for the Qwen-lift queue.

Aggregate judged verdicts, 54/54 bundles scored and judged, fingerprint
join clean (zero mismatches):

| run | PASS | PARTIAL | FAIL |
|---|---|---|---|
| baseline0 base/build (n=54) | 19 | ~ | ~ |
| baseline1-base (n=54) | 18 | 17 | 19 |

18/54 vs 19/54 is a statistical wash (SE of a ~35% rate at n=54 is about
3.5 bundles). Cell movement is symmetric: up C3 (+2), P1 (+1), S4 (+1);
down E1 (-2), S3 (-2), S1 (-1). No cell crossed the 0/3-vs-3/3 bar the
sample-size ruling requires before a per-cell delta means anything.

## Per-cell table

| cell | baseline1 | baseline0 base/build | verdicts (attempts 1-3) |
|---|---|---|---|
| A1 | 0/3 | 0/3 | FAIL, FAIL, PARTIAL |
| A2 | 1/3 | 1/3 | PARTIAL, PASS, PARTIAL |
| C1 | 0/3 | 0/3 | FAIL, FAIL, FAIL |
| C2 | 0/3 | 0/3 | FAIL, FAIL, FAIL |
| C3 | 3/3 | 1/3 | PASS, PASS, PASS |
| E1 | 1/3 | 3/3 | PARTIAL, PARTIAL, PASS |
| E2 | 0/3 | 0/3 | PARTIAL, PARTIAL, PARTIAL |
| E3 | 0/3 | 0/3 | PARTIAL, FAIL, PARTIAL |
| EU1 | 0/3 | 0/3 | FAIL, FAIL, FAIL |
| EU2 | 0/3 | 0/3 | FAIL, FAIL, FAIL |
| EU3 | 3/3 | 3/3 | PASS, PASS, PASS |
| P1 | 3/3 | 2/3 | PASS, PASS, PASS |
| P2 | 3/3 | 3/3 | PASS, PASS, PASS |
| P3 | 0/3 | 0/3 | FAIL, PARTIAL, PARTIAL |
| S1 | 1/3 | 2/3 | PARTIAL, PASS, PARTIAL |
| S2 | 0/3 | 0/3 | FAIL, FAIL, PARTIAL |
| S3 | 0/3 | 2/3 | PARTIAL, PARTIAL, FAIL |
| S4 | 3/3 | 2/3 | PASS, PASS, PASS |

Outcomes: 50 completed, 3 tool_denied (all C2), 1 cancelled (EU1 #3).

## Findings

### 1. The editing-loop class is eliminated on this surface

Zero of 54 bundles contain a run of 5 or more byte-identical consecutive
tool calls. Baseline0 had 12/216 (5.6%), including base/P1 build #3
cancelled at the turn cap with 40 identical find_stations explore calls.
P1 is now 3/3 PASS at about 12 turns per attempt. This was the class the
wire-teaching (ready-to-send next_call, sparse filter serialization,
corrected description) existed to kill; on this evidence it worked for
qwen, matching the earlier GLM A/B result.

### 2. The three declining cells are capability, not surface

None of the failure notes for E1, S3, or S1 reference the changed surface
(no find_stations refinement confusion, no edit_protocol misuse, no
stringify artifacts). The signatures are known families:

- E1 (3/3 down to 1/3): one attempt failed only validation cleanliness at
  save time (unacked TX, unconfigured rig); one attempt is final_text
  dishonesty, describing mail exchange the saved def never contained. Both
  are top-of-taxonomy families from baseline0.
- S3 (2/3 down to 0/3): all three attempts share the same structural triad:
  compose staged after the first connect, the required 5-minute delay
  omitted, and the second connect re-walking the full station list instead
  of pinning the successful peer. Identical signature three times at temp
  0.2 reads as mode-locking on one wrong plan, not noise; S3 is the first
  transcript read of the lift round.
- S1 (2/3 down to 1/3): two attempts with swapped branch arms (APRS
  "no gateway" fires on connect success). Same family as S3.

### 3. C2's failure mode is the C1-class predicate ambiguity, on the consent gate

All three C2 attempts read "test the gateway" as live in-session execution,
hit the send-authority guard (tool_denied, production-real behavior), and
stopped without authoring a routine. This is the direct-action-vs-authoring
misreading flagged for C1 at baseline0, now with a second cell exhibiting
it. Strengthens the predicate-review item in the lift queue; C1 and C2
review together.

### 4. EU1 remains zero-output, and it is not a loop

EU1 #3 was cancelled at the 40-turn cap with varied tool calls and zero
deliverables; the other two attempts failed with the familiar ~500-char
truncation smell. The output-truncation check stays first in the lift
queue.

### 5. Operational: cleanest ladder to date

54/54 scored on the first pass, no failed units, no sweep needed, no Xvfb
collisions. Wall clock about 2.5h including the mid-run correction stop.

## Next (pre-authorized chain)

Per the operator's standing grant recorded 2026-07-28: implement the
Qwen-lift queue (EU1 output-truncation, voacapl staging for E2/E3, C1/C2
predicate review, qhyre context pruning if bundle data supports it), PR,
merge on CI green, rebuild on R2 with the strings-gate, and launch the
next build-only 54-bundle ladder to measure lift.

Data: joined per-bundle rows in the session scratchpad
(`baseline1_joined.json`); raw bundles, run.log, and PROVENANCE.md in the
R2 run dir `~/6i8jz-run/battery-results/baseline1-base/`; judgments at
`dev/scratch/baseline1-judge/ladder2-judgments.jsonl` (local).
