# 10. No-squash merge for all PRs into integration branches; merge-commit (no-ff) replaces squash

Date: 2026-05-17
Status: Accepted (supersedes [ADR 0004](0004-per-task-branch-model.md) §"squash-merge into `feat/v0.0.1`" and the "Per-task branch with merge-commit (no squash). Rejected" alternative; the rest of ADR 0004 — per-task branches, branch-naming conventions, `gh pr merge --delete-branch`, post-merge `git branch -d` — stays accepted)
Deciders: cameronzucker, cedar

## Context

[ADR 0004](0004-per-task-branch-model.md) (2026-05-05) prescribed squash-merge as the default merge mode for per-task PRs landing on `feat/v0.0.1`. The rationale at the time:

> Squash-merge keeps `feat/v0.0.1` history one-commit-per-task while the un-squashed task branch retains the per-step record for the PR review trail.

That rationale rested on the un-squashed task branch persisting after merge. In practice, the per-task wrap deleted task branches immediately post-merge (`gh pr merge --delete-branch` + `git branch -d`). With the task branch gone, the per-step commits exist *only* in GitHub's PR-view UI — not in git itself, not in any local or remote ref. Anyone who clones the repo gets the squashed history with no recovery path other than navigating to the PR on github.com.

Cameron flagged this 2026-05-17 with the framing: "these are generally banned since they delete commit history and can result in genuine data loss." The two-week LFST sprint had already arrived at the same conclusion via the musing-bhabha incident class, and standing-conventions §3 (cross-project) codifies it:

> Squash-merge is **never** the right merge mode. The future reader of the integration-branch log is the agent doing forensics after a SNAFU. Per-commit history preserves bisect granularity, click-through readability, and surgical revertability. Squashing collapses task-internal commits into one opaque diff, which is exactly the information the forensics agent needs.

This ADR ratifies the no-squash rule for tuxlink, retroactively explaining the change. PRs #2-6 from the 2026-05-05 session were squash-merged under ADR 0004's prior rule; they cannot be un-squashed without violating the destructive-git ban (force-push would be required, banned per CLAUDE.md). The original commits for PRs #2-6 are preserved in GitHub's PR pages indefinitely as the audit trail. PR #7 onward (this sprint) merges as merge-commit (no-ff) under this new rule.

## Decision

### 1. Merge-commit no-fast-forward is the only sanctioned merge mode

Pull requests landing on `feat/v0.0.1` (and on `main` at release time) merge as merge commits with no fast-forward:

- **GitHub UI:** "Create a merge commit" button (the only enabled option per the 2026-05-17 repo-settings change).
- **`gh` CLI:** `gh pr merge <#> --merge --delete-branch` (NOT `--squash`, NOT `--rebase`).
- **Local merge (rare, e.g., the `ALLOW_INTEGRATION_COMMIT=1` carve-out path):** `git merge --no-ff <branch>` (NOT `--squash`, NOT `--ff-only`).

The repo settings (`allow_squash_merge: false`, `allow_merge_commit: true`, `delete_branch_on_merge: true`) enforce the GitHub side; the `block-destructive-git.sh` hook does not gate the merge command itself (it gates the destructive operations that squash-merge would have enabled retroactively, like `--force` push to undo a wrong merge).

### 2. Polish-before-push discipline replaces squash's WIP-noise mitigation

The original ADR 0004 squash decision was partly motivated by avoiding noisy WIP commits (`wip:`, `fixup!`, `oops:`) cluttering the integration branch. The replacement: **clean up WIP commits via non-interactive rebase on local un-pushed commits BEFORE `git push`.**

```bash
# On the task branch, before pushing:
git rebase <base-branch>                        # linearize against base
# (interactive rebase is banned per C1's hook update; squash/fixup must be
# avoided. Use `git commit --fixup=<sha>` + `git rebase --autosquash` if you
# need that workflow, but only on local un-pushed commits, never on shared
# history. Per the destructive-git ban, --amend on pushed commits is also
# blocked.)
git push -u origin <branch>                     # push the polished history
```

Once pushed, commits are immutable. The push gates the polish — anything you wanted to clean up needed to happen before the push.

### 3. ADR 0004 status: amended in part; superseded clauses called out

ADR 0004's per-task-branch model itself stays in force. The specific clauses superseded by this ADR:

- §"squash-merge into `feat/v0.0.1`" (the merge-mode prescription).
- §"git branch -d (NOT -D); git refuses if unmerged" wrap step (still correct, but the wrap's `git merge --squash` example in the body is now `git merge --no-ff`).
- §"Alternatives considered: Per-task branch with merge-commit (no squash). Rejected." — the rejection is itself superseded; this ADR is the new decision in that alternative's direction.

ADR 0004 §"Decision" wrap-text remains historically accurate as the at-the-time rule.

## Consequences

**Positive:**

- Per-task commits preserved on the integration branch via the merge commit's second-parent linkage. `git bisect` and `git blame` work at any granularity, not just at task boundaries.
- The "no recovery from git history" failure mode is eliminated. Anyone cloning the repo fresh sees the full per-step history; no dependency on GitHub's PR-view UI for archaeology.
- Aligns with standing-conventions §3 + LFST practice; cross-project knowledge transfer becomes cleaner.
- Compatible with the existing per-task-branch + `gh pr merge --delete-branch` + `git branch -d` wrap from ADR 0004; the only change is the merge-mode flag.

**Negative:**

- The integration branch (`feat/v0.0.1`) graph is no longer linear — each task PR adds a merge commit. `git log --oneline` shows more rows than under squash. For navigation, `git log --oneline --first-parent` collapses to merge-only.
- The 6 squash-merges already on `feat/v0.0.1` (PRs #2-6 + the 2026-05-05 docs handoff PR) remain squashed. Their underlying per-step commits live on GitHub's PR pages indefinitely as the audit trail but not in git.
- Operators (including agents) accustomed to squash-merge from other projects need to retrain. The disabled GitHub squash button + the hook's commit-discipline message provide the in-the-moment reminder.
- The polish-before-push discipline requires the operator to actually clean up WIP commits before pushing. If WIP noise gets shipped, the integration branch acquires noisy rows that can't be retroactively cleaned (forbidden by the destructive-git ban on pushed-commit `--amend` and on `git rebase -i` for shared history).

## Alternatives considered

- **Continue squash-merging.** Rejected per Cameron's 2026-05-17 pushback + standing-conventions §3 + the data-loss framing. The original ADR 0004 rationale was internally inconsistent (claimed history-preservation in a workflow that immediately deleted the source of that history); the inconsistency was a discovery, not a re-decision.

- **Rebase-merge.** Replays the task branch's commits one-by-one onto the integration tip, no merge commit. Produces a linear integration log with per-commit detail. Considered but rejected because: (a) the rebase rewrites commit SHAs, which breaks any external references to specific task-branch SHAs (e.g., in PR review comments or external docs); (b) the merge commit itself is informational — it captures the "this PR landed at this point" boundary that's useful for forensics; (c) the LFST experience favored merge-commit no-ff specifically over rebase-merge.

- **Retroactively un-squash PRs #2-6.** Rejected. Would require force-push to `feat/v0.0.1` (banned per CLAUDE.md). The cost of historical impurity is bounded; the cost of normalizing force-push as a recovery path is unbounded.

- **Author an in-place ADR 0004 amendment** (edit ADR 0004's text to flip the merge-mode prescription). Rejected per the ADR README §Lifecycle: "If a later ADR overrides it, the original's status changes to `Superseded by NNNN` … the original's content stays — it's the historical record." This ADR is the supersession; ADR 0004 stays as the historical record of the original decision.
