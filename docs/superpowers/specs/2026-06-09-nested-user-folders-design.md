# Nested user folders — Phase 3 hierarchy design

**Date:** 2026-06-09
**Author:** slate-glade-sparrow
**bd issue:** tuxlink-ka3z (nested folders, Phase 3)
**Status:** Accepted 2026-06-09 — design decisions D1–D6 below.
**Supersedes:** D3 ("Flat structure, no nesting in v1") of [2026-06-02-user-folders-design.md](2026-06-02-user-folders-design.md), whose Phase 3 explicitly deferred nested folders.
**Mock companion (local-only, gitignored):** `.superpowers/brainstorm/1489878-1780987670/content/` — `nesting-depth.html`, `delete-with-children.html`, `create-and-reparent.html`. Re-render via the brainstorming visual-companion server.

---

## 1. Premise

User folders are flat today. Phase 2 ([2026-06-02-user-folders-design.md](2026-06-02-user-folders-design.md), tuxlink-f62f) shipped create / rename / delete / move-message for a single flat tier of folders under the mailbox root. This spec adds **one level of nesting** — a top-level folder may contain subfolders — so an operator can group related folders ("net traffic by net", "weather by region").

The design's organizing principle: **a folder's identity (its slug) is independent of its location.** Slugs are globally unique and the on-disk directory layout stays flat; the hierarchy is metadata in the registry. This makes re-parenting a metadata edit rather than a filesystem move, keeps message files immovable during reorganization, and bounds every operation to a two-level tree.

### 1.1 What Phase 2 already provides

- `UserFolder = { slug, display_name, created_at }` in [src-tauri/src/user_folders.rs](../../../src-tauri/src/user_folders.rs); TS mirror in [src/mailbox/types.ts](../../../src/mailbox/types.ts).
- `.folders.json` registry with a forward-compat `version: 1` field and a `folders: Vec<UserFolder>` adjacency-free flat list.
- On-disk layout: `folder_dir(slug) = root.join(slug)` ([src-tauri/src/native_mailbox.rs:213](../../../src-tauri/src/native_mailbox.rs#L213)) — one flat directory per folder at the mailbox root, messages slug-keyed.
- Message move into a folder: drag-drop onto a folder row ([src/mailbox/FolderSidebar.tsx](../../../src/mailbox/FolderSidebar.tsx)) and a right-click "Move to ▸ Folders" submenu ([src/mailbox/MessageContextMenu.tsx:120-139](../../../src/mailbox/MessageContextMenu.tsx#L120-L139)).
- Delete dialog with three message-disposition modes — move-to-Inbox (default), move-to-Archive, delete-permanently ([src/mailbox/DeleteFolderDialog.tsx](../../../src/mailbox/DeleteFolderDialog.tsx)).
- Delete cascade modes already implemented backend-side: `delete_user_folder` with move-to-inbox vs delete-cascade message handling ([src-tauri/src/native_mailbox.rs:314](../../../src-tauri/src/native_mailbox.rs#L314)).

The nested-folders feature reuses all of the above. The new surface is small: one schema field, one new backend command, one extended command, and a recursive sidebar render.

---

## 2. Decisions

### D1 — Nesting depth: capped at 2 levels

The folder hierarchy is capped at **two levels**: a top-level folder may contain subfolders, and a subfolder is a leaf (it cannot contain folders).

**Rationale.** The operator use case is two levels ("net traffic by net", "weather by region"). The sidebar is ~200 px wide; a third indent level truncates realistic display names ("King County" → "King Co…"). The cap is a validation constant, not a stored property — raising it to 3 later is a one-line change with no data migration. Lowering an arbitrary-depth tree, by contrast, would force re-flattening operators' real trees. The asymmetry favors starting conservative.

**Lift condition.** Raise the cap only on explicit operator feedback citing a valid use case for deeper nesting.

**Cycle prevention is structural.** Because a subfolder cannot gain children, no cycle can form. The depth-cap validation (D4) is the sole enforcement mechanism; no descendant-walk is required.

### D2 — Schema-v2 migration shape: flat on disk, hierarchy in registry metadata

`UserFolder` gains an optional parent reference; the on-disk directory layout does not change.

```rust
pub struct UserFolder {
    pub slug: String,
    pub display_name: String,
    pub created_at: String,
    #[serde(default)]
    pub parent_slug: Option<String>, // None = top-level folder
}
```

- The `.folders.json` registry becomes an **adjacency list**: each folder names its parent by slug, or `None` for a top-level folder.
- **Slugs stay globally unique** (already enforced by the reserved-name + uniqueness checks in [user_folders.rs](../../../src-tauri/src/user_folders.rs)). A subfolder's directory remains `root.join(slug)` — the parent does **not** appear in the path. `folder_dir` is unchanged.
- The TS mirror in [types.ts](../../../src/mailbox/types.ts) gains optional `parentSlug`.

**Migration is near-transparent.** `#[serde(default)]` means an existing `version: 1` registry — whose `UserFolder` records have no `parent_slug` field — deserializes cleanly with every folder as top-level, which is the correct interpretation of a flat registry. **No data-transform pass and no directory relocation.** However, the version field is NOT auto-bumped by changing `Registry::default` alone — a deserialized v1 registry retains `version: 1` and `save_registry` would write it back unchanged. An explicit `normalize_to_current` step (set `version = CURRENT_REGISTRY_VERSION`, run `validate_registry` self-heal) MUST run on load before any mutation so the next save persists `version: 2`. A registry whose `version` exceeds `CURRENT_REGISTRY_VERSION` MUST be rejected with a surfaced error, never silently defaulted or overwritten (forward-corruption guard). The serialized `parent_slug`/`parentSlug` field MUST use `#[serde(skip_serializing_if = "Option::is_none")]` so a top-level folder emits no key (not `null`), matching the TS `parentSlug?: string` optional shape. *(See the Codex R1 amendments at the end of the implementation plan for the validated specifics.)*

**Why flat-on-disk over nested-on-disk.** Decoupling identity (slug) from location (path) makes re-parenting (D3) a single registry-field edit with zero file moves, and keeps message files outside the blast radius of every reorganize and delete-cascade. A nested-on-disk layout (`root/<parent>/<child>/`) would make re-parenting and delete-cascade real directory moves on operator message data, each carrying partial-move / crash-mid-operation / permission failure modes, and would require a genuine migration pass that physically relocates v1 directories. Flat-on-disk stores "less" but puts only the trivially-rewritable registry — never message files — at risk during structural change.

### D3 — Re-parenting: metadata-only, via drag-drop and a "Move to…" menu

Moving an existing folder under another top-level folder, or back to top level, edits the moved folder's `parent_slug` in `.folders.json`. **No message file moves**, regardless of how many messages the folder holds.

Two surfaces share one validation rule set (D4):

- **Drag-drop** — drag a folder row onto a top-level folder row to nest it; drag it onto the "Folders" section header to promote it back to top level. Extends the drag-drop mechanism already built for messages.
- **Context-menu "Move to…" submenu** — right-click a folder → "Move to ▸" listing valid parent targets plus "Top level". Mirrors the shipped message "Move to" pattern ([MessageContextMenu.tsx:120-139](../../../src/mailbox/MessageContextMenu.tsx#L120-L139)); invalid targets are excluded from the list.

Both surfaces are provided in v1. The context-menu surface is near-free (it mirrors existing message-move UI); the drag-drop surface honors the expectation set by message drag-drop (a folder that looks draggable should be).

### D4 — Re-parent and create validation (shared rule set)

A re-parent of folder `S` to parent `P` (where `P = None` means "top level") is valid iff:

1. `P` is not `S` (no self-parent).
2. `P` is `None`, or `P` is an existing **top-level** folder (a subfolder cannot be a parent — enforces the 2-level cap, D1).
3. `S` has **no children** when `P` is a folder (moving a folder-with-children under a parent would create a third level — rejected). `S`-with-children may still move to top level.
4. `P` exists in the registry (when not `None`).

Folder creation validation extends Phase 2's rules (slug shape, reserved names, global uniqueness) with: a `parent_slug` argument, when present, must reference an existing top-level folder (rules 2 + 4).

The "Move to…" submenu lists only targets that satisfy these rules; drag-drop drop-targets reject a drop that violates them. Both surfaces return the same `BackendError` on a server-side validation failure (defense in depth).

### D5 — Create flow

- The global "+" button in the sidebar "Folders" header creates a **top-level** folder (unchanged from Phase 2).
- Right-click a **top-level** folder → **"New subfolder here"** → the existing `NewFolderDialog` ([src/mailbox/NewFolderDialog.tsx](../../../src/mailbox/NewFolderDialog.tsx)), prefilled with the parent context (the dialog shows which folder the new subfolder lands in).
- Right-click a **subfolder** → the "New subfolder here" item is **hidden** (the 2-level cap forbids it). Rename, Delete, and "Move to…" (D3) remain — a subfolder can be re-parented to another top-level folder or promoted to top level.

### D6 — Delete with children

Deleting a **leaf** (a subfolder, or a childless top-level folder) is unchanged: the existing three-mode dialog disposes of its messages.

Deleting a **top-level folder that has subfolders**:

- The same three message-disposition modes apply (move-to-Inbox default / move-to-Archive / delete-permanently), applied to **all** messages in the parent folder and its direct children.
- The subfolders are removed along with the parent.
- The delete dialog gains a **blast-radius line** stating the scope before confirmation, e.g. "Will remove 2 subfolders (SATERN, ARES) and all messages they contain." The line names the subfolder count + names (derivable from the already-loaded folder list); it does **not** assert a total message count — per-user-folder message counts are not currently surfaced to the host (`FolderSidebar` defers them), so a numeric total would require a new backend count command. Omitting the number is the YAGNI default (Codex R1 finding #11); a future count command can add it.
- The default remains the **non-destructive** mode (move-to-Inbox); the permanent-delete mode stays a separately-styled, explicit choice. Data loss is the only irreversible consequence in this feature, so deletion is never the path of least resistance.
- The cascade is **one level deep** (cap-bounded): the parent plus its direct children, gathered from the registry by `parent_slug`. No recursive descent.

---

## 3. Backend / IPC

| Command | Change |
|---|---|
| `folder_create(display_name, parent_slug?)` | New optional `parent_slug` argument; validated per D4. |
| `folder_move(slug, new_parent_slug?)` | **New** command. Registry-only re-parent; validation per D4; no filesystem move. |
| `delete_user_folder(...)` | Extend to gather direct children by `parent_slug` and dispose of parent + children per the chosen mode (D6). |
| `mailbox_move(from, to, id)` | **Unchanged.** Nested folders render as additional slug targets; message move is already slug-keyed and location-independent. |
| `folder_dir(slug)` | **Unchanged** — `root.join(slug)`, flat. |

Registry read/write gains `version: 2` handling and the `parent_slug` field per D2.

---

## 4. Frontend

| File | Change |
|---|---|
| [FolderSidebar.tsx](../../../src/mailbox/FolderSidebar.tsx) | Flat `.map()` → grouped render: each top-level folder followed by its children indented one level, with per-parent expand/collapse. Empty-state hint preserved. Drag-drop re-parent: folder rows draggable; top-level rows and the section header are folder drop-targets (D3). |
| [FolderContextMenu.tsx](../../../src/mailbox/FolderContextMenu.tsx) | Add "New subfolder here" (shown only on top-level folders, D5) and a "Move to…" submenu (valid targets per D4). |
| [NewFolderDialog.tsx](../../../src/mailbox/NewFolderDialog.tsx) | Accept + display parent context when creating a subfolder. |
| [DeleteFolderDialog.tsx](../../../src/mailbox/DeleteFolderDialog.tsx) | Blast-radius line when the target has subfolders (D6). |
| [useUserFolders.ts](../../../src/mailbox/useUserFolders.ts) | Create mutation gains `parentSlug`; add `useMoveUserFolder`. |
| [types.ts](../../../src/mailbox/types.ts) | `UserFolder` gains optional `parentSlug`. |

---

## 5. Testing

**Rust ([user_folders.rs](../../../src-tauri/src/user_folders.rs) + [native_mailbox.rs](../../../src-tauri/src/native_mailbox.rs)):**
- Schema-v2 round-trip (serialize/deserialize with and without `parent_slug`).
- v1→v2 transparent load: a `version: 1` registry with no `parent_slug` fields loads with all folders top-level; next write emits `version: 2`.
- `folder_create` with a valid parent; rejection when parent is a subfolder / missing / reserved.
- `folder_move` validation: reject self-parent, subfolder-as-parent (depth cap), missing parent, and moving a folder-with-children under a parent; accept move-to-top-level for a folder-with-children.
- Delete cascade: parent + direct children, each disposition mode (move-to-Inbox / move-to-Archive / delete) relocating or removing messages across both tiers.

**TypeScript ([FolderSidebar.test.tsx](../../../src/mailbox/FolderSidebar.test.tsx) + new):**
- Tree render: subfolders indent under their parent; expand/collapse toggles child visibility.
- "New subfolder here" visible on top-level folders, hidden on subfolders.
- "Move to…" target filtering matches D4.
- Drag-drop re-parent: valid drop re-parents; invalid drops (self, onto subfolder, folder-with-children onto a parent) are rejected.
- Delete dialog renders the blast-radius line with correct subfolder + message counts.

**Browser smoke (mandatory, jsdom cannot verify):** the recursive tree at the real ~200 px sidebar width — indentation, expand/collapse affordance, drag-drop target highlighting, name legibility at depth 2.

---

## 6. Forward-compatibility: WLE message import

A future feature lets operators import legacy Winlink Express ("WLE") mailboxes into Tuxlink. This nested-folders design is deliberately **import-neutral and import-friendly**, and does not constrain that feature:

- An importer is a converter that writes through Tuxlink's folder/message APIs (`folder_create`, write-message); it never mirrors WLE's on-disk shape into Tuxlink's. The flat-on-disk decision (D2) does not tie Tuxlink's format to WLE.
- WLE user folders are flat (to be re-verified against prior-art implementations such as Pat when the import feature is specced, as WLE internals are an unreliable area for general knowledge). Import therefore maps WLE folders to Tuxlink **top-level** folders; nesting is a superset the operator applies afterward.
- Under the metadata-hierarchy model (D2), an operator who imports a large mailbox and then organizes its folders into a hierarchy pays only registry edits — no mass file moves on freshly-imported message data.

The hard parts of WLE import (reading WLE's message store, mapping MIDs / read-state / attachments / timestamps, import idempotency) live entirely above the folder-layout layer and belong in the import feature's own spec. `parent_slug` remains Tuxlink's own concept; the schema does not adopt WLE folder semantics.

---

## 7. Discipline note

This feature changes a **persisted schema** and is data-adjacent (the only irreversible consequence in scope is message loss during delete-cascade). Per the project's discipline-triage rule, it is a hard-to-undo decision and runs through `build-robust-features` — including the cross-provider Codex adversarial review — before TDD. The adversarial round's highest-value target is the D4 validation rule set: it is the only logic that, if incomplete, could leave folders in an invalid state.
