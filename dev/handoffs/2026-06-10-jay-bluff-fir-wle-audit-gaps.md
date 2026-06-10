# 2026-06-10 jay-bluff-fir - WLE help audit gaps

## Summary

Continued the WLE help-corpus audit follow-through. The audit itself and the
first docs passes are now represented by PRs, and two additional small WLE
docs gaps were filed and fixed. Product gaps discovered during the pass were
filed but not claimed unless they were small enough to fix safely here.

## PRs

- PR #566, `bd-tuxlink-tdjs/wle-help-corpus`: WLE help corpus audit. CI green.
- PR #568, `bd-tuxlink-ng91/contacts-docs`: contacts/groups docs. CI green
  after rerunning a transient crates.io amd64 verify failure.
- PR #569, `bd-tuxlink-v3aq/message-docs`: message-management/operator-admin
  docs. CI green.
- PR #570, `bd-tuxlink-1doe/connection-docs`: connection walkthrough docs plus
  `tuxlink-nnws` ARDOP Radio-only intent fix. CI green.
- PR #571, `bd-tuxlink-1e1f/settings-reference`: Settings operator-reference
  rewrite. CI pending at handoff.
- PR #572, `bd-tuxlink-7znu/glossary-wle-terms`: WLE-derived glossary terms.
  CI pending at handoff.

## New Issues

- `tuxlink-1e1f`: Docs: expand Settings into an operator reference. Fixed in PR
  #571.
- `tuxlink-7znu`: Docs: fill WLE-derived glossary gaps. Fixed in PR #572.
- `tuxlink-8ql1`: Product gap: operator usage statistics view. Filed, unclaimed.
- `tuxlink-i1ee`: Product gap: new-message notification and forwarding
  preferences. Filed, unclaimed.
- `tuxlink-hjhk`: Product gap: Winlink ACCEPTLIST helper flow. Filed,
  unclaimed.

Existing audit-linked issues still relevant:

- `tuxlink-px36`: WLE mailbox migration tool/feature. Open and unclaimed.
- `tuxlink-zmzx`: WLE mailbox migration docs. Open, depends on `tuxlink-px36`.
- `tuxlink-pxf`: attachment shrink-to-fit. Open, already tracked.
- Forms/catalog/GRIB/pending-inbound are already tracked or closed by existing
  issues/PRs.

## Validation

For PR #570 after the ARDOP intent fix:

- `pnpm exec vitest run src/radio/modes/ArdopRadioPanel.test.tsx src/shell/AppShell.radioPanel.test.tsx src/radio/types.test.ts src/radio/radioPanelVisibility.test.ts`
- `pnpm lint:docs`
- `git diff --check`

For PR #571:

- `git diff --check`
- `pnpm lint:docs`
- `pnpm exec vitest run src/help/topics.test.ts`

For PR #572:

- `git diff --check`
- `pnpm lint:docs`
- `pnpm exec vitest run src/help/topics.test.ts`

`bd dolt push` was run; no Dolt remote is configured, so bd reported local-only
storage and skipped remote push.

## Worktree State

- Root checkout: `bd-tuxlink-xygm/recover-handoffs`; shared dirty state remains
  (`.beads/issues.jsonl` modified, unrelated untracked handoff present). Do not
  clean this without operator direction.
- `worktrees/bd-tuxlink-tdjs-wle-help-corpus`: clean, PR #566.
- `worktrees/bd-tuxlink-ng91-contacts-docs`: clean, PR #568.
- `worktrees/bd-tuxlink-v3aq-message-docs`: clean, PR #569.
- `worktrees/bd-tuxlink-1doe-connection-docs`: clean, PR #570.
- `worktrees/bd-tuxlink-1e1f-settings-reference`: clean, PR #571.
- `worktrees/bd-tuxlink-7znu-glossary-wle-terms`: clean, PR #572.

## Next Session

Start by checking PR #571 and #572 CI. If green, merge the docs-only PRs after
operator review preference. Then decide whether to take the larger WLE mailbox
migration feature (`tuxlink-px36`) or leave it for a design-focused session.
