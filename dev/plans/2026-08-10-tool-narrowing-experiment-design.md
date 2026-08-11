# Tool-narrowing experiments — DRAFT pending operator validation

The confirmed step-3 shape ("classifier-driven tool narrowing, lazy tool
schemas, and prefill warm-up + static-prefix discipline") left two questions
the operator ruled empirical. This is the plan to answer them by testing.
Written to be read linearly; every piece is named in words, in place.

## The two questions

1. **When the shortlist misses.** The classifier picks a short list of
   relevant tools for each request. When the right tool is not on that
   list, how does the assistant still get to it — and how often does that
   situation actually come up?
2. **How much narrowing per assistant.** A frontier model may do best
   seeing every tool; the small CPU models may only be usable when
   narrowed. Where the line sits for each tier is unknown.

## Groundwork first — no models involved

Three artifacts, all in this repo:

- **A description file of all 92 tools** — name, one line on what it does,
  its tier and taint/arm flags — generated from the tool registry exactly
  the way the catalog enrichment is generated from the catalog, with the
  same cannot-drift CI test. This one file is written once and used three
  times: as the classifier's tool corpus, as the payload of the
  "what tools are there?" inventory tool, and as the docs input.
- **A labeled request list**: 40–60 realistic operator asks, each labeled
  with the tool that should handle it. Same pattern as the 44 labeled
  catalog queries.
- **The shortlist-size chart**: run the classifier over the labeled
  requests and chart how often the right tool lands in its top 5, top 8,
  top 12, top 16. This picks the shortlist size and tells us how rare
  "right tool missing from the list" really is — before any model time is
  spent.

## The experiment

Give the same tasks to three assistants:

- the **small model** — the same ~1.7B class from the operator's
  CPU-viability hand-poke, for comparability; the tier narrowing must
  rescue. Served from the Spark cluster beside Inkling if it fits
  (operator amendment 2026-08-10: R2 is not a suitable inference host).
  Serving it on fast hardware does not distort the questions asked here:
  CPU wall-clock pain is already established by the CPU-viability eval,
  and this experiment measures capability-tier behavior and token
  counts, which the serving host doesn't change;
- **Inkling** on the Spark — the shipped default, so it gets the fullest
  comparison;
- **GPT-5.6-Luna** over OpenRouter as the frontier arm (operator
  amendment 2026-08-10: the cheapest frontier-class option; its
  frontier-ness is "a bit questionable" per the operator — remember that
  caveat when reading its row as the capable-model ceiling).

Each assistant sees the tools presented one of four ways:

- **Everything** — all 92 tool descriptions up front. Today's shipped
  behavior; the baseline that reproduces the ~15k-token cost.
- **Everything plus a hint** — all 92, plus one classifier line saying
  which few look relevant. For strong models this may be all that's
  needed; it is the purest form of "advise, don't gate."
- **Narrowed with a safety net** — full descriptions only for the
  shortlisted tools, plus a compact list of all 92 names with one-liners,
  and any tool callable by name with its description loaded on demand.
  The shape we currently expect to ship. **Hard requirement (operator,
  2026-08-10): at minimum the Inkling tier must always be able to get
  past the classifier to the full toolset if it needs or wants it — the
  classifier's narrowing is never a wall for a capable model.**
- **Narrowed, nothing else** — the shortlist only. Not a shipping
  candidate; it exists to measure how bad a miss is with no recovery path.

Inside "narrowed with a safety net," three recovery flavors are compared:
name-guessing alone (no list — does a model ever call an unlisted tool?),
the name list (look it up, then call it), and the name list plus an
automatic classifier re-ask when the model signals it is missing a tool.

Not every assistant runs every setup. Inkling runs all of them. The small
CPU model runs the baseline (or just its token-cost measurement, if full
tasks are wall-clock-prohibitive — recorded either way), the safety-net
setup, and the shortlist-only diagnostic. The frontier model runs the
baseline, the hint setup, and the safety-net setup.

## The tasks

- About **30 ordinary tasks** where the classifier's list contains the
  right tool — adapted from existing bench cells (mailbox triage, catalog
  compose, config reads, routines).
- About **15 tasks deliberately phrased so the right tool is NOT on the
  list** — verified with the classifier while writing them. These exist
  purely to watch recovery happen or fail.
- About **10 tasks needing no tool at all** — to check that narrowing
  doesn't provoke made-up tool calls.

Every combination runs at least three times (rates, not one-shot
pass/fail), graded both ways per battery methodology: deterministically
against fixture truth, plus the LLM judge.

## What gets measured

Did the task succeed. Did the assistant call the right tool, a wrong tool,
or a tool that does not exist. On the deliberately-missing tasks: did it
recover, and what did recovery cost in extra turns and tokens. And for
every setup on every assistant: how many tokens the tool presentation
costs and how long the first response takes — which turns "narrowing pays
off more than linearly" from a claim into a chart.

Classifier-only accuracy (from the groundwork) is reported separately from
end-to-end task results, so classifier quality and integration effects
don't blur together.

## What the results decide

- **The shortlist size**: the smallest number where the groundwork chart
  flattens.
- **Which recovery flavor ships** — or, if the groundwork shows misses are
  rare at the chosen size, the simplest one.
- **Per assistant tier, the default presentation**: the least-restrictive
  setup that is not measurably worse than showing everything, while
  costing meaningfully less. Expected but unproven: frontier and Inkling
  want "everything plus a hint"; the small CPU model needs "narrowed with
  a safety net." The runs confirm or kill that.
- Whether the presentation is per-backend configuration or automatic.

The operator rules on the numbers; nothing auto-decides.

## Where it runs, in what order

Groundwork in this repo. Task adaptation and runs in the bench repo
(private; sanitized exports only). Serving: the Spark cluster for both
the small model and Inkling (already up), OpenRouter for GPT-5.6-Luna.
Order: groundwork → the cheap token-cost measurements → the full Inkling
comparison → the reduced small-CPU and frontier rows → analysis and a
decision brief.

## Honest limits

The labeled sets are authored by one session against the registry and
catalog (bench-generated pairs are the hardening path). One machine per
tier. The deliberately-missing tasks measure recovery, not how often
misses happen in the field — the groundwork chart measures that. Inkling
is graded on final content only with generous token limits (its
reasoning-first wire shape, established in the parseability spike).

Session: moss-tamarack-taiga, 2026-08-10. Status: **APPROVED by the
operator 2026-08-10** ("The design is otherwise sound and approved") with
two amendments applied above: the small model serves from the Spark
cluster, not R2 ("The R2 is not a suitable inference host"), and the
frontier arm is GPT-5.6-Luna over OpenRouter (cheapest frontier-class;
operator's frontier-ness caveat noted). Groundwork is a go.

Third operator note (2026-08-10, during the curation iteration): the
curated synonym vocabulary is "just a token/friction reducer" — if it is
load-bearing as the solution, the classifier has failed as a classifier.
The real robustness lives in the recovery paths and the reachability
requirement above; do not chase phrasing space with hand vocabulary, and
read curation-driven chart lifts as friction reduction, not mechanism.
