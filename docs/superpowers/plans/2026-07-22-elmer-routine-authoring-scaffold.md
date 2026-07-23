# Elmer Routine-Authoring Scaffold — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: use superpowers:subagent-driven-development or superpowers:executing-plans to implement each sub-plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Replace the discarded hidden "Routine CI" workflow engine with a user-invoked, agent-driven routine-authoring **skill**, and prove (or disprove) that it lifts weaker models via a valid experiment.

**Architecture:** The agent drives the normal continuous Elmer agent loop with the real routines MCP tools; a compact **authoring skill** (prose the agent follows, injected whole when the user selects "Build Carefully") supplies the procedure. Deterministic constraints stay in **code** (the mechanical harness); only hidden *cognition* is deleted. We build both the pure-prose skill and the skill+mechanical-harness, each mechanical lever independently toggleable, and **measure the difference**.

**Tech Stack:** Rust (`src-tauri`), Tauri commands, the existing routines MCP tool surface + validator + affordance catalog, the existing battery plumbing (`run-matrix.sh`, `elmer_score`), qwen-3.5-122b (local, R2) + hosted models via OpenRouter.

## Global Constraints

- MSRV 1.75 (`src-tauri/Cargo.toml`); clippy `incompatible_msrv` denied — no APIs stabilized 1.76+.
- **No `cargo` builds/tests on this dev Pi** — write the Rust + tests, push, let CI (both arches) or R2 compile/run. `pnpm vitest run` on a single file is fine locally.
- Conventional commits; `Agent: <moniker>` + `Co-Authored-By` trailers; per-task branches `bd-<id>/<slug>`; worktrees under bd-issue ownership (ADR 0008).
- RADIO-1 (ADR 0018): agents write/test transmit-path code via mocks/loopback/CI; never key a real radio. The authoring benchmark stops at a **created draft** — never enabled, run, or transmitting.
- **Design north-star (do not re-derive the engine):** model cognition must be agent-visible; deterministic constraints stay in code. In-the-moment "teaching" is NOT a lift mechanism (models loop, don't adapt) — **absorb** what the model emits at the boundary. Full rationale: `dev/scratch/tuxlink-elmer-routine-scaffold-redesign-conversation.md` (GPT-5.6 review, repo-grounded @e805a97) + memory `project_routine_ci_workflow_is_lift_scaffold`.

---

## Decomposition (sub-plans, sequence, gates)

| # | Sub-plan | Depends on | Gate |
|---|---|---|---|
| **P0** | Teardown + deck-clear (detailed below) | — | none |
| **P1** | "Build Carefully" skill-delivery plumbing | P0 | delivery design (I propose, operator sanity-checks) |
| **P2** | The authoring skill content | P1 | **operator reviews content** |
| **P3** | Mechanical levers, each a toggle: `routines_create`, no-progress governor, tool-result budgets, queryable catalog | P0 | none (feature-flags) |
| **P4** | Corpus: **reuse** the existing difficulty matrix; measure unaided ceiling to find the discriminating band; honesty/gap tasks scored on honest refusal, not completion | — | reuse (no new authoring); operator confirms keeping honesty tasks |
| **P5** | Eval harness: Base vs +Skill (+ ablation arms), unaided-ceiling protocol (3–5 baseline runs, pre-registered bands), frozen substrate + station fixtures, artifact-first blinded judge, four metric families (hard-task lift, easy-task regression, honesty/safety, resource/loop) | P1,P2,P3,P4 | — |

**Sequence:** P0 → (P1+P2 core scaffold) → P3 toggles → P5 harness; **P4 runs in parallel** (reuse). First measurement is the clean Base-vs-+Skill A/B; ablations follow.

**P1–P5 are detailed in follow-up plans** once their prerequisites are met (P1 after the delivery-mechanism design pass; P2/P4 at the operator-review checkpoints). This document details **P0** in full.

---

## Teardown inventory (from `src-tauri/src/elmer/workflow/`, 4483 lines)

**Reused (keep):**
- `catalog.rs` — `build_affordance_catalog` (affordance enumeration).
- `ci.rs` — `run_routine_ci` (deterministic validator adapter).
- From `artifacts.rs`: the types the two above reference — `Affordances`, `AffordanceAction`, `CiReport`, `CiVerdict`, `CiFinding`.

**Rotten (delete):**
- `engine.rs` (`run_workflow`), `phases.rs` (`build_prompt`/phase instructions/`capture_artifact`), `model.rs` (`PhaseModel`/`SessionPhaseModel`), `manifest.rs` (`WorkflowManifest`), `present.rs` (`build_present`), `router.rs` (`select_depth`), `scorers.rs` (WorkflowRun-scorers), `integration_tests.rs`, `part97_tests.rs`.
- From `artifacts.rs`: `Intent`, `Draft`, `DraftNode`, `DraftBranch`, `Present`, `PhaseRecord`, `WorkflowRun`, `Depth`, `PhaseName`, and the F1b lenient deserializers (`de_string_lenient`, `de_vec_string_lenient`, `value_to_string`).

**Consumers to rework:** `src/bin/elmer_battery.rs` (drop the Full arm + `run_full_arm` + 3-arm enum; keep credits/meters/bundle/corpus plumbing + the Base arm as the reference), `src/elmer/mod.rs` (`pub mod workflow;` stays; workflow re-exports shrink).

---

### Task 0.1: Salvage the provider context-overflow fix onto its own branch

The one commit worth keeping from the abandoned battery-fix PR #1244 is the provider-level `ContextWindowExceeded` classification (protects any near-window Elmer turn, product-wide).

**Files:** none created; git-only (cherry-pick `src-tauri/tuxlink-agent-frontend/src/provider.rs` changes from `cef44703`).

- [ ] **Step 1: Create a branch off main for the salvage**

```bash
cd /home/administrator/Code/tuxlink   # standalone cd first (race hook)
```
```bash
python3 .claude/scripts/new_tuxlink_worktree.py --slug provider-ctx-overflow-400 --issue tuxlink-t3jci --base main --moniker gully-cedar-birch
```

- [ ] **Step 2: Re-apply ONLY the F2b/N1 change** (the `is_context_overflow_body` helper + the 400→`ContextWindowExceeded` classification + its test) from `provider.rs` as it exists on `bd-tuxlink-ch4po/round2-fullarm-fixes@cef44703`. Do NOT bring the F1b JSON-shape prompts or N2 Emit resolver — those die with the pipeline. Diff to copy: the `is_context_overflow_body` fn, the `if status.as_u16() == 400 && is_context_overflow_body(&snippet)` block, and the `context_overflow_body_is_detected_but_ordinary_400s_are_not` test. Leave the proportional-margin changes out unless review wants them (they are harmless but pipeline-motivated).

- [ ] **Step 3: Push + open PR**, let CI compile/run both arches. Verify green.

- [ ] **Step 4: Close PR #1244** with a comment pointing at the salvage PR and this plan.

```bash
gh pr close 1244 --comment "Superseded: the pipeline these fixes served is being torn out (bd tuxlink-t3jci). The one product-relevant piece (context-overflow-400 → bounded ContextWindowExceeded) is salvaged in PR #<salvage>."
```

### Task 0.2: Split `artifacts.rs` — keep only the reused types

**Files:**
- Modify: `src-tauri/src/elmer/workflow/artifacts.rs` (reduce to `Affordances`, `AffordanceAction`, `CiReport`, `CiVerdict`, `CiFinding` + their round-trip tests)
- Modify: `src-tauri/src/elmer/workflow/mod.rs`

- [ ] **Step 1:** In `artifacts.rs`, delete `Intent`, `Draft`, `DraftNode`, `DraftBranch`, `Present`, `PhaseRecord`, `WorkflowRun`, `Depth`, `PhaseName`, the three lenient deserializer fns, and every test exercising a deleted type. Keep `Affordances`, `AffordanceAction`, `CiReport`, `CiVerdict`, `CiFinding` and their round-trip tests.

- [ ] **Step 2:** Confirm `catalog.rs` and `ci.rs` still reference only retained types (compiler is the check in Step 4).

- [ ] **Step 3:** In `mod.rs`, reduce the `pub use artifacts::{...}` line to the retained types only.

- [ ] **Step 4 (verify — CI/R2, not Pi):** `cargo build --manifest-path src-tauri/Cargo.toml` compiles once Tasks 0.3–0.5 land; standalone this task won't compile (dependents still reference deleted types) — so commit 0.2–0.5 together and verify at 0.5.

### Task 0.3: Delete the rotten modules

**Files:** Delete `engine.rs`, `phases.rs`, `model.rs`, `manifest.rs`, `present.rs`, `router.rs`, `scorers.rs`, `integration_tests.rs`, `part97_tests.rs` under `src-tauri/src/elmer/workflow/`.

- [ ] **Step 1:** `git rm` the nine files.
- [ ] **Step 2:** In `mod.rs`, remove their `pub mod` lines and every `pub use` re-export sourced from them (`engine::*`, `phases::*`, `model::*`, `manifest::*`, `present::*`, `router::*`, `scorers::*`).

### Task 0.4: Retract the workflow re-export surface

**Files:** Modify `src-tauri/src/elmer/workflow/mod.rs` so the module exports only: `build_affordance_catalog`, `CatalogError`, `run_routine_ci`, and the retained artifact types.

- [ ] **Step 1:** Rewrite `mod.rs` to the minimal surface. Keep the module doc but strip references to the deleted pipeline.

### Task 0.5: Strip the Full arm from the battery; keep the plumbing

**Files:** Modify `src-tauri/src/bin/elmer_battery.rs`.

- [ ] **Step 1:** Remove `run_full_arm`, `RunFullArgs`, `outcome_from_workflow_run`, `phase_payloads_of`, the `Arm::Full` variant and its match arm, and every `workflow::`/`run_workflow`/`WorkflowRun`/`PhaseName` import. Keep `Arm::Base` (the reference). `Arm::MatchedControl` may stay as an optional confound-control arm OR be removed — remove it for now; P5 reintroduces arms deliberately.
- [ ] **Step 2:** Remove `phase_payloads` and `workflow_run` from `CellResult` and from the `outcome.json` writer; keep `outcome`, credits, meters, cost, `tool_schema_sha256`.
- [ ] **Step 3:** Delete tests referencing deleted symbols (`phase_payloads_projects_*`, `outcome_from_workflow_run_*`).

- [ ] **Step 4 (verify — CI/R2):** push the 0.2–0.5 commit; CI `verify` (clippy `--all-targets --locked -D warnings` + `cargo test`) must pass on both arches. On R2: `cargo build --manifest-path src-tauri/Cargo.toml --bin elmer_battery --bin elmer_score` compiles; a `base/<task>` cell still runs and writes `outcome.json`.

### Task 0.6: Commit teardown + update trackers

- [ ] **Step 1:** Commit 0.2–0.5 as one atomic teardown (they only compile together):

```bash
git add -A src-tauri/src/elmer/workflow/ src-tauri/src/bin/elmer_battery.rs
git commit -m "$(cat <<'EOF'
refactor(elmer)!: tear out the hidden routine-CI workflow engine (tuxlink-t3jci)

Removes the discarded phase-engine cognition (engine/phases/model/manifest/
router/present/scorers + phase-artifact structs + the battery Full arm),
keeping only the reused mechanical pieces: the affordance catalog
(build_affordance_catalog), the deterministic validator (run_routine_ci),
and their types. The Base arm (production agent loop + routines tools)
remains as the experiment's reference. Delivery of the replacement authoring
skill is P1 (bd tuxlink-t3jci).

BREAKING CHANGE: the elmer::workflow public surface no longer exports the
phase pipeline (run_workflow, PhaseModel, WorkflowManifest, build_prompt, …).

Agent: gully-cedar-birch
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 2:** `bd update tuxlink-ch4po` — note the battery-fix work is superseded by the teardown; close if nothing else pends. Close `tuxlink-zwlv5` (Feasibility-gate: moot under teardown). Keep `tuxlink-nirxk` (tool-result balloon → folds into P3 result-budgets).

- [ ] **Step 3:** Push; open the teardown PR; CI green; merge.

---

## Self-review notes

- **Spec coverage:** P0 covers teardown + deck-clear + salvage. The design/experiment (P1–P5) are captured as the decomposition + north-star references; each gets its own detailed plan at its gate.
- **No placeholders in P0:** every task names exact files/symbols. The one deferral ("re-apply the F2b/N1 diff") names the exact commit + the exact functions to copy.
- **Type consistency:** retained types (`Affordances`/`AffordanceAction`/`CiReport`/`CiVerdict`/`CiFinding`) match the reused `catalog.rs`/`ci.rs` consumers; deleted types are removed from every consumer in the same atomic commit.
