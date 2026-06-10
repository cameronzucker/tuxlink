# Handoff - 2026-06-10 jay-bluff-fir - Message-management docs

## Summary

Created PR #569 to fix the WLE-help-derived message-management and
operator-admin docs gap. The PR is docs-only and adds Tuxlink-native guidance
for message kinds, routing, addressing, attachments, Accept List limitations,
local archive backup/import/export, session-log troubleshooting, and
Express-parity gaps.

## Branch and PR

- Worktree: `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-v3aq-message-docs`
- Branch: `bd-tuxlink-v3aq/message-docs`
- PR: https://github.com/cameronzucker/tuxlink/pull/569
- Base: `main`
- Issue: `tuxlink-v3aq`
- Commit: `c6690e2`

## Files changed

- `docs/user-guide/07-mailbox-model.md`
  - Adds backup/import/export guidance for `native-mbox/`.
  - Clarifies that in-app import/export and Express/Pat conversion are not
    shipped yet.
- `docs/user-guide/18-the-mailbox.md`
  - Explains mailbox message kinds: plain mail, forms, catalog/weather
    responses, inbound attachments.
  - Adds Outbox failure triage and session-log pointers.
- `docs/user-guide/19-composing.md`
  - Clarifies callsign/email addressing and connection-target separation.
  - Documents account-side Accept List/spam limitations.
  - Adds image-size practice and notes missing Express-style resize/crop tools.
- `docs/user-guide/29-troubleshooting.md`
  - Adds session-log-first diagnostics, Outbox triage, internet mail/Accept
    List troubleshooting, background/statistics boundaries, and archive
    migration pointers.
- `docs/user-guide/32-from-express-or-pat.md`
  - Adds parity-gap rows for send-as/message-type selector, outbound
    attachments/image tools, templates, Accept List controls, import/export,
    background tasks, unattended connects, and statistics.
  - Tightens migration wording around archive conversion not being shipped.

## Validation

- `git diff --check`
- `pnpm lint:docs`
- pre-push `pnpm lint:docs`

## CI state at handoff

PR #569 is mergeable. All four checks had started and were still in progress:

- `verify (ubuntu-latest, amd64)`
- `verify (ubuntu-24.04-arm, arm64)`
- `build-linux (ubuntu-latest, amd64)`
- `build-linux (ubuntu-24.04-arm, arm64)`

## Notes

`tuxlink-v3aq` remains open in bd until PR #569 merges. A bd note was added
with the PR link and validation summary.

The shared root checkout state remains intentionally dirty from unrelated
tracker/handoff work:

- Branch: `bd-tuxlink-xygm/recover-handoffs`
- Dirty: staged/modified `.beads/issues.jsonl`
- Untracked: `dev/handoffs/2026-06-10-arroyo-lichen-grouse-request-center-reskin-shipped-pr559.md`

Do not clean that root state without operator direction.

## Next action

Watch PR #569 CI. If it passes, merge normally. If it fails, the likely issue
surface is docs link lint, because no code or registry files changed.
