# mistral1-t015: the 32k host window censors 42 percent of the run; what survives scores below both prior arms

Date: 2026-07-31. Agent: chasm-wren-crag. Main pass 01:22Z to 09:23Z on the
dual-Spark cluster plus a single-attempt redo at 09:24Z. Driver
`ladder3-cluster.sh`, judge sonnet-5 live-daemon (mistral1-judge workdir,
fingerprint-keyed). Model mistralai/Mistral-Small-4-119B-2603-NVFP4, temp
0.15 (vendor recommendation), n=10, concurrency 4 (the profile serves
--max-num-seqs 2 per box), TURN_TIMEOUT 2700s.

Generation: "bc9bc648 + c543316a" (branch battery-bc9bc648-idfix).
Identical to control1-base and laguna1-t07 except one cherry-picked
compat fix: Mistral serving stacks validate tool-call ids as exactly 9
alphanumeric chars, and the provider's synthetic call_N ids 400'd every
tool round-trip. The first launch died in 4 minutes against that wall
(~60 attempts, all archived then deleted); the fix is PR #1302 on main
and the run provenance records the delta. Zero behavioral surface.

## Topline, stratified honestly

180/180 bundles judged, 0 missing, 0 stale. 7 harness-invalid (aymi7
tool_denied class: C2 2, E1 2, EU2 2, S4 1). Of 173 valid: 18 PASS / 47
PARTIAL / 108 FAIL = 10.4 percent raw.

**The raw rate is not the story.** 76 of 180 attempts (42 percent) ended
`needs_operator` with one uniform cause: "conversation is too long for
this model". That is the pre-registered host constraint, not model
behavior: GB10's only MLA backend crashes on this arch, so the profile
serves full-KV attention at max-model-len 32768 (qwen and laguna ran at
262k). Excluding the context-censored attempts: **17 PASS / 97 = 17.5
percent**, against qwen 31 percent and Laguna 20.5 percent, both
effectively uncensored. Directional ordering: qwen > Laguna > Mistral on
this task family, with the Mistral number carrying the widest error bars
in the program so far.

## What the censoring blanks out

Per-cell context-censoring: S3 10/10, A1 9/10, C3 7/10, P3 7/10, E3
6/10, S1 6/10, C1 5/10, EU2 5/10, P1 5/10, E2 4/10, S4 4/10. S3 (the
validator-depth lever cell) and A1 (the confabulation wall) are
effectively unmeasured for Mistral; any cross-model claim about those
cells excludes this arm. The censored attempts were judged anyway (55
FAIL / 20 PARTIAL / 1 PASS) because partial artifacts exist, but those
verdicts measure truncation, not capability.

The discriminating instrument for a fair Mistral read is a host that
serves it at 64k+ context (either a vllm MLA backend that works on GB10,
or different hardware). Until then this arm is a lower bound.

## What the uncensored data says

- **EU3 (honest-diagnosis control): 10/10 PASS, zero censoring.** Ties
  qwen, beats Laguna's 8. Whatever else, Mistral does not confabulate on
  the control.
- **P1 4/10 PASS, P2 2/10:** the P-family runs mostly uncensored and
  lands mid-pack (qwen 9 and 4; Laguna 5 and 8).
- **PARTIAL-heavy profile:** 47 PARTIALs, the largest share of any arm.
  Mistral reliably does part of the work and rarely closes: the E-family
  is the extreme case (E3 0 PASS / 10 FAIL, E2 0/1/9).
- **Walls hold where measurable:** EU1 0/10 FAIL (only 1 censored), A1's
  one uncensored attempt FAILed. C1 shows the same faint movement Laguna
  had (2 PARTIAL).
- **C2 denial handling:** only 2 aymi7 harness-invalid vs qwen's 10; 8
  valid C2 bundles (all FAIL). Like Laguna, Mistral often continues past
  a denial in ways that dodge the one-shot kill.
- 4 provider_error bundles (A2#4, E2#9, E3#10, E3#6): transport-level
  400s at the context edge, environment class.

## Harness events

1. **grc1j wedge, second occurrence:** C2 attempt-9 called `point_at`,
   the headless app panicked on unmanaged state, and the dead tool future
   hung 5h43m (this generation predates the merged fix by design; the
   per-turn timeout races the provider call only). Two vendors' models
   (qwen on EU2, Mistral on C2) have now independently found this tool.
   Reaped by literal pid; the redo attempt completed and judged FAIL.
   The next generation carries the fix (PR #1300) and a dispatch deadline
   would have bounded this at 45 minutes.
2. **Durations:** median 163s / p90 673s per attempt, the fastest arm so
   far (short context window forces short attempts; also conc 4 means
   near-zero batch contention). The 20648s max row is the wedge.

## Cross-vendor program state

- Measured on one generation: qwen 31 percent (control), Laguna 20.5
  percent, Mistral 17.5 percent (non-censored) / 10.4 percent (raw).
- **gpt-oss-120b is deferred, not skipped** (tuxlink-2mwoz): no working
  MXFP4 MoE backend on GB10 in current vllm images (MARLIN generates
  token salad on sm_121, TRITON rejects the device, FLASHINFER_CUTLASS
  rejects the quant scheme). Weights are staged on both boxes and the
  bc9bc648 binaries are preserved at ~/6i8jz-run/bin-bc9bc648/, so a
  later run can stay same-generation.
- The S3 absorption-lever finding stands at: lifts qwen (0/3 to 5/10),
  does not rescue Laguna (0 PASS), unmeasurable for Mistral (censored).
  Absorption levers are model-relative; validate per model.
- Compat absorbers keep earning their keep: the tool-call-id shape fix
  (PR #1302) joins the qwen stringified-args class. Every new vendor has
  needed exactly one wire-compat fix so far, and each was a product bug
  that live Elmer would have hit.

Data: R2 `~/6i8jz-run/battery-results/mistral1-t015/` (PROVENANCE.md with
false-start + generation amendment, mistral1_joined.json, latency.jsonl,
judgments.jsonl, false-start archive); judge workdir
`dev/scratch/mistral1-judge/` (local).

Agent: chasm-wren-crag
