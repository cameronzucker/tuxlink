# 17. Branch lifecycle state machine + pre-commit/pre-push hook enforcement

Date: 2026-06-01
Status: Accepted
Deciders: cameronzucker, sorrel-alder-cypress (this session), vetch-plover-ridge (Codex 2026-06-01 P0 #1 + P1 #12 + P2 #17)

## Context

Tuxlink uses the per-task branch model ([ADR 0004](0004-per-task-branch-model.md)) with merge-commit (no-squash, [ADR 0010](0010-no-squash-merge.md)) preserving every task-branch commit on the integration branch. Worktrees are mandatory under bd-issue ownership ([ADR 0008](0008-worktrees-mandatory-under-bd-issue-ownership.md)). Together these create a population of feature branches that go through a predictable lifecycle: created off a base, gather commits, get a PR, get merged, and ideally get retired.

On 2026-06-01 the operator surfaced the "v1p failure mode" during the morning forensic session: an agent had continued committing to `bd-tuxlink-v1p/html-forms-execution` *after* PR #200 merged it, producing orphan commits unreachable from any open PR. These commits did real work, but landed nowhere — and were only discovered because the operator noticed `tauri dev` was building from a stale binary. The same checkout had been 858 commits behind `origin/main` for two weeks because no rebase forward had happened.

The Codex adrev round on the proposed convergence-discipline (transcript at `dev/adversarial/2026-06-01-convergence-discipline-codex.md`) identified the underlying defect: **branch lifecycle is implicit**. A post-merge agent has no signal that the branch they're on is now "dead"; their next `git commit` succeeds; their next `git push` succeeds (because the branch still exists on origin until somebody runs `git push origin --delete`); the commits go nowhere. Codex P0 #1:

> A hook that runs immediately after `gh pr merge` checks the branch only at merge time. In the v1p failure, the bad commits happened *after* the merge, so `rev-list <merged-tip>..origin/<branch>` would have been `0` when the hook fired. Treat merge as a branch lifecycle transition. After merge, mark the branch dead and enforce that state in a `git commit` / `git push` preflight: if the current branch already has a merged PR or a local tombstone, deny further commits/pushes and instruct the agent to create a fresh bd issue/branch/PR.

Codex P1 #12 added the session-end ordering corollary:

> CLAUDE.md requires bd updates, handoff doc, commit, and push at session end. If the PR is merged before those steps, agents are incentivized to keep committing to the merged branch. Codify ordering: session-end artifacts must be committed before merge, or the agent must create a new follow-up branch after merge. "PR merge" should be a terminal action for that branch/worktree.

Codex P2 #17 named the discipline:

> Add an ADR defining branch states: active, PR-open, merged/dead, follow-up, disposed. Then update CLAUDE.md/AGENTS.md as summaries per the existing documentation propagation contract.

`scripts/converge-build.sh` v1 ([tuxlink-qepd](#), PR #203) classifies branches but only *warns* on merged-dead — by design, to ship fast. The lift to *refuse* requires hook-level enforcement that survives outside `converge-build`'s call site: every `git commit` and every `git push`, regardless of who invoked them (Claude Code, Codex CLI, human shell, `gh` web merges).

## Decision

Define an explicit branch lifecycle state machine and enforce its no-write states via standard git hooks installed under `.githooks/` (not `.claude/hooks/` — the enforcement is universal, not Claude-specific).

### The state machine

```
┌──────────┐  bd create + gh pr create   ┌──────────┐
│  active  │ ──────────────────────────▶ │ pr-open  │
└──────────┘                             └──────────┘
     │                                         │
     │ branch off merged predecessor           │ gh pr merge
     │ (new bd issue)                          ▼
     │                                   ┌──────────────┐
     ▼                                   │ merged-dead  │ ───┐
┌──────────┐                             └──────────────┘    │ commit/push: REFUSE
│follow-up │                                                 │ (operator path: create
└──────────┘                                                 │  new branch off main)
     │                                                       │
     │ (becomes active in its own lifecycle)                 │
     ▼                                                       ▼
   ...                                              ┌────────────────┐
                                                    │    disposed    │
                                                    └────────────────┘
                                                    (worktree removed,
                                                    branch deleted from
                                                    origin via gh pr merge
                                                    --delete-branch or
                                                    explicit push --delete)
```

Closed-without-merge PRs are equivalent to `merged-dead` for write enforcement (the PR is terminal; the branch should not accumulate new work without a fresh decision).

**Classification heuristics (the hooks consult these in order):**

1. **Branch is `main` or any protected ref:** the destructive-git hook + GitHub branch protection cover this layer; the lifecycle hooks do not apply.
2. **Bot-owned branch** (matches `release-please--*`, `dependabot/*`): always allow commits (bots manage their own lifecycle).
3. **`gh pr list --head <branch> --state merged --limit 1` returns a result with `mergedAt`:** state = `merged-dead`. **Refuse commit + push.**
4. **`gh pr list --head <branch> --state closed --limit 1` returns a result without `mergedAt`:** state = `closed-dead`. **Refuse commit + push** unless the operator explicitly reopens the PR.
5. **`gh pr list --head <branch> --state open` returns a result:** state = `pr-open`. Allow.
6. **Otherwise:** state = `active`. Allow.

**Graceful degradation:** if `gh` is unavailable or unauthenticated, the hooks WARN ON STDERR ("could not consult gh — branch state unknown") and allow the operation. The CI scheduled audit ([tuxlink-ui3i](#), separate sub-issue) is the independent backstop for cases the local hooks miss (web-UI merges, force-pushes, agents with no `gh` auth).

### Hook installation

`.githooks/pre-commit` + `.githooks/pre-push` ship in-tree. Operators activate them via:

```bash
git config core.hooksPath .githooks
```

`scripts/install-githooks.sh` is the convenience wrapper: it sets the config and runs a sanity check that both hooks are executable.

Hooks are written in POSIX-portable bash. They depend only on `git` + `gh` (the latter degrades gracefully) — same dependency surface as `converge-build.sh`. They do not invoke Claude-specific tooling.

### Bypass policy

Per the project's `--no-verify` ban (destructive-git hook C1), `git commit --no-verify` is denied at the Claude tool layer. The hooks add a *true* enforcement that survives outside Claude Code: a non-Claude shell that doesn't load the destructive-git harness can still bypass via `--no-verify`, BUT the CI nightly audit (tuxlink-ui3i) will detect the orphan commit retroactively.

Operators who genuinely need to commit on a merged-dead branch (e.g., a hotfix amended commit that needs to land on the same merged ref before a force-push remediation) use the documented escape hatch:

```bash
TUXLINK_BRANCH_LIFECYCLE_OVERRIDE=I-know-what-Im-doing git commit ...
```

The hook requires the **exact sentinel string** (per Codex 2026-06-01 P2 — bare `=true` or `=1` or `=please` does not bypass; the operator must type the documented value). The override logs to `dev/scratch/branch-lifecycle-overrides.log` for audit.

`git -c core.hooksPath=/dev/null commit ...` is **NOT a sanctioned bypass**: it would disable the hook silently, defeating the audit trail. A future addition to the destructive-git hook (`.claude/hooks/block-destructive-git.sh`) should add it to the deny list alongside `--no-verify`; filed as a follow-up (the destructive-git hook is the right enforcement layer for that pattern).

### Session-end ordering rule (Codex P1 #12)

Per session-end discipline (CLAUDE.md §Session Completion + `standing-conventions-cross-project.md` §7), session artifacts (handoff doc, bd `--close`, final commits) MUST land BEFORE the PR is merged. If the operator merges first and discovers a missing artifact afterward:

- DO NOT commit to the now-merged branch (the hook refuses).
- Open a fresh bd issue (`bd create --title "follow-up to <merged-id>: <thing>"`).
- Create a new `bd-<new-id>/<slug>` branch off `main`.
- Land the artifact there with its own PR.

This converts the orphan-post-merge anti-pattern into the deliberate `follow-up` state in the state machine.

### Watched failure modes

1. **Concurrent merge race**: agent A is committing on branch `bd-x/foo`; agent B merges its PR while A is in flight. A's pre-commit hook fires AFTER the merge, refusing A's commit even though A's work was in flight before the merge. **Disposition:** this is correct hook behavior — A's commit IS now an orphan-post-merge attempt. A re-bases onto the new `main` and creates a follow-up branch if the work is still relevant.

2. **`gh` quota exhaustion**: hooks fail open (warn + allow). **Disposition:** graceful degradation is intentional; the CI nightly audit (tuxlink-ui3i) is the backstop.

3. **Detached HEAD**: hooks see no symbolic branch ref. **Disposition:** detached HEAD is rare in operator workflows; hooks fail open with a clearer warning ("detached HEAD — branch lifecycle classification skipped").

4. **First clone on a fresh machine**: `core.hooksPath` not yet set. **Disposition:** `scripts/install-githooks.sh` is the operator's first-run step; the README + CLAUDE.md will point to it.

5. **Merge-commit-on-task-branch**: an operator runs `git merge main` ON the task branch (vs. rebasing). The branch is still `pr-open` — commits should still be allowed. **Disposition:** classification heuristic only consults PR state; merge commits don't change branch state.

## Consequences

- Two new files in-tree under `.githooks/`. Both POSIX-portable, single-file, no submodules.
- One new file under `scripts/install-githooks.sh` (convenience wrapper).
- One new Python test suite under `tests/branch_state_test.py` for the classification logic (uses subprocess + a temp git repo for fixtures).
- The CLAUDE.md propagation contract permits ONE operational doc to point at this ADR. CLAUDE.md gets the pointer; the existing AGENTS.md parity-check covers the parity surface.
- `converge-build.sh` v1's branch-classification logic is *unchanged* by this ADR (v1 only warns; the hooks are the orthogonal enforcement layer). A future v2 of converge-build (`tuxlink-pxmi` or follow-up) may choose to use the hooks' classification function as a shared library; that's a refactor, not a contract change.
- The override env-var creates a documented escape hatch. The audit log at `dev/scratch/branch-lifecycle-overrides.log` (gitignored) is the forensic record.
- The CI nightly audit ([tuxlink-ui3i](#)) is now an explicit complement to local hooks rather than a "nice to have" — it catches `--no-verify` bypasses and non-local merge paths.

## Alternatives considered

### A. Post-merge hook (the rejected pre-Codex design)

The original proposal: a hook that fires on `PostToolUse` for `gh pr merge` and checks the branch is clean. Codex P0 #1 demolished this: the commit that creates the orphan happens AFTER the merge, so a merge-time check finds nothing wrong. **Rejected** — the check needs to be at commit time, not merge time.

### B. Convergence-build-only enforcement

Make `scripts/converge-build.sh` refuse to operate on merged-dead branches; leave `git commit` / `git push` unguarded. **Rejected** — agents and operators commit constantly outside `converge-build`. The script's classification is one wrapper; the hooks are the universal enforcement.

### C. Claude-specific hooks under `.claude/hooks/`

A PreToolUse hook on `git commit` could intercept Claude Code's tool calls and classify the branch. **Partially adopted but insufficient**: Claude hooks only see Claude Code's tool calls. Codex CLI, `gh` web merges, raw shell from a human operator, and `gh pr merge` invoked from a non-Claude script all bypass `.claude/hooks/`. The standard git hooks at `.githooks/` are the layer that's universal across all those invocation paths. The .claude/ destructive-git hook remains useful as a defense-in-depth layer for the cases where Claude IS the invoker.

### D. Branch-name regex enforcement (no `gh` dependency)

Refuse commits to branches whose names match `merged-*` / `dead-*`. **Rejected** — relies on agents renaming branches after merge (they don't). PR state via `gh` is the actual source of truth and is robust to naming convention drift.

### E. Local tombstone file (`.git/merged-branches.json`)

Maintain a local list of merged-dead branches updated by a PostToolUse hook on `gh pr merge`. Cheaper than `gh pr list` per commit. **Rejected for v1** — the tombstone goes stale immediately if any other actor merges via web UI / API. The `gh` query is authoritative; if performance becomes a problem later, the tombstone can be added as a cache layer with TTL.
