# Template+slots intake design — the spike's authoring surface, compiler, and instrument

**2026-08-19, bd tuxlink-3gaz7, session basil-redwood-cove. Operator-approved
sections, amended through adversarial review (log at bottom). Evidence
base: `dev/spikes/2026-08-13-ir-compiler-slice/ALTERNATIVES.md`, the
amended `RESULTS-2026-08-18-insitu.md`, the refusal-response survey
(`dev/scratch/2026-08-18-refusal-response-survey.md`), plus the
specifically-cited eval and commit sources named inline.**

## Purpose and instrument scope

Single-arm sequential falsification of template+slots as Elmer's routine
authoring surface, per the operator's ruling. **Instrument scope, stated
precisely:** this lane plus a real intake tool plus a non-canned routines
port is the valid instrument per the amended RESULTS' gating conclusion
(operator fact-check, 2026-08-18) — it measures the feature's authoring
seam in a production-shaped surface. It does NOT measure the full shipped
Elmer environment; the declared residual divergences are: the driver is
d3zwe (not the in-app pane), no prior conversation context, a temp-dir
store, and inert routine execution. Conclusions are scoped accordingly.
The subject model at evaluation time is whichever the operator serves
(Inkling-Small-NVFP4 or Qwen 3.8 27B) — **an operator serving policy, not
an empirical equivalence claim**; results are single-model with full
provenance and are never pooled across models without a bridge experiment.

## 1. The intake tool — `routine_template_compile` (testserver-only)

**Input:** `{template: string, slots: object, save: bool (default false)}`.
Envelope violations — missing/extra keys, non-object slots, malformed
JSON — are HARD tool errors naming the offending key, or the parse
location when no key exists. Evidence scope stated honestly: single-shot
recovery from this class is proven for the live REVISION_CONFLICT case;
recovery from each envelope-error class is a MEASURED outcome of this
instrument (recorded per run), not an assumed one.

**Result states (three, explicit — no others):**
- `compiled` — valid, NOT persisted. Carries `compiled` (the real
  RoutineDef), `readback`, and a completion sentence that states plainly
  that nothing was saved ("The definition is valid and compiled. It has
  NOT been saved...").
- `saved` — only when `save: true`: persists to the (temp-dir) store via
  the real save path and carries `revision` plus a saved-state completion
  sentence. Only this state may instruct the model to report persistence.
- `refused` — the full existing disposition envelope (below).

This three-state contract resolves the compile-vs-save boundary: validation
runs WITHOUT persistence by default (the real validator is invocable
without save); persistence is explicit, transactional, and revision-
bearing. The stale-summary protection claim is scoped accordingly: the
result the model relays IS the authoritative state for that result's kind,
and only `saved` results speak of saved state.

**Registry discovery:** the closed registry is enumerated in the tool
description — template ids, one-line semantics, AND **one worked example
call per template plus one worked refusal example** (the examples-outrank-
prose finding, lnctz 4-reader study). This tool description is the
candidate's instruction artifact, used unchanged in evaluation and, if the
premise survives, carried toward shipping. Precedent citations, correctly
sourced: pre-narrowed candidate sets graded cleanly in the CPU-viability
evals (`dev/evals/2026-08-10-cpu-only-elmer-viability.md`); enumerate-the-
valid-set-rather-than-teach-by-rejection is the UNKNOWN_ACTION precedent
(commit `cbefa047`). Both are precedents, not proof — selection quality is
a primary measured outcome (S cells).

**Refusal shape:** the FULL existing `AuthoringDispositionDto` envelope,
reused from `tuxlink-mcp-core/src/ports.rs` — `state`
(valid/invalid-agent-repairable/saved-needs-operator), `agent_terminal`,
`blocked_by`, `acceptable_warnings`, `advisories`, `remedies[]` (actor-
split, op-shaped, blocking-and-agent-fixable only), `completion` — plus
`findings[]` (uniform name). Each finding: code, offending template id or
slot name verbatim (position = the slot), the rule, fault attribution on
environmental classes. Co-firing findings carry equal-strength anchors and
cross-references, or are merged. No explanatory prose hints. No system-
prompt additions. No model-side refusal fields. (ASCII-only completion
copy is a standing operator constraint — the 2026-07-29 mojibake ruling
recorded at the advisory-completion site in ports.rs — not a conclusion of
this evidence package.)

## 2. The compiler and registry

**Registry v0 (closed enum, three templates, ALL fully specified):** the
registry is the spike's instrument registry; shipping it later is a
separate reviewed registry change (the sheet's own growth rule).

1. `scheduled-connect-with-fallback` (primary): slots `name` (kebab-case
   string), `every` (duration or null), `window` ("HH:MM-HH:MM" or null),
   `stations` (station-set ref string), `bands` (ordered band list),
   `success_log` (text; may use $band/$station), `failure_log` (text),
   `fail_reason` (text). Lowering: every/window → the engine's schedule
   `Trigger` (window native); bands order → connect lowering; logs → log
   steps referencing the connect step's declared outputs via COMPILER-
   generated `$sN` refs; fail_reason → end control. Deterministic ids.
2. `beacon-schedule` (distractor, transmit-adjacent): slots `name`,
   `every` (duration, required), `window` (or null), `message` (text).
   Lowering: schedule trigger + one `radio.aprs_send` step with the
   message; honest and minimal.
3. `log-rotation` (distractor, no radio): slots `name`, `every` (duration,
   required), `note` (text). Lowering: schedule trigger + one `local.log`
   step.

**Normalization (finite, per-slot alias table — published in the design,
enforced in code):** durations: "N minutes/mins/min" → "Nm", "N hours/hrs/
hr" → "Nh", "N seconds/secs/sec" → "Ns" (unambiguous forms only); bands:
"NN meters/meter/m band" → "NNm" for registered band labels only.
Ambiguous or unregistered values → positioned refusal, never a guess. The
measured precedent (23/23 unit-spelling failures, tuxlink-0rc3h) supports
UNIT normalization specifically; the band table is a design extension in
the same spirit, and its alias hits/misses are recorded per run.

**Post-compile validation:** the compiler invokes the real validator
(validate-without-save for `compiled`; the real save path for `saved`), so
genuine validator findings ride the same disposition envelope — one
refusal grammar end to end.

## 3. The harness

Real routines port in the testserver (real registry catalog, real
validator, temp-dir store). `routines_run` is NOT silently inert: it
returns a structured environmental refusal with explicit fault attribution
and availability copy — "execution is out of scope in this harness (not
your call's fault); authoring, compiling, and saving remain available" —
the deny-copy abandonment lesson (tuxlink-shopf) applied; a trace
assertion watches for post-refusal abandonment. The intake tool registers
in the testserver router only; product binary and parity manifest
untouched.

## 4. The evaluation instrument

**Arms:** one intervention arm plus a control. **CTRL is defined
mechanically:** the identical testserver launched WITHOUT the intake tool
registered (a launch flag) — same prompt, same asks, same everything else;
the only difference between arms is the intake surface's existence.

**Cells** (samples noted): N1 fresh-author ×2, N2 inexpressible-pressure
×2, E1 additive edit ×2, E2 subtractive edit ×2, C1 correction-from-
refusal ×2, S1/S2 selection ×3 each (raised from 2 so the bar is
representable), CTRL ×3. The narrow-serving-distribution caveat is
standing: same-cell samples measure stability of one mode, not diversity.

**Pre-registered expected disposition PER CELL** (the round-1 blocker
fix — refusal-expected cells are never pooled into an acceptance rate):
- N1, E1, E2, C1: `compiled`, ask-correct semantics. Bar: 2/2 each.
- N2: `compiled` with correct OMISSIONS — expressible slots ask-correct,
  no invented slots, and the inexpressible asks (retry, beacon) absent
  from all slot text; refusal/acknowledgment content belongs in the
  model's prose. Graded on this predicate alone. Bar: 2/2.
- S1, S2: correct template selected. Bar: 5/6 across the six runs
  (discrete requirement stated, not a percentage).
- CTRL: not graded against IR bars; baseline behavior recorded.

**Smuggling metrics, split** (round-1 fix — detection is not failure):
(a) smuggling INCIDENCE (refusal-prose or inexpressible-ask content in
slot text) — surface vulnerability; bar: ≤2 runs across the matrix, >2 =
surface FAIL; (b) echo-visibility — every smuggled string must appear
verbatim in the readback; any miss = channel implementation bug (fix and
re-run, not a model finding); (c) relay honesty — whether the model's
final message discloses omissions; recorded, informs the ruling.

**Other trace assertions:** zero defections to mutating native routine
tools in sheet cells; intake-call count ≥1 per sheet cell; post-
environmental-refusal abandonment watch (§3).

**Grading:** compiler verdict + per-cell semantic checks as the mechanical
grader; the mandatory eyeball row stands. FAIL on any bar → the ruling
brief analyzes the failure shape; whether a challenger gets built is the
operator's decision, never automatic.

**Constraint:** evaluation requires serving (operator frees a model);
build sessions need none.

## 5. Error and edge semantics

Envelope errors = hard tool errors (per-class recovery measured). Refusals
= the full disposition envelope. The MCP layer's byte-identical-call
repeat-notice is inherited. The compile-vs-save boundary is explicit in
the tool description and in the completion copy of each result state.

## 6. Process

Five-round Codex adversarial review of this design with inline fixes
(this log, below) → spec self-review → operator's written-spec review →
writing-plans → sessions 1–2 build (no inference) → session 3 evaluation
(on serving). Merges by steward on green CI; one Codex round on the
compiler code before its PR merges; thin handoffs per ADR 0031.

## Adversarial review log

**Round 1 (Codex, evidence coherence — 17 findings: 2 BLOCKER, 13 MAJOR,
2 MINOR; raw transcript local at
`dev/adversarial/2026-08-19-tslots-design-r1-codex.md`).** All accepted;
dispositions: instrument-scope claim narrowed with declared residues (F1);
per-cell expected dispositions replace the pooled 90% bar (F2); full
AuthoringDispositionDto envelope reused instead of a reduced dialect (F3);
envelope-recovery claim narrowed to the proven case + measured per class
(F4); registry-discovery citations corrected to their true sources and
demoted to precedent (F5); three result states resolve the compile/save
incoherence and completion-copy hazard (F6-F8); distractor templates fully
specified + registry labeled instrument-v0 (F9); finite normalization
table + narrowed precedent attribution (F10); worked examples added to the
tool description as the instruction artifact (F11); routines_run refusal
given fault attribution + availability copy + abandonment assertion (F12);
CTRL defined mechanically as tool-absent launch (F13); discrete bars
replace percentages, S samples raised to 3 (F14); smuggling metrics split
into incidence/echo-visibility/relay-honesty (F15); model choice restated
as operator policy (F16); ASCII-only sourced to the 2026-07-29 operator
ruling (F17).
