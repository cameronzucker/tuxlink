# Tuxlink

> **Note:** This file is a **summary with links** into [CLAUDE.md](CLAUDE.md)
> for non-Claude agent harnesses (Codex, etc.). The substantive rules live in
> CLAUDE.md; this file points at them. When CLAUDE.md changes, update the
> summary line here only if the change is something a non-Claude agent reading
> just this file needs to see.
>
> **Codex / non-Claude agents:** do not treat this file as a complete operating
> manual. It is the entry point. Before substantive work, open CLAUDE.md and the
> linked ADR/spec/pitfall sections that govern the request. If you cannot load
> or follow a referenced local skill/workflow, stop and diagnose that failure;
> do not silently substitute an ad-hoc process.

> **Project framing is pending.** This repo has just been initialized. The
> project structure, commands, testing, and hardware sections below are
> placeholders. The office-hours kickoff session will populate them. The ethos
> + workflow + safety sections are in force from day 1 and should not wait for
> framing.

## Project structure

_TBD: populate after office-hours kickoff._

## Commands

_TBD: populate after office-hours kickoff._

## Testing

_TBD: populate after office-hours kickoff._

## Codex / non-Claude startup contract

For every fresh Codex/non-Claude session, complete this checklist before code,
git, bd status changes, PRs, or release claims:

1. Generate or adopt the session moniker. Normal path: run
   `python3 .claude/scripts/get_agent_moniker.py`, state `Agent: <moniker>` in
   the first user-facing message, and persist that moniker for the whole
   session. If resuming a session that already made commits/PRs with a moniker,
   keep the existing moniker for continuity.
2. Read this file and CLAUDE.md. Treat CLAUDE.md as authoritative; this file is
   a non-Claude summary and routing layer.
3. Establish repository provenance before making claims: current branch,
   `git status --short`, worktree path, local HEAD, and relationship to
   `origin/main`. Local checkouts may be stale; inspect `origin/main` directly
   for remote workflow/release facts.
4. Read the most recent relevant handoffs under `dev/handoffs/`, the bd issue
   being worked, the applicable ADRs/specs/pitfalls, and Claude memory files at
   `~/.claude/projects/-home-administrator-Code-tuxlink/memory/` when the work
   is more than a trivial one-command answer.
5. Inspect recent PR precedent before opening or editing a PR. Match title,
   body, verification, moniker, and merge-discipline conventions unless an ADR
   or explicit operator instruction says otherwise.
6. If the user is reporting a bug, failed build, failing CI, broken release, or
   surprising runtime behavior, follow the investigate/bug-hunt workflow before
   fixing. Evidence first, fix second.

Detailed checklist: [docs/agent-workflows/codex-primary-agent-parity.md](docs/agent-workflows/codex-primary-agent-parity.md).

## Evidence-first behavior protocol

This protocol is mandatory for Codex / non-Claude agents. It applies to
behavior questions, bug analysis, logging/session/user-visible surface changes,
CI/release workflow changes, and any user-visible regression.

1. Do not propose an implementation shape until the exact current call paths
   involved have been inspected in `origin/main`.
2. Do not rely on PR titles, PR bodies, memory, summaries, handoffs, or prior
   agent claims as evidence. Inspect the merged diff and current code.
3. Answer behavior questions in this order:
   - Observed code facts, with file/function references.
   - Uncertainty or unverified areas.
   - Implications.
4. If the user asks "does this mean X," verify X directly before answering.
5. For logging/session/user-visible surfaces, trace both directions before
   proposing a fix:
   - What reaches the UI/operator-visible surface.
   - What reaches diagnostic/export/archive surfaces.
6. Say "I have not verified that yet" instead of making a confident
   architectural inference.
7. Any PR touching these areas must include an `Evidence Checked` section that
   lists the inspected call paths and remaining uncertainty. A vague or missing
   evidence section is grounds to reject the PR.

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

Codex fallback rule: if the native harness does not expose Claude's Skill tool
or does not list a named project skill, read the local `SKILL.md` and follow it
manually. Common local roots include `/home/administrator/.claude/skills/` and
`/home/administrator/Code/agent-skills/plugins/`. A missing or unreadable skill
is a blocker to surface, not permission to improvise.

When a skill prescribes subagents, use the available subagent mechanism or
operator-delegated Codex windows exactly as the skill requires. Do not collapse
a required multi-agent bug hunt, adversarial review, or plan review into a
single-agent pass for convenience. Persist the reports where the skill says to
persist them, then keep PR scope reviewable.

Project-specific high-friction routes:
- `bug-hunt-cycle` means the full phased workflow: scope, parallel exploratory
  / holistic / multipass hunters, consolidated validation, test-gap analysis,
  presentation, fix plan, plan review, and committed reports when required.
- `investigate` means no code fix until symptoms, recent changes, reproduction,
  root cause, and regression coverage are established.
- `build-robust-features` means the project's robust feature discipline,
  including required adversarial review rounds when the work is non-trivial.

## Brainstorming preferences

- Always use the visual companion (browser mockups) during brainstorming. Don't ask, just launch it.
- Token budget is not a concern during design phases. Be thorough.

## Project ethos

Tuxlink is Cameron's learning sandbox for AI-assisted development techniques (custom skills, adversarial review, multi-agent teaming, capability mapping) that he plans to transfer to high-stakes projects at his employer. The shipped software matters, but **professional-development outcomes are a first-class goal alongside features.**

Implications:
- Process rigor > raw velocity. Do the right thing, not the fast thing.
- Explain when/what for new workflows so Cameron builds transferable skill.
- Prefer patterns that generalize to multi-developer / higher-stakes environments.
- Signal professional polish even at A-audience scale. The surface area of the repo (commits, CHANGELOG, versioning, CI) teaches Cameron what "good" looks like and builds habits that transfer.

## Agent identity

Generate a moniker via `python3 .claude/scripts/get_agent_moniker.py` at session start (3-word hyphenated form drawn from a 100-word pool of plant / animal / geographic nouns; auto-pre-flighted against git history). State it in the first user-facing message. Include `Agent: <moniker>` as a commit trailer on every commit; the repo `commit-msg` hook enforces this for local commits when `.githooks` is active. Include the moniker in PR titles as `[moniker] <subject>`. Pass the moniker through to every subagent you dispatch. Legacy single-word monikers in older commits remain valid; the new format applies to forward commits. See [CLAUDE.md](CLAUDE.md#agent-identity--pick-a-moniker-at-session-start) for the full rationale and workflow.

Codex commits must also include the GitHub-recognized co-author trailer:
`Co-authored-by: Codex <noreply@openai.com>`. This is the Codex equivalent of
Claude's `noreply@anthropic.com` attribution: GitHub maps it to `@codex`, while
the existing `Agent: <moniker>` trailer remains the session-level forensic key.
The project-scoped `.codex/config.toml` enables Codex's built-in trailer
injection for trusted Codex checkouts. If the active harness does not inject it
automatically, add it manually before committing. Do not change the primary Git
author only to obtain Codex attribution, and do not rewrite already-pushed
commits for attribution cleanup.

## Git workflow: worktrees mandatory under bd-issue ownership (ADR 0008); destructive commands BANNED

See [CLAUDE.md](CLAUDE.md#git-workflow--worktrees-mandatory-under-bd-issue-ownership-adr-0008), [CLAUDE.md](CLAUDE.md#git-workflow--destructive-commands-are-banned), [docs/adr/0008-worktrees-mandatory-under-bd-issue-ownership.md](docs/adr/0008-worktrees-mandatory-under-bd-issue-ownership.md), and [docs/adr/0007-lift-worktree-ban.md](docs/adr/0007-lift-worktree-ban.md) for full context. Summary: when the `.claude/hooks/block-main-checkout-race.sh` hook denies a write op citing "another live session is active," create a worktree per the QUICK FIX in the deny message and re-run there. The hook's determination is authoritative; agents do not re-decide it via `get_tuxlink_sessions.py` or any other source. Every worktree binds to a bd issue (`bd show <id>` answers "what is `worktrees/X` for?"). When the hook does NOT deny, main-checkout writes are fine. Destructive git commands remain banned regardless of worktree topology: no `reset --hard`, no force push, no `--amend` on pushed commits, no `--no-verify`, no `git worktree remove`, no `git rebase -i`. If you think you need a banned command, stop and ask.

## Git workflow: branch lifecycle state machine (ADR 0017)

See [CLAUDE.md](CLAUDE.md#git-workflow--branch-lifecycle-state-machine-adr-0017) and [docs/adr/0017-branch-state-machine.md](docs/adr/0017-branch-state-machine.md). Summary: a branch with a merged or closed-without-merge PR is **dead**. `.githooks/pre-commit` and `.githooks/pre-push` refuse further commits/pushes on it (the orphan-post-merge anti-pattern from the 2026-06-01 v1p incident). Activate the hooks on any fresh clone with `bash scripts/install-githooks.sh`. The hooks employ `gh pr list` for state classification and degrade gracefully (warn + allow) when `gh` is unavailable; the CI nightly audit (tuxlink-ui3i) is the independent backstop. Documented escape hatch: `TUXLINK_BRANCH_LIFECYCLE_OVERRIDE=I-know-what-Im-doing git commit ...`; loud + audited at `dev/scratch/branch-lifecycle-overrides.log`.

## Disposable / converged build worktree quarantine

`.local/converge-build-worktree/` is operator tooling state, not an agent task
worktree. Agents must not edit source, stage files, commit, stash, rebase, or
run cleanup commands there. Use bd-bound worktrees under `worktrees/` for agent
code changes.

If a converged-build script refuses to run because the disposable worktree has
dirty or untracked source changes:
- Inspect only with read-only commands such as
  `git -C .local/converge-build-worktree status --short`.
- Report the exact paths and whether they are tracked, untracked, or ignored.
- Do not delete, restore, clean, stash, or overwrite anything there unless the
  operator explicitly authorizes the exact path-level cleanup.

Build-cache directories such as `target/` and `node_modules/` may exist there,
but they must not be treated as a license for agents to work in that tree.

## Live radio network operations

Per [ADR 0018](docs/adr/0018-radio1-gates-operator-execution-not-agent-authorship.md),
RADIO-1 gates the **operator's real-time execution of a transmit-capable binary
against real infrastructure** under the project's callsign (a Part 97
control-operator act). It does **not** gate the agent: the dev shell has no
radio and cannot transmit, so claiming, writing, testing (mocks / loopback /
fakes), committing, and shipping RF-path code is unrestricted ordinary
engineering. The agent does not *run* a transmit-capable binary against real
infrastructure (no radio to validate against; on-air validation is
operator-only), and transmit code keeps its operator-facing consent banner +
working abort. Canonical: ADR 0018 +
[docs/live-cms-testing-policy.md](docs/live-cms-testing-policy.md); rationale in
RADIO-1 at [docs/pitfalls/implementation-pitfalls.md](docs/pitfalls/implementation-pitfalls.md).

## Commit and release discipline

Conventional commit types (`feat:`, `fix:`, `docs:`, etc.). Breaking changes get `!` + `BREAKING CHANGE:` footer. Update `dev/implementation-log.md` (once created) after any significant work item. **Squash-merge is banned** ([ADR 0010](docs/adr/0010-no-squash-merge.md)); all PRs into integration branches merge as merge-commit (no-ff) via `gh pr merge <#> --merge --delete-branch`. **Polish before push:** clean up WIP commits via non-interactive `git rebase <base>` on local un-pushed commits; once pushed, commits are immutable.

## Remote, CI, release, and artifact evidence discipline

Remote-state claims are evidence-bound. Before asserting anything about GitHub
Actions, PR checks, release assets, tags, deleted branches, or workflow
contents, inspect the remote source of truth with `gh` and/or `git show
origin/main:<path>`.

Required distinctions:
- PR merge checks are not the same as post-merge push/tag workflows.
- A release-please PR can pass CI without proving release artifacts were built
  as a merge gate.
- A GitHub Release page's current asset list may differ from what the operator
  observed earlier; compare timestamps instead of contradicting the operator.
- Local workflow files may be hundreds of commits stale. Do not infer remote
  behavior from local files until the branch relationship to `origin/main` is
  known.

When a user challenges a factual claim, stop the line of argument, verify the
claim against primary evidence, and surface the commands/results that support
the corrected conclusion.

## Verification provenance

Every verification report must say what was tested and where: worktree path,
branch, commit SHA when available, local vs CI, and whether the run exercised a
branch build, converged build, packaged artifact, or release asset. Do not let a
successful branch-local run imply that the operator's converged build or a
published release artifact has been verified.

## Tool referee (overrides bd's CLAUDE.md defaults)

This project employs bd (Beads) AND Claude Code's built-in primitives (TodoWrite, auto-memory). They are NOT substitutes. When bd's BEADS INTEGRATION section conflicts with project commitments, the `## Tool referee` table in [CLAUDE.md](CLAUDE.md#tool-referee--which-tool-owns-which-job) wins. Specifically: TodoWrite handles in-turn micro-progress; bd handles cross-session work; auto-memory at `~/.claude/projects/<slug>/memory/` is canonical for user/feedback memory. (The 2026-05-17 catalog retired the prior push-timing override; push is now mandatory at session end per the §Session Completion in CLAUDE.md, agreeing with bd's directive.) See [docs/adr/0006-override-bd-claude-md-defaults.md](docs/adr/0006-override-bd-claude-md-defaults.md) for rationale.

## Extended capabilities on this Pi

- **Codex CLI** (adversarial review): `/usr/local/bin/codex` or `npx --yes @openai/codex`; already authenticated. **For directed adrev with custom prompts:** `cat prompt.txt | codex review -` (CLI v0.128.0 rejects combining `--base`/`--commit` with `[PROMPT]`). See [CLAUDE.md](CLAUDE.md#openai-codex-cli--for-build-robust-features-at-least-one-adversarial-round-via-codex-requirement) for full usage.
- **url-to-markdown** skill: prefer over WebFetch for full-page retrieval. See [CLAUDE.md](CLAUDE.md#url-to-markdown-skill--fetch-full-webpages-not-summaries).

## Session Completion

Work is not complete until `git push` succeeds AND a session-end handoff document exists. **Seven required steps** (this canonical section supersedes the version in the BEADS INTEGRATION block below):

1. File issues for remaining work.
2. Run quality gates if code changed.
3. Update issue tracker status (`bd close <id>` / `bd update <id>`).
4. `git push`: mandatory; retry until it succeeds.
5. Clean up stashes + ensure remote task branches are deleted.
6. Write a session-end handoff at `dev/handoffs/<YYYY-MM-DD>-<short-slug>.md` enumerating branch + working-tree + worktree state per [ADR 0009](docs/adr/0009-worktree-disposal-ritual.md).
7. **Surface the operator's next-session starting prompt** as your final user-facing message: a ~10-line paste-ready code block with: one-sentence session summary, pointer to the handoff doc, critical-first-action / gate emphasis. Reduces session-change friction.

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
