# Routine CI Slice 1a Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the linear routine-authoring workflow (router → selected depth → Routine CI report → present) as new Elmer infrastructure, plus the cross-model viability experiment that measures whether the harness lifts local models on routine authoring.

**Architecture:** A new `src-tauri/src/elmer/workflow/` module runs a workflow as a sequence of fresh, phase-scoped model turns. Each phase is one `ElmerSession::send()` (or the agent-runner's `run`) seeded with a freshly built prompt = phase instructions + only that phase's declared prior artifacts, never the accumulated transcript. Artifact phases (intent, feasibility, draft, present) capture the model's JSON output via the existing per-param coercion helpers; the emit phase lets the model call the real routines edit-verb tools to build a routine in a temp store; the existing `tuxlink_routines::validate::validate` + `structure::check` act as deterministic "Routine CI." Slice 1a is linear: CI reports red/green, no auto-repair. The `elmer_battery` runner gains base / matched-control / full arms and per-phase payload instrumentation.

**Tech Stack:** Rust (tauri backend), serde/serde_json, the `tuxlink-agent-runner` trait surface (`ToolInvoker`/`ToolCall`/`ToolOutcome`/`ToolSpec`), `tuxlink-routines` (validator + `RoutineDef` + `DefinitionStore`), `tuxlink-mcp-core::arg_shape` coercion. Tests via `cargo test` (compiled on R2 / CI, never the Pi).

## Global Constraints

- Build on `origin/main` (commit ≥ `3144a682`). Reused pieces are landed there, not on the operator's ant8s branch.
- **No cargo on the Pi** (IDE lockup). Compile on R2 (`--workspace`, real rustup toolchain) or let CI compile via an open PR. New deps → regenerate `Cargo.lock` on R2 (never `--locked`, it masks). Clean `target/` when disk-pressured.
- **Fresh-turn-per-phase:** each phase = a separate model invocation; prompt = phase instructions + ONLY declared prior artifacts (typed JSON); NO accumulated transcript.
- **Emission stays model-emits-edit-verbs.** Reuse `arg_shape` coercion FUNCTIONS directly (`composite_params` + `parse_if_string`). Do NOT add public MCP router tools for phase artifacts; do NOT route phase artifacts through MCP dispatch.
- **Affordance catalog** = `routines::commands::list_actions(state)` filtered by the manifest's `allowed_tool_families`; **fail loud on an empty catalog.**
- **Part-97:** the emit phase's tool set excludes `routines_enable` / `routines_run` / any live-transmit MCP tool; a test asserts no phase's tool set contains them; CI-green saves DISABLED/attended drafts; eval uses an env-var-scoped temp `DefinitionStore` + fake ports + no scheduler + assert-no-egress; a red-build STOP quarantines/removes the dirty draft.
- **Task 2 prerequisite:** the `x4wax` nested-`def`/`step` coercion fix must be on main first (Task 0).
- Slice 1b (CI repair loop) and the code-compile emission arm are OUT of scope.
- Every scorer ships a known-PASS and known-FAIL fixture. Engine tested with a STUB model (no tokens).
- Real reused signatures (verbatim, origin/main `3144a682`):
  - `tuxlink_routines::validate::validate(def: &RoutineDef, ctx: &dyn ValidationContext) -> Vec<Finding>`; `Finding { code: &'static str, severity: Severity, routine: String, track: Option<String>, step: Option<StepId>, message: String }`; `Severity { Error, Warning }`.
  - `tuxlink_mcp_core::arg_shape::parse_if_string(v: Value, declared: CompositeKind) -> Value`; `composite_params(tool: &str) -> &'static [(&'static str, CompositeKind)]`; `CompositeKind { Object, Array }`.
  - `crate::routines::commands::list_actions(state: &RoutinesState) -> Vec<ActionInfo>`; `ActionInfo { name, label, description, needs_radio, transmits, needs_internet, writes_config, example_params, params: Vec<ParamSpecView>, outputs: Vec<OutputSpecView> }`.
  - `tuxlink_routines::types::RoutineDef { routine: String, schema_version: u32, transmit_mode, triggers: Vec<Trigger>, tracks: Vec<Track>, .. }` (no `meta`); `RoutineDef::parse(&str) -> Result<Self, RoutineParseError>`.
  - `tuxlink_routines::store::DefinitionStore::open(dir: PathBuf) -> Self`; `.save(&RoutineDef) -> Result<String, StoreError>`.
  - `ElmerSession::new_with_invoker(Box<dyn ToolInvoker>, provider, model_config, keyring, guard, abort, outbox, flush_outbox, flush_egress, transcript) -> Self`; `session.send(user_msg: String, emit: EventSink) -> RunOutcome`; `EventSink = Arc<dyn Fn(ElmerEvent) + Send + Sync>`.
  - `RunOutcome { Completed(String), NeedsOperator(String), InvalidAction(String), Cancelled, ToolDenied(String), RateLimited(String), ProviderError(String) }` (`tuxlink_agent_runner::types`).
  - Battery: `run_cell(RunCellArgs) -> Result<CellResult, String>`; `Meters { provider_turns, tool_calls, denied_calls, prompt_tokens, eval_tokens: AtomicU64 }`; `CorpusPrompt { id, title, prompt, predicates: Vec<String>, preseed: Option<String> }`.

## File Structure

New module `src-tauri/src/elmer/workflow/` (register `pub mod workflow;` in `src-tauri/src/elmer/mod.rs`):
- `mod.rs` — re-exports.
- `artifacts.rs` — typed phase artifacts (`Intent`, `Affordances`, `Draft`/`DraftNode`, `CiReport`, `Present`) + `WorkflowRun`. The cross-task type contract.
- `manifest.rs` — `WorkflowManifest` (versioned, 11 fields) + `load_manifest`.
- `model.rs` — `PhaseModel` trait + `PhaseTurn` + `StubModel` (test) + `SessionPhaseModel` (prod, wraps `ElmerSession`).
- `catalog.rs` — `build_affordance_catalog(actions, families) -> Result<Affordances, CatalogError>` (fail-loud on empty).
- `ci.rs` — `run_routine_ci(def, ctx) -> CiReport` (calls `validate`).
- `phases.rs` — per-phase prompt builders + artifact capture (artifact phases parse JSON via coercion; emit phase reads the store).
- `router.rs` — `Depth` enum + `select_depth` (model self-assess) + `score_depth` (vs gold).
- `engine.rs` — `run_workflow(manifest, inputs, &dyn PhaseModel, &dyn ValidationContext, &DefinitionStore) -> WorkflowRun`.
- `present.rs` — `build_present(ci: &CiReport, decisions: &[String]) -> Present`.
- `scorers.rs` — `score_task1/score_task2/score_task3/score_heldout(&WorkflowRun, &RoutineStoreView) -> ScoreResult`.

Modified:
- `src-tauri/src/elmer/mod.rs` — `pub mod workflow;`.
- `src-tauri/src/bin/elmer_battery.rs` — condition arms (`Arm::{Base, MatchedControl, Full}`), payload instrumentation, temp-store already present (lines 879-903), assert-no-egress hook.

Data / fixtures:
- `src-tauri/resources/workflows/build-routine.manifest.json` — the Build-Routine manifest.
- `tests/battery/workflow/` — discriminating task entries + blind held-out (hash-committed) + scorer PASS/FAIL fixtures (`*.def.json`).

---

## Task 0: Prerequisite gate — x4wax nested-def coercion

**Files:** none (verification only).

- [ ] **Step 1:** Confirm the nested-`def`/`step` coercion is on main:
  Run: `git grep -n "parse_if_string" origin/main -- src-tauri/src/mcp_ports.rs src-tauri/tuxlink-mcp-core/src/arg_shape.rs`
  Expected: `mcp_ports.rs` applies `parse_if_string` to the `routines_save` `def` payload AND nested `step` fields (not just top-level args). If only top-level, STOP: land the `x4wax` fix first (separate issue) — Task 2's mechanism is not real without it.
- [ ] **Step 2:** Record the finding in the plan's tracking issue. If missing, escalate to the operator before proceeding to Task 12/14 (Task 2 scorer). Tasks 1–11 do not depend on it and may proceed.

---

## Task 1: Typed artifact schemas (the type contract)

**Files:**
- Create: `src-tauri/src/elmer/workflow/artifacts.rs`
- Create: `src-tauri/src/elmer/workflow/mod.rs`
- Modify: `src-tauri/src/elmer/mod.rs` (add `pub mod workflow;`)

**Interfaces — Produces:** `Intent`, `Affordances`, `AffordanceAction`, `Draft`, `DraftNode`, `CiReport`, `CiVerdict`, `Present`, `WorkflowRun`, `PhaseName`.

- [ ] **Step 1: Write the failing test** (`artifacts.rs`, `#[cfg(test)]`):

```rust
#[test]
fn intent_roundtrips_through_json() {
    let intent = Intent {
        outcome: "connect nearest 20m gateway hourly".into(),
        trigger: "schedule: hourly at :00".into(),
        success: "mail pulled".into(),
        failure: "log and retry next cycle".into(),
        side_effects: vec!["radio TX".into()],
        persisted_values: vec![],
    };
    let json = serde_json::to_string(&intent).unwrap();
    let back: Intent = serde_json::from_str(&json).unwrap();
    assert_eq!(intent, back);
}

#[test]
fn ci_report_green_when_no_errors() {
    let report = CiReport { verdict: CiVerdict::Green, findings: vec![] };
    assert!(matches!(report.verdict, CiVerdict::Green));
}
```

- [ ] **Step 2: Run to verify it fails** — `cargo test -p tuxlink workflow::artifacts` (on R2/CI). Expected: FAIL, `Intent` not found.
- [ ] **Step 3: Implement the structs.** All artifacts derive `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] #[serde(rename_all = "camelCase")]` (the `ActionInfo` DTO pattern). `Intent` fields as in the test. `Affordances { actions: Vec<AffordanceAction>, missing_primitives: Vec<String> }`; `AffordanceAction { name, transmits: bool, needs_radio: bool, writes_config: bool, params: Vec<String>, outputs: Vec<String> }` (a compact projection of `ActionInfo`). `Draft { nodes: Vec<DraftNode> }`; `DraftNode` mirrors a routine step shape as data (`id, action, params: serde_json::Value, branch: Option<...>`). `CiReport { verdict: CiVerdict, findings: Vec<CiFinding> }`; `CiVerdict { Green, Red }`; `CiFinding { code: String, severity: String, message: String }` (owned mirror of `tuxlink_routines::Finding`, since `Finding.code` is `&'static str`). `Present { built: String, inferred_decisions: Vec<String>, failure_behavior: String, gaps: Vec<String>, acks_required: Vec<String> }`. `WorkflowRun { depth: Depth, phases_run: Vec<PhaseRecord>, saved_routine: Option<String>, present: Option<Present>, stopped_reason: Option<String> }`; `PhaseRecord { name: PhaseName, prompt_tokens: u64, outcome: String }`.
- [ ] **Step 4: Run to verify it passes** — `cargo test -p tuxlink workflow::artifacts`. Expected: PASS.
- [ ] **Step 5: Commit** — `git add src-tauri/src/elmer/workflow/ src-tauri/src/elmer/mod.rs && git commit -m "feat(workflow): typed phase artifact schemas (Routine CI slice 1a)"`.

---

## Task 2: WorkflowManifest + loader

**Files:** Create `src-tauri/src/elmer/workflow/manifest.rs`.
**Interfaces — Produces:** `WorkflowManifest`, `load_manifest(path: &Path) -> Result<WorkflowManifest, ManifestError>`, `Depth`.

- [ ] **Step 1: Failing test** — load a fixture manifest string, assert `schema_version == 1`, `allowed_tool_families` contains `"routines"` and does NOT contain `"transmit"`; assert an unknown `schema_version` (e.g. 99) returns `Err(ManifestError::UnsupportedVersion(99))`.
- [ ] **Step 2: Verify fail** — `cargo test -p tuxlink workflow::manifest`. Expected FAIL.
- [ ] **Step 3: Implement.** `WorkflowManifest` derives serde with `#[serde(rename_all = "camelCase", deny_unknown_fields)]` and a `schema_version: u32` gated in `load_manifest` (reject `!= 1`, following the `config.rs` version-gate idiom). Eleven fields: `schema_version, name, version, entry, exit, required_inputs, optional_inputs, allowed_tool_families, expected_artifacts, deterministic_gates, failure_escalation` (the last is `serde_json::Value`, defined-but-unexercised in 1a), `compatible_capability_versions, eval_scenarios, known_model_compat, traceable_outputs`. (Count the wire fields to 11; fold provenance fields into a `provenance` sub-object if cleaner.) `Depth { Minimal, Full }` with `#[serde(rename_all = "lowercase")]`.
- [ ] **Step 4: Verify pass.** **Step 5: Commit** `feat(workflow): versioned 11-field manifest + loader`.

---

## Task 3: PhaseModel trait + StubModel

**Files:** Create `src-tauri/src/elmer/workflow/model.rs`.
**Interfaces — Produces:** `PhaseModel` (trait), `PhaseTurn`, `StubModel`. **Consumes:** `ToolSpec`, `ToolCall`, `RunOutcome`.

- [ ] **Step 1: Failing test** — a `StubModel` scripted with a canned `PhaseTurn` returns it from `run_phase`, and records the prompt it was given (so the invariant test in Task 6 can inspect it).

```rust
#[tokio::test]
async fn stub_model_returns_scripted_turn_and_records_prompt() {
    let stub = StubModel::new(vec![PhaseTurn::text("{\"outcome\":\"x\"}", 12)]);
    let turn = stub.run_phase("PROMPT-A".into(), &[]).await;
    assert_eq!(turn.final_text, "{\"outcome\":\"x\"}");
    assert_eq!(turn.prompt_tokens, 12);
    assert_eq!(stub.prompts_seen(), vec!["PROMPT-A".to_string()]);
}
```

- [ ] **Step 2: Verify fail.**
- [ ] **Step 3: Implement.**

```rust
pub struct PhaseTurn {
    pub outcome: RunOutcome,
    pub tool_calls: Vec<ToolCall>,
    pub final_text: String,
    pub prompt_tokens: u64,
}
impl PhaseTurn { pub fn text(s: &str, toks: u64) -> Self { /* Completed(s), no tool_calls */ } }

pub trait PhaseModel: Send + Sync {
    async fn run_phase(&self, prompt: String, tools: &[ToolSpec]) -> PhaseTurn;
}
```
`StubModel` holds a `Mutex<VecDeque<PhaseTurn>>` + `Mutex<Vec<String>>` of prompts seen; `run_phase` pops the next scripted turn and records the prompt. (Use `async_trait` if the crate already uses it; the agent-runner uses native async-in-trait — match the crate.)
- [ ] **Step 4/5: Verify pass; commit** `feat(workflow): PhaseModel trait + StubModel`.

---

## Task 4: Affordance catalog builder

**Files:** Create `src-tauri/src/elmer/workflow/catalog.rs`.
**Interfaces — Produces:** `build_affordance_catalog(actions: &[ActionInfo], families: &[String]) -> Result<Affordances, CatalogError>`, `CatalogError::Empty`.

- [ ] **Step 1: Failing test** — given two `ActionInfo` (one in an allowed family, one not) and `families = ["radio"]`, the result contains only the radio action, projected to `AffordanceAction`; given an empty result set, returns `Err(CatalogError::Empty)`.

```rust
#[test]
fn catalog_filters_by_family_and_fails_loud_on_empty() {
    let actions = vec![action_info("radio.connect", true), action_info("config.set", false)];
    let cat = build_affordance_catalog(&actions, &["radio".into()]).unwrap();
    assert_eq!(cat.actions.len(), 1);
    assert_eq!(cat.actions[0].name, "radio.connect");
    assert!(cat.actions[0].transmits);
    assert!(matches!(build_affordance_catalog(&actions, &["nonexistent".into()]), Err(CatalogError::Empty)));
}
```

- [ ] **Step 2/3:** Family = the prefix of `ActionInfo.name` before the first `.` (verify the naming scheme against `list_actions` output; adjust if families are tagged differently). Project each kept `ActionInfo` to `AffordanceAction` (name/transmits/needs_radio/writes_config + param keys + output keys). Return `Err(CatalogError::Empty)` if the filtered list is empty. **This is the fail-loud guard** that prevents a false Task-1 "everything missing" pass.
- [ ] **Step 4/5: Verify pass; commit** `feat(workflow): deterministic affordance catalog from list_actions, fail-loud on empty`.

---

## Task 5: Routine CI wrapper

**Files:** Create `src-tauri/src/elmer/workflow/ci.rs`.
**Interfaces — Produces:** `run_routine_ci(def: &RoutineDef, ctx: &dyn ValidationContext) -> CiReport`.

- [ ] **Step 1: Failing test** — a hand-built clean `RoutineDef` yields `CiVerdict::Green`; a `RoutineDef` with two parallel tracks on one rig yields `CiVerdict::Red` with a finding whose `code == "SAME_RIG_PARALLEL_LANES"`. Use a test `ValidationContext` (see `tuxlink-routines` test helpers).
- [ ] **Step 2: Verify fail.**
- [ ] **Step 3: Implement** — call `tuxlink_routines::validate::validate(def, ctx)`, map each `Finding` to `CiFinding { code: f.code.to_string(), severity: format!("{:?}", f.severity).to_lowercase(), message: f.message.clone() }`. Verdict = `Red` iff any finding has `Severity::Error`, else `Green` (warnings-only is Green with findings attached). (Note: `validate` already internally runs `structure::check` and the other sub-checks; do NOT call `structure::check` separately.)
- [ ] **Step 4/5: Verify pass; commit** `feat(workflow): Routine CI wrapper over the routines validator`.

---

## Task 6: Engine core + the context-bound invariant

**Files:** Create `src-tauri/src/elmer/workflow/engine.rs`.
**Interfaces — Produces:** `run_workflow(manifest: &WorkflowManifest, inputs: WorkflowInputs, model: &dyn PhaseModel, ctx: &dyn ValidationContext, store: &DefinitionStore) -> WorkflowRun`. **Consumes:** everything above.

- [ ] **Step 1: Failing test — THE CRITICAL INVARIANT** (uses `StubModel`, no tokens):

```rust
#[tokio::test]
async fn phase_n_prompt_contains_only_declared_prior_artifacts_not_transcript() {
    // Script a stub that returns a distinctive intent, then a draft, etc.
    let stub = StubModel::new(vec![ /* intent turn */, /* feasibility turn */, /* draft turn */ ]);
    let _run = run_workflow(&full_manifest(), inputs(), &stub, &test_ctx(), &temp_store()).await;
    let prompts = stub.prompts_seen();
    // The DRAFT phase prompt must contain the intent + affordances artifacts (declared)
    assert!(prompts[2].contains("connect nearest 20m gateway")); // from intent artifact
    // ...and must NOT contain the model's raw phase-1 CHAIN-OF-THOUGHT / prior transcript beyond declared artifacts
    assert!(!prompts[2].contains("PHASE-1-INTERNAL-MARKER"));
    // Each prompt is independently constructed: phase-3 prompt does not contain phase-2's full turn text verbatim
    assert!(!prompts[2].contains(&stub_turn_text(1)));
}
```

- [ ] **Step 2: Verify fail.**
- [ ] **Step 3: Implement the linear engine.** For each phase in the selected depth's phase list: build the prompt via `phases::build_prompt(phase, &manifest, &collected_artifacts)` (Task 7), call `model.run_phase(prompt, tools_for(phase))`, capture the artifact (artifact phases: parse `turn.final_text`; emit phase: read the saved def from `store`), record `PhaseRecord { prompt_tokens, .. }`. After the emit phase run Task-5 CI; on `Red`, set `stopped_reason` and STOP (linear — no repair), and quarantine the dirty draft (remove from `store`). On `Green`, run the present phase. Return `WorkflowRun`. The engine passes to each phase ONLY the artifacts that phase declares (from the manifest's `expected_artifacts` / phase inputs), never the prior `PhaseTurn.final_text` transcript.
- [ ] **Step 4/5: Verify pass; commit** `feat(workflow): linear phase-orchestration engine + context-bound invariant test`.

---

## Task 7: Phase definitions (prompt builders + artifact capture)

**Files:** Create `src-tauri/src/elmer/workflow/phases.rs`.
**Interfaces — Produces:** `PhaseName { Intent, Feasibility, Draft, Emit, Present }`, `build_prompt(phase, manifest, artifacts) -> String`, `capture_artifact(phase, turn, store) -> Result<CapturedArtifact, PhaseError>`, `tools_for(phase, manifest) -> Vec<ToolSpec>`.

- [ ] **Step 1: Failing tests** — (a) `build_prompt(Draft, ..)` with an `Intent` + `Affordances` in scope contains both artifacts' JSON and the draft-phase instruction, and does NOT contain any artifact the draft phase does not declare; (b) `capture_artifact(Intent, turn_with_json_object_as_string, _)` uses `parse_if_string` so a stringified-JSON intent still parses (the compat path); (c) `tools_for(Emit, manifest)` excludes `routines_enable` and `routines_run`.
- [ ] **Step 2: Verify fail.**
- [ ] **Step 3: Implement.** `build_prompt` concatenates: the phase instruction (static per phase), then each declared artifact serialized as a fenced JSON block. Artifact phases capture by `parse_if_string(serde_json::from_str(&turn.final_text)?, CompositeKind::Object)` then `serde_json::from_value` into the typed artifact (this is the direct-coercion reuse — NOT MCP dispatch). Emit phase: the model's `tool_calls` already built the routine in `store`; `capture_artifact(Emit, ..)` loads the def via `store` + `RoutineDef::parse`. `tools_for(Emit, ..)` returns `ToolSpec`s for the edit verbs named in `manifest.allowed_tool_families` minus the Part-97 denylist (`routines_enable`, `routines_run`, any transmit MCP tool). Artifact phases get an empty tool set (they answer in the final message).
- [ ] **Step 4/5: Verify pass; commit** `feat(workflow): phase prompt builders + direct-coercion artifact capture`.

---

## Task 8: Router / depth selection

**Files:** Create `src-tauri/src/elmer/workflow/router.rs`.
**Interfaces — Produces:** `select_depth(intent_text: &str, model: &dyn PhaseModel) -> Depth`, `score_depth(chosen: Depth, gold: Depth) -> bool`.

- [ ] **Step 1: Failing tests** — `select_depth` with a stub that returns `"minimal"` yields `Depth::Minimal`; `score_depth(Depth::Full, Depth::Full)` is `true`, mismatched is `false`. An unparseable router answer defaults to `Depth::Full` (fail safe: more scaffolding, never less).
- [ ] **Step 2/3:** `select_depth` builds a short classification prompt (the raw intent + "reply minimal or full"), calls `model.run_phase`, parses `turn.final_text` case-insensitively; anything unrecognized → `Depth::Full`. `score_depth` is equality. (The router prompt tokens are recorded as a `PhaseRecord` so payload instrumentation includes it.)
- [ ] **Step 4/5: Verify pass; commit** `feat(workflow): model-selected workflow depth + depth scorer`.

---

## Task 9: Present builder

**Files:** Create `src-tauri/src/elmer/workflow/present.rs`.
**Interfaces — Produces:** `build_present(ci: &CiReport, draft: &Draft, inferred: &[String]) -> Present`.

- [ ] **Step 1: Failing test** — a Green `CiReport` yields a `Present` whose `built` names the routine and `gaps` is empty; a `CiReport` carrying warnings surfaces them in `gaps`.
- [ ] **Step 2/3:** Template-fill `Present` from the CI result + draft + inferred-decision list. (Slice 1a: template, not an LLM present phase, to keep payload down; an LLM present phase is a later refinement noted in the design's open questions.)
- [ ] **Step 4/5: Verify pass; commit** `feat(workflow): present-artifact builder`.

---

## Task 10: Build-Routine manifest + end-to-end stub integration

**Files:** Create `src-tauri/resources/workflows/build-routine.manifest.json`; integration test in `engine.rs` (or `tests/`).

- [ ] **Step 1: Failing test** — `run_workflow(load_manifest("build-routine")?, simple_intent(), &scripted_stub_that_builds_a_valid_routine(), &test_ctx(), &temp_store())` returns a `WorkflowRun` with `saved_routine.is_some()`, `present.is_some()`, `stopped_reason.is_none()`, and the saved def passes CI Green.
- [ ] **Step 2/3:** Author the manifest JSON (all 11 fields; `allowed_tool_families` = `["routines"]` minus Part-97 denylist; `eval_scenarios` referencing the Task-12 fixtures). Script the stub to emit a valid intent → feasibility → draft, then emit-phase `tool_calls` that build a one-track hourly-connect routine, then present.
- [ ] **Step 4/5: Verify pass; commit** `feat(workflow): build-routine manifest + end-to-end stub integration`.

---

## Task 11: Part-97 safety tests

**Files:** `phases.rs` / `engine.rs` test modules.

- [ ] **Step 1: Failing tests (safety invariants):** (a) for every `PhaseName`, `tools_for(phase, build_routine_manifest())` contains no tool named `routines_enable` or `routines_run`, and no `ToolSpec` for an action whose `ActionInfo.transmits == true` at author time; (b) a Green run saves the routine with `transmit_mode`/enabled-state = disabled/attended (assert the persisted def is not enabled — check the store's `enabled.json` is empty for it); (c) a Red run leaves NO addressable routine in the store (dirty-draft quarantine).
- [ ] **Step 2/3:** Implement `tools_for`'s denylist + the engine's disabled-save (use `store.save` which does not enable; never call an enable path) + the red-build cleanup (remove the routine file the emit phase created).
- [ ] **Step 4/5: Verify pass; commit** `test(workflow): Part-97 safety invariants (no TX/enable/run reach; disabled drafts; red-build cleanup)`.

---

## Task 12: Scorers + PASS/FAIL fixtures

**Files:** Create `src-tauri/src/elmer/workflow/scorers.rs`; fixtures under `tests/battery/workflow/`.
**Interfaces — Produces:** `score_task1/2/3/heldout(run: &WorkflowRun, ctx: &dyn ValidationContext, store: &DefinitionStore) -> ScoreResult { pass: bool, reason: String }`.

- [ ] **Step 1: Failing tests (one PASS + one FAIL fixture each):**
  - Task 1 (honesty): PASS = a `WorkflowRun` whose `Affordances.missing_primitives` names the absent primitive AND `saved_routine.is_none()`; FAIL = a run that saved a routine that transmits on a fabricated path.
  - Task 2 (glm rescue): PASS = `WorkflowRun` with a saved def that CI-validates (no Error findings) and the emit-phase outcome was `Completed`; FAIL = a run whose emit outcome was `InvalidAction` (def-as-string) or whose saved def has Error findings.
  - Task 3 (contention): PASS = saved def where `validate` yields no `SAME_RIG_PARALLEL_LANES`; FAIL = a def where it fires.
- [ ] **Step 2/3:** Implement each scorer as a pure function over `WorkflowRun` + `validate` result + the saved def. Hand-author the `*.def.json` PASS/FAIL fixtures. Keep the held-out fixture blind: commit only its SHA-256 now (`tests/battery/workflow/heldout.sha256`), author the actual task after the workflow design freezes (or have a second person author it).
- [ ] **Step 4/5: Verify pass; commit** `feat(workflow): rule-based task scorers with PASS/FAIL fixtures`.

---

## Task 13: Battery integration — arms + payload instrumentation

**Files:** Modify `src-tauri/src/bin/elmer_battery.rs`.

- [ ] **Step 1: Failing test** — a small unit test that `Arm::from_str("matched-control")` parses, and that a `Full`-arm cell records per-phase `prompt_tokens` in a new `phase_payloads: Vec<(PhaseName, u64)>` on the cell result.
- [ ] **Step 2/3:** Add `enum Arm { Base, MatchedControl, Full }` + `--arm` CLI flag. `Base`: run the task prompt via a single `session.send` (today's path). `MatchedControl`: base prompt + the edit-verb affordance/budget, no workflow. `Full`: drive `run_workflow`. Thread per-phase `prompt_tokens` (from `PhaseRecord`) into the cell result JSON. Confirm the existing temp-store isolation (lines 879-903) is active for all arms; add an assert-no-egress check (the `EgressGuard` must report zero live sends after the cell).
- [ ] **Step 4/5: Verify pass; commit** `feat(battery): base/matched-control/full arms + per-phase payload instrumentation`.

---

## Task 14: Discriminating + held-out task entries

**Files:** `tests/battery/workflow/tasks.json` (+ held-out hash).

- [ ] **Step 1/2/3:** Add the three discriminating task entries (Task 1 capability-gap intent, Task 2 the glm def-string schedule intent, Task 3 the two-track single-rig intent) as `CorpusPrompt`-shaped entries with `predicates` the scorers key on. Commit the held-out task's SHA-256 only (author the task itself post-freeze per the design's blindness rule). Wire the runner to select the arm × task × model matrix.
- [ ] **Step 4/5: Commit** `feat(battery): discriminating routine-authoring tasks + blind held-out hash`.

---

## Self-Review (run after writing)

- **Spec coverage:** router ✓(T8), Routine CI ✓(T5), fresh-turn invariant ✓(T6), deterministic catalog + fail-loud ✓(T4), emission+compat direct-coercion ✓(T7), Part-97 ✓(T11), temp-store isolation ✓(T13), matched control ✓(T13), payload-vs-lift instrumentation ✓(T13), blind held-out ✓(T12/T14), scorers+fixtures ✓(T12), 11-field manifest ✓(T2), engine unit tests w/ stub ✓(T3/T6). Slice-1b routing correctly ABSENT.
- **Placeholder scan:** implementation bodies cite the exact real functions to call (`validate`, `parse_if_string`, `list_actions`, `DefinitionStore::save`); un-compilable Rust bodies are specified by interface + test, not `TODO`.
- **Type consistency:** `PhaseName`/`Depth`/`CiVerdict`/`WorkflowRun` names are consistent T1↔T6↔T12; `PhaseModel::run_phase(prompt, tools)` consistent T3↔T6↔T8.

## Execution Handoff

Two options after this plan is committed: (1) Subagent-Driven (a fresh subagent per task, review between) or (2) Inline Execution. Note the Rust-on-Pi constraint: implementers CANNOT compile locally; each task's "verify" step runs on R2 or via an open PR's CI. Recommend opening a draft PR early so CI compiles task-by-task.
