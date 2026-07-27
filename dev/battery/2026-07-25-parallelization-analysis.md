# Ladder parallelization and cost analysis (2026-07-25)

Author: condor-basalt-hemlock. Measured against the live Ladder-2 run. Two
subjects:

1. **Throughput.** Why the ~14h serial wall clock was a driver property rather
   than a hardware one, what was measured at each width, and the serving-config
   change that took prefill from 786 to 1,852 tok/s for free.
2. **Cost.** A measured price model for the current hybrid and for fully-hosted
   runs, the builder-vs-reviewer economics, and what N-way parallel construction
   with convergence would actually cost. See
   [Cost model (measured 2026-07-25)](#cost-model-measured-2026-07-25).

Headline results: local admission cap `--max-num-seqs 2` was the binding
constraint and lifting it to 8 is worth **2.36x**; a full ladder run costs
**~$0.90** today (reviewer only, qwen local) or **$14 to $26** fully hosted; and
the builder outcosts the reviewer **25x to 61x on identical work** because it is
an agentic loop rather than a single shot.

Measurements are read-only against the live run except where explicitly noted
(the `--max-num-seqs` change required a vLLM restart, done with the driver
stopped so no cell was disturbed).

## The bottleneck is the driver, not the model endpoint

`ladder2.sh` is a strictly serial double loop: one `elmer_battery` at a time,
`for skill in $SKILLS; for cell in $CELLS`. Nothing about the workload requires
that. The 36 `(skill, cell)` pairs are fully independent; within a pair,
`build -> review -> revise` is the only real dependency, and `rev_off` / `rev_on`
fan out from the same shared build. The 3 determinism attempts are independent
samples, not a sequence, so they parallelize too.

Structural concurrency available: 36 wide during build, 72 during revise, more
with attempt-level fan-out.

## Evidence the local Spark is idle

Sampled from the vLLM endpoint's own `/metrics` while the serial run was mid-cell
(zero added load):

| metric | value | reading |
|---|---|---|
| `num_requests_running` | 1.0 | one sequence in the batch, ever |
| `num_requests_waiting` | 0.0 | nothing queued |
| `request_queue_time_seconds` mean | 0.01 s | no contention at all |
| `kv_cache_usage_perc` | 1.4 - 2.3 % | KV is nowhere near the limit |
| `num_preemptions_total` | 0 | no memory pressure |
| `iteration_tokens_total` p50 | 1 token | decode runs at batch size 1 |
| `time_to_first_token` | mean 14.3 s, p50 <=20 s, p99 <=80 s | prefill dominates latency |
| `request_time_per_output_token` | 0.07 s (~14 tok/s) | bandwidth-bound decode |

The engine spends nearly all its time decoding a single sequence one token per
step. vLLM does continuous batching, so N concurrent sequences would decode N
tokens per step at close to the same step latency. Decode should scale near
linearly. Prefill is the uncertainty: the workload is prompt-dominated (see
below), and concurrent prefills contend for compute in a way decode does not.

### The endpoint also has duty-cycle idle, not just batch headroom

A 120 s counter delta taken during the live serial run:

- prefill: 94,330 tokens => **786 tok/s**
- decode: 846 tokens => **7.0 tok/s**
- at the closing instant: `num_requests_running = 0`, `kv_cache_usage = 0.00%`

Two things follow. First, sustained decode (7 tok/s) is about half the
actively-decoding rate implied by TPOT (0.07 s/token => ~14 tok/s), so the engine
is only generating about half the time. Second, it was caught **completely idle**
at the sample instant. The serial driver leaves real dead time between turns
while `elmer_battery` runs tools, scores, and writes files locally, and the GPU
does nothing during it.

So there are two independent sources of headroom, not one: unused batch width
*and* unused duty cycle. Concurrent cells fill the gaps as well as widening the
batch, which is why a modest local width is worth trying before paying for
Option B.

## The workload is overwhelmingly prefill

Measured across 76 real bundles from the live run:

- prompt tokens: 34,676,653
- eval tokens: 144,381
- **ratio 240 : 1**
- mean per bundle: 456,272 prompt / 1,900 eval
- heaviest bundle: 2,359,727 prompt tokens (A1, `needs_operator`)

This is an agentic loop resending a growing context every turn. It shapes every
conclusion below: cost is set by input tokens, and latency is set by prefill.

## Option A: modest local parallelism on the Spark

Keep `qwen35-122b-nvfp4` on twin-bramble, widen the driver to 3-6 concurrent
cells.

**Why this is the low-risk option:** it costs nothing, and it sidesteps the
cost-ceiling defect in Option B entirely, because `g31en` already made the
credits baseline non-fatal for non-OpenRouter endpoints. The `$2` per-cell
ceiling is inert for local builds today.

**Predicted gain (WRONG, kept for the record):** given zero queueing, 1.4-2.3% KV,
batch size 1, and the engine caught idle mid-run, a 3-4 way run was expected to
land ~4-5h instead of ~14h, with prefill contention flagged as the one unknown
that could flatten the curve early.

### MEASURED at width 3 (2026-07-25, live run)

That unknown is the whole answer. 180 s counter delta at `LADDER2_CONC=3`:

| | serial baseline | conc=3 | gain |
|---|---|---|---|
| prefill | 786 tok/s | 913 tok/s | **1.16x** |
| decode | 7.0 tok/s | 14.4 tok/s | **2.05x** |

Decode doubled exactly as continuous batching predicts, because the scheduler
holds `num_requests_running = 2`. But decode is ~0.4% of the token volume at
240:1, so it barely moves the wall clock. Prefill gained only 16%.

### Why the 16% does NOT prove saturation (corrected)

An earlier revision of this doc concluded prefill compute was saturated, citing
`nvidia-smi` reporting 96% GPU utilization. **That conclusion was wrong and is
retracted.** Operator correction: this box reads ~96% whenever it is serving any
model at all. `utilization.gpu` is the fraction of time at least one kernel is
resident, not the fraction of compute capacity consumed, so a memory-stalled or
under-occupied kernel pins it high. It cannot distinguish saturation from idle
capacity and is not evidence either way.

The real constraint is a hand-set flag. The server on `inference`
(tailnet alias; true hostname `gx10-65aa`) runs:

```
vllm serve nvidia/Qwen3.5-122B-A10B-NVFP4
  --max-num-seqs 2          <-- vLLM's default is 256
  --max-model-len 262144
  --gpu-memory-utilization 0.90
  --tensor-parallel-size 1
```

`--max-num-seqs 2` is what produces `waiting_by_reason = capacity` while KV sits
at 2.3% and preemptions are 0. The scheduler refuses a third sequence because it
was configured to, not because it lacks memory or compute.

**So the 1.16x was measured under a cap of 2, and the scaling curve above 2 is
untested.** With only two slots, each cell alternates prefill and decode, so two
prefills overlap only part of the time; limited overlap opportunity is at least
as good an explanation for the 16% as compute saturation. The honest position is
that we do not yet know which.

Net expected wall clock at the current cap: roughly **12h instead of 14h**.

### RESOLVED: the cap was the constraint, and lifting it is worth 2.36x

Raised `--max-num-seqs` from 2 to 8 and re-measured over 180 s at
`LADDER2_CONC=8`:

| | serial (seqs=2, conc=1) | conc=3 (seqs=2) | **conc=8, seqs=8** |
|---|---|---|---|
| prefill | 786 tok/s | 913 tok/s | **1,852 tok/s** |
| decode | 7.0 tok/s | 14.4 tok/s | 7.8 tok/s |

**Prefill: 2.36x serial, 2.03x the width-3 run.** Since the workload is 240:1
prefill-dominated, prefill throughput is what sets the wall clock, so this is a
real ~2.4x on the ladder. It was free: one flag, no code change, no money.

The scheduler now admits what it was refusing before: `num_requests_running` went
5 then 7 (versus a hard 2), `num_requests_waiting` fell 3 -> 1, KV rose to only
9.3%, and **`num_preemptions_total` stayed at 0**. So the canary never tripped;
there was simply headroom being withheld by configuration.

Sizing context from the startup log: `GPU KV cache size: 672,336 tokens` and
`Maximum concurrency for 262,144 tokens per request: 9.79x`. Admission was pinned
at 2 against a memory ceiling of ~9.79, roughly 5x tighter than the hardware
required. (The 9.79 exceeds the naive 672,336/262,144 = 2.6 because this model
uses hybrid attention; the startup log shows a GDN linear-attention prefill
kernel, so only some layers carry full KV.)

Decode fell versus width 3 (14.4 -> 7.8 tok/s). That is expected rather than a
regression: with 8 cells in flight, a larger share of engine steps are spent on
prefill, and prefill competes with decode for the same compute. At 240:1 the
trade is strongly favourable.

Scaling is sub-linear, which bounds further tuning: 4x the admission (2 -> 8)
bought 2.03x the prefill. Compute contention is now doing real work, so a further
raise to 12-16 would likely yield well under proportional gains. The memory
ceiling would allow it (real per-turn contexts are ~45-65k, not 262k, so the
672k-token pool supports roughly 13 concurrent at realistic lengths), but each
change costs a ~13 min model reload. Not obviously worth it; revisit only if the
prefill curve is wanted for its own sake.

### The lever, as originally identified

vLLM's own guidance is that `max_num_seqs x max_model_len` must fit the KV
budget. At the configured 262144 that worst case is ~524k tokens for 2
sequences. But this workload's real per-turn contexts are ~45-65k, not 256k, so
8 sequences at ~50k each (~400k tokens) plausibly fits the same pool. KV at 2.3%
with zero preemptions says there is substantial room.

Next test: raise `--max-num-seqs` to ~8, leave `--max-model-len` alone, re-run
the 180 s throughput delta, and watch `num_preemptions_total` (currently 0) as
the canary. If preemptions climb, back off.

Two vLLM instances behind a reverse proxy is NOT the way to get there on this
box: 119 GB total with 113 GB already in use, and the 122B NVFP4 weights alone
are ~61 GB, so a second copy does not fit. Raising the flag on the single
instance achieves the same concurrency without duplicating weights.

**Caveat before touching it:** changing the flag requires restarting vLLM, which
kills in-flight ladder requests (the driver is idempotent, so units are redone)
AND affects every other consumer of that shared endpoint. Not an agent-side
decision.

Queue time rose from 0.01 s to 0.25 s cumulative mean, and TTFT was essentially
unchanged (14.32 -> 14.60 s cumulative). So width 3 is not inflating the 1800 s
wall-clock budget meaningfully, and is safe to leave running. There is simply
little to gain.

**Conclusion: keep width 3 (it is free and slightly positive), but local
parallelism is not the path to faster iteration on a prefill-dominated
workload.**

**Canaries to watch while ramping:** `request_queue_time_seconds` (currently
~0.01 s) and `time_to_first_token` p90 (currently <=40 s). If queue time climbs
past a few seconds or TTFT p90 roughly doubles, that is saturation. Ramp
3 -> 4 -> 6, measuring at each step. Do not jump straight to wide.

## Option B: everything on OpenRouter with pinned providers

Both models pin cleanly with the `provider.order` + `quantizations` +
`allow_fallbacks: false` pattern `review.py` already uses for Nemotron:

| role | model | provider | quant | ctx | $/M in | $/M out |
|---|---|---|---|---|---|---|
| builder | `qwen/qwen3.5-122b-a10b` | DeepInfra | fp4 | 262,144 | 0.29 | 2.40 |
| builder alt | same | SiliconFlow | fp8 | 262,144 | 0.26 | 2.08 |
| reviewer | `nvidia/nemotron-3-super-120b-a12b` | Nebius | fp4 | 262,144 | 0.30 | 0.90 |

Account has no rate limit (`requests: -1`, not free tier), so the ceiling is
provider capacity rather than OpenRouter.

**Cost: $14 to $26 per complete 108-condition ladder** at DeepInfra fp4. The
spread is mean-vs-median: bundle means (410k build / 476k revise prompt tokens)
are pulled up by outliers such as A1 at 2.36M, while medians are 212k / 233k.
$26 is the mean-weighted figure and $13.87 the median-weighted one. Neither
provider exposes a prompt-cache discount, so every resent turn bills at full list
price. That is the entire cost of the 240:1 ratio. Full derivation and the
measured reviewer costs are in the cost-model section below.

**Speedup:** ~1h wall clock at 12-24 wide, versus ~14h serial. Past roughly 36
the critical path stops shrinking, because it becomes the single slowest chain
(A1: three build attempts that each run to the 30-minute wall).

**Comparability caveat:** OpenRouter reports a generic `fp4`, not specifically
NVFP4. PR #1240 established local <-> API qwen functional parity, but that was a
different endpoint; a spot-check is warranted before trusting a cross-run
comparison.

## Cost model (measured 2026-07-25)

### Method, and why the token estimator is not trusted here

Two ways to price a run: multiply measured tokens by list price, or read the
OpenRouter `/api/v1/credits` account delta. **The credits delta is the meter; the
token estimate is an approximation that has now been observed wrong in both
directions.**

- Estimating Nemotron review calls at ~4 chars/token gave $0.00337 per call.
- The credits delta over the same window gave **$0.00658 per call**, roughly 2x
  higher. Four chars/token is too generous for JSON and routine definitions,
  which tokenize closer to 3 or worse.
- The opposite error is already on record in this project: the in-harness token
  estimate **overshot** the real credits delta 4x on Anthropic models, because
  provider-side prompt caching bills cached input at a fraction of list, and it
  cost-cancelled a healthy cell at $0.52 actual spend (Stage-P2, bd l264r).

So: estimate to plan, meter to decide. The credits-derived numbers below are the
ones to quote.

Caveat on precision: the $0.00658 figure derives from a 5-call window, so treat
the reviewer total as **$0.50 to $1.00**, not a tight number.

### What a run costs today (qwen local, Nemotron on OpenRouter)

| item | value |
|---|---|
| measured cost per Nemotron review call | $0.0066 (credits-derived) |
| review calls per full ladder | ~134 (72 rev conditions x 1.86 attempts observed) |
| **Nemotron cost per full ladder run** | **~$0.90** |
| qwen builds and revises | $0 (local Spark) |

For contrast, moving qwen to OpenRouter as well (Option B) puts a full run at
$14 to $26. The reviewer is therefore about 3 to 6 percent of a fully-hosted
run. All of the money is in the builder.

### Why the reviewer is cheap: it is the loop, not the coverage

The reviewer's low cost is NOT mainly because it runs on fewer arms. Reviews fire
on 2 of 3 conditions (rev_off and rev_on, not the raw build), worth only ~1.5x.
Measured on the **same unit of work** (one rev condition, same arm, same cell):

| | tokens |
|---|---|
| qwen revise bundle | 476,209 (mean); 232,965 (median) |
| Nemotron review call | ~7,800 chars-derived / ~19,000 credits-derived |

**The builder costs 25x to 61x the reviewer on identical work.** The mechanism is
turn count:

```
qwen revise:  17.9 provider turns x 26,574 tokens/turn = 476k
nemotron:      1 turn
```

An agentic loop resends its whole conversation every turn, so turn 18 pays again
for turns 1 through 17. Cumulative prompt tokens grow roughly **quadratically**
in turn count while output stays flat. That is what produces the 240:1
prompt-to-eval ratio, and it is why the absence of a prompt-cache discount hits
this workload harder than almost any other shape: caching is precisely a discount
on the resent prefix.

**Generalizable rule: in an agent system, cost is dominated by loop length, not by
model choice or component count.** Adding a single-shot critic is nearly free.
Letting the builder take five more turns is not.

### Scaling to parallel construction with convergence

Modelling an N-way parallel build requiring convergence, using median bundle
sizes, 36 build conditions and 72 revise conditions at the observed attempt rates
(1.77 and 1.86), priced at DeepInfra fp4 ($0.29/M in, $2.40/M out):

| scenario | tokens | OpenRouter all-in |
|---|---|---|
| today (1 build/cell) | 45M | $13.87 |
| 3-way build, 1 convergence pass | 72M | $22.28 |
| 3-way build, 2 convergence ROUNDS | 112M | $34.90 |
| 5-way build, 2 rounds | 166M | $51.73 |

**On the current hybrid every row costs $0 in cash**, because qwen is local. The
cost of parallel construction here is Spark time, not money. Nemotron stays at
~$0.90 in every row, because single-shot critics do not multiply.

**Design lever: rounds are more expensive than ways.** Going 1-way to 3-way costs
1.6x. Adding a second convergence *round* costs another 1.6x, because each round
is a fresh agentic loop at ~230k tokens while an extra parallel builder is just
one more loop running beside the others. Converging via a **single-shot
comparator** (Nemotron economics, ~19k tokens) rather than by sending builders
around again is roughly 12x cheaper per unit of convergence pressure.
Convergence-by-judging scales; convergence-by-rebuilding is what produces the $50.

### Wall-clock and the second-Spark question

The token model cross-checks against measured throughput: 45M tokens at the
post-fix 1,852 tok/s prefill is ~6.8h, which matches the observed ~6h run.
Extending it, 5-way x 2 rounds at 166M tokens is ~25h on one Spark.

A second Spark is justified here, but by the **scaling curve rather than the
price**: 4x the admission cap bought only 2.03x prefill, so the box is
compute-bound and 3x to 5x more build work cannot be absorbed by more
concurrency. A second GB10 adds real compute. Note its interconnect
(ConnectX-7, 200 Gb/s, ~25 GB/s) is adequate for sharding inference and rollouts
but slow for data-parallel gradient sync, so it buys roughly 2x compute and 2x
memory rather than 2x training speed.

## Blocker 1 (hard, Option B only): the cost ceiling meters account-wide spend

`src-tauri/src/bin/elmer_battery.rs:75` sets `DEFAULT_CELL_CEILING_USD = 2.0`.
The watchdog decides whether a **cell** blew its budget from an **account-wide**
number:

```rust
let usage_before = credits_before.total_usage;   // captured at cell start
...
live_spend = Some((now.total_usage - usage_before).max(0.0));
```

`fetch_credits` GETs `/api/v1/credits`, which is total account usage. Serially
this is fine, because only one cell spends at a time, so the account delta is
that cell's spend. Under N-way concurrency every in-flight cell attributes all N
cells' spend to itself. At ~$0.13/cell, 12 concurrent cells cross the $2 ceiling
within minutes and mass-cancel, recorded as `cancelled` with "cell cost ceiling
reached". The effect is also order-dependent: later-starting cells see a larger
delta sooner.

This is the same ceiling as **bd tuxlink-l264r**, still unfixed, which already
cost-cancelled frontier models once and was mis-attributed as capability. Two
latent problems that only interact once both changes land together: moving qwen
to OpenRouter re-arms a ceiling that is currently dormant, and parallelism makes
that ceiling read N times too high at the same moment.

**Fix:** meter per-cell from the response's own `usage` field rather than account
credits, or drop the ceiling to the ledger stop as l264r already proposes.

## Blocker 2 (both options): the 30-minute wall is wall-clock

From the watchdog comment: *"The runner's own Limits (30 min run / per-turn
timeout) bound the cell regardless."* That is the 1800 s that produced A1's
`needs_operator` at 36 turns and 2.36M prompt tokens, against a **dedicated**
endpoint showing `num_requests_running = 1`.

A1 is already close to the wall serially: at ~20 s TTFT p50 it spends roughly
12 minutes of its 30 in prefill alone. Any latency inflation, from local batching
or from shared provider tenancy, pushes heavy cells over. Because the limit is
wall-clock, that converts a throughput change into a false `needs_operator`
capability verdict.

**Fix before widening either way:** raise the limit in proportion to the chosen
width, or make it turn-based rather than wall-clock.

## Secondary: Xvfb orphans leak per run

`xvfb-run -a` is leaving servers behind. R2 currently has ~13 orphaned Xvfb
processes (`:99` through `:109`) alive from runs days old. At 36-way concurrency
that leak grows linearly with width. Also budget ~150 MB RSS per concurrent
`elmer_battery`, plus one Xvfb each.

## Recommendation (REVISED after measuring Option A)

The original recommendation was "try Option A first, reach for Option B only if
its curve flattens early." The curve flattened immediately: 1.16x on the
dimension that matters. Revised:

1. **DONE, and it settled the question: local is worth ~2.4x, for free.** The
   1.16x had been measured against a hand-set admission cap of 2. Raising
   `--max-num-seqs` to 8 took prefill from 786 to 1,852 tok/s with zero
   preemptions. Option B is therefore NOT needed to make the ladder tolerable;
   it remains the answer only if same-hour iteration (~1h runs) becomes the
   requirement, since a single box still cannot beat elastic provider capacity.
   The measured local ceiling and its sub-linear scaling are recorded above.
2. **Fix Blocker 1 (account-wide credits metering) in the same change.** It is
   dormant today only because builds run on the Spark; moving qwen to OpenRouter
   re-arms it at the same moment concurrency makes it read N times too high.
3. **Fix Blocker 2 (wall-clock `max_response_duration`) before widening on
   OpenRouter**, where shared tenancy makes latency inflation far more likely
   than it was locally. Both blockers need a Rust rebuild, so do them together
   and treat the result as a clean re-run rather than a mixed dataset.
4. Keep width 3 locally in the meantime. It is free, measured safe (queue time
   0.25 s against an 1800 s budget), and worth ~15%.
5. Do not edit a driver while a run is in flight. bash reads a script
   incrementally, so an in-place edit can corrupt execution. Deploy a new file
   and restart instead, which is what `ladder2-par.sh` did.
