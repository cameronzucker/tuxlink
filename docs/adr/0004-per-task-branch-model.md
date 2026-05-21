# 4. Per-task branch model with squash-merge

Date: 2026-05-05
Status: Accepted (squash-merge clause superseded by [ADR 0010](0010-no-squash-merge.md) 2026-05-17; the per-task-branch model itself, branch naming, post-PR delete, and the per-task wrap conventions remain accepted)
Deciders: cameronzucker, alder

## Context

The original v0.0.1 plan (committed 2026-04-22 by agent `lichen`) prescribed a single integration branch — `feat/v0.0.1` off `main` — with all 19 task commits landing directly on the integration branch in sequence. That model is what a solo human developer in a linear workflow would naturally pick: one head pointer, one final review, no merge conflicts.

By 2026-05-05, the project's orchestration model had evolved. Tuxlink uses parallel AI subagents, an `auto-claude` watchdog framework for unattended runs, and is preparing to adopt [Beads](https://github.com/steveyegge/beads) for dependency-aware task tracking. Cameron self-described the orchestration shape as "closer to dark factory than solo dev."

In the parallel-agent context, the single-branch model exposes three problems:

1. **Race surface.** Two subagents writing to the same branch tip at the same time produce merge conflicts that the agents are not well-equipped to resolve.
2. **Isolation loss.** A botched task in the middle of the chain cannot be reverted without affecting downstream tasks already committed.
3. **Orchestration mismatch.** `auto-claude`'s lease model assumes one agent per branch — single-branch means only one agent can work at a time, defeating the watchdog's parallelism. Beads' hash-based IDs assume per-issue branches.

The sister Geographica project documented two real near-misses where subagents on a single shared branch performed destructive operations during cross-task confusion (recovered via `git reflog`); the response in that project was to ban `git worktree`, which addressed the symptom but not the root cause.

## Decision

Tuxlink v0.0.1 (and successor pre-1.0 development cycles) uses a **per-task-branch model**:

1. **One-time:** `feat/v0.0.1` is created off `main` and pushed to origin. It is the integration branch for the v0.0.1 release cycle.
2. **Per task:** the agent (or human) branches from `feat/v0.0.1` (`task-NN-<slug>` or, with Beads, `bd-<id>/<slug>`), implements the task on that branch, runs tests, opens a PR against `feat/v0.0.1`.
3. **After PR approval** (review subagent or human): squash-merge into `feat/v0.0.1` carrying the task commit's subject + body, delete task branch with `-d` (NOT `-D`).
4. **At release** (Task 19): `feat/v0.0.1` ff-merges into `main`, tag `v0.0.1`, push.

**Direct commits to `main` or `feat/v0.0.1` are blocked at the harness level** by a `PreToolUse` hook on `git commit`. A one-shot `ALLOW_INTEGRATION_COMMIT=1` environment-variable carve-out exists for the legitimate squash-merge step.

## Consequences

**Positive:**
- Isolation: a botched task is a discardable branch, not a contamination of the chain.
- Parallelism: Agent A on `task-3` and Agent B on `task-7` cannot collide.
- Review granularity: one task's diff per PR.
- Composes with `auto-claude`: `session_boot.sh` does the "before" branch ops, `session_exit.sh` does the squash-merge.
- Composes with Beads: branch names become `bd-<id>/<slug>` with collision-free hash IDs.
- Engineering control on integration branches via the hook converts an administrative rule ("don't commit to main") into a structural one (the harness refuses to).

**Negative:**
- 19 PRs to review for v0.0.1 instead of one. Mitigated by review subagents and the small per-PR diff size.
- Adds branch-creation and squash-merge steps to every task. Mitigated by `auto-claude` automating both ends.
- The `ALLOW_INTEGRATION_COMMIT=1` carve-out is a foot-gun if used carelessly; the env var is per-shell-invocation, not persistent.

## Alternatives considered

- **Single integration branch with all 19 task commits on it (the original plan).** Rejected for the reasons above. Was the implicit default in the 2026-04-22 plan; the plan has since been patched to reflect this ADR.
- **Stacked PRs (Sapling / Graphite style).** Each task is a PR depending on the previous, forming a stack. Rejected for v0.0.1: requires Graphite tooling investment, adds operational overhead, doesn't compose as cleanly with `auto-claude`'s lease-per-branch model. Reconsider for future workflows where review density is the bottleneck.
- **Per-task branch with merge-commit (no squash).** Rejected. Preserves the per-step commit history on `feat/v0.0.1` but produces a noisier integration branch that's harder to bisect. Squash-merge keeps `feat/v0.0.1` history one-commit-per-task while the un-squashed task branch retains the per-step record for the PR review trail.
- **`git worktree` for branch isolation.** Initially used in geographica, then banned project-wide after subagents confused worktree topology and performed destructive ops on the wrong checkout. Banned in tuxlink CLAUDE.md from day 1 — see [CLAUDE.md §Git workflow](../../CLAUDE.md#git-workflow--worktrees-are-banned).
