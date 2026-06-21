# Message Delete + Trash — design

- **Issue:** tuxlink-wl7n
- **Date:** 2026-06-20
- **Status:** approved (brainstorm 2026-06-20)

## Problem

The mailbox is a CRUD surface with no Delete. A message can be received, read,
moved, and archived, but never removed. `docs/user-guide/07-mailbox-model.md`
already describes the intended model — "Stays until the operator moves to Archive
**or deletes**" — so Delete was always part of the design and simply never got
built. Users have hit the gap.

## Goals

- Add Delete for messages in **every** folder (Inbox, Sent, Outbox, Archive, user
  folders).
- Make Delete **recoverable**: deleted messages go to a Trash folder, not straight
  to permanent removal.
- Keep Trash **distinct from Archive**. Archive is intentional long-term retention
  ("keep this, I'll find it later"); Trash is discard-with-a-grace-period ("gone,
  recoverable for a while, then purged").
- Provide the full lifecycle: Delete → Restore → Empty Trash / permanent delete,
  plus time-based auto-purge.

## Non-goals (YAGNI)

Undo-toast, drag-to-trash, cross-device Trash sync, a Drafts folder rework.

## Model & verbs

Messages persist as `{mid}.b2f` files inside per-folder directories, with `.read`
and `.identity` sidecars; the existing `Mailbox::move_to` (`native_mailbox.rs:315`)
already unlinks the source, writes the destination, and carries the sidecars. The
Trash lifecycle reuses that machinery.

| Verb | Behavior | Confirm? |
|---|---|---|
| **Delete** | Move the message into the **Deleted** system folder. Recoverable. | No — undoable, friction-free like Archive |
| **Restore** | Move the message from Deleted back to its origin folder. | No |
| **Empty Trash** | Permanently purge every message in Deleted. | Yes |
| **Delete permanently** | Permanently purge one message (only from inside Trash). | Yes |
| **Auto-purge** | Background sweep purges Deleted items older than the retention window. | n/a |

"Permanent purge" reuses the existing permanent-delete path used by the user-folder
delete cascade: unlink the `{mid}.b2f` (and its sidecars) and drop the search-index
row via `index_delete(mid)` (`native_mailbox.rs:815`).

### Restore-to-origin via a `.trash` sidecar

At delete time, write a `{mid}.trash` sidecar (parallel to `.read` / `.identity`)
holding the origin folder, the origin identity, and the deletion timestamp:

```json
{ "origin": "inbox", "origin_full": "N0CALL", "deleted_at": "2026-06-20T18:30:00Z" }
```

- `origin` is the source folder reference (`"inbox"` / `"sent"` / `"outbox"` /
  `"archive"` / a user-folder slug).
- `origin_full` is the source **identity** (FULL), recorded when the origin is a
  per-identity folder (Inbox / Archive / user folders, which live under
  `root/mailbox/{FULL}/`). It is absent/ignored for the shared folders (Sent /
  Outbox). This is what lets restore re-home to the *correct* identity's folder —
  the folder name alone is not enough because Inbox/Archive are per-identity. (For
  Sent/Outbox the existing `.identity` sidecar already travels with the message.)
- `deleted_at` is the RFC3339 UTC instant the message entered Trash. It is the
  authoritative clock for auto-purge (file mtimes are too fragile to trust).

**Restore** reads `origin` + `origin_full`, moves the message back to that folder
under that identity, and removes the `.trash` sidecar. If the sidecar is missing or
names a folder/identity that no longer exists (e.g. a deleted user folder or
removed identity), restore falls back to the active identity's **Inbox**. The
`.trash` sidecar is carried like the other sidecars on the delete move and removed
on restore/purge.

### Auto-purge

- **Retention window: 30 days, configurable in Settings, ON by default.**
- A sweep runs on mailbox init (startup) and on a periodic timer; it purges every
  Deleted item whose `deleted_at` is older than the window.
- Purge is the permanent-delete path (unlink + `index_delete`). Best-effort and
  logged; a single failure never aborts the sweep.

## Per-folder behavior

Delete is available from every folder and always targets Trash. Two cases differ:

- **Outbox (queued, unsent):** Delete → Trash like any folder; `origin: "outbox"`,
  so **Restore re-queues** the message to the Outbox send queue. Delete is **always
  permitted**, including during a live session — it is the operator's "cancel this
  queued send" control. (An earlier draft of this design proposed blocking delete
  for a message *actively transmitting* in a live session; that guard was **struck**
  per operator 2026-06-21. Sessions are long and the Outbox is an awaiting-send
  holding area, so blocking or greying delete there reads as a broken client; and
  the send loop snapshots messages at connect time, so deleting the file does not
  corrupt an in-flight transfer.)
- **Inside the Trash (Deleted) folder:** the per-message actions are **Restore** and
  **Delete permanently** — there is no "delete to Trash" on a message already in
  Trash. The folder-level action is **Empty Trash**.

## Backend

### Data model

- Add `Deleted` to the Rust `MailboxFolder` enum (`winlink_backend.rs:34`, currently
  `Inbox, Sent, Outbox, Archive`). `as_str(Deleted) -> "deleted"`. The TypeScript
  `MailboxFolder` type (`src/mailbox/types.ts:87`) already includes `'deleted'`
  (presently disabled); enable it.
- The Deleted folder is a **shared** system folder directory (one Trash, like
  Sent/Outbox — not per-identity). Identity context is preserved per-message by the
  `.trash` sidecar's `origin_full` (and the existing `.identity` sidecar for
  Sent/Outbox-origin messages), so restore re-homes to the right identity without a
  per-identity Trash directory. The Trash view lists all deleted messages; whether
  it filters to the active identity is a UI choice that follows the existing
  mailbox identity-scoping model (the per-message origin identity is available
  either way).

### Commands (`ui_commands.rs`)

Mirror the existing move/bulk command shapes (`mailbox_move`,
`message_move_bulk`, each item carrying its folder):

- `message_delete` / `message_delete_bulk` — move item(s) to Deleted; write each
  `.trash` sidecar with `{origin, deleted_at}`.
- `message_restore` / `message_restore_bulk` — move item(s) from Deleted back to
  the recorded origin (Inbox fallback); remove the `.trash` sidecar.
- `trash_empty` — permanent purge of all Deleted items.
- `trash_purge_one` — permanent purge of a single Deleted item.
- Auto-purge sweeper wired into the mailbox init path + a periodic tick.

### Testable (pure) pieces — TDD

- `.trash` sidecar serialize/deserialize round-trips `{origin, origin_full,
  deleted_at}` (including the `origin_full`-absent shared-folder case).
- Restore targets the recorded origin folder under `origin_full`; falls back to the
  active identity's Inbox on a missing/dangling sidecar.
- The purge selector returns exactly the items older than the retention window
  (boundary-inclusive), given a fixed "now".

## Frontend

- **`MessageContextMenu` (`MessageContextMenu.tsx`)** — add **Delete** (below
  Archive). When the current folder is Deleted, show **Restore** + **Delete
  permanently** instead of Delete.
- **`MessageView` (`MessageView.tsx:536`)** — add a **Delete** button next to the
  Archive button, with a **Del** key accelerator (parallels Archive's `A`). In the
  Deleted folder it becomes Restore + Delete-permanently.
- **`MessageBulkBar` (`MessageBulkBar.tsx`)** — add **Delete** for multi-select;
  Restore + Delete-permanently when the selection is in Trash.
- **`FolderSidebar` (`FolderSidebar.tsx`)** — enable the **Deleted** folder entry;
  surface **Empty Trash** in the Trash folder view.
- **Confirm dialogs:** reuse the `DeleteFolderDialog` modal pattern for the
  **permanent** actions only (Empty Trash, Delete permanently). Delete-to-Trash and
  Restore have no confirm.
- **Settings:** add the auto-purge retention toggle + day count (default 30 / on).

## Edge cases

- Deleting a message already in Trash is not offered (the menu swaps to Restore /
  Delete-permanently).
- Restore of a message whose origin folder was itself deleted → Inbox.
- Outbox message actively transmitting → Delete blocked (guarded), with a clear
  reason surfaced to the operator.
- Auto-purge and a manual Empty Trash racing → both are the same idempotent unlink +
  `index_delete`; a missing file is a no-op.
- Search index: every permanent purge drops the index row so purged messages do not
  appear in search; restored messages are re-indexed at their destination folder.

## Testing

- Rust unit tests for the pure pieces above + command-level tests mirroring the
  existing move/bulk command tests.
- Frontend tests for the context-menu / bulk-bar / message-view wiring and the
  folder-dependent action swap (Delete vs Restore/Delete-permanently).
- **Wire-walk at done-time** (operator supplies the flows greenfield) before the PR
  is marked ready.

## Docs

Update `docs/user-guide/07-mailbox-model.md`: document the Deleted folder and the
Delete → Restore → Empty/auto-purge lifecycle, completing the existing "or deletes"
reference.
