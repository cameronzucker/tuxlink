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

Pick a moniker (single lowercase word, ctrl+F-friendly, not a human first name) at session start and include `Agent: <moniker>` as a commit trailer on every commit. Pass the moniker through to every subagent you dispatch. See [CLAUDE.md](CLAUDE.md#agent-identity--pick-a-moniker-at-session-start) for the full rationale and workflow.

## Git workflow — worktrees are BANNED, destructive commands are BANNED

See [CLAUDE.md](CLAUDE.md#git-workflow--worktrees-are-banned) and [CLAUDE.md](CLAUDE.md#git-workflow--destructive-commands-are-banned) for the full list and rationale. Summary: all branch work in the main repo, no `git worktree`, no `reset --hard`, no force push, no `--amend` on pushed commits, no `--no-verify`. If you think you need one of these, stop and ask.

## Live radio network operations — READ BEFORE ANY TRANSMISSION

No automation, test, subagent, CI job, scheduled task, or AI agent
initiates a transmission under the project's amateur callsign without
explicit, scoped, per-invocation consent from the licensee. Full rules
at [docs/live-cms-testing-policy.md](docs/live-cms-testing-policy.md)
and RADIO-1 in [docs/pitfalls/implementation-pitfalls.md](docs/pitfalls/implementation-pitfalls.md).
This is Part 97 regulatory compliance, not a style rule.

## Commit and release discipline

Conventional commit types (`feat:`, `fix:`, `docs:`, etc.). Breaking changes get `!` + `BREAKING CHANGE:` footer. Update `dev/implementation-log.md` (once created) after any significant work item.

## Tool referee (overrides bd's CLAUDE.md defaults)

This project uses bd (Beads) AND Claude Code's built-in primitives (TodoWrite, auto-memory). They are NOT substitutes. When bd's BEADS INTEGRATION section conflicts with project commitments, the `## Tool referee` table in [CLAUDE.md](CLAUDE.md#tool-referee--which-tool-owns-which-job) wins. Specifically: TodoWrite is for in-turn micro-progress; bd is for cross-session work; auto-memory at `~/.claude/projects/<slug>/memory/` is canonical for user/feedback memory; the operator owns push timing. See [docs/adr/0006-override-bd-claude-md-defaults.md](docs/adr/0006-override-bd-claude-md-defaults.md) for rationale.

## Extended capabilities on this Pi

- **Codex CLI** (adversarial review) — `npx --yes @openai/codex exec "<prompt>"`. Not on `$PATH`; already authenticated. See [CLAUDE.md](CLAUDE.md#openai-codex-cli--for-build-robust-features-at-least-one-adversarial-round-via-codex-requirement) for full usage.
- **url-to-markdown** skill — prefer over WebFetch for full-page retrieval. See [CLAUDE.md](CLAUDE.md#url-to-markdown-skill--fetch-full-webpages-not-summaries).

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
