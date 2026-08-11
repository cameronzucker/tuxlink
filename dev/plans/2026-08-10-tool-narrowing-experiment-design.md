# Tool-narrowing experiment design — DRAFT pending operator validation

Settles the two OPEN empirical points from the confirmed step-3 shape
("classifier-driven tool narrowing (efk3k), lazy tool schemas, and prefill
warm-up + static-prefix discipline + progress UI (8dkcy)" — CPU-viability
field correction, operator-ratified 2026-08-10):

- **RQ1 — miss recovery**: when the classifier's narrowed shortlist omits
  the tool a task needs, by what mechanism does the system recover, and at
  what rate does the miss even occur?
- **RQ2 — bindingness per tier**: for each backend tier, how binding should
  narrowing be — from advisory note over the full surface to
  shortlist-only context?

Doctrine anchor: ADR 0030 — verdicts advise capable models and fully drive
behavior only in flows with no capable model. The experiment operationalizes
where each backend sits on that line, with numbers.

## Prerequisite (E0, in-repo): the tool-surface corpus

The classifier cannot narrow tools until the tool surface exists as corpus
instance #2 on the (already corpus-generic) tuxlink-classify machinery:

1. **Registry-generated enrichment** of all 92 MCP tools: name, one-line
   intent, tier, taint/arm flags — generated FROM `router.rs`'s registry with
   a drift-gate parity test, exactly the catalog-enrichment pattern. This
   ONE artifact also becomes the 6vyk4 inventory tool's payload and the
   nsnre docs input.
2. **Labeled query set**: 40–60 plain-language operator asks → correct
   tool(s), authored against the registry (same pattern as the catalog's
   44-query floor; bench-generated pairs harden it later).
3. **Corpus-2 calibration**: run the existing eval/calibration harness over
   (bge-small, tool-surface, template v1) → measured reject-floor/ask-margin
   + **top-k hit-rate curve at k ∈ {5, 8, 12, 16}** — this curve picks the
   shortlist size and directly bounds RQ1's expected miss rate.

## Factors

**Backends (3 tiers):**

| id | backend | why |
|---|---|---|
| B1 | ~1–2B instruct, CPU on R2 (the y28so class, same model for comparability) | the tier narrowing must rescue |
| B2 | `inkling-small-nvfp4` on the Spark | the shipped default assistant |
| B3 | one frontier arm via OpenRouter (GLM-5.2 per roadmap) | capable-model ceiling / doctrine-pure reference |

**Surface conditions:**

| id | context contains | narrowing role |
|---|---|---|
| S1 | all 92 schemas (today's shipped shape) | control; reproduces the ~15k tax |
| S2 | all 92 schemas + classifier advisory ("likely relevant: …") | ADR doctrine-pure for capable tiers |
| S3 | top-k schemas + compact inventory (92 × name+one-liner, ~1–2k tok) + lazy-by-name schema load | the candidate shipping shape |
| S4 | top-k schemas only, nothing else | diagnostic: raw miss cost bound |

RQ1's recovery mechanisms nest inside S3 as sub-conditions:
**S3a** lazy-by-name only (no inventory — does the model ever guess names?),
**S3b** inventory + lazy-by-name (can it look up, then call?),
**S3c** S3b + harness-side classifier re-query when a miss signal fires
(model says it lacks a tool / calls an unknown name).

**Pruned matrix** (full crossing is 3×6; these 12 cells carry the decisions):

- B2 (most decision weight): S1, S2, S3a, S3b, S3c, S4
- B1: S1 (prefill curve may substitute for full tasks if wall-clock is
  prohibitive — recorded either way), S3b, S4
- B3: S1, S2, S3b

## Task set (bench-fixture adaptation, built in the bench repo)

- **T-in (~30)**: correct tool ranks inside top-k for the phrasing — the
  common case. Adapted from existing bench cells (mailbox triage, catalog
  compose, config reads, routines).
- **T-out (~15)**: engineered so the correct tool ranks OUTSIDE top-k —
  verified during construction by running the classifier itself on the
  phrasing. This is the RQ1 probe.
- **T-none (~10)**: no tool needed (direct answer / honest refusal) —
  guards against narrowing inducing hallucinated calls.

Per battery methodology: **3 runs per cell minimum** (reliability as a rate,
not binary; re-run failures), deterministic grading from fixture truth
**plus the LLM backup scorer** on every battery.

## Metrics

1. Task success rate (primary).
2. Correct-tool-call rate; wrong-tool rate; **hallucinated-name rate**.
3. **RQ1: recovery rate on T-out** (reached the needed tool via the cell's
   mechanism) + recovery cost (extra turns/tokens).
4. **Prefill tokens + time-to-first-token** per cell — turns the
   "narrowing pays superlinearly" claim into a measured curve per tier.
5. Corpus-2 classifier-only metrics from E0 (top-k hit rate, reject gap) —
   reported separately from end-to-end so classifier quality and
   integration effects don't confound.

## Decision mapping (questions the results answer; operator rules on numbers)

- **k** (shortlist size): smallest k where E0 hit-rate plateaus.
- **RQ1 → shipping recovery mechanism**: S3a vs S3b vs S3c on T-out
  recovery rate at acceptable cost; if E0's miss rate at chosen k is
  negligible, simpler wins.
- **RQ2 → per-tier default surface**: for each backend, the least-binding
  condition that is not measurably worse than S1 on success while paying
  materially less prefill. Expected shape (to be confirmed, not assumed):
  B3→S2, B2→S2 or S3b, B1→S3b.
- Whether the surface condition is config-exposed per backend or automatic.

## Infrastructure

- Runner: the bench harness already drives MCP tools against fixture worlds
  (4,138-call evidence base); it gains a surface-condition control (which
  schemas are presented + the inventory blob + lazy-load hook) —
  `tuxlink-mcp-testserver` is the natural place for the condition switch.
- Classifier in the loop via `tuxlink-classify` (corpus-generic — no code
  changes expected beyond a thin CLI/shim for the harness).
- Serving: Spark (Inkling, already up), R2 CPU (B1), OpenRouter key via
  keyring (B3). Bench adaptation and runs live in the bench repo (private;
  sanitized exports only).
- Order: E0 (repo) → E1 prefill-cost curves (cheap, no tasks) → E2 = B2
  full row → E3 = B1/B3 reduced rows → E4 analysis + decision brief.

## Threats to validity (named up front)

Labeled sets authored by one session against the registry/catalog
(bench-generated pairs are the hardening path); one hardware sample per
tier; Inkling's reasoning-first wire shape (grade final content only,
generous max_tokens — established in the parseability spike); small-model
arg-stringification quirks (lenient deserializers are already standard);
T-out is engineered, so its absolute miss rate is not an estimate of field
miss rate — E0's hit-rate curve is.

Session: moss-tamarack-taiga, 2026-08-10. Status: DRAFT — operator
validates axes, prunes, sizes, metrics, and decision mapping before E0.
