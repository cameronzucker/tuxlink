# Tuxlink

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
The skill has specialized workflows that produce better results than ad-hoc answers.

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

## Extended capabilities available on this dev Pi

### OpenAI Codex CLI — for `build-robust-features`' "at least one adversarial round via Codex" requirement

**Codex IS installed on this Pi. It is NOT on `$PATH`.** `which codex` returns nothing, which is why assistants keep missing it. Invoke via `npx`:

```bash
# Non-interactive agent call
npx --yes @openai/codex exec "<prompt>"        # alias: codex e

# Purpose-built code review (what adversarial rounds typically want)
npx --yes @openai/codex review --commit <SHA> "<attack-angle prompt>"
npx --yes @openai/codex review --uncommitted "<prompt>"      # staged + unstaged + untracked
npx --yes @openai/codex review --base main    "<prompt>"     # current branch vs base

# Optional: stdin-piped prompt
cat spec.md | npx --yes @openai/codex exec -
```

- **Authentication:** ChatGPT-mode, cached at `~/.codex/auth.json`. Already authenticated — no setup needed.
- **When to use:** when a workflow (notably `superpowers:build-robust-features`) explicitly calls for "at least one round via Codex." Substitute Claude agents only when this is genuinely unavailable — it isn't unavailable here.
- **MCP-server mode:** `npx --yes @openai/codex mcp-server` — expose Codex as an MCP server if you want the main loop to call it like a tool.

Write adversarial-review output to `dev/adversarial/<date>-<topic>-codex.md` to match the existing naming pattern once `dev/` is created.

### `url-to-markdown` skill — fetch FULL webpages, not summaries

Installed at `/home/administrator/.claude/skills/url-to-markdown/`. Invoke via the `Skill` tool (name: `url-to-markdown`) or directly:

```bash
python3 /home/administrator/.claude/skills/url-to-markdown/scripts/bootstrap.py "https://url" --json --out /tmp
```

**Prefer this over `WebFetch` whenever you need the full content of a page** (product pages, docs, wikis, articles). `WebFetch` runs the page through a summarizer that can drop critical details. `url-to-markdown` downloads the raw content, converts to markdown with YAML frontmatter, and writes to disk so you can read it verbatim.

Returns a JSON envelope; parse the `output_path` and then `Read` the resulting `.md` file. Handles Cloudflare-class bot protection via TLS fingerprint impersonation. Gracefully reports paywalls, SPAs, PDFs, and feeds instead of producing garbage.

## Project ethos

Tuxlink is Cameron's learning sandbox for AI-assisted development techniques —
custom skills, adversarial review, multi-agent teaming, capability mapping —
that he plans to transfer to high-stakes projects at his employer. The
shipped software matters, but **professional-development outcomes are a
first-class goal alongside features.**

Implications:
- Process rigor > raw velocity. Do the right thing, not the fast thing.
- Explain when/what for new workflows so Cameron builds transferable
  skill.
- Prefer patterns that generalize to multi-developer / higher-stakes
  environments.
- Signal professional polish even at A-audience scale — the surface area
  of the repo (commits, CHANGELOG, versioning, CI) teaches Cameron what
  "good" looks like and builds habits that transfer.

## Agent identity — pick a moniker at session start

**At the very start of every session** (after reading CLAUDE.md and the most-recent handoff, before taking any action on the repo), generate a moniker via the script and state it in your first user-facing message:

```bash
python3 .claude/scripts/get_agent_moniker.py
```

This draws 3 words without replacement from a 100-word pool of plant / animal / geographic nouns and hyphen-joins them (e.g. `towhee-wren-aspen`). Combinatorial space ≈ 970,200 trios; collision probability under 1% across project lifetime. The script pre-flights against `git log --all --grep="^Agent: <candidate>"` automatically; if a collision is detected, it retries up to `--max-attempts` times before giving up. This replaces the prior manual `grep -ri <name> .` + `git log --all --grep` dance.

The moniker:

- Is hyphen-joined three-word form (single-word legacy monikers in commit history — `alder`, `cedar`, etc. — remain valid; the new format applies to forward commits).
- Is **ctrl+F-friendly** by construction (the pool excludes common code-identifiers and human first names).
- Persists for the entire session — do not change it mid-session.
- Passes through to every subagent you dispatch: include `"You are agent <moniker>; use this in your commit trailers."` in each Agent tool prompt so subagent-authored commits are grep-discoverable too.

**Include the moniker in every git action as a commit trailer:** `Agent: <moniker>` on its own line in the commit message, alongside the existing `Co-Authored-By:` trailer.

```
<subject>

<body paragraphs>

Agent: juniper
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

**Also include in:** branch names when creating them (`agent-<moniker>/<topic>` for throwaway branches; regular `feat/` / `fix/` prefixes are fine for shared feature branches but still add the trailer inside commits), and PR titles if you open one (`[juniper] <subject>`).

**Why:** triage + forensics. When a session goes sideways — a mysterious `git reset --hard`, a stale regression, an unclear commit authorship — Cameron needs to grep the commit graph for "which agent did this" without reconstructing it from timestamps. `git log --grep="^Agent: juniper"` returns the full trail for this session. `git log --all --grep="^Agent:"` enumerates every agent that has ever touched the repo.

**If you forget to set a moniker early in the session:** pick one now and apply it to all forward commits. Do not retroactively amend earlier commits (amending shared/recent commits is banned — see below).

## Git workflow — worktrees mandatory under bd-issue ownership (ADR 0008)

When two or more Claude Code sessions are simultaneously live against this repository, any session not holding the main-checkout lease MUST perform its write work in a worktree, not in the main checkout. For solo-session work (the typical case today), worktrees remain optional — use `git checkout` in the main repo when no isolation benefit is gained. The lease-detection mechanism is automatic per the `.claude/hooks/block-main-checkout-race.sh` hook (D1); agents do not need to check for concurrency manually.

**Worktree ownership rule.** A worktree is permitted IFF:

1. A **bd issue** is in `in_progress` and claims the worktree (path recorded in the issue body or via `bd remember`). `bd show <id>` is the canonical answer to "what is `worktrees/X` for?"
2. The branch follows the per-task convention ([ADR 0004](docs/adr/0004-per-task-branch-model.md)): `bd-<id>/<slug>` preferred when the bd issue exists; otherwise `agent-<moniker>/<slug>` or `task-NN-<slug>`.
3. The worktree path is `worktrees/<bd-id-or-slug>/` at the repo root (`worktrees/` is `.gitignore`d).
4. The session adheres to all other CLAUDE.md rules (moniker discipline, commit discipline, destructive-git ban, session-end handoff).

A worktree without a bd-issue claim is an anti-pattern. If you encounter one (stale handoff, prior orphan), either (a) retroactively claim it with a bd issue, or (b) inventory + archive + dispose per the disposal ritual (ADR 0009, forthcoming as part of this sprint's D3).

**Pattern A (harness-spawned ephemeral worktrees** — the `Agent` tool's `isolation: "worktree"` parameter) is uncontroversially permitted; the harness manages create + dispose, no per-worktree bd issue required.

**Multi-worktree coordination via bd dep edges.** When two or more worktrees are simultaneously `in_progress`, maintain the dependency graph via `bd dep add <consumer-id> <provider-id>`. `bd ready` reflects unblocked work at any moment.

**Full rationale, alternatives considered, and watched failure modes:** [ADR 0008](docs/adr/0008-worktrees-mandatory-under-bd-issue-ownership.md), which supersedes [ADR 0007](docs/adr/0007-lift-worktree-ban.md)'s "permitted but optional" framing. ADR 0007 remains accepted as the historical record of why the original Geographica-era ban was lifted.

### Worktree disposal ritual ([ADR 0009](docs/adr/0009-worktree-disposal-ritual.md))

`git worktree remove` is banned (destructive-git hook denies it per C1). Disposal uses the 4-step ritual:

```bash
# Step 1 — Inventory (from inside the worktree being disposed)
git status --short                                          # tracked dirty
git ls-files --others --exclude-standard                    # untracked
git ls-files --others --ignored --exclude-standard          # gitignored on disk
git stash list                                              # worktree-scoped stashes

# Step 2 — Propagate (commit + push) or archive
#   For propagate: git add ..., git commit -m "...", git push origin <branch>
#   For archive:   tar czf .claude/worktree-archives/<name>-$(date -u +%Y%m%dT%H%M%SZ).tar.gz <worktree-path>

# Step 3 — Physical remove
rm -rf <worktree-path>

# Step 4 — Prune git's registry
git worktree prune
```

`.claude/worktree-archives/` is `.gitignore`d. The archive directory is per-machine, not pushed to origin. The hook denies `git worktree remove` regardless of how the worktree looks "clean" — `.beads/embeddeddolt/` is the canonical example of gitignored-but-stateful content the git check misses.

**Why no shortcut:** the LFST musing-bhabha incident (May 2026) lost untracked content via `git worktree remove`. The ritual is the replacement; see [ADR 0009](docs/adr/0009-worktree-disposal-ritual.md) for full context and watched failure modes.

## Git workflow — destructive commands are BANNED

The [`.claude/hooks/block-destructive-git.sh`](.claude/hooks/block-destructive-git.sh) hook denies destructive git operations at the harness layer. **The hook is the canonical enforcement; do not work around it.** If a hook denial surprises you, the right move is to find a non-destructive alternative — never `--no-verify`, never an end-run.

**Full banned list and rationale:** see the hook source for the regex-precise list, and [standing-conventions §1](https://github.com/cameronzucker/cz-agent-skills/blob/main/docs/standing-conventions-cross-project.md) for the cross-project rule. Quick reference (not the authoritative list — the hook is):

- `git reset --hard <ref>` — use `git revert <commit>` or restore named files.
- `git push --force` / `-f` / `--force-with-lease` — open a new PR or ask.
- `git checkout -- .` / `git restore .` / `git clean -f` — name files explicitly.
- `git branch -D` / `--delete --force` — use `-d`, which refuses unmerged.
- `git commit --amend` on pushed or other-authored commits — create a new commit.
- `git rebase -i` / `--interactive` — banned outright per C1; use `git rebase <base>` for non-interactive linear replays.
- `git worktree remove` — use the disposal ritual ([ADR 0009](docs/adr/0009-worktree-disposal-ritual.md)).
- `git reflog expire --expire=now` / `git gc --prune=now` — strips the recovery safety net.
- `git filter-branch` / `git filter-repo` — mass history rewrite.
- `--no-verify` / `--no-gpg-sign` / `-c commit.gpgsign=false` — bypasses the project's gates.

**Why hooks, not just prose:** the 2026-04-20 Geographica incident — a subagent ran `git reset --hard feat/noaa-conus` on `dev`, wiping 7 commits including a shipped fix; recovered via reflog only because the regression was caught within the 14-day `git gc` window. Geographica's CLAUDE.md *correctly documented* the rule at the time of the incident. **Prose alone did not prevent it; the hook layer does.**

**If you think you need a banned command:** stop and surface the situation to the user with a proposed non-destructive alternative.

## Live radio network operations — READ BEFORE ANY TRANSMISSION

No automation, test, subagent, CI job, scheduled task, or AI agent
initiates a transmission under the project's amateur callsign without
the station licensee giving explicit, scoped, per-invocation consent at
the moment of the run. Cached credentials, stored env vars, repo
secrets, and "the user said yes to this last week" are NOT consent.

This is a Part 97 regulatory requirement, not a style guideline. Full
rules, rationale, and the required consent-gate protocol live at
[docs/live-cms-testing-policy.md](docs/live-cms-testing-policy.md) and
the RADIO-1 entry in
[docs/pitfalls/implementation-pitfalls.md](docs/pitfalls/implementation-pitfalls.md).

**Subagent rule:** if your task touches any code path that could
transmit, refuse to run it in your shell. Write the code, commit it,
let the licensee run it manually. If your task seems to require you to
run a live-CMS binary to verify completion, your task is misspecified
— STOP and escalate.

## Commit and release discipline

- Use conventional commit types: `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`, `perf:`, `ci:`, `build:`. Match the commit `type:` to the actual intent. Never use `fix:` for docs fixes or `feat:` for internal refactors.
- Prefer scoped commits (`feat(<scope>): ...`) when the change is localized to one subsystem. Scopes will be defined after office-hours sets the project structure.
- Breaking changes: add `!` suffix and a `BREAKING CHANGE:` footer with a one-line user-facing explanation.
- Update `dev/implementation-log.md` (once created) after any significant work item: plan executed, feature shipped, bug hunt cycle completed, adversarial review completed. Entry goes at the top, reverse-chronological, keyed by date + topic.
- **Polish before push.** Per [ADR 0010](docs/adr/0010-no-squash-merge.md): squash-merge is banned, so the integration branch will preserve every task-branch commit. Clean up WIP / fixup / "oops" commits via non-interactive `git rebase <base>` on **local un-pushed commits** before `git push`. Once pushed, commits are immutable (the destructive-git ban on `--amend` of pushed commits and on `git rebase -i` ensures this). The push gates the polish.

## Documentation propagation contract

For any project-policy claim — an ADR, a spec section, an operator decision — the **canonical source is the ADR or spec itself**. CLAUDE.md, AGENTS.md, plan templates, pitfalls docs, and memory entries are **pointers**, not parallel statements.

**Maximum three propagation sites per ADR:**

1. The ADR itself (always).
2. The spec section it amends, if any.
3. One operational doc — CLAUDE.md OR plan template OR pitfalls — pick one.

Memory entries cite the ADR; they do not restate it. Narrowly-scoped operational recipes that are inherently a how-to (e.g., the exact JSON shape for `.claude/session-leases/main-checkout.json` once D1 lands, or the worktree-disposal ritual step-by-step) MAY live in CLAUDE.md or pitfalls docs where the operator will look for them. The rule is "don't restate what the spec/ADR already says," not "don't write recipes."

**Why:** Without this contract, ADRs and CLAUDE.md drift apart. The same rule appears in three places with slightly different wording; one place is updated, the others rot. The propagation contract makes the ADR/spec the single canonical source.

**Cross-project authority:** [`standing-conventions-cross-project.md` §9](https://github.com/cameronzucker/cz-agent-skills/blob/main/docs/standing-conventions-cross-project.md) carries the portable version of this rule. The two should stay aligned; if they diverge, the standing-conventions doc wins and this section gets a corrective commit.

## Parity with `AGENTS.md`

[AGENTS.md](AGENTS.md) is a deliberate **summary with links** to this file's sections, intended for non-Claude agent harnesses (Codex CLI, `codex review`, and future tooling that picks up the standard `AGENTS.md` convention) where pulling the whole CLAUDE.md inline would be wasteful. It is NOT a full mirror; the substantive rules live here and AGENTS.md points to them.

**Upkeep discipline.** Every PR that changes a rule in CLAUDE.md MUST also do the AGENTS.md parity check, in the same PR. The check:

1. Locate the AGENTS.md section that summarizes the CLAUDE.md section you changed.
2. If the change is purely-additive content (clarification, expanded example, new link) AND the AGENTS.md summary line is still accurate, no AGENTS.md update is needed.
3. If the change adds, removes, or renames a CLAUDE.md section, OR alters the load-bearing summary AGENTS.md was providing, update AGENTS.md in the same PR.
4. If a CLAUDE.md change introduces a load-bearing rule for non-Claude agents and no AGENTS.md section currently summarizes it, add one.

Drift between CLAUDE.md and AGENTS.md is a defect. It violates the project's propagation contract (see [§"Documentation propagation contract"](#documentation-propagation-contract) above: CLAUDE.md is the source of truth for substantive rules; AGENTS.md is a pointer).

**When in doubt, ship the AGENTS.md update alongside the CLAUDE.md change.** A redundant tweak is cheaper than a drift bug; the parity check is meant to be light, not skipped.

**Cross-project authority:** [`standing-conventions-cross-project.md` §10](https://github.com/cameronzucker/cz-agent-skills/blob/main/docs/standing-conventions-cross-project.md).

## Tool referee — which tool owns which job

This project uses both Claude Code's built-in primitives (TodoWrite, auto-memory) and `bd` (Beads). They serve overlapping but **non-redundant** roles. When `bd`'s auto-managed section below (`<!-- BEGIN BEADS INTEGRATION -->`) prescribes a rule that conflicts with the table here, **the table wins.** See [docs/adr/0006-override-bd-claude-md-defaults.md](docs/adr/0006-override-bd-claude-md-defaults.md) for full rationale and watched failure modes.

| Concern | Owns it | Notes |
|---|---|---|
| Cross-session task tracking with deps | `bd` | Primary. Use `bd ready` / `bd update --claim` / `bd close`. |
| In-turn micro-progress within one session | TodoWrite | Claude Code primitive; ephemeral; correct for "read X, edit Y, run Z" lists. |
| User profile + cross-cutting feedback | Auto-memory at `~/.claude/projects/<slug>/memory/` | Harness-native, auto-loaded each session via `MEMORY.md` index. Already seeded; do not migrate to bd. |
| Issue-adjacent factoids discovered during a task | `bd remember` | Use for knowledge linked to a specific issue. Cross-project user/feedback stays in auto-memory. |
| Branch model | Per-task branch + merge-commit (no-ff) | See [ADR 0004](docs/adr/0004-per-task-branch-model.md) (per-task model) + [ADR 0010](docs/adr/0010-no-squash-merge.md) (no-squash) + [ADR 0008](docs/adr/0008-worktrees-mandatory-under-bd-issue-ownership.md) (worktree-issue ownership). |

**Specific overrides of bd's BEADS INTEGRATION block** (rules below the BEADS INTEGRATION marker that this section explicitly supersedes):

- bd says *"do NOT use TodoWrite, TaskCreate, or markdown TODO lists"* → **Override:** TodoWrite is the right primitive for in-turn working memory; bd is the right primitive for cross-session work units. Use both, for their respective layers.
- bd says *"Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files"* → **Override:** the Claude Code auto-memory directory at `~/.claude/projects/<slug>/memory/` is harness-native and remains canonical for user / feedback / project memory. Use `bd remember` for issue-tracker-adjacent factoids only.
- bd says *"Work is NOT complete until `git push` succeeds … YOU must push"* → **No longer overridden** as of 2026-05-17. Per [§Session Completion](#session-completion) and standing-conventions §7, push is now mandatory at session end. bd's directive on this point now agrees with project policy.

**If you discover a fourth bd directive that conflicts with project commitments:** extend the table above AND ADR 0006's override list. Do NOT silently soften an override.

## Session Completion

Work is not complete until `git push` succeeds AND a session-end handoff document exists. This rule is **unconditional** per [`standing-conventions-cross-project.md` §7](https://github.com/cameronzucker/cz-agent-skills/blob/main/docs/standing-conventions-cross-project.md) and Decision 1 of the 2026-05-17 LFST→tuxlink port catalog. (The Tool referee table's "Push timing | Operator" row above is out of date and will be reconciled in this sprint's cleanup pass; this section is authoritative.)

**Required steps before ending any session:**

1. File issues for remaining work discovered during the session (`bd create ...`).
2. Run quality gates if code changed (tests, linters, builds).
3. Update issue tracker status (`bd close <id>` / `bd update <id>`).
4. **`git push`** — mandatory. If push fails, resolve the failure and retry until it succeeds. Do NOT stop before pushing.
5. Clean up: clear stashes, ensure remote task branches are deleted (`gh pr merge --delete-branch` handles this automatically for landed PRs; manual `git push origin --delete <branch>` for branches that didn't reach merge).
6. Write a session-end handoff document to `dev/handoffs/<YYYY-MM-DD>-<short-slug>.md` enumerating: branch state, working-tree state, in-flight worktrees + their untracked + gitignored-stateful content (per [ADR 0009](docs/adr/0009-worktree-disposal-ritual.md) §"Handoff documents enumerate worktree state"), what was completed, what is in-progress, what is pending decision.

**Never say "ready to push when you are."** Push is the session's responsibility, not the operator's. The handoff document closes the context loop so the next session — possibly on a different machine — can continue without manual reconstruction from `git log`.

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
