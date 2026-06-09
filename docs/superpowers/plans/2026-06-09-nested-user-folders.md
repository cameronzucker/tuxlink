# Nested User Folders Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add one level of folder nesting (top-level → subfolder) to user folders: a schema-v2 `parent_slug` metadata migration, backend validation + re-parent + cascade-delete, a recursive sidebar tree, create-subfolder + move-to UX, and drag-drop re-parent.

**Architecture:** Hierarchy is **registry metadata only** — `UserFolder` gains `parent_slug: Option<String>`; the on-disk layout stays flat (`folder_dir = root.join(slug)`, globally-unique slugs). Re-parenting is a one-field registry edit with zero file moves. Depth is capped at 2, so cycle prevention is structural and the delete cascade is one level deep. Full design: [docs/superpowers/specs/2026-06-09-nested-user-folders-design.md](../specs/2026-06-09-nested-user-folders-design.md) (decisions D1–D6).

**Tech Stack:** Rust (`src-tauri`, serde, Tauri commands) · TypeScript/React (`src/mailbox`, TanStack Query) · Vitest + jsdom (TS) · `cargo test` (Rust).

**Working tree:** worktree `worktrees/bd-tuxlink-ka3z-nested-folders`, branch `bd-tuxlink-ka3z/nested-folders` (off `origin/main`), bd issue tuxlink-ka3z. Run Rust tests with `cargo test --manifest-path src-tauri/Cargo.toml`; TS tests with `pnpm vitest run`.

---

## File Structure

**Backend (Rust):**
- `src-tauri/src/user_folders.rs` — `UserFolder` gains `parent_slug`; `Registry` default version → 2; add `validate_reparent` + `children_slugs` helpers. Owns schema + validation.
- `src-tauri/src/native_mailbox.rs` — `create_user_folder` gains parent; new `move_user_folder`; `delete_user_folder` cascades to children. Owns disk + registry mutation.
- `src-tauri/src/ui_commands.rs` — `UserFolderDto` gains `parentSlug`; `folder_create` gains `parentSlug` arg; new `folder_move` command.
- `src-tauri/src/lib.rs` — register `folder_move` in the Tauri `invoke_handler`.

**Frontend (TypeScript/React):**
- `src/mailbox/types.ts` — `UserFolder` gains optional `parentSlug`.
- `src/mailbox/useUserFolders.ts` — create mutation gains `parentSlug`; add `useMoveUserFolder`.
- `src/mailbox/FolderSidebar.tsx` — flat map → grouped tree (top-level + indented children, expand/collapse, folder drag-drop).
- `src/mailbox/FolderContextMenu.tsx` — "New subfolder here" (top-level only) + "Move to…" submenu.
- `src/mailbox/NewFolderDialog.tsx` — accept + show parent context.
- `src/mailbox/DeleteFolderDialog.tsx` — blast-radius line when target has children.

**Convention reminders:** every commit needs `Agent: <moniker>` + `Co-Authored-By:` trailers. Run `cargo clippy --all-targets -D warnings` (re-run to exit 0 — it hides later-target lints) and the full `pnpm vitest run` before any push (CI `verify` gate is stricter than scoped runs — memory `scoped_vitest_misses_contract_tests`).

---

## Task 1: Schema v2 — `parent_slug` field + transparent migration

**Files:**
- Modify: `src-tauri/src/user_folders.rs` (struct `UserFolder` ~L51-56; `Registry::default` ~L66-70)
- Test: same file, `#[cfg(test)] mod tests`

- [ ] **Step 1: Write failing migration + roundtrip tests**

Add to the `tests` module in `user_folders.rs`:

```rust
#[test]
fn v1_registry_loads_with_all_folders_top_level() {
    // A version:1 file whose folder records predate parent_slug must
    // deserialize with parent_slug == None (every folder top-level).
    let dir = tempfile::tempdir().unwrap();
    let v1 = r#"{"version":1,"folders":[
        {"slug":"nets","display_name":"Nets","created_at":"2026-06-02T22:00:00Z"}
    ]}"#;
    fs::write(dir.path().join(REGISTRY_FILENAME), v1).unwrap();
    let reg = load_registry(dir.path());
    assert_eq!(reg.folders.len(), 1);
    assert_eq!(reg.folders[0].parent_slug, None);
}

#[test]
fn v2_roundtrips_parent_slug() {
    let dir = tempfile::tempdir().unwrap();
    let reg = Registry {
        version: 2,
        folders: vec![
            UserFolder { slug: "nets".into(), display_name: "Nets".into(),
                created_at: "2026-06-02T22:00:00Z".into(), parent_slug: None },
            UserFolder { slug: "ares".into(), display_name: "ARES".into(),
                created_at: "2026-06-02T22:01:00Z".into(), parent_slug: Some("nets".into()) },
        ],
    };
    save_registry(dir.path(), &reg).unwrap();
    let loaded = load_registry(dir.path());
    assert_eq!(loaded.folders, reg.folders);
}

#[test]
fn new_registry_default_is_version_2() {
    assert_eq!(Registry::default().version, 2);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml user_folders::tests::v1_registry_loads_with_all_folders_top_level user_folders::tests::v2_roundtrips_parent_slug user_folders::tests::new_registry_default_is_version_2`
Expected: FAIL — `parent_slug` field does not exist; default version is 1.

- [ ] **Step 3: Add the field + bump the default version**

In `UserFolder` (after `created_at`):

```rust
    pub created_at: String,
    /// Parent folder slug, or `None` for a top-level folder. `#[serde(default)]`
    /// makes a v1 registry (records without this field) load with every folder
    /// top-level — the correct interpretation of a flat registry (spec D2).
    #[serde(default)]
    pub parent_slug: Option<String>,
```

In `Registry::default`:

```rust
impl Default for Registry {
    fn default() -> Self {
        Registry { version: 2, folders: Vec::new() }
    }
}
```

Then fix the three existing tests that construct `UserFolder` literals or assert `version == 1`: add `parent_slug: None` to the literals in `save_then_load_roundtrips`, and update `load_returns_empty_when_missing` to assert `reg.version == 2`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml user_folders::`
Expected: PASS (all existing + new tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/user_folders.rs
git commit   # subject: "feat(folders): schema v2 — parent_slug field + transparent v1 migration (tuxlink-ka3z)"
```

---

## Task 2: `validate_reparent` + `children_slugs` helpers (D4 rule set — the risk center)

**Files:**
- Modify: `src-tauri/src/user_folders.rs`
- Test: same file

This is the validation core the spec flags for the adversarial review. The rule set (D4): a move of `slug` to `new_parent` (`None` = top level) is valid iff (1) `new_parent != slug`; (2) `new_parent` is `None` or an existing **top-level** folder; (3) `slug` has no children when `new_parent` is `Some`; (4) `new_parent` exists when `Some`.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn children_slugs_returns_direct_children_only() {
    let reg = Registry { version: 2, folders: vec![
        uf("nets", None), uf("ares", Some("nets")), uf("satern", Some("nets")),
        uf("weather", None),
    ]};
    let mut kids = children_slugs(&reg, "nets");
    kids.sort();
    assert_eq!(kids, vec!["ares".to_string(), "satern".to_string()]);
    assert!(children_slugs(&reg, "weather").is_empty());
}

#[test]
fn validate_reparent_rejects_self_parent() {
    let reg = Registry { version: 2, folders: vec![uf("nets", None)] };
    assert!(validate_reparent(&reg, "nets", Some("nets")).is_err());
}

#[test]
fn validate_reparent_rejects_subfolder_as_parent() {
    // ares is a subfolder of nets; nothing may be parented under it (cap).
    let reg = Registry { version: 2, folders: vec![
        uf("nets", None), uf("ares", Some("nets")), uf("weather", None),
    ]};
    assert!(validate_reparent(&reg, "weather", Some("ares")).is_err());
}

#[test]
fn validate_reparent_rejects_missing_parent() {
    let reg = Registry { version: 2, folders: vec![uf("nets", None)] };
    assert!(validate_reparent(&reg, "nets", Some("ghost")).is_err());
}

#[test]
fn validate_reparent_rejects_moving_folder_with_children_under_a_parent() {
    // nets has a child; nesting nets under weather would create depth 3.
    let reg = Registry { version: 2, folders: vec![
        uf("nets", None), uf("ares", Some("nets")), uf("weather", None),
    ]};
    assert!(validate_reparent(&reg, "nets", Some("weather")).is_err());
}

#[test]
fn validate_reparent_allows_folder_with_children_to_top_level() {
    let reg = Registry { version: 2, folders: vec![
        uf("nets", Some("weather")), uf("ares", Some("nets")), uf("weather", None),
    ]};
    // moving nets (which has child ares) to top level is fine.
    assert!(validate_reparent(&reg, "nets", None).is_ok());
}

#[test]
fn validate_reparent_allows_leaf_under_top_level() {
    let reg = Registry { version: 2, folders: vec![
        uf("nets", None), uf("weather", None),
    ]};
    assert!(validate_reparent(&reg, "weather", Some("nets")).is_ok());
}

// test helper
fn uf(slug: &str, parent: Option<&str>) -> UserFolder {
    UserFolder { slug: slug.into(), display_name: slug.into(),
        created_at: "2026-06-09T00:00:00Z".into(),
        parent_slug: parent.map(|s| s.to_string()) }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml user_folders::tests::validate_reparent user_folders::tests::children_slugs`
Expected: FAIL — `validate_reparent` / `children_slugs` not defined.

- [ ] **Step 3: Implement the helpers**

Add to `user_folders.rs` (module level, near `folder_dir`):

```rust
/// Direct children of `slug` (folders whose `parent_slug == Some(slug)`).
/// Depth is capped at 2, so children are always leaves.
pub fn children_slugs(reg: &Registry, slug: &str) -> Vec<String> {
    reg.folders.iter()
        .filter(|f| f.parent_slug.as_deref() == Some(slug))
        .map(|f| f.slug.clone())
        .collect()
}

/// Returns true if `slug` names a top-level folder present in the registry.
fn is_top_level(reg: &Registry, slug: &str) -> bool {
    reg.folders.iter().any(|f| f.slug == slug && f.parent_slug.is_none())
}

/// Validate a re-parent of `slug` to `new_parent` (None = top level) against
/// the D4 rule set. Returns a human-readable error on rejection (surfaced via
/// `BackendError::message_rejected`).
pub fn validate_reparent(
    reg: &Registry,
    slug: &str,
    new_parent: Option<&str>,
) -> Result<(), String> {
    match new_parent {
        None => Ok(()), // promote to top level is always structurally valid
        Some(parent) => {
            if parent == slug {
                return Err("a folder cannot be its own parent".into());
            }
            if !is_top_level(reg, parent) {
                return Err(format!(
                    "'{parent}' cannot be a parent: it must be an existing top-level folder"
                ));
            }
            if !children_slugs(reg, slug).is_empty() {
                return Err(
                    "this folder has subfolders; move or remove them before nesting it".into(),
                );
            }
            Ok(())
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml user_folders::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/user_folders.rs
git commit   # "feat(folders): validate_reparent + children_slugs (D4 rule set) (tuxlink-ka3z)"
```

---

## Task 3: `create_user_folder` accepts a parent

**Files:**
- Modify: `src-tauri/src/native_mailbox.rs` (`create_user_folder`, the method that writes a new registry entry; near the rename method ~L260-304)
- Test: same file, `#[cfg(test)] mod tests`

> Read the current `create_user_folder` signature before editing. It currently takes `display_name: &str` and returns `Result<UserFolder, BackendError>`. Add an `parent_slug: Option<&str>` parameter, validate it (parent must exist + be top-level, reusing `validate_reparent` semantics for create), and persist it on the new `UserFolder`.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn create_subfolder_sets_parent_slug() {
    let mb = test_mailbox(); // existing helper in this module
    let parent = mb.create_user_folder("Nets", None).unwrap();
    let child = mb.create_user_folder("ARES", Some(&parent.slug)).unwrap();
    assert_eq!(child.parent_slug.as_deref(), Some("nets"));
}

#[test]
fn create_subfolder_under_missing_parent_is_rejected() {
    let mb = test_mailbox();
    assert!(mb.create_user_folder("ARES", Some("ghost")).is_err());
}

#[test]
fn create_subfolder_under_a_subfolder_is_rejected() {
    let mb = test_mailbox();
    let nets = mb.create_user_folder("Nets", None).unwrap();
    let ares = mb.create_user_folder("ARES", Some(&nets.slug)).unwrap();
    // ares is a subfolder; creating under it would be depth 3.
    assert!(mb.create_user_folder("KingCo", Some(&ares.slug)).is_err());
}
```

> If no `test_mailbox()` helper exists, mirror the construction used by the existing `user_folder_create_list_delete_roundtrip` test (it builds a `Mailbox` over a `tempfile::tempdir`). Reuse that exact pattern.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml native_mailbox::tests::create_subfolder`
Expected: FAIL — `create_user_folder` takes one arg / signature mismatch.

- [ ] **Step 3: Add the parameter + validation**

Update `create_user_folder` to accept `parent_slug: Option<&str>`. After deriving + validating the slug and loading the registry, before pushing the new entry:

```rust
        // Validate parent (spec D4): must be an existing top-level folder.
        if let Some(parent) = parent_slug {
            user_folders::validate_reparent(&reg, &slug, Some(parent))
                .map_err(BackendError::message_rejected)?;
        }
```

Set `parent_slug: parent_slug.map(|s| s.to_string())` on the constructed `UserFolder`. Update every existing in-crate caller of `create_user_folder` to pass `None`.

> `validate_reparent` checks "slug has no children" — for a brand-new slug that's trivially true, so it's safe to reuse for create. `BackendError::message_rejected` is the existing constructor used by rename/create rejections; grep for its exact name (it may be `BackendError::MessageRejected(..)` or a helper) and match the existing usage in this file.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml native_mailbox::tests::create_subfolder native_mailbox::tests::user_folder_create`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/native_mailbox.rs
git commit   # "feat(folders): create_user_folder accepts + validates parent (tuxlink-ka3z)"
```

---

## Task 4: `move_user_folder` — metadata-only re-parent

**Files:**
- Modify: `src-tauri/src/native_mailbox.rs`
- Test: same file

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn move_user_folder_reparents_without_touching_disk() {
    let mb = test_mailbox();
    let nets = mb.create_user_folder("Nets", None).unwrap();
    let weather = mb.create_user_folder("Weather", None).unwrap();
    // a message lives in weather's dir; it must NOT move on disk.
    let dir = user_folders::folder_dir(&mb.root, &weather.slug);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("M1.b2f"), b"x").unwrap();

    mb.move_user_folder(&weather.slug, Some(&nets.slug)).unwrap();

    let reg = user_folders::load_registry(&mb.root);
    let moved = reg.folders.iter().find(|f| f.slug == weather.slug).unwrap();
    assert_eq!(moved.parent_slug.as_deref(), Some("nets"));
    assert!(dir.join("M1.b2f").exists(), "message file must not move on re-parent");
}

#[test]
fn move_user_folder_rejects_invalid_reparent() {
    let mb = test_mailbox();
    let nets = mb.create_user_folder("Nets", None).unwrap();
    let ares = mb.create_user_folder("ARES", Some(&nets.slug)).unwrap();
    let weather = mb.create_user_folder("Weather", None).unwrap();
    // weather under ares (a subfolder) violates the cap.
    assert!(mb.move_user_folder(&weather.slug, Some(&ares.slug)).is_err());
}

#[test]
fn move_user_folder_promotes_to_top_level() {
    let mb = test_mailbox();
    let nets = mb.create_user_folder("Nets", None).unwrap();
    let ares = mb.create_user_folder("ARES", Some(&nets.slug)).unwrap();
    mb.move_user_folder(&ares.slug, None).unwrap();
    let reg = user_folders::load_registry(&mb.root);
    assert_eq!(reg.folders.iter().find(|f| f.slug == ares.slug).unwrap().parent_slug, None);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml native_mailbox::tests::move_user_folder`
Expected: FAIL — `move_user_folder` not defined.

- [ ] **Step 3: Implement (registry-only mutation)**

```rust
    /// Re-parent a user folder by editing its `parent_slug` in the registry.
    /// `new_parent == None` promotes it to top level. No filesystem move —
    /// folder directories stay flat at `root/<slug>` (spec D2/D3). Validates
    /// against the D4 rule set; unknown `slug` → `NotFound`.
    pub fn move_user_folder(
        &self,
        slug: &str,
        new_parent: Option<&str>,
    ) -> Result<UserFolder, BackendError> {
        let mut reg = user_folders::load_registry(&self.root);
        if !reg.folders.iter().any(|f| f.slug == slug) {
            return Err(BackendError::message_rejected(format!("unknown folder '{slug}'")));
        }
        user_folders::validate_reparent(&reg, slug, new_parent)
            .map_err(BackendError::message_rejected)?;
        let folder = reg.folders.iter_mut().find(|f| f.slug == slug).unwrap();
        folder.parent_slug = new_parent.map(|s| s.to_string());
        let updated = folder.clone();
        user_folders::save_registry(&self.root, &reg)?;
        Ok(updated)
    }
```

> Match the `BackendError` constructor names to what the file already uses (grep `BackendError::` in this file). If `NotFound` is preferred over `message_rejected` for the unknown-slug case, follow the file's existing convention for "unknown user folder".

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml native_mailbox::tests::move_user_folder`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/native_mailbox.rs
git commit   # "feat(folders): move_user_folder — metadata-only re-parent (tuxlink-ka3z)"
```

---

## Task 5: `delete_user_folder` cascades to direct children

**Files:**
- Modify: `src-tauri/src/native_mailbox.rs` (`delete_user_folder` ~L314-352)
- Test: same file

The existing method disposes of one folder's messages by `DeleteAction`. Extend it to first dispose of each direct child the same way, then the parent. Cascade is one level deep (cap-bounded).

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn delete_parent_cascades_children_move_to_inbox() {
    let mb = test_mailbox();
    let nets = mb.create_user_folder("Nets", None).unwrap();
    let ares = mb.create_user_folder("ARES", Some(&nets.slug)).unwrap();
    seed_message(&mb, &nets.slug, "P1");
    seed_message(&mb, &ares.slug, "C1");

    mb.delete_user_folder(&nets.slug, DeleteAction::MoveToInbox).unwrap();

    let reg = user_folders::load_registry(&mb.root);
    assert!(reg.folders.is_empty(), "parent + child both removed from registry");
    let inbox = mb.folder_dir(MailboxFolder::Inbox);
    assert!(inbox.join("P1.b2f").exists());
    assert!(inbox.join("C1.b2f").exists(), "child message relocated too");
    assert!(!user_folders::folder_dir(&mb.root, &ares.slug).exists());
}

#[test]
fn delete_parent_cascades_children_delete_mode() {
    let mb = test_mailbox();
    let nets = mb.create_user_folder("Nets", None).unwrap();
    let ares = mb.create_user_folder("ARES", Some(&nets.slug)).unwrap();
    seed_message(&mb, &ares.slug, "C1");
    mb.delete_user_folder(&nets.slug, DeleteAction::Delete).unwrap();
    assert!(user_folders::load_registry(&mb.root).folders.is_empty());
    assert!(!user_folders::folder_dir(&mb.root, &ares.slug).exists());
}

fn seed_message(mb: &Mailbox, slug: &str, id: &str) {
    let dir = user_folders::folder_dir(&mb.root, slug);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{id}.b2f")), b"raw").unwrap();
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml native_mailbox::tests::delete_parent_cascades`
Expected: FAIL — children survive (current method deletes only the named folder).

- [ ] **Step 3: Implement the cascade**

At the top of `delete_user_folder`, before disposing of the named folder, gather + dispose of direct children by recursing once:

```rust
        let reg = user_folders::load_registry(&self.root);
        let children = user_folders::children_slugs(&reg, slug);
        for child in &children {
            // children are leaves (depth cap); each disposes by the same action.
            self.delete_one_user_folder(child, on_messages)?;
        }
        self.delete_one_user_folder(slug, on_messages)?;
        Ok(())
```

Refactor the existing single-folder body (the `if dir.exists() { match ... }` + registry-retain block) into a private `delete_one_user_folder(&self, slug, on_messages) -> Result<(), BackendError>` that the public `delete_user_folder` calls. `DeleteAction` is `Copy` (it's a small enum) — confirm or add `#[derive(Clone, Copy)]` so it can be passed in the loop; if it isn't `Copy`, bind it once and clone per call.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml native_mailbox::tests::delete`
Expected: PASS (new cascade tests + existing `delete_user_folder_*` tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/native_mailbox.rs
git commit   # "feat(folders): delete_user_folder cascades to direct children (tuxlink-ka3z)"
```

---

## Task 6: Tauri IPC — DTO field, `folder_create` parent arg, `folder_move` command

**Files:**
- Modify: `src-tauri/src/ui_commands.rs` (`UserFolderDto` ~L782-800; `folder_create` ~L815-823; add `folder_move`)
- Modify: `src-tauri/src/lib.rs` (register `folder_move` in `invoke_handler`)
- Test: `src-tauri/src/ui_commands.rs` tests

- [ ] **Step 1: Write a failing DTO test**

```rust
#[test]
fn user_folder_dto_carries_parent_slug() {
    let uf = crate::user_folders::UserFolder {
        slug: "ares".into(), display_name: "ARES".into(),
        created_at: "2026-06-09T00:00:00Z".into(),
        parent_slug: Some("nets".into()),
    };
    let dto = UserFolderDto::from(uf);
    assert_eq!(dto.parent_slug.as_deref(), Some("nets"));
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ui_commands::tests::user_folder_dto_carries_parent_slug`
Expected: FAIL — `UserFolderDto` has no `parent_slug`.

- [ ] **Step 3: Extend the DTO + commands**

In `UserFolderDto` add (serde renames to camelCase per the struct's existing `#[serde(rename_all = "camelCase")]`):

```rust
    pub parent_slug: Option<String>,
```

In the `From<UserFolder>` impl, map `parent_slug: f.parent_slug`.

Extend `folder_create` to accept `parent_slug: Option<String>` and pass `parent_slug.as_deref()` to `Mailbox::create_user_folder`. Add the new command:

```rust
#[tauri::command]
pub async fn folder_move(
    state: tauri::State<'_, AppState>,
    slug: String,
    parent_slug: Option<String>,
) -> Result<UserFolderDto, UiError> {
    let mb = /* obtain Mailbox from state — mirror folder_rename's state access */;
    let folder = mb.move_user_folder(&slug, parent_slug.as_deref())
        .map_err(UiError::from)?;
    Ok(UserFolderDto::from(folder))
}
```

> Copy the exact `state` access + error-mapping pattern from the adjacent `folder_rename` command (L830-839) — it shows how this file obtains the `Mailbox` and converts `BackendError` → `UiError`.

Register in `lib.rs`'s `tauri::generate_handler![...]` list, next to `folder_rename` / `folder_delete`:

```rust
        folder_move,
```

- [ ] **Step 4: Run to verify pass + build**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ui_commands::` then `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: PASS + clean build (the `invoke_handler` macro compiles `folder_move`).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/ui_commands.rs src-tauri/src/lib.rs
git commit   # "feat(folders): IPC — parentSlug DTO/create arg + folder_move command (tuxlink-ka3z)"
```

---

## Task 7: TS types + hooks (`parentSlug`, create-with-parent, `useMoveUserFolder`)

**Files:**
- Modify: `src/mailbox/types.ts` (`UserFolder` ~L86-90)
- Modify: `src/mailbox/useUserFolders.ts`
- Test: `src/mailbox/useUserFolders.test.tsx`

- [ ] **Step 1: Write failing hook tests**

Mirror the existing mutation-hook tests in `useUserFolders.test.tsx` (they mock `@tauri-apps/api/core`'s `invoke`). Add:

```ts
it('useCreateUserFolder forwards parentSlug to folder_create', async () => {
  const invoke = vi.mocked(coreInvoke);
  invoke.mockResolvedValue({ slug: 'ares', displayName: 'ARES', createdAt: 'x', parentSlug: 'nets' });
  const { result } = renderHook(() => useCreateUserFolder(), { wrapper });
  await act(async () => { await result.current.mutateAsync({ displayName: 'ARES', parentSlug: 'nets' }); });
  expect(invoke).toHaveBeenCalledWith('folder_create', { displayName: 'ARES', parentSlug: 'nets' });
});

it('useMoveUserFolder invokes folder_move with slug + parentSlug', async () => {
  const invoke = vi.mocked(coreInvoke);
  invoke.mockResolvedValue({ slug: 'weather', displayName: 'Weather', createdAt: 'x', parentSlug: 'nets' });
  const { result } = renderHook(() => useMoveUserFolder(), { wrapper });
  await act(async () => { await result.current.mutateAsync({ slug: 'weather', parentSlug: 'nets' }); });
  expect(invoke).toHaveBeenCalledWith('folder_move', { slug: 'weather', parentSlug: 'nets' });
});
```

> Match the existing test file's import names for `invoke` mock + `wrapper` (QueryClient provider). If the existing create test calls `mutateAsync('ARES')` with a bare string, this task changes the create mutation's input shape to an object — update that existing test too.

- [ ] **Step 2: Run to verify fail**

Run: `pnpm vitest run src/mailbox/useUserFolders.test.tsx`
Expected: FAIL — `useMoveUserFolder` undefined; create mutation takes a string.

- [ ] **Step 3: Implement**

In `types.ts`, `UserFolder`:

```ts
export interface UserFolder {
  slug: string;
  displayName: string;
  createdAt: string; // RFC 3339 UTC
  /// Parent folder slug; absent/undefined for a top-level folder (spec D2).
  parentSlug?: string;
}
```

In `useUserFolders.ts`, change the create mutation input + add the move hook:

```ts
export function useCreateUserFolder() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ displayName, parentSlug }: { displayName: string; parentSlug?: string }) =>
      invoke<UserFolder>('folder_create', { displayName, parentSlug }),
    onSuccess: () => { void qc.invalidateQueries({ queryKey: USER_FOLDERS_QUERY_KEY }); },
  });
}

/// Mutation: re-parent a user folder (spec D3). `parentSlug` undefined/absent
/// promotes to top level. Invalidates the folder list so the sidebar tree
/// re-renders. Metadata-only on the backend — no message moves.
export function useMoveUserFolder() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ slug, parentSlug }: { slug: string; parentSlug?: string }) =>
      invoke<UserFolder>('folder_move', { slug, parentSlug }),
    onSuccess: () => { void qc.invalidateQueries({ queryKey: USER_FOLDERS_QUERY_KEY }); },
  });
}
```

> Update the existing `folder_create` caller (`NewFolderDialog`) to pass an object — handled in Task 9, so this task may leave a type error there until Task 9; if so, do Task 9 immediately after and run both test suites together before pushing.

- [ ] **Step 4: Run to verify pass**

Run: `pnpm vitest run src/mailbox/useUserFolders.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/mailbox/types.ts src/mailbox/useUserFolders.ts src/mailbox/useUserFolders.test.tsx
git commit   # "feat(folders): TS parentSlug + create-with-parent + useMoveUserFolder (tuxlink-ka3z)"
```

---

## Task 8: `FolderSidebar` recursive tree render + expand/collapse

**Files:**
- Modify: `src/mailbox/FolderSidebar.tsx` (the `userFolders.map(...)` block ~L225-253)
- Test: `src/mailbox/FolderSidebar.test.tsx`

Group the flat `userFolders` list into top-level folders, each followed by its children indented one level. Add per-parent expand/collapse (default expanded). A folder with children shows a ▸/▾ toggle; a leaf shows the existing `▢` icon.

- [ ] **Step 1: Write failing render tests**

```tsx
const folders: UserFolder[] = [
  { slug: 'nets', displayName: 'Nets', createdAt: 'a' },
  { slug: 'ares', displayName: 'ARES', createdAt: 'b', parentSlug: 'nets' },
  { slug: 'weather', displayName: 'Weather', createdAt: 'c' },
];

it('renders children indented under their parent', () => {
  render(<FolderSidebar userFolders={folders} {...baseProps} />);
  const child = screen.getByTestId('user-folder-ares');
  // child carries a data attribute marking its depth (used for indent).
  expect(child).toHaveAttribute('data-depth', '1');
  expect(screen.getByTestId('user-folder-nets')).toHaveAttribute('data-depth', '0');
});

it('collapsing a parent hides its children', async () => {
  render(<FolderSidebar userFolders={folders} {...baseProps} />);
  await userEvent.click(screen.getByTestId('folder-toggle-nets'));
  expect(screen.queryByTestId('user-folder-ares')).toBeNull();
  // weather (top-level, unaffected) still shows.
  expect(screen.getByTestId('user-folder-weather')).toBeInTheDocument();
});
```

> Pull `baseProps` from the existing test file's render helper (it already constructs the required `onSelectFolder` / `selectedFolder` / etc. props). Reuse it verbatim.

- [ ] **Step 2: Run to verify fail**

Run: `pnpm vitest run src/mailbox/FolderSidebar.test.tsx`
Expected: FAIL — flat render has no `data-depth` / no toggle.

- [ ] **Step 3: Implement the grouped render**

Replace the flat `userFolders.map((uf) => ...)` with a derived tree + render. Compute, before the return:

```tsx
const topLevel = userFolders.filter((f) => !f.parentSlug);
const childrenOf = (slug: string) => userFolders.filter((f) => f.parentSlug === slug);
const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set());
const toggle = (slug: string) =>
  setCollapsed((prev) => { const n = new Set(prev); n.has(slug) ? n.delete(slug) : n.add(slug); return n; });
```

Render each top-level folder (existing button, plus a toggle when it has children and `data-depth="0"`), then, when not collapsed, its children with `data-depth="1"` and left padding. Extract the existing per-folder `<button>` into a local `renderFolderRow(uf, depth)` helper so top-level and child rows share markup; the child row adds `style={{ paddingLeft: 24 }}` and `data-depth={depth}`. The toggle is a small button with `data-testid={`folder-toggle-${uf.slug}`}` rendered only when `childrenOf(uf.slug).length > 0`.

> Keep the existing `data-testid={`user-folder-${uf.slug}`}`, drag handlers, context-menu handler, and active/drop-target classes intact on every row — Task 11 extends the drag handlers; existing message-drop onto a folder must keep working.

- [ ] **Step 4: Run to verify pass**

Run: `pnpm vitest run src/mailbox/FolderSidebar.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/mailbox/FolderSidebar.tsx src/mailbox/FolderSidebar.test.tsx
git commit   # "feat(folders): recursive sidebar tree + expand/collapse (tuxlink-ka3z)"
```

---

## Task 9: `NewFolderDialog` parent context + `FolderContextMenu` "New subfolder here"

**Files:**
- Modify: `src/mailbox/NewFolderDialog.tsx`
- Modify: `src/mailbox/FolderContextMenu.tsx`
- Test: `src/mailbox/FolderContextMenu` test (create if absent) + an existing NewFolderDialog test if present

> Read `NewFolderDialog.tsx` first; it currently calls `useCreateUserFolder().mutateAsync(displayName)`. Change it to pass `{ displayName, parentSlug }` and accept an optional `parentSlug?: string` + `parentName?: string` prop, showing "Inside: {parentName}" when present.

- [ ] **Step 1: Write failing context-menu test**

```tsx
it('shows "New subfolder here" on a top-level folder and hides it on a subfolder', () => {
  const top: UserFolder = { slug: 'nets', displayName: 'Nets', createdAt: 'a' };
  const sub: UserFolder = { slug: 'ares', displayName: 'ARES', createdAt: 'b', parentSlug: 'nets' };
  const { rerender } = render(<FolderContextMenu folder={top} x={0} y={0} {...handlers} />);
  expect(screen.getByTestId('folder-ctx-new-subfolder')).toBeInTheDocument();
  rerender(<FolderContextMenu folder={sub} x={0} y={0} {...handlers} />);
  expect(screen.queryByTestId('folder-ctx-new-subfolder')).toBeNull();
});
```

`handlers` = `{ onRename: vi.fn(), onDelete: vi.fn(), onClose: vi.fn(), onNewSubfolder: vi.fn(), onMoveTo: vi.fn() }`.

- [ ] **Step 2: Run to verify fail**

Run: `pnpm vitest run src/mailbox/FolderContextMenu`
Expected: FAIL — no such item / prop.

- [ ] **Step 3: Implement**

Add `onNewSubfolder: () => void` to `FolderContextMenuProps`. Render, above "Rename…", only when `!folder.parentSlug`:

```tsx
      {!folder.parentSlug && (
        <button type="button" role="menuitem" data-testid="folder-ctx-new-subfolder"
          onClick={() => { onNewSubfolder(); onClose(); }} className="tux-ctx-item">
          New subfolder here…
        </button>
      )}
```

In `NewFolderDialog.tsx`: add `parentSlug?: string; parentName?: string` props; render a `parentName` banner when present; change the submit to `mutateAsync({ displayName, parentSlug })`. The sidebar host (`FolderSidebar` consumer / `MailboxView`) wires `onNewSubfolder` to open `NewFolderDialog` with the right-clicked folder as parent — follow the existing wiring that opens the dialog from the header "+".

- [ ] **Step 4: Run to verify pass**

Run: `pnpm vitest run src/mailbox/FolderContextMenu src/mailbox/NewFolderDialog`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/mailbox/FolderContextMenu.tsx src/mailbox/NewFolderDialog.tsx src/mailbox/*.test.tsx
git commit   # "feat(folders): New subfolder here — context menu + dialog parent context (tuxlink-ka3z)"
```

---

## Task 10: `FolderContextMenu` "Move to…" submenu

**Files:**
- Modify: `src/mailbox/FolderContextMenu.tsx`
- Test: `src/mailbox/FolderContextMenu` test

The submenu lists valid re-parent targets — top-level folders other than the folder itself and other than its current parent — plus "Top level" (shown only when the folder is currently a subfolder). Mirror the message "Move to" list pattern ([MessageContextMenu.tsx:120-139](../../../src/mailbox/MessageContextMenu.tsx#L120-L139)).

- [ ] **Step 1: Write failing test**

```tsx
it('lists valid parents and excludes self, current parent, and subfolders', () => {
  const all: UserFolder[] = [
    { slug: 'nets', displayName: 'Nets', createdAt: 'a' },
    { slug: 'weather', displayName: 'Weather', createdAt: 'b' },
    { slug: 'ares', displayName: 'ARES', createdAt: 'c', parentSlug: 'nets' },
  ];
  // right-clicking 'weather' (top-level, no children): valid targets = nets; plus no "Top level" (already top).
  render(<FolderContextMenu folder={all[1]} allFolders={all} x={0} y={0} {...handlers} />);
  expect(screen.getByTestId('folder-move-nets')).toBeInTheDocument();
  expect(screen.queryByTestId('folder-move-weather')).toBeNull(); // self excluded
  expect(screen.queryByTestId('folder-move-ares')).toBeNull();    // subfolder can't be a parent
  expect(screen.queryByTestId('folder-move-top')).toBeNull();      // already top-level
});

it('offers "Top level" for a subfolder', () => {
  const all: UserFolder[] = [
    { slug: 'nets', displayName: 'Nets', createdAt: 'a' },
    { slug: 'ares', displayName: 'ARES', createdAt: 'c', parentSlug: 'nets' },
  ];
  render(<FolderContextMenu folder={all[1]} allFolders={all} x={0} y={0} {...handlers} />);
  expect(screen.getByTestId('folder-move-top')).toBeInTheDocument();
  expect(screen.queryByTestId('folder-move-nets')).toBeNull(); // current parent excluded
});
```

- [ ] **Step 2: Run to verify fail**

Run: `pnpm vitest run src/mailbox/FolderContextMenu`
Expected: FAIL — no `allFolders` prop / no move items.

- [ ] **Step 3: Implement**

Add props `allFolders: UserFolder[]` and `onMoveTo: (parentSlug: string | undefined) => void`. Compute valid targets:

```tsx
  const hasChildren = allFolders.some((f) => f.parentSlug === folder.slug);
  const targets = hasChildren ? [] : allFolders.filter(
    (f) => !f.parentSlug && f.slug !== folder.slug && f.slug !== folder.parentSlug
  );
  const canPromote = !!folder.parentSlug;
```

Render a "Move to" label + the `targets.map` of `data-testid={`folder-move-${f.slug}`}` items calling `onMoveTo(f.slug)`, plus, when `canPromote`, a `data-testid="folder-move-top"` item calling `onMoveTo(undefined)`. When `hasChildren && !canPromote`, render a disabled hint ("Move subfolders out first to nest this folder") so the absence isn't silent. The host wires `onMoveTo` to `useMoveUserFolder().mutate({ slug: folder.slug, parentSlug })`.

- [ ] **Step 4: Run to verify pass**

Run: `pnpm vitest run src/mailbox/FolderContextMenu`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/mailbox/FolderContextMenu.tsx src/mailbox/FolderContextMenu.test.tsx
git commit   # "feat(folders): Move to… submenu with D4 target filtering (tuxlink-ka3z)"
```

---

## Task 11: Drag-drop folder re-parent

**Files:**
- Modify: `src/mailbox/FolderSidebar.tsx`
- Test: `src/mailbox/FolderSidebar.test.tsx`

Make folder rows draggable; a top-level row is a folder-drop target (nest the dragged folder under it); the "Folders" section header is a drop target (promote to top level). Distinguish folder-drag from the existing message-drag via the `dataTransfer` payload type. Reject invalid drops client-side (mirror D4) and let the backend reject as defense-in-depth.

- [ ] **Step 1: Write failing test**

```tsx
it('dropping a folder onto a top-level folder calls onReparentFolder', () => {
  const onReparentFolder = vi.fn();
  render(<FolderSidebar userFolders={folders} onReparentFolder={onReparentFolder} {...baseProps} />);
  const target = screen.getByTestId('user-folder-nets');
  const dt = makeFolderDataTransfer('weather'); // sets a 'application/x-tuxlink-folder' payload
  fireEvent.drop(target, { dataTransfer: dt });
  expect(onReparentFolder).toHaveBeenCalledWith('weather', 'nets');
});

it('dropping a folder onto the Folders header promotes it to top level', () => {
  const onReparentFolder = vi.fn();
  render(<FolderSidebar userFolders={folders} onReparentFolder={onReparentFolder} {...baseProps} />);
  fireEvent.drop(screen.getByTestId('folders-section-header'), { dataTransfer: makeFolderDataTransfer('ares') });
  expect(onReparentFolder).toHaveBeenCalledWith('ares', undefined);
});
```

- [ ] **Step 2: Run to verify fail**

Run: `pnpm vitest run src/mailbox/FolderSidebar.test.tsx`
Expected: FAIL — no `onReparentFolder` / no folder-drag wiring.

- [ ] **Step 3: Implement**

Add prop `onReparentFolder?: (slug: string, parentSlug: string | undefined) => void`. On each folder row: `draggable`, `onDragStart` sets `e.dataTransfer.setData('application/x-tuxlink-folder', uf.slug)`. On top-level rows: in the existing `onDrop`, branch on payload type — if it's `application/x-tuxlink-folder`, call `onReparentFolder(draggedSlug, uf.slug)` (guard: ignore self-drop and a drop whose dragged folder has children, mirroring D4); else keep the existing message-drop path. Add `data-testid="folders-section-header"` + drop handler to the header that calls `onReparentFolder(draggedSlug, undefined)`. The host wires `onReparentFolder` to `useMoveUserFolder().mutate`.

- [ ] **Step 4: Run to verify pass**

Run: `pnpm vitest run src/mailbox/FolderSidebar.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/mailbox/FolderSidebar.tsx src/mailbox/FolderSidebar.test.tsx
git commit   # "feat(folders): drag-drop folder re-parent (nest + promote) (tuxlink-ka3z)"
```

---

## Task 12: `DeleteFolderDialog` blast-radius line

**Files:**
- Modify: `src/mailbox/DeleteFolderDialog.tsx` (help line ~L109-112)
- Test: `src/mailbox/DeleteFolderDialog` test (create if absent)

- [ ] **Step 1: Write failing test**

```tsx
it('shows a blast-radius line when the folder has subfolders', () => {
  const folder: UserFolder = { slug: 'nets', displayName: 'Nets', createdAt: 'a' };
  render(<DeleteFolderDialog folder={folder} childCount={2} childNames={['SATERN', 'ARES']}
    messageCount={10} onClose={vi.fn()} />);
  const note = screen.getByTestId('delete-folder-blast-radius');
  expect(note).toHaveTextContent('2 subfolders');
  expect(note).toHaveTextContent('SATERN');
});

it('omits the blast-radius line for a leaf folder', () => {
  const folder: UserFolder = { slug: 'drills', displayName: 'Drills', createdAt: 'a' };
  render(<DeleteFolderDialog folder={folder} childCount={0} messageCount={3} onClose={vi.fn()} />);
  expect(screen.queryByTestId('delete-folder-blast-radius')).toBeNull();
});
```

- [ ] **Step 2: Run to verify fail**

Run: `pnpm vitest run src/mailbox/DeleteFolderDialog`
Expected: FAIL — no `childCount` prop / no blast-radius node.

- [ ] **Step 3: Implement**

Add `childCount?: number; childNames?: string[]` to `DeleteFolderDialogProps`. When `childCount && childCount > 0`, render above the radio group:

```tsx
          {childCount && childCount > 0 ? (
            <div data-testid="delete-folder-blast-radius" className="tux-folder-help">
              Will remove {childCount} subfolder{childCount === 1 ? '' : 's'}
              {childNames?.length ? ` (${childNames.join(', ')})` : ''} and affect all messages inside.
            </div>
          ) : null}
```

The host computes `childCount`/`childNames` from `useUserFolders().folders.filter(f => f.parentSlug === folder.slug)` and passes them in.

- [ ] **Step 4: Run to verify pass**

Run: `pnpm vitest run src/mailbox/DeleteFolderDialog`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/mailbox/DeleteFolderDialog.tsx src/mailbox/DeleteFolderDialog.test.tsx
git commit   # "feat(folders): delete dialog blast-radius line for parents (tuxlink-ka3z)"
```

---

## Task 13: Full-stack gate + browser smoke

**Files:** none (verification only)

- [ ] **Step 1: Rust gate**

Run: `cargo clippy --all-targets --manifest-path src-tauri/Cargo.toml -- -D warnings` (re-run until exit 0 — it hides later-target lints), then `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: clean clippy + all tests pass.

- [ ] **Step 2: TS gate**

Run: `pnpm vitest run` (full suite — the CI `verify` gate runs the whole suite; a scoped run can miss a contract/snapshot test elsewhere)
Expected: all pass.

- [ ] **Step 3: Browser smoke (mandatory — jsdom cannot verify layout)**

Launch a fresh `tauri dev` (Ctrl+R is a no-op for code changes — restart), create `Nets`, create subfolder `ARES` under it, verify at the real ~200px sidebar width: indentation reads cleanly, the ▸/▾ toggle collapses/expands, drag `Weather` onto `Nets` nests it and drag back to the header promotes it, and the delete dialog shows the blast-radius line. Capture via grim per `grim_realapp_validation_pandora` (WebKitGTK, not Chromium — memory `chromium_not_webkitgtk_proxy`). This is operator-runnable; the agent makes it runnable + observable.

- [ ] **Step 4: Push**

```bash
git push   # pre-push runs lint:docs + build gates; do NOT --no-verify
```

- [ ] **Step 5: Open PR**

```bash
gh pr create --base main --head bd-tuxlink-ka3z/nested-folders \
  --title "[slate-glade-sparrow] feat: nested user folders (tuxlink-ka3z)" --body "..."
```

---

## Self-Review (completed by author)

**Spec coverage:** D1 depth-cap → enforced by `validate_reparent` (T2) + create (T3) + UI target-filtering (T8/T10/T11). D2 schema/migration → T1. D3 re-parent (drag + menu) → T4 backend, T10 menu, T11 drag. D4 rule set → T2 (single source), reused by T3/T4 and mirrored client-side T10/T11. D5 create flow → T9. D6 delete cascade → T5 backend, T12 dialog. IPC → T6. Forward-compat note (§6) is documentation-only, no task. ✔ all decisions mapped.

**Placeholder scan:** Backend tasks (T1–T6) carry exact code. UI tasks (T8–T12) specify exact test code, exact testids, and the precise edit shape; a few host-wiring touchpoints (which parent component opens the dialog / wires `onMoveTo`) say "follow the existing wiring" because that consumer wasn't read in full during planning — the executor must read the `FolderSidebar` consumer before T9/T10/T11 and follow its established open-dialog pattern. Flagged here rather than hidden.

**Type consistency:** `parent_slug` (Rust, snake) ↔ `parentSlug` (TS/DTO, camel via serde rename) used consistently. `validate_reparent(reg, slug, Option<&str>)`, `children_slugs(reg, slug)`, `move_user_folder(slug, Option<&str>)`, `useMoveUserFolder({slug, parentSlug})`, `onReparentFolder(slug, parentSlug|undefined)` — names stable across tasks. `DeleteAction` reused unchanged (made `Copy` in T5).
