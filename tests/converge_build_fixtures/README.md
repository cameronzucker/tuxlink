# Convergence-build failure-mode fixtures

This directory holds regression fixtures for the 7 known convergence failure modes catalogued in [dev/handoffs/2026-06-01-moss-cove-hemlock-convergence-discipline-handoff.md](../../dev/handoffs/2026-06-01-moss-cove-hemlock-convergence-discipline-handoff.md). Each fixture validates one mode's handler.

Per Codex 2026-06-01 P2 #18:

> Build shell/Python tests around fake repos for: post-merge commits, stale operator branch, missing pnpm symlink, staged `.beads/issues.jsonl`, untracked collisions, stale cargo binary, and port ownership.

Run via `python3 -m unittest tests.converge_build_fixtures_test`.

## Modes + handlers

| # | Mode | Handled by | Fixture |
|---|---|---|---|
| 1 | Orphan post-merge commits | `.githooks/pre-commit` (PR #204, merged) | `01-post-merge-commits.sh` |
| 2 | Stale operator branch | `converge-build.sh` `fetch_prune` + (v1) `rebase_forward` / (v2 PR #207) `ensure_disposable_worktree` | `02-stale-operator-branch.sh` |
| 3 | Missing pnpm symlink + up-to-date lockfile | `converge-build.sh` `maybe_wipe_build_artifacts` | `03-pnpm-symlink-rot.sh` |
| 4 | Staged `.beads/issues.jsonl` blocks rebase | (v1) `converge-build.sh` `stash_bd_state` / (v2) eliminated by removing rebase | `04-beads-jsonl-staged.sh` |
| 5 | Untracked-vs-tracked identical-content collision | (v1) `converge-build.sh` `resolve_untracked_collisions` / (v2) eliminated by disposable-wt fresh checkout | `05-untracked-collision.sh` |
| 6 | Stale `src-tauri/target/debug/tuxlink` | `converge-build.sh` `maybe_wipe_build_artifacts` (HEAD-change-detect) | `06-stale-cargo-target.sh` |
| 7 | Port 1420 ownership / strictPort | `converge-build.sh` `verify_port_free` + (PR #206) dev-server lease | `07-port-1420-collision.sh` |

## What the fixtures verify

Each fixture is a bash script under `tests/converge_build_fixtures/` that:

1. Sets up the failure scenario in a temp dir (fake git repo + minimal Vite/Tauri layout where needed).
2. Invokes the relevant converge-build component (function extracted via sed, OR the whole script).
3. Asserts the right exit code + side effect (stdout pattern, file existence, etc.).

The Python runner (`tests/converge_build_fixtures_test.py`) shells out to each fixture and aggregates pass/fail.

## What the fixtures do NOT verify

- **End-to-end real builds**: the fixtures stub `pnpm install` and `tauri dev` (network + compile-time prohibitive in tests). PR #203's own dry-run mode covers the orchestration; these fixtures cover the failure-mode handlers in isolation.
- **Cross-mode interactions**: each fixture exercises ONE mode; combined failure modes (e.g., stale branch + .beads staged + port collision) are out of scope.
- **v2-specific architecture** (PR #207's disposable worktree): fixtures for modes 2, 4, 5 are scoped to v1 (currently on main). When PR #207 merges, modes 2, 4, 5 fixtures may become "structural-immunity" assertions instead of behavioral tests.
