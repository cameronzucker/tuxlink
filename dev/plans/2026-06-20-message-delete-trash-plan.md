# Message Delete + Trash Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Delete for messages in every mailbox folder, routing to a recoverable Trash (Deleted) folder with Restore, Empty Trash, per-item permanent delete, and time-based auto-purge.

**Architecture:** A delete is a `move_to` into a new `Deleted` system folder plus a `{mid}.trash` sidecar recording origin folder, origin identity, and deletion time. Restore reads the sidecar and moves back. Permanent purge reuses the existing unlink + search-index-drop path. Each operation threads through the existing layers: `Mailbox` (storage) → `WinlinkBackend` trait → `NativeBackend` → Tauri command → TS binding → React component.

**Tech Stack:** Rust (Tauri backend, `serde_json`, `chrono`), React 18 + TypeScript (Vite), the existing `native_mailbox` store + `ui_commands` command surface.

**Spec:** `docs/design/2026-06-20-message-delete-trash-design.md`

## Global Constraints

- MSRV 1.75 (`src-tauri/Cargo.toml`); clippy `-D warnings` with `incompatible_msrv` denied — no API stabilized after 1.75.
- This Pi does not finish a cold cargo build; Rust compiles on CI only. `pnpm vitest run` is runnable locally on single files.
- `MailboxFolder` is `#[non_exhaustive]` (`winlink_backend.rs:34`) — adding a variant does not break external matches, but in-crate `match` sites must add the arm.
- Conventional commits; every commit carries the `Agent:` trailer (the executing session's moniker) + the `Co-Authored-By` trailer.
- Auto-purge: retention default **30 days**, configurable in Settings, **on by default**.
- Delete-to-Trash and Restore have **no** confirm dialog; Empty Trash and per-item permanent Delete **require** confirm.
- Worktree commit discipline: a standalone `cd <worktree>` before each `git` op (the main-checkout-race hook reads the payload cwd).

---

## File Structure

**Backend (`src-tauri/src/`):**
- `winlink_backend.rs` — add `MailboxFolder::Deleted` + `as_path` arm; add trait methods `delete_message_in`, `restore_message`, `empty_trash`, `purge_message`, `purge_expired_trash` to the `WinlinkBackend` trait + `NativeBackend` impls.
- `native_mailbox.rs` — `TrashMeta` struct + sidecar read/write; `Mailbox::{delete_message, restore_message, purge_message, empty_trash, purge_expired}`; `folder_dir` Deleted mapping; pure `trash_is_expired` selector.
- `ui_commands.rs` — `message_delete(_bulk)`, `message_restore(_bulk)`, `trash_empty`, `trash_purge_one` commands.
- `config.rs` — `trash_retention_days: u32` + `trash_auto_purge: bool` config fields (defaults 30 / true).
- `lib.rs` — register the new commands in `invoke_handler!`; start the auto-purge sweep in `.setup()`.

**Frontend (`src/`):**
- `mailbox/types.ts` — enable `'deleted'` in `MailboxFolder`; add command-wrapper + DTO types.
- `mailbox/mailboxCommands.ts` (or the existing command-wrapper module) — `deleteMessages`, `restoreMessages`, `emptyTrash`, `purgeMessage` `invoke` wrappers.
- `mailbox/MessageContextMenu.tsx`, `MessageView.tsx`, `MessageBulkBar.tsx`, `FolderSidebar.tsx` — wire the actions (folder-dependent).
- `mailbox/useMailbox.ts` — include `'deleted'` in the folder set.
- `shell/SettingsPanel.tsx` (or the mailbox settings section) — auto-purge toggle + day count.
- A confirm modal reusing the `DeleteFolderDialog.tsx` pattern for the permanent actions.

**Docs:** `docs/user-guide/07-mailbox-model.md`.

---

## Task 1: Add the `Deleted` system folder

**Files:**
- Modify: `src-tauri/src/winlink_backend.rs:34` (enum) + `:38-46` (`as_path`)
- Modify: `src-tauri/src/native_mailbox.rs` (`folder_dir`/`folder_dir_ns` + any exhaustive `match folder` over `MailboxFolder` that must gain a `Deleted` arm — grep `match.*folder` / `MailboxFolder::` to find them; e.g. the unread-default match at `:180`/`:240`)
- Modify: `src/mailbox/types.ts:87` (the TS `MailboxFolder` already lists `'deleted'`; confirm it is not filtered out as disabled in the folder lists)

**Interfaces:**
- Produces: `MailboxFolder::Deleted` with `as_path() == "deleted"`; `folder_dir(Deleted)` resolves a shared `root/deleted/` directory (shared like Sent/Outbox, NOT per-identity — see spec). Deleted messages default to **read** (never surface an unread badge on Trash): the unread-default matches that currently read `matches!(folder, Inbox | Archive)` stay false for `Deleted`.

- [ ] **Step 1: Write the failing test** (in `winlink_backend.rs` tests)

```rust
#[test]
fn deleted_folder_maps_to_deleted_path() {
    assert_eq!(MailboxFolder::Deleted.as_path(), "deleted");
}
```

- [ ] **Step 2: Run to verify it fails** — `cargo test --manifest-path src-tauri/Cargo.toml deleted_folder_maps_to_deleted_path` (CI; locally it will fail to compile: no `Deleted` variant). Expected: compile error / FAIL.

- [ ] **Step 3: Add the variant + path**

```rust
// winlink_backend.rs
pub enum MailboxFolder { Inbox, Sent, Outbox, Archive, Deleted }
// in as_path():
MailboxFolder::Deleted => "deleted",
```

- [ ] **Step 4: Add the `Deleted` arm to every in-crate exhaustive match.** Grep `MailboxFolder::Archive =>` and `matches!(folder, MailboxFolder::Inbox | MailboxFolder::Archive)` in `native_mailbox.rs`; for the unread-default matches, do NOT add `Deleted` (trash is read). Add `folder_dir` resolution for `Deleted` as a shared (non-namespaced) dir, mirroring how Sent/Outbox resolve (read `folder_dir_ns` at `:587` + `folder_dir` at `:601`).

- [ ] **Step 5: Run tests** — `cargo test … winlink_backend` + `cargo clippy … --all-targets -- -D warnings`. Expected: PASS, no warnings.

- [ ] **Step 6: Commit** — `git add -A && git commit -m "feat(mailbox): add Deleted system folder (tuxlink-wl7n)"`

---

## Task 2: `TrashMeta` sidecar (origin + identity + timestamp)

**Files:**
- Modify: `src-tauri/src/native_mailbox.rs` (add `TrashMeta` near the other DTOs; add `write_trash_sidecar`/`read_trash_sidecar` helpers next to the `.identity` sidecar helpers around `:297-310`)

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  pub struct TrashMeta { pub origin: String, pub origin_full: Option<String>, pub deleted_at: String }
  ```
  `origin` = the origin folder's `as_path()` or user-folder slug; `origin_full` = the origin identity FULL when the origin is per-identity (Inbox/Archive/user), else `None`; `deleted_at` = RFC3339 UTC. Helpers: `fn write_trash_sidecar(dir: &Path, mid: &str, m: &TrashMeta) -> io::Result<()>` writes `{mid}.trash` as pretty JSON; `fn read_trash_sidecar(dir: &Path, mid: &str) -> Option<TrashMeta>` (None on missing/garbage).

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn trash_meta_round_trips_through_sidecar() {
    let dir = tempfile::tempdir().unwrap();
    let m = TrashMeta { origin: "inbox".into(), origin_full: Some("N0CALL".into()), deleted_at: "2026-06-20T18:30:00Z".into() };
    write_trash_sidecar(dir.path(), "ABCD1234", &m).unwrap();
    assert_eq!(read_trash_sidecar(dir.path(), "ABCD1234"), Some(m));
    // Shared-folder origin: no identity.
    let m2 = TrashMeta { origin: "sent".into(), origin_full: None, deleted_at: "2026-06-20T18:31:00Z".into() };
    write_trash_sidecar(dir.path(), "EF", &m2).unwrap();
    assert_eq!(read_trash_sidecar(dir.path(), "EF"), Some(m2));
    // Missing / garbage → None.
    assert_eq!(read_trash_sidecar(dir.path(), "NOPE"), None);
    fs::write(dir.path().join("BAD.trash"), b"{not json").unwrap();
    assert_eq!(read_trash_sidecar(dir.path(), "BAD"), None);
}
```

- [ ] **Step 2: Run to verify it fails** (CI) — FAIL: `TrashMeta`/helpers undefined.

- [ ] **Step 3: Implement**

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrashMeta {
    pub origin: String,
    #[serde(default)]
    pub origin_full: Option<String>,
    pub deleted_at: String,
}

fn write_trash_sidecar(dir: &Path, mid: &str, m: &TrashMeta) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(m).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    fs::write(dir.join(format!("{mid}.trash")), json)
}

fn read_trash_sidecar(dir: &Path, mid: &str) -> Option<TrashMeta> {
    let raw = fs::read(dir.join(format!("{mid}.trash"))).ok()?;
    serde_json::from_slice(&raw).ok()
}
```

- [ ] **Step 4: Run** (CI) — PASS.
- [ ] **Step 5: Commit** — `feat(mailbox): TrashMeta sidecar for delete origin + timestamp (tuxlink-wl7n)`

---

## Task 3: `Mailbox::delete_message` (move to Trash + sidecar)

**Files:**
- Modify: `src-tauri/src/native_mailbox.rs` (new method after `move_to` at `:315`)

**Interfaces:**
- Consumes: `move_to` (Task 1's `Deleted` folder), `TrashMeta` + `write_trash_sidecar` (Task 2).
- Produces: `pub fn delete_message(&self, from: MailboxFolder, id: &MessageId, origin_full: Option<&str>, now_rfc3339: &str) -> Result<(), BackendError>`. Moves `{mid}.b2f` (+ carried sidecars) from `from` to `Deleted` via `move_to`, then writes a `{mid}.trash` sidecar in the `Deleted` dir with `{origin: from.as_path(), origin_full, deleted_at: now}`. `now_rfc3339` is injected (not `chrono::Utc::now()` inline) so the unit test is deterministic; the command layer passes `chrono::Utc::now().to_rfc3339()`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn delete_message_moves_to_trash_and_writes_sidecar() {
    let mb = test_mailbox(); // existing test helper that builds a Mailbox over a tempdir
    let id = MessageId::new("MID01");
    store_test_message(&mb, MailboxFolder::Inbox, &id); // helper: writes the .b2f
    mb.delete_message(MailboxFolder::Inbox, &id, Some("N0CALL"), "2026-06-20T00:00:00Z").unwrap();
    // Gone from Inbox, present in Deleted.
    assert!(!mb.folder_dir(MailboxFolder::Inbox).join("MID01.b2f").exists());
    assert!(mb.folder_dir(MailboxFolder::Deleted).join("MID01.b2f").exists());
    // Sidecar records origin.
    let meta = read_trash_sidecar(&mb.folder_dir(MailboxFolder::Deleted), "MID01").unwrap();
    assert_eq!(meta.origin, "in");
    assert_eq!(meta.origin_full.as_deref(), Some("N0CALL"));
    assert_eq!(meta.deleted_at, "2026-06-20T00:00:00Z");
}
```
(If `test_mailbox`/`store_test_message` helpers do not exist, add them mirroring the existing `move_to` tests in `native_mailbox.rs` — search the test module for how it constructs a `Mailbox` + writes a `.b2f`.)

- [ ] **Step 2: Run to verify it fails** (CI) — FAIL.
- [ ] **Step 3: Implement**

```rust
pub fn delete_message(
    &self,
    from: MailboxFolder,
    id: &MessageId,
    origin_full: Option<&str>,
    now_rfc3339: &str,
) -> Result<(), BackendError> {
    self.move_to(from, MailboxFolder::Deleted, id)?;
    let meta = TrashMeta {
        origin: from.as_path().to_string(),
        origin_full: origin_full.map(str::to_string),
        deleted_at: now_rfc3339.to_string(),
    };
    write_trash_sidecar(&self.folder_dir(MailboxFolder::Deleted), &id.0, &meta)?;
    Ok(())
}
```

- [ ] **Step 4: Run** (CI) — PASS.
- [ ] **Step 5: Commit** — `feat(mailbox): Mailbox::delete_message → Trash + origin sidecar (tuxlink-wl7n)`

---

## Task 4: `Mailbox::restore_message` (Trash → origin)

**Files:** Modify `src-tauri/src/native_mailbox.rs`.

**Interfaces:**
- Consumes: `read_trash_sidecar` (Task 2), `move_to` + a namespaced move for per-identity origins.
- Produces: `pub fn restore_message(&self, id: &MessageId) -> Result<(), BackendError>`. Reads `{mid}.trash` from `Deleted`; resolves the destination folder from `origin` (`"in"|"sent"|"out"|"archive"` → `MailboxFolder`, else a user-folder slug) under `origin_full`; moves it back; removes the `.trash` sidecar. Missing/dangling sidecar or unknown origin → restore to the active identity's Inbox. Add a pure helper `fn parse_origin_folder(origin: &str) -> Option<MailboxFolder>` (returns None for user-folder slugs, which route through the user-folder move path).

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn restore_returns_message_to_recorded_origin() {
    let mb = test_mailbox();
    let id = MessageId::new("MID02");
    store_test_message(&mb, MailboxFolder::Archive, &id);
    mb.delete_message(MailboxFolder::Archive, &id, None, "2026-06-20T00:00:00Z").unwrap();
    mb.restore_message(&id).unwrap();
    assert!(mb.folder_dir(MailboxFolder::Archive).join("MID02.b2f").exists());
    assert!(!mb.folder_dir(MailboxFolder::Deleted).join("MID02.b2f").exists());
    assert!(!mb.folder_dir(MailboxFolder::Deleted).join("MID02.trash").exists());
}

#[test]
fn restore_without_sidecar_falls_back_to_inbox() {
    let mb = test_mailbox();
    let id = MessageId::new("MID03");
    // Put a bare message in Deleted with no .trash sidecar.
    store_test_message(&mb, MailboxFolder::Deleted, &id);
    mb.restore_message(&id).unwrap();
    assert!(mb.folder_dir(MailboxFolder::Inbox).join("MID03.b2f").exists());
}
```

- [ ] **Step 2: Run to verify it fails** (CI) — FAIL.
- [ ] **Step 3: Implement** the method + `parse_origin_folder`. Use `move_to(Deleted, dest, id)` for system-folder origins; for a user-folder slug origin, route through the existing user-folder move path (find it via `move_to_user_folder`/`FolderRef::User` in `native_mailbox.rs`). Apply `origin_full` by targeting that identity's namespaced folder (mirror `for_identity(...)`/`folder_dir_ns`). After a successful move, `fs::remove_file(self.folder_dir(MailboxFolder::Deleted).join(format!("{}.trash", id.0)))` (ignore NotFound).

```rust
fn parse_origin_folder(origin: &str) -> Option<MailboxFolder> {
    match origin {
        "in" => Some(MailboxFolder::Inbox),
        "sent" => Some(MailboxFolder::Sent),
        "out" => Some(MailboxFolder::Outbox),
        "archive" => Some(MailboxFolder::Archive),
        _ => None, // user-folder slug
    }
}
```

- [ ] **Step 4: Run** (CI) — PASS.
- [ ] **Step 5: Commit** — `feat(mailbox): Mailbox::restore_message from Trash to origin (tuxlink-wl7n)`

---

## Task 5: Permanent purge (`purge_message`, `empty_trash`)

**Files:** Modify `src-tauri/src/native_mailbox.rs`.

**Interfaces:**
- Consumes: the existing `index_delete(mid)` (used by the cascade at `:815`).
- Produces: `pub fn purge_message(&self, id: &MessageId) -> Result<(), BackendError>` — unlink `{mid}.b2f` + `{mid}.read` + `{mid}.identity` + `{mid}.trash` from the `Deleted` dir (each ignore-NotFound) and call `self.index_delete(&id.0)`. `pub fn empty_trash(&self) -> Result<usize, BackendError>` — `purge_message` for every `*.b2f` in the `Deleted` dir; returns the count purged.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn purge_removes_message_and_all_sidecars() {
    let mb = test_mailbox();
    let id = MessageId::new("MID04");
    store_test_message(&mb, MailboxFolder::Inbox, &id);
    mb.delete_message(MailboxFolder::Inbox, &id, None, "2026-06-20T00:00:00Z").unwrap();
    mb.purge_message(&id).unwrap();
    let d = mb.folder_dir(MailboxFolder::Deleted);
    assert!(!d.join("MID04.b2f").exists());
    assert!(!d.join("MID04.trash").exists());
}

#[test]
fn empty_trash_purges_all_and_counts() {
    let mb = test_mailbox();
    for n in ["A1","B2","C3"] {
        let id = MessageId::new(n);
        store_test_message(&mb, MailboxFolder::Inbox, &id);
        mb.delete_message(MailboxFolder::Inbox, &id, None, "2026-06-20T00:00:00Z").unwrap();
    }
    assert_eq!(mb.empty_trash().unwrap(), 3);
    assert_eq!(std::fs::read_dir(mb.folder_dir(MailboxFolder::Deleted)).unwrap().filter_map(|e| e.ok()).filter(|e| e.path().extension().map_or(false, |x| x=="b2f")).count(), 0);
}
```

- [ ] **Step 2: Run to verify it fails** (CI) — FAIL.
- [ ] **Step 3: Implement** both methods (unlink loop + `index_delete`). Enumerate `*.b2f` in the Deleted dir for `empty_trash`.
- [ ] **Step 4: Run** (CI) — PASS.
- [ ] **Step 5: Commit** — `feat(mailbox): permanent purge_message + empty_trash (tuxlink-wl7n)`

---

## Task 6: Auto-purge selector + sweep

**Files:** Modify `src-tauri/src/native_mailbox.rs`.

**Interfaces:**
- Produces: pure `pub fn trash_is_expired(deleted_at_rfc3339: &str, now: chrono::DateTime<chrono::Utc>, retention_days: i64) -> bool` (true iff `now - deleted_at >= retention_days`; a malformed timestamp → false, never auto-purge an unparseable item). And `pub fn purge_expired(&self, now: chrono::DateTime<chrono::Utc>, retention_days: i64) -> Result<usize, BackendError>` — for each `*.b2f` in Deleted, read its `.trash` sidecar and `purge_message` when `trash_is_expired`; returns the count.

- [ ] **Step 1: Write the failing test**

```rust
use chrono::{TimeZone, Utc};
#[test]
fn trash_is_expired_respects_retention_window() {
    let now = Utc.with_ymd_and_hms(2026, 7, 20, 0, 0, 0).unwrap();
    assert!(trash_is_expired("2026-06-20T00:00:00Z", now, 30));  // exactly 30 days → expired (inclusive)
    assert!(trash_is_expired("2026-05-01T00:00:00Z", now, 30));
    assert!(!trash_is_expired("2026-07-10T00:00:00Z", now, 30)); // 10 days → kept
    assert!(!trash_is_expired("garbage", now, 30));              // unparseable → keep
}
```

- [ ] **Step 2: Run to verify it fails** (CI) — FAIL.
- [ ] **Step 3: Implement** `trash_is_expired` (parse RFC3339 via `chrono::DateTime::parse_from_rfc3339`, compare `now - parsed >= Duration::days(retention_days)`) and `purge_expired` (enumerate + read sidecar + purge).
- [ ] **Step 4: Run** (CI) — PASS.
- [ ] **Step 5: Commit** — `feat(mailbox): trash auto-purge selector + purge_expired sweep (tuxlink-wl7n)`

---

## Task 7: `WinlinkBackend` trait + `NativeBackend` methods

**Files:** Modify `src-tauri/src/winlink_backend.rs` (the `WinlinkBackend` trait + the `NativeBackend` impl that already has `move_between_folders`).

**Interfaces:**
- Consumes: Tasks 3–6 on `Mailbox`.
- Produces, on the `WinlinkBackend` trait (with default no-op/`unimplemented`-style bodies matching the trait's existing default pattern, then real `NativeBackend` impls):
  `async fn delete_message_in(&self, from: FolderRef, id: &MessageId, origin_full: Option<&str>) -> Result<(), BackendError>`,
  `async fn restore_message(&self, id: &MessageId) -> Result<(), BackendError>`,
  `async fn empty_trash(&self) -> Result<usize, BackendError>`,
  `async fn purge_message(&self, id: &MessageId) -> Result<usize, BackendError>` (0/1),
  `async fn purge_expired_trash(&self, retention_days: i64) -> Result<usize, BackendError>`.
  `NativeBackend` impls call the `Mailbox` methods, passing `chrono::Utc::now().to_rfc3339()` for delete and `chrono::Utc::now()` for purge. Mirror exactly how `move_between_folders` resolves `FolderRef` → `MailboxFolder` and dispatches.

- [ ] **Step 1: Write the failing test** — mirror an existing `NativeBackend` move test (find `move_between_folders` tests in `winlink_backend.rs`): delete a stored message via `delete_message_in`, assert it is gone from the source folder list and present in `mailbox_list("deleted")`.
- [ ] **Step 2: Run to verify it fails** (CI) — FAIL.
- [ ] **Step 3: Implement** the trait methods + `NativeBackend` impls.
- [ ] **Step 4: Run** (CI) — PASS + clippy clean (new trait methods need impls on every `WinlinkBackend` implementor; give the trait defaults so test/mocks compile).
- [ ] **Step 5: Commit** — `feat(backend): delete/restore/purge on WinlinkBackend + NativeBackend (tuxlink-wl7n)`

---

## Task 8: Tauri commands

**Files:** Modify `src-tauri/src/ui_commands.rs` (after `mailbox_move`/`message_move_bulk` at `:1426`/`:1496`) + register in `src-tauri/src/lib.rs` `invoke_handler!`.

**Interfaces:**
- Consumes: Task 7's backend methods; the existing `parse_folder_ref`, `BackendState`, `UiError`, and the `move_bulk_with_backend` bulk pattern at `:1510`.
- Produces (camelCase params to match the TS side):
  `message_delete(from: String, id: String, origin_full: Option<String>)`,
  `message_delete_bulk(items: Vec<MoveBulkItem-or-equivalent { id, folder }>)` (reuse the existing bulk item DTO; the per-item `folder` is the origin),
  `message_restore(id: String)`, `message_restore_bulk(ids: Vec<String>)`,
  `trash_empty() -> usize`, `trash_purge_one(id: String)`.
  `origin_full` is supplied by the caller from the message's identity (the frontend already has `MessageMeta.identity`); when absent the backend records `None`.

- [ ] **Step 1: Write the failing test** — mirror an existing command test (e.g. the `move_bulk_with_backend` test): call the bulk-delete helper against a fake/native backend over a tempdir, assert messages land in `deleted`.
- [ ] **Step 2: Run to verify it fails** (CI) — FAIL.
- [ ] **Step 3: Implement** the commands (mirror `mailbox_move` + `move_bulk_with_backend` structure) and add each to the `invoke_handler!` list in `lib.rs`.
- [ ] **Step 4: Run** (CI) — PASS.
- [ ] **Step 5: Commit** — `feat(commands): message delete/restore/empty/purge Tauri commands (tuxlink-wl7n)`

---

## Task 9: Auto-purge config + scheduled sweep

**Files:** Modify `src-tauri/src/config.rs` (config struct + defaults) + `src-tauri/src/lib.rs` (`.setup()`).

**Interfaces:**
- Produces: `Config.trash_auto_purge: bool` (default `true`) + `Config.trash_retention_days: u32` (default `30`), with `#[serde(default = ...)]` so existing configs upgrade cleanly. In `.setup()`, after the mailbox is constructed: run one `purge_expired_trash(retention_days)` at startup (when enabled), then spawn a periodic task (e.g. `tokio::time::interval` every 6 h) that re-runs it while enabled. Best-effort: log failures, never panic.

- [ ] **Step 1: Write the failing test** — config round-trips the two new fields with defaults when absent (mirror an existing `config.rs` serde-default test).
- [ ] **Step 2: Run to verify it fails** (CI) — FAIL.
- [ ] **Step 3: Implement** the config fields + the startup + interval sweep wiring.
- [ ] **Step 4: Run** (CI) — PASS.
- [ ] **Step 5: Commit** — `feat(mailbox): auto-purge config + startup/periodic trash sweep (tuxlink-wl7n)`

---

## Task 10: TS bindings + folder enablement

**Files:** Modify `src/mailbox/types.ts` + the command-wrapper module (grep for the existing `invoke('mailbox_move')` wrapper; add alongside it) + `src/mailbox/useMailbox.ts:32`.

**Interfaces:**
- Produces: `deleteMessages(items: {id: string; folder: MailboxFolder; identity?: string}[])`, `restoreMessages(ids: string[])`, `emptyTrash(): Promise<number>`, `purgeMessage(id: string)` — thin `invoke` wrappers. `'deleted'` enabled in the folder lists/`useMailbox` set. Keep the existing per-folder query-invalidation contract (`mailbox_move` already invalidates source+dest folder queries — delete invalidates source + `'deleted'`; restore invalidates `'deleted'` + dest; empty/purge invalidates `'deleted'`).

- [ ] **Step 1: Write the failing test** (vitest) — a wrapper test asserting `deleteMessages([{id:'X',folder:'inbox'}])` calls `invoke('message_delete_bulk', …)` with the right args (mock `@tauri-apps/api/core` `invoke`, mirror an existing wrapper test).
- [ ] **Step 2: Run** `pnpm vitest run src/mailbox/<wrapper>.test.ts` — FAIL.
- [ ] **Step 3: Implement** the wrappers + folder enablement.
- [ ] **Step 4: Run** vitest — PASS; `pnpm typecheck` clean.
- [ ] **Step 5: Commit** — `feat(mailbox): TS delete/restore/trash bindings + enable Deleted folder (tuxlink-wl7n)`

---

## Task 11: `MessageContextMenu` — Delete / Restore / Delete-permanently

**Files:** Modify `src/mailbox/MessageContextMenu.tsx` (+ `.test.tsx`).

**Interfaces:**
- Consumes: Task 10 wrappers; props pattern of the existing menu (it already takes `folder`, `onMoveTo`, `onArchive`).
- Produces: a new `onDelete` (folder ≠ `'deleted'`) shown below Archive; when `folder === 'deleted'`, render `onRestore` + `onDeletePermanently` (the latter opens the confirm) INSTEAD of Delete/Archive/Move. Delete-to-Trash has no confirm.

- [ ] **Step 1: Write the failing test** — render the menu with `folder='inbox'`, assert a "Delete" item that calls `onDelete`; render with `folder='deleted'`, assert "Restore" + "Delete permanently" and NO "Delete"/"Archive".
- [ ] **Step 2: Run** vitest — FAIL.
- [ ] **Step 3: Implement** the menu items (mirror the existing Archive item + `tux-ctx-separator`).
- [ ] **Step 4: Run** vitest + typecheck — PASS.
- [ ] **Step 5: Commit** — `feat(mailbox): Delete/Restore/Delete-permanently in message context menu (tuxlink-wl7n)`

---

## Task 12: `MessageView` — Delete button + `Del` key

**Files:** Modify `src/mailbox/MessageView.tsx:536` area (+ `.test.tsx`) and the keymap (`src/shell/chrome/menuModel.ts:42` has the Archive `A` accel — add a Delete entry).

**Interfaces:**
- Consumes: Task 10 wrappers. Produces: a Delete button next to Archive (`title="Delete (Del)"`); `Del` key triggers delete of the open message (folder ≠ deleted). In the Deleted folder the button row shows Restore + Delete-permanently.

- [ ] **Step 1: Write the failing test** — render `MessageView` for an inbox message, fire the Delete button → asserts the delete handler; render a deleted message → Restore + Delete-permanently shown.
- [ ] **Step 2: Run** vitest — FAIL.
- [ ] **Step 3: Implement** the button + key handler + menuModel entry.
- [ ] **Step 4: Run** vitest + typecheck — PASS.
- [ ] **Step 5: Commit** — `feat(mailbox): MessageView Delete button + Del accelerator (tuxlink-wl7n)`

---

## Task 13: `MessageBulkBar` — bulk Delete / Restore / Delete-permanently

**Files:** Modify `src/mailbox/MessageBulkBar.tsx` (+ `.test.tsx`).

**Interfaces:**
- Consumes: Task 10 wrappers. Produces: a Delete button on the bulk bar (selection in a normal folder) that bulk-deletes; when the current folder is `'deleted'`, show Restore + Delete-permanently (the latter confirms once for the whole selection).

- [ ] **Step 1: Write the failing test** — bulk bar with a selection in `inbox` → Delete calls `deleteMessages` with all selected ids+folder; in `deleted` → Restore + Delete-permanently.
- [ ] **Step 2: Run** vitest — FAIL.
- [ ] **Step 3: Implement** (mirror the existing bulk Move/Mark buttons).
- [ ] **Step 4: Run** vitest + typecheck — PASS.
- [ ] **Step 5: Commit** — `feat(mailbox): bulk Delete/Restore/purge in MessageBulkBar (tuxlink-wl7n)`

---

## Task 14: `FolderSidebar` Trash entry + Empty Trash + confirm modal

**Files:** Modify `src/mailbox/FolderSidebar.tsx` (+ `.test.tsx`); add a confirm modal mirroring `DeleteFolderDialog.tsx`.

**Interfaces:**
- Consumes: Task 10 `emptyTrash`. Produces: the `Deleted` ("Trash") folder entry in the sidebar; an **Empty Trash** action visible when viewing the Deleted folder; a confirm modal (reuse the `DeleteFolderDialog` structure) gating Empty Trash + per-item permanent delete. Confirm copy: "Permanently delete N message(s)? This cannot be undone." Empty/purge are the ONLY confirmed actions.

- [ ] **Step 1: Write the failing test** — Deleted folder shows an "Empty Trash" control; clicking it opens the confirm; confirming calls `emptyTrash`.
- [ ] **Step 2: Run** vitest — FAIL.
- [ ] **Step 3: Implement** the sidebar entry + Empty Trash + confirm modal.
- [ ] **Step 4: Run** vitest + typecheck — PASS.
- [ ] **Step 5: Commit** — `feat(mailbox): Trash sidebar entry + Empty Trash with confirm (tuxlink-wl7n)`

---

## Task 15: Settings — auto-purge toggle + retention days

**Files:** Modify the mailbox settings section (grep `SettingsPanel.tsx` for the mailbox/folder settings block) (+ test).

**Interfaces:**
- Consumes: Task 9 config fields (via the existing config read/write commands). Produces: a toggle "Automatically empty Trash after N days" (default on) + a number input for N (default 30), persisted through the existing config command path.

- [ ] **Step 1: Write the failing test** — the settings section renders the toggle + day input bound to config; changing them invokes the config-save path.
- [ ] **Step 2: Run** vitest — FAIL.
- [ ] **Step 3: Implement** the settings controls.
- [ ] **Step 4: Run** vitest + typecheck — PASS.
- [ ] **Step 5: Commit** — `feat(settings): Trash auto-purge toggle + retention days (tuxlink-wl7n)`

---

## Task 16: Docs

**Files:** Modify `docs/user-guide/07-mailbox-model.md`.

- [ ] **Step 1:** Document the Deleted (Trash) folder + the Delete → Restore → Empty/auto-purge lifecycle, completing the existing "or deletes" reference; note Trash ≠ Archive and the 30-day default auto-purge.
- [ ] **Step 2: Run** `pnpm lint:docs` — PASS (no broken links).
- [ ] **Step 3: Commit** — `docs(mailbox): document Delete + Trash lifecycle (tuxlink-wl7n)`

---

## Done-time gates (orchestrator)

- **Wire-walk** (hard gate): the operator supplies the key flows greenfield (e.g. "delete an inbox message and get it back", "empty the trash", "delete a queued outbox message"); trace each to code; any broken flow = not shipped.
- **Codex adversarial round** on the backend diff (the per-identity restore + the purge sweep + the new argv-free storage paths).
- **Operator converged-build smoke** post-merge.
- Full `pnpm vitest run` + CI (clippy + cargo test, both arches) green before merge.

## Self-Review notes

- Spec coverage: every spec section maps to a task (model→1-6, per-folder/Outbox→3/4/8 via origin, backend→1-9, frontend→10-15, docs→16, auto-purge→6/9/15).
- The Outbox "block delete while actively transmitting" guard is enforced at the command/UI layer (the frontend disables Delete for a message in a live send session — surface via the existing send-session state); noted in Task 8/12 wiring. If the live-send-state is not readily available to the UI, the fallback is the backend rejecting a delete of a message currently locked by a send session — confirm during Task 7/8 and add the guard where the send session holds the lock.
- `now` is injected into `delete_message`/`purge_expired`/`trash_is_expired` for deterministic tests; the command/backend layer supplies the real clock.
