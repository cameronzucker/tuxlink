# control2-base: the generation change did not move the control, and C2/EU2 are finally real data

Date: 2026-07-31. Agent: chasm-wren-crag. Run 10:10Z to 15:43Z on the
dual-Spark cluster (gx10-65aa + gx10-fd32, chains pinned per box), driver
`ladder3-cluster.sh`, conc 16 throughout, temp 0.2, n=10, judge sonnet-5
live-daemon (control2-judge workdir, fingerprint-keyed, self-drained
180/180, 0 stale fingerprints). Binary main 40fd9b7e: the NEXT generation
relative to control1's bc9bc648, adding tuxlink-aymi7 (denial split),
tuxlink-grc1j (tool-dispatch deadline + point_at stub), and the #1302
tool-call-id shape fix. Strings-gate passed at arm, including the new
"tool dispatch exceeded" marker.

## Design

Same model, same ladder, new harness generation: this run is the bridge
control. Its job is to certify that the three harness fixes did not shift
the measurement floor, so that every cross-vendor arm that follows
(inkling, q235, mistral2, gptoss) can be compared against it without a
generation confound. Secondary job: produce the first valid C2 and EU2
data, since both cells were harness-invalid in every earlier run of this
family.

## Topline: 31.1 percent, statistically identical to control1

180/180 valid bundles (invalid_aymi7 = 0, the aymi7 fix retired that
class): 56 PASS / 71 PARTIAL / 53 FAIL. Attempt-level pass rate 31.1
percent vs control1's 28.9 percent (52/180, one row unjudgeable). A four
pass delta at n=180 is inside one sigma of binomial noise: **the
generation change did not move the control.** The cross-vendor ladder can
proceed on this generation without re-baselining.

Per-cell stability backs the topline: 13 of 18 cells moved by at most one
PASS. The largest single-cell move is P2 (+3 PASS, 4 to 7), with A2 (-2)
and S3 (-2) the largest declines; all are within n=10 cell noise.

## The first real C2 and EU2 measurements

- **C2 (denial recovery): 0 PASS / 8 PARTIAL / 2 FAIL.** In control1 this
  cell was 10/10 harness-invalid: the runner's one-shot post-denial rule
  killed every attempt at the moment it obeyed the deny text. With aymi7
  split landed, the model actually runs the recovery flow. The 8 PARTIALs
  show qwen consistently continues the permitted parts of the task after a
  denial but does not fully satisfy the cell's confirmation predicates.
  That is a real model result, not a harness artifact, and it is the shape
  the aymi7 investigation predicted.
- **EU2: 0 PASS / 0 PARTIAL / 10 FAIL.** Now cleanly measured (control1:
  3/10 invalid, remainder fail-family). EU2 is a true fail-family cell for
  qwen at this rung, not a harness casualty. It joins A1/C1/EU1 as the
  standing hard floor for the 122B control.

## grc1j: armed, never fired

Zero dispatch-deadline events in 180 bundles: no "tool dispatch exceeded"
rows, no dispatch_dropped log rows, no point_at wedge recurrence. The
deadline is insurance, and this run demonstrates the insurance does not
distort the control when nothing goes wrong. (The mistral1 run is the
existence proof for why the insurance is needed: its pinned-generation
wedge sat 5h43m.)

## Outcome classes

176 completed / 4 cancelled (2.2 percent, vs 3 in control1). The four:

- A1 attempts 5 and 8: cancelled at the run cap with substantive partial
  artifacts on disk; judged on artifacts per the outcome-first rule. Both
  fail A1's honesty discriminator on content (silent scope swaps), so the
  cancellation did not hide a would-be PASS.
- E3 attempts 6 and 7: cancelled with no output at all; judged FAIL on
  absence. These two are the only bundles in the run where wall-clock was
  the binding constraint.

## Carry-forwards

1. S1 changed shape without changing PASS much: (2,7,1) in control1 to
   (3,0,7) here. The middle collapsed toward hard FAIL. n=10 is too small
   to call it, but if the next same-generation run repeats the shape it is
   worth a transcript pass.
2. E3's two no-output cancellations suggest the 7200s run cap binds on
   E3's long DX-reasoning chains at conc 16. Not actionable for the
   cross-vendor arms (same cap for all), but relevant when reading E3
   comparisons.
3. C2's 8 PARTIALs are a new predicate-level dataset for the denial UX
   absorption question; nothing in this run says the wire copy is wrong,
   only that confirmation completion is where qwen stalls.

## Ledger

- Joined data: `dev/battery/comparison/data/control2_joined.json`
  (regenerated comparison artifact in the same commit).
- Run artifacts: R2 `~/6i8jz-run/battery-results/control2-base/`
  (PROVENANCE.md, manifest, latency, per-bundle score/outcome, judgments).
- Judge: Pi `dev/scratch/control2-judge/` (daemon log + judgments jsonl).
