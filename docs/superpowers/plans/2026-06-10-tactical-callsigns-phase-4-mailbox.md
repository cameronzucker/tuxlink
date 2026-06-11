# Phase 4: Per-FULL Mailbox + Tagged Sent/Outbox — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax. Use the canonical type names from the master plan's "Canonical interface contract" verbatim (`Callsign`, `Address`, `SessionIdentity`, `IdentityHandle`). This phase is `tuxlink-2ns7`; it depends on Phase 3 (`tuxlink-0063`, handle threading) and builds on `tuxlink-9efs` (intent-filtered outbound drain) + `tuxlink-mzm4` (store-time unread-predicate fix).

**Goal:** Make the on-disk mailbox per-FULL-callsign for received mail (Inbox + per-callsign user folders), while keeping Sent + Outbox a single shared store in which every message is tagged with the identity it was sent/queued as. Drain the shared Outbox by the **active session identity** at send time so a session connected as `W1ABC` ships only `W1ABC`'s queued mail. Add an identity tag to the search index and keep unread accounting consistent with `list()` per destination folder (cf. `tuxlink-mzm4`).

**Architecture:** `Mailbox` gains a per-FULL **namespace root** for received-mail folders (`<root>/mailbox/<CALLSIGN>/inbox`, `<root>/mailbox/<CALLSIGN>/<user-slug>`) while Sent + Outbox stay at fixed shared paths (`<root>/sent`, `<root>/outbox`). An `IdentityTag` (the FULL `Callsign` string the message was authored under) travels with every Sent/Outbox message as a `<mid>.identity` sidecar and as a new `identity_tag` column in the search index. `build_outbound_proposals` takes the active `SessionIdentity` and drains only Outbox messages whose tag equals `session.mycall()`. Tactical mail lands in its parent FULL's inbox (per-tactical folder routing is `tuxlink-73nl`, out of scope; this phase leaves the integration seam — a documented `route_received` insertion point that today always targets the parent FULL inbox).

**Tech stack:** Rust (Tauri backend), SQLite FTS5 search index (`rusqlite`), the existing `winlink::message::Message` parser, `tempfile` for tests.

**Spec:** [`docs/superpowers/specs/2026-06-10-multiple-tactical-callsigns-design.md`](../specs/2026-06-10-multiple-tactical-callsigns-design.md) §"Mailbox model"
**Master plan:** [`docs/superpowers/plans/2026-06-10-tactical-callsigns-master-plan.md`](2026-06-10-tactical-callsigns-master-plan.md)

---

## On-disk path scheme + index column (data layout)

### Before (single-callsign, today)

```
<app_data>/native-mbox/
  inbox/      <mid>.b2f  <mid>.read
  sent/       <mid>.b2f  <mid>.read
  outbox/     <mid>.b2f  <mid>.read
  archive/    <mid>.b2f  <mid>.read
  <user-slug>/  <mid>.b2f  <mid>.read
  .folders.json
  search.db
```

### After (Phase 4)

```
<app_data>/native-mbox/
  mailbox/
    W1ABC/                          # per-FULL namespace
      inbox/    <mid>.b2f  <mid>.read
      archive/  <mid>.b2f  <mid>.read
      <user-slug>/ <mid>.b2f  <mid>.read
      .folders.json                 # per-FULL user-folder registry
    W7XYZ/
      inbox/    ...
      archive/  ...
      .folders.json
  sent/         <mid>.b2f  <mid>.read  <mid>.identity   # SHARED, tagged
  outbox/       <mid>.b2f  <mid>.read  <mid>.identity   # SHARED, tagged
  search.db
```

Decisions, locked:

- **Inbox + Archive + user folders are per-FULL.** Both `Inbox` and `Archive` hold received mail (cf. `direction_for_folder` and the `matches!(folder, Inbox | Archive)` unread predicate), so both move under `mailbox/<CALLSIGN>/`. The user-folder registry `.folders.json` is per-FULL (an operator's "Skywarn" folder under `W1ABC` is distinct from one under `W7XYZ`).
- **Sent + Outbox are shared, fixed at `<root>/sent` and `<root>/outbox`.** They are NOT namespaced by callsign. The `<mid>.identity` sidecar (one line: the FULL callsign the message was authored under) is the per-message tag.
- **`<mid>.read` sidecars keep their existing meaning + travel rules** — no change beyond living inside the new per-FULL dirs for received mail.
- **Callsign → path segment** is the FULL callsign uppercased, byte-validated through `Callsign::parse` (nonempty, no whitespace, ASCII-printable, `<=32`), so it is always a safe single path segment. A `/` or `\` or `.` cannot appear (slashes are whitespace-adjacent rejects; `Callsign::parse` already excludes whitespace and we additionally reject path separators in the namespacer — see Task 1).

### Search-index column

`messages_meta` gains `identity_tag TEXT` (nullable). It records the FULL callsign a Sent/Outbox message was authored under, and — for received mail — the FULL callsign whose inbox the message was delivered into. The FTS table is unchanged (identity is a structured filter, not free text). Schema bumps **v3 → v4**; existing v3 indices return `SchemaDrift` from `Index::open` and the operator's existing `tauri_search_rebuild_index` path recreates fresh.

```sql
-- added to messages_meta in init_schema:
identity_tag     TEXT,
-- added after the existing folder index:
CREATE INDEX idx_meta_identity ON messages_meta(identity_tag);
```

---

## Tasks

Order: **1 → 2 → 3 → 4 → 5 → 6**. Tasks 1–3 are the inbox-namespacing slice; Tasks 4–5 are the Sent/Outbox-tagging + drain slice; Task 6 is migration. The phase may break for a session between Task 3 and Task 4 (master plan's optional inbox-vs-tagging split).

Run each task's tests with the exact command shown, then the clippy gate:

```
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Re-run clippy until it exits 0 (it hides later-target lints behind the first failure — cf. `scoped_vitest_misses_contract_tests`).

---

### Task 1 — `IdentityNamespace`: resolve per-FULL received-mail dirs

**Files:**
- `src-tauri/src/native_mailbox.rs` — add `IdentityNamespace` + change `folder_dir` / `resolve_dir` to be namespace-aware (around the existing `folder_dir` at line 260 and `resolve_dir` at line 620).

The `Mailbox` learns an **active received-mail namespace**: a FULL callsign that selects which `mailbox/<CALLSIGN>/` subtree the per-FULL folders resolve to. Sent + Outbox ignore the namespace (always shared root paths).

- [ ] Write the failing test: storing to `W1ABC`'s inbox and `W7XYZ`'s inbox keeps them separate.

```rust
// in native_mailbox.rs `mod tests`
#[test]
fn per_full_inbox_namespaces_are_independent() {
    let dir = tempdir().unwrap();
    let mbox = Mailbox::new(dir.path());

    // Store one message into each FULL's inbox via the namespace selector.
    let a = mbox
        .for_identity("W1ABC")
        .store(MailboxFolder::Inbox, &raw("For Alpha", "a"))
        .unwrap();
    let x = mbox
        .for_identity("W7XYZ")
        .store(MailboxFolder::Inbox, &raw("For Xray", "x"))
        .unwrap();
    assert_ne!(a.0, x.0, "fixtures must carry distinct MIDs");

    // Each inbox sees only its own message.
    let alpha = mbox.for_identity("W1ABC").list(MailboxFolder::Inbox).unwrap();
    let xray = mbox.for_identity("W7XYZ").list(MailboxFolder::Inbox).unwrap();
    assert_eq!(alpha.len(), 1);
    assert_eq!(alpha[0].subject, "For Alpha");
    assert_eq!(xray.len(), 1);
    assert_eq!(xray[0].subject, "For Xray");

    // On-disk paths are namespaced under mailbox/<CALLSIGN>/inbox.
    assert!(dir.path().join("mailbox/W1ABC/inbox").join(format!("{}.b2f", a.0)).exists());
    assert!(dir.path().join("mailbox/W7XYZ/inbox").join(format!("{}.b2f", x.0)).exists());
    // And NOT at the legacy flat path.
    assert!(!dir.path().join("inbox").join(format!("{}.b2f", a.0)).exists());
}
```

- [ ] Add the namespace selector + a path-segment guard. `for_identity` returns a lightweight view that carries the active namespace; the underlying `Mailbox` state is unchanged so existing callers keep working (an un-namespaced `Mailbox` resolves received-mail folders into a **default** namespace — see Task 6 migration for what "default" is at runtime; in unit tests with no identity it is the literal `_default`).

```rust
/// Selects which per-FULL subtree received-mail folders resolve into.
/// Sent + Outbox ignore this (always shared root paths).
#[derive(Debug, Clone)]
pub struct IdentityNamespace(String);

impl IdentityNamespace {
    /// Build from a FULL callsign string. Rejects anything that is not a single
    /// safe path segment (defense in depth over `Callsign::parse`): no path
    /// separators, no `.`/`..`, nonempty.
    pub fn parse(callsign: &str) -> Result<Self, BackendError> {
        let c = callsign.trim();
        if c.is_empty()
            || c == "."
            || c == ".."
            || c.contains('/')
            || c.contains('\\')
            || c.contains('\0')
        {
            return Err(BackendError::MessageRejected(format!(
                "invalid identity namespace segment: {callsign:?}"
            )));
        }
        Ok(Self(c.to_string()))
    }
    pub fn as_str(&self) -> &str { &self.0 }
}

impl Mailbox {
    /// A view of this mailbox scoped to a FULL callsign's received-mail subtree.
    /// Received-mail folders (Inbox/Archive) + user folders resolve under
    /// `mailbox/<CALLSIGN>/`; Sent/Outbox stay shared.
    pub fn for_identity(&self, full_callsign: &str) -> ScopedMailbox<'_> {
        let ns = IdentityNamespace::parse(full_callsign)
            .unwrap_or_else(|_| IdentityNamespace("_default".to_string()));
        ScopedMailbox { inner: self, ns }
    }
}
```

- [ ] Introduce `ScopedMailbox<'_>` carrying `&Mailbox` + the namespace, and route the per-FULL operations through it. Keep `Mailbox`'s existing methods as the **shared/default** implementation (so Sent/Outbox + every current caller compiles unchanged); `ScopedMailbox` delegates to private namespace-aware helpers.

```rust
pub struct ScopedMailbox<'a> {
    inner: &'a Mailbox,
    ns: IdentityNamespace,
}

impl<'a> ScopedMailbox<'a> {
    pub fn store(&self, folder: MailboxFolder, raw: &[u8]) -> Result<MessageId, BackendError> {
        self.inner.store_ns(Some(&self.ns), folder, raw)
    }
    pub fn list(&self, folder: MailboxFolder) -> Result<Vec<MessageMeta>, BackendError> {
        self.inner.list_ns(Some(&self.ns), folder)
    }
    pub fn read(&self, folder: MailboxFolder, id: &MessageId) -> Result<MessageBody, BackendError> {
        self.inner.read_ns(Some(&self.ns), folder, id)
    }
    pub fn list_user(&self, slug: &str) -> Result<Vec<MessageMeta>, BackendError> {
        self.inner.list_user_ns(Some(&self.ns), slug)
    }
    pub fn move_between(&self, from: FolderRef, to: FolderRef, id: &MessageId) -> Result<(), BackendError> {
        self.inner.move_between_ns(Some(&self.ns), from, to, id)
    }
}
```

- [ ] Refactor `folder_dir` + `resolve_dir` + the user-folder root to be namespace-aware. The single load-bearing rule: **Sent/Outbox use the shared root; Inbox/Archive + user folders use the per-FULL root.** Introduce `received_root(ns)`:

```rust
impl Mailbox {
    /// Root for received-mail folders + user folders for a given namespace.
    /// `None` (un-namespaced default) → `<root>/mailbox/_default` so even the
    /// default path is uniform with the per-FULL layout.
    fn received_root(&self, ns: Option<&IdentityNamespace>) -> PathBuf {
        let seg = ns.map(|n| n.as_str()).unwrap_or("_default");
        self.root.join("mailbox").join(seg)
    }

    fn folder_dir_ns(&self, ns: Option<&IdentityNamespace>, folder: MailboxFolder) -> PathBuf {
        match folder {
            // Shared, never namespaced.
            MailboxFolder::Sent => self.root.join("sent"),
            MailboxFolder::Outbox => self.root.join("outbox"),
            // Per-FULL received mail.
            MailboxFolder::Inbox => self.received_root(ns).join("inbox"),
            MailboxFolder::Archive => self.received_root(ns).join("archive"),
        }
    }
}
```

Make the existing `folder_dir(folder)` delegate to `folder_dir_ns(None, folder)` so every current caller routes through the `_default` namespace; `store`/`list`/`read`/`move_to`/`set_read_state` get `*_ns` variants that thread the namespace, and the public no-namespace methods call the `None` form. User-folder helpers (`user_folders::folder_dir(&self.root, slug)`) become `user_folders::folder_dir(&self.received_root(ns), slug)` and the registry path `load_registry`/`save_registry` take `&self.received_root(ns)` (Task 3 covers per-FULL registries).

- [ ] Run it green.

```
cargo test --manifest-path src-tauri/Cargo.toml per_full_inbox_namespaces_are_independent
```

Expected: `test ... ok`. The two inboxes are separate directories; the legacy flat `inbox/` path no longer holds the message.

- [ ] Clippy gate, then commit.

```
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
git add src-tauri/src/native_mailbox.rs
git commit -m "feat(mailbox): per-FULL received-mail namespace (Inbox/Archive)

Add IdentityNamespace + ScopedMailbox so Inbox/Archive resolve under
mailbox/<CALLSIGN>/ while Sent/Outbox stay shared at the root. Existing
callers route through a _default namespace; Sent/Outbox paths unchanged.

Phase 4 (tuxlink-2ns7) of the multiple/tactical-callsigns work.

Agent: sandbar-raven-fox
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2 — Per-FULL unread accounting stays consistent with `list()`

**Files:**
- `src-tauri/src/native_mailbox.rs` — the `list_ns` body (mirrors `list`'s unread predicate, lines 114–150) + the index seed in `store_ns` (mirrors `store`'s `tuxlink-mzm4` seed, lines 74–98).

The unread predicate is per-destination-folder, exactly as today — `matches!(folder, Inbox | Archive)` and `!<mid>.read exists`. Namespacing must not change it: a freshly stored message in `W1ABC`'s inbox is unread; the same callsign's Sent is never unread; `W7XYZ`'s inbox is independent.

- [ ] Write the failing test: unread accounting is per-FULL and matches `list()`.

```rust
#[test]
fn unread_is_per_full_and_matches_list_predicate() {
    let dir = tempdir().unwrap();
    let mbox = Mailbox::new(dir.path());

    // W1ABC inbox: unread until marked; W7XYZ inbox untouched.
    let id = mbox.for_identity("W1ABC").store(MailboxFolder::Inbox, &raw("Net", "x")).unwrap();
    assert!(
        mbox.for_identity("W1ABC").list(MailboxFolder::Inbox).unwrap()[0].unread,
        "fresh per-FULL inbox message is unread"
    );
    // W7XYZ's inbox is empty — namespaces don't bleed.
    assert!(mbox.for_identity("W7XYZ").list(MailboxFolder::Inbox).unwrap().is_empty());

    // Marking read in W1ABC's namespace flips only W1ABC.
    mbox.for_identity("W1ABC")
        .set_read_state(&FolderRef::System(MailboxFolder::Inbox), &id, true)
        .unwrap();
    assert!(!mbox.for_identity("W1ABC").list(MailboxFolder::Inbox).unwrap()[0].unread);
}
```

(If `set_read_state` is not yet exposed on `ScopedMailbox`, add a delegating `set_read_state(&self, folder: &FolderRef, id, read)` that calls `self.inner.set_read_state_ns(Some(&self.ns), folder, id, read)` — the `*_ns` form resolves the sidecar dir via `folder_dir_ns`/`received_root`.)

- [ ] Implement `list_ns` as a copy of `list`'s body with `self.folder_dir(folder)` → `self.folder_dir_ns(ns, folder)`. The unread predicate line stays byte-for-byte identical:

```rust
meta.unread =
    matches!(folder, MailboxFolder::Inbox | MailboxFolder::Archive)
        && !path.with_extension("read").exists();
```

- [ ] Seed the index `unread` column in `store_ns` with the SAME predicate (preserve the `tuxlink-mzm4` fix). The extractor call already takes `unread`; pass `matches!(folder, MailboxFolder::Inbox | MailboxFolder::Archive)` exactly as `store` does today, and additionally set the new `identity_tag` (Task 4 adds the column; for received mail the tag is the namespace callsign — pass `ns.map(|n| n.as_str())`).

- [ ] Run green.

```
cargo test --manifest-path src-tauri/Cargo.toml unread_is_per_full_and_matches_list_predicate
```

Expected: `ok`. Also re-run the existing store/unread tests to confirm no regression:

```
cargo test --manifest-path src-tauri/Cargo.toml store_seeds_index_unread_matching_list_predicate
```

Expected: `ok` (the `_default` namespace preserves the existing flat behavior under `mailbox/_default/inbox`).

- [ ] Clippy gate, then commit.

```
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
git add src-tauri/src/native_mailbox.rs
git commit -m "feat(mailbox): per-FULL unread accounting matches list() predicate

list_ns reuses the matches!(folder, Inbox | Archive) && !<mid>.read
predicate verbatim; store_ns seeds the index unread column with the same
predicate (preserving the tuxlink-mzm4 fix), now per-FULL.

Phase 4 (tuxlink-2ns7).

Agent: sandbar-raven-fox
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 3 — Per-FULL user folders + the tactical-delivery seam

**Files:**
- `src-tauri/src/native_mailbox.rs` — `create_user_folder` / `list_user_folders` / `list_user` / `move_between` namespace-aware variants + a documented `route_received` seam.

User folders live under each FULL's subtree with a per-FULL `.folders.json`. Tactical mail lands in its **parent FULL's inbox** (no per-tactical folder yet — that is `tuxlink-73nl`). Leave a single, named insertion point so `tuxlink-73nl` can hook routing without re-namespacing.

- [ ] Write the failing test: user folders are per-FULL; a "Skywarn" folder under `W1ABC` is independent of one under `W7XYZ`.

```rust
#[test]
fn user_folders_are_per_full() {
    let dir = tempdir().unwrap();
    let mbox = Mailbox::new(dir.path());

    let a = mbox.for_identity("W1ABC").create_user_folder("Skywarn", None).unwrap();
    assert_eq!(a.slug, "skywarn");
    // W7XYZ sees no folders — its registry is separate.
    assert!(mbox.for_identity("W7XYZ").list_user_folders().is_empty());
    // W1ABC sees exactly its one folder.
    let listed = mbox.for_identity("W1ABC").list_user_folders();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].slug, "skywarn");

    // The registries live in distinct per-FULL roots.
    assert!(dir.path().join("mailbox/W1ABC/.folders.json").exists());
    assert!(!dir.path().join("mailbox/W7XYZ/.folders.json").exists());
}
```

- [ ] Write the failing test: tactical mail lands in the parent FULL's inbox via the routing seam.

```rust
#[test]
fn tactical_mail_routes_into_parent_full_inbox() {
    let dir = tempdir().unwrap();
    let mbox = Mailbox::new(dir.path());

    // A message addressed to tactical label "AIDSTATION-1" whose parent FULL is
    // W1ABC must land in W1ABC's inbox (tuxlink-73nl will later route it into a
    // per-tactical folder; this phase only guarantees the parent-FULL landing).
    let id = mbox
        .route_received("W1ABC", Some("AIDSTATION-1"), &raw("Tactical traffic", "x"))
        .unwrap();
    let inbox = mbox.for_identity("W1ABC").list(MailboxFolder::Inbox).unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].id, id);
    // It is NOT in W7XYZ's inbox.
    assert!(mbox.for_identity("W7XYZ").list(MailboxFolder::Inbox).unwrap().is_empty());
}
```

- [ ] Implement per-FULL user-folder methods on `ScopedMailbox`, delegating to `*_ns` forms that pass `&self.received_root(ns)` to `user_folders::load_registry` / `save_registry` / `folder_dir`. The validation, slug derivation, collision, and depth-cap logic are unchanged — only the registry/dir root is per-FULL.

- [ ] Add the routing seam. It is the single documented insertion point for `tuxlink-73nl`; today it always stores into the parent FULL's inbox regardless of the tactical label:

```rust
impl Mailbox {
    /// Store a received message for `parent_full`, optionally addressed to a
    /// tactical label riding under that FULL. Phase 4: tactical mail lands in
    /// the parent FULL's inbox. tuxlink-73nl hooks per-tactical-folder routing
    /// HERE — it will inspect `addressed_to_tactical` and choose a user-folder
    /// destination instead of Inbox. Until then the label is recorded only via
    /// the identity tag and the message goes to Inbox.
    pub fn route_received(
        &self,
        parent_full: &str,
        addressed_to_tactical: Option<&str>,
        raw: &[u8],
    ) -> Result<MessageId, BackendError> {
        // tuxlink-73nl SEAM: branch on `addressed_to_tactical` to a per-tactical
        // user folder. Phase 4 deliberately ignores it for the destination and
        // always targets the parent FULL's Inbox.
        let _ = addressed_to_tactical;
        self.for_identity(parent_full).store(MailboxFolder::Inbox, raw)
    }
}
```

- [ ] Run green.

```
cargo test --manifest-path src-tauri/Cargo.toml user_folders_are_per_full
cargo test --manifest-path src-tauri/Cargo.toml tactical_mail_routes_into_parent_full_inbox
```

Expected: both `ok`. Re-run the existing user-folder suite to confirm `_default` parity:

```
cargo test --manifest-path src-tauri/Cargo.toml user_folder_create_list_delete_roundtrip
```

Expected: `ok` (registry now at `mailbox/_default/.folders.json`).

- [ ] Clippy gate, then commit.

```
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
git add src-tauri/src/native_mailbox.rs
git commit -m "feat(mailbox): per-FULL user folders + tactical-delivery seam

User-folder registries + dirs move under mailbox/<CALLSIGN>/; add
route_received as the single tuxlink-73nl insertion point (tactical mail
lands in the parent FULL inbox for now).

Phase 4 (tuxlink-2ns7).

Agent: sandbar-raven-fox
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 4 — Identity tag on Sent/Outbox: sidecar + index column

**Files:**
- `src-tauri/src/search/index.rs` — `SCHEMA_VERSION` bump (line 19), `init_schema` column + index (lines 77–101), `upsert` SQL (lines 139–182), `QueryHit` (line 316) + `query` row map (line 350), a new `messages_by_identity` count helper.
- `src-tauri/src/search/extractor.rs` — `IndexRow.identity_tag` field (struct line 14) + `extract` signature (line 57).
- `src-tauri/src/native_mailbox.rs` — write the `<mid>.identity` sidecar on Sent/Outbox `store_ns`; pass `identity_tag` through the extractor call.

The shared Sent/Outbox store tags each message with the FULL callsign it was authored under. The tag persists two ways: the `<mid>.identity` sidecar (canonical on-disk, like `<mid>.read`) and the `identity_tag` index column (queryable).

- [ ] Bump the schema + add the column. In `index.rs`:

```rust
// v3 → v4 (tuxlink-2ns7): add `identity_tag` to messages_meta. Records the
// FULL callsign a message was authored under (Sent/Outbox) or delivered into
// (received mail). Existing v3 indices return SchemaDrift; the rebuild path
// recreates fresh.
pub const SCHEMA_VERSION: u32 = 4;
```

Add to the `messages_meta` DDL (after `direction TEXT NOT NULL,`):

```sql
identity_tag     TEXT,
```

Add the index alongside `idx_meta_folder`:

```sql
CREATE INDEX idx_meta_identity ON messages_meta(identity_tag);
```

- [ ] Write the failing index test: `identity_tag` round-trips through `messages_meta` and `QueryHit`.

```rust
// in index.rs `mod mutation_tests`
#[test]
fn identity_tag_round_trips() {
    let dir = tempdir().unwrap();
    let idx = Index::open(dir.path().join("search.db")).unwrap();
    let mut row = fixture_row("MID1", "outbox", "x", "y");
    row.identity_tag = Some("W1ABC".into());
    idx.upsert(&row).unwrap();
    let tag: Option<String> = idx
        .conn
        .query_row("SELECT identity_tag FROM messages_meta WHERE mid = 'MID1'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(tag.as_deref(), Some("W1ABC"));
}
```

(Update `fixture_row`, `r`, `sent_row`, `recv_row` in the index test modules + the `IndexRow` literals in `extractor.rs` tests to add `identity_tag: None`. These are mechanical field additions; they will fail to compile until the struct field exists — add the field first.)

- [ ] Add the field to `IndexRow` (extractor.rs) + the extractor signature, then wire it through `upsert`/`query`:

```rust
// extractor.rs IndexRow, after `routing_path`:
pub identity_tag: Option<String>,

// extractor.rs `extract` gains a trailing param:
pub fn extract(
    msg: &Message,
    folder: MailboxFolder,
    direction: Direction,
    unread: bool,
    transport_used: Option<String>,
    identity_tag: Option<String>,
) -> IndexRow {
    // ... set `identity_tag` in the returned struct literal ...
}
```

In `index.rs upsert`: add `identity_tag` to the INSERT column list + `?17` placeholder, the `ON CONFLICT DO UPDATE SET` clause (`identity_tag = excluded.identity_tag`), and the params (note `indexed_at` uses `strftime` so the new bind is `?17` before it — renumber: `indexed_at` becomes `strftime('%s','now')` still, so add `identity_tag` as `?17` and keep `indexed_at` as the SQL function, no bind). Add `identity_tag: Option<String>` to `QueryHit` and `row.get(16)?` in the `query` map (column index follows `routing_path` at 15).

- [ ] Write the failing mailbox test: storing to Sent under a FULL writes the identity sidecar + index tag.

```rust
// in native_mailbox.rs `mod index_hook_tests`
#[test]
fn sent_store_tags_identity_sidecar_and_index() {
    let dir = tempdir().unwrap();
    let (mbox, idx) = build_mailbox_with_index(dir.path());
    let id = mbox.for_identity("W1ABC").store(MailboxFolder::Sent, &raw("Sent", "x")).unwrap();

    // On-disk identity sidecar next to the shared Sent b2f.
    let sidecar = dir.path().join("sent").join(format!("{}.identity", id.0));
    assert!(sidecar.exists(), "Sent message must carry a <mid>.identity sidecar");
    assert_eq!(std::fs::read_to_string(&sidecar).unwrap().trim(), "W1ABC");

    // Index tag.
    let tag: Option<String> = idx
        .lock().unwrap().conn
        .query_row("SELECT identity_tag FROM messages_meta WHERE mid = ?1", [&id.0], |r| r.get(0))
        .unwrap();
    assert_eq!(tag.as_deref(), Some("W1ABC"));
}
```

- [ ] Implement: in `store_ns`, after the b2f write, when `folder` is `Sent` or `Outbox` and a namespace is present, write `<mid>.identity` containing `ns.as_str()`. Pass `ns.map(|n| n.as_str().to_string())` as the extractor's `identity_tag`. For Inbox/Archive the tag is also the namespace callsign (received into that FULL's inbox), so the same pass-through is correct.

- [ ] Run green.

```
cargo test --manifest-path src-tauri/Cargo.toml identity_tag_round_trips
cargo test --manifest-path src-tauri/Cargo.toml sent_store_tags_identity_sidecar_and_index
```

Expected: both `ok`.

- [ ] Clippy gate, then commit.

```
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
git add src-tauri/src/search/index.rs src-tauri/src/search/extractor.rs src-tauri/src/native_mailbox.rs
git commit -m "feat(mailbox,search): tag Sent/Outbox messages by authoring identity

Add identity_tag column (schema v3->v4) + <mid>.identity sidecar so each
shared-store Sent/Outbox message records the FULL callsign it was authored
under. Extractor + index upsert/query carry the tag.

Phase 4 (tuxlink-2ns7).

Agent: sandbar-raven-fox
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 5 — Drain the shared Outbox by the active session identity

**Files:**
- `src-tauri/src/winlink_backend.rs` — `build_outbound_proposals` signature + body (lines 272–319); the move-to-Sent send-time hooks (lines 1903/1906, 2563–2570, etc.) tag the moved message.
- `src-tauri/src/native_mailbox.rs` — a `read_identity_tag(folder, id)` helper + `move_to` carries the `.identity` sidecar.

`build_outbound_proposals` gains the active `SessionIdentity` (or, in this phase's slice, its FULL `Callsign` as `&str` — the `SessionIdentity` type lands in Phase 3 and is threaded by Phase 3's call-site conversion; Phase 4 filters on `session.mycall().as_str()`). Only Outbox messages whose `<mid>.identity` tag equals the active FULL are drained.

- [ ] Write the failing test: the drain returns only the active identity's queued messages.

```rust
// in winlink_backend.rs `mod build_outbound_proposals_tests`
#[test]
fn drain_returns_only_active_identity_outbox() {
    let dir = tempdir().unwrap();
    let mailbox = Mailbox::new(dir.path());

    // Queue one message as W1ABC and one as W7XYZ into the SHARED outbox.
    let alpha = compose_message("W1ABC", &["W1AW"], &[], "Alpha out", "a", 1_716_200_000);
    let xray = compose_message("W7XYZ", &["W1AW"], &[], "Xray out", "x", 1_716_200_600);
    mailbox.for_identity("W1ABC").store(MailboxFolder::Outbox, &alpha.to_bytes()).unwrap();
    mailbox.for_identity("W7XYZ").store(MailboxFolder::Outbox, &xray.to_bytes()).unwrap();

    // Active session = W1ABC: only Alpha's message is proposed.
    let out = build_outbound_proposals(&mailbox, SessionIntent::Cms, None, Some("W1ABC")).unwrap();
    assert_eq!(out.len(), 1, "only the active identity's queued mail drains; got {out:?}");
    assert_eq!(out[0].title, "Alpha out");

    // Active session = W7XYZ: only Xray's.
    let out2 = build_outbound_proposals(&mailbox, SessionIntent::Cms, None, Some("W7XYZ")).unwrap();
    assert_eq!(out2.len(), 1);
    assert_eq!(out2[0].title, "Xray out");
}
```

- [ ] Write the failing test: an untagged legacy Outbox message (no `.identity` sidecar) drains for any active identity (back-compat / migration safety — a pre-Phase-4 queued message has no tag and must not be stranded).

```rust
#[test]
fn untagged_outbox_message_drains_for_any_identity() {
    let dir = tempdir().unwrap();
    let mailbox = Mailbox::new(dir.path());
    // Store directly via the un-namespaced Mailbox (no .identity sidecar written).
    let legacy = compose_message("W1ABC", &["W1AW"], &[], "Legacy", "x", 1_716_200_000);
    mailbox.store(MailboxFolder::Outbox, &legacy.to_bytes()).unwrap();

    let out = build_outbound_proposals(&mailbox, SessionIntent::Cms, None, Some("W7XYZ")).unwrap();
    assert_eq!(out.len(), 1, "an untagged legacy draft is not stranded by identity filtering");
}
```

- [ ] Add `read_identity_tag` to `Mailbox`:

```rust
impl Mailbox {
    /// Read a message's `<mid>.identity` tag, if present. `None` = untagged
    /// (legacy / pre-Phase-4) — callers treat untagged as "matches any".
    pub fn read_identity_tag(&self, folder: MailboxFolder, id: &MessageId) -> Option<String> {
        let p = self.folder_dir(folder).join(format!("{}.identity", id.0));
        std::fs::read_to_string(p).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
    }
}
```

- [ ] Thread the active-identity filter into `build_outbound_proposals`. Add a trailing `active_full: Option<&str>` param (None = no identity filter, the back-compat behavior for callers not yet identity-aware). Inside the loop, after the `selected` intersection, skip a message whose tag is `Some(tag)` and `tag != active`:

```rust
pub fn build_outbound_proposals(
    mailbox: &Mailbox,
    intent: SessionIntent,
    selected: Option<&std::collections::HashSet<String>>,
    active_full: Option<&str>,
) -> Result<Vec<session::OutboundMessage>, BackendError> {
    // ... existing safety gate unchanged ...
    let mut outbound = Vec::new();
    for meta in mailbox.list(MailboxFolder::Outbox)? {
        if let Some(sel) = selected {
            if !sel.contains(&meta.id.0) { continue; }
        }
        // Identity drain gate (tuxlink-2ns7): a session drains only its own
        // queued mail. An untagged (legacy) message has no tag and drains for
        // anyone, so it is never stranded by the migration.
        if let Some(active) = active_full {
            if let Some(tag) = mailbox.read_identity_tag(MailboxFolder::Outbox, &meta.id) {
                if tag != active { continue; }
            }
        }
        // ... existing read + to_proposal unchanged ...
    }
    Ok(outbound)
}
```

- [ ] Update every call site of `build_outbound_proposals` to pass the active identity. The dial/listen sites that hold a `SessionIdentity` (post-Phase-3) pass `Some(session.mycall().as_str())`; sites that have not yet been identity-converted pass `None` (explicitly — a `None` is back-compat-safe). Existing tests that call with `None` get a fourth arg `None`.

- [ ] Make `move_to` carry the `<mid>.identity` sidecar (so the Outbox→Sent transition at send time keeps the tag). In `move_to` and `move_between`, alongside the existing `<mid>.read` carry logic, move `<mid>.identity` if present:

```rust
let src_id_marker = self.folder_dir(from).join(format!("{}.identity", id.0));
if src_id_marker.exists() {
    let tag = fs::read(&src_id_marker)?;
    fs::write(dst_dir.join(format!("{}.identity", id.0)), tag)?;
    fs::remove_file(&src_id_marker)?;
}
```

- [ ] Write the failing test: Outbox→Sent keeps the identity tag.

```rust
// in native_mailbox.rs `mod tests`
#[test]
fn send_time_move_outbox_to_sent_keeps_identity_tag() {
    let dir = tempdir().unwrap();
    let mbox = Mailbox::new(dir.path());
    let id = mbox.for_identity("W1ABC").store(MailboxFolder::Outbox, &raw("Out", "x")).unwrap();
    assert_eq!(mbox.read_identity_tag(MailboxFolder::Outbox, &id).as_deref(), Some("W1ABC"));

    mbox.move_to(MailboxFolder::Outbox, MailboxFolder::Sent, &id).unwrap();

    assert!(mbox.read_identity_tag(MailboxFolder::Outbox, &id).is_none(), "no orphan tag in outbox");
    assert_eq!(
        mbox.read_identity_tag(MailboxFolder::Sent, &id).as_deref(),
        Some("W1ABC"),
        "the identity tag travels with the message into Sent"
    );
}
```

- [ ] Run green.

```
cargo test --manifest-path src-tauri/Cargo.toml drain_returns_only_active_identity_outbox
cargo test --manifest-path src-tauri/Cargo.toml untagged_outbox_message_drains_for_any_identity
cargo test --manifest-path src-tauri/Cargo.toml send_time_move_outbox_to_sent_keeps_identity_tag
```

Expected: all three `ok`. Then the full mailbox + drain suites for no regression:

```
cargo test --manifest-path src-tauri/Cargo.toml build_outbound_proposals_tests
cargo test --manifest-path src-tauri/Cargo.toml native_mailbox
```

Expected: all `ok`.

- [ ] Clippy gate, then commit.

```
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
git add src-tauri/src/winlink_backend.rs src-tauri/src/native_mailbox.rs
git commit -m "feat(mailbox): drain shared Outbox by active session identity

build_outbound_proposals filters Outbox by the active FULL callsign;
untagged legacy drafts are never stranded. move_to/move_between carry the
<mid>.identity sidecar so the send-time Outbox->Sent move keeps the tag.

Phase 4 (tuxlink-2ns7).

Agent: sandbar-raven-fox
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 6 — Migration: legacy flat mailbox → per-FULL `_default`, then re-home to the FULL

**Files:**
- `src-tauri/src/native_mailbox.rs` — a `migrate_legacy_layout(default_full: &Callsign)` method.

The existing install has `inbox/`, `sent/`, `outbox/`, `archive/`, `<user-slug>/`, `.folders.json` flat at the root. Migration: the existing inbox/archive/user-folders move under `mailbox/<DEFAULT_FULL>/`; existing Sent/Outbox messages get a `<mid>.identity` sidecar tagging them as `DEFAULT_FULL`; the search index is rebuilt by the operator's existing `tauri_search_rebuild_index` path (the schema bump already forces this). `DEFAULT_FULL` is the migrated single FULL identity (the spec's "existing `identity.callsign` becomes the one FULL identity") — Phase 2 supplies it; Phase 4's migration takes it as a parameter.

- [ ] Write the failing test: a legacy flat mailbox migrates to per-FULL with inbox intact + Sent/Outbox tagged.

```rust
#[test]
fn migrate_legacy_flat_layout_to_per_full() {
    let dir = tempdir().unwrap();
    // Seed a pre-Phase-4 flat layout directly on disk.
    let mbox = Mailbox::new(dir.path());
    // store() (un-namespaced) writes to mailbox/_default; for the migration test
    // we want the TRUE legacy flat path, so write raw into <root>/inbox etc.
    for (folder, subj) in [("inbox", "In A"), ("archive", "Arch A")] {
        let raw = raw(subj, "x");
        let mid = crate::winlink::message::Message::from_bytes(&raw)
            .unwrap().header("Mid").unwrap().to_string();
        let d = dir.path().join(folder);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(format!("{mid}.b2f")), &raw).unwrap();
    }
    let sent_raw = raw("Sent A", "s");
    let sent_mid = crate::winlink::message::Message::from_bytes(&sent_raw)
        .unwrap().header("Mid").unwrap().to_string();
    std::fs::create_dir_all(dir.path().join("sent")).unwrap();
    std::fs::write(dir.path().join("sent").join(format!("{sent_mid}.b2f")), &sent_raw).unwrap();

    // Migrate, naming the default FULL.
    let full = crate::identity::Callsign::parse("N7CPZ").unwrap();
    mbox.migrate_legacy_layout(&full).unwrap();

    // Inbox + Archive now live under mailbox/N7CPZ/.
    assert_eq!(mbox.for_identity("N7CPZ").list(MailboxFolder::Inbox).unwrap().len(), 1);
    assert_eq!(mbox.for_identity("N7CPZ").list(MailboxFolder::Archive).unwrap().len(), 1);
    // Legacy flat dirs are gone.
    assert!(!dir.path().join("inbox").exists());
    assert!(!dir.path().join("archive").exists());
    // Sent stays shared but is now tagged with the default FULL.
    assert_eq!(
        mbox.read_identity_tag(MailboxFolder::Sent, &MessageId(sent_mid)).as_deref(),
        Some("N7CPZ"),
    );
    assert!(dir.path().join("sent").exists(), "Sent stays at the shared root");
}
```

- [ ] Implement `migrate_legacy_layout`:
  - Idempotent: if `<root>/mailbox/<DEFAULT_FULL>/inbox` already exists, return `Ok(())` early.
  - For `inbox` and `archive`: if `<root>/<name>` exists, `fs::rename` its `.b2f` + `.read` files (and the whole dir) into `received_root(Some(ns))/<name>/`. Use `received_root(Some(&IdentityNamespace::parse(default_full.as_str())?))`.
  - For any legacy top-level user-folder slugs recorded in `<root>/.folders.json`: move each slug dir into the per-FULL root and move `.folders.json` itself to `received_root(...)/.folders.json`.
  - For `sent` and `outbox` (stay put): for each `<mid>.b2f` lacking a `<mid>.identity` sidecar, write one containing `default_full.as_str()`.
  - Do NOT touch `search.db` — the v3→v4 schema bump already forces the operator's rebuild path, which re-extracts every message (now reading the new `mailbox/<CALLSIGN>/` paths + `.identity` sidecars). Document this in a comment.

- [ ] Run green.

```
cargo test --manifest-path src-tauri/Cargo.toml migrate_legacy_flat_layout_to_per_full
```

Expected: `ok`.

- [ ] Clippy gate, then commit.

```
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
git add src-tauri/src/native_mailbox.rs
git commit -m "feat(mailbox): migrate legacy flat layout to per-FULL namespace

migrate_legacy_layout re-homes inbox/archive/user-folders under
mailbox/<DEFAULT_FULL>/, tags existing Sent/Outbox with the default FULL,
and relies on the v3->v4 schema bump to force the index rebuild. Idempotent.

Phase 4 (tuxlink-2ns7).

Agent: sandbar-raven-fox
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Phase definition of done

- All six tasks' tests green; the full backend suite green:
  ```
  cargo test --manifest-path src-tauri/Cargo.toml native_mailbox
  cargo test --manifest-path src-tauri/Cargo.toml --lib search::
  cargo test --manifest-path src-tauri/Cargo.toml build_outbound_proposals_tests
  ```
- Clippy clean (re-run until exit 0):
  ```
  cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
  ```
- CI green on both arches; PR merged; `bd close tuxlink-2ns7`.

---

## Self-review

- **Per-FULL inbox.** Tasks 1–3 move Inbox + Archive + user folders (both received-mail folders, per `direction_for_folder` + the `matches!(folder, Inbox | Archive)` predicate) under `mailbox/<CALLSIGN>/`, with a per-FULL `.folders.json`. `per_full_inbox_namespaces_are_independent` + `user_folders_are_per_full` prove separation; `tactical_mail_routes_into_parent_full_inbox` proves the parent-FULL landing and leaves the named `route_received` seam for `tuxlink-73nl` (per-tactical folder routing stays out of scope).
- **Shared, tagged Sent/Outbox.** Sent + Outbox stay at the fixed shared root paths; Task 4 adds the `<mid>.identity` sidecar + `identity_tag` index column (schema v3→v4). Task 5's drain (`drain_returns_only_active_identity_outbox`) filters the shared Outbox by `session.mycall()`, and `move_to`/`move_between` carry the tag so the send-time Outbox→Sent move preserves it. `untagged_outbox_message_drains_for_any_identity` guards migration safety (legacy drafts not stranded).
- **Index/unread consistency.** `store_ns` seeds the index `unread` column with the SAME `matches!(folder, Inbox | Archive)` predicate `list_ns` uses (the `tuxlink-mzm4` invariant, now per-FULL); `unread_is_per_full_and_matches_list_predicate` pins it, and `store_seeds_index_unread_matching_list_predicate` re-runs green against the `_default` namespace. The `identity_tag` is a structured filter column (indexed), not FTS text, so free-text search is unaffected.
- **Canonical types.** Uses `Callsign` (`identity::Callsign::parse`/`.as_str()`), `Address`, `SessionIdentity`/`SessionIdentity::mycall()` verbatim from the master plan contract. The Phase-4 slice filters on `session.mycall().as_str()`; where Phase 3's `SessionIdentity` is not yet threaded at a call site, the `active_full: Option<&str>` param is passed `None` (back-compat-safe, never a silent leak — `None` means "no identity filter," and tagged-vs-active mismatch only ever *narrows*).
- **Migration.** Task 6 is idempotent, re-homes received mail under the default FULL, tags existing Sent/Outbox, and defers the index rebuild to the existing operator rebuild path forced by the schema bump — no bespoke index migration code.
