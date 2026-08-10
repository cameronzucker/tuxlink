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

## FIELD CORRECTION (operator hand-poke, same day — supersedes the probe-only framing)

The probe scoreboard above is a 5-tool-schema condition. The operator then
pointed the SHIPPED 0.105.0 Elmer at the same 1.7B server: the production
system prompt + full MCP tool surface prefills **~15,000 tokens**, and
measured prefill throughput **degrades as the sequence grows** (24.4 → 20.4
→ 16.9 tok/s at 2k/4k/6k tokens) — putting the real first-turn cost at an
estimated **18–25 minutes with nothing visible**. Verdict: CPU-only against
today's full assistant surface is **wildly impractical**, full stop. The
probe results stand as the *capability floor* — tool selection and honest
refusal are not the problem — which is exactly why the unlock is prompt
work, not model work: classifier-driven tool narrowing (tuxlink-efk3k),
lazy tool schemas, and prefill warm-up + static-prefix discipline + progress
UI (tuxlink-8dkcy). The degradation curve means narrowing pays superlinearly
(shorter prefix AND faster per-token). Further CPU testing is pointless
until that work lands; this line of testing is CLOSED here.

## Drafted warning language for the first-party docs (tuxlink-nsnre)

> **Running the assistant without a GPU or inference server.** Today,
> CPU-only operation against the full assistant is not practical: the first
> message can take 15+ minutes to begin answering on Pi-class hardware,
> because the assistant's full tool catalog must be processed by the model
> first. We measured the underlying models themselves doing fine on simple
> requests — correct tool choice, honest refusals — so this limit is about
> prompt size, not model ability, and planned work (tool narrowing and
> prompt pre-warming) is aimed at making CPU-only mode genuinely usable for
> patient, single-step requests. Small models also state amateur-radio facts
> wrong with full confidence, so documentation grounding stays important at
> every size. A modest GPU or a network inference server avoids all of this
> today. Tuxlink never downloads or starts a model unless you configure one.

Cross-links: tuxlink-y28so (this test), tuxlink-nsnre (inert-Elmer affordance
+ docs, now unblocked), tuxlink-efk3k addendum 6 point 1, tuxlink-0mudm
(grounding), tuxlink-nyyr2 notes (thinking-suppression mechanism evidence).
