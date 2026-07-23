# Session handoff — gully-cedar-birch (2026-07-23)

Long, high-arc session. Started as "continue the Routine CI battery (fix F1b/F2b)"
and pivoted entirely: the multi-phase **workflow engine is the wrong abstraction**
and has been **torn out**; the replacement is an **agent-driven routine-authoring
skill**, to be proven by a **valid Base-vs-+Skill lift experiment**. P0 teardown is
built + pushed; the design is settled; the next session builds P1.

## THE ONE THING TO KNOW

**Do NOT re-derive the workflow engine.** The whole point of this redesign is that
model cognition must be **agent-visible**; deterministic constraints stay in **code**
(a mechanical harness), and the scaffold is **prose the agent follows (a skill)** on
the normal Elmer agent loop with real tools — NOT a Rust engine that drives a blind
agent through hidden phases. Full rationale + the settled design is in memory
`project_routine_ci_workflow_is_lift_scaffold` and the GPT-5.6 consult transcript
`dev/scratch/tuxlink-elmer-routine-scaffold-redesign-conversation.md`.

## Operator decisions this session (SETTLED — do not relitigate)

1. **Throw out the JSON-phase workflow pipeline** (engine/phases/router/manifest/
   present/scorers). It was "homework" — hidden cognition, the model's weak path
   (free-text typed-JSON per phase), and it doesn't help the model.
2. **The scaffold = an agent-driven prose SKILL** ("Build Carefully" / Routine
   Authoring mode), invoked by user judgment for hard tasks. Deterministic pieces
   (validator, catalog, naming, loop-governor, result-budgets) stay in code as a
   **mechanical harness** the agent is *equipped with*, not driven by.
3. **Build BOTH designs and measure the diff** ("why guess when we can know?"):
   pure-prose skill (arm 2) vs skill + full mechanical harness (arm 7), each
   mechanical lever a **separate toggle** for ablation. The prose-vs-mechanics
   question is measured, not settled by argument.
4. **Reuse the existing difficulty-matrix corpus** — do NOT author new "impossible"
   tasks. Honesty/capability-gap tasks (`no_routine_expected`) stay and are scored on
   **honest refusal, not completion** (confabulation is the failure). Find the
   discriminating band by measuring each model's unaided ceiling.
5. **GPT-5.6 ban is LIFTED** — "too useful to not use." Repeal ADR 0023/0026 via a PR
   (ADR 0028). GPT-5.6-Sol is now callable (CLI-version issue was the blocker, not the
   subscription): `npx --yes @openai/codex@latest exec -m gpt-5.6-sol …` works today
   (installed codex 0.140.0 is too old; latest 0.145.0). Permanent fix needs
   `sudo npm install -g @openai/codex@latest` (operator sudo).

## What is DONE + pushed

- **bd `tuxlink-t3jci`** (P1 feature/epic) + worktree
  `worktrees/bd-tuxlink-t3jci-routine-authoring-scaffold` (branch
  `bd-tuxlink-t3jci/routine-authoring-scaffold`, off main). Has `node_modules`.
- **Plan doc**: `docs/superpowers/plans/2026-07-22-elmer-routine-authoring-scaffold.md`
  — settled design, six-sub-plan decomposition (P0–P5) with gates, detailed P0.
- **P0 teardown** (PR **#1245**, commits `6c1f5867` teardown + `9253746d` clippy fix):
  removed ~4380 lines of engine cognition; kept the reused mechanical pieces
  (`build_affordance_catalog`, `run_routine_ci`, and the `Affordances`/`CiReport`
  types in `artifacts.rs`). Battery keeps Base + MatchedControl as the reference.
- **PR #1244 CLOSED** (its F1b/N2 fixes die with the pipeline). The one
  product-relevant piece (context-overflow-400 → bounded `ContextWindowExceeded`
  provider fix) is **tracked as a bd task** for salvage off the ch4po branch (code
  preserved on `origin/bd-tuxlink-ch4po/round2-fullarm-fixes@cef44703`).

## CI STATUS — confirm before building on P0

At wrap time: `build-linux` passed on `6c1f5867` (the app compiles). `verify` (clippy
`-D warnings`) FAILED on `6c1f5867` on four leftovers (unused `Limits`/`RunEvent`/
`DefinitionStore`/`MonolithValidationContext` imports + orphaned `RunCellArgs.config_dir`);
**`9253746d` fixes exactly those**, but **a fresh CI run for `9253746d` had not
registered yet** (queue/concurrency lag). **NEXT-SESSION FIRST ACTION: confirm PR
#1245 verify is green for `9253746d` (re-trigger if no run appears — an empty commit
or `gh workflow run`), then merge P0.** The fixes are precise + high-confidence, but
verify it, don't assume.

## The plan (P0 done → P5), with gates

- **P1 — "Build Carefully" skill delivery** (WRITABLE NOW; seam pinned). Injection
  seam = `ElmerSession::send` where it resolves `system_prompt_override`
  (src-tauri/src/elmer/session.rs:393 → `build_turn_provider_from_parts`, system_prompt
  ~L919/970). When authoring mode is on, compose the effective system prompt =
  `(override ?? ELMER_SYSTEM_PROMPT) + "\n\n" + AUTHORING_SKILL`. Add an `elmer_send`
  param + thread it + a UI toggle in `ElmerPane`. **The SAME seam serves the battery
  +Skill arm** (it already builds `ElmerProvider::new_vetted(..., system_prompt, ...)`),
  so the A/B is confound-free. Only `system_prompt_override` (full-replace user
  setting) exists today — this is net-new *append* plumbing.
- **P2 — skill content** (compact ~9-step procedure; two-namespace framing at TOP:
  Elmer-time tools vs routine-time actions). **OPERATOR-REVIEW GATE.**
- **P3 — mechanical levers, each a toggle**: `routines_create` (title→kebab slug,
  absorbs naming = the doom-loop fix), no-progress governor, tool-result budgets
  (folds in bd `tuxlink-nirxk`), queryable catalog.
- **P4 — corpus REUSE** + unaided-ceiling protocol (3–5 baseline runs, pre-registered
  bands). **OPERATOR confirms keeping honesty tasks (recommended yes).**
- **P5 — eval harness**: Base vs +Skill then one-lever ablations; difficulty MATRIX
  (composition × semantic-selection × editing × capability/honesty × ambiguity ×
  side-effects); frozen substrate + recorded station fixtures; artifact-first BLINDED
  judge; four metric families (hard-task lift, easy-task regression, honesty/safety,
  resource/loop). Confound to fix: Emit tool schemas were degraded vs production.

## Pending / follow-ups (bd)

- `tuxlink-t3jci` — the epic (in_progress).
- Provider-fix salvage task (filed this session, P3) — re-apply the 400→ContextWindowExceeded
  classification from the ch4po branch onto a fresh branch.
- `tuxlink-nirxk` — tool-result balloon → folds into P3 result-budgets. Keep.
- `tuxlink-zwlv5` — Feasibility-gate hallucination → MOOT under teardown; close.
- `tuxlink-ch4po` — battery-fix issue, superseded by the teardown; close/note.
- ADR-repeal PR (0028, GPT-5.6 ban lift) — authorized, not yet done.

## Memories written this session (READ these, they carry the design)

- `project_routine_ci_workflow_is_lift_scaffold` — the whole redesign: lift-scaffold
  principle, agent-visible-cognition frame, build-both-and-measure, the mechanical
  harness list, experiment design, teach-before-action/absorb-at-boundary.
- Updated `feedback_gpt56_shadow_adrev` context (ban lifted).

## Worktree / branch state

- Main checkout is the operator's (branch bd-tuxlink-ant8s); untouched.
- `worktrees/bd-tuxlink-t3jci-routine-authoring-scaffold` — THIS work; branch
  `bd-tuxlink-t3jci/routine-authoring-scaffold` (PR #1245 open). node_modules present.
  This handoff + the plan doc live here (and in PR #1245, not yet on main).
- Prior worktrees `bd-tuxlink-ch4po-*` — battery/pilot work, now superseded
  (disposable per ADR 0009; code preserved on origin for the salvage).
