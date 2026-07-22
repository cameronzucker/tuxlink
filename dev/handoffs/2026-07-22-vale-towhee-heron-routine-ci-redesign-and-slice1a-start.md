# Session handoff — vale-towhee-heron (2026-07-22)

Continuation of bluff-oriole-basalt's 2026-07-22 session. This session took the
approved "Elmer as an agentic harness" design through a plan-eng-review gate that
uncovered the design was the wrong shape, redesigned it from first principles
(**Routine CI**), committed the new design + implementation plan publicly, and
STARTED execution (Task 1 landed, draft PR live, CI compiling).

## What happened this session (the arc)

1. **Committed the prior handoff** to main (`d0fceaf3`) from a clean worktree.
2. **Fixed the ant8s git situation** (operator-authorized one-time force-push).
   The stray commit `0726bcea` on `origin/bd-tuxlink-ant8s/ardop-connect-fixes`
   was 106 files of accidental `git add -A` sweep with ZERO source code (the real
   routines fix was already on main via #1234). Force-pushed the clean tip
   `81fd0a2a`; the remote is now clean. Swept docs/images remain on the operator's
   disk as untracked files (nothing lost). PR #1224 (misfiled do-not-merge) now
   points at the clean tip — operator may close it. Some swept handoffs (e.g.
   2026-06-10-gorge-cedar-hawk) are NOT on main; committing them is optional cleanup.
3. **Ran /plan-eng-review** on the approved office-hours design. Locked mechanism
   decisions (fresh-turn-per-phase, deterministic catalog, module boundary, no
   eval-only behavior + blind held-out, engine/scorer tests, payload-vs-lift) AND
   split the slice (1a linear, 1b routing). A Codex outside-voice round added 14
   findings. But the review surfaced that the design's 7-phase workflow was a
   **build-robust-features port ill-suited to a bounded domain**.
4. **Brainstormed the real design** (operator-driven, converged over ~6 questions):
   **Routine CI** (validator + wire-walk ARE the review — no LLM self-review, since
   Elmer can't spawn subagents; green build = disabled/attended draft) + the model
   **reaches for its own workflow depth** (self-triage as a scored gate capability).
   Emission stays model-emits-edit-verbs + compat layers (Task 2 measures them);
   code-compile is a FUTURE testable arm, not this slice.
5. **Committed the design** to main: `docs/superpowers/specs/2026-07-22-routine-ci-authoring-workflow-design.md` (`3144a682`).
6. **Wrote + committed the plan** to main: `docs/superpowers/plans/2026-07-22-routine-ci-slice-1a.md` (`c379fe46`) — 15 TDD tasks grounded in verified origin/main signatures.
7. **Started execution** (subagent-driven-development) on bd-tuxlink-w8zxt.

## Provenance note (important to the operator)

Routine CI is the operator's own clean-room Tuxlink concept, explicitly NOT
imported from his employer. The design + plan were committed to public `main`
BEFORE any implementation code — an immutable, timestamped, authored public
record. Draft PR #1241 for the code is anchored to those commits. This ordering
(concept public, then built in the open) is the "developed in the open, not
stolen from work" timeline the operator wanted.

## Current state

- **main:** carries the prior handoff, the Routine CI design + plan. Up to date.
- **Active feature branch:** `bd-tuxlink-w8zxt/routine-ci-1a` (off main at `c379fe46`).
  - **Draft PR #1241** — https://github.com/cameronzucker/tuxlink/pull/1241 — CI PENDING at handoff (verify + build-linux, amd64 + arm64).
  - Task 0 verified (x4wax whole-def coercion landed on main via `resolve_save_def`).
  - Task 1 committed `7eadcc09`: the `src-tauri/src/elmer/workflow/` module —
    typed phase artifacts (Intent, Affordances, Draft, CiReport, Present,
    WorkflowRun) + PhaseName/Depth enums + serde round-trip tests. Controller
    fixed two cross-task contract bugs the implementer flagged (Depth is
    {Minimal,Full}; PhaseName = Router/Intent/Feasibility/Draft/Emit/Ci/Present).
- **Active worktree:** `worktrees/bd-tuxlink-w8zxt-routine-ci-1a` (ADR-0008, bound to
  w8zxt). Untracked/gitignored: `node_modules/` (reproducible, installed for the
  pre-push docs-lint hook) and `.superpowers/sdd/` (the SDD ledger + task brief/report;
  gitignored scratch). No stashes of mine. Do NOT dispose — execution is in progress.
- **bd:** `tuxlink-w8zxt` (P1, in_progress) tracks the whole slice; its notes carry the
  execution status. SDD ledger: `worktrees/bd-tuxlink-w8zxt-routine-ci-1a/.superpowers/sdd/progress.md`.

## What is completed / in-progress / pending

- DONE: git fix, plan-eng-review, Codex round, redesign, design doc, plan, Task 0, Task 1 (pending CI).
- IN PROGRESS: slice 1a execution (Task 1 landed; 14 tasks remain).
- PENDING DECISION: none blocking. The plan's held-out task stays blind (author post-freeze / hash-precommit).

## CRITICAL first actions next session

1. **Check PR #1241 CI.** If Task 1 is red, fix it (foundational serde structs —
   likely a derive/import/mod nit; low risk) before continuing. `gh pr checks 1241`.
2. **Resume SDD execution at Task 2** (WorkflowManifest + loader). Read the SDD ledger
   first (it is the recovery map — trust it + `git log` over recollection). Tasks 3/4/5
   (PhaseModel+stub, catalog, CI wrapper) are parallelizable after the type contract.
   Continue subagent-driven-development: fresh implementer per task, tell each "You are
   agent <your-moniker>", give it the exact origin/main signatures from the plan's
   Global Constraints (do not let subagents invent APIs), and remember subagents CANNOT
   compile on the Pi and CANNOT commit in the worktree — they write, the parent commits,
   CI verifies.
3. Note the x4wax COR-3-ordering nuance to verify during Task 12 (the Task 2 scorer):
   confirm the battery/agent-runner path lets the def-string coercion run before COR-3
   rejects it.

## Parked (from bluff-oriole-basalt, still parked)

- New-model battery sweep (gpt-oss-120b, nemotron-120b, inkling via OpenRouter) on R2:
  `run-newmodels.sh`, partial data in `~/tuxlink-battery-build/battery-results/post-6epl8-1/`.
- The l264r $2 cell-ceiling fix (raise/remove for the workflow experiment) — the plan's
  NOT-in-scope calls it out; it is the parked confound fix, orthogonal to slice 1a.
- The corrected 4-model matrix + token-curve journal entry in dev/battery/journal.md.

## Rust build reality

No cargo on the Pi (IDE lockup). All Rust verification is via PR #1241's CI (MSRV 1.75;
verify + build-linux on amd64 + arm64). Open-PR-early is the pattern; each task's commit
triggers CI.
