# Tuxlink

> **Note:** This file is a **summary with links** into [CLAUDE.md](CLAUDE.md)
> for non-Claude agent harnesses (Codex, etc.). The substantive rules live in
> CLAUDE.md; this file points at them. When CLAUDE.md changes, update the
> summary line here only if the change is something a non-Claude agent reading
> just this file needs to see.

> **Project framing is pending.** This repo has just been initialized. The
> project structure, commands, testing, and hardware sections below are
> placeholders that will be filled in during the office-hours kickoff
> session. The ethos + workflow + safety sections are in force from day 1
> and should not wait for framing.

## Project structure

_TBD — populate after office-hours kickoff._

## Commands

_TBD — populate after office-hours kickoff._

## Testing

_TBD — populate after office-hours kickoff._

## Skill routing

When the user's request matches an available skill, ALWAYS invoke it using the Skill
tool as your FIRST action. Do NOT answer directly, do NOT use other tools first.

Key routing rules:
- Product ideas, "is this worth building", brainstorming → invoke office-hours
- Bugs, errors, "why is this broken", 500 errors → invoke investigate
- Ship, deploy, push, create PR → invoke ship
- QA, test the site, find bugs → invoke qa
- Code review, check my diff → invoke review
- Update docs after shipping → invoke document-release
- Weekly retro → invoke retro
- Design system, brand → invoke design-consultation
- Visual audit, design polish → invoke design-review
- Architecture review → invoke plan-eng-review
- Save progress, checkpoint, resume → invoke checkpoint
- Code quality, health check → invoke health

## Brainstorming preferences

- Always use the visual companion (browser mockups) during brainstorming — don't ask, just launch it
- Token budget is not a concern during design phases — be thorough

## Project ethos

Tuxlink is Cameron's learning sandbox for AI-assisted development techniques —
custom skills, adversarial review, multi-agent teaming, capability mapping —
that he plans to transfer to high-stakes projects at his employer. The
shipped software matters, but **professional-development outcomes are a
first-class goal alongside features.**

Implications:
- Process rigor > raw velocity. Do the right thing, not the fast thing.
- Explain when/what for new workflows so Cameron builds transferable skill.
- Prefer patterns that generalize to multi-developer / higher-stakes environments.
- Signal professional polish even at A-audience scale — the surface area of the repo (commits, CHANGELOG, versioning, CI) teaches Cameron what "good" looks like and builds habits that transfer.

## Agent identity

Generate a moniker via `python3 .claude/scripts/get_agent_moniker.py` at session start (3-word hyphenated form drawn from a 100-word pool of plant / animal / geographic nouns; auto-pre-flighted against git history). Include `Agent: <moniker>` as a commit trailer on every commit. Pass the moniker through to every subagent you dispatch. Legacy single-word monikers in older commits remain valid; the new format applies to forward commits. See [CLAUDE.md](CLAUDE.md#agent-identity--pick-a-moniker-at-session-start) for the full rationale and workflow.

## Git workflow — worktrees mandatory under bd-issue ownership (ADR 0008); destructive commands BANNED

See [CLAUDE.md](CLAUDE.md#git-workflow--worktrees-mandatory-under-bd-issue-ownership-adr-0008), [CLAUDE.md](CLAUDE.md#git-workflow--destructive-commands-are-banned), [docs/adr/0008-worktrees-mandatory-under-bd-issue-ownership.md](docs/adr/0008-worktrees-mandatory-under-bd-issue-ownership.md), and [docs/adr/0007-lift-worktree-ban.md](docs/adr/0007-lift-worktree-ban.md) for full context. Summary: when the `.claude/hooks/block-main-checkout-race.sh` hook denies a write op citing "another live session is active," create a worktree per the QUICK FIX in the deny message and re-run there — the hook's determination is authoritative; agents do not re-decide it via `get_tuxlink_sessions.py` or any other source. Every worktree binds to a bd issue (`bd show <id>` answers "what is `worktrees/X` for?"). When the hook does NOT deny, main-checkout writes are fine. Destructive git commands remain banned regardless of worktree topology — no `reset --hard`, no force push, no `--amend` on pushed commits, no `--no-verify`, no `git worktree remove`, no `git rebase -i`. If you think you need a banned command, stop and ask.

## Git workflow — branch lifecycle state machine (ADR 0017)

See [CLAUDE.md](CLAUDE.md#git-workflow--branch-lifecycle-state-machine-adr-0017) and [docs/adr/0017-branch-state-machine.md](docs/adr/0017-branch-state-machine.md). Summary: a branch with a merged or closed-without-merge PR is **dead** — `.githooks/pre-commit` and `.githooks/pre-push` refuse further commits/pushes on it (the orphan-post-merge anti-pattern from the 2026-06-01 v1p incident). Activate the hooks on any fresh clone with `bash scripts/install-githooks.sh`. The hooks use `gh pr list` for state classification and degrade gracefully (warn + allow) when `gh` is unavailable; the CI nightly audit (tuxlink-ui3i) is the independent backstop. Documented escape hatch: `TUXLINK_BRANCH_LIFECYCLE_OVERRIDE=I-know-what-Im-doing git commit ...` — loud + audited at `dev/scratch/branch-lifecycle-overrides.log`.

## Live radio network operations — READ BEFORE ANY TRANSMISSION

No automation, test, subagent, CI job, scheduled task, or AI agent
initiates a transmission under the project's amateur callsign without
explicit, scoped, per-invocation consent from the licensee. Full rules
at [docs/live-cms-testing-policy.md](docs/live-cms-testing-policy.md)
and RADIO-1 in [docs/pitfalls/implementation-pitfalls.md](docs/pitfalls/implementation-pitfalls.md).
This is Part 97 regulatory compliance, not a style rule.

## Commit and release discipline

Conventional commit types (`feat:`, `fix:`, `docs:`, etc.). Breaking changes get `!` + `BREAKING CHANGE:` footer. Update `dev/implementation-log.md` (once created) after any significant work item. **Squash-merge is banned** ([ADR 0010](docs/adr/0010-no-squash-merge.md)); all PRs into integration branches merge as merge-commit (no-ff) via `gh pr merge <#> --merge --delete-branch`. **Polish before push** — clean up WIP commits via non-interactive `git rebase <base>` on local un-pushed commits; once pushed, commits are immutable.

## Tool referee (overrides bd's CLAUDE.md defaults)

This project uses bd (Beads) AND Claude Code's built-in primitives (TodoWrite, auto-memory). They are NOT substitutes. When bd's BEADS INTEGRATION section conflicts with project commitments, the `## Tool referee` table in [CLAUDE.md](CLAUDE.md#tool-referee--which-tool-owns-which-job) wins. Specifically: TodoWrite is for in-turn micro-progress; bd is for cross-session work; auto-memory at `~/.claude/projects/<slug>/memory/` is canonical for user/feedback memory. (The prior push-timing override was retired 2026-05-17 — push is now mandatory at session end per the §Session Completion in CLAUDE.md, agreeing with bd's directive.) See [docs/adr/0006-override-bd-claude-md-defaults.md](docs/adr/0006-override-bd-claude-md-defaults.md) for rationale.

## Extended capabilities on this Pi

- **Codex CLI** (adversarial review) — `/usr/local/bin/codex` or `npx --yes @openai/codex`; already authenticated. **For directed adrev with custom prompts:** `cat prompt.txt | codex review -` (CLI v0.128.0 rejects combining `--base`/`--commit` with `[PROMPT]`). See [CLAUDE.md](CLAUDE.md#openai-codex-cli--for-build-robust-features-at-least-one-adversarial-round-via-codex-requirement) for full usage.
- **url-to-markdown** skill — prefer over WebFetch for full-page retrieval. See [CLAUDE.md](CLAUDE.md#url-to-markdown-skill--fetch-full-webpages-not-summaries).

## Session Completion

Work is not complete until `git push` succeeds AND a session-end handoff document exists. **Seven required steps** (the BEADS INTEGRATION block below has its own version, superseded by this canonical section):

1. File issues for remaining work.
2. Run quality gates if code changed.
3. Update issue tracker status (`bd close <id>` / `bd update <id>`).
4. `git push` — mandatory; retry until it succeeds.
5. Clean up stashes + ensure remote task branches are deleted.
6. Write a session-end handoff at `dev/handoffs/<YYYY-MM-DD>-<short-slug>.md` enumerating branch + working-tree + worktree state per [ADR 0009](docs/adr/0009-worktree-disposal-ritual.md).
7. **Surface the operator's next-session starting prompt** as your final user-facing message — a ~10-line paste-ready code block with: one-sentence session summary, pointer to the handoff doc, critical-first-action / gate emphasis. Reduces session-change friction.

See [CLAUDE.md §Session Completion](CLAUDE.md#session-completion) for full text + rationale.

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:ca08a54f -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

## Session Completion

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   bd dolt push
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
<!-- END BEADS INTEGRATION -->
