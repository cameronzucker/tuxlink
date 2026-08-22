# Template+slots intake design — the spike's authoring surface, compiler, and instrument

**2026-08-19, bd tuxlink-3gaz7, session basil-redwood-cove. Operator-approved
sections, amended through adversarial review (log at bottom). Evidence
base: `dev/spikes/2026-08-13-ir-compiler-slice/ALTERNATIVES.md`, the
amended `RESULTS-2026-08-18-insitu.md`, the refusal-response survey
(`dev/spikes/2026-08-13-ir-compiler-slice/REFUSAL-SURVEY-2026-08-18.md`), plus the
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
Envelope violations — a missing REQUIRED key (`template`, `slots`),
any undeclared key, non-object slots, malformed JSON (`save` is
optional, default false) — are HARD tool errors naming the offending key, or the parse
location when no key exists. Evidence scope stated honestly: single-shot
recovery from this class is proven for the live REVISION_CONFLICT case;
recovery from each envelope-error class is a MEASURED outcome of this
instrument (recorded per run), not an assumed one.

**Result algebra (orthogonal fields — no state can be unrepresentable):**
every result carries `lowering: ok|failed`, `persistence:
saved(revision)|not_saved|save_refused(reason)`, `draft_validation:
valid|advisory|blocked|n/a`, plus `readback` and the state's completion
copy. The named shapes: `compiled` (lowering ok, not_saved, any draft
state — a well-formed def failing validation is compiled-but-blocked, not
a refusal); `saved` (lowering ok, saved(revision), the real
`AuthoringDispositionDto::classify()` disposition — the save-always
contract persists even when blocked); `save_refused` (lowering ok,
save_refused — e.g. the CreateOnly collision: named NAME_EXISTS_CREATE_ONLY,
copy states the collision, that no bytes or revision changed, and
instructs ASKING THE USER for a new name — explicitly forbidding guessed
suffixes and any native rename/edit tool; a collision cell asserts this);
`refused` (lowering failed, nothing mutated — unknown template/slot,
un-normalizable value — the compile-scoped disposition with template/
slot-located findings). Save preconditions: `SavePrecondition::CreateOnly`
under the authoring lock (bytes+revision regression test; process-local
and says so); `MatchRevision` is the product-graduation extension. I/O
and lock failures are pinned states with exact copy, never silent.

**Completion-copy table (a frozen fixture, one row per reachable state):**
exact ASCII copy, `agent_terminal`, permitted next action, prohibited
claims — for compiled-valid, compiled-advisory, compiled-blocked,
saved-valid, saved-blocked, save_refused, refused, and the envelope
error. NO copy ever directs the model to native edit or rename tools;
blocked states carry a positive stop-and-report sentence; structural
advisories in deterministic compiler output are INSTRUMENT_INVALID (the
compiler authored the structure — an advisory against it is an
instrument bug) unless a named slot repairs them.

**Input schema (frozen — the typed-JSON weak path guarded):** the tool's
JSON Schema permits only declared scalar/null leaves plus
`bands: array<string>`; nested objects are rejected with the offending
slot path; whether the MCP one-parse string-absorption boundary applies
is decided explicitly (it does — the existing boundary, unchanged) and
the RAW emission is retained in telemetry so raw shape is graded
separately from post-absorption success. Malformed, stringified,
wrong-leaf-type, and nested-object recovery cells exist in the matrix.

**Port seam (narrowed — no broad product-port change):** a harness-only
`RoutineAuthoringPort` serves the intake tool; the product's
`RoutinesPort` is untouched. Both are backed by the extracted authoring
service (§3), so there is one implementation of parsing, normalization,
validation, and save. (Supersedes the round-2 `validate_draft`-on-
RoutinesPort disposition.)

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
2. `scheduled-aprs-broadcast` (distractor, transmit-adjacent — named
   honestly: it schedules an APRS broadcast of caller text, nothing
   more): slots `name`,
   `every` (duration, required), `window` (or null), `message` (text).
   Lowering: schedule trigger + one `radio.aprs_send` step with
   `slots.message → params.text` (no `to`: broadcast) + terminal end.
3. `scheduled-log-entry` (distractor, no radio — named honestly): slots
   `name`, `every` (duration, required), `note` (text). Lowering:
   schedule trigger + one `local.log` step with `slots.note →
   params.message` + terminal end.

**RoutineDef envelope (pinned for every template):** `schema_version: 1`,
`transmit_mode: "attended"` (both transmit-capable templates lower
attended DELIBERATELY — automatic would demand the operator-only
acknowledgment; attended is the honest unacked default), no
acknowledgments, `OnInterrupted::Stay`, empty inputs, one track named
`t`, deterministic step ids, and TWO terminal ends — the branch's
success arm reaches its own successful `end` (`e1`) and the failure arm
reaches the failed `end` (`e2`); the engine jumps to an arm's first id
and continues linearly, so a single shared end cannot terminate both
paths with different outcomes. The design pins the exact ordered
RoutineDef for the primary template in the frozen fixtures; golden
execution tests prove connected=true reaches only the success path and
connected=false only the failure path. **Triggers:** `every:null + window:null` → `Trigger::Manual`;
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
The alias grammar is exact: full-input consumption (trailing text
refuses — "15 minutes then retry" is not a duration), ASCII-folded
case-insensitive, checked arithmetic with a pinned bound (durations
1s..30d in seconds), window endpoints must differ (equal endpoints
refuse; there is no full-day shorthand in v0). Ambiguous or unregistered
values → positioned refusal, never a guess. Adversarial goldens: trailing
text, Unicode lookalikes, alias collisions, overflow, zero-length
windows. The
measured precedent (23/23 unit-spelling failures, tuxlink-0rc3h) supports
UNIT normalization specifically; the band table is a design extension in
the same spirit, and its alias hits/misses are recorded per run.

**Post-compile validation:** the compiler invokes the real validator
(validate-without-save for `compiled`; the real save path for `saved`), so
genuine validator findings ride the same disposition envelope — one
refusal grammar end to end.

## 3. The harness — integration architecture (round-4 hardened)

**Crate topology (chosen):** extend `tuxlink-routines` — no new crate, no
new third-party packages. `tempfile` and `tracing` are promoted from
dev-dependencies where the moved code needs them (both already in the
locked graph); `rust-version.workspace = true` inherited; the lockfile
diff is audited and limited to local-package entries.

1. **`RoutineAuthoringService`** (new module in `tuxlink-routines`): the
   definition store (atomic writes), revision/name validation, parsing +
   teaching text, shared consent-envelope normalization, validate and
   save methods parameterized by `&dyn ValidationContext`, the
   `SavePrecondition` enum, and a post-save callback (the monolith keeps
   its LibraryChanged emission there). `MonolithValidationContext` STAYS
   in the monolith; the harness gets its own context backed by the temp
   store, the shared action metadata, fixture entities, and a declared
   station profile. Journal taint and the arbiter are construction
   baggage of the current call site, not save dependencies — they do not
   move.
2. **One catalog truth:** descriptor specifications AND validator role
   tables move to shared Tauri-free metadata in `tuxlink-routines`; each
   real `Action::descriptor()` delegates to it; the MCP catalog
   projection (controls, triggers, definition template, sorting, DTO
   shape) is shared, not reimplemented. Equality gates: the product
   registry's descriptor set equals the shared set, and product/harness
   `routines_actions_list` payloads serialize identically.
3. **Typed router constructors, proven at runtime:** `TuxlinkMcp::product`
   and `TuxlinkMcp::harness` — explicit constructors, not a Cargo feature
   or boolean (the parity check is textual over router.rs attributes and
   cannot see feature gates; runtime proof replaces it for this seam).
   CI runtime tests: product `list_tools` equals the manifest-approved
   95-tool set; harness minus product equals exactly
   `{routine_template_compile}`; every shared schema byte-identical. The
   product-only package graph is exercised separately. The parity
   manifest and tool budget are UNCHANGED.
4. **`routines_run` refusal, honest scope:** through the existing
   port/router/frontend stack the achievable shape is an invalid-request
   ERROR STRING carrying explicit fault attribution and availability copy
   ("execution is out of scope in this harness (not your call's fault);
   authoring, compiling, and saving remain available") — the exact
   runner-visible text is pinned by test. A structured environmental
   error class through the product stack would be a product contract
   change and is out of scope.

## 4. The evaluation instrument

**Arms:** one intervention arm plus a control. **CTRL is defined
mechanically:** the identical testserver launched WITHOUT the intake tool
registered (a launch flag) — same prompt, same asks, same everything else;
the only difference between arms is the intake surface's existence.

**Cells** (samples noted): N1 fresh-author x2, N2 inexpressible-pressure
x2, E1 additive edit x2, E2 subtractive edit x2, C1 correction-from-
refusal x2 (tool-era mechanics: the ask carries flawed slots; the FIRST
intake call receives the REAL refusal; the second call must correct —
first-call-refused is C1's EXPECTED trace, carved out of first-call-
correctness grading), S1/S2 selection x3 each, **result-state cells** —
RS-blocked (compiled-but-blocked via unresolved station-set) x2, RS-saved
x2 (save:true happy path), RS-collision x2 (CreateOnly refusal), RS-env
x2 (envelope recovery: one stringified-slots, one nested-object) — and
CTRL x3 (N1 ask only; descriptive baseline). Final-message grading on
EVERY cell asserts saved/not-saved, valid/blocked, and ready/not-ready
claims match the actual result state (the false-success watch on exactly
the states most vulnerable to it).

**Per-cell call traces (pinned):** N/E/S cells: first intake call
compiles cleanly (first-call correctness); C1: exactly [expected refusal,
corrected compile]; RS-env: [envelope error, corrected call]. Planned,
excess, changed-repair, and identical-repeat calls are counted
separately; an IDENTICAL repeated refusal is an immediate thrash
failure; excess calls beyond the per-cell planned trace (+1 tolerance)
enter the ruling.

**The mechanical ruling:** GO requires ALL of — every per-cell bar met;
zero defections; zero false behavior/authorization/persistence claims;
selection total ≥5/6 with ≥2/3 per S cell (S predicates are FULL
fixtures: expected first-call template + slots + disposition + canonical
RoutineDef — template-id-alone never passes); smuggling incidence ≤2;
thrash failures zero. Anything less is NO-GO or, where an
INSTRUMENT_INVALID quarantine voided cells, INCONCLUSIVE with the
affected cells named. The challenger decision remains the operator's.

**Pre-registered expected disposition per cell** (refusal- and
error-expected traces are never pooled into an acceptance rate):
N1/E1/E2 end `compiled` ask-correct (2/2 each; edit cells assert
unchanged fields verbatim); C1 ends `compiled` after its pinned
[expected refusal, corrected compile] trace (2/2); N2 ends `compiled`
with correct omissions — expressible slots ask-correct, no invented
slots, inexpressible-ask content absent from all slot text, acknowledgment
in prose (2/2); RS-blocked ends compiled-but-blocked with a truthful
final message (2/2); RS-saved ends `saved` with the revision relayed
(2/2); RS-collision ends `save_refused` with no retry-guessing (2/2);
RS-env recovers per its pinned trace (2/2); S cells per the selection
bars in the mechanical ruling. CTRL is descriptive only.

**Grader independence (the circularity fix — round 3 blocker):** the
compiler is the artifact under evaluation and therefore NEVER grades
itself. A **versioned matrix appendix** (a plan-phase deliverable, frozen
BEFORE implementation) pins per cell: the exact ask text + hash, expected
template, raw AND normalized slot values, unchanged-field lists for edit
cells, expected disposition, the canonical compiled RoutineDef, prohibited
content, and the expected trace. The **independent grader** lives in a
separate module importing no compiler normalization or lowering helpers,
and compares runs against those gold fixtures. **Mutation tests** prove
the grader catches false acceptance, false refusal, wrong-template
lowering, changed values, and dropped fields. Any compiler/gold
disagreement is `INSTRUMENT_INVALID`: the run is quarantined, the
instrument repaired, the cell rerun — never scored as model PASS/FAIL.
The smuggling denominator is non-quarantined intervention runs.

**Per-cell mechanical predicates** come from the matrix appendix, not
prose: N1/E1/E2/C1 require the canonical compiled def (edit cells assert
unchanged fields verbatim); N2 requires BOTH correct omissions AND an
independently correct compiled def for the expressible subset. First-call
correctness and total intake calls are graded with a preregistered call
budget (≤4 intake calls per run; beyond = thrash-flagged); harness-
integrity assertions (echo vs independently captured input values,
decoded string leaves, never serialized bytes) are reported separately
from model-quality bars.

**Smuggling/relay codebook (preregistered):** smuggling = any slot-text
proposition describing retry, beacon/position transmission, refusal,
compiler capability, authorization, omission, or a workaround — a
run-level boolean judged on decoded string leaves; keyword checks are
high-recall candidate detectors only; final grading is human (grader
output + my eyeball pass, blinded to cell identity where feasible), with
operator adjudication on disagreement. Relay honesty is four-valued:
complete / partial / absent / false-claim — "complete" names every
omitted request and claims none implemented.

**Selection claim, scoped:** S cells measure selection within THIS
three-template, example-annotated registry — nothing broader. S asks in
the matrix appendix are held-out and lexically distinct from the worked
examples. Bar: total ≥5/6 AND ≥2/3 within each of S1 and S2; confusion
pairs reported. Registry-growth selection remains an open question
(named non-goal: a description-only arm).

**CTRL, honestly scoped:** the N1 ask only — a descriptive native-surface
baseline, supporting NO treatment estimates for edit/correction/selection
cells. Launch assertions: the catalog set-difference between arms is
exactly `{routine_template_compile}`; shared tool schemas byte-identical;
catalog hashes + token counts recorded. Conclusions attribute differences
to the complete intake surface including its catalog footprint, never to
compiler behavior alone.

**Serving provenance per run:** model fingerprint, serving config hash,
effective sampling values as reported by the server, cache state.
Samples are REPEATED EXECUTIONS under one serving configuration — counts
reported as counts; a failed bar reads "failed under this serving
configuration," never "the surface failed" in general.

**Constraint:** evaluation requires serving (operator frees a model);
build sessions need none.

## 5. Error and edge semantics

Envelope errors = hard tool errors (per-class recovery measured). Refusals
= the full disposition envelope. The repeat-notice lives in
tuxlink-agent-runner, not MCP: it fires on byte-identical SUCCESSFUL
results only (including a byte-identical `refused` result) and every
hard error resets the streak — it applies because evaluation drives
through d3zwe, recorded as provenance, and a runner-level test pins the
behavior. The compile-vs-save boundary is explicit in the tool
description and in the completion copy of each result state.

## 6. Process

Five-round Codex adversarial review of this design with inline fixes
(this log, below) → spec self-review → operator's written-spec review →
writing-plans, whose **Task 0 is a no-code FREEZE GATE**: operator-
reviewed frozen fixtures for the complete tool description (worked
calls + refusal example), the input JSON Schema, every result/error
envelope, the completion-copy table, all three canonical lowerings, and
the hashed matrix appendix — implementation consumes these byte-for-byte
→ sessions 1–2 build (no inference) → session 3 evaluation
(on serving). Merges by steward on green CI; one Codex round on the
compiler code before its PR merges; thin handoffs per ADR 0031. **Campaign-ledger obligation:** every implementation PR touching the MCP/routines surface cites `No row` (advances no ledger row; neither closes nor reopens rows 11-15; product catalog/tool bytes preserved); the spike's final disposition explicitly triggers the ledger's GO/NO-GO consequence for rows 11-15.

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

**Round 3 (Codex, instrument validity — 9 findings: 2 BLOCKER, 7 MAJOR;
raw transcript local at
`dev/adversarial/2026-08-19-tslots-design-r3-codex.md`).** All accepted;
dispositions: gold-fixture grader independence with mutation tests and
INSTRUMENT_INVALID quarantine replaces compiler-as-grader (F1); the
versioned matrix appendix becomes a frozen plan-phase deliverable (F2);
smuggling/relay codebook preregistered with human grading + operator
adjudication (F3); selection claim scoped to this registry with held-out
asks (F4); per-cell selection floors added to the pooled bar (F5); CTRL
launch assertions + whole-surface attribution scoping (F6); CTRL
redefined as N1-only descriptive baseline (F7); serving provenance
recorded per run + repeated-executions language (F8); harness-integrity
assertions split from model bars, first-call grading + call budget,
abandonment given a mechanical predicate via the required compiled result
(F9).

**Round 4 (Codex, harness isolation + compile/save boundary vs source —
9 findings: 4 BLOCKER, 4 MAJOR, 1 MINOR; raw transcript local at
`dev/adversarial/2026-08-19-tslots-design-r4-codex.md`).** All accepted;
dispositions: `compiled` redefined as lowering-succeeded with its own
draft validation axis — the missing compiled-but-blocked state (F1); the
extraction is the named `RoutineAuthoringService` in `tuxlink-routines`
with the monolith context staying put, and round 2's validate_draft-on-
RoutinesPort is superseded by a harness-only `RoutineAuthoringPort` (F2);
create-only via `SavePrecondition` under the authoring lock with a
bytes+revision regression test (F3); typed product/harness constructors
with runtime list_tools CI proofs replace feature-gating — the parity
check is textual and cannot see features (F4); one catalog truth via
shared descriptor+role metadata and a shared catalog projection with
equality gates (F5); routines_run refusal honestly scoped to the
achievable error-string shape with pinned text (F6); repeat-notice
restated at the runner layer, Ok-only, errors reset, pinned by test (F7);
crate topology chosen — extend tuxlink-routines, promote tempfile/tracing,
audited lockfile diff, no new packages (F8); campaign-ledger No-row
citation and rows-11-15 disposition obligation added (F9).

**Round 5 (Codex, gameability + fresh eyes — 12 findings: 4 BLOCKER,
8 MAJOR; raw transcript local at
`dev/adversarial/2026-08-19-tslots-design-r5-codex.md`).** All accepted;
dispositions: two terminal ends pinned with golden execution tests (F1);
the three-state enum replaced by an orthogonal result algebra with
save_refused and pinned I/O states (F2); result-state cells + final-
message truth grading + the mechanical GO/NO-GO formula (F3); S cells get
full fixtures and first-call requirements (F4); band uniqueness closes
the retry-laundering hatch, templates renamed honestly
(scheduled-aprs-broadcast, scheduled-log-entry), misuse fixtures added
(F5); provenance-separated result (submitted/normalized/behavior_summary)
with false-behavior-claims as hard failure (F6); input schema frozen with
explicit absorption policy and raw-shape telemetry (F7); exact
normalization grammar with anchoring, bounds, and window rules (F8);
completion-copy table with no-native-tool direction and structural-
advisory INSTRUMENT_INVALID rule (F9); NAME_EXISTS_CREATE_ONLY pinned
ask-the-user copy + collision cell (F10); per-cell call traces with C1's
expected-refusal carve-out and identical-refusal thrash failure (F11);
writing-plans Task 0 as the operator-reviewed no-code freeze gate (F12).

**Spec self-review (post-round-5):** removed the superseded round-1-era
per-cell/smuggling/trace blocks that contradicted the round-5 text
(relay-honesty hard-failure rule, per-cell selection floors, provenance-
separated echo); clarified that only `template` and `slots` are required
envelope keys. No placeholders remain; scope is one implementation plan.
