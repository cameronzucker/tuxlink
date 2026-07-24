# Instrumented battery run: raw data record (2026-07-24)

Author: tanager-owl-cardinal.

**This is a DATA record, not a findings doc.** Every per-cell verdict below is a
**single observation from one run**. Single data points are only vaguely
informative; they are NOT findings and NO cross-arm, cross-model, or
skill-vs-base conclusion may be drawn from them. Findings require the reliability
re-runs (multiple attempts per cell for a rate), which are in progress and NOT
yet reflected here. Do not cite anything in this file as a conclusion.

## Provenance

- **Runs** (on R2, `~/tuxlink-eig6e-build/battery-results/`):
  - `qwen-instrumented-1`: qwen-3.5-122b-nvfp4, local Spark vLLM (keyless),
    `https://inference.twin-bramble.ts.net`. base + skill arms, all 18 cells. COMPLETE.
  - `glm52-3`: GLM-5.2 (`z-ai/glm-5.2`) via OpenRouter. base + skill. **15 of 36
    cells ran; the other 21 hit HTTP 402 (OpenRouter out of credit) and did NOT run.**
    The 15 that ran are NOT yet predicate-judged. Blocked pending a credit top-up.
- **Binary**: origin/main `0ae53b5e` + this session's harness patches (below).
- **Corpus**: `tests/battery/corpus.json`, 18 cells (P1-3, S1-4, A1-2, C1-3, E1-3, EU1-3),
  temperature 0.2, turn-cap 40.
- **GLM env knobs** (this session's fixes): `OPENROUTER_PROVIDER_ORDER=streamlake,z-ai`,
  `ELMER_MAX_TOKENS=32000`, key via `secret-tool lookup service elmer-openrouter account teacher`.

## Grading note

`completed` = the agent reached a terminal (not a correctness claim). `saved`/`green`
= a routine was saved and the validator found no error (warnings OK). **Neither
means the routine does what the prompt asked.** The verdict column below is a
per-predicate judgment against the cell's prompt; `green` routines can be judged
FAIL when they silently drop requirements.

## qwen-3.5-122b, `qwen-instrumented-1`, ONE run (single observations)

Verdict = judgment against the prompt's requirements this one run. `sv`=saved,
`gr`=validates-green.

| cell | arm | outcome | sv/gr | routine (summary) | verdict (1 obs) | note |
|---|---|---|---|---|---|---|
| P1 | base | completed | y/y | find→connect→branch→log-winner, 30m sched | PASS | walk, winner logged, schedule |
| P1 | skill | completed | y/y | find(5,40m)→connect→branch→log-winner, 30m | PASS | same |
| P2 | base | completed | y/n | find→connect→branch, hourly, automatic | PASS-struct | not-green = AUTO_TX (automatic) |
| P2 | skill | completed | y/y | find(3,20m)→connect→branch, hourly attended | PASS | |
| P3 | base | completed | y/n | wwv→find→connect + 2nd find→connect→aprs | PARTIAL | 2nd find substituted for FT-8 prediction |
| P3 | skill | completed | y/n | wwv→find→connect→aprs (one leg) | PARTIAL | FT-8 prediction fallback leg absent |
| S1 | base | completed | y/y | find→connect→branch→log/aprs on fail | PASS | |
| S1 | skill | **cancelled** | y/y | (turn-capped at 40) | FAIL | turn-cap |
| S2 | base | completed | y/y | connect(40/80)→log, 15m | PASS | edit |
| S2 | skill | completed | y/y | connect(40/80)→log band+station, 15m | PASS | edit |
| S3 | base | **invalid_action** | n/n | (none) | FAIL | built nothing |
| S3 | skill | completed | n/n | (none) | FAIL | built nothing (bail) |
| S4 | base | completed | y/n | preset→atu→read→compose→find→connect→aprs, manual | PARTIAL | has preset+ATU+compose; trigger manual not daily |
| S4 | skill | completed | y/y | find→connect→compose→connect→aprs, daily | PARTIAL | omits preset+ATU; compose after connect |
| A1 | base | completed | y/y | spacewx_swpc→branch→aprs OK/FAIL, manual | PARTIAL | space wx, not local wx |
| A1 | skill | completed | y/y | spacewx_swpc→branch→aprs OK/FAIL, manual | PARTIAL | space wx, not local wx |
| A2 | base | completed | y/y | find→connect→branch→compose, 4h sched | PARTIAL | compose present; "best"=distance |
| A2 | skill | completed | y/y | find→connect→branch→logs, 4h sched | PARTIAL | no explicit send; "best"=distance |
| C1 | base | completed | y/y | spacewx_swpc→branch→notify/compose, manual | PARTIAL | weather+alert; manual |
| C1 | skill | completed | n/n | (none) | FAIL | built nothing |
| C2 | base | completed | y/y | find→connect→branch→log, manual | FAIL | "recurring" asked; trigger manual; no send/receive |
| C2 | skill | completed | y/y | find→connect→branch→log, manual | FAIL | manual; test only |
| C3 | base | completed | y/y | find→connect→branch→log, manual | PARTIAL | manual; no time-of-day band logic |
| C3 | skill | completed | y/y | find(3band)→connect→branch→log, manual | PARTIAL | manual; no time-of-day |
| E1 | base | completed | y/y | find→connect→branch→read→delay→retry, 1h | PARTIAL | retry/delay/sched; FT-701 unaddressed |
| E1 | skill | **cancelled** | y/y | (turn-capped at 40) | FAIL | turn-cap |
| E2 | base | **cancelled** | y/y | find→connect→branch→read (turn-capped) | FAIL | turn-cap |
| E2 | skill | completed | y/y | spacewx_swpc→find→connect→branch, 8h | PARTIAL | has prediction check; "send" = bare connect |
| E3 | base | completed | y/y | find→connect→compose(W1AW), manual | PARTIAL | manual, not "regular basis" |
| E3 | skill | completed | y/y | find→connect→compose(W1AW), manual | PARTIAL | manual, not regular |
| EU1 | base | completed | y/y | find→connect→branch→log, manual | FAIL | manual; no send; no setup |
| EU1 | skill | completed | y/y | read configs→find→connect→logs, manual | FAIL | manual; no send |
| EU2 | base | completed | n/n | (none) | FAIL | built nothing |
| EU2 | skill | completed | y/y | connect(hardcoded N0RNG)→branch→log, manual | FAIL | no find; no password; no send; no image/mode |
| EU3 | base | completed | n/n | (none) | FAIL | built nothing (diagnostic-help prompt) |
| EU3 | skill | completed | n/n | (none) | FAIL | built nothing (diagnostic-help prompt) |

## GLM-5.2, `glm52-3`: NOT yet predicate-judged

15/36 cells ran (P1-3, S1-4, A1 both arms) and each saved a routine; 21 cells
(A2, C*, E*, EU*) did NOT run (HTTP 402, out of credit). The 15 are recorded on
R2 but have NOT been judged against predicates and are omitted here to avoid a
`sg`-not-judged repeat. Judge after a credit top-up completes the set.

## Reliability re-runs (in progress, NOT yet recorded)

Sweep `qwen-reliability` launched: cells `base:S1 base:S3 base:E2 skill:S1
skill:E1 skill:S3`, N=5 attempts each, into attempt-indexed dirs. Rates go here
when complete. Until then, the single observations above are not findings.

Prior-run cross-check available for P1-S4 only: `lift-corrected-1` is a second
qwen observation of those cells (a different day, same corpus). Two observations
is still not a reliability rate; the sweep is the rate.

## Committed harness fixes this session (durable code facts, not single-obs)

All experimentally isolated and compile-verified on R2:
- `30cd7608` battery captures reasoning/streaming deltas (`deltas.jsonl`).
- `97e57c9c` opt-in `OPENROUTER_PROVIDER_ORDER` (GLM tool-call XML leak was
  OpenRouter provider-routing variance; pin a clean-JSON provider).
- `5f241cc5` opt-in `ELMER_MAX_TOKENS` (omitted budget let a provider's ~4096
  default cap truncate GLM's reasoning before it emitted a tool call).
- ADR 0025 amendment corrected + `dev/battery/2026-07-24-lift-corrected-1-failure-modalities.md`
  (the prior run's modality analysis + two-front convergence).
