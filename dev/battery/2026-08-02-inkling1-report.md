# inkling1: first Day-9 model arm — beats the control with a spiky specialist profile

Run: 2026-08-01 06:42 to 20:45 AZT, 180/180 attempts on ONE continuous engine
session (14h02m), zero infrastructure failures, 180/180 judged (sonnet-5),
join clean (0 stale fingerprints, 0 missing). Agent chasm-wren-crag.

Model: `thinkingmachines/Inkling-Small-NVFP4` (276B MoE, 12B active, released
2026-07-23), served TP2 across both DGX Sparks. Serving this model at all
required four load-bearing fixes beyond every published recipe; the durable
record is [`dev/runbooks/inkling-dual-spark/`](../runbooks/inkling-dual-spark/README.md).
Battery config: temp 0.2, ctx 262k, `ELMER_MAX_TOKENS=3000`, **concurrency 1**
(serial — the sconv Triton kernel faults on multi-request batches on SM121;
see runbook dead-ends). Generation `40fd9b7e`, same as control2.

## Topline: 35.6 percent strict — the first arm to beat the control

| | PASS | PARTIAL | FAIL | strict | lenient (P+0.5·PT) |
|---|---|---|---|---|---|
| **inkling1** | 64 | 38 | 78 | **35.6%** | **46.1%** |
| control2 (Qwen3.5 122B) | 56 | — | — | 31.1% | — |

+4.5 points strict over the standing control, on identical harness generation
and judge. Size-class asterisk: Inkling-Small is 276B total / 12B active vs
Qwen3.5's 122B / 10B active; the like-for-like size-class comparison is the
queued q235 arm (Qwen3-235B-A22B).

## The profile is spiky, not flat

| cell | PASS | PARTIAL | FAIL | | cell | PASS | PARTIAL | FAIL |
|---|---|---|---|---|---|---|---|---|
| P1 | 9 | 1 | 0 | | C1 | 0 | 0 | 10 |
| P2 | 10 | 0 | 0 | | C2 | 0 | 0 | 10 |
| P3 | 0 | 4 | 6 | | C3 | 5 | 1 | 4 |
| S1 | 5 | 4 | 1 | | E1 | 4 | 1 | 5 |
| S2 | 5 | 5 | 0 | | E2 | 0 | 2 | 8 |
| S3 | 1 | 6 | 3 | | E3 | 1 | 3 | 6 |
| S4 | 6 | 0 | 4 | | EU1 | 0 | 0 | 10 |
| A1 | 0 | 6 | 4 | | EU2 | 0 | 5 | 5 |
| A2 | 8 | 0 | 2 | | EU3 | 10 | 0 | 0 |

Inkling saturates cells the control never has (P2 10/10, EU3 10/10 — the
pure-troubleshooting cell, notable given the ladder-expansion epic tuxlink-69qtv
will grow exactly that tier) and zeroes cells the control scores on (C1, C2,
EU1 all 0/10). This is a specialist temperament: when the task shape matches,
it is near-frontier; when it does not, it does not degrade gracefully — it
collapses (see taxonomy below). Qwen's curve is flatter in both directions.

## Failure taxonomy: one tic, three roads (7 invalid_action of 180)

Every hard `invalid_action` termination is the same terminal behavior — a tool
called with **literal null arguments**, repeated after an explicit validation
error — reached by three distinct roads:

1. **Capability-gap refusal** (P3, 3/10): the cell's fallback leg exceeds the
   action catalog by design (gap cartography). The model explores the right
   direction (`ft8_heard_stations`), cannot serialize what it wants, and
   null-saves rather than approximating. Reproduced at ELMER_MAX_TOKENS=3000
   AND 8000 (captest A/B) — model behavior, not output-cap truncation.
   Identical 12-turn trajectory across three independent conversations.
2. **Schema-composition breakdown** (S1, 1/10): in-catalog task; model
   submits 96-252-char skeletal `routines_save` fragments, patching one
   validator complaint at a time (a real save is ~1.7-2.9k chars), then
   degenerates to null — after re-reading the definition template.
3. **Non-save tools too** (C2, 3/10): `predict_path` called with null — the
   tic is tool-general, not save-specific.

The remaining 71 FAILs are judged failures of *saved* work: control-flow
defects (missing branch guards, unconditional legs — the P3 attempt-1 pattern:
green-but-wrong), dropped requirements, and wrong-action substitutions.

Follow-up already filed: `tuxlink-bohfp` — an Inkling-specific skill-arm
(prompt-side policy: never null-call; on validation failure recompose complete;
approximate-and-log on capability gaps) after the multi-model baseline set,
with the ladder expansion (tuxlink-69qtv) as its regression instrument. If the
prompt arm moves the tic, Tinker-hosted LoRA (~$50-150, 12B-active billing) is
the escalation.

## Energy: first wall-metered arm (partial coverage)

Shelly plug logging began mid-run, covering 12.5 of 14.0 hours:
**3.82 kWh measured, ~4.3 kWh extrapolated for the full run** — cluster
average ~308 W (b70: 160 W avg, 4cc: 148 W avg), i.e. **~24 Wh per battery
attempt**. First full-coverage arm will be q235 (tuxlink-d8868 tracks report
integration). For the field thesis: a dual-Spark inference station under
sustained agentic load is a ~300 W appliance.

## Caveats

- **Serial serving.** conc=1 / max_num_seqs=1 is a stability requirement on
  this hardware, not a choice; it makes wall-clock (14h) incomparable to other
  arms but does not touch verdict validity.
- **ELMER_MAX_TOKENS=3000** decode cap (GLM-5.2 precedent). The captest A/B
  showed the signature failure is cap-independent.
- Six quarantined invalid runs precede this one
  (`inkling1-invalid*` on R2); none contributed data. The seventh quarantine
  (invalid6) was an orphaned-supervisor incident, also excluded.
- Judge is sonnet-5 with fingerprint-verified join, same as all prior arms.

## Ledger

- Bundles: `r2-poe:~/6i8jz-run/battery-results/inkling1/` (+ `inkling1_joined.json`)
- Joined data: `dev/battery/comparison/data/inkling1_joined.json`
- Comparison artifact: `dev/battery/comparison/battery-comparison.html` (5 runs)
- Serving runbook: `dev/runbooks/inkling-dual-spark/`
- Forensics narrative: `bd show tuxlink-fa6x4`
- Follow-ups: tuxlink-bohfp (skill arm), tuxlink-69qtv (ladder expansion),
  tuxlink-d8868 (energy in reports), tuxlink-ulzuv (harness dead-endpoint
  bundles)
