# Lift-corrected-1: failure-modality analysis (WHY, not just what)

Date: 2026-07-24
Author: tanager-owl-cardinal
Run: `battery-results/lift-corrected-1` on R2 (`~/tuxlink-eig6e-build`), binary
built from origin/main `0ae53b5e` (code-identical to `1e07aa15`).
Model: qwen35-122b-nvfp4 (keyless local vLLM on the Spark). Arms: base + skill.
Scope: 14 of 36 cells completed before the run was stopped (P1-S4, both arms;
plus a killed `base/A1`). This document analyzes the *mechanism* of each
non-PASS rung, classifies each modality as a Tuxlink product issue vs a model
issue vs a harness/instrument issue, and proposes fixes. It does NOT re-run.

## Method + a hard caveat on evidence

There is **no captured chain-of-thought**. The transcript logs `User`,
`ToolCall`, `ToolResult`, and the final `Assistant` message only. qwen ran
tool-calls-only; `base/S1` has *zero* assistant messages across all 40 turns.
So "the reasoning" reconstructed here is the **decision trajectory** (each call,
what it saw back, its next call) plus the final self-report. That is enough to
read the mechanism, but the absence of intermediate reasoning is itself a
finding (M7). All `ToolResult` error bodies were read verbatim; the `ok=False`
results are actionable, not bare (this is load-bearing for classification).

## Verdicts recap (hand-judged, judge-primary; deterministic layer is inert)

The scorer's `deterministic_verdict` only emits pass/fail for
`classification == "BUILDABLE"`, and every corpus cell has `classification: ""`,
so all 14 scored `inconclusive` and the human judge is sole grader. Judged:
base 2 PASS / 4 PARTIAL / 1 FAIL; skill 3 PASS / 3 PARTIAL / 1 FAIL. The
modalities below explain every non-PASS.

## Modalities

### M1: Editing verbs force multi-call remove/re-add dances (turn inflation) [TUXLINK]

**Evidence.** `base/S4` (20 calls) and `base/P1` (29 calls) spend most of their
budget on this. Two API constraints interact:

- `KIND_CHANGE_REJECTED`: `routines_step_update {patch:{control:"end"}}` on an
  action step returns `invalid arguments: [KIND_CHANGE_REJECTED] patch may not
  turn an action step into a control step (or back) — remove and re-add
  instead`.
- `DUPLICATE_STEP_ID`: after the initial `routines_save` created steps s1..sN,
  the model's re-plan calls `routines_step_add {id:"s2"}` and gets
  `[DUPLICATE_STEP_ID] step id "s2" already exists`. It must `routines_step_remove`
  then re-add.

Each restructure becomes fail -> remove -> re-add (3 calls for 1 logical edit).
The errors are clear and the model recovers, but the turn cost is large and it
is the dominant inefficiency in base's authoring cells.

**Classification: TUXLINK ergonomics.** The errors are actionable (not the
defect); the *shape of the edit API* is. Caller-assigned step ids invite
`DUPLICATE_STEP_ID`; immutable step kind forces remove/re-add for a
one-field intent (action -> end).

**Proposed fix.** (a) auto-assign step ids on `routines_step_add` (append/insert
semantics; caller-supplied id optional), eliminating `DUPLICATE_STEP_ID` as a
class; and/or (b) a `routines_step_replace` (upsert) verb that swaps a step
wholesale, collapsing the kind-change dance to one call. Both reduce turn count
and failure surface for the small model without weakening validation.

### M2: Prose-only remedy on `ARM_FALLTHROUGH_LEAK` -> no-op loop to turn cap [TUXLINK]

**Evidence.** `base/S1` (FAIL, cancelled 40/40). The branch `s3` fell through;
the warning names the remedy in prose ("insert an end control after the then
arm"). `disposition.remedies` was `[]`. The model re-sent the *same* branch
patch (`then:[s4,s7]` alternating with `then:[s4]`) ~36 times, every result
`applied:false`, never inserting an `end`. It could not translate prose into the
different tool call (`routines_step_add {control:"end"}`).

**Classification: TUXLINK.** Already filed as **tuxlink-90vcc** (P1) and folded
into **ADR 0025** (Amendment 2026-07-24, the quality/diagnostic-actionability
parity dimension). Authoring findings must carry typed `disposition.remedies`
(LSP-CodeAction equivalent), as consent findings already do.

### M3: `find_stations goal.kind: "nearest"` rejected [MODEL, minor TUXLINK naming]

**Evidence.** `base/P1` and `skill/P1` first call
`find_stations {goal:{kind:"nearest"}}` -> `unknown variant 'nearest', expected
'connect-now' or 'best-at'`. Both recover in one retry (`best-at` / `connect-now`).

**Classification: mostly MODEL** (it conflated "nearest station", a *distance
filter*, with `goal.kind`, which is *connect timing*). Minor Tuxlink angle: the
enum naming invites the conflation. LOW severity (1 wasted call, clear error,
clean recovery). Candidate fix only if it recurs at scale: accept `nearest` as
an alias or clarify the param description that kind is about timing, not distance.

### M4: `skill/S3` false-infeasibility bail (built nothing) [MODEL, TUXLINK discoverability]

**Evidence.** `skill/S3` (FAIL, saved nothing, 2 calls). Assistant message:
"the routine actions available do not include a way to automatically
receive/download the response to a catalog request after waiting 5 minutes ...
`radio.connect` ... sends AND receives in one step ... you can't stage a request,
wait, then connect separately to receive." It concluded the task was infeasible
and stopped. `base/S3` solved it by calling `radio.connect` a **second** time
(the second connect receives the response).

**Classification: MODEL** (the "build carefully" discipline backfired into
over-analysis and a false infeasibility conclusion) **with a TUXLINK
discoverability angle**: `radio.connect`'s "call again to receive a pending
response" pattern is not surfaced in the action description. The model read
`radio.connect` as atomic send+receive and saw no receive-only path. Making the
"connect-again-to-receive" idiom explicit in the action doc/outputs would likely
have prevented the bail. This is the sharpest example of the skill *hurting*
(base built it; skill talked itself out of it).

### M5: FT-8 prediction gap: parity + disclosure split [TUXLINK parity + MODEL behavior]

**Evidence.** P3 asks for FT-8 station-intelligence + propagation-prediction
ranking on the fallback leg. `skill/P3` states it plainly: "There is no
`predict_path` action available in the routine catalog. The `predict_path` tool
I have access to is an Elmer tool, not a routine action." `base/P3` silently
substituted a second `find_stations` as the "prediction group" and did not
disclose the gap.

**Classification: TWO issues.** (a) **TUXLINK parity gap**: `predict_path`
exists as an Elmer/agent tool but has no Routines-action counterpart, an
ADR-0024 dual-actionability violation (a capability reachable one way, invisible
the other). (b) **MODEL behavior split**: base's undisclosed substitution vs
skill's honest disclosure. The cell is designed as gap-cartography, so this is
expected; the durable finding is that the parity gap is real and named by the
model.

### M6: `AUTO_TX_UNACKED` disclosure inconsistency [MODEL, 90vcc-adjacent]

**Evidence.** `base/P1` set `transmit_mode:automatic` (honoring "every 30 min"),
hit `AUTO_TX_UNACKED`, but its final summary did **not** surface the operator-ack
requirement, unlike `base/P3` and `base/S3` (same model, same run) which
explicitly explained it. So P1 silently shipped an automatic-unacked routine.

**Classification: MODEL** (inconsistent disclosure of a finding that *does*
carry a typed remedy). Reinforces M2/tuxlink-90vcc: even where the typed remedy
exists, disclosure is unreliable; the structured remedy is what a downstream
UI/agent can act on deterministically rather than relying on the model to narrate.

### M7: No chain-of-thought captured [HARNESS / instrument]

**Evidence.** The transcript captures calls, results, and the final message only;
`base/S1` has zero assistant messages across 40 turns. Diagnosing "why" required
inferring from the action trajectory.

**Classification: HARNESS.** Not a Tuxlink product issue and not a model issue.
Worth recording: if the battery is to teach us *why* a cell fails (this session's
whole point), capturing the provider's reasoning/`reasoning_content` where the
model emits it, and at minimum logging a per-turn assistant text even when empty,
would make future modality analysis first-class rather than reconstructed.

## Tuxlink-attributable, in priority order (the "improve our errors" candidates)

1. **M1** editing ergonomics (auto-id + `routines_step_replace`), dominant turn
   inflation, affects nearly every authoring cell. NEW, unfiled.
2. **M2** typed authoring remedies, filed **tuxlink-90vcc**, ADR 0025 amended.
3. **M4** `radio.connect` connect-again-to-receive discoverability, NEW, unfiled.
4. **M5a** `predict_path` Routines-action parity (ADR 0024), NEW, unfiled.
5. **M3** `find_stations goal.kind` naming, LOW, watch-only unless it recurs.

Model-attributable (not a Tuxlink build target): M3 (conflation), M4/skill
over-analysis, M5b disclosure split, M6 disclosure inconsistency. These are what
the *skill* (teaching) layer exists to address, not the product.

## Convergence check (2026-07-24): two independent fronts, and a correction

Before building anything, this analysis was put through a two-front adversarial
convergence check per operator direction: Front 1, a Codex (gpt-5.5) blind
re-derivation of the modalities from the same raw evidence (`dev/scratch/lift-raw-evidence.txt`),
with no sight of this doc; Front 2, an independent source-grounded critique of
this doc's conclusions. Both are archived under `dev/adversarial/` (gitignored).

**The check overturned M2, and I verified the correction against source.**

- **M2 was misattributed.** base/S1 did not loop because `ARM_FALLTHROUGH_LEAK`
  lacked a typed remedy. `ARM_FALLTHROUGH_LEAK` is a *warning*; base/S1 was already
  `validates_green`; and `remedies: []` on a warning-only routine is a *deliberate,
  tested anti-ping-pong invariant* (`tuxlink-mcp-core/src/ports.rs:1644`, test at
  `:2214`). The real mechanism is a MODEL non-termination (tuxlink-m5oia): the model
  re-sent a byte-identical no-op patch ~35 times, ignoring `applied: false` (the
  m5oia guard at `src/routines/commands.rs:911`) and the `state: valid` disposition
  telling it to stop. skill/S1 resolved the same warning. Front 1 independently
  classified this MODEL (its finding #4); Front 2 flagged it as the strongest
  disagreement. The originally proposed fix (typed remedy on the warning) would have
  violated the tested invariant. **ADR 0025's amendment and tuxlink-90vcc are corrected
  accordingly.**
- **M1 was overstated / conflated.** `KIND_CHANGE_REJECTED` occurs once in the corpus;
  the turn inflation is mostly `DUPLICATE_STEP_ID` from the model doing a full
  `routines_save` then re-adding ids it already used. The skill arm hit zero of either
  class. So M1 is largely MODEL state-tracking; the auto-id fix is still sound but is
  not the headline.
- **M6 softened.** The disclosure claim rests on base/P1's final message, which is
  truncated in the evidence; the `AUTO_TX` *state* fact holds, the disclosure claim is
  under-evidenced.
- **M3 / M4 / M5 / M7 held** across all three.

**New Tuxlink findings the fronts surfaced that this pass missed** (Codex, file:line-cited;
to be independently verified before building):

- **vara-fm vocabulary drift** between the MCP and routine `find_stations` surfaces
  (`PARAM_VALUE_NOT_ALLOWED`).
- **`params` patch replaces the whole object** (not merge) → `MISSING_REQUIRED_PARAM`
  when skill/S2 patched only `bands`.
- **`rig.tune_atu` advertised but runtime-unimplemented**, no validation flag.
- **Runtime grid hardcoded at authoring** (skill/S4 read `data.read source:grid` then
  composed the literal `DM33`; the action exposes no per-source output to reference).
- **Branch fall-through affordance** (Codex #5): auto-terminate `then` arms by default,
  or escalate transmit-bearing fall-through from warning to a blocking error. This is
  the correct structural replacement for the withdrawn M2 fix.

## Build targets (operator-approved, verify-then-build)

Per operator decision after convergence: build (1) the branch-fallthrough affordance,
(2) the cross-surface vocab/semantics drifts, and (3) `predict_path` Routines-action
parity (M5a). Each Codex-surfaced source claim is independently verified before its
build. The model-attributable modalities (m5oia non-termination, skill/S3
false-infeasibility, disclosure inconsistency) are teaching-layer work, not product
builds.
