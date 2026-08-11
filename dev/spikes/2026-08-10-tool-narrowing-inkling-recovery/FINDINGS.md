# Inkling recovery spike — the safety net carries the load
# (SELECTION-LAYER PROBE ONLY — see the scope correction at the bottom)

Operator directive tested same-night ("Test it now"): can Inkling get past
the classifier to the full toolset when the shortlist is wrong? Served
Inkling (`inkling-small-nvfp4`), real calibrated top-12 shortlists, 38
calls, mechanical grading. Raw evidence: `results/rows.jsonl`.

## The numbers

| condition | wrong-shortlist probes | ordinary hits | no-tool asks |
|---|---|---|---|
| shortlist only (no net) | **0/3** | 10/12 | 4/4 |
| shortlist + full name list | **3/3** | 11/12 | 4/4 |

Fabricated tool names: **0/38** — even with all 92 names in context, no
invented tools on any ask, including the no-tool ones.

## What each number means

**Recovery works, exactly as the reachability requirement demands.** On
all three asks where the classifier's top-12 was genuinely wrong even
after curation ("send a message to my brother" — message_send absent;
"make me a routine that fetches solar conditions" — routines_save absent,
the embedding swamped by the words *solar conditions*; the digirig
setup-help ask — docs tools at rank 16), Inkling read past the advisory
shortlist and picked the correct tool from the full name list.

**Without the net, the failure mode is safe but dead-ended:** all three
misses came back NONE — honest refusal, no wrong pick, but the operator's
request goes unserved. This is the measured cost of a wall, and the
empirical case for the operator's ruling that the full surface stays
reachable.

**The net does not hurt the common path.** 11/12 vs 10/12 on ordinary
hits — and one hit actually improved with the full list visible
(catalog-request: the narrowed run picked the wrong sibling
grib_send_request; with the net it picked catalog_send_inquiry). The one
safety-net hit failure was a single reasoning-stall (null content, the
known wire-shape artifact — 1 in 38 calls), not a wrong answer.

**The curation-is-not-the-solution framing is confirmed empirically:**
two of the three probes stayed unreachable by vocabulary curation, and
the net recovered both. The synonym table reduces friction; the name
list + reachability is the mechanism.

## SCOPE CORRECTION (operator challenge, same night — supersedes the
## "validated end-to-end" phrasing this section originally carried)

This spike is a **selection-layer probe under a synthetic harness**: a
minimal made-up system prompt, a one-line `TOOL:` text grammar, one-liner
tool descriptions, no real JSON schemas, no function-calling wire, none
of Elmer's production system prompt. That isolates the presentation
variable — which is what a first probe should do — but it does NOT
validate the production path, and the original "validated end-to-end for
the Inkling tier" sentence overclaimed. Retracted.

What transfers from this probe: the model can use a name list to reach
past a wrong shortlist, and 92 names in context induced zero fabricated
names. What does NOT transfer and remains open: behavior under the real
~15k-token system prompt + schema context, and — the live-or-die
question for the lazy-schema mechanism — whether a model will EMIT a
tool call for a function that is not in its `tools` array because prose
told it it exists. The v3 battery (real Elmer system prompt from source,
real schemas via the function-calling API, all four model rows) is the
validation instrument; nothing is "validated" until it runs.

Session: moss-tamarack-taiga, 2026-08-10 night AZT.
