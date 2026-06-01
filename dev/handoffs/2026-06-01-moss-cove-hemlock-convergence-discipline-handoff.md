# Handoff: 2026-06-01 — convergence-discipline handoff — moss-cove-hemlock

**Agent:** moss-cove-hemlock (this session) → next agent (autonomous ~10h window, operator AFK at $DAYJOB)
**Session shape:** Full day. Started as the overnight HTML Forms backlog executor (items A–N, all shipped); evolved into a deep forensic + design session driven by the operator's discovery that "consolidated builds" never converged. The current handoff is the post-forensic state: convergence-discipline scoped, Codex-reviewed, sub-issues filed; **implementation work starts with this handoff.**

## TL;DR for the next agent

You are picking up the implementation of `tuxlink-edvb` — the umbrella convergence-discipline issue. The design is **Codex-reviewed** (3300-line transcript at `dev/adversarial/2026-06-01-convergence-discipline-codex.md` in THIS worktree; mandatory pre-read). Six sub-issues are filed (P1: `qepd`, `21j8`; P2: `8d7y`, `pxmi`, `ui3i`; P3: `8zho`).

**Your first deliverable** is `tuxlink-qepd` — the v1 `scripts/converge-build.sh` + `pnpm dev:converged` wrapper. The operator needs this to stop wasting time on ad-hoc rebase + wipe + reinstall + launch sequences that keep failing on the same 7 known modes. Ship that PR fast (≤1.5h target), then chip through the remaining sub-issues per the priority order below.

**Operator is at $DAYJOB ~10 hours**. Decisive autonomous execution; no check-ins (`feedback_decisive_autonomous_execution`). Ship PRs as units; do NOT merge them (operator-only); each PR awaits operator review on wake.

## Mandatory pre-reads (in order)

1. **This file** (you're here). Take it top-to-bottom.
2. `dev/adversarial/2026-06-01-convergence-discipline-codex.md` — Codex's 21 findings; the proposed design pre-Codex has 2 P0 ship-blockers, so do not implement from the original proposal verbatim. Each filed sub-issue has Codex's revisions baked into its scope; cross-reference if uncertain.
3. `bd show tuxlink-edvb` — umbrella issue. Sub-issue priority + ordering rationale.
4. Each sub-issue body (`bd show tuxlink-qepd` first).

Do NOT skip 2. The original proposal would have failed on first use. The sub-issues encode the corrections; the codex transcript shows the reasoning.

## The seven known failure modes the discipline must handle

Catalog from today's session, captured here so the script's logic stays honest:

| # | Failure | Root cause | v1 handling |
|---|---|---|---|
| 1 | Orphan post-merge commits (v1p) | Agent commits to a branch whose PR already merged; never reopens follow-up PR | `tuxlink-21j8` (branch state machine + pre-commit hook). v1 script only WARNS; v2 BLOCKS via hooks. |
| 2 | Operator's main checkout 858 commits stale | task-amd-main-ui not rebased forward in 2 weeks; `tauri dev` builds + launches old binary | v1 script's rebase step. |
| 3 | `pnpm install` lying about state | Lockfile + store consistent but symlinks stale; install reports "already up to date"; vite import-resolution fails | v1 script: `rm -rf node_modules` unconditionally; `--fresh` flag retains, `--cached` opts out via lockfile diff (`tuxlink-pxmi` improves this later). |
| 4 | `.beads/issues.jsonl` auto-staged blocks rebase | bd's auto-export stages this file on every `bd update`/`bd close`; never committed | v1 script: explicit `git stash push -m "converge-build-bd-state" .beads/issues.jsonl` before rebase. |
| 5 | Untracked-vs-tracked identical-content collision | Operator's checkout has untracked dev/handoffs/foo.md; same path tracked on origin/main from another branch's PR; rebase refuses | v1 script: SHA-compare each untracked file vs origin/main; auto-remove identical; STOP-AND-ASK on diff. |
| 6 | Stale cargo target/ from old binary | `tauri dev` reuses target/debug/tuxlink built against pre-rebase code | v1 script: `rm -rf src-tauri/target`. |
| 7 | Parallel `tauri dev` instances + :1420 strictPort | Multiple worktrees can't bind same port; operator sees a different worktree's build | v1 script: `pkill -f "tauri dev|target/debug/tuxlink|node.*vite"`. (`tuxlink-8d7y` upgrades this to a proper lease later.) |

## Critical gotchas you WILL hit (and how to handle each)

### 1. You CANNOT write to the main checkout

Hook `block-main-checkout-race.sh` denies it. Always work in a worktree owned by a bd issue per ADR 0008. The operator's main checkout is `task-amd-main-ui`, currently 8 commits ahead of `origin/main` (handoff docs only). Treat task-amd-main-ui as **operator state, not agent state**.

For convergence-discipline work: each sub-issue gets its own worktree off `main`. Use:

```bash
python3 .claude/scripts/new_tuxlink_worktree.py --slug <slug> --issue tuxlink-<id> --base main --moniker <your-moniker>
```

### 2. bd auto-stages `.beads/issues.jsonl` on every bd call

Every `bd update` / `bd close` / `bd create` you run will stage this file. If you try to rebase or pull while this is staged, git refuses. **In the worktree where you're WORKING**, this isn't a problem (you commit it as part of your work or stash it). **In the operator's main checkout**, it's been a recurring blocker today.

The v1 script (`tuxlink-qepd`) explicitly stashes this file before rebase. Bake that pattern into anything you write that touches a rebase path.

### 3. The convergence-build worktree pattern is a P2 deferral

The v1 script in `tuxlink-qepd` builds from the operator's main checkout (rebased forward). That's a known SoT-muddiness compromise per Codex P1 #8 (the cleaner design builds from a disposable worktree at exactly `origin/main` — that's `tuxlink-pxmi`). Don't try to land both in v1; v1 ships fast, v2 refactors. Note this in the v1 PR body so the operator knows.

### 4. Codex CLI has daily quota

If you run a codex adrev round and get a ~5-line "ERROR: You've hit your usage limit ... try again at HH:MM" output, that's `feedback_codex_quota_gotcha`. **Defer the adrev round; do NOT substitute Claude.** Move to the next sub-issue and come back later. Each sub-issue's PR ideally gets a Codex round (per `feedback_codex_post_subagent_review`), but quota constraints may force serial rather than parallel adrev passes.

### 5. Do not merge PRs

You're an agent. PRs await operator review. `gh pr merge` is hook-watched and out of scope for this session. Even your own PRs: open, push, leave for the operator's review queue on wake.

### 6. Per-task branch + worktree convention

Each sub-issue → its own bd-tuxlink-<id>/<slug> branch + worktree per ADR 0004/0008. Worktrees stay alive until their PR merges; then dispose via ADR 0009 ritual. The script worktree (`bd-tuxlink-qepd-converge-build-script`) likely outlives your session — that's fine.

### 7. Session-end discipline

Per `standing-conventions-cross-project §7` + CLAUDE.md §Session Completion, you owe an end-of-session handoff + push of every commit. Even if you didn't ship all 6 sub-issues, document where each stands. Don't strand work locally; push every commit you make.

## Recommended priority order (one-session ~10h budget)

```
[P1] tuxlink-qepd  v1 script + pnpm dev:converged wrapper       ~1.5h
[P1] tuxlink-21j8  Branch state machine ADR + pre-commit/pre-push hooks  ~2.5h
[P2] tuxlink-8d7y  Dev-server lease at ~/.config/tuxlink/dev-server.json  ~1.5h
[P2] tuxlink-pxmi  Refactor converge-build to disposable-worktree-at-origin/main  ~1.5h
[P2] tuxlink-ui3i  CI scheduled audit GitHub Action              ~1h
[P3] tuxlink-8zho  Test fixtures for the 7 failure modes         ~2h
```

Totals: ~10h. If you slip, drop `8zho` first (P3 polish), then `ui3i` (P2 but lowest immediate-value), then `pxmi` (P2 but the v1 script already works without it).

Each is independent — they don't strictly depend on each other in code, though `tuxlink-21j8`'s hooks reinforce `tuxlink-qepd`'s script. Ship `qepd` first; everything else parallelizes.

## Per-PR discipline (per CLAUDE.md + briefing patterns)

1. `bd update <id> --claim --status=in_progress`
2. Create worktree via `new_tuxlink_worktree.py`
3. Write the code with TDD (verbatim tests first; superpowers:test-driven-development if relevant)
4. Verify (`cargo test`, `pnpm vitest`, `pnpm exec tsc --noEmit`, `cargo clippy -D warnings`)
5. Codex adrev (custom-prompt mode per CLAUDE.md; capture to `dev/adversarial/...codex.md`) — **quota-permitting**; defer the round if quota stub returned
6. Apply P0/P1 findings inline; file P2/P3 as bd issues
7. `git push` immediately after each commit (`feedback_never_hold_a_push`)
8. `gh pr create` against main with operator-browser-smoke checklist in body
9. `bd update <id> --notes "shipped as PR #N"`; leave in_progress until operator merges

## What landed earlier in this session (operator-visible state)

All 4 PRs from the overnight slate landed (193 / 197 / 195 / 196 / 199 / 200 / 201). `gh pr list --state open` is empty as of session-handoff time. `origin/main` HEAD includes:
- HTML Forms P0 trim + window resize + ICS-213 polish (v1p, PR #200)
- Mailbox sort newest-first + fresh Position/ICS-309/DA forms (mjc8, PR #201)
- Cc field enabled end-to-end (#197)
- WLE Standard Forms snapshot bundle + P1 backend infra (#195 axum/multer/wle_templates/skin/multipart/http_server + capability)
- Version-string sweep (#193)
- HTML Forms P1/P2/P3 plan docs (#196)
- VARA TCP transport (#199, ridge-oak-peregrine's work)

Plus the planning sprint commits + the design spec already landed.

## bd state at handoff

In-progress bd issues you should be aware of:

- `tuxlink-edvb` — umbrella convergence-discipline (this is the parent)
- `tuxlink-jy6p` — transient Codex adrev tracking; close ONLY after you've read `dev/adversarial/2026-06-01-convergence-discipline-codex.md` and confirmed the sub-issues encode the findings correctly
- `tuxlink-mjc8` — operator-owned mailbox sort + P2 form rebuilds (PR #201 merged); operator will close

Open + ready bd issues (your queue, in priority order):
- `tuxlink-qepd` — v1 script (P1, START HERE)
- `tuxlink-21j8` — state machine + hooks (P1)
- `tuxlink-8d7y` — dev-server lease (P2)
- `tuxlink-pxmi` — disposable-worktree-build refactor (P2)
- `tuxlink-ui3i` — CI audit (P2)
- `tuxlink-8zho` — test fixtures (P3)

## In-flight worktrees at handoff (per ADR 0009)

**`worktrees/bd-tuxlink-jy6p-convergence-adrev/`** (this handoff's host):
- Tracked dirty: `.beads/issues.jsonl` (auto-staged by bd; will be in `git status`)
- Untracked: none beyond the codex transcript + this handoff doc (committed at end of session)
- Gitignored-stateful: `dev/adversarial/2026-06-01-convergence-discipline-codex.md` (3300 lines; the canonical adrev for the discipline; keep it alive)
- Stashes: none
- Disposition: keep alive while `tuxlink-edvb` is in_progress (anchors the adrev transcript). Dispose via ADR 0009 ritual after the discipline ships v1; **archive the codex transcript first** (the directory is `.gitignore`d so the raw bytes leave the repo's git history on disposal).

**Other live worktrees** (operator-owned; don't touch):
- `worktrees/bd-tuxlink-7fr-ax25-packet/` — AX.25 packet work
- `worktrees/bd-tuxlink-9phd-strip-pat-add-native-attachments/` — Pat strip lineage
- `worktrees/bd-tuxlink-9yx-integration-smoke/` — prior integration smoke (different model)
- `worktrees/bd-tuxlink-hblz-vara-tcp/` — pre-rebase vara-tcp (the rebased one shipped via #199; this is leftover)
- `worktrees/bd-tuxlink-mjc8-mailbox-sort/` — operator-owned mailbox sort (PR #201 merged; operator may dispose)
- `worktrees/bd-tuxlink-v1p-html-forms-execution/` — operator was using this for `tauri dev` earlier; possibly still running

## Anti-patterns from today — DO NOT REPEAT

1. **Handing the operator a raw command sequence that ignores known failure modes**. Every command surface needs to encode bd staging + untracked collisions + stale node_modules. The v1 script (`tuxlink-qepd`) is your way to STOP doing this ad-hoc.

2. **Claiming `origin/main` is the SoT without verifying divergence**. Run `diff --shortstat` between the operator's checkout and origin/main BEFORE asserting convergence. The 858-commit-behind discovery is an embarrassment-class lesson.

3. **Splitting multi-step commands across line-wrapping in operator paste blocks**. If you absolutely must hand the operator a multi-step sequence, use ONE paste-block per logical step OR encode into a script + paste the heredoc.

4. **Filing bd issues without remembering they stage `.beads/issues.jsonl`**. After ANY bd command, your worktree (and the operator's, if they pull from your push) is dirty. Plan accordingly.

5. **Trusting Codex output without verifying it's not a quota stub**. Always `wc -l` the captured transcript; 5 lines = quota error, 1500+ = real review.

## Starting prompt for the next session

The operator's session-start-briefing hook reads from `dev/handoffs/<latest>.md` in the main checkout. This file is NOT in the main checkout — it's in this worktree. The starting prompt explicitly tells the next agent where to look. Hand the operator this paste-block on their next-session start:

```
You're starting an autonomous ~10h window. The convergence-discipline
work (tuxlink-edvb) is scoped + Codex-reviewed. Mandatory pre-read:

  worktrees/bd-tuxlink-jy6p-convergence-adrev/dev/handoffs/
    2026-06-01-moss-cove-hemlock-convergence-discipline-handoff.md

Then read the codex transcript at:
  worktrees/bd-tuxlink-jy6p-convergence-adrev/dev/adversarial/
    2026-06-01-convergence-discipline-codex.md

START WITH tuxlink-qepd (v1 converge-build script + pnpm dev:converged
wrapper). Then tuxlink-21j8, 8d7y, pxmi, ui3i, 8zho in priority order.

Decisive autonomous execution per feedback_decisive_autonomous_execution.
No check-ins. Ship PRs; do not merge them. /loop autonomous dynamic mode.

/loop
```

The trailing `/loop` invokes the autonomous-loop pattern with dynamic pacing. Each tick chips a sub-issue forward.

---

Agent: moss-cove-hemlock
