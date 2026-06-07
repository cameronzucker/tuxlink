# Codex Primary-Agent Parity Checklist

This checklist is for Codex and other non-Claude harnesses entering Tuxlink
without loaded Claude context. It complements, but does not replace, CLAUDE.md
and AGENTS.md.

## Cold Start

1. Read AGENTS.md, then open CLAUDE.md and the linked sections relevant to the
   request.
2. Generate or adopt a session moniker:
   `python3 .claude/scripts/get_agent_moniker.py`.
3. State `Agent: <moniker>` in the first user-facing message.
4. Establish provenance: worktree path, current branch, local HEAD, dirty state,
   and relationship to `origin/main`.
5. Read the latest relevant handoffs, the bd issue, applicable ADRs/specs, and
   Claude memory files under
   `~/.claude/projects/-home-administrator-Code-tuxlink/memory/`.
6. Inspect recent PR precedent before opening or editing a PR.

## Skill Parity

If the request routes to a skill but the harness lacks Claude's Skill tool,
read the local `SKILL.md` and execute the workflow manually. Do not silently
substitute a generic Codex plan.

Common local roots:

- `/home/administrator/.claude/skills/`
- `/home/administrator/Code/agent-skills/plugins/`

Required behavior:

- Missing/unreadable skill: stop and diagnose.
- Required subagents: use Codex subagents, delegated Codex windows, or the
  operator-approved equivalent. Do not collapse the workflow to one agent.
- Required reports: write them where the skill prescribes, then keep code PRs
  scoped and reviewable.

High-friction skills:

- `investigate`: symptoms, reproduction, recent changes, root cause, then fix.
- `bug-hunt-cycle`: full phased hunt with exploratory, holistic, and multipass
  hunters before consolidated validation.
- `build-robust-features`: robust-feature plan/review/adrev discipline for
  non-trivial work.

## Git, PR, And Commit Forensics

- Use bd-bound worktrees under `worktrees/` for agent task branches when the
  worktree rules require it.
- Do not commit on merged/closed PR branches.
- Every local commit needs an `Agent: <moniker>` trailer. The repo
  `commit-msg` hook enforces this when `.githooks` is active.
- PR titles use `[moniker] <subject>`.
- PR bodies include exact verification provenance: path, branch, SHA, local vs
  CI, and branch/converged/package/release distinction.

## Remote Evidence

Use remote primary evidence for remote claims:

- `gh pr view`, `gh pr checks`, `gh run list`, `gh run view`
- `gh release view`
- `git show origin/main:<path>`

Do not infer current GitHub behavior from stale local workflow files. Keep these
distinct: PR checks, post-merge push workflows, tag workflows, and release-page
assets.

## Operator-Owned State

`.local/converge-build-worktree/` is not an agent worktree. Inspect only when it
blocks the operator, report exact paths, and wait for explicit path-level
authorization before cleanup.

Never initiate live radio transmission or live-CMS activity without explicit,
scoped, per-invocation consent from the licensee.

## Stop Conditions

Stop and surface evidence when:

- A required skill cannot be loaded or executed.
- A command would clean, delete, restore, overwrite, rebase, force-push, amend,
  or otherwise mutate operator-owned state.
- The operator challenges a factual claim.
- Remote evidence contradicts local checkout state.
- Verification would require live RF or other gated operator consent.
