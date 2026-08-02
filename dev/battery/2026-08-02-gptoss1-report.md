# gptoss1: the cross-vendor arm lands fourth. The baseline set is complete.

Run: 2026-08-02 09:59 to 14:51 AZT, 180 attempts in 5h01m at concurrency 4 on
the dual-solo topology (one gpt-oss-120b per Spark, both tailnet endpoints —
first arm to use it). 178/180 valid (2 egress-assert unit kills, see caveats),
178/178 judged, join clean (0 stale, 0 missing). Agent chasm-wren-crag.

Model: `openai/gpt-oss-120b` (120B MoE MXFP4, harmony format), temp 1.0 per
OpenAI recommendation, ctx 131072 (model max), generation `40fd9b7e` — same
judge and harness as the full baseline set. Serving this model closes
`tuxlink-2mwoz`: the July "unservable on GB10" verdict died with the eugr
vLLM 0.26.1rc1 image, where the same MARLIN MXFP4 backend that produced token
salad on every older image now generates clean text (runbook details in the
issue).

## The completed five-model baseline

| | PASS | PARTIAL | FAIL | strict | lenient | Wh/attempt |
|---|---|---|---|---|---|---|
| inkling1 (276B-A12B) | 64 | 38 | 78 | **35.6%** | 46.1% | ~24 (serial) |
| control2 (Qwen3.5 122B) | 56 | 71 | 53 | 31.1% | **50.8%** | — |
| q235 (Qwen3 235B-A22B) | 39 | 64 | 76 | 21.8% | 39.7% | 4.6 |
| **gptoss1 (gpt-oss-120b)** | 31 | 75 | 72 | **17.4%** | 38.5% | 7.0 |
| mistral2 (Mistral Small 4 119B) | 25 | 57 | 98 | 13.9% | 29.7% | 3.4 |

gpt-oss-120b slots between the Qwen generations and Mistral: a very wide
PARTIAL band (75 of 178, the widest in the set) with few clean PASSes. It
starts work credibly almost everywhere and finishes it almost nowhere.

## Per-cell profile

| cell | PASS | PARTIAL | FAIL | | cell | PASS | PARTIAL | FAIL |
|---|---|---|---|---|---|---|---|---|
| P1 | 2 | 1 | 7 | | C1 | 0 | 0 | 10 |
| P2 | 8 | 2 | 0 | | C2 | 0 | 0 | 9 |
| P3 | 0 | 9 | 1 | | C3 | 3 | 5 | 2 |
| S1 | 5 | 5 | 0 | | E1 | 0 | 9 | 1 |
| S2 | 0 | 9 | 1 | | E2 | 0 | 4 | 6 |
| S3 | 0 | 5 | 5 | | E3 | 0 | 4 | 6 |
| S4 | 1 | 7 | 2 | | EU1 | 0 | 4 | 6 |
| A1 | 0 | 4 | 6 | | EU2 | 0 | 0 | 10 |
| EU3 | 9 | 0 | 0 | | A2 | 3 | 7 | 0 |

Two notable shapes:

- **The P1 anomaly is churn, not confusion.** Six of ten attempts on the
  EASIEST cell were cancelled with zero output — temp-1.0 harmony reasoning
  loops exhausted the attempt budget before saving anything. 21 cancelled
  across the run (matching q235's 20, but gptoss's concentrate on P1). The
  best P3 showing in the set (9 PARTIALs) proves the capability is there when
  the loops converge.
- The floor/ceiling cells identified by the 2026-08-02 cross-model sweep
  (C1/C2/EU2 zero everywhere, EU3 ceiling) hold for gptoss exactly, further
  confirming those cells currently measure the harness, not the model
  (tuxlink-pvlyh, tuxlink-opyuy).

## Energy

- **1.244 kWh total** (b70 0.599 + 4cc 0.645) over 4h52m, cluster average
  ~258 W, peak ~381 W
- **~7.0 Wh per attempt** — pricier than the TP2 arms (q235 4.6, mistral2
  3.4) despite the smaller model: reasoning churn and conc 4 spend more
  wall-clock per attempt

## Caveats

- **2 egress-assert unit kills** (C2#10, EU3#7) — the tuxlink-lmrd4 class,
  now 3 occurrences all under concurrency, zero on the serial run. Excluded
  as harness-invalid.
- **3 needs_operator** (context-censored at the 131k ceiling) — counted in
  the censored column, first non-mistral1 occurrences.
- temp 1.0 (vendor recommendation) vs 0.2/0.15 elsewhere; directional
  comparisons only.
- Dual-solo topology means per-attempt serving conditions differ from the
  TP2 arms (two independent engines, no cross-node traffic).

## Ledger

- Bundles: `r2-poe:~/6i8jz-run/battery-results/gptoss1-t10/` (+ `gptoss1_joined.json`)
- Joined data: `dev/battery/comparison/data/gptoss1_joined.json`
- Comparison artifact: `dev/battery/comparison/battery-comparison.html` (8 runs)
- Serving record: `bd show tuxlink-2mwoz` (GB10 bring-up + required flags)
- Harness-sweep findings that bound interpretation: tuxlink-pvlyh,
  tuxlink-opyuy, tuxlink-tx870
- Next: DSV4-Flash candidate (tuxlink-3rtgl), Solar-Open2 (tuxlink-4stvz),
  bohfp skill arm, 69qtv ladder expansion (operator-curated)
