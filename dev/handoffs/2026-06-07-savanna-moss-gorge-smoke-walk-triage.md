# 2026-06-07 savanna-moss-gorge smoke-walk triage

## Summary

Continued alpha candidate smoke-walk triage from `dev/scratch/Tuxlink bd Issues-to-File.txt` using moniker `savanna-moss-gorge`.

Opened PR #457 for item 6 / `tuxlink-ewtb`: image attachment previews in the message reader. Item 27 was verified already fixed by closed issue `tuxlink-n8gm` and merged PR #449, so no duplicate was filed.

## Branches and PRs

- `bd-tuxlink-ewtb/attachment-image-preview`
  - Worktree: `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-ewtb-attachment-image-preview`
  - Commit: `173188b fix(mailbox): preview image attachments`
  - PR: https://github.com/cameronzucker/tuxlink/pull/457
  - PR state at handoff: open, mergeable, CI in progress.
  - Worktree state at handoff: clean and pushed to `origin/bd-tuxlink-ewtb/attachment-image-preview`.

Previously opened autonomous PRs from this run remain for operator CI/merge monitoring:

- PR #454 / `tuxlink-wiwb`: session log message movement summary.
- PR #455 / `tuxlink-2lsd`: operating modes guide.
- PR #456 / `tuxlink-c8qk`: known Winlink clients guide.

## Verification

For PR #457:

- `git diff --check`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib attachment_preview`
- `pnpm exec vitest run src/mailbox/AttachmentStrip.test.tsx src/mailbox/MessageView.test.tsx src/shell/AppShell.test.tsx`
- `pnpm typecheck`
- Pre-push hook: `pnpm lint:docs`

Commit message was verified before push and contains:

- `Agent: savanna-moss-gorge`
- `Co-authored-by: Codex <noreply@openai.com>`

## Duplicate and Fixed Checks

Item 6 was verified against current `origin/main`, existing bd issues, merged PRs, Hamexandria, and the WLE corpus before filing `tuxlink-ewtb`.

Adjacent but not duplicate:

- `tuxlink-4or5` / PR #415: attachment list + Save As.
- `tuxlink-9ylw` / PR #401 and PR #412: binary body handling.
- `tuxlink-pxf`: outbound attachment shrink-to-fit.

Item 27 is already fixed:

- bd: `tuxlink-n8gm`, closed.
- PR: #449, merged 2026-06-07 at `184c49d`.
- Current `origin/main` includes enabled Drafts sidebar, local draft listing/reopen wiring, and Save Draft persistence.

## Local State

- Root checkout remains a control checkout on `bd-tuxlink-xygm/recover-handoffs` with pre-existing dirty/stale `.beads` and untracked handoff/design artifacts. I did not clean or revert them.
- `bd dolt push` was attempted; no Dolt remote is configured, so bd reported local storage only.
- Hamexandria path verified: `/home/administrator/Code/tuxlink/dev/scratch/ham-knowledge-store`.
- WLE corpus path verified: `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-wiwb-session-log-message-summary/dev/research/winlink-group-corpus-2026-06-04`.

## Next Session

First action: check CI on PR #457 and the earlier open PRs, but do not auto-merge unless Cameron explicitly asks.

No additional smoke-list item is currently approved for autonomous implementation. Item 5 remains deferred. Ask Cameron for the next approved item set before filing more smoke-walk issues.
