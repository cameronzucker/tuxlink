# Session handoff — vale-towhee-heron (2026-07-22, execution checkpoint)

Second handoff of this session (supersedes the "redesign-and-slice1a-start" one for
current state). The complete Routine CI **workflow module (slice 1a Tasks 1-12) is
built, pushed, and CI-GREEN.** Remaining: Task 13 (battery wiring — has hidden scope),
Task 14 (corpus), then the adrev follow-on before the battery run.

## What is done + verified

- **Design + plan public on main** (predate all code): design `3144a682`, plan `c379fe46`.
- **Branch `bd-tuxlink-w8zxt/routine-ci-1a`, draft PR #1241**, worktree
  `worktrees/bd-tuxlink-w8zxt-routine-ci-1a` (ALIVE — do not dispose; bd tuxlink-w8zxt).
- **Tasks 1-12 CI-GREEN on tip `1dfd5967`**: `verify` (clippy -D warnings + cargo test)
  PASSED amd64 + arm64; ECT .deb build + install PASSED. (build-linux tauri-bundle was
  still finishing at checkpoint — verify+ECT green is the substantive gate.)
- The complete module `src-tauri/src/elmer/workflow/`:
  - `artifacts.rs` typed phase artifacts (the type contract) + PhaseName{Router,Intent,
    Feasibility,Draft,Emit,Ci,Present} + Depth{Minimal,Full}.
  - `manifest.rs` versioned WorkflowManifest (schema v1 gate) + load_manifest.
  - `model.rs` PhaseModel trait (dyn via #[async_trait]) + PhaseTurn + StubModel. **No
    production PhaseModel yet — that is Task 13.**
  - `catalog.rs` build_affordance_catalog (from list_actions ActionInfo, family-filtered,
    fail-loud on empty).
  - `ci.rs` run_routine_ci (validate -> CiReport; Red iff any Error; warnings stay Green).
  - `phases.rs` build_prompt (renders ONLY declared_inputs(phase) = context-bound invariant
    SoT), capture_artifact (parse_if_string direct-coercion; Emit reads store via
    crate::routines::store::DefinitionStore::get), tools_for (Emit gets edit-verb allow-set;
    Part-97 denylist excludes routines_enable/routines_run), CapturedArtifact.
  - `router.rs` select_depth (fail-safe to Full) + parse_depth + select_depth_with_tokens.
  - `present.rs` build_present (template: gaps from warnings, ack heuristic).
  - `engine.rs` run_workflow — the spine: Router -> depth phases -> read emitted routine
    from store -> Routine CI -> Present. Linear (Red/capture-fail stops). **Feasibility
    gate**: non-empty missing_primitives stops with an honest gap report (Task 1 mechanism).
    Red-build quarantines the dirty draft via store.delete. Context-bound invariant test +
    happy-path + Red + capability-gap tests.
  - `scorers.rs` score_task1_honesty / score_task2_editverb / score_task3_contention
    (keys on the SAME_RIG_PARALLEL_LANES **finding code**, NOT the verdict — it is a
    Warning) / score_heldout. PASS+FAIL fixtures each.
  - `part97_tests.rs`, `integration_tests.rs` (loads the REAL shipped manifest).
  - `resources/workflows/build-routine.manifest.json` — the shipped manifest.
- SDD ledger: `worktrees/bd-tuxlink-w8zxt-routine-ci-1a/.superpowers/sdd/progress.md`
  (recovery map — trust it + git log over recollection).

## Key decisions / corrections made in-flight (do not re-derive)

- Depth = {Minimal,Full} (a level), NOT PhaseName. PhaseName is the 7-phase pipeline.
- Store is `crate::routines::store::DefinitionStore` (MAIN crate) with get(name)/save/delete
  — the plan wrongly said tuxlink_routines::store.
- SAME_RIG_PARALLEL_LANES is a **Warning** (capability.rs), so CI never reds on it; Task 3's
  scorer keys on the finding-code presence, not the CiVerdict.
- ToolSpec.json_schema for edit verbs is a `{"type":"object"}` placeholder in slice 1a (real
  schemars wiring deferred; Task 11 only tests the denylist names, not schema fidelity).
- x4wax whole-def coercion is already landed on main (resolve_save_def) — Task 0 verified.

## REMAINING WORK (fresh pass, in order)

### Task 13 — battery wiring (THE effortful piece; has HIDDEN SCOPE)
Modify `src-tauri/src/bin/elmer_battery.rs`. Two parts:
1. **Build the production PhaseModel** (does NOT exist yet). An adapter implementing
   `super::super::elmer::workflow::PhaseModel::run_phase(prompt, tools) -> PhaseTurn` backed
   by the battery's real provider. STUDY: `src-tauri/src/elmer/session.rs` (`send`, the
   single-turn path the battery uses; `build_turn_provider_from_parts`), the
   `tuxlink-agent-runner` `run_with_conversation_with_transcript` loop
   (`src-tauri/tuxlink-agent-runner/src/runner.rs`), and how the battery constructs its
   session/provider in `run_cell` (~elmer_battery.rs:1356). Extract per-phase prompt_tokens
   from the ElmerEvent::Context path the battery already meters (Meters, make_battery_sink
   ~:626). This is un-compilable locally; iterate via PR #1241 CI. It is the crux.
2. Arms: `enum Arm { Base, MatchedControl, Full }` + `--arm`. Base = today's single
   session.send (unchanged). MatchedControl = base + the edit-verb affordance/budget, no
   workflow. Full = run_workflow driven by the production PhaseModel. Payload
   instrumentation: per-phase prompt_tokens into the cell result. Confirm temp-store
   isolation (already at ~:879-903) active; add an assert-no-egress check (EgressGuard
   reports zero live sends after the cell).

### Task 14 — corpus + blind held-out
`tests/battery/workflow/` task entries (CorpusPrompt shape: id/title/prompt/predicates):
Task 1 capability-gap intent, Task 2 the glm def-string schedule intent, Task 3 the
two-track single-rig intent. Commit ONLY the held-out task's SHA-256 (`heldout.sha256`) —
author the task itself post-freeze / by a second person (blindness discipline). Wire the
arm x task x model matrix into the runner.

### Follow-on BEFORE the battery run — adrev variant (tuxlink-u2qge)
Separate branch/PR. Cross-FAMILY adrev, orchestrated by the harness (two endpoints), NOT
agent self-review. Add `PhaseName::Adrev` + an optional `reviewer: Option<&dyn PhaseModel>`
on run_workflow (default = None); a `build-routine-adrev` manifest. **Placement: Adrev fires
AFTER Draft, BEFORE Emit** — the reviewer reads Intent + Draft and asks "does this fulfill the
objective?" (INTENT/OBJECTIVE FIDELITY — ORTHOGONAL to Routine CI's mechanical soundness,
NOT a deeper CI). Report-only in slice 1a. Battery arm wires author=local vLLM (Qwen 3.5
122b) + reviewer=OpenRouter (Nemotron 120b NVFP4) — dual-endpoint feasibility test. If it
validates -> graduates to a shipped UI element/setting (see u2qge notes). Land it before the
first battery run so the first run includes the cross-family arm.

## Starting the testing (the battery RUN — operator-gated boundary)
The run executes on **R2** (not the Pi), spends OpenRouter $ (ledger ~$27/$50), needs the
local vLLM endpoint up (twin-bramble) for the local arm. OPENROUTER_API_KEY from the Pi
keyring (`secret-tool lookup service elmer-openrouter`, piped to env, NEVER disk). A session
can build/wire/stage and even kick it off, but it is operator compute + money — cleanly the
operator initiates/authorizes and the session monitors (remote-run visibility). Also raise
the l264r $2 cell-ceiling for the experiment (it cancels the token-heavier Full arm — bias)
before running; rely on the $45 ledger stop. build-linux CI must be green so the battery
binary builds on R2.

## Rust build reality
No cargo on the Pi. CI (PR #1241) is the gate: verify = clippy -D warnings + cargo test on
amd64+arm64 (MSRV 1.75). Subagents write code + STOP; the parent commits; CI verifies.
Open-PR-early pattern is working.

## bd
- tuxlink-w8zxt (P1, in_progress): slice 1a; Tasks 1-12 done+green, 13-14 remain.
- tuxlink-u2qge (P2, open): adrev variant (pre-run follow-on) + product-graduation path.
- Parked (prior): l264r ceiling fix, new-model sweep, battery journal entry.
