# Mark Messages Read/Unread — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a complete read/unread control to tuxlink's mailbox — mark one message or a multi-message selection read/unread, with read-state surfaced across the Inbox, user folders, and Archive.

**Architecture:** The backend read-state machinery (`unread` flag, `.read` sidecar, index column) already exists for the Inbox. This plan adds a folder-ref-aware `set_read_state` primitive (covering user folders + Archive via the existing `resolve_dir` seam), surfaces unread beyond the Inbox, makes `message_read` a pure read with client-side mark-on-open, and builds the frontend selection model + bulk action bar + context-menu affordance using OS-standard `Ctrl/Shift+click`.

**Tech Stack:** Rust (Tauri commands, `async_trait` backend), TypeScript/React 19, TanStack Query, vitest + `@testing-library/react`, react-virtuoso.

**Spec:** [docs/superpowers/specs/2026-06-08-read-unread-design.md](../specs/2026-06-08-read-unread-design.md) · **bd:** tuxlink-etxt · **Worktree:** `worktrees/bd-tuxlink-etxt-read-unread` · **Branch:** `bd-tuxlink-etxt/read-unread`

**Commit trailer for every commit:**
```
Agent: opossum-lupine-magnolia
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
```

**Gate before pushing (CI parity):** `cargo test --manifest-path src-tauri/Cargo.toml`, `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` (re-run until exit 0 — it hides later-target lints), `pnpm -C . typecheck`, full `pnpm -C . vitest run`. RADIO-1: this feature touches no transmit path; no on-air gate.

---

## File Structure

**Backend (Rust):**
- `src-tauri/src/native_mailbox.rs` — add `set_read_state(&FolderRef, id, read)`; make `mark_read` a wrapper; surface unread for user folders + Archive.
- `src-tauri/src/winlink_backend.rs` — add `set_read_state` to the `WinlinkBackend` trait + `NativeBackend` impl.
- `src-tauri/src/ui_commands.rs` — add `message_set_read_state` + `message_set_read_state_bulk` commands + `MessageRefDto`; make `message_read` pure.
- `src-tauri/src/lib.rs` — register the two new commands.

**Frontend (TS/React):**
- `src/mailbox/useMessage.ts` — replace the inbox-invalidate effect with ref-guarded mark-read-on-open.
- `src/mailbox/MessageList.tsx` — selection-set state, `Ctrl/Shift+click` + keyboard gestures, header-gate widening, `isOpen` vs `inSelection` row props.
- `src/mailbox/MessageBulkBar.tsx` — **new** bulk action bar component.
- `src/mailbox/MessageContextMenu.tsx` — add the single-message Mark read/unread item.
- `src/mailbox/readState.ts` — **new** tiny helper module (`folderBearsReadState`).
- `src/shell/AppShell.tsx` — wire bulk + single + context-menu callbacks (command + `['mailbox']` invalidation); Archive unread count.
- `src/shell/AppShell.css` — `.row.in-selection` treatment (generalize `.selected`).

**Tests:** extend `src-tauri/src/native_mailbox.rs` inline tests, `src-tauri/src/search/index.rs` inline tests; `src/mailbox/useMessage.test.ts`, `src/mailbox/MessageList.test.tsx`, `src/mailbox/MessageContextMenu.test.tsx` (new), `src/mailbox/MessageBulkBar.test.tsx` (new), `src/shell/AppShell.test.tsx`.

---

## Phase 1 — Backend: read-state read + write across folders

### Task 1: `Mailbox::set_read_state` (folder-ref-aware mark read/unread)

**Files:**
- Modify: `src-tauri/src/native_mailbox.rs` (add `set_read_state` near `mark_read` at `:191`; refactor `mark_read` to delegate)
- Test: `src-tauri/src/native_mailbox.rs` (inline `#[cfg(test)] mod tests`, after `marking_a_missing_message_read_is_not_an_error` at `:664`)

- [ ] **Step 1: Write the failing tests**

Add to the test module (the existing tests use `tempdir()`, `Mailbox::new`, `store`, and a `raw(subject, body)` helper):

```rust
#[test]
fn mark_unread_removes_the_marker() {
    let dir = tempdir().unwrap();
    let mbox = Mailbox::new(dir.path());
    let id = mbox.store(MailboxFolder::Inbox, &raw("Hello", "Body")).unwrap();
    mbox.mark_read(MailboxFolder::Inbox, &id).unwrap();
    assert!(!mbox.list(MailboxFolder::Inbox).unwrap()[0].unread);

    mbox.set_read_state(&FolderRef::System(MailboxFolder::Inbox), &id, false).unwrap();

    assert!(mbox.list(MailboxFolder::Inbox).unwrap()[0].unread, "mark unread must re-surface as unread");
}

#[test]
fn set_read_state_on_missing_message_is_not_an_error() {
    let dir = tempdir().unwrap();
    let mbox = Mailbox::new(dir.path());
    let r = mbox.set_read_state(&FolderRef::System(MailboxFolder::Inbox), &MessageId::new("NOPE"), false);
    assert!(r.is_ok());
}

#[test]
fn set_read_state_works_on_a_user_folder() {
    let dir = tempdir().unwrap();
    let mbox = Mailbox::new(dir.path());
    let uf = mbox.create_user_folder("Net Traffic").unwrap();
    let id = mbox.store(MailboxFolder::Inbox, &raw("Net", "x")).unwrap();
    mbox.move_between(FolderRef::System(MailboxFolder::Inbox), FolderRef::User(uf.slug.clone()), &id).unwrap();

    mbox.set_read_state(&FolderRef::User(uf.slug.clone()), &id, true).unwrap();

    assert!(!mbox.list_user(&uf.slug).unwrap()[0].unread, "set read must clear unread in a user folder");
}
```

(If `FolderRef` is not already imported in the test module, add `use super::FolderRef;` or the existing `use super::*;` covers it — check the top of the `mod tests` block.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml set_read_state mark_unread_removes 2>&1 | tail -20`
Expected: FAIL — `no method named set_read_state`.

- [ ] **Step 3: Implement `set_read_state` and make `mark_read` delegate**

Replace the existing `mark_read` (`native_mailbox.rs:191-211`) with:

```rust
/// Set a message's read-state by adding (`read = true`) or removing
/// (`read = false`) the `<mid>.read` sidecar next to its `<mid>.b2f`.
/// Folder-ref aware: works for system folders AND user-folder slugs via
/// `resolve_dir`. Tolerant: a message with no file on disk is a no-op
/// (it may have been moved/removed between the list view and the action),
/// and removing an absent marker is not an error.
pub fn set_read_state(
    &self,
    folder: &FolderRef,
    id: &MessageId,
    read: bool,
) -> Result<(), BackendError> {
    let dir = self.resolve_dir(folder);
    if !dir.join(format!("{}.b2f", id.0)).exists() {
        return Ok(());
    }
    let marker = dir.join(format!("{}.read", id.0));
    if read {
        fs::write(&marker, [])?;
    } else {
        match fs::remove_file(&marker) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    // Best-effort index hook — filesystem write already succeeded above.
    if let Some(idx) = self.index.as_ref() {
        match idx.lock() {
            Ok(guard) => {
                if let Err(e) = guard.update_unread(&id.0, !read) {
                    eprintln!("search-index update_unread failed for mid={}: {e}", id.0);
                }
            }
            Err(e) => eprintln!("search-index lock poisoned during update_unread: {e}"),
        }
    }
    Ok(())
}

/// Mark a message read. Thin wrapper over [`Mailbox::set_read_state`] kept for
/// existing call sites. System-folder convenience signature.
pub fn mark_read(&self, folder: MailboxFolder, id: &MessageId) -> Result<(), BackendError> {
    self.set_read_state(&FolderRef::System(folder), id, true)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib native_mailbox 2>&1 | tail -25`
Expected: PASS — new tests green AND the existing `marking_an_inbox_message_read_flips_it` / `read_state_persists_across_mailbox_instances` / `mark_read_updates_unread_in_index` still pass (mark_read now delegates).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/native_mailbox.rs
git commit -m "feat(mailbox): folder-ref-aware set_read_state primitive (tuxlink-etxt)

Add Mailbox::set_read_state(&FolderRef, id, read) covering system + user
folders via resolve_dir; mark/unmark the .read sidecar + update the search
index. mark_read becomes a thin wrapper.

Agent: opossum-lupine-magnolia
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: Surface unread for user folders + Archive

**Files:**
- Modify: `src-tauri/src/native_mailbox.rs` (`list` at `:111-112`; `list_user` at `:359`)
- Test: `src-tauri/src/native_mailbox.rs` (inline tests)

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn archive_messages_surface_unread() {
    let dir = tempdir().unwrap();
    let mbox = Mailbox::new(dir.path());
    let id = mbox.store(MailboxFolder::Archive, &raw("A", "x")).unwrap();
    assert!(mbox.list(MailboxFolder::Archive).unwrap()[0].unread, "archived received mail surfaces unread");

    mbox.set_read_state(&FolderRef::System(MailboxFolder::Archive), &id, true).unwrap();
    assert!(!mbox.list(MailboxFolder::Archive).unwrap()[0].unread);
}

#[test]
fn user_folder_messages_surface_unread() {
    let dir = tempdir().unwrap();
    let mbox = Mailbox::new(dir.path());
    let uf = mbox.create_user_folder("Skywarn").unwrap();
    let id = mbox.store(MailboxFolder::Inbox, &raw("Net", "x")).unwrap();
    mbox.move_between(FolderRef::System(MailboxFolder::Inbox), FolderRef::User(uf.slug.clone()), &id).unwrap();

    assert!(mbox.list_user(&uf.slug).unwrap()[0].unread, "received mail in a user folder surfaces unread");
}
```

Also update the comment in `non_inbox_folders_never_report_unread` (`:622-624`) so it reads "Sent/Outbox" not "Sent/Outbox/Archive" (Archive now surfaces unread); the test body (Sent + Outbox only) is unchanged and still passes.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib surface_unread 2>&1 | tail -20`
Expected: FAIL — both assert `unread` is true but current code returns false for Archive and user folders.

- [ ] **Step 3: Implement the surfacing change**

In `Mailbox::list` (`native_mailbox.rs:111-112`), widen the condition:

```rust
// Unread is a received-mail concept: the Inbox and Archive (which holds
// received mail) surface it. Sent/Outbox are the operator's own messages.
meta.unread =
    matches!(folder, MailboxFolder::Inbox | MailboxFolder::Archive)
        && !path.with_extension("read").exists();
```

In `Mailbox::list_user` (`native_mailbox.rs:357-360`), flip the flag:

```rust
pub fn list_user(&self, slug: &str) -> Result<Vec<MessageMeta>, BackendError> {
    let dir = user_folders::folder_dir(&self.root, slug);
    Self::list_dir(&dir, /*surface_unread=*/ true)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib native_mailbox 2>&1 | tail -25`
Expected: PASS — new tests green, `non_inbox_folders_never_report_unread` still green.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/native_mailbox.rs
git commit -m "feat(mailbox): surface unread for user folders and Archive (tuxlink-etxt)

Un-defer Phase 2.5: received mail in user folders and Archive now reports
unread from the .read sidecar, the same predicate the Inbox uses. Sent/Outbox
stay read-less.

Agent: opossum-lupine-magnolia
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: Trait + `NativeBackend` `set_read_state`

**Files:**
- Modify: `src-tauri/src/winlink_backend.rs` (trait method near `mark_read` at `:701`; `NativeBackend` impl near `:1086`)

- [ ] **Step 1: Write the failing test**

`NativeBackend` is exercised in `src-tauri/src/winlink_backend.rs` inline tests (search for `mod tests` / `NativeBackend::new`). Add a test mirroring the existing backend-construction pattern (find a current test that builds a `NativeBackend` over a tempdir to copy the harness):

```rust
#[tokio::test]
async fn native_backend_set_read_state_round_trips() {
    let dir = tempdir().unwrap();
    let backend = native_backend_over(dir.path()); // reuse the existing test ctor helper
    // store an inbox message via the backend's mailbox, then mark read/unread.
    let id = backend.mailbox.store(MailboxFolder::Inbox, &raw_msg("Hi", "x")).unwrap();
    backend.set_read_state(FolderRef::System(MailboxFolder::Inbox), &id, true).await.unwrap();
    assert!(!backend.mailbox.list(MailboxFolder::Inbox).unwrap()[0].unread);
    backend.set_read_state(FolderRef::System(MailboxFolder::Inbox), &id, false).await.unwrap();
    assert!(backend.mailbox.list(MailboxFolder::Inbox).unwrap()[0].unread);
}
```

(Match the exact constructor + raw-message helper used by the nearby `NativeBackend` tests; names like `native_backend_over` / `raw_msg` are placeholders for whatever that file's harness already provides — copy from an adjacent test.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib set_read_state_round_trips 2>&1 | tail -20`
Expected: FAIL — `no method named set_read_state` on the trait.

- [ ] **Step 3: Add the trait method + impl**

In the `WinlinkBackend` trait, after `mark_read` (`winlink_backend.rs:701-704`), add a default no-op (mirroring `mark_read`'s best-effort default):

```rust
/// Set a message's read-state (mark read or unread). Folder-ref aware so
/// user folders and Archive are covered. Best-effort: default is a no-op.
/// `NativeBackend` overrides it to write/remove the read-marker.
async fn set_read_state(
    &self,
    _folder: crate::native_mailbox::FolderRef,
    _id: &MessageId,
    _read: bool,
) -> Result<(), BackendError> {
    Ok(())
}
```

In the `NativeBackend` impl, after `mark_read` (`winlink_backend.rs:1086-1088`), add:

```rust
async fn set_read_state(
    &self,
    folder: crate::native_mailbox::FolderRef,
    id: &MessageId,
    read: bool,
) -> Result<(), BackendError> {
    self.mailbox.set_read_state(&folder, id, read)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib winlink_backend 2>&1 | tail -25`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/winlink_backend.rs
git commit -m "feat(backend): WinlinkBackend::set_read_state trait method + native impl (tuxlink-etxt)

Agent: opossum-lupine-magnolia
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: `message_set_read_state` command + registration

**Files:**
- Modify: `src-tauri/src/ui_commands.rs` (add command near `mailbox_move` at `:1305`)
- Modify: `src-tauri/src/lib.rs` (register at the `invoke_handler` block, `:376`)

- [ ] **Step 1: Write the command (no unit test at the command layer — covered by the trait test in Task 3 and the frontend wire tests in Phase 3; commands are thin dispatch)**

Add to `ui_commands.rs` (mirror `mailbox_move`'s shape — `parse_folder_ref`, `state.current()`):

```rust
/// Set a single message's read-state. `read = true` marks read, `false` marks
/// unread. Folder may be a system folder or a user-folder slug.
#[tauri::command]
pub async fn message_set_read_state(
    folder: String,
    id: String,
    read: bool,
    state: State<'_, BackendState>,
) -> Result<(), UiError> {
    let folder_ref = parse_folder_ref(&folder)?;
    let mid = MessageId::new(&id);
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;
    backend.set_read_state(folder_ref, &mid, read).await?;
    Ok(())
}
```

- [ ] **Step 2: Register the command**

In `lib.rs` `invoke_handler!` (after `message_read` at `:376`):

```rust
            crate::ui_commands::message_read,          // Task 13 (tuxlink-y5c)
            crate::ui_commands::message_set_read_state, // tuxlink-etxt (read/unread)
```

- [ ] **Step 3: Build to verify registration + types**

Run: `cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | tail -15`
Expected: builds clean (command signature serde-compatible; `read: bool` arrives as the JS `read` arg).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/ui_commands.rs src-tauri/src/lib.rs
git commit -m "feat(commands): message_set_read_state command (tuxlink-etxt)

Agent: opossum-lupine-magnolia
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: `message_set_read_state_bulk` command + `MessageRefDto`

**Files:**
- Modify: `src-tauri/src/ui_commands.rs` (add `MessageRefDto` + the bulk command)
- Modify: `src-tauri/src/lib.rs` (register)

- [ ] **Step 1: Add the DTO + command**

In `ui_commands.rs` (near the other DTOs at the top; `MessageRefDto` carries a per-item folder so a cross-folder search-results selection is correct):

```rust
/// One message reference for a bulk operation. Each carries its own folder so a
/// cross-folder search-results selection (which mixes folders) stays correct.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct MessageRefDto {
    pub folder: String,
    pub id: String,
}
```

```rust
/// Set the read-state of every listed message. Best-effort per item: a missing
/// message is a no-op (matching the single-message path). One command call per
/// bulk action keeps frontend round-trips bounded.
#[tauri::command]
pub async fn message_set_read_state_bulk(
    items: Vec<MessageRefDto>,
    read: bool,
    state: State<'_, BackendState>,
) -> Result<(), UiError> {
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;
    for item in items {
        let folder_ref = parse_folder_ref(&item.folder)?;
        let mid = MessageId::new(&item.id);
        backend.set_read_state(folder_ref, &mid, read).await?;
    }
    Ok(())
}
```

- [ ] **Step 2: Register**

In `lib.rs` after `message_set_read_state`:

```rust
            crate::ui_commands::message_set_read_state_bulk, // tuxlink-etxt (bulk read/unread)
```

- [ ] **Step 3: Build to verify**

Run: `cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | tail -15`
Expected: builds clean.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/ui_commands.rs src-tauri/src/lib.rs
git commit -m "feat(commands): message_set_read_state_bulk batch command (tuxlink-etxt)

Per-{folder,id} items so a cross-folder search selection is correct.

Agent: opossum-lupine-magnolia
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Phase 2 — Auto-read fix (pure `message_read` + client mark-on-open)

### Task 6: Make `message_read` a pure read

**Files:**
- Modify: `src-tauri/src/ui_commands.rs` (`message_read` at `:1196-1203`)
- Test: any backend/command test asserting `message_read` mutates read-state (search `message_read` in test modules)

- [ ] **Step 1: Find and update the contract test**

Run: `grep -rn "message_read" src-tauri/src --include=*.rs | grep -iE "test|mark|unread"`
If a test asserts opening marks read **through `message_read`**, change it to assert `message_read` does NOT mutate read-state (mark-on-open moves client-side in Task 7). The `Mailbox`/`set_read_state` tests (Task 1–2) remain the read-state coverage. If no such test exists, note that and proceed.

- [ ] **Step 2: Remove the server-side mark**

Delete the mark-on-read block in `message_read` (`ui_commands.rs:1196-1203`):

```rust
    // (removed) Opening a message no longer mutates read-state server-side.
    // Mark-on-open is now a once-per-open-transition client effect (useMessage),
    // so an explicit "mark unread" on the open message is not undone by a
    // reading-pane refetch (window focus / poll). See tuxlink-etxt design §1.4.
```

So `message_read` ends at `parse_raw_rfc5322(&id, &body.raw_rfc5322)` with no side effect.

- [ ] **Step 3: Build + run backend tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml 2>&1 | tail -20`
Expected: PASS (with the Step-1 test updated).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/ui_commands.rs
git commit -m "refactor(commands): make message_read a pure read (tuxlink-etxt)

Mark-on-open moves to a once-per-open client effect so an explicit Mark Unread
on the open message is not undone by a reading-pane refetch (design §1.4).

Agent: opossum-lupine-magnolia
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 7: Client-side mark-read-on-open (once per open transition)

**Files:**
- Create: `src/mailbox/readState.ts`
- Modify: `src/mailbox/useMessage.ts` (the effect at `:86-96`)
- Test: `src/mailbox/readState.test.ts` (new), `src/mailbox/useMessage.test.ts`

- [ ] **Step 1: Write the failing tests**

`src/mailbox/readState.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { folderBearsReadState } from './readState';

describe('folderBearsReadState', () => {
  it('is true for received-mail folders', () => {
    expect(folderBearsReadState('inbox')).toBe(true);
    expect(folderBearsReadState('archive')).toBe(true);
    expect(folderBearsReadState('skywarn-net')).toBe(true); // a user-folder slug
  });
  it('is false for the operator’s own / non-received folders', () => {
    for (const f of ['sent', 'outbox', 'drafts', 'deleted']) {
      expect(folderBearsReadState(f)).toBe(false);
    }
  });
});
```

In `src/mailbox/useMessage.test.ts`, add (the file already mocks `invoke`):

```ts
it('marks the opened message read once per open transition', async () => {
  const invoke = vi.mocked(await import('@tauri-apps/api/core')).invoke;
  // render useMessage with selection {folder:'inbox', id:'M1'} and isSuccess true
  // assert invoke called with ('message_set_read_state', { folder:'inbox', id:'M1', read:true })
  // re-render with the SAME selection + a new dataUpdatedAt → assert NOT called again
});

it('does not mark read for sent/outbox/drafts', async () => {
  // selection {folder:'sent', id:'S1'}, isSuccess true → invoke('message_set_read_state', …) NOT called
});
```

(Match the existing render harness in `useMessage.test.ts` — it already constructs selections and mocks the query.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `pnpm -C . vitest run src/mailbox/readState.test.ts src/mailbox/useMessage.test.ts 2>&1 | tail -25`
Expected: FAIL — `folderBearsReadState` undefined; mark-read not called.

- [ ] **Step 3: Implement the helper + the effect**

`src/mailbox/readState.ts`:

```ts
import type { MailboxFolderRef } from './types';

/// Folders that carry read-state (received mail): Inbox, Archive, and any
/// user-folder slug. Sent/Outbox/Drafts/Deleted are the operator's own or
/// non-received messages and never track unread.
const READLESS = new Set(['sent', 'outbox', 'drafts', 'deleted']);

export function folderBearsReadState(folder: MailboxFolderRef): boolean {
  return !READLESS.has(folder);
}
```

In `useMessage.ts`, replace the effect at `:86-96` with a ref-guarded mark-on-open:

```ts
import { useEffect, useRef } from 'react';
import { folderBearsReadState } from './readState';
// ...
const markedRef = useRef<string | null>(null);
useEffect(() => {
  if (!selection || !result.isSuccess) return;
  if (!folderBearsReadState(selection.folder)) return;
  const key = `${selection.folder}/${selection.id}`;
  if (markedRef.current === key) return; // once per open transition, not per refetch
  markedRef.current = key;
  void invoke('message_set_read_state', {
    folder: selection.folder,
    id: selection.id,
    read: true,
  }).then(() => queryClient.invalidateQueries({ queryKey: ['mailbox'] }));
}, [selection?.folder, selection?.id, result.isSuccess, queryClient]);
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `pnpm -C . vitest run src/mailbox/readState.test.ts src/mailbox/useMessage.test.ts 2>&1 | tail -25`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/mailbox/readState.ts src/mailbox/readState.test.ts src/mailbox/useMessage.ts src/mailbox/useMessage.test.ts
git commit -m "feat(mailbox): mark read on open via once-per-transition client effect (tuxlink-etxt)

Routes mark-on-open through message_set_read_state for inbox/user-folder/Archive;
ref-guarded so it fires once per open, never on a refetch — making an explicit
Mark Unread on the open message stick (design §1.4).

Agent: opossum-lupine-magnolia
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Phase 3 — Frontend: selection model, bulk bar, affordances

### Task 8: Selection-set state + `Ctrl/Shift+click` gestures in `MessageList`

**Files:**
- Modify: `src/mailbox/MessageList.tsx` (`MessageRow` props at `:113-127`/`:139`; `MessageList` body)
- Modify: `src/shell/AppShell.css` (add `.layout-b .row.in-selection` near `.row.selected` at `:696`)
- Test: `src/mailbox/MessageList.test.tsx`

- [ ] **Step 1: Write the failing tests**

In `MessageList.test.tsx` (it already renders `MessageList`/`MessageRow`):

```tsx
it('Ctrl+click adds a row to the selection without opening it', async () => {
  const onSelect = vi.fn();
  const onSelectionChange = vi.fn();
  // render MessageList with messages [M1,M2,M3], selectedId=null, onSelect, onSelectionChange
  fireEvent.click(screen.getByTestId('message-row-M2'), { ctrlKey: true });
  expect(onSelect).not.toHaveBeenCalled();
  expect(onSelectionChange).toHaveBeenLastCalledWith(new Set(['M2']));
});

it('plain click opens and clears the selection set', async () => {
  // with selection {M2} already present, plain-click M1
  fireEvent.click(screen.getByTestId('message-row-M1'));
  expect(onSelect).toHaveBeenCalledWith('M1');
  expect(onSelectionChange).toHaveBeenLastCalledWith(new Set()); // cleared
});

it('Shift+click selects the contiguous range from the anchor', async () => {
  // anchor M1 (ctrl+click), then shift+click M3 → {M1,M2,M3} over the sorted order
  fireEvent.click(screen.getByTestId('message-row-M1'), { ctrlKey: true });
  fireEvent.click(screen.getByTestId('message-row-M3'), { shiftKey: true });
  expect(onSelectionChange).toHaveBeenLastCalledWith(new Set(['M1', 'M2', 'M3']));
});
```

- [ ] **Step 2: Run to verify fail**

Run: `pnpm -C . vitest run src/mailbox/MessageList.test.tsx 2>&1 | tail -25`
Expected: FAIL — no `onSelectionChange` prop / gesture handling.

- [ ] **Step 3: Implement the selection model**

In `MessageList.tsx`:
1. `MessageRowProps`: rename the existing `selected` to `isOpen`; add `inSelection: boolean`. Update the `className` builder (`:167`) to `['row', message.unread?'unread':'', isOpen?'selected':'', inSelection?'in-selection':'']`. Update the row click handler to delegate modifier detection to the parent:

```tsx
onClick={(e) => onRowClick(message.id, { ctrl: e.ctrlKey || e.metaKey, shift: e.shiftKey })}
```

2. `MessageListProps`: add `selectedIds: Set<string>` and `onSelectionChange: (next: Set<string>) => void` (and keep `selectedId`/`onSelect`). Maintain an `anchorId` ref inside `MessageList`.
3. `onRowClick(id, mods)` logic:

```tsx
const anchorRef = React.useRef<string | null>(null);
const onRowClick = (id: string, mods: { ctrl: boolean; shift: boolean }) => {
  if (mods.shift && anchorRef.current) {
    const ids = sortedMessages.map((m) => m.id);
    const a = ids.indexOf(anchorRef.current);
    const b = ids.indexOf(id);
    if (a !== -1 && b !== -1) {
      const [lo, hi] = a < b ? [a, b] : [b, a];
      onSelectionChange(new Set(ids.slice(lo, hi + 1)));
      return;
    }
  }
  if (mods.ctrl) {
    const next = new Set(selectedIds);
    next.has(id) ? next.delete(id) : next.add(id);
    anchorRef.current = id;
    onSelectionChange(next);
    return;
  }
  // plain click: open + clear selection set
  anchorRef.current = id;
  if (selectedIds.size > 0) onSelectionChange(new Set());
  onSelect(id);
};
```

4. Pass `inSelection={selectedIds.has(msg.id)}` and `isOpen={msg.id === selectedId}` to each `MessageRow` (`:337-346`).

In `AppShell.css`, after `.layout-b .row.selected` (`:696-702`):

```css
.layout-b .row.in-selection { background: rgba(245, 159, 60, 0.10); position: relative; }
.layout-b .row.in-selection::before {
  content: ''; position: absolute; left: 0; top: 0; bottom: 0; width: 3px; background: var(--accent);
}
```

- [ ] **Step 4: Run to verify pass**

Run: `pnpm -C . vitest run src/mailbox/MessageList.test.tsx 2>&1 | tail -25`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/mailbox/MessageList.tsx src/shell/AppShell.css src/mailbox/MessageList.test.tsx
git commit -m "feat(mailbox): selection set + Ctrl/Shift+click multi-select in MessageList (tuxlink-etxt)

Agent: opossum-lupine-magnolia
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 9: Keyboard model — `Space` toggles, `Enter` opens (contract change + contract test)

**Files:**
- Modify: `src/mailbox/MessageList.tsx` (`MessageRow` `onKeyDown` at `:177-182`)
- Test: `src/mailbox/MessageList.test.tsx`

- [ ] **Step 1: Write the failing contract test**

```tsx
it('keyboard contract: Enter opens, Space toggles selection (does not open)', () => {
  const onSelect = vi.fn();
  const onSelectionChange = vi.fn();
  // render; focus message-row-M2
  fireEvent.keyDown(screen.getByTestId('message-row-M2'), { key: 'Enter' });
  expect(onSelect).toHaveBeenCalledWith('M2');

  onSelect.mockClear();
  fireEvent.keyDown(screen.getByTestId('message-row-M2'), { key: ' ' });
  expect(onSelect).not.toHaveBeenCalled();          // Space no longer opens
  expect(onSelectionChange).toHaveBeenLastCalledWith(new Set(['M2'])); // Space toggles
});
```

- [ ] **Step 2: Run to verify fail**

Run: `pnpm -C . vitest run src/mailbox/MessageList.test.tsx -t "keyboard contract" 2>&1 | tail -20`
Expected: FAIL — Space currently calls `onSelect` (open).

- [ ] **Step 3: Implement the keymap change**

Replace `MessageRow`'s `onKeyDown` (`MessageList.tsx:177-182`):

```tsx
onKeyDown={(e) => {
  if (e.key === 'Enter') {
    e.preventDefault();
    onSelect(message.id);          // Enter opens
  } else if (e.key === ' ') {
    e.preventDefault();
    onToggleSelect(message.id);    // Space toggles selection (grid/listbox semantic)
  }
}}
```

Thread an `onToggleSelect(id)` prop from `MessageList` that does the ctrl-click toggle logic (reuse `onRowClick(id, { ctrl: true, shift: false })`).

- [ ] **Step 4: Run to verify pass**

Run: `pnpm -C . vitest run src/mailbox/MessageList.test.tsx 2>&1 | tail -25`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/mailbox/MessageList.tsx src/mailbox/MessageList.test.tsx
git commit -m "feat(mailbox): Space toggles selection, Enter opens — keyboard contract (tuxlink-etxt)

Narrows the row key handler so Space owns selection-toggle (standard listbox
semantic); shipped with a contract test so a wiring regression fails CI loudly.

Agent: opossum-lupine-magnolia
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 10: `MessageBulkBar` component + header-gate widening

**Files:**
- Create: `src/mailbox/MessageBulkBar.tsx`
- Modify: `src/mailbox/MessageList.tsx` (header slot at `:322-327`)
- Test: `src/mailbox/MessageBulkBar.test.tsx` (new), `src/mailbox/MessageList.test.tsx`

- [ ] **Step 1: Write the failing tests**

`MessageBulkBar.test.tsx`:

```tsx
it('renders the count and fires read/unread/clear callbacks', () => {
  const onMarkRead = vi.fn(), onMarkUnread = vi.fn(), onClear = vi.fn();
  render(<MessageBulkBar count={3} onMarkRead={onMarkRead} onMarkUnread={onMarkUnread} onClear={onClear} />);
  expect(screen.getByText(/3 selected/i)).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: /mark read/i })); expect(onMarkRead).toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: /mark unread/i })); expect(onMarkUnread).toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: /clear/i })); expect(onClear).toHaveBeenCalled();
});
```

In `MessageList.test.tsx`:

```tsx
it('shows the bulk bar when a selection exists, even with no sort handler', () => {
  // render MessageList with selectedIds={new Set(['M1','M2'])} and NO onSortStateChange
  expect(screen.getByRole('toolbar', { name: /selection actions/i })).toBeInTheDocument();
});
it('hides the bulk bar when the selection is empty', () => {
  // selectedIds=new Set(); onSortStateChange provided → sort control shows, no toolbar
  expect(screen.queryByRole('toolbar', { name: /selection actions/i })).toBeNull();
});
```

- [ ] **Step 2: Run to verify fail**

Run: `pnpm -C . vitest run src/mailbox/MessageBulkBar.test.tsx src/mailbox/MessageList.test.tsx 2>&1 | tail -25`
Expected: FAIL.

- [ ] **Step 3: Implement the bar + the gate**

`src/mailbox/MessageBulkBar.tsx`:

```tsx
export interface MessageBulkBarProps {
  count: number;
  onMarkRead: () => void;
  onMarkUnread: () => void;
  onClear: () => void;
}
export function MessageBulkBar({ count, onMarkRead, onMarkUnread, onClear }: MessageBulkBarProps) {
  return (
    <div className="message-bulk-bar" role="toolbar" aria-label="Selection actions" data-testid="message-bulk-bar">
      <span className="bulk-count" aria-live="polite">{count} selected</span>
      <span className="bulk-spacer" />
      <button type="button" className="bulk-btn primary" onClick={onMarkRead}>Mark read</button>
      <button type="button" className="bulk-btn" onClick={onMarkUnread}>Mark unread</button>
      <button type="button" className="bulk-btn clear" aria-label="Clear selection" onClick={onClear}>✕</button>
    </div>
  );
}
```

In `MessageList.tsx`, widen the header gate (`:323`) and render the bar:

```tsx
{(onSortStateChange || selectedIds.size > 0) && (
  <div className="rows-pane-header" data-testid="rows-pane-header">
    {selectedIds.size > 0 ? (
      <MessageBulkBar
        count={selectedIds.size}
        onMarkRead={() => onBulkSetReadState?.(selectedIds, true)}
        onMarkUnread={() => onBulkSetReadState?.(selectedIds, false)}
        onClear={() => onSelectionChange(new Set())}
      />
    ) : (
      onSortStateChange && <MessageListSortControl value={sortState} onChange={onSortStateChange} />
    )}
  </div>
)}
```

Add `onBulkSetReadState?: (ids: Set<string>, read: boolean) => void` to `MessageListProps`. Add bulk-bar CSS to `AppShell.css` (reuse the `.rows-pane-header` strip; the bar is `display:flex; align-items:center; gap:8px`).

- [ ] **Step 4: Run to verify pass**

Run: `pnpm -C . vitest run src/mailbox/MessageBulkBar.test.tsx src/mailbox/MessageList.test.tsx 2>&1 | tail -25`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/mailbox/MessageBulkBar.tsx src/mailbox/MessageBulkBar.test.tsx src/mailbox/MessageList.tsx src/mailbox/MessageList.test.tsx src/shell/AppShell.css
git commit -m "feat(mailbox): bulk action bar replacing the sort header on selection (tuxlink-etxt)

Agent: opossum-lupine-magnolia
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 11: Wire selection + bulk callbacks in `AppShell`

**Files:**
- Modify: `src/shell/AppShell.tsx` (selection state; `<MessageList>` mount at `:933-946`)
- Test: `src/shell/AppShell.test.tsx`

- [ ] **Step 1: Write the failing App-level test (production mount path)**

```tsx
it('bulk Mark read invokes the batch command with per-folder items and refreshes', async () => {
  const invoke = vi.mocked(await import('@tauri-apps/api/core')).invoke;
  // render <AppShell/> on the Inbox with ≥2 messages; ctrl+click two rows; click Mark read
  // assert invoke called with ('message_set_read_state_bulk',
  //   { items: [{folder:'inbox', id:'M1'}, {folder:'inbox', id:'M2'}], read:true })
});
```

- [ ] **Step 2: Run to verify fail**

Run: `pnpm -C . vitest run src/shell/AppShell.test.tsx -t "bulk Mark read" 2>&1 | tail -20`
Expected: FAIL.

- [ ] **Step 3: Implement the wiring**

In `AppShell.tsx`: add `const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());` cleared on `selectedFolder` change (effect). Add the bulk handler:

```tsx
const bulkSetReadState = useCallback(async (ids: Set<string>, read: boolean) => {
  const byId = new Map(visibleMessages.map((m) => [m.id, m] as const));
  const items = [...ids].map((id) => ({
    folder: (byId.get(id)?.folder as string | undefined) ?? selectedFolder,
    id,
  }));
  await invoke('message_set_read_state_bulk', { items, read });
  void queryClient.invalidateQueries({ queryKey: ['mailbox'] });
}, [visibleMessages, selectedFolder, queryClient]);
```

Pass to `<MessageList>` (`:933-946`): `selectedIds={selectedIds}`, `onSelectionChange={setSelectedIds}`, `onBulkSetReadState={bulkSetReadState}`.

- [ ] **Step 4: Run to verify pass**

Run: `pnpm -C . vitest run src/shell/AppShell.test.tsx 2>&1 | tail -25`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/shell/AppShell.tsx src/shell/AppShell.test.tsx
git commit -m "feat(shell): wire selection set + bulk read/unread in AppShell (tuxlink-etxt)

Agent: opossum-lupine-magnolia
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 12: Single-message context-menu item (Mark as read/unread)

**Files:**
- Modify: `src/mailbox/MessageContextMenu.tsx`
- Modify: `src/mailbox/MessageList.tsx` (pass an `onSetReadState` through to the menu) and `src/shell/AppShell.tsx` (handler)
- Test: `src/mailbox/MessageContextMenu.test.tsx` (new)

- [ ] **Step 1: Write the failing tests**

```tsx
it('offers Mark as read for an unread message and Mark as unread for a read one', () => {
  const onSetReadState = vi.fn();
  const unread = { ...baseMsg, unread: true };
  const { rerender } = render(<MessageContextMenu message={unread} folder="inbox" x={0} y={0} userFolders={[]} onSetReadState={onSetReadState} onMoveTo={vi.fn()} onArchive={vi.fn()} onClose={vi.fn()} />);
  fireEvent.click(screen.getByRole('menuitem', { name: /mark as read/i }));
  expect(onSetReadState).toHaveBeenCalledWith(true);

  rerender(<MessageContextMenu message={{ ...baseMsg, unread: false }} folder="inbox" x={0} y={0} userFolders={[]} onSetReadState={onSetReadState} onMoveTo={vi.fn()} onArchive={vi.fn()} onClose={vi.fn()} />);
  expect(screen.getByRole('menuitem', { name: /mark as unread/i })).toBeInTheDocument();
});

it('omits the read/unread item for folders without read-state', () => {
  render(<MessageContextMenu message={{ ...baseMsg, unread: false }} folder="sent" x={0} y={0} userFolders={[]} onSetReadState={vi.fn()} onMoveTo={vi.fn()} onArchive={vi.fn()} onClose={vi.fn()} />);
  expect(screen.queryByRole('menuitem', { name: /mark as (read|unread)/i })).toBeNull();
});
```

- [ ] **Step 2: Run to verify fail**

Run: `pnpm -C . vitest run src/mailbox/MessageContextMenu.test.tsx 2>&1 | tail -20`
Expected: FAIL.

- [ ] **Step 3: Implement the menu item**

In `MessageContextMenu.tsx`: add `onSetReadState: (read: boolean) => void` to the props. Above the "Move to" label (`:89`), render (only when `folderBearsReadState(folder)`):

```tsx
{folderBearsReadState(folder) && (
  <>
    <button
      type="button" role="menuitem" className="tux-ctx-item"
      data-testid="ctx-set-read-state"
      onClick={actAndClose(() => onSetReadState(message.unread))}
    >
      {message.unread ? 'Mark as read' : 'Mark as unread'}
    </button>
    <div className="tux-ctx-separator" />
  </>
)}
```

(`onSetReadState(message.unread)`: an unread message → `read = true`; a read message → `read = false`.) Import `folderBearsReadState` from `./readState`. Thread `onSetReadState` from `MessageList` (it already mounts `MessageContextMenu` at `:351-366`) up to `AppShell`, where the handler is:

```tsx
const setMessageReadState = useCallback(async (id: string, folder: MailboxFolderRef, read: boolean) => {
  await invoke('message_set_read_state', { folder, id, read });
  void queryClient.invalidateQueries({ queryKey: ['mailbox'] });
}, [queryClient]);
```

- [ ] **Step 4: Run to verify pass**

Run: `pnpm -C . vitest run src/mailbox/MessageContextMenu.test.tsx src/mailbox/MessageList.test.tsx 2>&1 | tail -25`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/mailbox/MessageContextMenu.tsx src/mailbox/MessageContextMenu.test.tsx src/mailbox/MessageList.tsx src/shell/AppShell.tsx
git commit -m "feat(mailbox): single-message Mark as read/unread in the context menu (tuxlink-etxt)

Agent: opossum-lupine-magnolia
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 13: Keyboard shortcut `U` (toggle read-state of the focused/open message)

**Files:**
- Modify: `src/mailbox/MessageList.tsx` (row `onKeyDown`, alongside the Task-9 handler)
- Test: `src/mailbox/MessageList.test.tsx`

- [ ] **Step 1: Confirm no keymap collision, then write the failing test**

Run: `grep -rn "key === 'u'\|key === 'U'\|'KeyU'\|\bU\b" src/shell src/mailbox --include=*.tsx | grep -i key` — confirm `U` is free in the message-list focus context. If taken, pick an alternative and update the spec §2 note. Then:

```tsx
it('U toggles the focused message read-state', () => {
  const onSetReadState = vi.fn();
  // render with onRowSetReadState; focus message-row-M1 (unread)
  fireEvent.keyDown(screen.getByTestId('message-row-M1'), { key: 'u' });
  expect(onSetReadState).toHaveBeenCalledWith('M1', true); // unread → read
});
```

- [ ] **Step 2: Run to verify fail**

Run: `pnpm -C . vitest run src/mailbox/MessageList.test.tsx -t "U toggles" 2>&1 | tail -20`
Expected: FAIL.

- [ ] **Step 3: Implement**

Extend the `MessageRow` `onKeyDown` (from Task 9):

```tsx
} else if (e.key === 'u' || e.key === 'U') {
  e.preventDefault();
  onRowSetReadState(message.id, message.unread); // unread → read(true); read → unread(false)
}
```

Thread `onRowSetReadState(id, read)` from `MessageList` → reuse the same `AppShell` `setMessageReadState` handler from Task 12 (the row carries `message.folder ?? folder`).

- [ ] **Step 4: Run to verify pass**

Run: `pnpm -C . vitest run src/mailbox/MessageList.test.tsx 2>&1 | tail -25`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/mailbox/MessageList.tsx src/mailbox/MessageList.test.tsx
git commit -m "feat(mailbox): U keyboard shortcut toggles message read-state (tuxlink-etxt)

Agent: opossum-lupine-magnolia
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 14: Archive unread count badge (user-folder badge deferred)

**Files:**
- Modify: `src/shell/AppShell.tsx` (`counts` memo at `:446-455`)
- Test: `src/shell/AppShell.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
it('the Archive folder badge counts unread archived messages', async () => {
  // render <AppShell/> with archive messages where 2 are unread
  // assert the archive folder count badge reads 2 (data-testid="folder-count-archive")
});
```

- [ ] **Step 2: Run to verify fail**

Run: `pnpm -C . vitest run src/shell/AppShell.test.tsx -t "Archive folder badge" 2>&1 | tail -20`
Expected: FAIL — archive count is total, not unread.

- [ ] **Step 3: Implement**

In the `counts` memo (`AppShell.tsx:446-455`), change archive from total to unread:

```tsx
archive: archive.messages.filter((m) => m.unread).length,
```

(Inbox already filters unread. **User-folder count badges are intentionally NOT added here** — AppShell fetches only the selected folder's messages, so per-user-folder counts would require the deferred `user_folders_list_with_counts` N+1 query, which is out of scope per spec §3.4. User-folder unread still surfaces in-list via Task 2. Add a one-line code comment saying so.)

- [ ] **Step 4: Run to verify pass**

Run: `pnpm -C . vitest run src/shell/AppShell.test.tsx 2>&1 | tail -25`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/shell/AppShell.tsx src/shell/AppShell.test.tsx
git commit -m "feat(shell): Archive folder badge counts unread (tuxlink-etxt)

User-folder count badges stay deferred (need the N+1 query); user-folder unread
still surfaces in-list.

Agent: opossum-lupine-magnolia
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Phase 4 — Verification

### Task 15: Full gate + WebKitGTK/grim browser smoke

**Files:** none (verification only)

- [ ] **Step 1: Backend gate**

Run: `cargo test --manifest-path src-tauri/Cargo.toml 2>&1 | tail -15` → all pass.
Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings 2>&1 | tail -15` → re-run until exit 0 (it hides later-target lints once a target fails).

- [ ] **Step 2: Frontend gate**

Run: `pnpm -C . typecheck 2>&1 | tail -15` → clean.
Run: `pnpm -C . vitest run 2>&1 | tail -20` → all files pass (watch for the menu-model / contract tests the CI gate runs that scoped runs miss).
Reap any leaked workers: `pkill -9 -f vitest 2>/dev/null; pgrep -f vitest || echo "clean"`.

- [ ] **Step 3: WebKitGTK browser smoke (operator-run, per browser-smoke-before-ship)**

Restart `tauri dev` (Ctrl+R is a no-op for changed code) and walk: open an inbox message (marks read; dot clears); right-click → Mark as unread (sticks — dot returns and stays after a window-focus round-trip); `Ctrl+click` several rows + Shift+click a range (bulk bar shows "N selected"); Mark read / Mark unread (dots flip, Archive/Inbox badges update); `Esc` clears; verify a user folder shows unread dots and the toggle works there. Capture fidelity via grim (NOT Chromium — WebKitGTK clips differently). This step is operator-gated; agents stage it runnable + observable.

- [ ] **Step 4: Push + open PR**

```bash
git push
gh pr create --base main --head bd-tuxlink-etxt/read-unread \
  --title "[opossum-lupine-magnolia] feat: mark messages read/unread (tuxlink-etxt)" \
  --body "Implements docs/superpowers/specs/2026-06-08-read-unread-design.md. See plan docs/superpowers/plans/2026-06-08-read-unread.md."
```

---

## Self-Review (run against the spec)

- **§1.1 gestures** → Task 8 (Ctrl/Shift+click, plain-click-clears). **§1.2 keyboard** → Task 9 (Space/Enter) + Task 13 (U). **§1.3 bulk bar** → Task 10. **§1.4 open-vs-selection + auto-read fix** → Task 6 (pure read) + Task 7 (client mark-on-open) + Task 8 (selection orthogonal). **§2 single-message** → Task 12 (menu) + Task 13 (shortcut). **§3.1 mark_unread** → Task 1. **§3.2 batch** → Task 5. **§3.3 surface user-folder/Archive unread** → Task 2 + Task 3. **§3.4 counts** → Task 14 (Archive; user-folder deferred, documented). **§4 frontend** → Tasks 8/10/11/12. **§5.1 header gate** → Task 10. **§5.2 Space contract + test** → Task 9. **§6 tests** → each task's tests + Task 15 gate + grim smoke. Every spec section maps to a task.
- **Placeholder scan:** the only hand-offs to the executing engineer are the Rust test-harness ctor names in Task 3 (`native_backend_over`/`raw_msg`) — flagged explicitly to copy from the adjacent existing test — and the keymap-collision check in Task 13. No silent TODOs.
- **Type consistency:** `set_read_state(&FolderRef, &MessageId, bool)` (Mailbox) vs `set_read_state(FolderRef, &MessageId, bool)` (trait/Native — owned `FolderRef` to match `move_between_folders`); `message_set_read_state` args `{folder, id, read}` and bulk `{items:[{folder,id}], read}` match the `invoke(...)` call sites in Tasks 7/11/12; `onSelectionChange(Set<string>)`, `onBulkSetReadState(Set,bool)`, `onSetReadState(bool)`, `onRowSetReadState(id,bool)`, `folderBearsReadState` are used consistently across Tasks 8–14; `isOpen`/`inSelection` row props replace the overloaded `selected`.
