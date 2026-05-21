# Handoff — YYYY-MM-DD <short-slug>

> **Template usage.** Copy this file as `dev/handoffs/YYYY-MM-DD-<short-slug>.md` at session end. The session-end handoff is required per CLAUDE.md §"Session Completion" / standing-conventions §7; the structure here is the minimum the next session needs to pick up cold. Fill or remove each section honestly — empty sections are fine; missing sections are not.

**From agent:** `<moniker>` (from `python3 .claude/scripts/get_agent_moniker.py` at session start)
**Session arc:** `<one-sentence summary of the work shape>`
**Status:** `<pushed | committed-not-pushed (justify why) | mid-task (what's open) | review-pending>`

---

## Next session's starting prompt

> Paste this verbatim into a fresh Claude Code session. Lead with the reads-before-action sequence so the next agent grounds itself in the same context. Then state the first action.

> I'm resuming the tuxlink project. `<previous moniker>` handed off `<YYYY-MM-DD>`. Read these before doing anything:
>
> 1. `dev/handoffs/<YYYY-MM-DD>-<slug>.md` — this handoff.
> 2. `CLAUDE.md` — full project rules. Pay attention to `## Tool referee`, `## Documentation propagation contract`, `## Session Completion`, and the worktree sections (ADR 0008 + ADR 0009).
> 3. `docs/adr/` — the ADR set. `<call out the load-bearing ADRs for this work>`.
> 4. `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` — the v0.0.1 plan.
> 5. `docs/pitfalls/implementation-pitfalls.md` and `docs/pitfalls/testing-pitfalls.md`.
>
> Once read:
>
> - Generate a fresh moniker via `python3 .claude/scripts/get_agent_moniker.py`. Auto-pre-flighted against git history.
> - Run `bd ready` to see available work; check `bd show <id>` for any in-progress issues you may be inheriting.
> - `<first concrete action — what should the next session DO first?>`.
> - Take time on the work; quality over speed.

---

## What landed in this session

`<For each substantive change, summarize: what was changed, why, what it supersedes if anything, and the PR # if merged. Keep it scannable.>`

| Item | What | PR # | Status |
|---|---|---|---|
| `<#>` | `<summary>` | `<PR>` | `<merged | open | draft | drafted-not-pushed>` |

---

## State at pause

### What's pushed to origin

```
<branch>  <SHA>  <one-line description if interesting>
```

`<note any branch protection, divergence between main and integration branches, etc.>`

### Working-tree state

`<clean | dirty (enumerate)>`

Run from the main checkout:
- `git status --short` — `<paste output or summarize>`
- `git ls-files --others --exclude-standard` — `<untracked, if any>`
- `git stash list` — `<stashes, if any>`

### In-flight worktrees (per ADR 0009 disposal-ritual requirement)

> Per ADR 0009, the handoff MUST enumerate untracked + gitignored-stateful content for every living worktree. Run `git worktree list` to find them; for each, run from inside the worktree:
> - `git status --short`
> - `git ls-files --others --exclude-standard` (untracked)
> - `git ls-files --others --ignored --exclude-standard` (gitignored on disk — filter against `dev/state-paths.md` for what's restorable from elsewhere)
> - `git stash list` (worktree-scoped stashes)

`<for each worktree:>`

#### Worktree `<path>` (claimed by bd `<issue-id>`, branch `<branch>`)

- **Tracked dirty:** `<list>`
- **Untracked:** `<list>`
- **Gitignored-stateful** (not in `dev/state-paths.md` as auto-restorable): `<list>`
- **Stashes:** `<list>`
- **Disposition for at-risk content:** `<will-commit | will-archive | will-discard | pending-decision>`

`<If no worktrees in flight: "No worktrees in flight.">`

### bd state

```
<bd stats output: total / open / in_progress / blocked / closed / ready>
```

In-progress issues claimed by this session (`bd list --status=in_progress`):

| Issue ID | Title | Last update | Disposition |
|---|---|---|---|
| `<id>` | `<title>` | `<date>` | `<close-on-next-session | continue | unclaim>` |

`<note any newly-unblocked work from this session's closures>`

---

## Open decisions for the next agent or Cameron

`<List decisions the next session needs to make before proceeding. Include enough context for each that the next session can decide without re-deriving.>`

1. **`<decision title>`** — `<context>`. Options: `<A | B | C>`. Recommendation: `<one of the options, with reason>`.

`<If none: "No open decisions; the next-session starting prompt is unambiguous.">`

---

## Plan amendments queued

`<List any plan files that need amendment because of discoveries this session. Format per amendment:>`

- **`<file>` `<section>`** — `<what to change and why>`. Recommended action: `<small docs commit on a `task-amend-<slug>` branch, squash-merged to feat/v0.0.1>`.

`<If none: "No plan amendments queued.">`

---

## Reminders for the next agent

- bd directives in `<!-- BEGIN BEADS INTEGRATION -->` are overridden by `## Tool referee` in CLAUDE.md (per ADR 0006). Use TodoWrite for in-turn working memory; auto-memory at `~/.claude/projects/...` for cross-session knowledge.
- `set -o pipefail` for any pipeline ending in `tail` / `head` that you care about the exit code of.
- The substring-matching destructive-git hook also catches banned patterns in commit-message text. Workaround: `git commit -F /tmp/msg.txt` (write the message to a file, then commit by file).
- Per-task-branch wrap: branch off `feat/v0.0.1` → commit → push → PR (`gh pr create --base feat/v0.0.1`) → `gh pr merge --merge --delete-branch` (NOT `--squash`) → `git pull --ff-only origin feat/v0.0.1` → `git branch -d` → `bd close` if a bd issue was claimed.
- `<add session-specific reminders here>`

---

**If something in this handoff looks wrong tomorrow:** the previous-session agent (`<moniker>`) wasn't perfect; flag it before acting on it. Source of truth for any rule that this handoff restates: the ADRs and CLAUDE.md (per the propagation-contract rule in CLAUDE.md §"Documentation propagation contract").

---

## ⚠️ AGENT REMINDER — Don't end the session yet

After committing this handoff document, **your final user-facing message MUST include a paste-ready "next session's starting prompt" for the operator** per CLAUDE.md §Session Completion step 7. ~10 lines. Format: a single fenced markdown code block. Contents:

- **One sentence** framing what happened this session (so the next session right-sizes its reads-before-action).
- **A pointer** to this handoff doc by path (`dev/handoffs/<YYYY-MM-DD>-<your-slug>.md`).
- **The critical first action or gate** the next session must not skip (especially gates that `bd ready` would not surface — e.g., a brainstorm gate, a review requirement, a stakeholder check-in).

The session-start-briefing hook surfaces the most-recent handoff filename automatically; the operator's paste tells the next agent to READ the handoff and emphasizes what's implicit-droppable. Without step 7, the operator types freeform "continue where we left off" and the next agent stumbles into the gates by luck rather than design.

This template's own existence is partly to remind you of step 7. Don't skip it.
