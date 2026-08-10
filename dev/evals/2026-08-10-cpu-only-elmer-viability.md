# CPU-only Elmer viability on Pi-class hardware (tuxlink-y28so)

Operator-dispatched 2026-08-10 (ADR-0030 point-1 rider): measure whether
CPU-only local models are useful for *literally anything* in the Elmer role,
so the first-party docs can carry an informed warning instead of vibes.

Setup: Raspberry Pi 5 (4× Cortex-A76, 16GB), llama.cpp b10344 prebuilt arm64
`llama-server` (`--jinja`, 8K ctx, 4 threads), userland, no GPU. Models:
Qwen3-4B-Instruct-2507 Q4_K_M (2.4G) and Qwen3-1.7B Q4_K_M (1.1G) with
thinking DISABLED via `--chat-template-kwargs '{"enable_thinking":false}'` —
which worked cleanly, incidentally proving the request-side thinking-suppression
mechanism discussed for the provider (Arm-C shape). 22 Elmer-shaped probes per
model, strictly sequential: tool-call fidelity (10), catalog disambiguation
ask-vs-pick (5), ham factual QA (5), honest refusal (2). Deterministic grading;
raw rows in `dev/scratch/cpu-elmer-viability/results/` (local).

## Scoreboard

| | Qwen3-4B-Instruct | Qwen3-1.7B (no-think) |
|---|---|---|
| Tool-call fidelity (load-bearing) | **10/10** | **10/10** |
| Disambiguation ask-vs-pick | 5/5* | 5/5 |
| Ham factual QA | 4/5 | 3/5 |
| Honest refusal (no fabricated call) | 2/2 | 2/2 |
| Median wall per task | 12.8 s | 4.4 s |
| Median generation rate | 2.6 tok/s | 6.0 tok/s |
| Server RSS while active | 5.8 GB | 3.3 GB |

\* Scored 4/5 by the automated grader; the "miss" picked the right item then
politely asked to confirm — a grader-strictness artifact (the grader demanded
no question mark on picks), not a model failure. Corrected here per the same
outcome-over-path doctrine the bench judge uses.

## What the misses actually were

Every real failure was **ham-lore knowledge**, not agency: the 1.7B claimed
QRZ means "located in a specific area" and that DM33 is a two-character grid;
the 4B rendered QRZ as "is anyone there?" (adjacent, not the standard "who is
calling me?"). Zero tool-selection errors, zero fabricated tool calls, zero
fake-success claims across 44 probes. Small local models mangle amateur-radio
trivia *confidently* — which is precisely the case for the docs-grounding tool
(tuxlink-0mudm: retrieve-then-answer, refuse when ungrounded).

## The architecture synergy worth naming

The disambiguation probes handed the model five pre-narrowed candidates — the
exact surface the T1 request classifier produces. Both models handled that
last-mile perfectly, while neither could plausibly chew the raw 1,477-item
catalog at these speeds. **The classifier tiers make CPU-only Elmer *more*
viable, not less**: T0+T1 shrink the problem (candidates, thresholds, geo
parsing) so a small slow generator only performs the final step. The
degradation ladder composes.

## Honest caveats

Single-turn probes only (multi-turn agent loops compound the latency), 8K
context (a production system prompt + tool catalog is larger — prompt-eval
time grows with it), n=22 per model, keyword grading (lenient), one hardware
sample. Long-form output at 2.6–6 tok/s is genuinely painful: a 300-token
reply is 1–2 minutes.

## Drafted warning language for the first-party docs (tuxlink-nsnre)

> **Running the assistant without a GPU or inference server.** Elmer can run
> against a small model on your computer's CPU alone. In our measurements on
> a Raspberry Pi 5, a 4B-class model chose the correct tool for every simple
> request we tested and honestly declined things it couldn't do — but each
> short reply took roughly 5–15 seconds, longer answers take minutes, and it
> used 3–6 GB of RAM while active. Small models also state amateur-radio
> facts wrong with full confidence, so keep the assistant's documentation
> grounding enabled. CPU-only mode is workable for patient, single-step
> requests; a modest GPU or a network inference server removes both the
> speed and the accuracy pressure. Tuxlink never downloads or starts a model
> unless you configure one.

Cross-links: tuxlink-y28so (this test), tuxlink-nsnre (inert-Elmer affordance
+ docs, now unblocked), tuxlink-efk3k addendum 6 point 1, tuxlink-0mudm
(grounding), tuxlink-nyyr2 notes (thinking-suppression mechanism evidence).
