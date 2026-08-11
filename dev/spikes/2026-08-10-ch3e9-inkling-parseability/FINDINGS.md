# ch3e9 step-1 spike — Inkling parseability of the enriched catalog index

**Verdict: GO.** The enrichment format is parseable and usable by the served
Inkling (`inkling-small-nvfp4`, Spark vLLM). Format pick: **JSONL** (edges the
compact-text form on selection and is the structured choice for step 2).

Operator condition satisfied (ADR 0030 ruling 4): "index enrichment proceeds
only if it passes muster with a spike against Inkling to ensure it's
parseable."

## Numbers (44 labeled queries × 2 formats + 6 structural probes × 2, one run)

| Format | Structural parse | Select (answer/ask/none) |
|---|---|---|
| JSONL | **6/6** | **42/44 (95.5%)** |
| compact text | **6/6** | 41/44 (93.2%) |

Zero hallucinated item ids in 100 calls. Zero null-content (reasoning-only)
responses at max_tokens=1600. Slice: 120 enriched entries (~realistic
post-classifier shortlist size), all query-labeled items present.

## Grader-bug disclosure (affects nothing above; first-cut numbers were wrong)

The first-cut grader reported 36/44 & 35/44. Twelve "failures" were a caller
bug: section-kind queries post-checked `True in by_id` (boolean membership
against dict keys), clobbering CORRECT `ASK:` responses. Fixed in `spike.py`
(isinstance guard); numbers above are the offline re-grade of the same
recorded responses (`results/rows.jsonl` is the raw evidence).

## Residual failures — one class, adjudicated

All five (3 distinct queries) are confident `ITEM:` picks where the label
wanted `ASK:`; none picked a wrong or invented id:

- `buoy-sf` (both formats): picked `NDBC46026` — the label's own single
  correct item. The label's ask-preference encodes caution among many buoy
  ids; the pick is factually right. Defensible.
- `nv-reno` (both): picked `NV_TAB_RENO`, one of the label's TWO valid items
  (tab vs zone). Mild ask-calibration miss on a genuine two-way tie.
- `grib-generic` (text only; JSONL asked): picked `CUSTOM.GRIB` over
  `MAXSAEA_GRIB` on "grib files". Mild; the winning format got it right.

The residual class is exactly what the product design absorbs upstream: the
T1 classifier's **ask-margin threshold** (T0-owned) decides when the shortlist
is close enough to force a disambiguation, so the LM never free-solos the
tie-break; and staging still transmits nothing without the operator connect.

## What this licenses (step 2)

1. Enrich the full 1,477-item catalog in the JSONL entry shape
   `{id, section, title, intent, synonyms}` (production enrichment quality is
   step-2 authorship; this spike's per-section templates were deliberately
   mechanical).
2. Build the T1 request-classifier surface per ADR 0030 (bge-small over the
   enriched index; two T0-owned thresholds; advisory DTO
   `{corpus, item_ref, score, verdict}`), narrowing to a shortlist slice of
   ~this spike's size for the agent.
3. Bench-style validation inherits this harness: the 44 labeled queries are
   the floor; a catalog-disambiguation cell family (bench TR-CATALOG-*
   friction) is the growth path. Manual smoke: the operator's field case
   ("pull the weather for my local area") must produce either the right NWS
   zone item or a bounded ask.

Repro: `python3 spike.py` (serving pre-flight first; endpoint + model pinned
in-script). Session: moss-tamarack-taiga, 2026-08-10 evening AZT.
