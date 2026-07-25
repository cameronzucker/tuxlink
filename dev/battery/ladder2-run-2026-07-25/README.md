# Ladder-2 raw exports (run of 2026-07-25)

Distilled artifacts from the completed Ladder-2 run, committed because the source
tree lives only on R2 at `~/tuxlink-eig6e-build/battery-results/ladder2/`, which
is gitignored and on a machine that can be rebuilt. Analysis of these files is in
[`../2026-07-25-ladder2-results-and-failure-analysis.md`](../2026-07-25-ladder2-results-and-failure-analysis.md).

**Read the validity section of that analysis before using this data.** 50 of the
220 bundles are wall-clock truncations caused by a mid-run serving change, and
the base-vs-skill arm comparison is confounded as a result.

| file | rows | what it is |
|---|---|---|
| `judgments.jsonl` | 220 | Sonnet-5 predicate judge verdicts. `{id, overall, per_predicate[{predicate, verdict, why}], note, judge, judged_at}`. The per-predicate `why` text is the source for the agent-side failure taxonomy. |
| `outcomes.jsonl` | 220 | Per-bundle harness outcome: `outcome`, `cancel_reason`, `detail`, `duration_secs`, `provider_turns`, `tool_calls`, `prompt_tokens`, `eval_tokens`, plus `bundle`. |
| `tool_call_errors.jsonl` | 266 | **Only the non-ok tool calls**, `result_preview` stripped. Source for the Tuxlink-side taxonomy. `status` is `invalid_args` or `denied`; `detail` carries the message the model actually saw. |
| `tool_call_summary.jsonl` | 219 | Per-bundle call counts plus the ordered `(tool, status)` sequence. Enough to reconstruct retry loops without the 6.8 MB of full payloads. |
| `saved_defs.jsonl` | 219 | The routine definitions the model actually saved. The work products. |
| `manifest.jsonl` | - | Append-only driver checkpoint, one line per unit with `det_fail`. |
| `latency.jsonl` | 143 | Per-unit wall clock and outcome under the parallel driver, tagged with the concurrency width. Written only after the parallel driver was introduced, so it does not cover the serial era. |
| `run.log` | - | Driver progress markers for both the serial and parallel phases. Contains two `LADDER2 START` lines (a relaunch) and the final `LADDER2-PAR COMPLETE`. |

## Bundle id format

`<arm>/<cell>/<condition>/<attempt>`, e.g. `skill/E1/rev_on/attempt-3`.

- arm: `base` (raw prompt) or `skill` (Build-Carefully scaffold)
- condition: `none` (the raw build), `rev_off`, `rev_on` (Nemotron review then a
  qwen revise, reviewer reasoning off vs on)
- attempt: `attempt-1..3`; attempts 2 and 3 exist only where attempt 1 was a
  deterministic failure, so **attempts are not independent samples** and a
  failing condition contributes correlated draws. Use best-of-attempts per
  condition for verdict comparisons.

On disk the `none` condition lives under `build/`, not `none/`. The exports
normalise it to `none` in the `bundle` field.

## Not exported

Full `tool_calls.jsonl` (6.8 MB), `transcript/*.jsonl`, `deltas.jsonl`,
`scores.jsonl`, and the Nemotron `critique.txt` / `critique.meta` files remain on
R2 only. Pull them from there if a deeper trace is needed; they were left out to
keep the repo lean, not because they are uninteresting. The critiques in
particular are the input to any future analysis of reviewer quality.
