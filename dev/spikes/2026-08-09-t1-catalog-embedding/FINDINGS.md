# Findings — T1 catalog-embedding spike (efk3k Phase 1 evidence)

Two-host run: R2 (i3-N305, x86) as the realistic platform per the operator's
2026-08-09 ruling — with a LAN inference backend in the deployment, classifier
inference co-locates with Elmer's backend, so x86 is the measurement target and
the Pi 5 numbers are worst-case-floor context. Full table:
[results/REPORT.md](results/REPORT.md). This document is evidence for the
Phase-2 ADR decision brief; it decides nothing.

## Retrieval quality (zero-shot, real 1,477-item catalog, 44 labeled queries)

1. **bge-small-en-v1.5 is the front-runner: 97.2% top-1, 100% section** on the
   `sec_desc`/`full` templates — identical on both hosts (deterministic). Its
   single item-level miss is the deliberately-hard coordinates probe (below).
   It is also the only model with a **zero-overlap reject gap**: worst no-match
   similarity 0.655 vs best true-match minimum 0.707 — a workable
   "not-a-catalog-request" threshold exists with margin to spare.
2. MiniLM-L6 (22M): 94.4% top-1 / 100% section on `full`, the widest
   answer-class margins (0.17–0.25) and smallest footprint.
3. e5-small and gte-small reach 94.4% top-1 but their similarity ranges are
   compressed (everything ≈0.82–0.87); gte's `desc` reject gap **inverts**
   (no-match max 0.870 > true-match min 0.847) — poor no-match rejection
   zero-shot. gte was the only model to hit 100% top-5 (`sec_desc`).

## Margin behavior confirms the answer-vs-ask design

Answer-class median margins are an order of magnitude above ask-class
(bge: 0.10–0.11 vs 0.006–0.016). Two deterministic thresholds fall out:
a reject floor on top-1 similarity, and an ask trigger on top1−top2 margin.
Both live in T0 (deterministic policy); the encoder only supplies scores.

## The two design-story queries

- **Locality**: "pull the weather for my local area" alone lands in a tight
  ambiguous cluster (margin 0.021 → ask). Appending station grid + state makes
  the top-3 **all WX_US_AZ** with a within-section margin of 0.002 — i.e.
  context resolves the section, the residual margin correctly asks *which* AZ
  product. This is the ch3e9 locality-aware-defaults direction, quantified.
- **Coordinates**: "buoy report near san francisco" fails semantically (Atlantic
  buoys outrank NDBC46026). Geo/numeric fields belong to **T0 structured
  parsing at catalog-index time**; embeddings are the wrong tool there.

## Item-text template

Embedding `SECTION ID: description` ("full") beats description-only by
8–17 top-1 points depending on model. Section context does most of the work;
ID tokens cost nothing and help exact-name asks. The ch3e9 prototype should
embed the full record.

## Latency and RAM (the platform story)

- **True single-query compute on R2: ~14ms** (bge, isolated probe, median of
  20 warm encodes; threads=4 13.7ms ≈ threads=8 14.0ms, threads=1 20.7ms).
  Batch throughput ~5.8ms/item (8.5s precompute / 1,477).
- **Instrument note (open, does not change conclusions):** the in-harness
  per-query numbers for bge/MiniLM/e5 read 150–160ms on R2 while gte read
  18ms in the same loop — the harness CAN see fast encodes, so the 150ms
  class is a measurement-context artifact (numpy scoring work interleaved
  between encode calls, plausibly thread-pool wake effects), not model cost.
  Even taken at face value it is interactive-fine; the native-runtime number
  is bounded by the probe and batch figures.
- Pi 5 floor: 127–238ms python-stack medians, session-contended — functional,
  not the target.
- **RSS ~1.0–1.2GB on both hosts is the Python/torch stack tax**, not the
  models (weights are 90–130MB). The kickoff's 60–300MB T1 RAM budget is
  achievable only with a native runtime (candle/ort) — an ADR decision point.
- Production thread config: 2–4 intra-op threads is the knee; taking all
  8 cores buys nothing.

## Caveats / next

- Zero-shot only; no fine-tuning. The 44-query set was authored by one session
  against the catalog itself (no bench corpus vendored) — bench-generated
  labeled pairs will harden these numbers and enable per-family calibration.
- Pi gte-small (landed in the follow-up commit): accuracy replicates R2
  (0.944 top-1 `sec_desc`) but its batch path is pathological on the Pi —
  4,318s (72 min) per template precompute vs bge's 49s, 1,023ms median
  per-query — reinforcing the model-specific batch slowness seen on R2
  (4–7× bge) and disqualifying gte for any Pi-floor role.
- Licenses: bge-small (MIT), MiniLM (Apache-2.0), e5 (MIT), gte (MIT) — all
  AGPLv3-compatible; weights treated as data per placement rule 4.
