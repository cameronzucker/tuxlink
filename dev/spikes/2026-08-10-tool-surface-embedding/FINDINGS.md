# Tool-surface shortlist-size chart — groundwork result

The first run of the classifier over the registry-generated tool corpus
(92 tools, mechanical enrichment only: name tokens + tier words as
synonyms, first-sentence one-liners), against the 55 labeled operator
asks. R2, bge-small (same snapshot as all prior runs), branch head
7d20f3db.

## The chart (47 tool-labeled queries)

| shortlist size | right tool on the list |
|---|---|
| 5 | 39/47 (83.0%) |
| 8 | 40/47 (85.1%) |
| **12** | **43/47 (91.5%)** |
| 16 | 43/47 (91.5%) |

The curve flattens at 12 — that is the shortlist size, per the approved
design's decision rule.

No-tool rejection already works: the highest score any no-tool ask
reached (0.5682) sits below the lowest score any real ask reached
(0.5958). Margins are noisy at this enrichment quality; threshold
calibration for the tool corpus is DEFERRED until after curation (the
classifier errors loudly on the uncalibrated corpus by design — nothing
mints verdicts from these interim numbers).

## What the misses say (and what the design says to do about them)

Every hard miss is a vocabulary gap, not a ranking noise problem:

- "what frequency is the radio on" never finds `rig_status` (its text
  never says frequency/dial/VFO; the FT-8 tools outrank it).
- "send a message to my brother's winlink address" never finds
  `message_send` (catalog-request tools outrank it).
- "make me a routine that fetches solar conditions" never finds
  `routines_save` (its text is about persistence/revision semantics, not
  creating).
- "how do I set up a digirig" never finds `docs_search`/`docs_read`.

Plus near-misses in the 6–12 band of the same class (`message_read` at
rank 10, `send_form` at 11, `predict_path` at 10, `routines_list` at 6).

The approved design pre-authorized exactly this fallback: mechanical
enrichment first, "curate later if hit-rate demands — the chart will
tell us." It has: **next iteration is a curated synonym/intent table for
the weak tools** (a const beside TOOL_TIERS so the registry emitter
stays the single generator, with the same in-same-change discipline),
then re-run this chart. The recovery-mechanism experiment still matters
afterward — curation shrinks the miss class, it won't zero it.

Session: moss-tamarack-taiga, 2026-08-10 night AZT.
