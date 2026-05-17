# 9. Worktree disposal ritual (inventory → archive → physical remove → prune)

Date: 2026-05-17
Status: Accepted (complements [ADR 0008](0008-worktrees-mandatory-under-bd-issue-ownership.md) — that ADR governs creation + ownership; this ADR governs end-of-life. Both apply.)
Deciders: cameronzucker, cedar

## Context

[ADR 0008](0008-worktrees-mandatory-under-bd-issue-ownership.md) ratified worktrees as the mandated coordination primitive for concurrent write work, under bd-issue ownership. ADR 0008 governs **creation**: when a worktree is permitted, how it's claimed, what its branch and path conventions are. It does not govern **end-of-life**.

End-of-life is where the failure modes hide. The LFST `musing-bhabha-805d0f` incident (May 2026) — referenced throughout the 2026-05-17 LFST→tuxlink port catalog — was a disposal failure, not a creation failure. The worktree was correctly used for its intended purpose; the disposal sequence ran `git worktree remove`, which performs a direct-unlink of the worktree's working tree. On Windows with Volume Shadow Copy disabled, this bypassed the Recycle Bin and silently destroyed untracked content. Files that existed only in that worktree — including a strategic-positioning design spec variant filename — were permanently lost. The cherry-pick survived in git history; the variant filename did not.

The variant only resurfaced because Cameron had separately exported the spec to other people via fileshare and could re-download it from a non-local backup. **Whatever else was in the worktree's untracked working tree at the moment of removal is permanently gone.**

The general failure mode: `git worktree remove`'s own "is it clean?" check does not surface gitignored-but-stateful content. `.beads/embeddeddolt/` is the canonical example for tuxlink — bd's local Dolt database is gitignored but holds the only copy of issue state that hasn't been auto-exported to `.beads/issues.jsonl` yet. A `git worktree remove` on a worktree with unsynced bd state would silently lose that state.

The C1 PR (landed earlier this sprint) already added `git worktree remove` to the destructive-git hook's ban list. That closes the immediate failure mode. This ADR documents the **replacement workflow** — what to do *instead* of `git worktree remove` — so the ban is paired with a sanctioned safe path.

## Decision

### The 4-step disposal ritual

Worktree disposal requires four steps in order. There is no shortcut. The destructive-git hook enforces the prohibition on `git worktree remove` (via the C1 update); the ritual provides the alternative.

**Step 1 — Inventory.** From inside the worktree being disposed:

```bash
git status --short                                          # tracked dirty
git ls-files --others --exclude-standard                    # untracked
git ls-files --others --ignored --exclude-standard          # gitignored on disk
git stash list                                              # worktree-scoped stashes
```

The four commands cover the four categories of at-risk content: dirty-tracked, untracked, gitignored-stateful, stashed. If any return non-empty, proceed to Step 2 before Step 3. If all return empty AND `bd show <worktree-issue-id>` confirms the work is closed, proceed directly to Step 3.

**Step 2 — Propagate or archive.** Anything Step 1 surfaced that is not safe to lose gets either:

- **Propagated forward:** commit + push to a topic branch (`git add`, `git commit -m "..."`, `git push origin <branch>`), OR
- **Archived locally:** `cd` to the main repo first, then tar from outside the doomed worktree:

```bash
cd <main-repo-path>                                                      # CRITICAL: leave the worktree before archiving
tar czf .claude/worktree-archives/<worktree-name>-$(date -u +%Y%m%dT%H%M%SZ).tar.gz <full-worktree-path>
```

The `cd` is load-bearing: if you `tar czf .claude/worktree-archives/...` from inside the doomed worktree, the relative path resolves to `<worktree>/.claude/worktree-archives/...` — and Step 3's `rm -rf <worktree>` then deletes the archive along with the worktree. The codex 2026-05-17 D4 review caught this in the original recipe; the cd-back-first formulation is the fix.

The archive captures the full working tree (including untracked + ignored). It is intended for "I'm not sure if I need this; preserving it is cheap; deciding later is fine." `.claude/worktree-archives/` is `.gitignore`d — archives stay per-machine, not pushed to origin.

**Step 3 — Physical remove.** Once Step 1 + 2 are complete:

```bash
rm -rf <worktree-path>
```

`rm -rf` is the OS-level delete. It is not hook-gated; once the inventory and archive are done, operator (or agent on operator-implicit authorization within the disposal workflow) discretion applies. `rm` on Linux generally bypasses any trash mechanism by default — that's why Step 2 is mandatory before Step 3. The hook does not block `rm -rf` because doing so would block too much unrelated legitimate work; the discipline lives in the ritual.

**Step 4 — Prune git's registry.**

```bash
git worktree prune
```

`git worktree prune` cleans git's internal worktree registry of entries whose working trees no longer exist on disk. Skipping this leaves stale entries that show up as ghost rows in `git worktree list` until pruned. Always run after Step 3.

### Forbidden alternative: `git worktree remove`

The C1 PR's destructive-git hook bans `git worktree remove` outright. The hook's deny message points back to this ritual. Bypassing the hook (`--no-verify`, manual hook disable, etc.) is also banned per the existing destructive-git rules. There is no sanctioned path that uses `git worktree remove`; if a workflow seems to require it, the workflow is wrong — re-derive via the ritual.

### Handoff documents enumerate worktree state

The Session Completion section (added in C4, amended in this PR) requires the session-end handoff document to enumerate, for each living worktree:

- Worktree path + claiming bd issue (per ADR 0008)
- Untracked content (`git ls-files --others --exclude-standard` output)
- Gitignored-but-stateful content (`git ls-files --others --ignored --exclude-standard` output, filtered against `dev/state-paths.md` for which paths are recoverable from elsewhere)
- Disposition for any at-risk content: will-commit / will-archive / will-discard / pending-decision

A handoff that does not enumerate worktree state for a living worktree is a defect. A future session reading the handoff has no way to safely dispose of the worktree without re-running Step 1 — which is fine, but explicit handoffs avoid the situation where a disposal happens without anyone realizing Step 1 was needed.

### `.claude/worktree-archives/` lives outside git

The archive directory is `.gitignore`d. Archives are per-machine, large (full working tree zips), and represent ephemeral safety-net state — not project artifacts. Treat them like backups: prune periodically when no longer needed.

## Consequences

**Positive:**

- The musing-bhabha failure mode is structurally blocked: `git worktree remove` is hook-denied (C1), and the only sanctioned alternative is the ritual which front-loads inventory + archive.
- Disposal becomes auditable: the archive (if created) is a timestamped snapshot of the worktree at the moment of disposal; the handoff document enumerates what was at risk. Post-incident reconstruction is possible from the archive.
- The cost of the ritual is small in the common case (no at-risk content → 4 commands → done) and bounded in the worst case (archive a worktree of any size in seconds).
- Aligns tuxlink's worktree disposal with LFST's discipline (LFST's ADR 0008 §"Worktree disposal discipline" originated this ritual after the musing-bhabha incident).

**Negative:**

- Adds 4 commands to a workflow that previously was 1 command (`git worktree remove`). For frequent worktree turnover this adds ambient cost. D4's `new_tuxlink_worktree.sh` script could be paired with a future `dispose_tuxlink_worktree.sh` that runs the ritual end-to-end; not in scope for this sprint.
- Operators (including Cameron) accustomed to `git worktree remove` from other projects will need to retrain. The hook's deny message points at the ritual to ease this.
- `.claude/worktree-archives/` accumulates artifacts; if not periodically pruned, it can grow large. The `.gitignore` keeps it out of the repo but not off the filesystem. Operator hygiene.

**Watched failure modes** (signals this ADR needs revision):

1. **An archive turns out to be insufficient** — Step 2's `tar czf` excludes something important (symlinks, permissions, extended attributes on Linux). → Response: switch to a different archive format (`zip` for cross-platform; `tar --xattrs --acls` for full Linux fidelity) if a recovery scenario reveals a gap.
2. **Operators bypass the ritual** under time pressure. → Response: the hook ban + the ADR-grounded language in CLAUDE.md and the deny message are the deterrent; if bypass happens, treat as a near-miss + document the incident.
3. **`bd show <id>` doesn't actually confirm work-closed for a worktree.** A stale `in_progress` issue could be misread as "work still in flight" when the worktree is actually abandoned. → Response: extend the `bd doctor` checks to surface worktrees whose bd issue's `last_updated` is significantly stale; surface those for operator triage.
4. **The archive directory grows unbounded.** → Response: periodic operator-driven `find .claude/worktree-archives/ -mtime +90 -delete` to prune; consider a `bd remember`-tracked "last pruned" timestamp.

## Alternatives considered

- **Allow `git worktree remove` for "clean" worktrees** (where git's own check passes). Rejected. The check does not surface gitignored-but-stateful content; the canonical example (`.beads/embeddeddolt/`) is exactly the case where git's clean check passes but real state would be destroyed.
- **Require archive ALWAYS, even for empty worktrees.** Rejected. Adds cost without benefit in the common case (work was committed + pushed, no at-risk content); the ritual's Step 1 → Step 2 conditional captures the right semantics.
- **Build a sanctioned safe-remove wrapper** (`safe-worktree-remove.sh`) that runs the ritual end-to-end and only then calls `git worktree remove`. Deferred. A wrapper is the right ergonomic move; this ADR establishes the manual workflow first so the wrapper has an unambiguous reference implementation. Wrapper can land as a follow-up.
- **Codify the ritual in standing-conventions §4 only and skip a project-specific ADR.** Rejected. The ritual is a project-relevant decision: tuxlink's hook layer (C1) is the structural enforcement; the ADR is the project-specific record that the operator + the hook + the ritual are coordinated. Standing-conventions §4 already does point here; this ADR is the receiving-end documentation.
