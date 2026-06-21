# Handoff — Message Delete + Trash (tuxlink-wl7n) WIP, CORE shipped + CI-green, finish remaining tasks

Agent: osprey-taiga-hawk · 2026-06-21 · PR #844 (draft) · branch `bd-tuxlink-wl7n/message-delete-trash`

## TL;DR
The **single-message Delete button is built, functional, and CI-GREEN on both arches**
(backend Rust clippy + cargo test + frontend typecheck/vitest all pass at commit
`13168e2a`). Built task-by-task via `superpowers:subagent-driven-development`. Remaining:
the user-folder-origin fix, auto-purge sweep + settings, bulk delete, the proper confirm
modal + Empty Trash sidebar, and docs — all specced + planned. **Next session: START with a
Codex review of the branch** (operator's call — a second set of eyes before finishing).

## Artifacts (read these first)
- **Spec:** `docs/design/2026-06-20-message-delete-trash-design.md` (APPROVED)
- **Plan:** `dev/plans/2026-06-20-message-delete-trash-plan.md` (16 tasks)
- **SDD ledger (resume map):** `"$(git rev-parse --git-path sdd)/progress.md"` — which tasks are done, which remain.
- **Per-batch implementer reports:** `dev/scratch/wl7n-sdd/*.md` (signatures, deviations, unverifiable items).

## DONE this session (committed + pushed to #844; backend CI green at 13168e2a)
- **Backend Tasks 1-8** (all in `src-tauri/src/`): `MailboxFolder::Deleted` (shared `deleted/` dir);
  `TrashMeta {origin, origin_full, deleted_at}` sidecar; `Mailbox::delete_message`/`restore_message`/
  `purge_message`/`empty_trash`/`purge_expired` + pure `trash_is_expired`; `WinlinkBackend` trait
  methods + `NativeBackend` impls; the 6 Tauri commands (`message_delete`/`_bulk`,
  `message_restore`/`_bulk`, `trash_empty`, `trash_purge_one`) registered in `lib.rs`.
- **Frontend Tasks 10-12 + AppShell wiring**: TS wrappers (`src/mailbox/mailboxCommands.ts`:
  `deleteMessages`/`restoreMessages`/`emptyTrash`/`purgeMessage`); Deleted folder enabled
  (`useMailbox`, `FolderSidebar`); Delete / Restore / Delete-permanently in `MessageContextMenu`
  + `MessageView` (+ `Del` key + `menuModel` entry); AppShell handlers
  (`deleteByIdAndFolder`/`restoreById`/`purgeById`) threaded via `MessageList` → context menu
  + into `MessageView`. **Single-message delete works end-to-end.** Frontend fully verified
  (3124 vitest + tsc green).

## REMAINING (resume in this order)
1. **MUST-FIX (correctness): user-folder-origin delete.** `Mailbox::delete_message` takes a
   system `MailboxFolder`; `NativeBackend::delete_message_in` routes a `FolderRef::User(slug)`
   origin through a plain `move_between` with NO `.trash` sidecar, so restoring a message deleted
   from a **user folder** lands in Inbox, not the user folder. Fix: generalize `delete_message`
   to accept a `FolderRef` origin (record the slug + identity in `.trash`); update callers
   (`delete_message_in`, the Task-3 tests). Restore already handles a user-folder slug origin.
2. **Task 9** — auto-purge: `config.rs` `trash_auto_purge: bool` (default true) +
   `trash_retention_days: u32` (default 30), `#[serde(default)]`; `lib.rs` `.setup()` startup
   `purge_expired_trash(days)` + a `tokio::time::interval` (~6h) sweep while enabled (best-effort).
3. **Task 13** — `MessageBulkBar` bulk Delete (+ Restore / Delete-permanently in Trash). The
   selection-mode context menu currently routes to the single-message handlers (noted in
   `appshell-wire-report.md`); wire the real bulk path.
4. **Task 14** — replace AppShell's `window.confirm` stopgap (`// TODO(tuxlink-wl7n Task 14)` in
   `purgeById`) with a proper modal mirroring `DeleteFolderDialog.tsx`, for Empty Trash +
   Delete-permanently (the ONLY confirmed actions). Add the **Empty Trash** action to
   `FolderSidebar` when viewing the Deleted folder.
5. **Task 15** — Settings: auto-purge toggle + retention-days input (binds the Task-9 config).
6. **Task 16** — docs: `docs/user-guide/07-mailbox-model.md` (Deleted folder + Delete→Restore→
   Empty/auto-purge lifecycle; the doc already says "or deletes").
7. **Done-gates:** Codex adrev on the backend diff; operator-greenfield wire-walk; **rebase onto
   main AFTER the Leaflet map-engine migration lands** (disjoint — mailbox, not map); mark #844
   ready + merge.

## CRITICAL operational gotchas for the next session
- **No local Rust compile** (this Pi can't finish a cold cargo build). CI is the ONLY Rust gate
  (~15 min/cycle). Subagents write Rust they can't verify → CI surfaces clippy `-D warnings`
  lints one compile-phase at a time. **Arm every backend subagent with the known traps upfront**
  to compress cycles: use `std::io::Error::other(e)` (NOT `Error::new(ErrorKind::Other,e)`);
  `opt.is_some_and(..)` (NOT `map_or(false,..)`); do NOT use `is_none_or` (1.82 > MSRV 1.75);
  no `.len()==0`, no needless clone/borrow, no `if let Ok(_)/Some(_)`. **Push to the draft PR
  early and watch `verify` on both arches.** (2 lints already cost cycles this session:
  io_other_error, unnecessary_map_or — both fixed.)
- **Subagents can't commit in the worktree** (main-checkout hook denies). Protocol used:
  implementer edits files + runs vitest (frontend) + STOPS uncommitted; **parent commits**
  (standalone `cd <worktree>` first, then `git`) + pushes. Frontend tasks ARE locally
  vitest-verifiable (node_modules installed) — have those subagents run `pnpm exec vitest run
  <files>` + `pnpm exec tsc --noEmit` and fix to green before reporting.
- Worktree: `worktrees/bd-tuxlink-wl7n-message-delete-trash`, bd issue `tuxlink-wl7n` (in_progress).

## Working-tree state
- Worktree clean (all committed + pushed). Untracked: `dev/scratch/wl7n-sdd/*.md` (the implementer
  reports — gitignored scratch, keep for resume context). The SDD ledger is in `.git/.../sdd/`.

## Also still open (unrelated)
- **k9pg download fix is merged to main** awaiting the operator's converged-build smoke (download
  North America + Local; confirm the progress bar climbs with a live MB/s and completes).
