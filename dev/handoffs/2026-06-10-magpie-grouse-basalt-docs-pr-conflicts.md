# Handoff - 2026-06-10 magpie-grouse-basalt - docs PR conflict sweep

## Summary

Investigated open docs-only PR merge conflicts and resolved the two conflicted
branches:

- PR #569: `docs: fill WLE message-management guide gaps`
- PR #571: `docs: expand Settings operator reference`

PR #572 was checked and was already clean/mergeable, so it was left untouched.

## PR #571

- Branch/worktree:
  `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1e1f-settings-reference`
- Commit pushed: `26d4967 docs(settings): merge main into settings reference`
- Conflict: `docs/user-guide/27-settings.md`
- Resolution: kept the settings operator-reference rewrite, folded in main's
  contacts/groups persistence notes, new-machine keyring guidance, and the
  Contacts and groups "Where next" link.
- Post-push GitHub state: `CLEAN` / `MERGEABLE`
- Checks: CI verify and release build passed on amd64 and arm64.

## PR #569

- Branch/worktree:
  `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-v3aq-message-docs`
- Commit pushed: `bc15328 docs(messages): merge main into message guide`
- Conflict: `docs/user-guide/19-composing.md`
- Resolution: kept the WLE-derived addressing-practice section and folded in
  main's contacts/groups recipient autocomplete guidance.
- `docs/user-guide/32-from-express-or-pat.md` auto-merged cleanly.
- Post-push GitHub state: `CLEAN` / `MERGEABLE`
- Checks: CI verify and release build passed on amd64 and arm64.

## Local validation

- PR #571 worktree:
  - `git diff --check origin/main...HEAD`
  - `pnpm lint:docs`
  - `pnpm exec vitest run src/help/topics.test.ts`
- PR #569 worktree:
  - `git diff --check origin/main...HEAD`
  - `pnpm lint:docs`
- Pre-push docs link linter also passed for both branch pushes.

## Current open docs PR states

- #569: clean, mergeable, checks green.
- #571: clean, mergeable, checks green.
- #572: clean, mergeable, checks green; not modified in this pass.

## Checkout state

- Both PR worktrees are clean against their remotes after push.
- Root checkout still has pre-existing dirty state on
  `bd-tuxlink-xygm/recover-handoffs`:
  - modified `.beads/issues.jsonl`
  - untracked
    `dev/handoffs/2026-06-10-arroyo-lichen-grouse-request-center-reskin-shipped-pr559.md`
  - untracked
    `dev/handoffs/2026-06-10-magpie-grouse-basalt-release-047-conflict.md`
  - this handoff file
- Do not clean or rewrite the root checkout casually.

## Next action

The docs-only PRs are ready for operator review/merge. Merge order may matter
only insofar as later docs PRs can reintroduce normal changelog/docs drift; as
of this handoff, #569, #571, and #572 are all clean and green.
