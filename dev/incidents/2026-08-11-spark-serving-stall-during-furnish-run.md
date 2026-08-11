# Spark serving crash during the furnish re-test — incident reconstruction

2026-08-11, ~07:18 AZT (14:18Z). The operator took the Spark cluster
offline after observing the Inkling serving cyclically crashing with
"dozens of simultaneous concurrencies" during the `inkling-v25-furnish`
bench run. The serving stays offline until the crash is sufficiently
explained (operator ruling). This document is the client-side
reconstruction from the complete wire ledger; the server-side half
needs the cluster logs, which are the operator's to open.

## What the wire ledger proves (client side, complete)

The furnish run's every completed exchange is recorded by the rewrite
proxy (`proxy-ledger-inkling-v25-furnish/`). Facts:

- Run window: first chat 13:28:03Z, last successful chat **14:18:18Z**.
  174 units completed in ~50 min (~180 units/h), strictly serial
  (width-1 runner; one `elmer_battery` at a time, verified by process
  count during the run).
- **All 484 chat completions returned 200.** No chat request was ever
  refused or errored through the proxy while the serving lived.
- Request sizes were small and boring: ~25–30KB floor-cell prompts
  (5–6k tokens). The furnish mechanism added at most a handful of
  schemas per request (64 requests carried furnished schemas). Nothing
  anomalous at the 14:18 boundary — the last completed unit was
  TR-ROUTINE-ACTIONS-LIST (29–30s attempts, normal).
- The 966 "404s" in the ledger are NOT serving failures: they are the
  vendored battery's meter polls against endpoints vLLM has never had —
  `/api/v1/credits` (OpenRouter's credits API, 450×) and `/api/tags`
  (ollama's list API, 344×), plus `/api/v1/models` (172×). vLLM 404s
  them constantly, by design, in every run including stock v25. They
  alternate with the real traffic at the battery's normal cadence and
  can look like flapping if read without the path split.
- After 14:18:18Z: zero completed chats. The next unit's request(s)
  HUNG — and hung exchanges produce NO ledger rows (rows are written on
  completion), which also means the ledger's in-flight reconstruction
  (peak 7 concurrent, all brief) undercounts precisely the pathological
  window.

## Reconciling "dozens of simultaneous concurrencies"

A width-1 client cannot OFFER dozens of parallel requests — but it can
ACCUMULATE them behind a stalled engine. The client stack is extremely
patient: the runner's per-turn timeout is 1800s, the proxy holds
upstream connections up to 900s, the session rebuilds its provider per
turn and retries. If the engine stalls (the serving's documented crash
class is a Triton illegal-access in `_fused_sconv_kernel`), serial
requests arrive one at a time, never complete, and pile up as open
sequences/connections on the server side. Every engine restart is then
greeted immediately by the queued turn plus retries — which, from the
cluster's vantage, renders exactly as "cyclic crashes under dozens of
concurrent requests," even though the client never sent two chats at
once by intent.

Separately, the bench dashboard was showing "dozens of cells in
progress" at the same time — that was the run_id label collision
(fixed earlier this morning) leaving stuck-looking cells during the
outage, which compounded the appearance of a runaway-parallel client.

## What was NOT the cause, on the evidence

- Not request content: small floor prompts, all 200-completing until
  the boundary; the furnish schemas add ~1-3KB.
- Not client-side parallelism by design: width 1, one battery process,
  serial units, ~180/h.
- Not the fixture/proxy rewrite failing: zero fail-closed misses in
  either experimental run.

## My failings regardless of root cause (owned)

1. **No upstream-health circuit breaker.** My monitors watched for MY
   failure modes (fixture misses, runner death) and never for serving
   distress. When the engine stalled, my stack kept offering load —
   patient timeouts made it worse — until the operator intervened. Any
   future run against the Sparks carries a breaker: N consecutive
   hung/failed chats → stop the runner, alert, never re-offer.
2. **Evidence destruction on shutdown.** Killing the runner tore down
   the hung-socket pile before counting it (proxy thread count was
   taken too late). Snapshot first, then kill.
3. **The dashboard label bug** muddied the operator's read of the
   incident at the worst possible moment.

## What would confirm root cause (server side — operator's call)

Around 14:18:18Z ± 2 min, on head and worker: the vllm container logs
(engine traceback — the Triton `_fused_sconv_kernel` illegal-access
signature is the known class), `dmesg`/OOM records on both nodes, and
any control-plane metrics history (running-sequence count over the
window would directly confirm the pile-behind-a-stall picture). The
serving remains untouched by agents until the operator rules.

## State at write time

Bench runner killed (174/339 units, all completed-clean, usable for the
floor-tier furnish comparison). Judge draining the completed bundles.
Proxies idle (no upstream traffic possible; serving offline). Nothing
agent-side will touch the Spark cluster.

Session: moss-tamarack-taiga.
