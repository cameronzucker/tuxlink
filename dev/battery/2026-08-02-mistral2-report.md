# mistral2: uncensored context does not rescue Mistral Small 4. New floor at 13.9 percent.

Run: 2026-08-02 06:10 to 08:06 AZT, 180/180 attempts in 2h01m (fastest arm) at
concurrency 8 on one continuous engine session (eugr TP2, zero restarts).
180/180 judged (sonnet-5), join clean (0 stale, 0 missing). Agent
chasm-wren-crag.

Model: `mistralai/Mistral-Small-4-119B-2603-NVFP4` (119B MoE, mistral-native
weights), served with the mistral tokenizer/config/load trio and the mistral
tool parser at 262k context, temp 0.15 (vendor recommendation, matching
mistral1). Generation `40fd9b7e`, same judge and harness as control2, inkling1,
and q235. This is the mistral1 rerun the queue owed: mistral1 ran 42 percent
context-censored at a 32k host cap; this arm removes that ceiling entirely.

## Topline: last place on both metrics

| | PASS | PARTIAL | FAIL | strict | lenient (P+0.5&middot;PT) |
|---|---|---|---|---|---|
| inkling1 (276B-A12B) | 64 | 38 | 78 | **35.6%** | 46.1% |
| control2 (Qwen3.5 122B) | 56 | 71 | 53 | 31.1% | **50.8%** |
| q235 (Qwen3 235B-A22B) | 39 | 64 | 76 | 21.8% | 39.7% |
| **mistral2 (Mistral Small 4 119B)** | 25 | 57 | 98 | **13.9%** | 29.7% |

Answering the question this arm existed to ask: no, the 32k ceiling was not
what held Mistral back. mistral1 passed 17.5 percent of its NON-censored
attempts; mistral2 with the ceiling removed passes 13.9 percent overall
(14.8 percent even excluding the encoder-killed attempts below). Harness
generations differ (mistral1 ran pre-idfix), so the comparison is
directional, but the direction is flat-to-down, not up. In this size class
and price band, Qwen3.5 122B does more than twice mistral2's strict rate on
identical tasks.

## Per-cell profile

| cell | PASS | PARTIAL | FAIL | | cell | PASS | PARTIAL | FAIL |
|---|---|---|---|---|---|---|---|---|
| P1 | 5 | 4 | 1 | | C1 | 0 | 0 | 10 |
| P2 | 3 | 4 | 3 | | C2 | 0 | 3 | 7 |
| P3 | 0 | 4 | 6 | | C3 | 0 | 6 | 4 |
| S1 | 1 | 3 | 6 | | E1 | 1 | 7 | 2 |
| S2 | 0 | 3 | 7 | | E2 | 0 | 2 | 8 |
| S3 | 0 | 6 | 4 | | E3 | 0 | 3 | 7 |
| S4 | 4 | 3 | 3 | | EU1 | 0 | 0 | 10 |
| A1 | 0 | 1 | 9 | | EU2 | 0 | 2 | 8 |
| A2 | 1 | 6 | 3 | | EU3 | 10 | 0 | 0 |

EU3 is 10/10 — every model in the set saturates the pure-troubleshooting
cell, which keeps flagging it as the easiest tier (ladder-expansion epic
tuxlink-69qtv). Everything else erodes: even P1, which every other arm
passes 9-10/10, drops to 5/10. C1 and EU1 are 0-for-10, consistent with the
set-wide graveyard cells.

## The strict-encoder tax: 11 attempts killed by HTTP 400 (tuxlink-5uwnj)

New failure class for the baseline set: 11 attempts ended `provider_error`
on hard 400s from the mistral serving path, two signatures:

1. **Dotted function names** (8x): the model hallucinates catalog ACTION
   names (`data.read`, `data.spacewx_swpc`, `local.set_identity`,
   `local.log`) as tool-call function names. The strict tekken encoder
   rejects dots when re-encoding the conversation on the next request, so
   the attempt dies one turn after the hallucination. Hermes-family stacks
   bounce the same mistake back as a soft validation error, and models
   routinely recover; here there is no second chance.
2. **Unpaired tool calls/responses** (3x, all E2): "Not the same number of
   function calls and responses" — the encoder requires strict pairing;
   some harness denial/error paths violate it.

All 11 judged FAIL (saved work absent or partial), which is correct: the
trigger is the model's own malformed call. But the *severity* is
stack-specific, and the fix (client-side pre-send repair + local name
validation) is Elmer-relevant for any real Mistral deployment. Filed as
`tuxlink-5uwnj`. Engine continuity is unaffected: zero restarts, the 400s
are per-request.

## Energy

Full wall-metering, second full-coverage arm:

- **0.618 kWh total** (head 0.322, worker 0.296) over 2h01m
- Cluster average **308 W**, peak ~398 W
- **~3.4 Wh per attempt** — cheapest arm yet (q235: 4.6, inkling1 serial: ~24)

## Caveats

- Two burn-in false starts quarantined (`mistral2-burnin-invalid*-nokey`):
  `OPENROUTER_API_KEY` missing from the launch environment, required by the
  binary even for local endpoints. No data contribution.
- temp 0.15 per vendor recommendation vs 0.2 on Qwen-family arms;
  cross-model comparisons remain directional.
- 14 cancelled (turn-cap/wall-clock), between control2's 4 and q235's 20.

## Ledger

- Bundles: `r2-poe:~/6i8jz-run/battery-results/mistral2/` (+ `mistral2_joined.json`)
- Joined data: `dev/battery/comparison/data/mistral2_joined.json`
- Comparison artifact: `dev/battery/comparison/battery-comparison.html` (7 runs)
- Serving recipe: `inference:~/spark-vllm-docker/recipes/mistral-small-4-119b-nvfp4.yaml`
- Arm issue: `bd show tuxlink-nuke4`; encoder bug: `tuxlink-5uwnj`
- Next arm: gptoss (tuxlink-2mwoz — re-check the unservable bug first)
