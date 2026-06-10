# Handoff - 2026-06-10 jay-bluff-fir - Connection walkthrough docs

## Summary

Created PR #570 to fix the WLE-help-derived connection walkthrough docs gap.
The PR is docs-only and adds concrete operator procedures for starting shipped
connection modes, reviewing pending inbound messages, and mapping Winlink
Express session names to Tuxlink's operating-mode/protocol sidebar model.

## Branch and PR

- Worktree: `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1doe-connection-docs`
- Branch: `bd-tuxlink-1doe/connection-docs`
- PR: https://github.com/cameronzucker/tuxlink/pull/570
- Base: `main`
- Issue: `tuxlink-1doe`
- Commit: `afbda11`

## Files changed

- `docs/user-guide/08-picking-a-transport.md`
  - Reframes Connect as the selected radio panel's primary action.
  - Adds alpha UI session-start flow from the Connections sidebar.
  - Adds concrete walkthroughs for Telnet CMS, Packet CMS, ARDOP/VARA CMS,
    Peer-to-peer, Radio-only, Post Office, and Network Post Office.
  - Documents that gateway/favorite Connect buttons prefill only and do not
    transmit.
  - Documents Review Pending Messages behavior.
- `docs/user-guide/33-operating-modes.md`
  - Adds a Winlink Express session-name mapping table.
  - Marks PACTOR, Robust Packet/RPR, Iridium GO, and HF Auto Connect as
    unshipped/omitted.
  - Points maintainers to the WLE parity closure plan and Telnet Post Office
    design using plain file paths because the in-app user-guide link linter
    forbids links outside the user-guide bundle.

## Validation

- `git diff --check`
- `pnpm lint:docs`
- pre-push `pnpm lint:docs`

## CI state at handoff

PR #570 is mergeable. All four checks had started and were in progress:

- `verify (ubuntu-latest, amd64)`
- `verify (ubuntu-24.04-arm, arm64)`
- `build-linux (ubuntu-latest, amd64)`
- `build-linux (ubuntu-24.04-arm, arm64)`

## Follow-up Filed

Filed and claimed `tuxlink-nnws`: ARDOP Radio-only is exposed in
`SESSION_TYPES`, but `ArdopRadioPanel` still hardcodes CMS panel/exchange
intent. The new docs call this out as an alpha caveat and recommend VARA for
Radio-only operations until ARDOP is made intent-aware.

## Notes

`tuxlink-1doe` remains open in bd until PR #570 merges. A bd note was added
with the PR link, validation summary, and `tuxlink-nnws` follow-up.

The shared root checkout state remains intentionally dirty from unrelated
tracker/handoff work:

- Branch: `bd-tuxlink-xygm/recover-handoffs`
- Dirty: staged/modified `.beads/issues.jsonl`
- Untracked: `dev/handoffs/2026-06-10-arroyo-lichen-grouse-request-center-reskin-shipped-pr559.md`

Do not clean that root state without operator direction.

## Next action

Watch PR #570 CI. If it passes, merge normally. If it fails, the likely issue
surface is docs link lint, because no code or registry files changed.
