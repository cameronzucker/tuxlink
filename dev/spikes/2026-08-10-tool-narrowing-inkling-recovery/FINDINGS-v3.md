# Real-context battery (v3) — consolidated findings, all four tiers

The validation instrument the operator's devil's-advocate rounds drove us
to: no invented system prompt (production sends none), the registry's real
JSON schemas in the function-calling `tools` array, grading on emitted
tool calls. Conditions: **everything** (all 92 schemas — today's shipped
shape) vs **narrowed-net** (top-12 schemas + one narrowing-layer system
message carrying the advisory shortlist and the full 92-name inventory,
any tool callable by name). 28 cases per condition; raw rows in
`results-v3/`. Inkling ran at the production temperature (0.2), twice,
so condition effects can be read against its own run-to-run variance
(±1–2 per bucket); other rows are single runs at server-default
temperature — a known limitation, stated once here.

## Tier table (real wire; Inkling = mean of two runs)

| datum | small 1.5B | mid 30B | Inkling (shipped) | Luna (frontier) |
|---|---|---|---|---|
| hits, everything | 9/12 | 5/12 | 2.5/12 | 3/12 |
| hits, narrowed-net | 10/12 | 5/12 | 3/12 | 6/12 |
| two-turn, narrowed-net | 0/5 | **5/5** | 3.5/5 | 2/5 |
| outside-array calls (labeled-correct) | 0 | **3 (3)** | 10 (2.5) | 0 |
| no-tool declines, narrowed-net | 4/4 | 4/4 | 2.5/4 | 4/4 |
| fabricated names | 0/56 | 0/56 | 0/112 | 0/56 |
| median prompt tokens, everything → narrowed | 20,483 → 6,063 | 20,467 → 6,063 | 17,565 → 5,571 | 16,364 → 4,988 |

**Absolute hit numbers are not comparable across tiers** and undercount
agentic models by design: every 30B/Luna/Inkling hit-failure is a
purposeful call (zero no-calls in the whole battery), dominated by
read-before-act pre-flights. The 1.5B "wins" hits precisely because it is
not agentic. Within-model condition comparisons remain valid; absolute
success needs the bench's multi-call agentic loop.

## Conclusion 1, as litigated (supersedes "zero behavior cost")

Narrowing cuts the real prompt ~70% on every tier (and the full surface
does not even FIT common local serving defaults — both local models
400'd on the everything condition until server context was raised past
~20k). Behavior cost, per paired per-case analysis:

- 30B: 26/28 outcome-concordant, one case lost / one gained (within
  sampling noise at unset temperature), 6/28 different picks. No net
  cost; not literally zero behavior change.
- 1.5B: net +1 under narrowing.
- Luna: net +3 under narrowing on hits (mechanism visible: fewer visible
  tools → less pre-flight → more direct action) BUT two real recovery
  LOSSES on stale-shortlist multiturns — full-surface Luna found
  printer_list/the bluetooth lister among the 92; narrowed Luna, refusing
  to leave its array, picked wrong in-array tools. Directional cost for
  strict-contract models; the fix is per-turn re-classification.
- Inkling: condition differences are within its measured run-to-run
  variance — narrowing is behavior-neutral for the shipped tier while
  cutting its prompt 68%.

## Conclusion 2 refined: reachability by tier, now with all four measured

- **Luna (strict-contract frontier): never leaves the array** (0/56).
  Needs a declared mechanism (loader function or harness re-classify).
- **30B: leaves it rarely and perfectly** (3/3 correct by-name
  recoveries, 5/5 multiturn). Prose reachability simply works here.
- **Inkling: leaves it freely and PRINCIPLED-ly** — ~10 outside-array
  calls per run, but the split shows what they are: `server_info`
  arm/taint pre-flights before transmit-adjacent asks (the documented
  pre-check discipline, fetched by name when the shortlist omits it),
  `docs_search` grounding on ham-knowledge questions (defensible against
  our "none" labels), and genuine recoveries (printer_list in BOTH runs,
  the bluetooth lister, docs-lookup). Zero fabricated names in 112 calls.
- **1.5B: never leaves the array and can't track turns** — harness-side
  recovery only.

**The actionable design finding:** add a small deterministic
ALWAYS-INCLUDE pin-set to the narrowed surface — `server_info`, the
abort tier, `docs_search` — alongside the classifier's top-12. That
converts nearly all of Inkling's outside-array traffic into in-array
calls for ~3 schemas of cost, serves the pre-flight discipline instead
of fighting it, and shrinks the by-name channel to genuine recoveries.
The unknown-call rejection backstop (catalog-id validation pattern)
covers what remains.

## Standing conclusions carried forward

Ask-affordance is tier-dependent (v2: a self-serve ASK collapsed
Inkling; the deterministic ask-margin owns asking below frontier).
Fabrication is a text-grammar phenomenon only (one invented name all
night, v2 30B; zero across 280 real-wire calls). The instrument ladder
v1→v2→v3 is documented in FINDINGS.md / FINDINGS-v2.md with each
retraction; nothing below the real wire is cited as validation, and the
bench agentic loop (frozen pre-computed shortlist fixtures; the
classifier never runs inside the benchmark) is the final instrument.

## Coverage honesty (operator challenge, recorded)

The catalog floor's labeled set is ~70% weather-shaped over a ~90%
weather catalog; help-docs, gateway-lists, keps, nets, and emcomm
sections are thinly probed. The classifier-lane fix is a non-weather
extension of the labeled floor; the operator's six-model panel
(GLM-4.7-Flash, Qwen3.5-9B, Qwen3.5-27B, Nemotron Nano, Mistral Nemo
12B, Gemma 4) is the next measurement block and none of it has run yet
— tonight's model pool was ad-hoc.

Session: moss-tamarack-taiga, 2026-08-10 → 11. Serving incidents during
the runs and their resolutions are recorded in the runbook and PR #1335.
