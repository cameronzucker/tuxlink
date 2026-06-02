# User folders — unified Archive + custom-folder design

**Date:** 2026-06-02
**Author:** pika-cedar-tanager
**bd issues:** tuxlink-ca5x (Archive wiring, Phase 1) · tuxlink-f62f (custom user folders, Phase 2)
**Status:** Accepted 2026-06-02 — all recommendations in D1–D10 stand.
**Mock companion:** [docs/design/mockups/2026-06-02-user-folders-mocks.html](../../design/mockups/2026-06-02-user-folders-mocks.html) — high-fidelity dark mock; sidebar variants A/B/C; Move-to picker; new-folder + delete dialogs; decisions table.

---

## 1. Premise

Archive and custom user folders are **one mechanism**, not two. Both are movable mailbox destinations that hold messages with no protocol-side semantics — the backend just stores files on disk. The frontend currently renders Archive disabled with a `soon` badge and has no concept of user folders at all.

This spec unifies them.

### 1.1 Why "unified" matters

If Archive is "system folder" and custom folders are a separate mechanism, the codebase carries two parallel concepts through every list / move / search / index call — the existing `MailboxFolder` enum plus a new `UserFolderId` type. Every UI affordance has to handle both shapes. Every backend method has to dispatch on which kind. Cameron has stated a preference for plumbing-class fixes that don't introduce unnecessary architecture (memory: `feedback_no_ceremony_spiral_on_small_fixes`); the unified concept is the smaller architecture.

### 1.2 What the backend already has

Critical finding from this brainstorm — the backend **already supports Archive end-to-end**:

- `winlink_backend::MailboxFolder::Archive` exists in the enum ([src-tauri/src/winlink_backend.rs:34](../../../src-tauri/src/winlink_backend.rs#L34))
- `ui_commands::parse_folder("archive")` returns `Ok(Archive)` ([src-tauri/src/ui_commands.rs:165](../../../src-tauri/src/ui_commands.rs#L165))
- `native_mailbox.rs` has an `archive/` directory and `Mailbox::move_to` works for all folder pairs
- `mailbox_list` Tauri command accepts `"archive"`

The Archive folder is **frontend-blocked only.** Phase 1 (tuxlink-ca5x) is roughly 10 files of frontend change + one thin Tauri command (`mailbox_move`).

Phase 2 (tuxlink-f62f) is the architectural work — extending the closed enum to an open string-named set.

---

## 2. Concept (recommended: variant B from the mock)

### 2.1 Sidebar structure

```
MAILBOX                       ← system folders: closed set
  ▣ Inbox
  ▢ Sent
  ▢ Outbox
  ▢ Drafts

FOLDERS                  +    ← user folders: open set, + creates new
  ▢ Archive
  ▢ <user folder 1>
  ▢ <user folder 2>
  ...

CONNECTIONS                   ← unchanged
  ▾ VHF / Packet
  ...
```

The "Folders" section header includes a persistent `+` button that opens the new-folder dialog. The first install seeds `Archive` as the only user folder; subsequent folders are operator-created.

**Why this placement?** See decision D1 in the mock. The Connections section already uses a labeled accordion-style heading; "Folders" mirrors that. The `+` button stays visible regardless of list length. Archive sits among user folders, which enforces the unified mental model: Archive isn't special; it's just the default-seeded user folder.

### 2.2 What's special about Archive

Nothing structural. Archive is:

- The default-seeded user folder on first install (so `A` keyboard shortcut and "Archive" button have a target out of the box).
- A **reserved display name** at creation time — `New folder…` rejects "Archive" if it would collide with the existing one.

That's it. A user who deletes Archive can recreate it. The Reading-pane "Archive" button targets whatever user folder currently has the slug `archive`; if it doesn't exist, the button is disabled with a tooltip.

---

## 3. Data model

### 3.1 Backend: directories + metadata sidecar

The on-disk store stays directory-keyed (matches today):

```
<mailbox_root>/
  inbox/                        ← system folder (closed set)
    <mid>.b2f
    <mid>.read
  outbox/                       ← system
  sent/                         ← system
  archive/                      ← user folder; seeded at first launch
    <mid>.b2f
  ares-drills/                  ← user folder (slug)
    <mid>.b2f
  .folders.json                 ← user-folder registry
```

`.folders.json`:

```json
{
  "version": 1,
  "folders": [
    { "slug": "archive",     "display_name": "Archive",     "created_at": "2026-06-02T00:00:00Z" },
    { "slug": "ares-drills", "display_name": "ARES Drills", "created_at": "2026-06-02T18:14:00Z" }
  ]
}
```

**Why directory-keyed + sidecar?**

- Listing folders = read `.folders.json` (one file). No filesystem walk per startup.
- Move = filesystem rename (same as today's `Mailbox::move_to`).
- Rename = edit the JSON (slug stays stable; only display name changes). No mass file move.
- Delete = remove the dir + remove the JSON entry.
- Recovery: if `.folders.json` is missing, rebuild it from the directory listing with display names = slugs (or, with a lower-case convention, prompt the user to assign display names).
- Search index keys on slug → folder lookup remains a simple map.

### 3.2 Rust types

Extend the `MailboxFolder` enum to carry a user-folder slug:

```rust
// src-tauri/src/winlink_backend.rs
pub enum MailboxFolder {
    Inbox,
    Sent,
    Outbox,
    User(String),  // slug; "archive" is just a slug, no longer an enum variant
}
```

**Breaking change.** `MailboxFolder::Archive` goes away as a variant; it becomes `MailboxFolder::User("archive".into())`. The dir-name mapping (currently `"archive"` in `as_path_segment`) moves into the `User` arm.

`parse_folder` ([src-tauri/src/ui_commands.rs:160](../../../src-tauri/src/ui_commands.rs#L160)) extends:

```rust
pub fn parse_folder(folder: &str) -> Result<MailboxFolder, UiError> {
    match folder {
        "inbox"  => Ok(MailboxFolder::Inbox),
        "outbox" => Ok(MailboxFolder::Outbox),
        "sent"   => Ok(MailboxFolder::Sent),
        slug if is_valid_user_slug(slug) => Ok(MailboxFolder::User(slug.into())),
        _ => Err(UiError::Internal { detail: "unknown folder".into() }),
    }
}
```

The `is_valid_user_slug` predicate checks: lowercase, 1–40 chars, `[a-z0-9-]+`, no leading dash. It does NOT check existence — that's the registry's job, surfaced as `BackendError::NotFound` from the storage layer.

### 3.3 TypeScript types

```ts
// src/mailbox/types.ts
export type SystemFolder = 'inbox' | 'outbox' | 'sent' | 'drafts';
export type UserFolderSlug = string;
export type MailboxFolder = SystemFolder | UserFolderSlug;

export interface UserFolder {
  slug: UserFolderSlug;
  displayName: string;
  createdAt: string; // RFC 3339
  /// Optional count for badge rendering; undefined → loading or n/a.
  count?: number;
}
```

**Question for the operator:** keep `drafts` and `deleted` (currently disabled) in `SystemFolder` or drop them? Recommend: keep `drafts` (local-store only, already wired); drop `deleted` (was a placeholder; no longer needed once user-folder delete-with-cascade is in).

### 3.4 New Tauri commands

| Command | Args | Returns | Notes |
|---|---|---|---|
| `user_folders_list` | — | `UserFolder[]` | Cached frontend-side with TanStack Query; refetched on create/rename/delete. |
| `folder_create` | `display_name: string` | `UserFolder` | Validates, sanitizes to slug, persists to `.folders.json`, creates the directory. |
| `folder_rename` | `slug: string, display_name: string` | `UserFolder` | Updates display name only. Slug stays stable so messages don't have to move. |
| `folder_delete` | `slug: string, on_messages: 'move_inbox' \| 'move_archive' \| 'delete'` | `()` | Refuses if `archive` and `on_messages` would orphan; refuses unknown slug. |
| `mailbox_move` | `from: string, to: string, mid: string` | `()` | Thin wrapper around `Mailbox::move_to`. Required for both Phases. |

`mailbox_list` already accepts a folder string; the slug extension is transparent on the wire (the type signature changes but Tauri's invoke is just `string`).

---

## 4. Frontend wiring

### 4.1 FolderSidebar restructure

`MAILBOX_ITEMS` becomes two things:

```ts
const SYSTEM_ITEMS = [
  { id: 'inbox',  label: 'Inbox',  icon: '▣' },
  { id: 'sent',   label: 'Sent',   icon: '▢' },
  { id: 'outbox', label: 'Outbox', icon: '▢' },
  { id: 'drafts', label: 'Drafts', icon: '▢' },
];
```

The "Folders" section is rendered dynamically from `useUserFolders()` (new hook wrapping `user_folders_list`). The `+` button opens the new-folder dialog. Right-click on a user folder opens its context menu (rename / mark all read / empty / delete).

### 4.2 useMailbox hook

`BACKEND_FOLDERS` expands to include any slug. The simplest fix:

```ts
const SYSTEM_BACKEND_FOLDERS: ReadonlySet<SystemFolder> = new Set(['inbox', 'outbox', 'sent']);

export function isBackendFolder(folder: MailboxFolder): boolean {
  // System folder OR any user-folder slug (we don't enumerate them; the backend
  // returns NotFound for unknown slugs, which the hook surfaces as an empty list).
  return SYSTEM_BACKEND_FOLDERS.has(folder as SystemFolder) || folder !== 'drafts';
}
```

Or, more explicitly, take the user-folder list from `useUserFolders()` and union it. Either works; explicit is probably clearer.

### 4.3 Reading-pane toolbar

Add (between Forward and Move-to):

- **Archive** button — calls `mailbox_move(currentFolder, 'archive', mid)`. Disabled if no `archive` folder exists in the registry.
- **Move to ▾** dropdown — opens the same submenu structure as the right-click Move-to (mock §"Message context menu").

Keyboard shortcuts:

- `A` — Archive currently-selected message (when message-list or reading-pane has focus).
- `V` — Open Move-to picker.
- `⌫` — Delete (already specced; not part of this work).

### 4.4 New components

- `NewFolderDialog` — single text input, validation, Create/Cancel.
- `RenameFolderDialog` — same shape, prefilled with current display name.
- `DeleteFolderDialog` — radio choice (mock §"Delete folder dialog").
- `MoveToPicker` — used by Reading-pane dropdown AND right-click context menu. Recent folders + all folders sections, + New folder entry at bottom that flows into NewFolderDialog with the move queued.
- `FolderContextMenu` — right-click on a user folder.
- `MessageContextMenu` — extended with the Move-to submenu.

---

## 5. Search interaction

`ChipStrip` already emits `FOLDER:value` tokens ([src/search/ChipStrip.tsx:18](../../../src/search/ChipStrip.tsx#L18)). For user folders:

- `folder:archive` resolves on slug — works today (or will, once the slug exists).
- `folder:ares-drills` resolves on slug.
- `folder:"ARES Drills"` resolves on display name (matched case-insensitive against the registry).

The search index ([src-tauri/src/search/](../../../src-tauri/src/search/)) currently stores the folder string on each message row; extending this to handle slugs requires no schema change (the column is already a `TEXT`). Renaming a folder doesn't reindex (slug is stable); deleting a folder + `on_messages='delete'` removes rows; `move_inbox` / `move_archive` updates rows via the existing `update_folder` path.

---

## 6. Decisions (settled 2026-06-02)

Operator approved the recommendation column wholesale. The full decision table is in the mock; the load-bearing settled outcomes are:

- **D1** — Sidebar placement: **B** (separate "Folders" section, `+` button on the heading).
- **D2** — Archive: pre-seeded user folder; reserved name at creation only.
- **D3** — Flat structure (no nesting in v1).
- **D4** — Backend registry: `<root>/.folders.json` sidecar.
- **D5** — Move via right-click context menu + `A` shortcut for Archive; multi-select + drag-drop deferred to Phase 3.
- **D6** — Delete non-empty folder: prompt with radio choice (move-to-inbox default).
- **D7** — Slug: `[a-z0-9-]+`, 1–40 chars; display name 3–40 chars; reserved names: Inbox/Sent/Outbox/Drafts.
- **D8** — Search: slug match + quoted display-name match against the FOLDER: token.
- **D9** — User folders show total count (matching Sent), not unread.
- **D10** — Compose target = Drafts only for v1; folder-target on send deferred.

---

## 7. Implementation sequencing

### Phase 0 — this spec (≈ now)

Decisions D1–D10 settled. This document approved. PR opens with only this spec + the mock.

### Phase 1 — tuxlink-ca5x (Archive wiring)

Scope: get Archive working without the open-set refactor. Decisions D2 and D7 still settled, but `MailboxFolder` stays as a closed enum (Archive becomes an enabled variant); only the frontend touchpoints change.

**Files touched** (estimated):

- `src-tauri/src/ui_commands.rs` — add `mailbox_move` command.
- `src/mailbox/types.ts` — add `'archive'` to `MailboxFolder`.
- `src/mailbox/useMailbox.ts` — add `'archive'` to `BACKEND_FOLDERS`.
- `src/mailbox/FolderSidebar.tsx` — flip Archive's `enabled: true`, drop `v01: true`.
- `src/mailbox/MessageReading.tsx` (or wherever the reading-pane toolbar lives) — add Archive button + `A` shortcut.
- Tests: `useMailbox.test.ts`, `FolderSidebar.test.tsx`, plus a Rust `mailbox_move` test in `ui_commands.rs`.

ETA: ~1 day. Stand-alone PR — does NOT depend on Phase 2.

### Phase 2 — tuxlink-f62f (custom user folders)

The open-set refactor: `MailboxFolder::User(String)`, `.folders.json` registry, `folder_create/rename/delete` commands, dynamic sidebar, Move-to picker, dialogs, folder context menu, slug validation, reserved-name list, search-index integration.

**Files touched** (estimated, in addition to Phase 1):

- `src-tauri/src/winlink_backend.rs` — `MailboxFolder` enum change.
- `src-tauri/src/native_mailbox.rs` — folder registry (load/save `.folders.json`), `list_user_folders`, `create_folder`, `delete_folder`, `rename_folder`.
- `src-tauri/src/ui_commands.rs` — new commands; `parse_folder` slug branch.
- `src/mailbox/types.ts` — `SystemFolder` / `UserFolder` / `UserFolderSlug` split.
- `src/mailbox/useMailbox.ts` — dynamic backend-folders check.
- `src/mailbox/useUserFolders.ts` — new hook.
- `src/mailbox/FolderSidebar.tsx` — system items split, user-folder rendering from hook, right-click context menu hook-up.
- `src/mailbox/NewFolderDialog.tsx`, `RenameFolderDialog.tsx`, `DeleteFolderDialog.tsx`, `MoveToPicker.tsx`, `FolderContextMenu.tsx` — new.
- `src/mailbox/MessageContextMenu.tsx` — Move-to submenu.
- Tests across the above.

ETA: ~3-5 days. Lands as a single PR or split into Phase 2a (registry + backend commands) and Phase 2b (UI).

### Phase 3 — deferred

- Multi-select move (`Shift+Click`, `Cmd/Ctrl+Click`).
- Drag-drop move (HTML5 DnD from message row onto sidebar folder).
- Nested folders.
- Draft-to-folder moves.
- Folder reorder.

File separately as bd issues if/when wanted.

---

## 8. Watched failure modes

- **Slug collision on rename.** Renaming a folder doesn't change the slug, so the existing `inbox` → `ares-drills` filesystem path doesn't churn. But if a user creates "ARES Drills", deletes it, then creates "Ares-Drills", they hit `ares-drills` again. Backend `folder_create` should treat the slug as canonical and reject duplicates regardless of display-name capitalization.
- **`Mailbox::list` for a deleted-but-still-referenced folder.** TanStack Query may have the folder in a cached `mailbox_list` request when the registry no longer knows about it. The backend returns an empty list for missing directories today; we keep that behavior. Frontend invalidates the cache on `folder_delete` success.
- **`.folders.json` corruption.** If the JSON parse fails on startup, recover by scanning the mailbox root for directory entries and reconstructing the registry with display names = slugs (with a one-time warning toast). Don't fail to start.
- **`archive` slug deleted on a system without a default.** A fresh install seeds Archive. A user who deletes it has no Archive button target. The button should disable + show a tooltip "Archive folder was deleted — create one to re-enable." Recreate via `+ New folder` → name "Archive" → the reservation check passes (since no Archive exists).
- **Search-index column for user-folder slug.** The index stores folder strings on each row. When a folder is deleted with `on_messages='delete'`, the index needs to delete those rows; `move_inbox`/`move_archive` updates them. No schema change; the existing `update_folder` path handles it.

---

## 9. Out of scope

- IMAP-style folder subscription.
- Cross-machine folder sync (single-machine app today).
- Folder-level permissions or sharing.
- Per-folder retention rules.
- Folder color coding / icons (defer to themes if asked).
- Move-on-read rules (Gmail-style filters).

---

## 10. Cross-references

- bd issues: `tuxlink-ca5x` (Phase 1), `tuxlink-f62f` (Phase 2)
- Mock companion: [docs/design/mockups/2026-06-02-user-folders-mocks.html](../../design/mockups/2026-06-02-user-folders-mocks.html)
- Existing backend touchpoints:
  - [src-tauri/src/winlink_backend.rs:34](../../../src-tauri/src/winlink_backend.rs#L34) — `MailboxFolder` enum
  - [src-tauri/src/native_mailbox.rs:145](../../../src-tauri/src/native_mailbox.rs#L145) — `Mailbox::move_to`
  - [src-tauri/src/ui_commands.rs:160](../../../src-tauri/src/ui_commands.rs#L160) — `parse_folder`
  - [src-tauri/src/ui_commands.rs:188](../../../src-tauri/src/ui_commands.rs#L188) — `mailbox_list`
- Existing frontend touchpoints:
  - [src/mailbox/types.ts:69](../../../src/mailbox/types.ts#L69) — `MailboxFolder` type
  - [src/mailbox/useMailbox.ts:20](../../../src/mailbox/useMailbox.ts#L20) — `BACKEND_FOLDERS`
  - [src/mailbox/FolderSidebar.tsx:29](../../../src/mailbox/FolderSidebar.tsx#L29) — `MAILBOX_ITEMS`
  - [src/search/ChipStrip.tsx:18](../../../src/search/ChipStrip.tsx#L18) — `FOLDER:` search token
- Related ADRs: ADR 0008 (worktrees), ADR 0010 (no squash-merge — phase 1 + phase 2 will be separate merge commits).
