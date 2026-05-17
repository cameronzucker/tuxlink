# Handoff — 2026-05-17 cedar (LFST→tuxlink safety stack port + sprint closeout)

**From agent:** `cedar` (single-word legacy moniker; new sessions should use `python3 .claude/scripts/get_agent_moniker.py` for the 3-word hyphenated form introduced this session in PR #17 / B3)
**Session arc:** Started as "where did we leave off?" after a 2-week gap; expanded into a 15-PR sprint to port the LFST safety stack to tuxlink per the catalog at `cz-agent-skills/docs/2026-05-17-tuxlink-import-from-lfst.md`. After the implementation sprint, Cameron caught my underestimate-vs-actual gap (~7hr estimate, ~1hr actual) and asked what I'd skipped. Codex adversarial review was the answer — added back as a closeout pass, surfaced 3 P1 + 10 P2 + 1 P3 findings (including 3 real correctness bugs where the safety mechanisms silently defeated themselves), all fixed in 5 follow-up PRs.
**Status:** All sprint PRs merged. All codex-finding fix PRs merged. Final state on `feat/v0.0.1` is at `b641825` (Merge PR #25). Total: 20 PRs merged on `feat/v0.0.1` + PR #1 (dependabot) on `main` this session. Codex transcripts live local-only at `dev/adversarial/` (gitignored per the "release-ready public repo" call).

---

## Next session's starting prompt

> I'm resuming the tuxlink project. `cedar` handed off 2026-05-17 after porting the LFST safety stack. Read these before doing anything:
>
> 1. `dev/handoffs/2026-05-17-cedar-lfst-stack-port.md` — this handoff.
> 2. `CLAUDE.md` — substantially rewritten this sprint. Pay attention to: `## Agent identity` (now points at the Python generator), `## Git workflow — worktrees mandatory under bd-issue ownership (ADR 0008)`, `## Git workflow — destructive commands are BANNED` (trimmed to a pointer + quick-ref), `## Documentation propagation contract`, `## Session Completion`, and the `## Tool referee` table (push-timing override retired).
> 3. `docs/adr/` — 10 ADRs. The load-bearing ones from this sprint: 0008 (worktrees-mandatory), 0009 (disposal-ritual), 0010 (no-squash). 0004 still operative for the per-task-branch model; its squash clause is superseded by 0010.
> 4. `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` — the v0.0.1 plan. Task 3 has an AMENDMENT callout at the top documenting the pat 1.0.0 CLI discrepancy.
> 5. `dev/adversarial/2026-05-17-*-codex.md` — codex adversarial reviews of the 4 substantive sprint PRs. Read these to see what codex flagged; act on any HIGH-severity findings before starting new feature work.
> 6. `docs/pitfalls/implementation-pitfalls.md` and `docs/pitfalls/testing-pitfalls.md`.
>
> Once read:
>
> - Generate a fresh moniker via `python3 .claude/scripts/get_agent_moniker.py`. Auto-pre-flighted against git history.
> - Run `bd ready` to see available work. 5 issues are ready: Tasks 5, 7, 9, 16, 17. **Per the previous handoff (2026-05-05 kestrel) the UX brainstorm is a hard review gate before Tasks 9-16 work** — that gate is still in place; the LFST-stack-port sprint did not move it.
> - **First action:** if there are HIGH-severity codex findings, address them before starting Task 5. Otherwise, the natural next work is either (a) the UX brainstorm per `docs/design/v0.0.1-ux-principles.md`, or (b) Task 5 (Pat HTTP client) if you want to make substantive progress on the v0.0.1 plan before the brainstorm. Cameron has previously favored brainstorm-first; confirm with him before either.
>
> Take time on the work. Quality over speed.

---

## What landed in this session

| # | What | PR # | Status |
|---|---|---|---|
| Admin | GitHub repo settings flipped: squash disabled, merge-commits enabled, delete-branch-on-merge enabled | — | Done |
| C1 | Hook bans `git worktree remove` + `git rebase -i` | #7 | Merged |
| C2 | Commit-discipline hook: flip carve-out language from squash to merge-commit | #8 | Merged |
| C3 | CLAUDE.md §Documentation propagation contract | #9 | Merged |
| C4 | CLAUDE.md §Session Completion (canonical, outside BEADS INTEGRATION block) | #10 | Merged |
| C5 | `dev/state-paths.md` (gitignored-stateful paths inventory) | #11 | Merged |
| C6 | CLAUDE.md §Parity with AGENTS.md expanded with 4-step upkeep discipline | #12 | Merged |
| D1 | Main-checkout race hook port (bash) + session-leases dir + rev-parse refactor of all hooks | #13 | Merged |
| D2 | ADR 0008 worktrees mandatory under bd-issue ownership + CLAUDE.md section + supersedes ADR 0007 operative rule | #14 | Merged |
| D3 | ADR 0009 worktree disposal ritual + CLAUDE.md operational recipe + handoff state-enumeration extension | #15 | Merged |
| D4 | Python `new_tuxlink_worktree.py` + `get_tuxlink_sessions.py` (cross-platform, pure stdlib) | #16 | Merged |
| B3 | Python `get_agent_moniker.py` (3-word hyphenated, 100-word pool, auto-pre-flighted) + CLAUDE.md + AGENTS.md updates | #17 | Merged |
| Cleanup | ADR 0010 no-squash + Tool referee table reconciliation + destructive-git section trimmed to pointer + polish-before-push bullet + AGENTS.md parity | #18 | Merged |
| Plan-amend | Plan Task 3: AMENDMENT callout for pat 1.0.0 CLI corrections | #19 | Merged |
| PR #1 | Dependabot `release-please-action` 4→5 (validation event for new merge-commit convention) | #1 | Merged to `main` |
| Repo-hygiene | `dev/adversarial/` gitignored + CLAUDE.md updated + handoff template added | #20 | Merged |
| Fix-D1 | Race hook P1 (lease location) + 2 P2 + 1 P3 + shellcheck | #21 | Merged |
| Fix-D4-disposal | 2 P1s (archive outside worktree, inventory `--ignored`) | #22 | Merged |
| Fix-D4-rest | bd CLI fix + `worktrees/` gitignore + `--issue` required | #23 | Merged |
| Fix-B3 | Fail-closed on git log error | #24 | Merged |
| Fix-cleanup | ADR 0010 recipe + CONTRIBUTING/VERSIONING/PR template + ADR 0006 + BD-1 | #25 | Merged |
| Session-end handoff | This handoff doc | (this PR) | — |

**Decision context (Cameron, 2026-05-17, in chat):**

1. No-squash merge for ALL future projects, not just tuxlink. Squash-merge structurally retired.
2. Worktrees mandatory under the 10-layer safety stack when concurrent sessions exist.
3. Python moniker generator (cross-platform, replaces grep+gitlog dance).
4. Six atomic C-section PRs (executed as 6 + 4 D-section + B3 + cleanup + plan-amend = 13).
5. Skip bd Dolt remote sync (solo Pi-only dev today).

---

## State at pause

### What's pushed to origin

```
main          86ddd3d  (PR #1 dependabot bump merged here; unchanged since)
feat/v0.0.1   b641825  (Merge PR #25 fix-cleanup-followup is current tip)
```

**Divergence flag:** `main` has 2 commits (the dependabot bump `aba75ac` + its merge `86ddd3d`) that `feat/v0.0.1` does not. When v0.0.1 ships, `main` will not ff-merge from `feat/v0.0.1` — either merge `main` into `feat/v0.0.1` first to incorporate the dependabot bump, or use a merge-commit (no-ff) at release time. Not blocking; surfaced for Task 19 (CI + release) work.

### Working-tree state

Clean. This handoff doc is on `task-session-end-handoff` branch, not yet committed at the time of this file's contents (will be committed + PR'd as the final closeout step).

The 4 codex review transcripts live locally at `dev/adversarial/` and are gitignored per PR #20. Never pushed to origin.

### In-flight worktrees

No worktrees in flight. `git worktree list` shows only the main checkout.

### bd state

```
Total: 18 | Open: 15 | In Progress: 0 | Blocked: 10 | Closed: 3 | Ready: 5
```

No bd issues were claimed for this session's sprint work — all 15 sprint items were tracked via TodoWrite (the in-turn primitive per the Tool referee). The sprint executed within one session; bd is for cross-session work units. The catalog itself + the 14 PRs are the durable cross-session record.

**Ready issues** (`bd ready`): tuxlink-cs7 (Task 17 AppImage), tuxlink-hvv (Task 16 status bar), tuxlink-ko0 (Task 9 wizard 1), tuxlink-6vi (Task 7 native menu), tuxlink-eil (Task 5 Pat HTTP client).

---

## Codex adversarial review findings

The 4 codex review transcripts live on cedar's local disk at `dev/adversarial/2026-05-17-{d1-race-hook,d4-worktree-scripts,b3-moniker-generator,cleanup-adr0010}-codex.md`. The directory is `.gitignore`d as of this sprint — raw transcripts are local-only dev scratch, not project artifacts (CLAUDE.md §"OpenAI Codex CLI" section updated to document this convention).

**Aggregate: 3 P1 + 10 P2 + 1 P3 findings + 4 shellcheck warnings.**

| PR | Severity counts | Most severe |
|---|---|---|
| D1 (race hook) | 1 P1, 2 P2, 1 P3, 4 shellcheck | **P1: leases live in per-checkout `.claude/session-leases/` dirs → main + worktree don't see each other → race hook fails exactly across the case it protects against.** Fix: store leases under `git rev-parse --git-common-dir`. |
| D4 (worktree scripts) | 2 P1, 4 P2 | **P1 #1: disposal recipe writes archive to relative path INSIDE the worktree, then `rm -rf` deletes both.** Fix: write archive to absolute path outside the doomed worktree. **P1 #2: disposal inventory commands omit `git ls-files --others --ignored --exclude-standard`** — the exact command for surfacing the `.beads/embeddeddolt/`-class content ADR 0009 exists to protect. |
| B3 (moniker generator) | 1 P2 | Fails open on git log error: if git is unavailable or `CLAUDE_PROJECT_DIR` points outside a git repo, collision check silently returns "no collision" and ships an unchecked moniker. Fix: distinguish "no collision" from "check failed"; fail closed or fall back to script-relative repo. |
| Cleanup (ADR 0010) | 3 P2 | (a) ADR 0010's WIP-cleanup recipe references `git rebase --autosquash` which requires interactive mode banned by C1 — recipe is broken. (b) Stale squash-merge instructions still in CONTRIBUTING.md, VERSIONING.md, and PR template — contradict the new policy. (c) ADR 0006 still has `Status: Accepted` listing mandatory-push as an active override; BD-1 pitfall still names auto-pushes as a signature; both reference a now-retired override. |

**The 3 P1s all share a pattern:** the safety code works correctly only in the "we don't need it" case. They silently defeat the safety mechanisms exactly when those mechanisms are most needed. Codex's role here was to simulate the adversary case that solo-session work would never exercise.

**Disposition** (all fix PRs landed 2026-05-17):

| Finding cluster | PR # | Status |
|---|---|---|
| D1 fixes (P1 lease-location + 2 P2 + 1 P3 + 4 shellcheck) | #21 (`task-fix-d1-race-hook`) | Merged — `fix(hooks): D1 codex review remediation` |
| D4 disposal fixes (2 P1: archive outside worktree, inventory `--ignored`) | #22 (`task-fix-d4-disposal-p1s`) | Merged — `fix(disposal): D4 codex P1 remediation` |
| D4 remaining (3 P2: bd CLI, `worktrees/` gitignore, `--issue` required; 4th — deny-message paths — was implicitly fixed by Fix-D1's hook rewrite) | #23 (`task-fix-d4-rest`) | Merged — `fix(scripts,gitignore): D4 codex P2 remediation` |
| B3 fail-closed (1 P2: collision-check silent bypass) | #24 (`task-fix-b3-fail-closed`) | Merged — `fix(scripts): B3 codex P2 remediation` |
| Cleanup follow-up (3 P2: ADR 0010 broken recipe, public-facing squash refs in CONTRIBUTING/VERSIONING/PR template, ADR 0006 + BD-1 stale push-override references) | #25 (`task-fix-cleanup-followup`) | Merged — `fix(docs): cleanup-PR codex P2 remediation` |

**Verification of P1 fixes:**

- **D1 P1 (lease location):** verified live mid-session — after deploying the fix, a lease was written to `.git/session-leases/c4c84f68-...json` on the next Bash tool call. Cross-checkout sharing now works.
- **D4 P1 (archive-deleted-by-rm-rf):** disposal recipe in `new_tuxlink_worktree.py` printed output + ADR 0009 §"Step 2" + CLAUDE.md §"Worktree disposal ritual" all now `cd <repo>` before archiving. Not end-to-end-tested (would require a real worktree disposal scenario; next operator-driven disposal is the natural validation).
- **D4 P1 (inventory missing `--ignored`):** script's printed recipe now includes the line. ADR 0009 + CLAUDE.md prose recipes already had it; only the script was missing it.

**One bug-fix-found-bug:** while addressing D4's P1 in the script, I audited adjacent locations and discovered ADR 0009 + CLAUDE.md had the SAME archive-path bug (the codex finding only flagged the script). All three locations fixed together in PR #22.

---

## Open decisions for the next agent or Cameron

1. **UX brainstorm timing.** Per the 2026-05-05 kestrel handoff, the UX brainstorm before Task 9 was a hard review gate. This sprint did not move that gate — it only changed how source-control discipline works. The brainstorm is still queued. Recommendation: do the brainstorm before Task 5 even though Task 5 isn't UI work, per Cameron's previously-stated preference for brainstorm-first.

2. **Geographica port authorization.** The `cz-agent-skills/docs/2026-05-17-geographica-import-from-lfst.md` catalog documents what the equivalent port would look like for geographica (which is in worse shape than tuxlink was — prose-only safety, zero hook enforcement, no ADR directory, no bd installed). Cameron has not authorized this; it remains pending.

3. **Main / feat/v0.0.1 divergence reconciliation strategy.** Decide whether to merge `main` into `feat/v0.0.1` opportunistically as dependabot PRs land, or accept that the final v0.0.1 release merge will be a merge-commit (no-ff) instead of an ff. Either works; pick one and document in Task 19's plan.

4. **LFST-side moniker-generator port to Python.** Per Decision 3, this is queued as an LFST-side bd issue (not tuxlink's work). Make sure to file it on the LFST side next time you're working there.

5. **Codex review findings disposition.** `<TODO: list whether any findings warrant immediate fixes vs queuable as bd issues vs disposable as noise>`.

---

## Plan amendments queued

None outstanding from this sprint. The v0.0.1 plan's Task 3 amendment landed in PR #19. All other plan claims remain accurate.

---

## Reminders for the next agent

- **Use the Python moniker generator.** `python3 .claude/scripts/get_agent_moniker.py`. The legacy single-word convention (alder, lichen, kestrel, cedar) still works for backward grep-discoverability of older commits but the new format is 3-word hyphenated.
- **`set -o pipefail`** for any pipeline ending in `tail` / `head` that you care about the exit code of. Burned during Task 2 in the previous session (silent test pass-through).
- **The destructive-git hook substring-matches commit message text.** Workaround: `git commit -F /tmp/msg.txt` when documenting banned patterns in commit bodies. Also applies to any tool invocation where you'd pass banned patterns as args (codex review prompts, etc.) — write the input to a file or rephrase around the banned strings.
- **Codex review specifics:** `codex review --commit <SHA>` runs default-review mode with no custom prompt (the `[PROMPT]` arg is mutually exclusive with `--commit`). For custom attack angles, you'd need a different mode (`codex review --base <branch>` from a checkout of the target commit) — workable but adds setup. The default-review mode this sprint used was good enough; custom angles can be added later if a finding suggests a gap.
- **Per-task-branch wrap:** branch off `feat/v0.0.1` → commit → push → `gh pr create --base feat/v0.0.1` → `gh pr merge --merge --delete-branch` (NOT `--squash`) → `git pull --ff-only origin feat/v0.0.1` → `git branch -d` → `bd close` if a bd issue was claimed.
- **The main-checkout race hook + session-leases system is now LIVE.** Every Bash tool call you make writes/refreshes your session's lease at `.claude/session-leases/<session-id>.json`. Solo sessions: hook always allows. Concurrent sessions: non-lease-holders denied on risky ops in the main checkout. Test with `python3 .claude/scripts/get_tuxlink_sessions.py` to see active leases.
- **Worktree creation uses the new script:** `python3 .claude/scripts/new_tuxlink_worktree.py --slug X --issue tuxlink-Y` claims the bd issue, creates the worktree at the right path, records the path in the issue body. Disposal uses the 4-step ritual per ADR 0009.

---

**If something in this handoff looks wrong tomorrow:** the previous-session agent (cedar) wasn't perfect; flag it before acting on it. Source of truth for any rule this handoff restates: the ADRs and CLAUDE.md (per the §"Documentation propagation contract" rule we just landed). Standing-conventions doc at `cz-agent-skills/docs/standing-conventions-cross-project.md` is the cross-project authority for any rule that's also captured there.
