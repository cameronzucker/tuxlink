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

**Result states (three, explicit — states reflect MUTATION FACTS):**
- `compiled` — valid or advisory-clean per the draft validator, NOT
  persisted. Carries `compiled` (the real RoutineDef), `readback`
  (serialized compiler output, never a store read), and a completion
  sentence stating plainly that nothing was saved.
- `saved` — only when `save: true`. **Create-only for the spike**: an
  existing name is refused (the lost-update guard without carrying
  `expected_revision`; the revision-precondition extension is noted for
  product graduation). Uses the real save path — which by contract saves
  regardless of validator findings — so `saved` means PERSISTED and may
  carry a non-valid disposition (`blocked_by` + findings ride along); the
  real `AuthoringDispositionDto::classify()` applies here, where its
  save-centric states are true. Only `saved` results speak of persistence.
- `refused` — NOTHING mutated. Compile-stage refusals (unknown template,
  unknown slot, un-normalizable value, envelope-adjacent semantic errors)
  use a COMPILE-SCOPED disposition: same dialect family as the existing
  envelope, but with honest states — `refused-agent-repairable` (the agent
  can fix the call; nothing was saved; no operator needed) — and findings
  carrying `template`/`slot` locations (a CompileFindingDto mirroring
  FindingDto's shape). Compile refusals are NEVER fed through the existing
  `classify()`, whose states describe stored-routine authoring and would
  misreport an unknown slot as needs-operator.

**Port addition (shared crate, no product-tool change):**
`RoutinesPort::validate_draft(def)` — unsaved-definition validation with
the same consent normalization as the app's UI-only draft validator; the
existing port only validates stored names, so the compiled state needs
this seam. The product ROUTER's tool list is unchanged (CI's parity/
tool-budget tests prove it).

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

**Refusal shape summary:** save-path results reuse the full existing
`AuthoringDispositionDto` envelope verbatim (its states are true there);
compile-stage refusals use the compile-scoped disposition above. Both
carry `findings[]` (uniform name); each finding: code, offending entity
verbatim (template id or slot name; position = the slot), the rule, an
op-shaped remedy only when blocking and agent-fixable, fault attribution
on environmental classes. Co-firing findings: equal-strength anchors +
cross-references, or merged. No explanatory prose hints. No system-prompt
additions. No model-side refusal fields. (ASCII-only completion copy is
the standing 2026-07-29 operator ruling recorded at the
advisory-completion site in ports.rs.)

## 2. The compiler and registry

**Registry v0 (closed enum, three templates, ALL fully specified):** the
registry is the spike's instrument registry; shipping it later is a
separate reviewed registry change (the sheet's own growth rule).

1. `scheduled-connect-with-fallback` (primary): slots `name` (kebab-case
   string), `every` (duration or null), `window` ("HH:MM-HH:MM" or null),
   `stations` (station-set ref string), `bands` (ordered band list,
   REQUIRED non-empty — an empty list selects packet dialing and returns a
   null band, so `$band` would be unresolvable), `success_log`,
   `failure_log`, `fail_reason`. **Lowering (pinned):** ONE `radio.connect`
   step (`stations` + normalized `bands`; fallback order is the engine's:
   station-major, then band order within each station) → a `branch` on
   `$sN.connected` → `then`: the success log step (`$station`/`$band`
   rewritten to `$sN.station`/`$sN.band`) → `else`: the failure log step
   then the failed `end` with `fail_reason`. Failure-arm text must not
   reference connect outputs (a failed connect returns only
   `connected:false` + `last_error`; missing output paths are hard runtime
   errors).
2. `beacon-schedule` (distractor, transmit-adjacent): slots `name`,
   `every` (duration, required), `window` (or null), `message` (text).
   Lowering: schedule trigger + one `radio.aprs_send` step with
   `slots.message → params.text` (no `to`: broadcast) + terminal end.
3. `log-rotation` (distractor, no radio): slots `name`, `every` (duration,
   required), `note` (text). Lowering: schedule trigger + one `local.log`
   step with `slots.note → params.message` + terminal end.

**RoutineDef envelope (pinned for every template):** `schema_version: 1`,
`transmit_mode: "attended"` (both transmit-capable templates lower
attended DELIBERATELY — automatic would demand the operator-only
acknowledgment; attended is the honest unacked default), no
acknowledgments, `OnInterrupted::Stay`, empty inputs, one track named
`t`, deterministic step ids (`s1..sN`, `e1`), explicit terminal end on
every path. **Triggers:** `every:null + window:null` → `Trigger::Manual`;
`every:null + window:some` → REFUSED (a window is unrepresentable on a
manual trigger); schedules emit `if_missed: Skip` and `align: None`
explicitly. **The compiler itself enforces** positive, bounded integer
durations and valid clock ranges — the engine's validator does not check
interval syntax and the scheduler silently never fires on malformed or
non-positive intervals; golden tests cover zero, negative, overflow,
malformed, and out-of-range windows.

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

## 3. The harness — with the integration architecture stated

The real routines port cannot be dropped into the testserver as-is: the
monolith's port impl needs a Tauri AppHandle, the action DESCRIPTORS live
in the monolith, and the router's tool set is statically generated. The
design therefore owns this architecture explicitly:

1. **Extract a Tauri-free authoring core** (new small crate or an extension
   of `tuxlink-routines`): the action descriptor specs (params/outputs for
   `radio.connect`, `radio.aprs_send`, `local.log`, and the rest of the
   catalog), a file-backed store usable against a temp dir, the
   `ValidationContext` wiring, and `validate_draft`. The monolith's port
   impl and the testserver both consume it — one source of truth, no
   catalog drift.
2. **Testserver wiring:** the testserver gains the routines-authoring dep
   and serves the REAL catalog/validator/store through `RoutinesPort`.
3. **The intake tool registers via a router constructor flag** (or
   feature) enabled only by the testserver binary. The product binary's
   tool list is UNCHANGED, proven by the existing CI parity-manifest and
   tool-budget tests (an accidental product exposure fails `verify`).
4. `routines_run` returns the structured environmental refusal with fault
   attribution and availability copy ("execution is out of scope in this
   harness (not your call's fault); authoring, compiling, and saving
   remain available"); a trace assertion watches for post-refusal
   abandonment.

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

**Round 2 (Codex, engine/lowering semantics vs source — 11 findings:
4 BLOCKER, 6 MAJOR, 1 MINOR; raw transcript local at
`dev/adversarial/2026-08-19-tslots-design-r2-codex.md`).** All accepted;
dispositions: single connect step + explicit branch lowering with
`$sN.*` refs, station-major fallback documented, non-empty bands required
(F1, F2); `RoutinesPort::validate_draft` added as the unsaved-validation
seam (F3); compile-scoped disposition replaces dishonest reuse of the
save-centric state machine — round 1's full-reuse fix was overcorrected
and is now split by mutation fact (F4, F5); save is create-only for the
spike (F6); exact param mappings pinned (message→text, note→message)
(F7); every:null+window refused, if_missed/align pinned (F8); compiler
enforces duration/window validity — the scheduler silently never fires
otherwise (F9); full RoutineDef envelope pinned incl. attended-by-design
for transmit templates (F10); the Tauri-free authoring-core extraction
plus flag-gated router registration is now an explicit design component
with CI proof of an unchanged product tool list (F11).
