# Battery v2 — multi-model, vague intent, multi-turn (selection layer)

Extends v1 after the operator's devil's-advocate round. Still a
SELECTION-LAYER instrument (synthetic minimal system prompt, one-line
reply grammar, one-liner tool text — v1's scope correction applies
verbatim); v2 adds the vague single-turns under a TOOL:/ASK:/NONE
grammar, five two-turn cases against deliberately STALE turn-1
shortlists (three steering outside them), per-call token usage, and
model rows beyond Inkling. 28 cases × 2 conditions × model.

## Results so far (Inkling served on the Spark; GPT-5.6-Luna via OpenRouter)

| bucket (per condition) | Inkling v1 (no ASK) | Inkling v2 | Luna v2 |
|---|---|---|---|
| ordinary hits, narrowed | 10/12 | 3/12 | 8/12 |
| ordinary hits, safety-net | 11/12 | 3/12 | 9/12 |
| wrong-shortlist singles, net | **3/3** | 0/3 | 0/3* |
| vague single-turns (want ASK) | — | 4/4 | 4/4 |
| two-turn recovery, narrowed | — | 0/3 | 0/3 |
| two-turn recovery, safety-net | — | 1/3 | **3/3** |
| no-tool asks (want NONE) | 4/4 | 4/4 | 4/4 |
| fabricated names | 0/38 | 0/56 | 0/56 |

\* Luna's net-condition "failures" are not selection failures on
inspection: it committed to the right task and asked for ARGUMENTS
("What is your brother's Winlink address, and what subject...?", "What
time and timezone should the routine fetch...?") or took a defensible
first step (catalog_list before the inquiry; search before read). The
one-line grammar cannot represent ask-then-call; a function-calling
harness can. Instrument limitation, counted honestly.

## The headline finding: the ASK affordance collapses Inkling

One variable changed between Inkling v1 and v2: the grammar gained ASK.
Decisiveness collapsed — ASK on 18/24 ordinary-hit calls and 6/6 miss
calls (none-class stayed 8/8 NONE). Luna kept discrimination with the
identical option. The operator predicted this ("Inkling doesn't really
do vague intent; it's collaborator tier").

**Design consequence, now evidence-backed:** the two-threshold doctrine
lands from an unexpected angle. For the Inkling tier and below, asking
is the deterministic T0 ask-margin's decision — the model must NOT be
offered a self-serve ASK, or it takes the out everywhere. The capable
tier can hold the option (Luna uses it selectively and well). Grammar/
affordance design is tier-dependent, same as narrowing bindingness.

## Cross-turn recovery and the stale-shortlist worst case

The two-turn cases show the model the shortlist computed from the vague
turn-1 text, never re-run. Luna recovers 3/3 with the net and 0/3
without — the walled condition dead-ends across turns exactly as it does
in single turns. Inkling manages 1/3 even with the net under the
ASK-flood regime. Production implication: re-run the classifier per
turn (the stale-shortlist condition is cheap to avoid), and pair
Inkling-tier with deterministic ask gating.

## Cost (same runs, API-reported)

Median prompt tokens per call: narrowed ≈ 470–477; with the full-name
safety net ≈ 2,887–2,894 (~+2.4k/turn at one-liner density). Both are
far under the ~15k full-schema baseline measured in the CPU-viability
eval; schema-level costs get measured by the v3 real-context battery.

## Status of the other rows

Qwen2.5-1.5B (small) and Qwen3-30B-A3B-Instruct (the 20–30B class
people actually run; operator-added row) are downloaded on the Spark;
their batteries run under the Inkling-stop authorization and land in a
follow-up section. The v3 real-context battery (actual Elmer system
prompt, actual JSON schemas, function-calling wire, unlisted-tool
call-by-name as the lazy-schema live-or-die probe) is the validation
instrument for everything above.

Session: moss-tamarack-taiga, 2026-08-10 night AZT.
