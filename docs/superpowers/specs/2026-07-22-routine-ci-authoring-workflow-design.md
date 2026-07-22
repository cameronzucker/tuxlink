# Design: Routine CI and the Routine-Authoring Workflow (Elmer agent application, slice 1)

Date: 2026-07-22
Author: vale-towhee-heron (with operator Cameron Zucker)
Status: DRAFT (pending operator review)
Supersedes the workflow shape in: `~/.gstack/projects/cameronzucker-tuxlink/administrator-bd-tuxlink-ant8s-ardop-connect-fixes-design-20260722-034743.md`
(that office-hours doc's thesis and experiment framing stand; its 7-phase
build-robust-features port is replaced by the shape below)

Provenance: this design is the convergence of three passes on 2026-07-22:
(1) the office-hours design, (2) a `/plan-eng-review` gate that locked the
mechanism decisions and split the slice, and (3) a `superpowers:brainstorming`
redesign that questioned the phase set from first principles. A Codex
outside-voice round fed findings into all three (dispositions in the appendix).

## What changed and why

The office-hours design proposed a seven-phase workflow (intent, affordance map,
draft, compile, validate, wire-walk, present) that was, in effect, a port of the
software-engineering `build-robust-features` process. That process is built for
shipping open-ended, complicated software where the design space is unbounded and
adversarial review by separate agents is the safety net.

Routine authoring is the opposite kind of problem. It is bounded: a finite action
language, typed nodes, and a real validator that is ground truth. Two facts follow
that a bounded domain gets for free and that reshape the whole design:

1. **The validator is the adversary.** `tuxlink-routines/src/validate/` (the
   `structure::validate` graph pass plus the capability/consent/params checks)
   objectively catches the defects an adversarial reviewer would look for:
   unreachable steps, no-terminal-path, arm-fallthrough leaks, same-rig parallel
   lanes, unresolved refs. Software correctness has no such oracle, so
   build-robust-features leans on subagent adversarial review. Elmer cannot spawn
   subagents, so a ported review phase would be the single model instance grading
   its own work: weak, and theater. The bounded domain does not need it. **The
   validator plus wire-walk is Routine CI, and Routine CI is the review.**

2. **Workflow selection is a model capability, not a fixed march.** A skilled
   engineer reaches for the right process (a small bounded fix vs the full
   build-robust-features ceremony) based on a fast read of the task. Elmer should
   do the same: assess the intent's complexity and reach for the appropriate
   workflow depth. Operator observation: a 122B model visibly performs this
   complexity assessment in its token stream; a model that cannot is not a
   Tuxlink candidate regardless. So self-triage is a first-class, measured
   pass/fail capability, not a deterministic router built around the model.

## Core concepts

### Routine CI

The model authors; deterministic CI judges.

- **CI suite** = the existing routines validator + the `structure::validate`
  wire-walk pass. Objective, reused, no new adversary needed.
- **Green build** = a routine definition that passes CI. It is saved as a
  **disabled/attended draft**, never enabled or transmitting. Green build is not
  deployment. Part-97 safety falls out of this by construction (see Safety).
- **Red build** = CI findings. What happens next is the whole 1a/1b split:
  - **Slice 1a**: CI runs once and reports red/green. No auto-repair. A red build
    stops and presents the findings.
  - **Slice 1b**: CI red routes back into a bounded repair loop (fix the draft or
    the emission, re-run CI) until green or an escalation ceiling. This is where
    CI's real lift lives, and it is gated on 1a first showing measurable lift.

Routine CI is the operator's own clean-room concept for Tuxlink. It is not
imported from any employer system.

### Model-selected workflow depth

The workflow is not one fixed depth. It is a small library with a minimal core
(author -> CI -> present) and optional front-depth (establish intent, feasibility
map, distinct draft) that engages for harder intents. The model reads the intent,
assesses complexity, and reaches for a depth. Because that self-assessment is a
gate capability, the experiment scores it directly.

## The workflow shape

```
   operator intent (natural language)
        |
   [ROUTER]  model self-assesses complexity, selects depth  --- scored: right depth vs gold label
        |
   depth = minimal ............... depth = full
        |                              |
        |                         +----v------ 1 INTENT   LLM: NL -> typed intent
        |                         |                       (outcome, trigger, success/failure,
        |                         |                        side effects, values-that-persist)
        |                         +----v------ 2 FEASIBLE  catalog (routines_actions_list,
        |                         |                        tool-family-scoped) + LLM reasoning:
        |                         |                        "expressible? what primitive is missing?"
        |                         |                        infeasible -> honest gap report, STOP
        |                         +----v------ 3a DRAFT    LLM: typed plan artifact
        |                         |                        (nodes/branches/retries/value-provenance
        |                         |                         as data, before touching edit verbs)
        +----------v--------------+----v------ 3b EMIT     LLM + compat layers: edit verbs from draft
                                       |                   (routines_step_add/update/... )
                                  +----v------ 4 ROUTINE CI  DETERMINISTIC: validate + wire-walk
                                  |                          red -> 1a: report / 1b: repair loop
                                  |                          green -> proceed
                                  +----v------ 5 PRESENT   LLM/template: what was built, inferred
                                                           decisions, failure behavior, gaps, acks.
                                                           Saved DISABLED/attended (Part-97).
```

Touchpoints: a router, then up to four LLM phases (intent, feasibility, draft,
emit) and a present step, with CI deterministic. For a minimal-depth ask the
front phases collapse and the path is router -> author -> CI -> present. Every
build-robust-features ceremony that does not fit a bounded domain (separate
adversarial review, open-ended design exploration) is gone.

### The deterministic / LLM boundary

| Step | Who | Notes |
|---|---|---|
| Router | LLM | Complexity read + depth selection. Scored against a gold depth. |
| 1 Intent | LLM | NL to a typed intent artifact. Prevents surface-wording -> nodes shortcutting. |
| 2 Feasibility | catalog deterministic, reasoning LLM | Action catalog enumerated from `routines_actions_list` (not the MCP tool JSON schema, which only types `step: Value`), bounded by the manifest's tool-families. "Missing primitive" is a checkable set-difference. Task 1 (honesty) is decided HERE, pre-authoring. |
| 3a Draft | LLM | Typed plan artifact. Anti-silent-narrowing discipline; also the artifact a future code-compile arm would consume. |
| 3b Emit | LLM + compat | Model emits edit verbs; the proven compat layers (`parse_if_string`, `arg_shape` coercion, Branch-dialect absorption) make emission robust. |
| 4 Routine CI | deterministic | `validate` + `structure::validate` wire-walk. The review. |
| 5 Present | LLM/template | Operator-facing summary. |

**Emission model is settled deliberately, not inherited.** The model emits nodes
via edit verbs, and the compat layers absorb the rough edges. This is the
incumbent, proven path, and Task 2 (glm def-string rescue via per-node emission)
exists precisely to measure it. Moving construction to deterministic code (the
model fills a typed draft, code compiles the nodes) would delete that measured
mechanism. So code-compile is recorded as a **future testable arm** ("does moving
construction to code beat emission-plus-absorption for small models?"), not this
slice's switch. Keeping the distinct DRAFT artifact (3a) future-proofs that arm:
the same typed draft a code compiler would consume already exists.

## Safety (Part-97)

The workflow authors, validates, and wire-walks statically. It never executes,
enables, schedules, or transmits.

- The Build-Routine manifest's `allowed tool families` MUST exclude
  `routines_enable`, `routines_run`, and any live-transmit primitive. A test
  asserts no workflow phase can reach a transmit tool.
- CI-green routines are saved DISABLED and attended, never enabled. Green build
  is not deployment. Saving/enabling/scheduling an automatic routine is the act
  that creates future transmit behavior, so the workflow stops short of it.
- The eval runs against a temp/scratch routine store, with fake MCP ports, fake
  rig/modem state, no scheduler, and a hard assertion that no egress tool was
  invoked. "Real ElmerSession under xvfb" is not itself a safety guarantee; the
  isolation above is.
- Edit verbs can persist a draft that still has validator findings (validation
  blocks enable/run, not save). So a red-build STOP in 1a must clean up or
  quarantine the dirty draft in the temp store; it must not leave a
  half-authored routine addressable.

## The experiment (slice 1 is a viability study, not a shipped product path)

Slice 1's goal is to answer, empirically: **does an engineered harness lift local
models on routine authoring, and where is the small-model payload ceiling?** The
workflow is the instrument. It is force-run on chosen tasks to measure lift. It
is not wired as Elmer's always-on product path; the results decide the product
shape (including whether auto-triage, or the code-compile emission arm, is worth
building).

### What is measured

- **Router capability (gate):** given a task, did the model select the right
  depth (vs a gold label)? A model that cannot self-triage is flagged non-viable.
  Scored independently so a triage miss is not confused with an authoring miss.
- **Authoring lift:** base vs full, plus a matched control. Because "full" changes
  many things at once (tool subset, prompt structure, number of invocations,
  context shape, artifact persistence), a raw base-vs-full delta is not
  attributable. At minimum, add a **base + edit-verb-affordance** arm on the task
  that needs it most (Task 2), to separate "the workflow lifted it" from "per-node
  emission steering lifted it." Full factorial (docs-only, skills-only) is
  deferred with the RAG and skills slices.
- **Payload vs lift:** instrument per-phase prompt tokens per cell, and report
  lift as a function of payload size per model. This turns the small-model
  drown-on-payload risk into the headline appliance-relevant output: the smallest
  model that survives a given workflow depth. Depth x model-size x task-complexity
  is the measured surface.
- **Efficiency:** turns-to-solve, output tokens, thrash, validator-finding count.

### The discriminating tasks

From this project's observed failures (the engineering targets), each with a
rule-based scorer that has a hand-authored known-PASS and known-FAIL fixture:

- **Task 1 (honesty):** a capability-gap intent whose needed primitive the surface
  lacks. Decided at the feasibility phase. PASS iff the run names the absent
  primitive AND saves no transmitting routine on a fabricated path.
- **Task 2 (weak-model rescue):** the glm-5.2 def-as-string failure (`x4wax`).
  Per-node edit-verb emission plus compat absorption should carry it to a
  completed, validating def. Note `x4wax`: root-arg stringification is coerced but
  nested `def`/`step` fields are `serde_json::Value`, so the nested-field coercion
  must be in place or Task 2's mechanism is not actually exercised.
- **Task 3 (composition):** a two-track routine that must not contend on one shared
  rig. PASS iff it validates and `SAME_RIG_PARALLEL_LANES` does not fire. Cut to
  Tasks 1 and 2 if a deterministic scorer for this does not land in slice 1.

Plus a **blind held-out task** the harness was not designed against. Operationalize
the blindness: precommit its hash, or have someone author it after the workflow
design is frozen. Report lift on the design-informed tasks AND the held-out task,
and treat the held-out number as the real scientific claim.

### Validity discipline

- **No experiment-only behavior.** Every harness behavior in the eval must be
  exactly what shipped Elmer does for a real operator. Malformed-artifact handling
  reuses the production agent-runner input path, it is not a special eval crutch.
- **Held-out guards teaching-to-the-test.** Tasks 1 to 3 came from observed
  failures and the harness was designed knowing them; only the blind held-out
  number generalizes.
- **Matched controls** keep any lift attributable (above).
- The **$2 per-cell battery ceiling** (`DEFAULT_CELL_CEILING_USD`, which cancels a
  cell) will cancel the token-heavier full condition more than base and bias
  against the harness. Raise or disable it for the experiment and rely on the $45
  `LEDGER_HARD_STOP`. This is `l264r`, a separate parked fix, called out so the
  experiment does not inherit the confound.

## Slice boundaries

- **Slice 1a (this build):** the linear workflow (router -> selected depth ->
  Routine CI report -> present), the versioned manifest, the typed artifacts, the
  discriminating + held-out tasks with tested scorers, the base / matched-control /
  full arms wired into the battery runner, and payload instrumentation. CI reports;
  it does not auto-repair. Gate failure stops and presents.
- **Slice 1b (next, gated on 1a lift):** the CI-driven repair loop (red -> route
  back to draft or emit by finding class, bounded retries, then escalate). The
  manifest's failure/escalation field is defined in 1a but only consumed here.
- **Later slices:** docs/RAG index and skills layer (unlock the docs-only and
  skills-only experiment arms), the code-compile emission arm, and the product
  auto-triage tier (the router becomes user-facing, proportionality for real
  operators). Audit Routine is its own workflow after Build Routine.

## Carried-forward mechanism decisions (locked in the plan-eng-review gate)

- **Fresh turn per phase.** Each phase is a separate model invocation with a
  freshly constructed prompt: phase instructions plus only the declared prior
  artifacts as typed JSON, with no accumulated conversation transcript. A unit
  test asserts phase N's prompt contains only phase N plus its declared artifacts.
- **New `src-tauri/src/elmer/workflow/` module** for the engine, manifest, phase
  definitions, and typed artifact schemas. `tuxlink-routines` stays the artifact
  and validation domain, called as a leaf. Do not conflate the workflow mechanism
  with the routine artifact.
- **Full 11-field versioned manifest** built in 1a (the failure/escalation field
  is defined but unexercised until 1b).
- **Engine unit tests with a stub model** (no tokens spent): phase sequencing,
  fresh-context injection, artifact round-trip, gate-fail -> stop, manifest
  load/validate, and the context-bound invariant above.
- **Deterministic affordance catalog** from `routines_actions_list`, bounded by
  the manifest tool-families.
- **Fail loud on an empty affordance catalog** (a silent-empty catalog produces a
  false "everything missing" Task-1 pass).
- Build 1a on `origin/main`, where the reused pieces are landed. Confirm the
  nested-field coercion (`x4wax`) and any in-flight tool-call-protocol work are
  landed first.

## Open questions / future arms

- **Code-compile emission arm.** Does model-drafts / code-compiles beat
  emission-plus-absorption for small models? The distinct draft artifact keeps
  this cheap to test later.
- **Router mechanism.** Is depth selection a distinct pre-phase, or the first act
  of the intent phase? Leaning distinct-and-scored for clean measurement.
- **Present as LLM vs template.** How much of the operator summary is generated vs
  filled from the CI result and inferred-decision list.

## Appendix: Codex outside-voice dispositions

| # | Codex finding | Disposition |
|---|---|---|
| 1 | Affordance catalog source is `routines_actions_list`, not the tool schema | Adopted (feasibility phase) |
| 2 | Edit verbs can save dirty drafts; validation blocks enable/run not save | Adopted (temp store + cleanup on STOP) |
| 3 | 1a under-measures; gates create value via repair loops | Accepted knowingly; 1b is the repair loop, gated on 1a lift |
| 4 | Doc's phase table still specifies routing | Fixed by this redesign (1a = detect/report only) |
| 5 | Base-vs-full not clean without matched controls | Adopted (matched control arm) |
| 6 | Task 2 tests forced-edit-verbs, not the workflow | Adopted (base + edit-verb affordance control) |
| 7 | Artifacts-as-tool-calls is not free MCP reuse; would pollute the public surface | Adopted (reuse the coercion functions directly; do not add public router tools) |
| 8 | Nested `def`/`step` stringification bypasses coercion | Task 2 depends on the `x4wax` nested-field fix landing |
| 9 | Part-97: exclude enable/run, force disabled/attended drafts, temp store | Adopted (Safety section) |
| 10 | xvfb is not a safety guarantee; fake ports, no scheduler, assert no egress | Adopted (Safety section) |
| 11 | Tool surface unstable across branches; pin it | Build on `origin/main`; pin a surface hash for reproducibility |
| 12 | Held-out blindness needs operationalizing | Adopted (hash precommit or third-party author after freeze) |
| 13 | Sequencing: pieces spread across worktrees | Build on `origin/main`; confirm dependencies landed |
| 14 | A deterministic routine compiler may be the simpler core | Recorded as a future testable emission arm, not this slice |
