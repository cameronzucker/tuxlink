# Template+slots intake design — the spike's authoring surface, compiler, and instrument

**2026-08-19, bd tuxlink-3gaz7, session basil-redwood-cove. Operator-approved
design (sections approved in-session 2026-08-19) entering the five-round
adversarial review. Supersedes the refusal-channel sketch in
`dev/spikes/2026-08-13-ir-compiler-slice/SPEC-template-slots-spike.md`;
evidence base: `ALTERNATIVES.md`, the amended
`RESULTS-2026-08-18-insitu.md`, and the refusal-response survey
(`dev/scratch/2026-08-18-refusal-response-survey.md`).**

## Purpose

Single-arm sequential falsification of template+slots as Elmer's routine
authoring surface, per the operator's ruling: build the minimal feature
shape (intake tool + compiler + real harness port), evaluate ruthlessly
against pre-registered bars, and only on FAIL — by the operator's separate
decision — build a challenger. The subject model is whichever the operator
serves at evaluation time (Inkling-Small-NVFP4 or Qwen 3.8 27B; his ruling:
eval-equivalent), recorded with full provenance in the results.

## 1. The intake tool — `routine_template_compile` (testserver-only)

**Input:** exactly `{template: string, slots: object}`. Any other shape —
missing key, extra key, non-object slots, malformed JSON — is a HARD tool
error naming the offending key (the envelope class; models recover from
these cleanly, e.g. the live REVISION_CONFLICT single-shot recovery).

**Registry discovery:** the closed registry is enumerated in the tool
description itself — template ids plus one-line semantics each. Evidence:
pre-narrowed enumerated sets graded perfectly in the disambiguation evals;
the UNKNOWN_ACTION history rules out teach-by-rejection.

**Success result:** `{compiled, readback, completion}`.
- `compiled`: the real `RoutineDef` produced by the lowering.
- `readback`: the plain-language rendering, echoing every free-text slot
  VERBATIM — the defined refusal-smuggling channel: smuggled refusal prose
  becomes visible in the echo the model relays. Also closes the
  stale-summary class (the model summarizes from the result, not memory).
- `completion`: the report-and-stop sentence (the Laguna 37-edit polish
  loop lesson; ASCII-only per the 2026-07-29 mojibake ruling).

**Refusal result (structured, inside the result — the disposition
dialect):** `{blocked: true, blocked_by: [codes], findings: [...]}` where
each finding carries: `code` (SCREAMING_SNAKE), the offending template id
or slot name VERBATIM, the rule violated, and an op-shaped remedy ONLY when
the finding is blocking and agent-fixable; explicit fault attribution
("not your call's fault; retry later") on environmental classes. Field
name is `findings` uniformly (retiring the `routine_findings` drift). If
two findings can co-fire they carry equal-strength anchors and
cross-reference each other, or are merged (the 34-turn livelock lesson).
No explanatory prose hints (the kHz-hint regression; "prose is not a
fix"). No system-prompt additions anywhere. No model-side refusal fields
(zero supporting data; adjacent to the torn-out phase-pipeline shape).

## 2. The compiler and registry

**Registry (closed enum, three REAL templates** per the fixture-validity
doctrine — dual-purpose, ship-faithful):
1. `scheduled-connect-with-fallback` — the ratified sheet's template
   (name, every, window, stations, bands, success_log, failure_log,
   fail_reason).
2. `beacon-schedule` — periodic position announce; transmit-adjacent by
   design so selection errors are maximally visible.
3. `log-rotation` — housekeeping, no radio.
All three compile; distractors get minimal honest lowerings.

**Primary lowering** (grounded in `tuxlink-routines` types verified
2026-08-18): `every`/`window` → the engine's native schedule `Trigger`
(window is a first-class Option field); `bands` order → the connect-step
lowering; `success_log`/`failure_log` → log steps referencing the connect
step's declared outputs via compiler-generated `$sN` refs — the model
never sees an id or a reference (structurally deletes the
positional-guessing and id-surgery thrash classes); `fail_reason` → the
end control. Deterministic id generation.

**Leniency by normalization, never refusal, for natural spellings**
("every 15 minutes" → "15m"; band spellings) — the frequency-normalization
precedent (23/23 failures were unit spelling). Unknown template, unknown
slot, and un-normalizable values are named refusals per §1.

**Post-compile validation:** the compiler runs the real save→validate path
against the real engine, so genuine validator findings flow through the
SAME result shape — one refusal grammar end to end.

## 3. The harness

Replace the testserver's canned routines port with the real registry
catalog, real validator, and a temp-dir store; `routines_run` stays inert
with an honest out-of-scope error. Register the intake tool in the
testserver router only: the model observes a surface where the compiler
genuinely exists; the product binary and the parity manifest are
untouched. This satisfies the amended RESULTS' gating conclusion (the
valid instrument = this lane + a real intake tool + a non-canned port).

## 4. The evaluation instrument

**Cells** (2 samples each unless noted): N1 fresh-author, N2
inexpressible-pressure (retry + beacon), E1 additive edit, E2 subtractive
edit, C1 correction-from-refusal, S1/S2 template selection against the
distractor-bearing registry (correct template pre-registered per ask),
CTRL ×3 (no intake tool guidance — the native-surface baseline). Emission
is the intake tool call.

**Grading:** the compiler verdict is the primary mechanical grader
(accept/refuse + compiled-def semantic checks pre-registered per cell);
trace assertions (defection = any mutating native routine call; intake
calls counted); the mandatory eyeball row (standing discipline).

**Bars (operator-ratified defaults):** ≥90% of N/E/C runs
compiler-accepted with ask-correct semantics; zero defections; ≥80%
correct selection on S cells; refusal-channel gate — slot-text smuggling
visible in the echo in MORE than 2 runs = FAIL of the refusal-channel
design (1–2 = recorded finding). FAIL never auto-triggers the challenger;
that is the operator's decision.

**Constraint:** evaluation runs require serving (blocked until the
operator frees a model); build sessions need none.

## 5. Error and edge semantics

Envelope errors = hard tool errors. Compile/validate refusals = structured
results per §1. The MCP layer's byte-identical-call repeat-notice is
inherited. The tool is read-only with respect to the product: compilation
does not persist ANY routine to the store in evaluation mode unless the
cell asks for save semantics — the compile-vs-save boundary is explicit in
the tool description.

## 6. Process

This document → five-round adversarial review (Codex, per the operator's
approval) with findings fixed inline between rounds → spec self-review →
operator's written-spec review → `writing-plans` for the implementation
plan (TDD preambles, pitfalls checks, per-task file scoping) → sessions
1–2 build (no inference), session 3 evaluation (on serving). Merges by
steward on green CI; one Codex round on the compiler code before its PR
merges; thin handoffs pointing here per ADR 0031.
