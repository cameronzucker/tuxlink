# v4 — the Inkling-only battery (post-pivot): narrowing helps, pins don't

Operator pivot (2026-08-11 pre-dawn): the small-model arm is abandoned;
Inkling is the only backend that matters and the classifier must benefit
it, not harm it. v4 is that test at the selection layer: the real
function-calling wire (v3's instrument), all 44 in-top12 labeled cases
plus 13 two-turn cases (8 new stale-shortlist recovery/control pivots),
THREE conditions, production temperature 0.2, three repetitions,
reasoning streams and per-call latency captured. 612 rows, zero
transport errors. Raw rows + failure digest in `results-v4/inkling-v4/`.

Conditions: **everything** (all 92 schemas — today's shipped shape),
**narrowed-net** (classifier top-12 schemas + one system message with the
advisory shortlist and the full 92-name inventory, anything callable by
name), **narrowed-pinned** (narrowed-net + always-include pins:
`server_info`, `docs_search`, the abort section — the FINDINGS-v3
hypothesis under direct test).

## Headline: narrowing measurably improves Inkling while cutting its prompt 68%

Summed across every bucket (204 graded cases per condition over 3 reps):

| condition | total ok | hit (44×3) | two-turn (13×3) | none (4×3) | ambig (4×3) | median prompt | median latency* |
|---|---|---|---|---|---|---|---|
| everything (shipped) | 82/204 | 51/132 | 23/39 | 5/12 | 0/12 | 17,565 tok | 9.6s |
| narrowed-net | **101/204** | 56/132 | **34/39** | 5/12 | 3/12 | 5,571 tok | 4.3s |
| narrowed-pinned | 96/204 | 56/132 | 29/39 | 8/12 | 0/12 | 6,066 tok | 6.0s |

*Latency medians ran under partial mutual contention with the parallel
bench run — directionally robust, not precision figures; the token
medians are exact and load-independent.

Per-rep brackets (in the analyzer output) show the effects are stable
across all three reps, not one lucky draw: two-turn selection is 8/13,
8/13, 7/13 under everything and 11/13, 11/13, 12/13 under narrowed-net.

## Recovery: the narrowed surface recovers BETTER than the full one

The eight stale-shortlist two-turn cases (target absent from the frozen
turn-1 top-12) total: everything 17/24, narrowed-net 20/24,
narrowed-pinned 20/24. The emblematic case: "connect" → "hold on, first
get the radio onto 7.101 MHz USB" — the full 92-tool surface fails 0/3
(pre-flights into connection-status tools); the narrowed conditions go
straight to `rig_tune` by name 3/3, from OUTSIDE the tools array. The
inventory line plus a small surface beats a big surface for pivoting.

By-name lazy calling is proven at volume: 59 outside-array calls emitted
under narrowed-net (23 labeled-correct), 28 under pinned, **zero
fabricated tool names in 612 rows** (as in every battery this week —
fabrication remains a text-grammar phenomenon, absent on the real wire).

## The pin-set hypothesis is REFUTED as a default

FINDINGS-v3 proposed always-including `server_info`/`docs_search`/the
abort tier. v4 tests it directly and the mechanism works as predicted —
24 of narrowed-net's 59 outside-array calls were pin-absorbable, and
under pinned the outside-array count indeed halves (59 → 28). But the
BEHAVIOR delta is negative: pinned trails plain narrowing overall
(96 vs 101) and on two-turn selection (29 vs 34). The failure digest
shows why: pinned surfaces re-invite Inkling's read-before-act
discipline (`server_info` pre-flights before privacy/config mutations),
which single-shot grading scores as a miss, and the pins occasionally
divert first calls that plain narrowing sends straight to the goal tool.
Reachability needs no pins — Inkling leaves the array freely and
principled-ly (v3's finding, replicated at 3× the scale). Verdict:
**plain narrowed-net is the shape to wire**; pins stay available as a
policy knob, not a default.

## Reading the reasoning streams (the "what does it need" question)

331/612 rows carried a non-empty reasoning stream. Every failure class
autopsied resolves to one of three shapes, none of them "the classifier
broke it":

1. **Defensible pre-flights graded as misses.** `inbox-list` under
   narrowing picks `user_folders_list` (folder overview) before
   `mailbox_list` — the classifier ranked `mailbox_list` #1; the model
   chose an enumerate-then-list sequence. The bench's real agentic loop
   adjudicates this: TR-MAILBOX-LIST is judged `delivered` 3/3 in the
   narrowed arm. Same shape for the ambiguous single-turns (a bare
   "connect" gets a `server_info` state check, all conditions).
2. **Clarify-first on underspecified asks, condition-invariant.** "Send
   a message to my brother" → asks for the address and content, checks
   send authority. "Build me a routine" → grounds first. Identical with
   and without narrowing; the classifier's two genuine shortlist misses
   (`compose-send`, `routine-build`) therefore produce ZERO behavior
   difference at this layer.
3. **One genuine hard case:** `mt-stop-privacy` ("stop everything" → "I
   want to stop sharing my exact location") fails 0/9 in every
   condition — a phrasing the model reads as a stop/abort ask regardless
   of surface. A labeling/curation question, not a narrowing effect.

What Inkling "needs" from the classifier, on this evidence: nothing it
doesn't already get. The narrowed frame plus the name inventory is
sufficient; its own pre-flight discipline supplies the rest.

## Relation to the bench A/B (the outcome-level instrument)

Selection-level grading undercounts agentic models by design (every
"miss" above is a purposeful call). The outcome-level answer comes from
the paired bench run (`inkling-v25-full` stock vs `inkling-v25-narrowed`
through the frozen-fixture rewrite proxy — see `bench-arm/`), judged by
the same contract judge. Early paired floor cells: delivered-rate
identical, by-name lazy calls executing correctly in the live loop, zero
fixture misses fail-closed. Full A/B lands with the run.

Session: moss-tamarack-taiga, 2026-08-11. Instrument lineage
v1→v2→v3→v4 with each retraction recorded in the earlier FINDINGS files.
