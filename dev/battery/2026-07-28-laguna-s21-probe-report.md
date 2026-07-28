# Laguna S 2.1 NVFP4 on the bench: two probes, four redirects, and a mostly-shallow overfit verdict

Date: 2026-07-28. Agent: chasm-wren-crag. Model: poolside/Laguna-S-2.1-NVFP4
(117.6B/A8.5B MoE, ~71 GB NVFP4), served locally on the Spark via
vllm/vllm-openai:v0.25.1 (native FLASHINFER_CUTLASS NvFp4 path confirmed in
logs), DFlash speculative decoding, poolside_v1 parsers. Wire-compat harness
binary at main aea37642. Judge sonnet-5, standard rubric, single attempt per
cell (directional only per the n=10 ruling). Evidence issue: tuxlink-07vaa.

Context: the model is brand new, distilled, and carries community warnings
about overfitting to poolside's own tool stack. There is no reliable public
data on out-of-stack generalization; these probes are primary evidence.

## Headline numbers

| run | PASS | PARTIAL | FAIL | notes |
|---|---|---|---|---|
| probe @ temp 0.2 (battery convention) | 4 | 2 | 12 | 6 cap-out loops, 2 essay-dumps |
| probe @ temp 0.7 (card-recommended) | 5 | 3 | 10 | loops mostly dissolved; 2 harness-terminal deaths |
| qwen35-122b baseline1 (n=3, reference) | 18/54 | | | 33% |

Union-of-best across the two probes: 8 of 18 cells produced at least one
judged PASS from a single attempt, including S2 (0/6 for qwen across both
recent ladders) and a textbook EU3 honest-diagnosis pass.

## Failure taxonomy at 0.2, and what temperature changed

1. Edit-churn (P1): saved a green routine then made 37 successful polish
   edits to the turn cap. GONE at 0.7 (clean 6-turn completion).
2. Catalog-hunting (P3): 43 routines_actions_list calls probing for actions
   from its home stack (control.retry) and for agent TOOLS it expected as
   routine actions (predict_path, find_stations-with-recommendation). GONE
   at 0.7, replaced by a fast attempt that died on class 4 below.
3. Research-forever (E1, E2, EU1, EU2): 33-36 docs_read calls, never
   transitioning to build. PARTIALLY temperature-robust: E2 resolved; EU1
   and EU2 still cap out at 0.7; E1 DEGRADED into a 31x byte-identical
   docs_search loop, the classic degenerate class, on a tool surface
   (docs_search) that carries no repeat-query wire-teaching.
4. Stringified-def deaths (S3@0.2 redirect, S4 and P3 @0.7): model emits
   the routines_save def as a string with a JSON syntax slip; the boundary
   normalizer correctly declines to coerce unparseable strings, but the
   runner then TERMINATES the run instead of round-tripping an instructive
   error (tuxlink-le9h9). Object-shaped-but-wrong defs get retryable
   errors that models demonstrably recover from; one missing comma gets
   the death penalty. This asymmetry cost Laguna 1-2 passes at 0.7 and is
   ours, not the model's.
5. Essay-dumps (S3, A1, C1 at 0.2; A1, C1, E2 at 0.7): answers the task in
   10-17k chars of prose, zero or near-zero tool calls, nothing saved.
   S3 fully converted at 0.7 (24 working turns, saved+green). A1/C1 are
   prose-preferring at both temperatures.
6. Deny-text exit (C2, both temps): honest denial report, then task
   abandonment, exactly the tuxlink-shopf coaching defect measured on qwen
   and Inkling.

## Redirect experiments (operator-requested): mostly-shallow overfit

One operator sentence per failure class, injected via the harness's own
revise mechanism, with a demand for self-explanation:

- P1 churn: FULL correction (6 turns, saved+green+enabled). Explanation,
  verified against transcript: "chasing a clean validation - each edit
  fixed one warning only to surface another." Its learned stop criterion
  is warnings==0; our surface emits permanent acceptable_warnings by
  design. Fix: state completion in the validator disposition text.
- E2 research loop: corrected (built and saved). Explanation: "trying to
  verify exact JSON shapes before building... I should have trusted the
  catalog." Confidence calibration, not incapacity.
- P3 phantom actions: precise explanation (named control.retry,
  predict_path, ft8_heard_stations as what it hunted), revealing a
  tools-vs-actions namespace conflation; execution trailed off unfinished.
- S3 essay-dump: channel corrected (engaged tools immediately), then
  killed by the le9h9 terminal asymmetry before it could retry.

The deep-overfit signature (non-engagement under explicit correction)
appeared nowhere. Self-explanations were the most accurate of any model
tested in this program; all were checked against transcripts.

## Serving characteristics

40W GPU draw during serial probing (vs 60-80W for qwen ladders at width
8) with decode at the card's spec (~12-15 tok/s wall-inclusive). The
architecture (A8.5B active, 3:1 SWA at 512 tokens, FP8 KV, ~850K KV-token
capacity) implies materially better width scaling than qwen on the same
box; poolside reports near-linear throughput at low concurrency. To be
measured properly from latency.jsonl whenever a full ladder runs.

## Disposition

Absorption-fix dependencies before a ladder is worth running (all filed,
all also benefiting qwen, all gated on a qwen control ladder per the
2026-07-28 ruling): le9h9 (retryable stringify error), stop-signal
completion teaching in validator dispositions, shopf deny-text reword,
8mofz envelope absorption, and a docs_search repeat-query teaching
candidate from E1. After those land: full 54-bundle Laguna ladder at temp
0.7, alongside the qwen control.

Raw bundles: R2 ~/laguna-probe (0.2), ~/laguna-probe-t07 (0.7),
~/laguna-probe/redirect (interventions). Judgments in the session
scratchpad; per-cell three-way table on tuxlink-07vaa.
