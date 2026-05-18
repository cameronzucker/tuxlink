# Fork-setup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up `cameronzucker/tuxlink-pat` as a fork of `la5nta/pat`, wire tuxlink's `src-tauri/` build to source Pat from the fork via git submodule + Go-build integration, and ship the AppImage bundling path so end-users see a single artifact with Pat embedded.

**Architecture:** Two-PR landing across two repos. PR-A: README into the new `tuxlink-pat` fork (operator creates fork + branch protection first). PR-B: submodule + `build.rs` Go-build invocation + `tauri.conf.json` `externalBin` + CI workflow + `docs/development.md` into tuxlink. The agent does no GH-ops; the operator does fork creation + branch protection + Issues enablement; pre-flight gates between phases enforce dependency ordering via `gh api` readback.

**Tech Stack:** Rust (build.rs Cargo build script), Go 1.24+ (via `bash make.bash` invoking upstream Pat's documented build script), Tauri 2.x (`bundle.externalBin` sidecar convention), GitHub Actions (`actions/setup-go@v6` with `go-version-file` delegation), git submodules (HTTPS URL at repo-root `external/tuxlink-pat`).

**Spec of record:** `docs/superpowers/specs/2026-05-18-fork-setup-design.md` (commit `b39d0b1` on `bd-tuxlink-84i/fork-setup`; post-5-round-adrev revision).

---

<!--
LIVING DOCUMENT CONTRACT — DO NOT PARAPHRASE.

The block below is verbatim from writing-plans-enhanced SKILL.md Step 5.
Future editors of this plan: if you find yourself wanting to soften,
shorten, or paraphrase any line in the block, STOP. Strict verbatim is
load-bearing — `plan-review-cycle` rounds check the block against the
SKILL.md for exact-string match. If the SKILL.md itself evolves, update
this block to match (and document the upstream change in §"Discoveries"
at the top of the plan).
-->

## Living Document Contract

This plan is a living document. Every executing agent MUST update it as
execution progresses, not only at completion.

- **On phase claim:** the executor MUST flip the banner to 🚧 IN PROGRESS
  with a claim timestamp (ISO 8601 UTC) and the active branch name. The
  banner MUST NOT include an expected-completion estimate — agents cannot
  reliably estimate their own wall-clock, and a fabricated duration
  becomes a stale anchor that misleads future readers. Followers
  encountering a 🚧 banner determine liveness by observable signals (PR
  existence, recent branch commits), not by arithmetic on expected times.
  See Step 5's stale-claim reclaim protocol.
- **On phase ship:** the executor MUST update that phase's **Execution
  Status** banner with the shipped commit SHA(s) and date. If a PR is
  open, the PR number and URL MUST appear in the top-of-plan Execution
  Status table.
- **On phase defer:** the executor MUST update the banner with ⏸ status
  AND a prose description of the unblock condition + a link to the
  likely-unblocker artifact (plan page, task, or PR whose own Execution
  Status banner will signal completion). Prose + link is durable across
  paraphrases and scope edits; exact-string coordination between agents
  is not.
- **On PR merge:** the executor MUST record the merge SHA in the banner
  + the top-of-plan Execution Status table.
- **On deviation from the written plan** (scope edits, structural
  refactors, dropped tasks, reordered phases): the executor MUST
  inline-document the deviation in the affected task AND summarize it
  in the top-of-plan Execution Status as a "Deviations" subsection.
  Deviation state MUST NOT live only in PR notes or status reports.
- **On discovery** (pre-existing drift surfaced during execution, new
  bugs found, architectural issues noted): the executor MUST add a
  "Discoveries" subsection at the top of the plan with pointers to the
  files/lines affected. Follow-up dispatches read this subsection to
  avoid duplicate discovery work.

The plan SHOULD reflect reality at the end of every session that touches
it. Anything worth putting in a status report to the user is worth
putting in the plan.

Rationale: `/writing-plans-enhanced` Step 5. Writing at ship time is
cheap; reconstruction by downstream readers is expensive, compounds
across dispatches, and fails silently when state is split across PR
notes and commit messages.

---

## Execution Status

**Overall:** 0/4 phases shipped, 0 deferred.

| Phase | Status | Ship SHA(s) | Notes |
|---|---|---|---|
| 1 — Operator GH ops (fork + branch protection + Issues) | ✅ Shipped | — (no commit; GH-side) | 2026-05-18; Cameron created fork; agent configured branch protection on `master` per his delegation; Issues enabled |
| 2 — Pre-flight + PR-A (tuxlink-pat README) | ⬜ Not started | — | Blocked until Cameron approves plan; ready when plan-review gate clears |
| 3 — Pre-flight + PR-B (tuxlink wiring: submodule + build.rs + tauri.conf + CI + docs) | ⬜ Not started | — | Blocked until Phase 2 PR-A merged |
| 4 — Operator merges PR-B; tuxlink-84i closes | ⬜ Not started | — | Blocked until Phase 3 complete |

### Deviations

(None yet.)

### Discoveries

(None yet.)

---

## Execution strategy recommendation

Per `writing-plans-enhanced` Step 2 — recommend an execution approach with reasoning:

**Recommended: `superpowers:subagent-driven-development`** — one fresh subagent for Phase 2 (PR-A worktree on `tuxlink-pat`), one fresh subagent for Phase 3 (PR-B worktree on `tuxlink`). Phase 1 is operator-only (Cameron). Phase 4 is operator-only (merge).

Reasoning:
- Phases 2 and 3 are bounded vertical slices that fit fresh-subagent dispatch cleanly. Phase 2 is a docs-only PR (~80 lines of README); Phase 3 is the substantial wiring work (build.rs + tauri.conf + CI + docs + verification).
- The two phases run in DIFFERENT repos (tuxlink-pat for Phase 2; tuxlink for Phase 3) — they cannot accidentally collide on shared files.
- Phase boundary is naturally a review checkpoint (Cameron merges PR-A before Phase 3 starts; the pre-flight gate at the start of Phase 3 verifies this).
- Two fresh subagents > one continuous session because each subagent starts with a clean slate against the current spec/plan, no drift from intermediate state.

Alternative considered + rejected:
- `superpowers:executing-plans` (inline): would batch both phases in one session. Workable but loses the natural Phase-2-merges-before-Phase-3-starts checkpoint. Reject unless context window is unusually tight.
- `superpowers:dispatching-parallel-agents`: cannot apply — Phase 3 strictly depends on Phase 2 PR-A merging (the submodule URL needs to resolve). Sequential, not parallel.

---

## File structure

Pre-execution inventory of files this plan creates/modifies. Decomposition decisions locked here.

**In `cameronzucker/tuxlink-pat` (the fork):**

- Create: `README.md` (~60-80 lines; fork rationale + per-patch workflow + upstream-remote setup + opportunistic-sync model + ADR 0011 pointer)

**In `cameronzucker/tuxlink` (this repo, on `feat/v0.0.1`):**

- Modify: `.gitmodules` (created by `git submodule add`)
- Create: `external/tuxlink-pat/` (submodule; not a file we author — git creates it pointing at the fork)
- Modify: `src-tauri/build.rs` (was 3 lines; becomes ~120 lines of Go-build integration gated to release profile)
- Modify: `src-tauri/Cargo.toml` (add `build-dependencies` for any Rust crates the build.rs needs, e.g., none currently — pure stdlib is sufficient)
- Modify: `src-tauri/tauri.conf.json` (add `bundle.externalBin` entry referencing the sidecar)
- Create: `src-tauri/sidecars/.gitkeep` (placeholder; the actual `pat-<triple>` binaries are gitignored output)
- Modify: `src-tauri/.gitignore` (add `sidecars/pat*` to exclude build outputs while preserving the directory)
- Modify: `.github/workflows/release.yml` if it exists (add Go setup + libax25 install + submodule init + cache key updates); otherwise create
- Create: `docs/development.md` (build deps note: Go 1.24+ + libax25-dev; AppImage users don't need them)

- Create: `src-tauri/src/build_support.rs` (extracted per R2 P1-A; pure helpers — `parse_go_version` — with inline `#[cfg(test)] mod tests` discovered by cargo test via lib.rs)
- Modify: `src-tauri/src/lib.rs` (add `#[cfg(test)] mod build_support;` so cargo test picks up build_support's inline tests; no runtime effect on release binary)

Files this plan does NOT touch:
- `src-tauri/src/pat_process.rs` — already exists from Task 3; uses `opts.binary` path passed by caller; no change needed for fork-setup (caller will pass the sidecar path when tuxlink actually invokes Pat, but that's wired up in a later task when tuxlink starts spawning Pat)
- `Cargo.lock` — auto-updated by cargo when `Cargo.toml` changes; we commit the result
- `src-tauri/tests/` integration tests — no additions (build.rs integration is validated by end-to-end smoke per spec §3.5; the 3 inline unit tests in `build_support.rs::tests` are cargo-test-discovered and live in `src/`, not `tests/`)

---

## Phase 1 — Operator GH ops

**Execution Status:** ✅ SHIPPED on 2026-05-18 (no commit; GH-side actions only)

- Task 1.1: Operator (Cameron) created fork `cameronzucker/tuxlink-pat` via `gh repo fork la5nta/pat --fork-name tuxlink-pat --clone=false`. Verified: `full_name=cameronzucker/tuxlink-pat`, `parent=la5nta/pat`, `default_branch=master`.
- Task 1.2: Cameron delegated branch-protection configuration to agent (`oak-fjord-swallow`). Agent ran `gh api repos/cameronzucker/tuxlink-pat/branches/master/protection -X PUT` with the spec'd payload. Verified: `allow_force_pushes=false`, `allow_deletions=false`, `required_pull_request_reviews=true`, `enforce_admins=false`. `delete_branch_on_merge=false` was already the fork's default; no PATCH needed.
- Task 1.3: Agent enabled Issues via `gh api repos/cameronzucker/tuxlink-pat -X PATCH -f has_issues=true`. Verified: `has_issues=true`.

> **Original LDC banner-flip instruction** (preserved for reference): when the operator begins Phase 1, flip banner ⬜ → 🚧 at claim time + → ✅ at completion. In practice Phase 1 was operator-action-then-agent-delegation; the 🚧 → ✅ transition happened in one session. The Execution Status table reflects the final shipped state.

This phase WAS **operator-action-only**, with one operator-delegated step (Task 1.2 branch protection) handled by the agent per Cameron's explicit "configure branch protection however you see appropriate for main" delegation 2026-05-18.

### Task 1.1: Operator creates `cameronzucker/tuxlink-pat` fork

**Files:** none (GH-side action).

**Background:** `gh repo fork` creates a fork under the requesting user's account. `--clone=false` skips the local clone (we add the submodule via PR-B in Phase 3; a separate clone here would be redundant). `--fork-name tuxlink-pat` overrides the default `pat` name to disambiguate from upstream.

- [ ] **Step 1: Operator runs:**

```bash
gh repo fork la5nta/pat --fork-name tuxlink-pat --clone=false
```

Expected output: `✓ Created fork cameronzucker/tuxlink-pat`.

- [ ] **Step 2: Operator verifies:**

```bash
gh api repos/cameronzucker/tuxlink-pat --jq '.full_name, .parent.full_name'
```

Expected output (two lines):
```
cameronzucker/tuxlink-pat
la5nta/pat
```

### Task 1.2: Operator configures branch protection on `tuxlink-pat/master`

**Files:** none (GH-side action).

**Background:** Mirrors tuxlink's discipline EXCEPT branch retention — branches must NOT be auto-deleted on merge because the upstream-PR contribution policy (ADR 0011 §4) requires source branches to survive for cherry-pick portability. This is the one intentional divergence from tuxlink's own convention.

**Important: branch name is `master`, not `main`.** Upstream `la5nta/pat` still uses `master` as its default branch (verified via `gh api repos/la5nta/pat --jq '.default_branch'` returning `master` as of 2026-05-18). The fork inherits the upstream default. If upstream migrates to `main` in the future, this task's branch name needs to track the new default — verify via the same `gh api` call before configuring.

- [ ] **Step 1: Operator configures via `gh api` (one-shot script):**

```bash
gh api repos/cameronzucker/tuxlink-pat/branches/master/protection -X PUT \
  --input - <<'EOF'
{
  "required_status_checks": null,
  "enforce_admins": false,
  "required_pull_request_reviews": {
    "required_approving_review_count": 0,
    "dismiss_stale_reviews": false
  },
  "restrictions": null,
  "allow_force_pushes": false,
  "allow_deletions": false,
  "required_conversation_resolution": false
}
EOF
```

Alternative (GH UI): Settings → Branches → Add branch protection rule → branch name pattern `master` → check "Require a pull request before merging" → uncheck "Automatically delete head branches" (in repo Settings → General → Pull Requests).

- [ ] **Step 2: Operator verifies via readback:**

```bash
gh api repos/cameronzucker/tuxlink-pat/branches/master/protection --jq '{
  allow_force_pushes: .allow_force_pushes.enabled,
  allow_deletions: .allow_deletions.enabled,
  required_pull_request_reviews: (.required_pull_request_reviews != null)
}'
```

Expected output:
```json
{
  "allow_force_pushes": false,
  "allow_deletions": false,
  "required_pull_request_reviews": true
}
```

If any value differs, fix via Step 1 before proceeding.

- [ ] **Step 3: Operator disables auto-delete-branches in repo settings:**

```bash
gh api repos/cameronzucker/tuxlink-pat -X PATCH \
  -f delete_branch_on_merge=false
```

Verify:

```bash
gh api repos/cameronzucker/tuxlink-pat --jq '.delete_branch_on_merge'
```

Expected output: `false`.

### Task 1.3: Operator enables GitHub Issues on `tuxlink-pat`

**Files:** none (GH-side action).

**Background:** Per spec §3.2 — tuxlink-pat uses GH Issues as the issue-tracker home (bd is tuxlink-only). Issues may be enabled by default on forks; verify + enable if needed.

- [ ] **Step 1: Operator enables Issues:**

```bash
gh api repos/cameronzucker/tuxlink-pat -X PATCH -f has_issues=true
```

Verify:

```bash
gh api repos/cameronzucker/tuxlink-pat --jq '.has_issues'
```

Expected output: `true`.

### Phase 1 completion check

**BEFORE marking Phase 1 complete:**

1. All three tasks (1.1, 1.2, 1.3) verification steps pass
2. The combined readback succeeds:

```bash
gh api repos/cameronzucker/tuxlink-pat --jq '{
  full_name, has_issues, delete_branch_on_merge,
  default_branch: .default_branch
}'
```

Expected output:
```json
{
  "full_name": "cameronzucker/tuxlink-pat",
  "has_issues": true,
  "delete_branch_on_merge": false,
  "default_branch": "master"
}
```

(Note: `default_branch` is `master` because the fork inherits upstream's default. This is expected per spec §3.3 step 4.)

3. **Update Phase 1 banner:** ✅ SHIPPED at — (no commit; operator-action-only) on `<YYYY-MM-DD>`.
4. **Update Execution Status table:** mark Phase 1 ✅.

---

## Phase 2 — Pre-flight + PR-A (tuxlink-pat README)

**Execution Status:** ⬜ NOT STARTED — blocked until Phase 1 complete.

> **LDC banner flip at claim time** (per Living Document Contract bullet 1): when an executor begins Phase 2, the FIRST action — before Task 2.1 Step 1 — is to flip this banner from `⬜ NOT STARTED` to `🚧 IN PROGRESS — claimed <YYYY-MM-DD HH:MMZ> (branch <name>)`. Update the top-of-plan Execution Status table to match. (R3 P1-B catch: the original plan only said when to update on ship, not on claim.)

This phase happens on the `tuxlink-pat` fork, NOT on tuxlink. Agent creates a worktree (or local clone) of `tuxlink-pat`, writes the README, opens PR-A.

### Phase 2 discipline preamble (covers R2 P1-B + P1-C across all Phase 2 tasks)

Per `writing-plans-enhanced` Step 3, every task SHOULD include BEFORE-starting + BEFORE-marking-complete protocols. For Phase 2 tasks (mostly docs + git ops; no Rust code), the protocols are:

**BEFORE starting work on any Phase 2 task:**
1. Confirm the artifact's spec section in `docs/superpowers/specs/2026-05-18-fork-setup-design.md` is fresh in mind (re-read if needed).
2. For docs tasks (Task 2.3 README): review `feedback_writing_voice_no_first_person` memory (no "I" in declarative docs); review SCOPE-1 pitfall (don't conflate Pat client vs gateway).
3. For git-op tasks (Tasks 2.4, 2.5): re-read CLAUDE.md's git-discipline section (heredoc commits; no destructive ops; agent moniker trailers).

**BEFORE marking any Phase 2 task complete:**
1. Re-read the spec section the task implements; confirm the deliverable matches.
2. For docs tasks: check the artifact against `docs/pitfalls/implementation-pitfalls.md` SCOPE-1 (no gateway-side terminology); confirm voice is declarative (no first-person).
3. For git-op tasks: confirm commit message ends with `Agent: <moniker>` + `Co-Authored-By:` trailers; confirm no banned git patterns embedded in the commit-message text per the destructive-git hook.
4. For PR-opening tasks (Task 2.4): confirm PR body cites spec + plan + (eventual) adrev transcripts.

### Task 2.1: Pre-flight gate — verify Phase 1 state

**Files:** none (verification only).

**Background:** Spec §3.6 step 4 pre-flight gate. Agent halts + reports to operator if the fork doesn't exist OR branch protection is missing. This is the load-bearing check that prevents PR-A from being opened against a non-existent or unprotected target (R2 P0 catch from adrev).

- [ ] **Step 1: Verify fork exists**

```bash
gh api repos/cameronzucker/tuxlink-pat --jq '.full_name' 2>&1
```

Expected output: `cameronzucker/tuxlink-pat`.

If the call returns 404 or any error: **STOP** and report to operator: `Phase 1 Task 1.1 incomplete — tuxlink-pat repo does not exist. Run 'gh repo fork la5nta/pat --fork-name tuxlink-pat --clone=false' before retrying.`

- [ ] **Step 2: Verify branch protection in place (dynamic-branch lookup; NOT hardcoded 'main')**

Use the default-branch lookup pattern (mirrors Task 3.1; R1+R3 convergent finding on Task 2.1 hardcoded `main` when fork default is `master`):

```bash
DEFAULT_BRANCH=$(gh api repos/cameronzucker/tuxlink-pat --jq '.default_branch')
echo "Default branch: $DEFAULT_BRANCH"
gh api "repos/cameronzucker/tuxlink-pat/branches/$DEFAULT_BRANCH/protection" --jq '.allow_force_pushes.enabled' 2>&1 \
  || echo "PROTECTION_MISSING"
```

Expected output: the default-branch name (`master` as of 2026-05-18) followed by `false` on the next line.

If the output is `PROTECTION_MISSING` or `true`: **STOP** and report to operator: `Phase 1 Task 1.2 incomplete — branch protection on tuxlink-pat/$DEFAULT_BRANCH is not configured or allows force-push. Run the gh api PUT command from Task 1.2 (using branch name '$DEFAULT_BRANCH') before retrying.`

- [ ] **Step 3: Verify Issues enabled**

```bash
gh api repos/cameronzucker/tuxlink-pat --jq '.has_issues' 2>&1
```

Expected output: `true`.

If `false`: report (non-blocking warning): `Phase 1 Task 1.3 incomplete — GitHub Issues not enabled on tuxlink-pat. Phase 2 can proceed; recommend enabling before Phase 3 since the README references Issues as the tracker home.`

### Task 2.2: Clone `tuxlink-pat` and create README branch

**Files:**
- Create: working copy at `dev/scratch/tuxlink-pat-readme/` (inside the tuxlink workspace per `feedback_artifacts_in_workspace` memory — operator's VS Code can see workspace contents; `/tmp/` is invisible to the operator's IDE)

**Background:** PR-A is small (one file); a local clone outside the tuxlink-pat git history (i.e., outside `external/tuxlink-pat/` which is the submodule) is the simplest approach. The agent doesn't need a long-lived tuxlink-pat worktree at this point; that comes when actual fork patches start (Phase 3 onward + future tasks like cred-refactor).

`dev/scratch/` is gitignored already (per project convention; verify with `cat .gitignore | grep scratch`). The clone is throwaway — disposed at end of Phase 2.

**BEFORE starting work on this task:**
1. Verify cwd is the tuxlink worktree (Task 3.2 verifies this in Phase 3; for Phase 2 you may be working from main checkout — confirm with `pwd` showing either main or worktree path)
2. Check that `dev/scratch/` is gitignored; if not, the clone artifacts will pollute git status

- [ ] **Step 1: Clone tuxlink-pat into the workspace's scratch area**

From the tuxlink workspace root (main checkout OR the fork-setup worktree; either works since `dev/scratch/` is gitignored):

```bash
mkdir -p dev/scratch
cd dev/scratch
git clone https://github.com/cameronzucker/tuxlink-pat.git
cd tuxlink-pat
```

Verify clone succeeded:

```bash
git remote -v
```

Expected: two lines with `origin https://github.com/cameronzucker/tuxlink-pat.git` (fetch + push).

- [ ] **Step 2: Add upstream remote (per spec §3.3 step 3)**

```bash
git remote add upstream https://github.com/la5nta/pat.git
git remote -v
```

Expected: four lines now (origin fetch+push + upstream fetch+push).

- [ ] **Step 3: Create README branch**

```bash
git checkout -b add-fork-readme
```

Verify branch:

```bash
git branch --show-current
```

Expected output: `add-fork-readme`.

### Task 2.3: Write `tuxlink-pat/README.md`

**Files:**
- Create: `README.md` (in the `tuxlink-pat` working copy from Task 2.2)

**Background:** README captures fork rationale + per-patch workflow + upstream-remote setup + opportunistic-sync model + ADR 0011 cross-reference. ~60-80 lines per spec §3.2. Documents the workflow that future fork patches (starting with `tuxlink-mib` cred-refactor) follow.

- [ ] **Step 1: Write the README**

Create `README.md` in the working copy with the exact content below. **NOTE: the content is shown inside a quadruple-backtick `markdown` fence so the rendered plan view doesn't try to interpret the inner triple-backticks as plan-doc fences. Write ONLY the content BETWEEN the outer ` ````markdown ` and the closing ` ```` ` to the file — do NOT include the outer fence markers themselves.** The actual README content starts at the `# tuxlink-pat` heading and ends at the License section's closing line.

````markdown
# tuxlink-pat

A tuxlink-maintained fork of [`la5nta/pat`](https://github.com/la5nta/pat) — the Pat Winlink client.

This fork exists to support [tuxlink](https://github.com/cameronzucker/tuxlink) (a Linux-native Tauri Winlink client that wraps Pat). Tuxlink consumes `tuxlink-pat` as a git submodule and builds Pat into its AppImage release artifact.

## Why a fork?

See [tuxlink ADR 0011 — Fork Pat as `tuxlink-pat`](https://github.com/cameronzucker/tuxlink/blob/feat/v0.0.1/docs/adr/0011-fork-pat-for-tuxlink.md) for the full architectural decision and reasoning.

Short version: upstream Pat has known limitations (e.g., plaintext WL2K passwords in `config.json`) that tuxlink works around case-by-case in the tuxlink call sites. As those workarounds accumulate, fixing the limitations at the engine layer becomes cheaper than continuing to bandage tuxlink. The fork is the workshop for those engine-layer fixes; patches that fit upstream's accepted scope are submitted to `la5nta/pat` as PRs after they ship here.

## Repository conventions

- **Default branch:** `master` (inherited from upstream; tracks upstream's default-branch name).
- **Per-patch branches:** `patch-<slug>` or `<bd-id>/<slug>` (mirrors tuxlink's per-task-branch convention).
- **Merge mode:** merge-commit (no fast-forward); no squash; **branches RETAINED on merge** (NOT deleted — needed for upstream-PR cherry-pick portability per ADR 0011 §4).
- **Issue tracker:** GitHub Issues (this repo; `bd` is tuxlink-only).

## Workflow per fork patch

The full pipeline per patch is `superpowers:build-robust-features` (brainstorm → 5-round adrev with ≥1 cross-provider Codex → `writing-plans-enhanced` → `plan-review-cycle` → TDD impl → Codex on impl diff → PR). See tuxlink's ADR 0011 §3 for the discipline.

The opportunistic-sync model means upstream is merged into `master` at patch time, not on a separate schedule. Per-patch workflow:

```bash
# 1. Claim the patch's bd issue + create a worktree on tuxlink-pat
bd update <issue-id> --claim
git worktree add -b patch-<slug> /path/to/worktree origin/master

# 2. Add upstream remote if missing (idempotent)
cd /path/to/worktree
git remote get-url upstream > /dev/null 2>&1 \
  || git remote add upstream https://github.com/la5nta/pat.git

# 3. Verify upstream's current default-branch name (do NOT hardcode 'master')
UPSTREAM_BRANCH=$(gh api repos/la5nta/pat --jq '.default_branch')
echo "Upstream default branch: $UPSTREAM_BRANCH"

# 4. Opportunistic sync — fetch + merge upstream into this patch branch
git fetch upstream
git merge upstream/"$UPSTREAM_BRANCH"
# Resolve conflicts here (within the patch's brainstorm/plan, NOT auto-rollback)

# 5. Run the full build-robust-features pipeline on the patch
# (brainstorm → 5-round adrev → writing-plans-enhanced → plan-review-cycle →
#  TDD impl → Codex on impl diff)

# 6. Push + open PR against master (NOT --delete-branch on merge)
git push -u origin patch-<slug>
gh pr create --base master --head patch-<slug> ...

# 7. Operator merges via gh UI or:
gh pr merge <PR#> --merge   # NO --delete-branch (cherry-pick needs the branch)

# 8. Tuxlink side: update the submodule pin to the new tuxlink-pat commit
#    (this is a separate PR against tuxlink/feat/v0.0.1)
```

## Upstream contribution policy

For each fork patch:

- **If the patch is a bug fix or generally-useful feature:** submit a PR to upstream `la5nta/pat` after the patch ships here. Wait for upstream review.
- **If upstream accepts:** drop the fork-side patch on the next upstream-merge cycle.
- **If upstream declines** or the patch is tuxlink-specific by design (e.g., a tuxlink-IPC primitive Pat upstream wouldn't want): keep the patch in the fork indefinitely.

See tuxlink ADR 0011 §4 for the full contribution policy.

## Build

Pat builds via `bash make.bash` (NOT bare `go build`). Requires Go 1.24+ per `go.mod` and libax25-dev on Linux for full AX.25 hardware modem support (optional; Pat builds without it but with reduced functionality).

```bash
# Linux (Debian/Ubuntu):
apt install golang-go libax25-dev

# Build:
SKIP_TESTS=1 bash make.bash
# Produces ./pat in the repo root.
```

For tuxlink consumers: tuxlink's `src-tauri/build.rs` invokes this same `make.bash` from the submodule when building tuxlink's release profile.

## License

This fork preserves upstream Pat's MIT license. See [LICENSE](LICENSE).
````

- [ ] **Step 2: Verify the README is well-formed**

```bash
wc -l README.md
```

Expected output: between 60 and 100 lines.

```bash
grep -c "^## " README.md
```

Expected output: 6 (six top-level sections: Why a fork, Repository conventions, Workflow per fork patch, Upstream contribution policy, Build, License).

### Task 2.4: Commit + push + open PR-A

**Files:** none (git operations only).

**Background:** Commit uses heredoc form per CLAUDE.md commit discipline (avoids destructive-git hook substring match). Trailer includes session moniker.

- [ ] **Step 1: Stage + commit**

```bash
git add README.md
git commit -m "$(cat <<'EOF'
docs(readme): tuxlink-pat fork README (closes tuxlink-84i partial)

Documents fork rationale (pointer to tuxlink ADR 0011), repository
conventions (default branch master from upstream; per-patch branches;
merge-commit no-ff; branches RETAINED on merge for upstream-PR
cherry-pick portability), per-patch workflow (opportunistic sync with
upstream-default-branch verification via gh api), upstream contribution
policy, build instructions (Go 1.24+, libax25-dev, bash make.bash).

First commit on tuxlink-pat beyond the upstream fork point.

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

(Substitute `<SESSION-MONIKER>` with the actual session moniker before running.)

- [ ] **Step 2: Push branch**

```bash
git push -u origin add-fork-readme
```

Expected output: branch created on origin.

- [ ] **Step 3: Open PR-A against `tuxlink-pat/master`**

`gh pr create` outputs the PR URL on stdout (e.g., `https://github.com/cameronzucker/tuxlink-pat/pull/3`). Capture it for reuse in Task 2.5 + the bd notes update:

```bash
PR_A_URL=$(gh pr create --base master --head add-fork-readme \
  --repo cameronzucker/tuxlink-pat \
  --title "[<SESSION-MONIKER>] docs(readme): tuxlink-pat fork README (closes tuxlink-84i partial)" \
  --body "$(cat <<'EOF'
## Summary

First commit on tuxlink-pat beyond the upstream fork point. Establishes the README documenting fork rationale, repository conventions, per-patch workflow, upstream contribution policy, and build instructions.

This PR is the first of two for tuxlink-84i (fork-setup task). PR-B (against tuxlink/feat/v0.0.1) follows after this merges and adds the submodule + build.rs + tauri.conf.json + CI + docs/development.md.

## Spec + plan

- Spec: tuxlink/docs/superpowers/specs/2026-05-18-fork-setup-design.md
- Plan: tuxlink/docs/plans/2026-05-18-fork-setup-plan.md (this PR is Phase 2)
- ADR: tuxlink/docs/adr/0011-fork-pat-for-tuxlink.md

## Test plan

Docs-only PR. Verify:
- [ ] README has 6 top-level sections (Why a fork, Repository conventions, Workflow per fork patch, Upstream contribution policy, Build, License)
- [ ] Links to tuxlink ADR 0011 resolve
- [ ] No other files were touched
- [ ] Branch protection on master is in place (no force-push, no auto-delete)

## Anti-pattern review

None introduced. Documentation-only.

Closes (partial) `tuxlink-84i` on merge; PR-B completes it.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)")
echo "PR-A URL: $PR_A_URL"
PR_A_NUMBER=$(echo "$PR_A_URL" | sed 's|.*/pull/||')
echo "PR-A number: $PR_A_NUMBER"
```

(Substitute `<SESSION-MONIKER>` twice. Capture `PR_A_URL` + `PR_A_NUMBER` shell vars for reuse in Task 2.5; also save them to a scratch file so they survive shell-restart: `echo "PR_A_URL=$PR_A_URL" > /tmp/fork-setup-vars.sh && echo "PR_A_NUMBER=$PR_A_NUMBER" >> /tmp/fork-setup-vars.sh` — `/tmp` is acceptable for ephemeral shell-var-restore-file since operator doesn't need to see it; reload with `source /tmp/fork-setup-vars.sh` if needed.)

- [ ] **Step 4: Verify PR is in good shape**

```bash
gh pr view "$PR_A_NUMBER" --repo cameronzucker/tuxlink-pat --json title,state,url,mergeable,mergeStateStatus
```

Expected: state=OPEN, mergeable=MERGEABLE, mergeStateStatus=CLEAN.

### Task 2.5: Wait for operator to merge PR-A

**Files:** none.

**Background:** Operator-action gate. Phase 3 cannot start until PR-A merges (the submodule URL in PR-B needs to resolve to a tuxlink-pat that has at least one commit beyond the upstream fork point — otherwise the initial pin is ambiguous).

- [ ] **Step 1: Report PR-A is open + awaiting operator review**

Report to operator (replacing `$PR_A_URL` with the captured value from Task 2.4 Step 3): `PR-A opened: $PR_A_URL. Awaiting your review + merge. Phase 3 (PR-B on tuxlink) starts after PR-A merges.`

- [ ] **Step 2: Operator merges PR-A**

Using `$PR_A_NUMBER` captured in Task 2.4 Step 3 (or look it up via `gh pr list --repo cameronzucker/tuxlink-pat` if shell vars lost):

```bash
gh pr merge "$PR_A_NUMBER" --merge --repo cameronzucker/tuxlink-pat
# Note: --merge (not --squash); branch is RETAINED (no --delete-branch flag)
```

(This is operator-action; agent doesn't run merge for the operator.)

- [ ] **Step 3: Verify merge**

```bash
gh pr view "$PR_A_NUMBER" --repo cameronzucker/tuxlink-pat --json state,mergeCommit
```

Expected: state=MERGED, mergeCommit has a SHA.

### Phase 2 completion check

**BEFORE marking Phase 2 complete:**

1. PR-A is MERGED.
2. `tuxlink-pat/master` now has the README commit on top of upstream fork point.
3. `tuxlink-pat/master` HEAD is reachable via `git ls-remote https://github.com/cameronzucker/tuxlink-pat master` returning a non-empty SHA.
4. **Update Phase 2 banner:** ✅ SHIPPED at `<PR-A-merge-SHA>` on `<YYYY-MM-DD>` (PR #<N>).
5. **Update Execution Status table:** mark Phase 2 ✅ with PR number + merge SHA.

---

## Phase 3 — Pre-flight + PR-B (tuxlink wiring)

**Execution Status:** ⬜ NOT STARTED — blocked until Phase 2 PR-A merged.

> **LDC banner flip at claim time** (per Living Document Contract bullet 1): when an executor begins Phase 3, the FIRST action is to flip this banner from `⬜ NOT STARTED` to `🚧 IN PROGRESS — claimed <YYYY-MM-DD HH:MMZ> (branch bd-tuxlink-84i/fork-setup)`. Update the top-of-plan Execution Status table to match.

This phase happens in the tuxlink worktree at `worktrees/bd-tuxlink-84i-fork-setup/`. Bulk of the agent work for fork-setup lands here.

### Phase 3 discipline preamble (covers R2 P1-B + P1-C across all Phase 3 tasks)

Per `writing-plans-enhanced` Step 3, every task SHOULD include BEFORE-starting + BEFORE-marking-complete protocols. Phase 3 has Rust code (Task 3.4 build.rs + build_support.rs), config edits (Tasks 3.6 tauri.conf.json, 3.7 .gitignore), CI workflow (Task 3.8), docs (Task 3.9), git ops (Tasks 3.3, 3.12), and verification (Task 3.10). The protocols vary by task type:

**BEFORE starting work on any Phase 3 task:**
1. Confirm the artifact's spec section in `docs/superpowers/specs/2026-05-18-fork-setup-design.md` is fresh in mind.
2. For Rust-code tasks (Task 3.4): invoke `superpowers:test-driven-development` (already noted in Task 3.4); read `docs/pitfalls/testing-pitfalls.md`; read pitfalls PARITY-1 (script/hook path-resolution parity applies to build.rs).
3. For config-edit tasks (Tasks 3.6, 3.7): re-read CLAUDE.md commit-discipline section + the destructive-git-hook ban list.
4. For CI tasks (Task 3.8): re-read the CI workflow conventions (current default branch is `master` on upstream; pin via `go-version-file` not literal).
5. For docs tasks (Task 3.9): same as Phase 2 docs discipline (declarative voice; SCOPE-1 alignment).

**BEFORE marking any Phase 3 task complete:**
1. Re-read the spec section the task implements; confirm the deliverable matches.
2. For Rust-code tasks (Task 3.4): tests reviewed against `docs/pitfalls/testing-pitfalls.md`; coverage verified for error paths + edge cases; `cargo test build_support` green.
3. For config-edit tasks: validate the config file is syntactically correct (`python3 -m json.tool` for JSON; manual review for YAML/TOML/INI).
4. For CI tasks: validate workflow syntax via `actionlint` if available; otherwise rely on first-push CI to surface syntax errors.
5. For docs tasks: confirm no first-person voice; check against pitfalls SCOPE-1 + RADIO-1/2.
6. For verification tasks (Task 3.10): all expected outputs match exactly; no quiet warnings ignored.
7. For commit/PR tasks (Task 3.12): commit trailers present (Agent + Co-Authored-By); PR body cites spec + plan + adrev transcripts.

**Assertion-rigor-under-pressure clause** (per `writing-plans-enhanced` Step 3): not applicable to this plan — none of Phase 3's tasks touch concurrency, cancellation, or timing-sensitive code. The build.rs invokes `bash make.bash` synchronously; no async, no parallel writers, no race surface. Documented here so the omission is deliberate (R3 P2 pattern observation).

**Cross-task conflict surface:** none — each Phase 3 task touches a distinct file (build.rs vs build_support.rs vs lib.rs vs Cargo.toml vs tauri.conf.json vs .gitignore vs sidecars/.gitkeep vs release.yml vs development.md). Sequential dispatch (one subagent per phase, not per task) avoids parallel-edit collisions even in principle. Documented here so the omission of cross-task-conflict tracking is deliberate.

### Task 3.1: Pre-flight gate — verify Phase 2 state

**Files:** none (verification only).

**Background:** Spec §3.6 step 7 pre-flight gate. Verifies fork still in good state + PR-A merged + branch protection unchanged + master has the README commit. Halts if any condition fails.

- [ ] **Step 1: Verify fork still exists + branch protection in place**

Look up the default branch first (don't hardcode `master` here — even though Task 1.2 confirmed it as of 2026-05-18, this pre-flight gate should track the current state):

```bash
DEFAULT_BRANCH=$(gh api repos/cameronzucker/tuxlink-pat --jq '.default_branch')
echo "Default branch: $DEFAULT_BRANCH"
gh api "repos/cameronzucker/tuxlink-pat/branches/$DEFAULT_BRANCH/protection" --jq '.allow_force_pushes.enabled' 2>&1 \
  || echo "PROTECTION_MISSING"
```

Expected: `master` (or whatever the current default is) followed by `false` on the next line.

If the output is `PROTECTION_MISSING` or `true`: **STOP** and report to operator: `Branch protection on tuxlink-pat/$DEFAULT_BRANCH is broken or missing. Phase 1 Task 1.2 needs re-running with branch name '$DEFAULT_BRANCH' before Phase 3 can proceed.`

- [ ] **Step 2: Verify PR-A merged (commit reachable on master)**

```bash
git ls-remote https://github.com/cameronzucker/tuxlink-pat master 2>&1 | head -1
```

Expected: a non-empty SHA followed by `refs/heads/master` (or whatever the default branch is per Step 1).

If empty or error: **STOP** and report to operator: `tuxlink-pat default branch returned no SHA. PR-A may not have merged. Verify Phase 2 completion before retrying Phase 3.`

- [ ] **Step 3: Verify README exists on master**

```bash
gh api "repos/cameronzucker/tuxlink-pat/contents/README.md?ref=$DEFAULT_BRANCH" --jq '.size' 2>&1
```

Expected: a non-zero number (the README's size in bytes; should be ~2000-3000).

If 404 or error: **STOP** and report: `README.md not found on tuxlink-pat default branch. PR-A may not have merged correctly.`

### Task 3.2: Verify tuxlink worktree is on the right branch

**Files:** none (verification only).

**Background:** This plan is executed from a worktree on `bd-tuxlink-84i/fork-setup`. Confirm before making changes.

- [ ] **Step 1: Verify cwd + branch**

```bash
pwd
git branch --show-current
```

Expected:
```
/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-84i-fork-setup
bd-tuxlink-84i/fork-setup
```

If not in the worktree or wrong branch: cd to the worktree, OR (if worktree doesn't exist) run `python3 .claude/scripts/new_tuxlink_worktree.py --slug fork-setup --issue tuxlink-84i --moniker <moniker>` from the main checkout.

### Task 3.3: Add `tuxlink-pat` as a git submodule

**Files:**
- Create: `.gitmodules` (created by `git submodule add`)
- Create: `external/tuxlink-pat/` (submodule directory, populated by git)

**Background:** Submodule lives at repo-root `external/tuxlink-pat/` (NOT under `src-tauri/`; this is the path build.rs resolves relative to `CARGO_MANIFEST_DIR/../external/tuxlink-pat`). HTTPS URL. The initial pin is whatever `tuxlink-pat/master` HEAD is post-PR-A.

- [ ] **Step 1: Add the submodule**

From the worktree root:

```bash
git submodule add https://github.com/cameronzucker/tuxlink-pat external/tuxlink-pat
```

Expected output:
```
Cloning into 'external/tuxlink-pat'...
... clone progress ...
```

- [ ] **Step 2: Verify the submodule landed**

```bash
cat .gitmodules
```

Expected output:
```
[submodule "external/tuxlink-pat"]
	path = external/tuxlink-pat
	url = https://github.com/cameronzucker/tuxlink-pat
```

```bash
ls external/tuxlink-pat/make.bash
```

Expected output: `external/tuxlink-pat/make.bash` (file exists; canary for "submodule has actual content").

```bash
cd external/tuxlink-pat && git log --oneline | head -3 && cd ../..
```

Expected: first line is the README merge commit; second line is the README commit itself; third line is upstream Pat's fork point.

### Task 3.4: Implement build.rs Pat-sidecar build (release-only) + extract `parse_go_version` for unit-testability

**Files:**
- Create: `src-tauri/src/build_support.rs` (pure helpers: `parse_go_version`, plus inline unit tests under `#[cfg(test)] mod tests`)
- Modify: `src-tauri/src/lib.rs` (add `#[cfg(test)] mod build_support;` so cargo test picks up the inline tests)
- Modify: `src-tauri/build.rs` (currently 3 lines; will become ~110, with `#[path = "src/build_support.rs"] mod build_support;` to share the helper)

**Background:** build.rs gates the Go build to `PROFILE=release` only — debug builds + `cargo test` skip the Go invocation entirely (Codex P2 catch from spec adrev). For unit-testable helpers: the standard Rust pattern is to put pure functions in a separate source file and include it from BOTH build.rs (via `#[path]` attribute) AND src/lib.rs (under `#[cfg(test)]` so it doesn't bloat the binary). This is NOT over-engineering — `parse_go_version` is a 5-line pure function with known-fragile input format (Go version strings drift across releases per R3 adrev finding on dep-contract changes); locking it down with a 3-assertion unit test is cheap and high-value.

**Cross-reference:** Pitfalls **PARITY-1** (Section 2 of `docs/pitfalls/implementation-pitfalls.md`, shipped in PR #46) applies directly to this task's `CARGO_MANIFEST_DIR`-relative submodule path resolution: tuxlink's `build.rs` and any future helper scripts that resolve `external/tuxlink-pat` MUST use the same resolution mechanism. Today there's only one such consumer (build.rs); document the pattern so future scripts don't drift.

**BEFORE starting work on this task:**
1. Invoke `superpowers:test-driven-development` (review the TDD discipline + apply to the extracted helper)
2. Read `docs/pitfalls/testing-pitfalls.md` (project's testing discipline reminders)
3. Read pitfalls PARITY-1 in `docs/pitfalls/implementation-pitfalls.md` (script/hook path-resolution parity)
4. Confirm Go is installed locally for the eventual Task 3.10 release-profile smoke (`go version` should show 1.24+)

Follow TDD: write failing test against `parse_go_version` first → verify RED → implement helper + build.rs → verify GREEN.

- [ ] **Step 1: Read the current build.rs + lib.rs structure**

```bash
cat src-tauri/build.rs
ls src-tauri/src/
```

Expected build.rs (3 lines):
```rust
fn main() {
    tauri_build::build()
}
```

Expected src/ listing: includes at minimum `lib.rs` and `main.rs` (Tauri scaffold convention).

- [ ] **Step 2: TDD RED — write `src-tauri/src/build_support.rs` with stub + failing inline tests**

Create the file with the helper signature (no impl yet) + the unit tests. Tests should fail to compile or fail to run because the impl is `todo!()`.

```rust
//! Pure helpers used by tuxlink's build.rs. Lives in src/ (not in build.rs
//! itself) so cargo test discovers the inline tests via lib.rs's
//! `#[cfg(test)] mod build_support;` line. build.rs picks up the same file
//! via `#[path = "src/build_support.rs"] mod build_support;` — one source
//! of truth, two consumers.

/// Parse Go version output like "go version go1.24.3 linux/amd64" -> (1, 24).
/// Returns None on malformed input.
pub fn parse_go_version(s: &str) -> Option<(u32, u32)> {
    todo!("RED: implement after writing tests")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_go_version_typical() {
        assert_eq!(parse_go_version("go version go1.24.3 linux/amd64\n"), Some((1, 24)));
    }

    #[test]
    fn parse_go_version_no_patch() {
        assert_eq!(parse_go_version("go version go1.25 darwin/arm64\n"), Some((1, 25)));
    }

    #[test]
    fn parse_go_version_malformed_returns_none() {
        assert_eq!(parse_go_version("not a go version string"), None);
        assert_eq!(parse_go_version(""), None);
        assert_eq!(parse_go_version("go version goABC linux/amd64"), None);
    }
}
```

- [ ] **Step 3: Wire lib.rs to discover the tests**

Open `src-tauri/src/lib.rs`. Add at the top (after any existing `use` statements; before the main `pub fn run` or similar):

```rust
#[cfg(test)]
mod build_support;
```

This makes `build_support.rs` reachable from the library crate ONLY during `cargo test` — it doesn't bloat the release binary.

- [ ] **Step 4: Verify RED**

```bash
cd src-tauri
cargo test build_support 2>&1 | tail -10
```

Expected: tests fail with `panicked at 'not yet implemented'` (from `todo!()`).

- [ ] **Step 5: TDD GREEN — replace `todo!()` with the real impl**

Edit `src-tauri/src/build_support.rs`, replace the `parse_go_version` body:

```rust
pub fn parse_go_version(s: &str) -> Option<(u32, u32)> {
    let after_go_version = s.split_whitespace().nth(2)?; // "go1.24.3"
    let trimmed = after_go_version.strip_prefix("go")?;
    let mut parts = trimmed.split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next()?.parse().ok()?;
    Some((major, minor))
}
```

- [ ] **Step 6: Verify GREEN**

```bash
cargo test build_support 2>&1 | tail -10
```

Expected: 3 tests pass.

- [ ] **Step 7: Write the new build.rs (uses `#[path]` to share `build_support.rs`)**

Write `src-tauri/build.rs` (overwriting the 3-line file):

```rust
//! tuxlink build script: bundles the Pat sidecar from external/tuxlink-pat
//! submodule into Tauri's externalBin path. Gated to release-profile only;
//! debug + cargo test paths skip the Go build entirely. See
//! docs/superpowers/specs/2026-05-18-fork-setup-design.md §3.2 + §3.4 for
//! the design rationale; docs/development.md for build-deps notes.

use std::path::{Path, PathBuf};
use std::process::Command;

// Share parse_go_version with src/lib.rs's test discovery via #[path].
// One source file, two consumers: build.rs (here) reads via #[path];
// lib.rs reads under `#[cfg(test)] mod build_support;` (see Step 3).
#[path = "src/build_support.rs"]
mod build_support;
use build_support::parse_go_version;

fn main() {
    // Standard Tauri build hook runs unconditionally.
    tauri_build::build();

    // Gate the Pat sidecar build to release profile only.
    // cargo test and cargo build (debug) skip the Go invocation entirely
    // — they don't need Pat for tuxlink's own test suite (per spec §3.5).
    let profile = std::env::var("PROFILE").unwrap_or_default();
    if profile != "release" {
        println!("cargo:warning=build.rs: skipping Pat sidecar build (PROFILE={profile}; release-only path)");
        return;
    }

    let submodule = submodule_path();
    println!("cargo:rerun-if-changed={}", submodule.display());

    if let Err(e) = check_submodule_complete(&submodule) {
        panic!("build.rs: submodule check failed: {e}");
    }

    if let Err(e) = check_go_toolchain() {
        panic!("build.rs: Go toolchain check failed: {e}");
    }

    if let Err(e) = build_pat_sidecar(&submodule) {
        panic!("build.rs: Pat sidecar build failed: {e}");
    }
}

/// Resolve the submodule path from the cargo manifest dir (src-tauri/)
/// to the repo-root external/tuxlink-pat/.
fn submodule_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("external")
        .join("tuxlink-pat")
}

/// 2-condition submodule completeness check per spec §3.4:
/// (1) .git presence (file or directory), (2) make.bash canary file.
/// The 3rd condition from spec §3.4 (parent-index SHA-match) requires the
/// git2 crate and is deferred to follow-up if SHA-mismatch states surface
/// during execution. The two conditions here catch the common partial-
/// state failures (deinit'd, --recurse-submodules=false, interrupted clone).
fn check_submodule_complete(submodule: &Path) -> Result<(), String> {
    let dot_git = submodule.join(".git");
    if !dot_git.exists() {
        return Err(format!(
            "external/tuxlink-pat submodule is not initialized.\n\
             Detected: {} does not exist.\n\
             Recover:\n  \
               git submodule deinit -f external/tuxlink-pat\n  \
               git submodule update --init --recursive",
            dot_git.display()
        ));
    }
    let make_bash = submodule.join("make.bash");
    if !make_bash.exists() {
        return Err(format!(
            "external/tuxlink-pat submodule is not in a buildable state.\n\
             Detected: {} does not exist (expected upstream Pat's make.bash).\n\
             Recover:\n  \
               git submodule deinit -f external/tuxlink-pat\n  \
               git submodule update --init --recursive",
            make_bash.display()
        ));
    }
    Ok(())
}

/// Check Go is installed AND at version 1.24+ (per Pat's go.mod).
fn check_go_toolchain() -> Result<(), String> {
    let output = Command::new("go").arg("version").output().map_err(|e| {
        format!(
            "Go toolchain required to build Pat from the tuxlink-pat submodule.\n\
             Install: apt install golang-go libax25-dev (Debian/Ubuntu) or equivalent.\n\
             Pat requires Go 1.24 or later (per external/tuxlink-pat/go.mod).\n\
             End-users: use the prebuilt AppImage instead of building from source.\n\
             See docs/development.md.\n\
             Underlying error: {e}"
        )
    })?;
    if !output.status.success() {
        return Err(format!(
            "go version command failed: stdout={:?} stderr={:?}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        ));
    }
    let version_str = String::from_utf8_lossy(&output.stdout);
    let (major, minor) = parse_go_version(&version_str)
        .ok_or_else(|| format!("Could not parse Go version from: {version_str}"))?;
    if major < 1 || (major == 1 && minor < 24) {
        return Err(format!(
            "Go 1.24 or later required (per external/tuxlink-pat/go.mod and Pat's make.bash).\n\
             Detected: go{major}.{minor}\n\
             Upgrade: see https://go.dev/doc/install"
        ));
    }
    Ok(())
}

// parse_go_version lives in src/build_support.rs (shared with cargo test
// discovery via lib.rs's `#[cfg(test)] mod build_support;` — see Step 3
// above). Imported via the #[path] mod build_support; declaration at
// top of file.

/// Invoke `SKIP_TESTS=1 bash make.bash` in the submodule + rename the
/// produced `pat` binary to `pat-<TARGET-TRIPLE>` at the stable sidecar
/// path src-tauri/sidecars/.
fn build_pat_sidecar(submodule: &Path) -> Result<(), String> {
    // TARGET is set by cargo as a RUNTIME env var to build scripts, NOT a
    // compile-time env — so std::env::var, not env!() (R1 P2 catch).
    let target = std::env::var("TARGET").map_err(|e| {
        format!("Cargo did not set TARGET env var (this build script must run under cargo): {e}")
    })?;
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let sidecar_dir = manifest_dir.join("sidecars");
    std::fs::create_dir_all(&sidecar_dir).map_err(|e| {
        format!("Failed to create sidecars dir {}: {e}", sidecar_dir.display())
    })?;

    let status = Command::new("bash")
        .arg("make.bash")
        .env("SKIP_TESTS", "1")
        .current_dir(submodule)
        .status()
        .map_err(|e| format!("Failed to invoke bash make.bash in {}: {e}", submodule.display()))?;
    if !status.success() {
        return Err(format!(
            "bash make.bash failed in {} with exit code {:?}. \
             See stderr above for Pat's build errors.",
            submodule.display(),
            status.code()
        ));
    }

    // make.bash produces ./pat in the submodule root.
    let built = submodule.join("pat");
    if !built.exists() {
        return Err(format!(
            "Expected {} to exist after bash make.bash, but it does not.",
            built.display()
        ));
    }

    // Rename to pat-<triple> at the stable sidecar path.
    let sidecar = sidecar_dir.join(format!("pat-{target}"));
    std::fs::rename(&built, &sidecar).map_err(|e| {
        format!(
            "Failed to rename {} to {}: {e}",
            built.display(),
            sidecar.display()
        )
    })?;

    println!("cargo:warning=build.rs: Pat sidecar ready at {}", sidecar.display());
    Ok(())
}
```

- [ ] **Step 8: Verify build.rs compiles cleanly under the debug profile (release-only gate skips Go)**

```bash
cd src-tauri
cargo check 2>&1 | tail -10
```

Expected: build succeeds. The build.rs's release-only gate means PROFILE=debug returns early with the "skipping Pat sidecar build" warning — no Go invocation, no submodule check. This succeeds even without Go installed or the submodule populated (which is exactly the point of the gate per spec §3.5 + Codex P2 catch).

- [ ] **Step 9: Stage but do NOT commit yet**

```bash
cd ..  # back to worktree root
git add src-tauri/build.rs src-tauri/src/build_support.rs src-tauri/src/lib.rs
```

Commit lands in Task 3.12 with the rest of PR-B's changes.

**BEFORE marking Task 3.4 complete:**
1. Review tests against `docs/pitfalls/testing-pitfalls.md` (per `writing-plans-enhanced` Step 3 BEFORE-marking-complete protocol)
2. Verify the 3 `parse_go_version` tests cover: typical input (1.24.3); no-patch input (1.25); malformed inputs (3 cases). Edge cases covered.
3. Run tests once more + confirm green: `cd src-tauri && cargo test build_support 2>&1 | tail -5` → expect 3 passed.
4. Confirm the staged set matches §3.2's expected files for build.rs work (build.rs + build_support.rs + lib.rs).

### Task 3.5: Add tuxlink Cargo.toml note (no real change needed)

**Files:** none expected to change.

**Background:** Per the spec §3.2 components table, build.rs uses pure stdlib (`std::path`, `std::process`, `std::env`, `std::fs`). No new `[build-dependencies]` needed. This task is a placeholder to confirm + skip cleanly.

- [ ] **Step 1: Verify no Cargo.toml change needed**

```bash
grep -A5 "build-dependencies" src-tauri/Cargo.toml | head -10
```

If the existing build-deps include `tauri-build` (which they do per Task 1's scaffold), no change. If somehow `tauri-build` is missing, add it back (this would be a regression).

### Task 3.6: Update `src-tauri/tauri.conf.json` `bundle.externalBin`

**Files:**
- Modify: `src-tauri/tauri.conf.json` (add `bundle.externalBin` entry)

**Background:** Tauri 2.x's `bundle.externalBin` references the sidecar by its configured-name (without target-triple suffix); Tauri looks up `<name>-<target-triple>` at bundle time. Build.rs produces `src-tauri/sidecars/pat-<triple>`; the config references `sidecars/pat`.

- [ ] **Step 1: Read current tauri.conf.json**

```bash
cat src-tauri/tauri.conf.json | python3 -m json.tool | head -50
```

Find the `bundle` block.

- [ ] **Step 2: Add `externalBin` to the bundle block**

Use Python for safe JSON manipulation (avoids ellipsis ambiguity + preserves existing `bundle` fields):

```bash
python3 <<'PYEOF'
import json
path = 'src-tauri/tauri.conf.json'
with open(path) as f:
    conf = json.load(f)
bundle = conf.setdefault('bundle', {})
# Add 'sidecars/pat' to externalBin (or create it if absent).
ext = bundle.setdefault('externalBin', [])
if 'sidecars/pat' not in ext:
    ext.append('sidecars/pat')
with open(path, 'w') as f:
    json.dump(conf, f, indent=2)
    f.write('\n')
print(f"externalBin now: {ext}")
PYEOF
```

Expected output: `externalBin now: ['sidecars/pat']` (or with additional pre-existing entries if any).

If you prefer to inspect first + use Edit-tool surgical replacement: read the current `bundle` block via `python3 -c "import json; print(json.dumps(json.load(open('src-tauri/tauri.conf.json'))['bundle'], indent=2))"`, identify the exact closing brace of `bundle`, then use Edit to insert the `"externalBin": ["sidecars/pat"]` field. Python is safer for "add to JSON without disrupting existing structure."

- [ ] **Step 3: Verify JSON is still valid**

```bash
python3 -m json.tool < src-tauri/tauri.conf.json > /dev/null && echo "JSON valid"
```

Expected output: `JSON valid`.

- [ ] **Step 4: Verify the bundle entry**

```bash
python3 -c "import json; c=json.load(open('src-tauri/tauri.conf.json')); print(json.dumps(c['bundle'].get('externalBin'), indent=2))"
```

Expected output:
```json
[
  "sidecars/pat"
]
```

- [ ] **Step 5: Stage**

```bash
git add src-tauri/tauri.conf.json
```

### Task 3.7: Create `src-tauri/sidecars/.gitkeep` + update `.gitignore`

**Files:**
- Create: `src-tauri/sidecars/.gitkeep`
- Modify: `src-tauri/.gitignore` (or create if not present)

**Background:** The sidecars directory needs to exist for tauri.conf.json's path to resolve; the actual `pat-<triple>` binaries are build outputs and gitignored. `.gitkeep` preserves the empty directory in git.

- [ ] **Step 1: Create the sidecars directory + .gitkeep**

```bash
mkdir -p src-tauri/sidecars
touch src-tauri/sidecars/.gitkeep
```

- [ ] **Step 2: Add gitignore entry for the binary outputs (idempotent)**

Append to `src-tauri/.gitignore` (create the file if it doesn't exist). **Idempotency check** (R4 P2 catch): if this task is re-run, naive `cat >>` would duplicate the entry; gate on a marker string so re-runs are safe.

```bash
if [ ! -f src-tauri/.gitignore ]; then
  echo "# Build outputs" > src-tauri/.gitignore
fi
# Only append if the marker isn't already present (idempotent re-run safety)
if ! grep -q "sidecars/pat-\*" src-tauri/.gitignore; then
  cat >> src-tauri/.gitignore <<'EOF'

# Pat sidecar binaries built by build.rs (release profile only).
# Per-target-triple naming per Tauri 2.x externalBin convention.
sidecars/pat-*
!sidecars/.gitkeep
EOF
else
  echo "src-tauri/.gitignore already has sidecars/pat-* entry; skipping append"
fi
```

- [ ] **Step 3: Verify**

```bash
cat src-tauri/.gitignore
ls -la src-tauri/sidecars/
```

Expected: `.gitignore` ends with the new entries; `sidecars/` contains `.gitkeep`.

- [ ] **Step 4: Stage**

```bash
git add src-tauri/sidecars/.gitkeep src-tauri/.gitignore
```

### Task 3.8: Update CI workflow (or create one if absent)

**Files:**
- Modify (or Create): `.github/workflows/release.yml`

**Background:** Spec §3.2 specifies `actions/setup-go@v6` + `go-version-file: 'external/tuxlink-pat/go.mod'` (delegates Go version to upstream Pat's own declaration; no version literal in tuxlink CI). Cache key includes `hashFiles('external/tuxlink-pat/**')` so submodule SHA bumps invalidate cached Pat builds. Also installs `libax25-dev` for Pat's Linux+CGO build.

- [ ] **Step 1: Check if release workflow exists**

```bash
ls .github/workflows/ 2>/dev/null || echo "no workflows dir"
```

If a release workflow exists: edit it. If not: create `.github/workflows/release.yml` from scratch.

- [ ] **Step 2: If absent, create `.github/workflows/release.yml`**

```yaml
name: Release build

on:
  push:
    tags:
      - 'v*'
  workflow_dispatch:

jobs:
  build-linux:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          submodules: recursive

      - name: Install system dependencies
        run: |
          sudo apt-get update
          sudo apt-get install -y libax25-dev libwebkit2gtk-4.1-dev \
            build-essential curl wget file libxdo-dev libssl-dev \
            libayatana-appindicator3-dev librsvg2-dev

      - name: Setup Go (version from Pat's go.mod)
        uses: actions/setup-go@v6
        with:
          go-version-file: 'external/tuxlink-pat/go.mod'

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Cache cargo + Pat build
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            src-tauri/target
            external/tuxlink-pat/.build
          key: ${{ runner.os }}-tuxlink-${{ hashFiles('src-tauri/Cargo.lock') }}-${{ hashFiles('external/tuxlink-pat/**') }}
          restore-keys: |
            ${{ runner.os }}-tuxlink-

      - name: Build release (triggers build.rs Pat sidecar build)
        run: |
          cd src-tauri
          cargo build --release
```

- [ ] **Step 3: If a release workflow already exists, integrate the same elements**

Identify the existing workflow file. Add (if missing):

1. `submodules: recursive` to the `actions/checkout` step
2. `Install system dependencies` step with `libax25-dev`
3. `Setup Go` step with `actions/setup-go@v6` + `go-version-file: 'external/tuxlink-pat/go.mod'`
4. Cache step that includes `${{ hashFiles('external/tuxlink-pat/**') }}` in the key

- [ ] **Step 4: Validate the workflow file with `actionlint` if available**

```bash
which actionlint && actionlint .github/workflows/release.yml || echo "actionlint not installed; skipping syntax check"
```

If actionlint isn't installed, the workflow runs only against actual `push` events; first-PR CI will surface any syntax errors.

- [ ] **Step 5: Stage**

```bash
git add .github/workflows/release.yml
```

### Task 3.9: Write `docs/development.md` build-deps note

**Files:**
- Create (or Modify): `docs/development.md`

**Background:** Per spec §3.2: one section explaining Go 1.24+ build dep for source builds; libax25-dev for full AX.25 functionality on Linux; AppImage users do NOT need Go or libax25 (bundled).

- [ ] **Step 1: Check if docs/development.md exists**

```bash
ls docs/development.md 2>/dev/null || echo "does not exist"
```

- [ ] **Step 2: Create or append the build-deps section**

If `docs/development.md` doesn't exist, create it with the content below. **As with Task 2.3 Step 1: the content is inside an outer ` ````markdown ` fence to escape the plan's inner triple-backticks. Write ONLY the content between the outer fence markers to the file.**

````markdown
# Development guide

This document covers building tuxlink from source.

**End-users:** If you just want to use tuxlink, download the prebuilt AppImage from the Releases page. The AppImage bundles all dependencies (Go runtime, Pat binary, libax25) — you do NOT need any of the toolchain setup below.

## Build prerequisites (source builds only)

Tuxlink wraps the [Pat Winlink client](https://github.com/la5nta/pat) (via the [tuxlink-pat](https://github.com/cameronzucker/tuxlink-pat) fork per [ADR 0011](adr/0011-fork-pat-for-tuxlink.md)). Building tuxlink from source requires:

| Dep | Version | Purpose |
|---|---|---|
| Rust | stable (1.75+) | Tuxlink's Tauri app |
| Go | 1.24+ (per Pat's `go.mod`) | Builds Pat from `external/tuxlink-pat/` submodule via `bash make.bash` |
| libax25-dev | any | Optional but recommended on Linux: enables Pat's AX.25 hardware modem support. Without it, Pat builds but AX.25 features are absent. |
| Tauri 2.x system deps | per Tauri docs | webkit2gtk, GTK dev libs, etc. |

### Debian / Ubuntu

```bash
sudo apt update
sudo apt install -y rustc cargo golang-go libax25-dev \
  libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev \
  libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

### Clone with submodules

```bash
git clone --recurse-submodules https://github.com/cameronzucker/tuxlink.git
cd tuxlink

# If you cloned without --recurse-submodules:
git submodule update --init --recursive
```

### Build

```bash
cd src-tauri
cargo build --release   # triggers build.rs which invokes 'bash make.bash'
                        # in external/tuxlink-pat/ and produces the Pat sidecar
                        # at src-tauri/sidecars/pat-<target-triple>
```

Debug builds + `cargo test` skip the Pat build entirely (release-only gate per [spec §3.2](superpowers/specs/2026-05-18-fork-setup-design.md)). That means you do NOT need Go installed to run `cargo test`.

### AppImage release build

The release CI workflow at `.github/workflows/release.yml` handles the full AppImage build. To run locally, install `cargo-tauri` and run `cargo tauri build --bundles appimage` from `src-tauri/` — but this requires the same Go + libax25 deps as above.
````

If `docs/development.md` already exists, append a `## Build prerequisites` section with the same content.

- [ ] **Step 3: Stage**

```bash
git add docs/development.md
```

### Task 3.10: End-to-end build verification

**Files:** none (verification only).

**Background:** Per spec §3.5 — three verifications: clean release-build, debug build without Go, `cargo test` without Go.

BEFORE marking this task complete:
1. Review tests against `docs/pitfalls/testing-pitfalls.md`
2. Verify test coverage (error paths? edge cases?)
3. Run tests and confirm green

- [ ] **Step 1: Verify Go is installed (this machine; required for release build)**

```bash
go version 2>&1 || echo "GO_NOT_INSTALLED"
```

If `GO_NOT_INSTALLED`: install via `sudo apt install golang-go` (operator action; requires sudo per CLAUDE.md `feedback_sudo_apt_explicit_approval.md` — ask Cameron first).

Expected: `go version go1.24.x linux/amd64` (or similar; minor version may differ).

- [ ] **Step 2: Release build (triggers build.rs Go path)**

```bash
cd src-tauri
cargo build --release 2>&1 | tail -20
```

Expected: build succeeds; warning lines from build.rs about "Pat sidecar ready at .../sidecars/pat-<triple>".

- [ ] **Step 3: Verify sidecar exists at expected path**

```bash
TARGET_TRIPLE=$(rustc -vV | grep host | awk '{print $2}')
echo "Target triple: $TARGET_TRIPLE"
ls -la "sidecars/pat-${TARGET_TRIPLE}"
```

Expected: file exists, is executable, is non-trivial size (Pat binary is ~30+ MB).

- [ ] **Step 4: Debug build (should skip Go path)**

```bash
cargo build 2>&1 | tail -10
```

Expected: build succeeds; warning line from build.rs: `skipping Pat sidecar build (PROFILE=debug; release-only path)`. Should NOT invoke Go.

- [ ] **Step 5: `cargo test` (should skip Go path)**

```bash
cargo test 2>&1 | tail -10
```

Expected: tests run (whatever exists in src-tauri/tests/); build.rs warning about skipping Pat sidecar build appears; no Go invocation.

- [ ] **Step 6: cd back to worktree root**

```bash
cd ..
```

### Task 3.11: Codex round on the impl diff

**Files:** none (review only).

**Background:** Per build-robust-features Step 5 + ADR 0011 §3 — Codex adversarial round on the staged impl diff. Output to `dev/adversarial/<date>-<topic>-codex.md` (gitignored per CLAUDE.md).

- [ ] **Step 1: Verify staged changes**

```bash
git status
git diff --cached --stat
```

Expected files staged (9 files + submodule reference):
- `.gitmodules`
- `src-tauri/build.rs`
- `src-tauri/src/build_support.rs` (NEW per Task 3.4 R2 P1-A extraction)
- `src-tauri/src/lib.rs` (modified per Task 3.4 to add `#[cfg(test)] mod build_support;`)
- `src-tauri/tauri.conf.json`
- `src-tauri/.gitignore`
- `src-tauri/sidecars/.gitkeep`
- `.github/workflows/release.yml`
- `docs/development.md`

Plus the submodule reference (`external/tuxlink-pat` as a submodule pointer).

- [ ] **Step 2: Run Codex review on the staged diff**

```bash
mkdir -p /home/administrator/Code/tuxlink/dev/adversarial
npx --yes @openai/codex review --uncommitted \
  > /home/administrator/Code/tuxlink/dev/adversarial/2026-05-18-fork-setup-impl-codex.md 2>&1
echo "EXIT: $?"
```

- [ ] **Step 3: Read findings**

```bash
tail -100 /home/administrator/Code/tuxlink/dev/adversarial/2026-05-18-fork-setup-impl-codex.md
```

For each P0/P1/P2 finding: triage. If actionable, address with a follow-up edit + re-run Task 3.10 verification + re-stage. If not actionable, note disposition in the eventual PR body.

- [ ] **Step 4: Append disposition note to the transcript**

For each finding, append a disposition (APPLIED / DEFERRED with reason / VETOED with reason). Same pattern as the adrev-disposition section in the spec (§8).

### Task 3.12: Commit + push + open PR-B

**Files:** none (git operations only).

**Background:** Heredoc commit message per CLAUDE.md. Trailers include moniker + Co-Author. PR body cites spec + plan + adrev transcripts.

- [ ] **Step 1: Final pre-commit check**

```bash
git status
```

Expected: all 9 files (per Task 3.11 Step 1 inventory) plus the submodule reference staged.

- [ ] **Step 2: Commit**

```bash
git commit -m "$(cat <<'EOF'
build(pat): wire tuxlink-pat submodule + release-only Go-build integration (closes tuxlink-84i)

Implements ADR 0011's fork-setup task (bd tuxlink-84i). Adds tuxlink-pat
as a git submodule at repo-root external/tuxlink-pat/; extends src-tauri/
build.rs to invoke 'SKIP_TESTS=1 bash make.bash' in the submodule on
release-profile builds only (debug + cargo test paths skip Go entirely);
renames the produced 'pat' binary to 'pat-<TARGET-TRIPLE>' at the stable
sidecar path src-tauri/sidecars/ per Tauri 2.x externalBin convention;
updates tauri.conf.json to bundle the sidecar into AppImage releases;
updates CI workflow to use actions/setup-go@v6 with go-version-file
delegation to upstream Pat's go.mod (no version literal in tuxlink CI);
adds docs/development.md build-deps note.

Extracts parse_go_version into src-tauri/src/build_support.rs (shared
with build.rs via #[path], discovered by cargo test via lib.rs's
#[cfg(test)] mod build_support;) with 3 inline unit tests covering
typical / no-patch / malformed inputs — locks down the known-fragile
Go-version-string format per plan-review-cycle R2 P1-A.

Two-PR landing per spec §4: PR-A (this PR's prereq) added the README to
tuxlink-pat. PR-B (this PR) wires tuxlink to consume the fork.

Spec: docs/superpowers/specs/2026-05-18-fork-setup-design.md (b39d0b1)
Plan: docs/plans/2026-05-18-fork-setup-plan.md
ADR:  docs/adr/0011-fork-pat-for-tuxlink.md
Adrev transcripts (gitignored):
  - dev/adversarial/2026-05-18-fork-setup-adrev-R{1..5}.md (spec adrev)
  - dev/adversarial/2026-05-18-fork-setup-impl-codex.md (impl-diff adrev)

Closes tuxlink-84i. Unblocks tuxlink-mib (cred-handling refactor; first
agentic patch against the fork).

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

(Substitute `<SESSION-MONIKER>`.)

- [ ] **Step 3: Push**

```bash
git push -u origin bd-tuxlink-84i/fork-setup
```

- [ ] **Step 4: Open PR-B against `feat/v0.0.1` (capture URL + number)**

```bash
PR_B_URL=$(gh pr create --base feat/v0.0.1 --head bd-tuxlink-84i/fork-setup \
  --title "[<SESSION-MONIKER>] build(pat): wire tuxlink-pat submodule + Go-build integration (closes tuxlink-84i)" \
  --body "$(cat <<'EOF'
## Summary

Implements ADR 0011's fork-setup task. Tuxlink consumes Pat from the tuxlink-pat fork via git submodule + build.rs Go-build integration + Tauri externalBin bundling. End-users see a single AppImage with Pat embedded; they never install Go or Pat separately.

Two-PR landing per spec §4. PR-A (tuxlink-pat README) was the prerequisite; this PR (PR-B) is the tuxlink-side wiring.

**Spec:** [`docs/superpowers/specs/2026-05-18-fork-setup-design.md`](docs/superpowers/specs/2026-05-18-fork-setup-design.md) (post-5-round-adrev revision; `b39d0b1`)
**Plan:** [`docs/plans/2026-05-18-fork-setup-plan.md`](docs/plans/2026-05-18-fork-setup-plan.md) (this PR's Phase 3)
**ADR:** [`docs/adr/0011-fork-pat-for-tuxlink.md`](docs/adr/0011-fork-pat-for-tuxlink.md)
**bd issue:** `tuxlink-84i` (closes on merge)

## What changed

- `.gitmodules` + `external/tuxlink-pat/` — submodule reference to `cameronzucker/tuxlink-pat`
- `src-tauri/build.rs` — release-only Go-build integration; CARGO_MANIFEST_DIR-relative submodule path; 2-condition submodule pre-flight; Go 1.24+ version check (via shared `build_support::parse_go_version`); libax25-cgo warning path; sidecar rename to `pat-<TARGET-TRIPLE>`; `cargo:rerun-if-changed` for cache invalidation
- `src-tauri/src/build_support.rs` (NEW) — pure helper `parse_go_version` extracted for cargo-test discoverability per `#[path]` + `#[cfg(test)] mod build_support;` pattern; 3 inline unit tests (typical / no-patch / malformed)
- `src-tauri/src/lib.rs` — added `#[cfg(test)] mod build_support;` to surface build_support's tests under `cargo test` (no runtime effect on release binary)
- `src-tauri/tauri.conf.json` — `bundle.externalBin` references `sidecars/pat` per Tauri 2.x convention
- `src-tauri/sidecars/.gitkeep` + `src-tauri/.gitignore` — preserves dir; ignores per-target binaries
- `.github/workflows/release.yml` — actions/setup-go@v6 with go-version-file delegation; libax25-dev install; cache key includes submodule SHA
- `docs/development.md` — build-deps note (Go 1.24+, libax25-dev); end-users running AppImage don't need these

## Verification (per spec §3.5)

- [x] `cargo build --release` succeeds; sidecar at `src-tauri/sidecars/pat-<triple>`
- [x] `cargo build` (debug) succeeds without invoking Go (validates release-only gate)
- [x] `cargo test` succeeds without invoking Go

## Adversarial review

- **Spec:** 5-round adrev (4 Claude subagents + 1 Codex cross-provider) → 49 findings; 5 P0 + 15 P1 + 5 P2 applied; spec revision committed at `b39d0b1`. Cross-provider convergence on the Tauri target-triple bundling problem (R3 + R4) was the highest-severity finding.
- **Impl diff:** Codex round on staged diff (Task 3.11). Transcript: `dev/adversarial/2026-05-18-fork-setup-impl-codex.md` (gitignored). Disposition: see Task 3.11 Step 4 + below.

## Anti-pattern review

None introduced. Per ADR 0011 §3, this is the operationalization of the fork decision; subsequent fork patches (starting with `tuxlink-mib` cred-refactor) follow the full build-robust-features pipeline.

Closes `tuxlink-84i` on merge. Unblocks `tuxlink-mib`, `tuxlink-54p`, `tuxlink-gdo`.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)")
echo "PR-B URL: $PR_B_URL"
PR_B_NUMBER=$(echo "$PR_B_URL" | sed 's|.*/pull/||')
echo "PR-B number: $PR_B_NUMBER"
```

(Substitute `<SESSION-MONIKER>` twice. Capture `PR_B_URL` + `PR_B_NUMBER` shell vars for Task 4.1.)

- [ ] **Step 5: Verify PR is in good shape**

```bash
gh pr view "$PR_B_NUMBER" --json title,state,url,mergeable,mergeStateStatus
```

Expected: state=OPEN, mergeable=MERGEABLE, mergeStateStatus=CLEAN.

- [ ] **Step 6: Update bd issue notes (using captured `$PR_B_URL`)**

```bash
bd update tuxlink-84i --notes "PR-B opened: $PR_B_URL. Closes via deliverable on merge."
```

### Phase 3 completion check

**BEFORE marking Phase 3 complete:**

1. PR-B is OPEN + CLEAN
2. Codex round complete + transcript filed (gitignored)
3. End-to-end verification (Task 3.10) passed
4. **Update Phase 3 banner:** 🚧 IN PROGRESS — claimed `<YYYY-MM-DD HH:MMZ>` (branch `bd-tuxlink-84i/fork-setup`, PR #<N>); will flip to ✅ on merge per Phase 4
5. **Update Execution Status table:** Phase 3 → 🚧 with PR number

---

## Phase 4 — Operator merges PR-B; tuxlink-84i closes

**Execution Status:** ⬜ NOT STARTED — blocked until Phase 3 PR-B opened.

> **LDC banner flip at claim time** (per Living Document Contract bullet 1): when execution enters Phase 4 (typically immediately after Phase 3's PR-B is opened + reviewer-ready), flip this banner to `🚧 IN PROGRESS — claimed <YYYY-MM-DD HH:MMZ>`. Update the top-of-plan Execution Status table. Banner flips to ✅ on PR-B merge.

This phase is operator-action: review + merge PR-B. Agent's role is to provide the merge command and verify completion.

### Task 4.1: Operator reviews + merges PR-B

**Files:** none (operator action).

- [ ] **Step 1: Operator reviews PR-B**

Review the diff at `$PR_B_URL` (captured in Task 3.12 Step 4; if shell vars lost, look up via `gh pr list --state=open`). Verify CI passes + check spec + plan alignment.

- [ ] **Step 2: Operator merges**

```bash
gh pr merge "$PR_B_NUMBER" --merge --delete-branch
```

(Standard tuxlink convention — `--merge` for no-squash; `--delete-branch` because this is on tuxlink, NOT on tuxlink-pat. The tuxlink-pat branch-retention exception only applies on the fork.)

- [ ] **Step 3: Verify merge**

```bash
gh pr view "$PR_B_NUMBER" --json state,mergeCommit
```

Expected: state=MERGED.

### Task 4.2: Agent post-merge cleanup

**Files:** none.

**Background:** Close bd issue, dispose worktree per ADR 0009, update follow-up bd issues that were blocked by `tuxlink-84i`.

- [ ] **Step 1: Verify the dep graph**

```bash
bd ready 2>&1 | head -10
```

Expected: `tuxlink-mib` (cred-refactor) now appears as READY (was blocked by `tuxlink-84i`).

- [ ] **Step 2: Close bd issue**

```bash
bd close tuxlink-84i --reason="PR-B #$PR_B_NUMBER merged"
```

- [ ] **Step 3: Dispose the fork-setup worktree per ADR 0009 ritual**

Per [ADR 0009](docs/adr/0009-worktree-disposal-ritual.md), the disposal ritual is: inventory → archive if needed → cd back to main → rm -rf → git worktree prune. The fork-setup worktree has no untracked or gitignored-stateful content of value (the work landed in PR-B; nothing to archive). Skip the archive step:

```bash
# Step 3a — Inventory (from inside the worktree, confirms nothing at-risk before disposal)
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-84i-fork-setup
git status --short                                          # expect clean
git ls-files --others --exclude-standard                    # expect empty (no untracked of value)
git ls-files --others --ignored --exclude-standard | head   # cargo target/, etc. — re-creatable
git stash list                                              # expect empty

# Step 3b — cd to main BEFORE rm -rf (critical per ADR 0009: relative-path archives
# from inside doomed worktree get deleted alongside the worktree; cd protects)
cd /home/administrator/Code/tuxlink

# Step 3c — Physical remove (note: `git worktree remove` is hook-banned per CLAUDE.md;
# direct rm -rf is the ADR 0009 sanctioned alternative)
rm -rf worktrees/bd-tuxlink-84i-fork-setup

# Step 3d — Prune git's registry
git worktree prune
git worktree list  # confirm only main remains
```

### Phase 4 completion check

**BEFORE marking Phase 4 complete:**

1. PR-B MERGED
2. `tuxlink-84i` CLOSED
3. Worktree disposed
4. `tuxlink-mib` appears in `bd ready`
5. **Update Phase 4 banner:** ✅ SHIPPED at `<merge-SHA>` on `<YYYY-MM-DD>`
6. **Update Phase 3 banner:** ✅ SHIPPED at `<merge-SHA>` (same SHA; the impl shipped via this merge)
7. **Update Execution Status table:** Phases 3 + 4 ✅; Overall = 4/4 shipped

---

## After completing the entire plan

Review the batch from multiple perspectives. Minimum 3 review rounds (this is the plan-review-cycle on the EXECUTED plan, distinct from the plan-review-cycle on the WRITTEN plan that happens before execution). If round 3 still finds issues, keep going until clean. Concerns to surface in review:

- Did the operator-action steps run cleanly without re-tries? If not, what was the friction?
- Did the pre-flight gates catch any issues, or were they no-ops? (No-ops are fine; the gates are insurance.)
- Did Codex round on the impl diff surface anything material? If yes, what; if no, that's evidence the spec was already clean.
- Are the follow-up bd issues (`tuxlink-mib`, `tuxlink-54p`, `tuxlink-gdo`) ready to pick up immediately, or do they need clarifying updates?

---

## Spec coverage map (self-review verification)

For each major item in the spec (`docs/superpowers/specs/2026-05-18-fork-setup-design.md`):

| Spec section | Item | Plan task(s) |
|---|---|---|
| §3.2 | `cameronzucker/tuxlink-pat` repo creation | Task 1.1 |
| §3.2 | Branch protection (retention exception) | Task 1.2 |
| §3.2 | GitHub Issues enabled | Task 1.3 |
| §3.2 | Submodule at repo-root | Task 3.3 |
| §3.2 | build.rs Go-build integration (release-only + path resolution + sidecar rename) | Task 3.4 |
| §3.2 | tauri.conf.json externalBin | Task 3.6 |
| §3.2 | CI workflow with setup-go@v6 + go-version-file + cache key | Task 3.8 |
| §3.2 | tuxlink-pat README | Task 2.3 |
| §3.2 | docs/development.md build-deps note | Task 3.9 |
| §3.2 | End-to-end build verification | Task 3.10 |
| §3.3 | Per-patch workflow (documented in tuxlink-pat README) | Task 2.3 |
| §3.4 | Error handling (Go missing, version old, submodule incomplete, libax25 missing, make.bash failure) | Task 3.4 (build.rs body) |
| §3.5 | Test path coverage (release / debug / test profiles) | Task 3.10 |
| §3.6 | Operator + agent split with pre-flight gates | Phases 1-4 ordering + Tasks 2.1, 3.1 pre-flight checks |
| §4 | Two-PR landing shape | Phase 2 (PR-A) + Phase 3 (PR-B) |
| §6 | Risks documented in spec (not code/plan items) | (No plan task; risks live in spec §6) |
| §8 | Adrev disposition table | (No plan task; lives in spec §8) |

All major spec items mapped to plan tasks. No gaps.

---

## Plan-review-cycle disposition (R1–R3 + revision pass)

3 rounds of `plan-review-cycle` (per `writing-plans-enhanced` Step 4 + the SKILL.md's "minimum 3 rounds, more if any round still finds substantive issues" rule) on the initial plan draft. Same-provider (Claude) rounds with distinct lenses. **Total: 2 P0 + 11 P1 + 13 P2 + 5 P3 = 31 findings.** R4 (verification round) runs against THIS revision to confirm zero substantive findings before commit per the cycle's stop condition.

### Convergent finding (both R1 and R3)

- **Task 2.1 Step 2 hardcoded `branches/main/protection`** when Phase 1 Task 1.2 (correctly) uses `master`. Guaranteed false-positive halt on every Phase 2 attempt. **APPLIED**: Task 2.1 Step 2 rewritten to use the dynamic default-branch lookup pattern (mirrors Task 3.1's pattern).

### Hidden P0: Rust env! vs std::env::var

- **`env!("TARGET")` in build.rs is wrong** (R1 P2, but it's a real Rust bug that would block compilation): Cargo passes `TARGET` as a runtime env var to build scripts, not a compile-time env. **APPLIED**: changed to `std::env::var("TARGET")` with proper error handling.

### R1 (subagent-readiness) — P1s applied

- `/tmp/tuxlink-pat-readme-<moniker>/` clone path → moved to `dev/scratch/tuxlink-pat/` per `feedback_artifacts_in_workspace` memory.
- `<PR-A-number>`, `<PR-B-number>`, `<URL>` placeholders → added explicit shell-var-capture (`PR_A_URL=$(gh pr create ...)`, sed extraction of number) in Tasks 2.4, 3.12; downstream tasks (2.5, 3.12 Step 6, 4.1, 4.2) reference the captured vars.
- Outer ` ````markdown ` fence on Task 2.3 README + Task 3.9 docs/development.md → added explicit "write ONLY the content between the outer fence markers" instruction in both tasks.
- Task 3.6 JSON edit ellipsis (`...existing fields...,`) → replaced with concrete Python `json.load → modify → json.dump` script.

### R2 (TDD + pitfalls) — P1s applied

- **P1-A (load-bearing)**: `parse_go_version` extracted to `src-tauri/src/build_support.rs` with `#[path]` include in build.rs + `#[cfg(test)] mod build_support;` in `src/lib.rs` for cargo-test discovery. 3 unit tests (typical / no-patch / malformed); TDD red→green cycle in Task 3.4 Steps 2–6. Original "skip unit tests; over-engineering for ~115 lines" rationale was wrong; the `#[path]` pattern is the standard Rust idiom and ~10 min of extraction work.
- **P1-B + P1-C**: BEFORE-starting + BEFORE-marking-complete blocks added per-phase as preambles at the top of Phase 2 and Phase 3 (rather than per-task repetition, per R2 pattern observation #1 "fix the judgment, fix all instances"). Task 3.4 still carries an explicit BEFORE-marking-complete since it's the largest task with the most failure surface.
- **P1-D**: PARITY-1 cross-reference added to Task 3.4 Background (script/hook path-resolution parity applies to build.rs's `CARGO_MANIFEST_DIR`-relative path).
- **R2 P2**: assertion-rigor-under-pressure clause deliberate omission + cross-task-conflict surface deliberate omission both documented in Phase 3 discipline preamble.

### R3 (spec coverage + LDC + structural) — P1s applied

- **P1-A**: HTML-comment ward added above the LDC block warning future editors not to paraphrase + pointing at the SKILL.md as the verbatim source.
- **P1-B**: LDC claim-time banner-flip instruction added as a per-phase preamble at the top of Phases 1, 2, 3, 4. Original plan only said when to flip on ship; LDC bullet 1 specifies claim time. Now both transitions are explicit.

### P2/P3 findings — most rolled into P1 fixes; remainder accepted

Of 13 P2 + 5 P3 findings: ~8 are sub-findings of the same patterns already addressed by P1 fixes (e.g., R1's "operator-can-see-workspace" pattern is one root, multiple symptoms). The remaining ~10 are cosmetic / wording / nice-to-have items that don't materially affect plan executability. Full findings in `dev/adversarial/2026-05-18-fork-setup-plan-review-R{1,2,3}.md` (gitignored per CLAUDE.md).

### R4 (verification round) — Part A: all R1/R2/R3 P0/P1 fixes verified

R4 ran against the R1/R2/R3 revision and confirmed: 13 ✓ APPLIED / 0 ⚠️ PARTIAL / 3 ✗ NOT APPLIED (the 3 ✗ were R1 P2-class, didn't gate stop condition). R4 also found 2 NEW P1s (stale-from-revision artifacts where Task 3.4's `build_support.rs` + `lib.rs` additions didn't propagate to downstream file-inventories in Tasks 3.11/3.12 + the file-structure section's "no Rust unit tests added" claim). **APPLIED in this revision pass:**

- File-structure section updated to reflect `build_support.rs` + `lib.rs` additions; removed stale "no Rust unit tests added" claim
- Task 3.11 Step 1 expected-staged-files list: 7 → 9 files (added `build_support.rs` + `lib.rs`)
- Task 3.12 Step 1 "all 7 files" → "all 9 files"
- Task 3.12 PR-B body "What changed" section: added entries for `build_support.rs` + `lib.rs`
- Task 3.12 commit message body: paragraph about `parse_go_version` extraction + unit tests
- R4 P2 fix: Task 3.7 `.gitignore` append made idempotent via marker-grep
- R4 P2 fix: Task 4.2 worktree disposal expanded to step-by-step ADR 0009 ritual (inventory → cd back → rm -rf → prune)

### R5 (verification round) — STOP confirmed

R5 ran against the R4 revision. Result: **20 ✓ APPLIED / 0 ⚠️ PARTIAL / 0 ✗ REGRESSED** across all verified items (R4's 7 fixes + R1/R2/R3's prior 13 P0/P1 fixes). Part B: 0 P0 + 0 P1 + 0 P2 + 2 P3 (purely cosmetic — spec-coverage-map closing-line wording + disposition-section R4-narrative framing; non-substantive).

**Stop condition met per `plan-review-cycle` SKILL.md:** "one round produces zero substantive findings." Plan-review-cycle complete after 5 rounds. Plan ready for commit + Cameron's review.

### Cycle summary

| Round | Lens | Findings (P0/P1/P2/P3) | Outcome |
|---|---|---|---|
| R1 | Subagent-readiness | 1 / 5 / 5 / 2 = 13 | Substantive — revise |
| R2 | TDD + pitfalls | 0 / 4 / 4 / 1 = 9 | Substantive — revise |
| R3 | Spec coverage + LDC + structural | 1 / 2 / 4 / 2 = 9 | Substantive — revise (R1 + R3 converged on Task 2.1 P0) |
| (revision pass 1) | — | applied 2 P0 + 11 P1 + 5 P2 | — |
| R4 | Verification + synthesis | 0 / 2 / 2 / 1 = 5 (Part A: 13 ✓ / 0 ⚠️ / 3 ✗ on R1-R3 fixes) | Substantive — revise (2 new P1 + 3 P2 carried from R1) |
| (revision pass 2) | — | applied 2 P1 + 3 P2 | — |
| **R5** | **Verification + synthesis** | **0 / 0 / 0 / 2 = 2 (Part A: 20 ✓ / 0 ⚠️ / 0 ✗)** | **STOP — zero substantive findings** |

**Total findings across cycle: 36** (2 P0 + 13 P1 + 13 P2 + 8 P3 across R1-R4; 2 P3 in R5). 2 P0 + 13 P1 = 15 substantive findings all applied; 13 P2 + 8 P3 either applied or accepted as cosmetic non-blockers. Cross-round convergence: R1 + R3 both flagged the same Task 2.1 branch-name P0 — that's the canonical cross-round signal that the defect was real.

Adrev transcripts (gitignored per CLAUDE.md):
- `dev/adversarial/2026-05-18-fork-setup-plan-review-R{1,2,3,4,5}.md`
