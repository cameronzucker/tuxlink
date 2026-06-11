# Handoff - 2026-06-10 magpie-grouse-basalt - 0.47.0 release conflict

## Summary

Resolved the merge conflict on PR #567 (`chore(main): release 0.47.0`).
The branch now merges cleanly and CI is green on both configured platforms.

## Critical context checked first

- Read prior handoff:
  `dev/handoffs/2026-06-10-jay-bluff-fir-wle-help-corpus-pr566.md`
  from the `bd-tuxlink-tdjs-wle-help-corpus` worktree because it was not
  present in the root checkout.
- Checked PR #566: it is merged; GitHub attached/reran CI and both jobs passed:
  `verify (ubuntu-latest, amd64)` and `verify (ubuntu-24.04-arm, arm64)`.
- Raw WLE help extraction remains local-only under ignored
  `dev/winlink-reference/`; nothing from that corpus was touched.

## PR #567 resolution

- Worktree:
  `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-ps3a-release-047-conflict`
- Branch: `release-please--branches--main`
- PR: https://github.com/cameronzucker/tuxlink/pull/567
- Commit pushed: `613ad1c chore(release): resolve 0.47.0 changelog conflict`
- Conflict file: `CHANGELOG.md`
- Resolution: merged current `origin/main` into the release branch and kept the
  missing 0.47.0 changelog entry for:
  `fix(request): correct WebKitGTK render + 6-char grid defects in the re-skin`.
- Resulting PR diff against current `main`: one `CHANGELOG.md` insertion.

## Validation

- Local:
  - `git diff --check origin/main...HEAD`
  - `pnpm install --frozen-lockfile` in the release-conflict worktree because
    the fresh worktree had no `node_modules`.
  - `pnpm lint:docs`
- Push:
  - `git push origin release-please--branches--main` succeeded.
  - Pre-push docs link linter passed.
- GitHub:
  - PR #567 is `CLEAN` / `MERGEABLE`.
  - CI run `27307745931` passed:
    - `verify (ubuntu-latest, amd64)` in 10m23s.
    - `verify (ubuntu-24.04-arm, arm64)` in 13m26s.

## Beads

- Created and claimed `tuxlink-ps3a` for this conflict-resolution task.
- Closed `tuxlink-ps3a` after PR #567 became clean and CI passed.
- `bd dolt push` reported no remote configured, so the Beads state remains local
  in `.beads/`.

## Checkout state

- Release conflict worktree is clean:
  `release-please--branches--main...origin/release-please--branches--main`.
- Root checkout remains on `bd-tuxlink-xygm/recover-handoffs` with dirty state:
  - staged/modified `.beads/issues.jsonl` (now includes `tuxlink-ps3a`; also
    contains pre-existing Beads churn/reordering)
  - untracked
    `dev/handoffs/2026-06-10-arroyo-lichen-grouse-request-center-reskin-shipped-pr559.md`
  - this handoff file
- Existing stashes were present before this cleanup pass and were not touched.
- The release worktree was left in place because PR #567 remains open; do not
  remove it casually.

## Next action

PR #567 is ready for operator review/merge. Be aware that `v0.47.0` already
exists on `main`; after conflict resolution this PR is effectively a one-line
0.47.0 changelog correction, not a fresh version bump.
