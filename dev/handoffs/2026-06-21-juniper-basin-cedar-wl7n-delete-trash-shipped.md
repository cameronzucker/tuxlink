# Handoff — Message Delete + Trash (tuxlink-wl7n) SHIPPED to main

Agent: juniper-basin-cedar · 2026-06-21 · PR #844 **MERGED** (merge commit `08ce5fcb`)

## TL;DR
The Message Delete + Trash feature is **shipped to main** and CI-green on the
integrated tree (both arches). bd issue `tuxlink-wl7n` is **closed**. The wl7n
worktree was disposed (ADR 0009). One follow-up remains tracked: **tuxlink-hsfa**.

## What shipped
- **Recovered** the prior session's (osprey-taiga-hawk) uncommitted MUST-FIX
  (user-folder-origin delete) + Task 9 (auto-purge) — the 2026-06-21 WIP handoff
  reported a clean tree but those were never committed; recovered + committed.
- **Codex backend adrev** (operator gate): 6 findings —
  - Fixed: `parse_folder`/`parse_folder_ref` accept `"deleted"` (Trash was
    unreachable through IPC — wire-walk blocker); `validate_mid` guards on
    `purge_message`/`restore_message` (path traversal); auto-purge sweep routes
    through the managed `BackendState` (indexed + emits `mailbox:changed`);
    restore falls back to Inbox when the origin user-folder slug is gone.
  - #2 (bulk delete lost the owning identity) folded into Task 13.
  - #3 (shared `deleted/` dir collides on same-MID-across-identities) → **tracked
    tuxlink-hsfa (P2, OPEN)**.
- **Tasks 13–16** via subagent-driven-development, each with a per-task review:
  - 13: bulk Delete/Restore/Delete-permanently in MessageBulkBar + the Codex #2
    identity-correct delete (MessageRefDto gains `identity`; threaded through).
  - 14: `ConfirmPurgeDialog` (inline, `.tux-folder-*`) replacing the AppShell
    `window.confirm` stopgaps + FolderSidebar **Empty Trash**.
  - 15: Settings **Mailbox** section — auto-purge toggle + retention days
    (`config_set_trash_auto_purge`).
  - 16: `docs/user-guide/07-mailbox-model.md` — Deleted folder + lifecycle.
- **Final whole-branch review** (opus): F1 (dead Message→Delete menubar item)
  **fixed**; F2 (exclude Trash from default search) **implemented**; F3 (block
  Outbox delete during a live session) — **implemented then STRUCK per operator**:
  deleting from the Outbox is always permitted (cancel-this-queued-send); blocking
  it reads as a broken client + the send loop snapshots at connect, so the guard
  solved a non-problem. Design doc updated to record the decision.
- **6 CI failures** caught + fixed (no-local-compile tax): `tests/` Config
  literals ×2, two stale delete-origin assertions, the `tests/ui_commands_test`
  `parse_folder("deleted")` rejection assertion.
- **Wire-walk** (operator-supplied flows, all ✅ wired end-to-end, file:line):
  list right-click Delete; reading-pane Delete button; right-click on a
  selection (bulk); Empty Trash. (Trace recorded in the SDD ledger before
  disposal.)
- **Merged `origin/main` into the branch** (Leaflet→MapLibre migration landed,
  operator-confirmed) as a merge (force-push is banned); only `AppShell.tsx`
  overlapped and auto-merged clean. Verified tsc + 128 frontend tests on the
  integrated tree; CI green on both arches before merge.
- Note: GitHub dropped the `synchronize` event for the merge commit (this branch
  isn't main/feat, so only the PR `synchronize` triggers CI); an empty commit
  (`f64c404c`) forced a fresh run.

## State at session end
- **Branch:** `bd-tuxlink-wl7n/message-delete-trash` merged → `08ce5fcb` on main;
  remote branch deleted by `--delete-branch`. Main checkout remains on the
  operator's `bd-tuxlink-xygm/recover-handoffs` (untouched).
- **Worktree `worktrees/bd-tuxlink-wl7n-message-delete-trash`:** DISPOSED (ADR
  0009). Pre-disposal inventory: tracked clean (all merged), no untracked work;
  gitignored-only on disk (`dev/scratch/wl7n-sdd/` SDD reports + codex findings,
  `node_modules`, `target`, `.superpowers/sdd` review diffs, the SDD ledger in
  `.git/.../sdd/`). All local-only scratch per convention — nothing to propagate;
  archived: none.
- **In progress:** none.
- **Pending decision:** tuxlink-hsfa (P2) — shared `deleted/` dir same-MID
  cross-identity collision. Low real-world exposure (Winlink MIDs ~unique; the
  realistic case is the same message under two callsigns, losing a duplicate
  copy). Operator to decide fix-now vs accept-with-tracking. Robust fix sketch in
  the issue: identity-qualify Trash storage (`deleted/<ns>/<mid>`) + aggregate the
  shared listing.

## Also confirmed this session
- The k9pg download fix (separately merged to main) is **built and validated** by
  the operator — closed out, no action owed.
