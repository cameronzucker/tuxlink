# Handoff - 2026-06-10 jay-bluff-fir - Connection walkthrough docs

## Summary

Created PR #570 to fix the WLE-help-derived connection walkthrough docs gap.
The PR adds concrete operator procedures for starting shipped connection modes,
reviewing pending inbound messages, and mapping Winlink Express session names
to Tuxlink's operating-mode/protocol sidebar model. During grounding, it also
fixed the ARDOP Radio-only intent mismatch filed as `tuxlink-nnws`.

## Branch and PR

- Worktree: `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1doe-connection-docs`
- Branch: `bd-tuxlink-1doe/connection-docs`
- PR: https://github.com/cameronzucker/tuxlink/pull/570
- Base: `main`
- Issue: `tuxlink-1doe`
- Commits: `afbda11`, plus follow-up code/handoff commits pushed after this
  handoff was updated.

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
- `src/radio/modes/ArdopRadioPanel.tsx`
  - Accepts the selected ARDOP `RadioPanelMode`, uses it for the panel title,
    and passes its intent to `modem_ardop_b2f_exchange`.
- `src/shell/AppShell.tsx`
  - Passes the selected ARDOP mode into `ArdopRadioPanel`.
- `src/radio/modes/ArdopRadioPanel.test.tsx`
  - Adds coverage for the Radio-only panel title and Send/Receive IPC intent.
- `src/shell/AppShell.radioPanel.test.tsx`
  - Adds coverage for selecting **Radio-only -> ARDOP HF** in the sidebar.

## Validation

- `git diff --check`
- `pnpm lint:docs`
- `pnpm exec vitest run src/radio/modes/ArdopRadioPanel.test.tsx src/shell/AppShell.radioPanel.test.tsx src/radio/types.test.ts src/radio/radioPanelVisibility.test.ts`
- pre-push `pnpm lint:docs`

## CI state at handoff

PR #570 is mergeable. All four checks had started and were in progress:

- `verify (ubuntu-latest, amd64)`
- `verify (ubuntu-24.04-arm, arm64)`
- `build-linux (ubuntu-latest, amd64)`
- `build-linux (ubuntu-24.04-arm, arm64)`

## Follow-up Fixed

Filed and claimed `tuxlink-nnws`: ARDOP Radio-only was exposed in
`SESSION_TYPES`, but `ArdopRadioPanel` hardcoded CMS panel/exchange intent.
PR #570 now fixes this by passing the selected ARDOP mode through from
`AppShell`.

## Notes

`tuxlink-1doe` and `tuxlink-nnws` remain open in bd until PR #570 merges. A
bd note was added to `tuxlink-1doe` with the PR link, validation summary, and
`tuxlink-nnws` follow-up.

The shared root checkout state remains intentionally dirty from unrelated
tracker/handoff work:

- Branch: `bd-tuxlink-xygm/recover-handoffs`
- Dirty: staged/modified `.beads/issues.jsonl`
- Untracked: `dev/handoffs/2026-06-10-arroyo-lichen-grouse-request-center-reskin-shipped-pr559.md`

Do not clean that root state without operator direction.

## Next action

Watch PR #570 CI. If it passes, merge normally. If it fails, likely surfaces
are docs link lint, the focused ARDOP/AppShell tests, or TypeScript/Rust gates
around the now intent-aware ARDOP panel.
