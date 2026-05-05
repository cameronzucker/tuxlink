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

**At the very start of every session** (after reading CLAUDE.md and the most-recent handoff, before taking any action on the repo), pick a short moniker for yourself and state it in your first user-facing message. The moniker:

- Must be a single word, lowercase, no spaces, no punctuation.
- Must be **ctrl+F-friendly** — avoid words that already appear in the codebase/docs (run `grep -rci <name> .` mentally; if there are many hits, pick something else). Plant/animal/geographic nouns work well (`juniper`, `hemlock`, `sparrow`, `flint`).
- Avoid human first names to prevent confusion with Cameron, beta testers, or co-authors.
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

## Git workflow — worktrees are BANNED

Do NOT use `git worktree` in this project. All branch work happens via `git checkout` in the main repo at `/home/administrator/Code/tuxlink`.

**Rationale:** Carried forward from the Geographica project, where two near-misses in 2026-04 involved subagents `cd`'ing out of a worktree and performing destructive operations on the main repo's branch (one `git reset --hard` wiped 6+ commits from `dev`'s tip pointer; recovered via reflog). Worktree topology multiplies the blast radius of "subagent forgets which checkout it's in" errors.

**If you encounter an existing worktree** (e.g., `.claude/worktrees/<name>/`): do NOT use it. Check out the same branch in the main repo instead, and suggest that the user remove the worktree with `git worktree remove`.

**If a session handoff tells you to "work in the worktree at X"**: override that instruction. Check out the branch in the main repo, and flag the deviation to the user.

## Git workflow — destructive commands are BANNED

Do NOT run destructive git commands. There is never a legitimate reason for an agent to run these unprompted. If you think you need one, **stop and ask the user**.

**Banned commands (no exceptions without explicit user authorization for this specific call):**
- `git reset --hard <ref>` — destroys uncommitted work AND rewinds the branch tip. Use `git revert <commit>` for an additive undo, or ask the user which specific file to restore with `git checkout -- <path>`.
- `git push --force` / `git push -f` / `git push --force-with-lease` — rewrites remote history. If you need to replace a pushed commit, open a new PR or ask.
- `git checkout -- .` / `git restore .` / `git clean -f` / `git clean -fd` — wipes entire working-tree state. If you want to discard one file, name it explicitly after checking with the user.
- `git branch -D <branch>` / `git branch --delete --force` — force-deletes a branch even if unmerged. Use `git branch -d`, which refuses to delete unmerged branches.
- `git rebase -i` with squash/fixup/drop on shared commits — rewrites history. (`--no-edit` is not a valid `git rebase` flag and should never be passed.)
- `git commit --amend` on any commit that has been pushed OR that was authored by someone else. Always create a **new** commit to correct earlier work.
- `git reflog expire --expire=now` / `git gc --prune=now` — strips the safety net that would let us recover from the commands above.
- `git filter-branch` / `git filter-repo` — mass history rewrite.
- `--no-verify` (skips hooks) / `--no-gpg-sign` / `-c commit.gpgsign=false` — bypasses the project's commit gates. The hooks exist for a reason; if one fails, fix the root cause instead of skipping.

**Rationale:** On 2026-04-20, a subagent in the sister Geographica project ran `git reset --hard feat/noaa-conus` on the main checkout's `dev` branch, wiping 7 commits — including a runtime-validated bug fix that had been shipped to the live stack. Recovery took one `git merge` with manual conflict resolution, but only because all commits were still reachable via reflog; two weeks later and `git gc` would have pruned them permanently. Agents have no legitimate workflow that requires destructive operations; the pattern is always "something went wrong, let me start over" — which is a cue to **ask the user**, not reset.

**If you think you need one of these:** the correct action is to surface the situation to the user with a proposed non-destructive alternative.

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

## Parity with `AGENTS.md`

[AGENTS.md](AGENTS.md) is a deliberate **summary with links** to this file's sections, intended for non-Claude agent harnesses (Codex, etc.) where pulling the whole CLAUDE.md inline would be wasteful. It is NOT a full mirror; the substantive rules live here and AGENTS.md points to them. When changing rules in CLAUDE.md, check whether AGENTS.md's summary line for that section needs a corresponding update.


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
