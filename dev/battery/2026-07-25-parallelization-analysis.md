# Ladder parallelization analysis (2026-07-25)

Author: condor-basalt-hemlock. Measured against the live Ladder-2 run, read-only,
without perturbing it. Captures two options for cutting the ~14h serial wall
clock, the blockers each one hits, and what to fix first. Not yet filed as bd
issues (operator's call on timing).

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
240:1, so it barely moves the wall clock. **Prefill gained only 16%, which means
prefill compute on the Spark is effectively saturated even at one in-flight
request.** Two concurrent prefills each take roughly twice as long and aggregate
throughput hardly moves. The idle instant sampled earlier was a real duty-cycle
gap, but a small one in aggregate.

Net expected wall clock: roughly **12h instead of 14h**. Local parallelism is
worth about 15% on this workload, not 3-4x.

The scheduler also caps concurrency independently of memory:
`num_requests_waiting = 1` with `waiting_by_reason = capacity` while KV sits at
2.3%. That is a vLLM serving-config limit on twin-bramble (`max_num_seqs` or the
batched-token budget), not something tuxlink controls. Raising it would not help
much anyway, since prefill compute is the actual wall.

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

**Cost: ~$26 per complete 108-condition ladder** at DeepInfra fp4, derived from
the measured token counts above. Neither provider exposes a prompt-cache
discount, so every resent turn bills at full list price. That is the entire cost
of the 240:1 ratio.

**Speedup:** ~1h wall clock at 12-24 wide, versus ~14h serial. Past roughly 36
the critical path stops shrinking, because it becomes the single slowest chain
(A1: three build attempts that each run to the 30-minute wall).

**Comparability caveat:** OpenRouter reports a generic `fp4`, not specifically
NVFP4. PR #1240 established local <-> API qwen functional parity, but that was a
different endpoint; a spot-check is warranted before trusting a cross-run
comparison.

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

1. **Option B is the answer for faster iteration, and the operator's original
   instinct was right.** The reason is now concrete rather than speculative: the
   workload is 240:1 prefill-dominated, and prefill is compute-saturated on a
   single Spark. Concurrency on one box cannot fix that. On OpenRouter each
   concurrent request lands on separate provider hardware, so prefill genuinely
   parallelizes. That is where the 12-24x lives, and it is not reachable locally
   at any width.
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
