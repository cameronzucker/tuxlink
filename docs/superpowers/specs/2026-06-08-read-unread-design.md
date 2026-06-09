# Mark messages read/unread — design (tuxlink-etxt)

## Context

The message read-state machinery is roughly two-thirds built in the backend and
exposed only as a one-way, implicit side effect. The gaps this design closes are
the reverse direction (marking a message back to *unread*), an explicit operator
affordance (single and bulk), and surfacing read-state beyond the Inbox.

Existing, reusable infrastructure (do not rebuild):

- **Read-state model.** `MessageMeta` carries an `unread: bool` on both the Rust
  list type ([src-tauri/src/winlink_backend.rs:75-92](../../../src-tauri/src/winlink_backend.rs#L75-L92))
  and the TypeScript mirror ([src/mailbox/types.ts:15-36](../../../src/mailbox/types.ts#L15-L36)).
- **Read-state persistence.** A per-message empty sidecar file `<root>/<folder>/<mid>.read`
  marks a message read. A message is unread iff the sidecar is absent
  ([src-tauri/src/native_mailbox.rs:107-112](../../../src-tauri/src/native_mailbox.rs#L107-L112)).
  `Mailbox::mark_read` writes the sidecar idempotently and is no-op-safe for a
  missing message ([src-tauri/src/native_mailbox.rs:191-211](../../../src-tauri/src/native_mailbox.rs#L191-L211)).
  The sidecar travels with a message when it is moved between folders.
- **Search index.** The SQLite `messages_meta` table has an `unread INTEGER`
  column; `mark_read` fires an `update_unread` index hook
  ([src-tauri/src/search/index.rs:77-95](../../../src-tauri/src/search/index.rs#L77-L95)).
- **Implicit mark-read.** Opening a message in the reading pane invokes the
  `message_read` command, which calls `mark_read` best-effort
  ([src-tauri/src/ui_commands.rs:680-710](../../../src-tauri/src/ui_commands.rs#L680-L710));
  the frontend hook invalidates the mailbox query so the row updates
  ([src/mailbox/useMessage.ts:86-96](../../../src/mailbox/useMessage.ts#L86-L96)).
- **Unread surfacing — Inbox only.** `unread` is computed `true` only for the
  Inbox; user folders are listed with `surface_unread = false`, so every
  user-folder message reports `unread: false`
  ([src-tauri/src/native_mailbox.rs:354-360](../../../src-tauri/src/native_mailbox.rs#L354-L360)).
- **Unread count badge.** The Inbox folder badge counts unread messages
  client-side ([src/shell/AppShell.tsx:336-344](../../../src/shell/AppShell.tsx#L336-L344));
  the folder sidebar renders the badge ([src/mailbox/FolderSidebar.tsx:46-49](../../../src/mailbox/FolderSidebar.tsx#L46-L49)).
- **Row styling.** Unread rows render bold with an amber `unread-dot`; read rows
  render dimmed (`:not(.unread)`) ([src/shell/AppShell.css:683-796](../../../src/shell/AppShell.css#L683-L796)).
  No new CSS is required for the read/unread visual distinction itself.

## Scope

**In scope.** A complete read/unread control: mark a single message read or
unread; mark a multi-message selection read or unread; surface read-state across
the Inbox **and** user folders **and** Archive (every folder that holds received
mail); folder unread-count badges for those folders.

**Out of scope.** Sent, Outbox, and Drafts do not carry read-state — those hold
the operator's own outbound or draft messages, never "unread" received mail.
Per-row checkboxes (the standard `Ctrl`/`Shift`+click gestures plus their
keyboard equivalents cover both pointer and accessibility needs; see §1). A
dedicated "unread filter" view. Marking read/unread is local-only state and is
never transmitted on-air (RADIO-1: this design touches no transmit path).

**Alpha bar.** Per the project alpha posture, the chosen scope ships complete —
no partial slices, no operator-decision stubs.

## Vocabulary (referenced throughout — read once)

- **Read-state.** A per-message boolean. *Read* ⇔ the `.read` sidecar exists;
  *unread* ⇔ it is absent. Independent of which folder the message lives in.
- **Received mail.** Messages in the Inbox, in Archive, or in a user folder.
  Only received mail carries read-state. Sent/Outbox/Drafts are excluded.
- **The open message.** The single message shown in the reading pane. Tracked by
  `selectedId` (one id or null). Driven by a plain row click. Unchanged by this
  design except for the auto-read fix in §1.4.
- **The selection set.** The set of messages targeted by a bulk action. Tracked
  by a new `selectedIds` (a `Set<string>`). Built with `Ctrl`/`Shift`+click and
  keyboard equivalents. Orthogonal to the open message: adding a message to the
  selection set neither opens it nor marks it read.
- **The bulk action bar.** The control strip that replaces the message-list
  sort header while the selection set is non-empty. Carries the bulk
  *Mark read* / *Mark unread* / *Clear* actions.

## §1 — Multi-select UX (the bulk path)

### §1.1 — Selection gestures (OS-standard, Windows-native)

The selection set is built with the desktop-standard list multi-select gestures.
These are learned conventions, not memorized shortcuts; the audience is
Windows-first, so the modifier is `Ctrl` (rendered `⌘` on macOS via the platform
check).

| Gesture | Effect |
|---|---|
| `Click` | Open the message in the reading pane. **Clears** the selection set and sets the selection anchor to this message. (The open gesture, unchanged.) |
| `Ctrl` + `Click` | Toggle the clicked message in/out of the selection set. Does **not** open it. |
| `Shift` + `Click` | Select the contiguous range from the selection anchor to the clicked message. |
| `Ctrl` + `A` | Select every message in the active folder list. |
| `Esc` | Clear the selection set. |

The selection anchor is the most recent message added by a plain `Click` or
`Ctrl`+`Click`. `Shift`+`Click` selects the range between the anchor and the
target over the list's **active sort order**.

### §1.2 — Keyboard equivalents (accessibility parity)

Every selection operation has a keyboard path, so no pointer-only gesture is
load-bearing:

| Key | Effect |
|---|---|
| `↑` / `↓` | Move row focus (roving tabindex; rows are already `tabIndex={0}`). |
| `Shift` + `↑` / `↓` | Extend the selection set to the newly focused row. |
| `Space` | Toggle the focused row in/out of the selection set. |
| `Enter` | Open the focused message in the reading pane. |
| `Ctrl` + `A` / `Esc` | Select all / clear (as above). |

This requires a deliberate change to the existing row key handler — see §5.2.

### §1.3 — The bulk action bar

The selection set is populated **only** by the explicit multi-select gestures of
§1.1–§1.2; a plain `Click` opens a message and clears the set, so the bulk action
bar never appears merely from reading a message. While the selection set is
non-empty, the message-list sort header
([src/shell/AppShell.css:587](../../../src/shell/AppShell.css#L587)) is replaced
**in place** by the bulk action bar. The bar adds no vertical real estate, never
floats over the reading pane, and spans only the message-list pane width.

Layout, left → right: `N selected` (an `aria-live="polite"` count) · **Mark
read** (primary) · **Mark unread** · a right-aligned **Clear ✕**.
`role="toolbar"`. The bulk action bar carries the read/unread verbs only; bulk
*Move* / *Archive* of a selection is a separate feature (out of scope — see
"Out of scope"). The bar cross-fades to the sort header when the selection set
returns to empty; there is no separate "selection mode" to exit — an empty
selection set is the resting state.

**Mark read** writes the `.read` sidecar for every message in the selection set;
**Mark unread** removes it. Both operate via one batch command (§3.2). After the
action, the affected rows re-render (dots extinguish or reappear, bold/dim
flips), the folder unread badge updates, and the selection set is retained (the
operator may act again) until cleared.

### §1.4 — The open message vs. the selection set (and the auto-read fix)

The selection set (`selectedIds`) and the open message (`selectedId`) are
independent state. The selection-set gestures (`Ctrl`/`Shift`+click, `Space`,
`Ctrl`+`A`) never swap the reading pane; the reading pane always reflects
`selectedId` — the last message opened by a plain `Click`. Selection feedback
lives in the bulk action bar's `N selected` count, not the reading pane. A
*"N selected"* reading-pane placeholder is intentionally omitted so one message
stays readable while a selection is assembled.

**Auto-read fix.** Opening a message marks it read on every render of the
reading-pane query for an Inbox message ([src/mailbox/useMessage.ts:86-96](../../../src/mailbox/useMessage.ts#L86-L96)),
which would fight an explicit *Mark unread* on the open message. The auto-mark is
narrowed to fire **once per open transition** (when `selectedId` changes to a
message), not on every query settle. An explicit *Mark unread* on the open
message therefore sticks; the message re-reads only on the next distinct open.

## §2 — Single-message affordance

A single message does not require the selection set. Two entry points:

1. **Context menu.** A *Mark as read* / *Mark as unread* item is added to the
   existing right-click menu ([src/mailbox/MessageContextMenu.tsx](../../../src/mailbox/MessageContextMenu.tsx)),
   above the *Move to* group. The label reflects the message's current
   read-state: an unread message offers *Mark as read*; a read message offers
   *Mark as unread*. The item is hidden (not merely disabled) for folders that do
   not carry read-state (Sent/Outbox/Drafts).
2. **Keyboard shortcut.** A single unmodified key toggles the read-state of the
   focused (or open) message when the message list holds keyboard focus (proposed:
   `U`), consistent with the no-memorized-chord posture for basic operations. The
   exact key is validated against the existing keymap and the menu-model contract
   during implementation to avoid a collision.

Both entry points call the same single-message command (§3.1).

## §3 — Backend changes

### §3.1 — `mark_unread`

Add `Mailbox::mark_unread(folder, id)` mirroring `mark_read`
([src-tauri/src/native_mailbox.rs:191-211](../../../src-tauri/src/native_mailbox.rs#L191-L211)):
remove the `<mid>.read` sidecar if present; no-op-safe when the sidecar or the
message is absent; fire the `update_unread` index hook with `unread = true`.

Expose a single Tauri command `message_set_read_state(folder, id, read: bool)`
that dispatches to `mark_read` or `mark_unread`. Follow the established command
shape ([src-tauri/src/ui_commands.rs:743-758](../../../src-tauri/src/ui_commands.rs#L743-L758),
`mailbox_move`) and register it in the invoke handler
([src-tauri/src/lib.rs:249](../../../src-tauri/src/lib.rs#L249)).

### §3.2 — Batch command

Add `message_set_read_state_bulk(items: Vec<MessageRefDto>, read: bool)` for the
selection-set path, where each item carries `{ folder, id }`. It applies the §3.1
per-message operation to each item, resilient to individual missing messages
(best-effort, matching the existing mailbox tolerance). One command call per bulk
action keeps frontend round-trips bounded regardless of selection size. Carrying
a folder per item — rather than one folder for the whole batch — keeps the
command correct for a cross-folder search-results list, where a single list mixes
folders (`showFolderTag` mode); the frontend supplies each row's own
`message.folder ?? activeFolder`.

### §3.3 — Surface unread for user folders and Archive (un-defer Phase 2.5)

Read-state must surface wherever received mail lives. Change the user-folder and
Archive listing paths to compute `unread` from the `.read` sidecar (the same
predicate the Inbox uses, [src-tauri/src/native_mailbox.rs:107-112](../../../src-tauri/src/native_mailbox.rs#L107-L112)),
i.e. pass `surface_unread = true` to the shared list routine for `list_user`
([src-tauri/src/native_mailbox.rs:354-360](../../../src-tauri/src/native_mailbox.rs#L354-L360))
and for the Archive listing. Sent/Outbox/Drafts keep `surface_unread = false`.

### §3.4 — Unread counts for user folders and Archive

Extend the unread-count computation ([src/shell/AppShell.tsx:336-344](../../../src/shell/AppShell.tsx#L336-L344))
so user folders and Archive report `messages.filter(m => m.unread).length`, the
same derivation the Inbox uses, and the folder sidebar
([src/mailbox/FolderSidebar.tsx:46-49](../../../src/mailbox/FolderSidebar.tsx#L46-L49))
renders those badges. The counts are derived client-side from the already-fetched
folder lists; no new backend count query is introduced (the deferred
`user_folders_list_with_counts` N+1 optimization remains out of scope — the
existing per-folder lists already carry `unread` once §3.3 lands).

## §4 — Frontend changes

### §4.1 — Selection-set state

`MessageList` ([src/mailbox/MessageList.tsx](../../../src/mailbox/MessageList.tsx))
gains a `selectedIds: Set<string>` plus a selection-anchor id and the gesture
handlers of §1.1–§1.2. The selection set is owned by `MessageList` and cleared on
folder change. Bulk *Mark read* / *Mark unread* are raised to the parent via
callback props, mirroring the existing `onMoveMessage` / `onArchiveMessage` shape
([src/mailbox/MessageList.tsx:264-268](../../../src/mailbox/MessageList.tsx#L264-L268)),
so `AppShell` performs the §3 command call and the `['mailbox']` invalidation.
Plain `onSelect` continues to drive `selectedId` (the open message) and clears
the selection set.

### §4.2 — Row rendering

`MessageRow` ([src/mailbox/MessageList.tsx:139-232](../../../src/mailbox/MessageList.tsx#L139-L232))
gains a `selected` (in-selection-set) flag distinct from the existing `selected`
(is-open) prop — rename to avoid the collision (e.g. `isOpen` vs `inSelection`).
A row in the selection set renders the accent-soft background plus a 3px left
accent bar (reuse the `.selected` treatment from
[src/shell/AppShell.css:696-702](../../../src/shell/AppShell.css#L696-L702),
generalized). The unread dot and bold/dim treatment are untouched: selection is
visually distinct from read-state. The row stays draggable for single-message
move (unchanged); bulk move-by-drag is out of scope — a drag of a selected row
moves only the dragged row, and the existing `TUXLINK_DRAG_MIME` payload
([src/mailbox/MessageList.tsx:132](../../../src/mailbox/MessageList.tsx#L132)) is
unchanged.

### §4.3 — Bulk action bar component

A new `MessageBulkBar` renders the §1.3 strip. It mounts in the message-list
header slot when `selectedIds.size > 0`. The single-message context-menu item
(§2) is added to `MessageContextMenu`
([src/mailbox/MessageContextMenu.tsx](../../../src/mailbox/MessageContextMenu.tsx)).
Both surfaces call the §3 commands and invalidate the `['mailbox']` query, the
established cache pattern ([src/shell/AppShell.tsx:336-357](../../../src/shell/AppShell.tsx#L336-L357)).

## §5 — Two code truths that ride along

### §5.1 — Header render gate widens

The message-list header renders only when `onSortStateChange` is supplied
([src/mailbox/MessageList.tsx:323](../../../src/mailbox/MessageList.tsx#L323)).
The bulk action bar must render whenever the selection set is non-empty,
independent of whether the sort control is present. The gate becomes
`onSortStateChange || selectedIds.size > 0`, and the header slot renders the
sort control, the bulk bar, or nothing accordingly.

### §5.2 — `Space` is repurposed from open to select-toggle

The row key handler fires `onSelect` (open) on **both** `Enter` and `Space`
([src/mailbox/MessageList.tsx:177-182](../../../src/mailbox/MessageList.tsx#L177-L182)).
`Space` is narrowed to toggle the focused row in the selection set (the standard
grid/listbox semantic); `Enter` retains open. This is a keyboard-contract change
and **must** ship atomically with the gesture wiring and a contract test
(§6) — otherwise a build where the wiring lags would leave `Space` a silent
no-op, and the project CI gate (full vitest) would flag a contract drift.

## §6 — Test plan

**Rust unit tests** (extend [src-tauri/src/native_mailbox.rs](../../../src-tauri/src/native_mailbox.rs)
inline tests, which already cover `mark_read`):

- `mark_unread` removes the sidecar; a marked-read-then-unread message reports
  `unread: true`.
- `mark_unread` on a missing message / missing sidecar is a no-op, not an error.
- `mark_unread` updates the `unread` column in the search index.
- A user-folder message reports `unread: true` until marked read (regression for
  §3.3); Sent/Outbox/Drafts still report `unread: false`.
- Read-state round-trips when a message moves between Inbox and a user folder
  (the sidecar already travels; assert `unread` surfaces correctly post-move).
- `message_set_read_state_bulk` flips every listed `{folder, id}` item, including
  a batch that mixes folders; tolerates a missing id in the batch.

**Frontend tests (vitest):**

- Gesture unit tests on `MessageList`: `Ctrl`+click toggles selection without
  opening; `Shift`+click selects a range; plain click opens **and clears** the
  selection set (so a single open never raises the bulk bar); `Ctrl`+`A` selects
  all; `Esc` clears.
- `Space` toggles the focused row's selection and does **not** open;
  `Enter` opens (the §5.2 contract — include in the menu/keymap contract test
  the CI gate runs, so a wiring regression fails loudly).
- The bulk action bar appears at `selectedIds.size > 0` and disappears at 0, and
  renders even when no sort handler is present (the §5.1 gate).
- *Mark read* / *Mark unread* (single and bulk) call the right command with the
  right ids and invalidate the mailbox query.
- The context-menu item label reflects the message read-state and is absent in
  Sent/Outbox/Drafts.
- Folder unread badges render for user folders and Archive (§3.4).
- The reading pane keeps reflecting the open message (`selectedId`) while a
  selection set is assembled; selection-set gestures do not swap the pane (§1.4).
- App-level mount test exercising the production wiring (per the project rule to
  test the production mount path, not only the unit in scaffolding).

**Browser smoke (WebKitGTK via grim, not Chromium):** verify the selection
gestures, the `Ctrl`+click vs plain-click routing, the bulk bar layout fit, and
the row hit-testing render correctly in the real WebKitGTK runtime — CSS
specificity and click routing do not surface under jsdom or Chromium. Restart
`tauri dev` to load changes (Ctrl+R is a no-op).

## Out of scope (explicit)

- Per-row checkboxes; a "select mode" toggle; rubber-band drag-select.
- Bulk *Move* / *Archive* of a selection, and bulk move-by-drag — this feature's
  bulk path is read/unread only; bulk move is a separate follow-up.
- Read-state on Sent/Outbox/Drafts.
- An unread-only filter or saved view.
- The `user_folders_list_with_counts` N+1 backend count query (the client-side
  derivation suffices once §3.3 surfaces `unread`).
- Any transmit-path change (RADIO-1 — read/unread is local state only).

## Mockups

Approved interaction (standard multi-select, Windows-native, clean rows):
`standard-multiselect.html`, under this session's local mockup tree
`.superpowers/brainstorm/3505202-1780964073/content/` (gitignored scratch, not
committed). The design-panel exploration that informed the gesture-vs-affordance
decision is retained alongside it for reference.
