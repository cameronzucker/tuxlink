# Ladder-2 results and failure analysis (run of 2026-07-25)

Author: condor-basalt-hemlock. Records the completed Ladder-2 run, what is and is
not validly reportable from it, and a failure taxonomy separating agent-side
defects from Tuxlink-side defects. Raw exports live beside this file in
[`ladder2-run-2026-07-25/`](ladder2-run-2026-07-25/).

Companion: [`2026-07-25-parallelization-analysis.md`](2026-07-25-parallelization-analysis.md)
(throughput, serving config, cost model). Runbook:
[`2026-07-24-ladder2-runbook.md`](2026-07-24-ladder2-runbook.md).

## What was run

Factorial over the 18-cell corpus: builder `qwen35-122b-nvfp4` on the local
Spark, arms `base` (raw prompt) and `skill` (Build-Carefully scaffold), review
conditions `none` / `rev_off` / `rev_on` (Nemotron-3-super-120b via OpenRouter
pinned to Nebius fp4, reasoning off vs on). Deterministic re-runs to N=3 on
deterministic failure. Grading is two-layer: `elmer_score` deterministic plus a
Sonnet-5 predicate judge.

- 108 conditions, **220 scored bundles**, 220 judged, 2,881 tool calls.
- Completion marker `LADDER2-PAR COMPLETE` at 2026-07-25T18:07:16Z.
- Nemotron spend for the run: ~$0.90 (see the cost model in the companion doc).

## VALIDITY: part of this dataset is contaminated, by my change

Mid-run I raised the vLLM admission cap (`--max-num-seqs` 2 to 8) and the driver
width to 8. That was a large aggregate-throughput win and a large per-bundle
latency loss, and **the harness deadline is wall-clock**:

| | before the switch | after |
|---|---|---|
| `needs_operator` | 2 / 83 (**2%**) | 48 / 137 (**35%**) |
| median bundle wall clock | 264 s | **1,262 s** |
| p90 bundle wall clock | 900 s | 1,868 s |

All 50 `needs_operator` outcomes are deadline hits, not model behaviour: 31 hit
the 1800 s total budget and 19 hit the 600 s per-turn timeout. They average
**14.5 provider turns**, against 40 for genuine runaways (which surface as
`cancelled` at the 40/40 turn cap). A truncated run is not a capability signal.

**The single decisive distinction: latency alone does not invalidate a bundle;
only truncation does.** The model emits the same tokens whether served fast or
slow. Wall clock matters only because a deadline cuts the run off. So bundles
that completed are valid regardless of which regime they ran in.

### What that leaves

| claim | status |
|---|---|
| 89 of 108 conditions with a clean best attempt | **valid** |
| skill-arm review comparison (`none` vs `rev_off` vs `rev_on`) | **valid**, bias applied evenly (timeouts 34% / 38% / 29%) |
| every Tuxlink-side tool-call finding | **valid**, latency-independent |
| base-vs-skill arm comparison | **INVALID**, confounded with regime (base 13% vs skill 34% timeouts; base ran 72% pre-switch, skill 100% post) |
| base-arm review comparison | **INVALID**, its `none` column is flattered (2% vs 22% / 16%) |
| absolute pass rates in the skill arm | depressed by truncation; ordering holds, magnitude does not |

Process lesson: this failure mode was predicted in the companion doc as
"Blocker 2" before the change was made. The canary chosen was queue time, which
stayed benign at 0.25 s. The correct canary was **per-bundle wall clock against
the 1800 s budget**, which was instrumented in `latency.jsonl` and not watched.
Instrument the thing the deadline actually measures.

## Results (valid subset: skill arm, best-of-attempts)

| condition | n | PASS | PARTIAL | FAIL | pass | fail |
|---|---|---|---|---|---|---|
| `none` | 18 | 7 | 4 | 7 | 39% | 39% |
| `rev_off` | 18 | 7 | 8 | 3 | 39% | 17% |
| `rev_on` | 18 | 5 | 10 | 3 | 28% | 17% |

Three findings, in descending confidence:

1. **Adversarial review converts FAIL into PARTIAL but does not create PASS.**
   Fail rate more than halves (39% to 17%); pass rate is unchanged. Review
   reliably rescues broken routines and reliably fails to finish them. Six cells
   moved FAIL to PARTIAL (A1, C1, C2, E2, EU1, E3).
2. **Reasoning-ON is worse than reasoning-OFF.** 28% vs 39% pass at identical
   fail rate. Three cells regressed specifically under reasoning-on (S2, S4,
   EU3). This reproduces the direction seen in the partial mid-run data.
3. **EU3 was actively broken by review**: PASS at `none`, FAIL at both review
   conditions. EU3 is corpus-marked as a cell where no routine is expected, so a
   reviewer that assumes a routine should exist pushes the model off a correct
   refusal. Reviewing a decline-to-act cell needs different instructions.

Deterministic-vs-judge disagreement, all 220 bundles: 127 were `routine_saved`
AND `validates_green`, of which the judge passed only 28. **99 of 127
green-but-incomplete (78%).** This is the direct justification for keeping an
LLM predicate judge alongside the deterministic scorer.

## Failure taxonomy

The split is structural: **agent failures emit syntactically valid tool calls
with wrong semantics, so they are invisible in the tool log. Tuxlink failures are
exactly what the tool log captures.**

### Agent-side (from the saved defs, via judge per-predicate reasoning)

| mode | bundles | concentrated in |
|---|---|---|
| **branch polarity inverted** | 22 | S1, S3, A1 |
| orphan / duplicate routine introduced on revise | 22 | P2, S3 |
| required leg absent | 17 | A1 (7), EU1 (6) |
| `final_text` contradicts the saved def | 17 | P2 |
| schedule dropped to manual | 15 | **S4 (8)** |
| runaway loop, hit the 40/40 turn cap | 18 | (all `cancelled`) |

Branch polarity is the dominant genuine defect: the success action is wired into
the failure arm. S1 and S3 both produced routines that saved green, validated
clean, and did the opposite of the request. Deterministic validation cannot see
this, which is the second argument for the predicate judge.

### Tuxlink-side (2,881 tool calls, 266 non-ok = 9.2%)

| defect | calls | bundles | model recovers |
|---|---|---|---|
| `routines_get` returns an error on lookup miss | 46 | 46 | 78% |
| `KIND_CHANGE_REJECTED` on action/control patch | 33 | 30 | yes, via remove + re-add |
| `DUPLICATE_STEP_ID` | 24 | 19 | 54% |
| **untagged-enum "malformed" on `routines_save`** | 15 | 4 | **13%** |
| placement required | 12 | 12 | 92% |
| `ID_CHANGE_REJECTED` | 4 | 4 | n/a |

**Error-message actionability, not error frequency, determines cost.** Two
comparable-volume defects behaved completely differently:

- `KIND_CHANGE_REJECTED` says *"patch may not turn an action step into a control
  step (or back) — remove and re-add instead."* The model reads it and does
  exactly that (observed in `skill/EU1/build/attempt-2`, s18 reject then
  s19-s23 add/remove/add/remove). Cost ~3 turns.
- The untagged-enum error says *"data did not match any variant of untagged enum
  Step at line 1 column 523."* In `skill/E1/rev_on/attempt-3` the model called
  `routines_save` **nine consecutive times**, received the identical message
  each time, learned nothing, and the run died at `needs_operator`.

The serde `#[serde(untagged)]` diagnostic leaks an internal that names neither
the offending step nor the field, so there is nothing to act on.

### Harness-scope artifact (not a product bug, but it invalidates the EU cells)

119 calls denied by `harness_allowlist`, **79 of them in EU1 / EU2 / EU3**. Every
EU run opens by probing `vara_status`, `config_get_rig`, `config_get_ardop`,
`config_get_vara`, `rig_status`, `backend_status`, all denied, then falls back to
`docs_search` / `docs_read`. Up to 6 wasted turns before any authoring call; 46
turns lost across 17 bundles to leading-denial runs alone.

The model is behaving sensibly: reading config before authoring a VARA setup
routine is correct instinct. The harness forbids it. **EU-cell results therefore
measure "author blind", not "author", and should not be read as capability.**

Open product question, not a defect: should routine authoring expose read-only
state introspection? If production Elmer has these tools and the battery does
not, the battery is measuring a handicapped agent.

## Addressable, ranked

1. **Untagged-enum error message.** Highest severity per incident; causes
   unrecoverable retry loops. Tagged enum, or a custom deserializer naming the
   offending step id and field.
2. **`routines_get` on miss.** 46 bundles, one each. Return empty rather than an
   error; the model is doing a reasonable existence check.
3. **Wall-clock `max_response_duration`.** Empirically the top harness defect at
   50 lost bundles. `--turn-timeout-secs` already exists as a CLI flag and the
   driver simply never passes it, which covers 19 of the 50 with no code change.
   The other 31 need `max_response_duration` to become settable. Add an explicit
   `--max-run-secs` flag rather than raising `Limits::default()`, which is shared
   with production Elmer via `session.rs`.
4. **Server-assigned step ids.** Removes `DUPLICATE_STEP_ID` (19 bundles).
5. **EU-cell introspection.** Product decision, see above.

## Re-run scope (not yet executed)

19 of 108 conditions have a contaminated best attempt; 89 are clean and do not
need re-running. Affected cells: E1 (4), E2 (2), P3 (2), EU1 (2), then A1, P1,
EU2, EU3, S3, S1, A2, C2, E3 with one each. That is roughly 35 bundles, about 30
to 60 minutes at width 8 with the deadlines lifted.

Caveat: 19 is the scope for **best-attempt verdicts**. Trustworthy determinism
RATES need a larger set, since any condition with a timeout among its non-best
attempts has a distorted rate. Decide which is being reported before sizing.

Do the EU-cell allowlist decision first. Re-running EU1 / EU2 / EU3 under the
same denial regime reproduces "author blind" at greater expense.
