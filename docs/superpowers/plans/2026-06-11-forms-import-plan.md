# In-app Form Import Implementation Plan (Forms-push G5 + G6 + G11)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an in-app "Import…" flow so a stuck onboarding member can bring a third-party / organization's Winlink forms (single `.html`, a folder, or a `.zip`) into tuxlink's custom-forms dir, see a clear validate-before-write report, confirm overwrites, and have the forms appear in the catalog — in a real build — plus a short in-app help entry.

**Architecture:** Two-phase (preview → commit) over a `0700` temp staging dir. `forms_import_preview` stages + validates + classifies and mints an opaque registry token; `forms_import_commit` consumes the token single-shot, re-classifies under a shared lock, and atomically promotes into `custom_root` with `.prev` backup + rollback (mirroring `forms::updater::install`). Detection is driven by the `.txt` `Form:` directive (NOT an HTML heuristic) so it matches the real Windows-1252 bundle. A new `src-tauri/src/forms/import.rs` backend module + `src/compose/ImportSheet.tsx` frontend, wired into the existing `CatalogBrowser`. Also closes the `/folder/*` CSP exfil hole and adds custom-form uninstall.

**Tech Stack:** Rust (Tauri v2, `zip` deflate, `tempfile`, `tauri-plugin-shell`, `axum`), React/TypeScript (Vitest + jsdom), serde-tagged error enums.

**Source of truth:** `docs/superpowers/specs/2026-06-11-forms-import-design.md` — **§11 (adversarial-review dispositions) supersedes §1–10 where they conflict.** This plan implements the §11 revision.

**Worktree:** `worktrees/bd-tuxlink-z0le-forms-import` · branch `bd-tuxlink-z0le/forms-import`.

**Issues:** `tuxlink-z0le` (G5 single-file) · `tuxlink-fwob` (G6 bundle) · `tuxlink-48uc` (G11 help). Epic `tuxlink-zkuk`. Engine-identity follow-up: `tuxlink-8v3l` (OUT of this PR).

**Gate (non-negotiable):** the resulting PR is **NOT merge-ready** until the deferred cross-provider Codex adversarial round (`tuxlink-yqo4`, quota returns Jun 13) runs on the diff and is dispositioned. Build proceeds now; the merge gate holds.

---

## Conventions for every task

**TDD preamble (do this BEFORE writing code in any task):**
1. Read the skill at `.claude/skills/test-driven-development/` (or invoke `/test-driven-development`).
2. Read `docs/pitfalls/testing-pitfalls.md` and `docs/pitfalls/implementation-pitfalls.md`.
3. Follow TDD: write the failing test → run it to confirm it fails for the right reason → implement minimally → run to green → commit.

**Completion check (BEFORE marking any task complete):**
1. Review your tests against `docs/pitfalls/testing-pitfalls.md` (error paths? edge cases? no jsdom-can't-see-CSS traps?).
2. Run the relevant test subset and confirm green.
3. Commit with a conventional-commit subject and the trailers:
   ```
   Agent: delta-magpie-bog
   Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
   ```

**After every logical group of tasks (backend core / commit / frontend):** review the batch from multiple perspectives. Minimum three review rounds; if the third still finds substantive issues, keep going until clean. Then continue.

**Backend test command:** `cargo test --manifest-path src-tauri/Cargo.toml forms::import` (scope to the module; widen as needed). Clippy gate: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` (re-run until exit 0 — it hides later-target lints behind earlier ones).

**Frontend test command:** `pnpm -C . vitest run src/compose/ImportSheet` (scope; do NOT `pkill -f vitest` — it self-matches and kills the live run, memory `vitest-pkill-self-match`). Typecheck: `pnpm -C . typecheck`.

**Commit cwd discipline:** bash cwd can silently revert from the worktree to the main checkout mid-session (memory `pin_paths_in_worktree_sessions`). Pin `--manifest-path`/`-C` to absolute or worktree-relative paths; run a standalone `cd <worktree>` as its own Bash call before any `git` op, and avoid the literal token `merge-base` in commands (the main-checkout hook substring-matches it).

---

## File structure

**Backend — new:**
- `src-tauri/src/forms/import.rs` — the import engine: types, detection, path-safety, staging, classification, commit, cancel, reaper. Registered in `src-tauri/src/forms/mod.rs` as `pub mod import;`.

**Backend — modified:**
- `src-tauri/src/forms/mod.rs` — add `pub mod import;`.
- `src-tauri/src/forms/wle_templates.rs` — make `walk_html`'s extension filter accept `.html` AND `.htm` case-insensitively (so import detection == enumeration); expose `is_authoring_template_stem` to `import.rs` (it is already `pub(crate)`); add a `pub(crate)` helper `stem_id(path) -> Option<String>` if useful (optional). NO `(folder,id)` engine change (that's `tuxlink-8v3l`).
- `src-tauri/src/forms/updater.rs` — promote `INSTALL_LOCK` to `pub(crate)` so `import::commit` can share the forms-data-dir mutex.
- `src-tauri/src/forms/http_server.rs` — close the `/folder/*` CSP hole in `folder_handler` (§11.5).
- `src-tauri/src/ui_commands.rs` — add the Tauri commands: `forms_import_preview`, `forms_import_commit`, `forms_import_cancel`, `open_forms_folder`, `forms_custom_delete`. (Thin wrappers delegating to `forms::import`.)
- `src-tauri/src/lib.rs` — `.manage(ImportStagingRegistry)`, register the 5 new commands in `invoke_handler`, run the boot-time staging sweep in `setup`.

**Frontend — new:**
- `src/compose/ImportSheet.tsx` + `src/compose/ImportSheet.css` — the import sheet (source picker → preview report → confirm-overwrites → commit result).
- `src/compose/importApi.ts` — typed invoke bindings + TS mirror types for `ImportPlan` / `ImportEntry` / `ImportResult` / `ImportError`.

**Frontend — modified:**
- `src/compose/CatalogBrowser.tsx` — header `Import…` control, empty-custom-state CTA, footer `Open forms folder`, custom-categories-first sort, per-custom-form Remove, post-commit refresh+highlight, Escape state-machine extension, cancel-on-unmount.

**Docs — new/modified:**
- `docs/help/forms-import.md` (or the project's existing in-app help bundle location — confirm in Task 15) — G11 help entry.
- `dev/implementation-log.md` — top entry on completion.

---

## TASK GROUP A — Backend types + detection core

### Task 1: `forms::import` module skeleton + serde-tagged types

**Files:**
- Create: `src-tauri/src/forms/import.rs`
- Modify: `src-tauri/src/forms/mod.rs` (add `pub mod import;` after `pub mod http_server;`)

- [ ] **Step 1: Write the failing test** — serde shapes are the frontend contract; lock them.

```rust
// in src-tauri/src/forms/import.rs, #[cfg(test)] mod tests
#[test]
fn import_entry_serializes_camelcase_with_kind_tag() {
    let e = ImportEntry {
        rel_path: "AAMRON/Net Check-in Initial.html".into(),
        id: "Net Check-in Initial".into(),
        folder: "AAMRON".into(),
        kind: ImportKind::Added,
        reason: None,
        has_viewer: true,
    };
    let v = serde_json::to_value(&e).unwrap();
    assert_eq!(v["relPath"], "AAMRON/Net Check-in Initial.html");
    assert_eq!(v["kind"], "added");
    assert_eq!(v["hasViewer"], true);
    assert!(v["reason"].is_null());
}

#[test]
fn import_error_serializes_tag_content() {
    // Mirrors UiError's externally-tagged JSON so the frontend can switch on it.
    let err = ImportError::CommitConflict {
        reason: "catalog changed, re-preview".into(),
    };
    let v = serde_json::to_value(&err).unwrap();
    assert_eq!(v["kind"], "commitConflict");
    assert_eq!(v["reason"], "catalog changed, re-preview");
}
```

- [ ] **Step 2: Run to verify it fails** — `cargo test --manifest-path src-tauri/Cargo.toml forms::import::tests::import_entry` → FAIL (types not defined).

- [ ] **Step 3: Implement the types**

```rust
//! In-app form import (Forms-push G5+G6). Two-phase validate-before-write:
//! `preview` stages+classifies under a 0700 temp dir and mints a registry
//! token; `commit` consumes the token single-shot and atomically promotes
//! the validated set into the custom-forms dir. Detection is .txt-`Form:`-
//! directive-driven (spec §11.1). See docs/superpowers/specs/2026-06-11-forms-import-design.md.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportKind {
    Added,
    Update,           // collides with an existing CUSTOM form → needs operator confirm
    OverridesStandard,// shadows a BUNDLED form (intended; warn, no confirm)
    Companion,        // viewer/.txt/asset — copied, never catalog-surfaced
    Skip,             // intra-batch or cross-folder stem dupe
    Reject,           // failed validation; never written
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportEntry {
    pub rel_path: String,        // path relative to the (unwrapped) source root
    pub id: String,              // filename stem
    pub folder: String,          // category folder (relative; "" = root)
    pub kind: ImportKind,
    pub reason: Option<String>,  // populated for Skip/Reject + amber warnings
    pub has_viewer: bool,        // false → "no viewer" amber note on an authoring form
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPlan {
    pub staging_token: String,
    pub entries: Vec<ImportEntry>,
    pub summary: ImportSummary,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub added: usize,
    pub updated: usize,
    pub overrides_standard: usize,
    pub skipped: usize,
    pub rejected: usize,
    pub companions: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub installed: Vec<String>,  // ids written
    pub skipped_updates: Vec<String>, // Update entries the operator did NOT confirm
    pub entries: Vec<ImportEntry>,    // realized per-file outcome
}

/// Whole-operation failure (NOT per-entry — those live in ImportResult).
/// Externally tagged to mirror `UiError`'s frontend-switchable JSON.
#[derive(Debug, Clone, Serialize, thiserror::Error)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ImportError {
    #[error("import token expired or already used")]
    TokenExpired,
    #[error("staging failed: {reason}")]
    StagingFailed { reason: String },
    #[error("catalog changed during import: {reason}")]
    CommitConflict { reason: String },
    #[error("io error: {reason}")]
    Io { reason: String },
}
```

Add to `mod.rs`: `pub mod import;`.

- [ ] **Step 4: Run to verify green** — same command → PASS.
- [ ] **Step 5: Commit** — `feat(forms): import module skeleton + serde-tagged types (tuxlink-z0le)`.

---

### Task 2: `.txt`-directive detection + companion resolution + orphan-HTML fallback

This is the heart of §11.1. The import **unit is the `.txt` template**, not the HTML.

**Files:**
- Modify: `src-tauri/src/forms/import.rs`

- [ ] **Step 1: Write the failing tests** (build a temp fixture tree mirroring the real bundle — Windows-1252 bytes, `{FormServer}` placeholder, a viewer with no `Initial` suffix, an orphan readme).

```rust
// helper at top of tests mod
fn write_bytes(p: &std::path::Path, b: &[u8]) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, b).unwrap();
}

#[test]
fn detects_authoring_form_via_txt_form_directive() {
    let td = tempfile::tempdir().unwrap();
    let root = td.path();
    // .txt template binds input + viewer
    write_bytes(&root.join("AAMRON/Net Check-in.txt"),
        b"Form: Net Check-in Initial.html, Net Check-in Viewer.html\r\nMsg Type: ...\r\n");
    // authoring HTML: Windows-1252 (0x92 = right single quote), unsubstituted action
    write_bytes(&root.join("AAMRON/Net Check-in Initial.html"),
        b"<html><body><form method=post enctype=multipart/form-data \
          action=\"http://{FormServer}:{FormPort}\">It\x92s here</form></body></html>");
    write_bytes(&root.join("AAMRON/Net Check-in Viewer.html"), b"<html>viewer</html>");

    let cands = detect_candidates(root).unwrap();
    let authoring: Vec<_> = cands.iter().filter(|c| c.kind == CandidateKind::Authoring).collect();
    assert_eq!(authoring.len(), 1);
    assert_eq!(authoring[0].id, "Net Check-in Initial");
    assert_eq!(authoring[0].folder, "AAMRON");
    assert!(authoring[0].has_viewer, "viewer named in the .txt directive");
    // The viewer is a companion, never an authoring candidate.
    assert!(cands.iter().any(|c| c.kind == CandidateKind::Companion
        && c.rel_path.ends_with("Net Check-in Viewer.html")));
}

#[test]
fn authoring_form_with_zero_form_tag_still_detected_via_txt() {
    // §11.1: some real authoring forms (ARC 213 Message Initial.html) contain NO <form>.
    let td = tempfile::tempdir().unwrap();
    let root = td.path();
    write_bytes(&root.join("ARC 213 Message.txt"),
        b"Form: ARC 213 Message Initial.html\r\n");
    write_bytes(&root.join("ARC 213 Message Initial.html"),
        b"<html><body>no form element here, JS builds it</body></html>");
    let cands = detect_candidates(root).unwrap();
    assert!(cands.iter().any(|c| c.kind == CandidateKind::Authoring
        && c.id == "ARC 213 Message Initial"),
        "trust the .txt directive even when the <form> probe is inconclusive");
}

#[test]
fn orphan_html_with_form_action_placeholder_is_authoring() {
    let td = tempfile::tempdir().unwrap();
    let root = td.path();
    // no .txt governs this one → fallback HTML probe
    write_bytes(&root.join("Loose Initial.html"),
        b"<form METHOD=POST EncType=Multipart/Form-Data action='HTTP://LOCALHOST:8001'>x</form>");
    let cands = detect_candidates(root).unwrap();
    assert!(cands.iter().any(|c| c.kind == CandidateKind::Authoring && c.id == "Loose Initial"));
}

#[test]
fn orphan_non_form_html_is_rejected_not_added() {
    let td = tempfile::tempdir().unwrap();
    let root = td.path();
    write_bytes(&root.join("readme.html"), b"<html><body>About our group</body></html>");
    let cands = detect_candidates(root).unwrap();
    let r = cands.iter().find(|c| c.rel_path.ends_with("readme.html")).unwrap();
    assert_eq!(r.kind, CandidateKind::Reject);
    assert!(r.reason.as_deref().unwrap().contains("not a Winlink form"));
}

#[test]
fn viewer_and_sendreply_stems_are_companions_never_authoring() {
    // §11.1: run the SAME is_authoring_template_stem filter the catalog applies.
    let td = tempfile::tempdir().unwrap();
    let root = td.path();
    // orphan viewer (no .txt) that DOES contain <form> — must not import as a compose option
    write_bytes(&root.join("Foo Viewer.html"),
        b"<form method=post enctype=multipart/form-data action=\"http://{FormServer}:{FormPort}\">x</form>");
    write_bytes(&root.join("Foo SendReply.html"),
        b"<form method=post enctype=multipart/form-data action=\"http://{FormServer}:{FormPort}\">x</form>");
    let cands = detect_candidates(root).unwrap();
    assert!(cands.iter().all(|c| c.kind != CandidateKind::Authoring),
        "Viewer/SendReply stems are companions even when they contain a form");
}

#[test]
fn reads_windows1252_without_panicking() {
    let td = tempfile::tempdir().unwrap();
    let root = td.path();
    // lone 0x92/0xA0 bytes are invalid UTF-8 — read_to_string().unwrap() would panic
    write_bytes(&root.join("W.txt"), b"Form: W Initial.html\r\n");
    write_bytes(&root.join("W Initial.html"),
        &[b'<', b'f', b'o', b'r', b'm', b' ', 0x92, 0xA0, b'>']);
    let cands = detect_candidates(root).unwrap(); // must not panic
    assert!(!cands.is_empty());
}
```

- [ ] **Step 2: Run to verify fail** — FAIL (`detect_candidates`/`Candidate` undefined).

- [ ] **Step 3: Implement detection.** Key rules (§11.1):
  - Walk the source root. Read every file as **bytes**; decode with `String::from_utf8_lossy` (NEVER `read_to_string().unwrap()`).
  - First pass — parse every `.txt` (case-insensitive ext): extract the `Form:` directive (`Form: input.html[, display.html]`), `Attach:`, `ReplyTemplate:`. The named **input** is an `Authoring` candidate (trust the directive even if the `<form>` probe is inconclusive); the named **display** is a `Companion`. Resolve directive filenames against on-disk siblings **case-insensitively** (reuse the approach in `wle_templates::resolve_viewer_for`). Record the governed HTML filenames in a `governed: HashSet<PathBuf>`.
  - Second pass — every `.html`/`.htm` (case-insensitive) NOT in `governed`: it's an orphan. If `is_authoring_template_stem(stem)` is false → `Companion` (viewer/sendreply). Else run the fallback HTML probe: lossy-decode, lowercase, and accept iff it contains a `<form` with `method=post`, `enctype=multipart/form-data`, AND an `action=` whose value contains any of `{formserver}` / `{formport}` / `localhost` / `127.0.0.1` (all matched case-insensitively on the lowered text; tolerate single/double quotes and whitespace via a tolerant substring scan, not a strict regex). Pass → `Authoring`; fail → `Reject{reason:"not a Winlink form"}`.
  - `has_viewer` for an authoring candidate = true iff its `.txt` named a display form that exists on disk, OR a sibling viewer resolves via `resolve_viewer_for`.
  - Metadata files (`*.dat`, `Changelog.txt`, `*_Version.dat`, case-insensitive) → skip silently (do NOT emit a `Reject` row; §11.2).

```rust
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CandidateKind { Authoring, Companion, Reject }

#[derive(Debug, Clone)]
pub(crate) struct Candidate {
    pub id: String,
    pub folder: String,
    pub rel_path: String,
    pub abs_path: PathBuf,
    pub kind: CandidateKind,
    pub reason: Option<String>,
    pub has_viewer: bool,
}

/// Decode any file as text without panicking on Windows-1252 byte sequences.
fn read_text_lossy(p: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(p)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn is_metadata_file(name_lower: &str) -> bool {
    name_lower.ends_with(".dat")
        || name_lower == "changelog.txt"
        || name_lower.ends_with("_version.dat")
}

/// Tolerant probe: is this HTML a Winlink authoring form? (orphan fallback only)
fn html_looks_like_form(lowered: &str) -> bool {
    if !lowered.contains("<form") { return false; }
    let has_post = lowered.contains("method=post") || lowered.contains("method =post")
        || lowered.contains("method= post") || lowered.contains("method = post")
        || lowered.contains("method=\"post\"") || lowered.contains("method='post'");
    let has_multipart = lowered.contains("multipart/form-data");
    let targets_local = ["{formserver}", "{formport}", "localhost", "127.0.0.1"]
        .iter().any(|t| lowered.contains(t));
    has_post && has_multipart && targets_local
}
// detect_candidates(root): two-pass as described; returns Vec<Candidate>.
```

> **Implementation note (interpretation drift guard):** do NOT "improve" the probe into a full HTML parser or pull in a parsing crate — the tolerant lowercased substring scan is the chosen design (real forms are Windows-1252 and frequently malformed HTML; a strict parser rejects valid forms). Do NOT relax `is_authoring_template_stem` — reuse it verbatim from `wle_templates`.

- [ ] **Step 4: Run to verify green.**
- [ ] **Step 5: Commit** — `feat(forms): .txt-directive detection + companion resolution + orphan probe (tuxlink-z0le)`.

---

## TASK GROUP B — Path safety + staging + classification

### Task 3: Path-component validation + symlink rejection + archive caps

§11.5. These are pure functions, unit-tested in isolation before they're wired into staging.

**Files:** Modify `src-tauri/src/forms/import.rs`. Add caps near the top:

```rust
const MAX_IMPORT_ENTRIES: usize = 5_000;          // entry-count cap (zip + folder)
const MAX_SINGLE_FILE_BYTES: u64 = 16 * 1_048_576; // per-file cap
const MAX_TOTAL_BYTES: u64 = 300 * 1_048_576;      // aggregate uncompressed cap
const MAX_COMPRESSION_RATIO: u64 = 200;            // per-entry zip-bomb ratio guard
```

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn rejects_dotdot_and_absolute_and_reserved_components() {
    assert!(!is_safe_rel_path("a/../b"));
    assert!(!is_safe_rel_path("../b"));
    assert!(!is_safe_rel_path("/etc/passwd"));
    assert!(!is_safe_rel_path("a/./b"));      // single-dot component
    assert!(!is_safe_rel_path("a//b"));       // empty component
    assert!(!is_safe_rel_path(".hidden/b"));  // leading-dot component
    assert!(!is_safe_rel_path("a/\u{0}b"));   // NUL
    assert!(!is_safe_rel_path("a/b\tc"));      // control char
    assert!(!is_safe_rel_path("CON/x.html"));  // reserved Windows name
    assert!(!is_safe_rel_path("a/PRN"));
    // happy path:
    assert!(is_safe_rel_path("AAMRON/Net Check-in Initial.html"));
    assert!(is_safe_rel_path("Foo & Bar/Form.v1.html"));
}

#[test]
fn ratio_guard_flags_zip_bomb_entry() {
    // 1 KiB compressed declaring 1 GiB uncompressed → ratio 1e6 > 200.
    assert!(exceeds_ratio(compressed: 1024, uncompressed: 1024 * 1024 * 1024));
    assert!(!exceeds_ratio(compressed: 1_000_000, uncompressed: 5_000_000)); // 5x, fine
    assert!(!exceeds_ratio(compressed: 0, uncompressed: 0)); // empty entry, no div-by-zero
}
```

> Note the test uses named-arg comment style for readability; the actual fn is `exceeds_ratio(compressed: u64, uncompressed: u64) -> bool`.

- [ ] **Step 2: Run → FAIL.**
- [ ] **Step 3: Implement**

```rust
const RESERVED_WIN: &[&str] = &["con","prn","aux","nul",
    "com1","com2","com3","com4","com5","com6","com7","com8","com9",
    "lpt1","lpt2","lpt3","lpt4","lpt5","lpt6","lpt7","lpt8","lpt9"];

pub(crate) fn is_safe_rel_path(rel: &str) -> bool {
    if rel.is_empty() || rel.starts_with('/') || rel.starts_with('\\') { return false; }
    let comps: Vec<&str> = rel.split(['/', '\\']).collect();
    for c in comps {
        if c.is_empty() || c == "." || c == ".." { return false; }
        if c.starts_with('.') { return false; }              // leading-dot component
        if c.bytes().any(|b| b < 0x20 || b == 0x7f) { return false; } // control/NUL/DEL
        let stem_lower = c.split('.').next().unwrap_or("").to_ascii_lowercase();
        if RESERVED_WIN.contains(&stem_lower.as_str()) { return false; }
    }
    true
}

pub(crate) fn exceeds_ratio(compressed: u64, uncompressed: u64) -> bool {
    if compressed == 0 { return false; } // stored/empty entries can't be bombs by ratio
    uncompressed / compressed > MAX_COMPRESSION_RATIO
}
```

> **Belt-and-suspenders:** the stem that becomes a `form_id` must ALSO pass `crate::forms::validation::is_valid_form_id` (per §5). `is_safe_rel_path` validates the whole relative path strictly (write path); `is_valid_form_id` validates the catalog stem. Both are required — neither replaces the other (§11.5: `is_valid_form_id`'s space/dot/`&` relaxation is a receive-path concession; import additionally enforces strict path-component rules).

- [ ] **Step 4: Run → green.**
- [ ] **Step 5: Commit** — `feat(forms): strict path-safety + zip-bomb ratio guard for import (tuxlink-z0le)`.

---

### Task 4: Stage sources into a `0700` temp dir (file / folder / zip), with symlink rejection

**Files:** Modify `src-tauri/src/forms/import.rs`.

- [ ] **Step 1: Failing tests**

```rust
#[test]
fn stage_folder_copies_tree_into_0700_staging() {
    let td = tempfile::tempdir().unwrap();
    let src = td.path().join("org");
    write_bytes(&src.join("A/Form.txt"), b"Form: Form Initial.html\r\n");
    write_bytes(&src.join("A/Form Initial.html"),
        b"<form method=post enctype=multipart/form-data action=\"http://{FormServer}:{FormPort}\">x</form>");
    let staged = stage_sources(&[src.to_string_lossy().into()]).unwrap();
    assert!(staged.dir.join("A/Form Initial.html").exists());
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&staged.dir).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700, "staging dir must be owner-only");
    }
}

#[test]
#[cfg(unix)]
fn stage_rejects_symlink_in_folder_source() {
    let td = tempfile::tempdir().unwrap();
    let src = td.path().join("org");
    std::fs::create_dir_all(&src).unwrap();
    std::os::unix::fs::symlink("/etc/passwd", src.join("evil.html")).unwrap();
    let err = stage_sources(&[src.to_string_lossy().into()]).unwrap_err();
    matches!(err, ImportError::StagingFailed { .. });
}

#[test]
fn stage_zip_rejects_traversal_and_unwraps_standard_forms() {
    // build a zip with a leading Standard_Forms/ wrapper; assert unwrap.
    // build a second zip with an entry "../escape.html"; assert StagingFailed.
    // (use the `zip` crate's ZipWriter in-test.)
}

#[test]
fn stage_enforces_entry_count_and_total_byte_caps() {
    // a folder/zip with > MAX_IMPORT_ENTRIES files → StagingFailed.
    // a single file > MAX_SINGLE_FILE_BYTES → StagingFailed.
}
```

- [ ] **Step 2: Run → FAIL.**
- [ ] **Step 3: Implement `stage_sources`.** Contract:
  - Create staging via `tempfile::Builder::new().prefix("tuxlink-formimport-").tempdir()` and immediately `chmod 0700` on Unix (`std::os::unix::fs::PermissionsExt`). Keep the `TempDir` handle in the returned `Staged` so its lifetime is owned by the registry (Task 6), NOT auto-dropped here.
  - **File source:** copy the single `.html`/`.htm` (and, if a sibling `.txt`/viewer with a matching stem exists in the SAME source dir, pull those too).
  - **Folder source:** walk with `walkdir`. For EVERY entry, `symlink_metadata` first — reject symlinks (`StagingFailed`). Reject non-regular files. Validate each relative path with `is_safe_rel_path`. Enforce `MAX_IMPORT_ENTRIES`, `MAX_SINGLE_FILE_BYTES`, `MAX_TOTAL_BYTES` (stat-first, before reading).
  - **Zip source:** mirror `updater::extract_zip`'s hardening (`enclosed_name()` + canonical-prefix check) AND add the new caps: reject `archive.len() > MAX_IMPORT_ENTRIES`; for each entry check `exceeds_ratio(entry.compressed_size(), entry.size())` and the per-file + total byte caps; reject the entry's path via `is_safe_rel_path`. Unwrap a leading `Standard_Forms/` component (reuse `needs_wrap` logic — if every entry starts with `Standard_Forms/`, strip it).
  - Drop metadata files (`is_metadata_file`) during staging — don't copy them.

```rust
pub(crate) struct Staged {
    pub dir: std::path::PathBuf, // canonicalized staging root
    _guard: tempfile::TempDir,   // owns the dir; drop = rm -rf
}
pub(crate) fn stage_sources(sources: &[String]) -> Result<Staged, ImportError> { /* ... */ }
```

> **Pitfall guard (zip-slip):** copy `extract_zip`'s `canonical_dest` prefix check verbatim — `enclosed_name()` alone is necessary-not-sufficient. Re-read `src-tauri/src/forms/updater.rs:460-575` for the exact pattern.

- [ ] **Step 4: green. Step 5: Commit** — `feat(forms): hardened staging (0700, symlink/cap/traversal guards) (tuxlink-z0le)`.

---

### Task 5: Classify staged candidates against live custom + bundled catalog (folder-aware collisions)

§11.2 + §6. This turns `Candidate`s into `ImportEntry`s with the right `kind`.

**Files:** Modify `src-tauri/src/forms/import.rs`.

- [ ] **Step 1: Failing tests**

```rust
#[test]
fn classifies_added_update_override_skip() {
    // existing_custom_ids = {"Net Check-in Initial"}, bundled_ids = {"ICS213_Initial"}
    let existing_custom: HashSet<String> = ["Net Check-in Initial".to_string()].into();
    let bundled: HashSet<String> = ["ICS213_Initial".to_string()].into();
    let cands = vec![
        authoring("Brand New Initial", "AAMRON"),
        authoring("Net Check-in Initial", "AAMRON"),  // collides custom → Update
        authoring("ICS213_Initial", "ICS Forms"),     // collides bundled → OverridesStandard
        authoring("Dup Initial", "FolderX"),
        authoring("Dup Initial", "FolderY"),           // same stem twice in batch → 2nd Skip
    ];
    let entries = classify(cands, &existing_custom, &bundled);
    assert_eq!(kind_of(&entries, "Brand New Initial"), ImportKind::Added);
    assert_eq!(kind_of(&entries, "Net Check-in Initial"), ImportKind::Update);
    assert_eq!(kind_of(&entries, "ICS213_Initial"), ImportKind::OverridesStandard);
    // first Dup wins, second is Skip with a folder-naming reason
    let dups: Vec<_> = entries.iter().filter(|e| e.id == "Dup Initial").collect();
    assert_eq!(dups.iter().filter(|e| e.kind == ImportKind::Skip).count(), 1);
    assert!(dups.iter().find(|e| e.kind == ImportKind::Skip)
        .unwrap().reason.as_deref().unwrap().contains("duplicate stem"));
}

#[test]
fn override_standard_carries_amber_warning_reason() {
    let entries = classify(vec![authoring("ICS213_Initial","ICS Forms")],
        &HashSet::new(), &["ICS213_Initial".to_string()].into());
    let e = &entries[0];
    assert_eq!(e.kind, ImportKind::OverridesStandard);
    assert!(e.reason.as_deref().unwrap().to_lowercase().contains("replaces the standard"));
}

#[test]
fn companions_pass_through_as_companion_kind() {
    let entries = classify(vec![companion("Net Check-in Viewer", "AAMRON")],
        &HashSet::new(), &HashSet::new());
    assert_eq!(entries[0].kind, ImportKind::Companion);
}
```

- [ ] **Step 2: FAIL. Step 3: Implement `classify`.**
  - Iterate candidates. `Reject`/`Companion` pass straight through to the matching `ImportKind`.
  - For `Authoring`: track a `seen: HashSet<String>` of stems already classified IN THIS BATCH. If `seen.contains(id)` → `Skip{reason:"duplicate stem in <folder> (already importing <other folder>)"}`. Else insert, then: if `existing_custom.contains(id)` → `Update`; else if `bundled.contains(id)` → `OverridesStandard{reason:"Replaces the standard <id>"}`; else `Added`.
  - `has_viewer` flows from the candidate.
  - Tally `ImportSummary`.

> **Scope guard:** do NOT change catalog identity to `(folder, id)` here. Cross-folder stem dupes are *reported* (Skip), not *resolved*. The engine fix is `tuxlink-8v3l`, explicitly OUT of this PR (§11.2, §11.6).

- [ ] **Step 4: green. Step 5: Commit** — `feat(forms): folder-aware import classification (tuxlink-z0le)`.

---

## TASK GROUP C — Commands: preview, commit, cancel, registry

### Task 6: `ImportStagingRegistry` (token → staging) + `forms_import_preview` command + cancel + reaper

§11.4. The registry owns staged dirs by opaque token; commit is single-shot.

**Files:** Modify `src-tauri/src/forms/import.rs` (registry + free fns), `src-tauri/src/ui_commands.rs` (Tauri command wrappers).

- [ ] **Step 1: Failing tests** (registry is unit-testable without Tauri)

```rust
#[test]
fn registry_mint_resolve_consume_is_single_shot() {
    let reg = ImportStagingRegistry::default();
    let staged = make_staged_fixture();
    let token = reg.insert(staged);
    assert_eq!(token.len(), 16);
    assert!(reg.take(&token).is_some(), "first take resolves");
    assert!(reg.take(&token).is_none(), "second take → gone (single-shot)");
}

#[test]
fn registry_cancel_drops_staging_dir() {
    let reg = ImportStagingRegistry::default();
    let staged = make_staged_fixture();
    let path = staged.dir.clone();
    let token = reg.insert(staged);
    reg.cancel(&token);
    assert!(reg.take(&token).is_none());
    assert!(!path.exists(), "cancel rm -rf'd the staging dir");
}

#[test]
fn registry_reaps_entries_older_than_ttl() {
    let reg = ImportStagingRegistry::default();
    let token = reg.insert_with_age(make_staged_fixture(), /*secs_ago*/ 7200);
    reg.reap(/*ttl_secs*/ 3600);
    assert!(reg.take(&token).is_none(), "stale staging reaped");
}
```

- [ ] **Step 2: FAIL. Step 3: Implement registry.**

```rust
pub struct ImportStagingRegistry {
    inner: std::sync::Mutex<std::collections::HashMap<String, StagedEntry>>,
}
struct StagedEntry { staged: Staged, created_at: std::time::SystemTime }

impl Default for ImportStagingRegistry { /* empty map */ }
impl ImportStagingRegistry {
    pub fn insert(&self, staged: Staged) -> String { /* mint 16-hex, retry on collision */ }
    pub fn take(&self, token: &str) -> Option<Staged> { /* remove + return (single-shot) */ }
    pub fn cancel(&self, token: &str) { /* remove → Drop rm -rf's it */ }
    pub fn reap(&self, ttl_secs: u64) { /* drop entries older than ttl */ }
}
fn mint_import_token() -> String { /* identical 16-hex shape to http_server::mint_session_token */ }
```

> Reuse the **exact** 16-hex token shape from `src-tauri/src/forms/http_server.rs:1065` (`mint_session_token`). Do not invent a different format. Tokens are process-lifetime only (die on restart → cross-session replay closed, §11.4).

- [ ] **Step 4 (preview command): Failing integration test** (over a temp fixture, calling the free fn `preview_sources` that the Tauri command wraps):

```rust
#[test]
fn preview_classifies_without_writing_to_custom_root() {
    let td = tempfile::tempdir().unwrap();
    let custom_root = td.path().join("custom"); // empty
    std::fs::create_dir_all(&custom_root).unwrap();
    let src = make_org_fixture(td.path()); // AAMRON/* with one authoring + viewer + .txt
    let reg = ImportStagingRegistry::default();
    let plan = preview_sources(&[src], &custom_root, &bundled_ids_fixture(), &reg).unwrap();
    assert!(plan.entries.iter().any(|e| e.kind == ImportKind::Added));
    assert_eq!(plan.staging_token.len(), 16);
    // NOTHING was written to custom_root during preview:
    assert_eq!(std::fs::read_dir(&custom_root).unwrap().count(), 0);
}
```

- [ ] **Step 5: Implement `preview_sources`** = `stage_sources` → `detect_candidates(staged.dir)` → resolve `existing_custom` ids (`wle_templates::list(custom_root only)` or a directory walk) + `bundled` ids → `classify` → `reg.insert(staged)` → assemble `ImportPlan`. The Tauri command:

```rust
// src-tauri/src/ui_commands.rs
#[tauri::command]
pub async fn forms_import_preview(
    sources: Vec<String>,
    app: tauri::AppHandle,
    reg: tauri::State<'_, std::sync::Arc<crate::forms::import::ImportStagingRegistry>>,
) -> Result<crate::forms::import::ImportPlan, crate::forms::import::ImportError> {
    let custom_root = crate::forms::wle_templates::custom_root_for_app(&app);
    let bundle_root = crate::forms::wle_templates::bundle_root_for_app(&app)
        .map_err(|e| crate::forms::import::ImportError::Io { reason: e.to_string() })?;
    let reg = reg.inner().clone();
    tokio::task::spawn_blocking(move || {
        crate::forms::import::preview_sources(&sources, &custom_root, &bundle_root, &reg)
    })
    .await
    .map_err(|e| crate::forms::import::ImportError::Io { reason: format!("join: {e}") })?
}
```

> `preview_sources` is the single canonical free fn (same name the Task-6 tests call). Run it under `spawn_blocking` (synchronous `std::fs`). It resolves the bundled-id set from `bundle_root` and the existing-custom-id set by walking `custom_root` (a direct stem-walk — no need for the full `Template` machinery). Surface staging failures as `ImportError::StagingFailed`.

- [ ] **Step 6: cancel command** — `forms_import_cancel(token, reg)` → `reg.cancel(&token)`. Always returns `Ok(())` (idempotent; cancelling an unknown token is a no-op).

- [ ] **Step 7: Commit** — `feat(forms): staging registry + import preview/cancel commands (tuxlink-z0le)`.

---

### Task 7: `forms_import_commit` — single-shot, re-classify under shared lock, atomic promote + `.prev` backup

§11.4 — the data-safety heart. Mirrors `updater::install`'s rename/backup/rollback.

**Files:** Modify `src-tauri/src/forms/import.rs`, `src-tauri/src/forms/updater.rs` (promote `INSTALL_LOCK` to `pub(crate)`), `src-tauri/src/ui_commands.rs`.

- [ ] **Step 1: Failing tests**

Split into a **sync pure core** (`commit_core` — write/backup/rollback, easily tested) and an **async wrapper** (`commit` — token consume + shared lock).

```rust
// commit_core tests (sync, over temp dirs — no token, no lock):
#[test]
fn commit_core_writes_only_approved_overwrites() {
    // staged has: Added "New Initial", Update "Existing Initial" (already in custom_root).
    // approved=[]                  → New written, Existing untouched, skipped_updates=["Existing Initial"].
    // approved=["Existing Initial"] → Existing replaced, a .prev-<ts> of the old file remains.
}
#[test]
fn commit_core_backs_up_overwritten_custom_form_to_prev() {
    // an applied Update leaves a .prev-<ts> copy of the prior file (data-loss guard).
}
#[test]
fn commit_core_aborts_with_conflict_if_added_became_update() {
    // classify in staged says X=Added; pre-seed X into custom_root; commit_core re-classifies
    // under its own read → CommitConflict (classification TOCTOU).
}
#[test]
fn commit_core_ignores_approved_ids_not_in_plan() {
    // approved=["Not In Plan"] must NOT force-write anything.
}

// commit wrapper tests (async, token lifecycle):
#[tokio::test]
async fn commit_consumes_token_recommit_is_token_expired() {
    let (reg, token, custom_root, bundled) = setup_committable();
    assert!(commit(&token, &[], &custom_root, &bundled, reg.clone()).await.is_ok());
    let err = commit(&token, &[], &custom_root, &bundled, reg).await.unwrap_err();
    assert!(matches!(err, ImportError::TokenExpired));
}
```

- [ ] **Step 2: FAIL. Step 3: Implement.**

`commit_core(staged: &Staged, approved_overwrite_ids: &[String], custom_root: &Path, bundle_root: &Path) -> Result<ImportResult, ImportError>` (sync):
  1. Re-detect + re-classify the staged tree against the now-live `custom_root` + the bundled-id set resolved from `bundle_root` (reuse `detect_candidates` + `classify`; resolve bundled ids by a stem-walk of `bundle_root`, same as `preview_sources`). If any `Added` is now an `Update`, or an approved `Update` no longer matches an existing custom form, abort `CommitConflict{reason:"catalog changed, re-preview"}` (classification TOCTOU).
  2. Write set: all `Added` + all `OverridesStandard` + `Update`s whose id ∈ `approved_overwrite_ids ∩ {plan Update ids}` + all `Companion`s. Never write `Skip`/`Reject`. (Binds overwrites to the plan: approved ids not in the plan are ignored — §11.4.)
  3. Each dest = `custom_root.join(rel_path)`. **`symlink_metadata`-check each existing dest path component** (defend a pre-seeded symlink in the operator's own custom dir; §11.5). Create parent dirs.
  4. **Overwrite safety:** if the dest exists, `rename` it to `<dest>.prev-<ts>` before writing the new file (mirror `updater::install`'s `.prev` backup). On any write error mid-batch, roll back the renames + new files done so far.
  5. Return `ImportResult { installed, skipped_updates, entries }`.

`commit(token, approved, custom_root, bundle_root, reg) -> Result<ImportResult, ImportError>` (async; `bundle_root: &Path`):
  1. `let staged = reg.take(token).ok_or(ImportError::TokenExpired)?;` — single-shot consume (re-commit → `TokenExpired`).
  2. `let _g = crate::forms::updater::INSTALL_LOCK.lock().await;` — share the forms-data-dir mutex so import and a concurrent standard-forms refresh can't race. The promote is brief; hold the guard across an inline `commit_core(&staged, …)` call (no `spawn_blocking` here — keep the guard on this task).
  3. Return `commit_core`'s result. `staged` drops at end → staging dir `rm -rf`'d.

```rust
// updater.rs — promote:
pub(crate) static INSTALL_LOCK: Lazy<tokio::sync::Mutex<()>> = /* unchanged init */;
```

Tauri command:

```rust
#[tauri::command]
pub async fn forms_import_commit(
    staging_token: String,
    approved_overwrite_ids: Vec<String>,
    app: tauri::AppHandle,
    reg: tauri::State<'_, std::sync::Arc<crate::forms::import::ImportStagingRegistry>>,
) -> Result<crate::forms::import::ImportResult, crate::forms::import::ImportError> {
    let custom_root = crate::forms::wle_templates::custom_root_for_app(&app);
    let bundle_root = crate::forms::wle_templates::bundle_root_for_app(&app)
        .map_err(|e| crate::forms::import::ImportError::Io { reason: e.to_string() })?;
    crate::forms::import::commit(&staging_token, &approved_overwrite_ids,
        &custom_root, &bundle_root, reg.inner().clone()).await
}
```

- [ ] **Step 4: green. Step 5: Commit** — `feat(forms): single-shot import commit with re-classify + .prev backup (tuxlink-z0le)`.

---

### Task 8: `open_forms_folder` command (xdg-open via shell) + boot-time staging sweep

§11.4.

**Files:** Modify `src-tauri/src/ui_commands.rs`, `src-tauri/src/lib.rs`.

- [ ] **Step 1: Failing test** (the dir-resolution + create-if-absent logic is unit-testable; the actual `xdg-open` spawn is not — keep that thin and untested, test the resolver):

```rust
#[test]
fn forms_folder_path_creates_dir_when_absent() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("tuxlink/forms/custom");
    assert!(!target.exists());
    ensure_custom_dir(&target).unwrap();
    assert!(target.is_dir());
}
```

- [ ] **Step 2: FAIL. Step 3: Implement.**
  - `open_forms_folder(app)` resolves `custom_root` strictly via `app.path().data_dir()` (NOT `custom_root_for_app`'s relative fallback — §11.4 says refuse if `data_dir()` is unavailable). `ensure_custom_dir` creates it. Then spawn `xdg-open <path>` via `tauri-plugin-shell` (`app.shell().command("xdg-open").args([path]).spawn()`). If the spawn fails (no file-manager handler under labwc/Wayland), return a typed error string the frontend surfaces as a toast (`"No file manager is registered to open folders."`). Return `Result<(), String>`.
  - **Boot sweep** in `lib.rs` `setup`: after `.manage(ImportStagingRegistry)`, spawn a one-shot that removes stale `tuxlink-formimport-*` dirs left in the OS temp root by a previous crashed run (older than the TTL). Keep it best-effort (log + ignore errors).

- [ ] **Step 4: green. Step 5: Commit** — `feat(forms): open-custom-folder command + boot staging sweep (tuxlink-z0le)`.

---

### Task 9: `forms_custom_delete(ids)` uninstall command

§11.3.

**Files:** Modify `src-tauri/src/forms/import.rs` (or a small `forms::custom_ops` — keep it in `import.rs` for cohesion), `src-tauri/src/ui_commands.rs`.

- [ ] **Step 1: Failing tests**

```rust
#[test]
fn delete_removes_custom_form_and_companions() {
    // custom_root has "Foo Initial.html" + "Foo Viewer.html" + "Foo.txt".
    // delete(["Foo Initial"]) removes all three (the authoring form + resolved companions).
}
#[test]
fn delete_only_touches_custom_root_never_bundle() {
    // a bundled id passed to delete is a no-op (not found in custom_root) — returns ok, removes nothing.
}
#[test]
fn delete_rejects_ids_that_escape_custom_root() {
    // an id that fails is_valid_form_id (e.g. contains "/") is rejected, nothing deleted.
}
```

- [ ] **Step 2: FAIL. Step 3: Implement `delete_custom_forms(ids, custom_root)`.**
  - For each id: validate with `is_valid_form_id` (reject bad). Find the matching authoring `.html`/`.htm` under `custom_root` by stem. Resolve + remove its companions (the `.txt` of the same stem, and `resolve_viewer_for` result). `symlink_metadata`-check before removing (don't follow a symlink out of `custom_root`). Only ever `remove_file` under the canonical `custom_root`.
  - Return the list of removed ids.

```rust
#[tauri::command]
pub async fn forms_custom_delete(ids: Vec<String>, app: tauri::AppHandle)
    -> Result<Vec<String>, String> { /* resolve custom_root, delegate */ }
```

- [ ] **Step 4: green. Step 5: Commit** — `feat(forms): custom-form uninstall command (tuxlink-z0le)`.

---

### Task 10: Close the `/folder/*` CSP exfil hole

§11.5. Independent of the import flow but **in scope before untrusted forms can be imported.**

**Files:** Modify `src-tauri/src/forms/http_server.rs` (`folder_handler`, ~860).

- [ ] **Step 1: Failing tests** (axum `oneshot`, mirroring the existing `/folder/*` tests at ~1278):

```rust
#[tokio::test]
async fn folder_handler_refuses_html_htm_svg() {
    // request /folder/page.html, /folder/page.htm, /folder/icon.svg
    // each → 403 (scriptable sinks are not servable as form-adjacent assets).
}
#[tokio::test]
async fn folder_handler_sets_csp_and_nosniff_on_served_assets() {
    // request /folder/style.css → 200 with Content-Security-Policy: FORM_CSP
    // and X-Content-Type-Options: nosniff.
}
```

- [ ] **Step 2: FAIL. Step 3: Implement.** In `folder_handler`, after computing `ext`:
  - If `ext` ∈ {`html`, `htm`, `svg`} → `(StatusCode::FORBIDDEN, "asset type not allowed").into_response()`.
  - On the success path, add to the response headers: `header::CONTENT_SECURITY_POLICY = FORM_CSP` and `X-Content-Type-Options: nosniff`.

```rust
let ext = canonical.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
if matches!(ext.as_str(), "html" | "htm" | "svg") {
    return (StatusCode::FORBIDDEN, "asset type not allowed").into_response();
}
// ... existing read + content-type ...
headers.insert(header::CONTENT_SECURITY_POLICY, FORM_CSP.parse().unwrap());
headers.insert("X-Content-Type-Options", "nosniff".parse().unwrap());
```

> Note: the existing `"html" | "htm" => "text/html..."` arm in the content-type match becomes dead for `/folder/*` (we 403 first) — leave the arm; it's harmless and the root `/` route still serves HTML via `html_with_csp`.

- [ ] **Step 4: green. Step 5: Commit** — `fix(forms): close /folder CSP exfil hole (refuse html/svg, add CSP+nosniff) (tuxlink-z0le)`.

---

### Task 11: Make enumeration accept `.htm` + register state/commands in `lib.rs`

§11.1 (import == enumeration) + wiring.

**Files:** Modify `src-tauri/src/forms/wle_templates.rs`, `src-tauri/src/lib.rs`.

- [ ] **Step 1: Failing test** (wle_templates):

```rust
#[test]
fn walk_html_accepts_htm_and_uppercase_extension() {
    let td = tempfile::TempDir::new().unwrap();
    let root = td.path().join("Standard_Forms/Cat");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("A Initial.HTM"), "<html>x</html>").unwrap();
    std::fs::write(root.join("B Initial.html"), "<html>x</html>").unwrap();
    let got = walk_html(&td.path().join("Standard_Forms"), TemplateSource::Custom);
    let ids: std::collections::HashSet<_> = got.iter().map(|t| t.id.as_str()).collect();
    assert!(ids.contains("A Initial"), ".HTM (uppercase) must enumerate");
    assert!(ids.contains("B Initial"));
}
```

- [ ] **Step 2: FAIL** (current filter is `x == "html"`, lowercase-exact). **Step 3:** change the `walk_html` extension filter to:

```rust
.filter(|e| e.path().extension()
    .and_then(|x| x.to_str())
    .map(|x| x.eq_ignore_ascii_case("html") || x.eq_ignore_ascii_case("htm"))
    .unwrap_or(false))
```

- [ ] **Step 4: green** (run the full `wle_templates` test module — confirm the existing count tests still pass; if a count assertion shifts because a `.htm` now enumerates, that's a real fixture change — update the expected count and note it).

- [ ] **Step 5: Wire `lib.rs`:**
  - `.manage(std::sync::Arc::new(crate::forms::import::ImportStagingRegistry::default()))` alongside the other `.manage(...)` calls (~line 176).
  - Add to `invoke_handler![...]` near the other `forms_*` commands (~579): `crate::ui_commands::forms_import_preview, crate::ui_commands::forms_import_commit, crate::ui_commands::forms_import_cancel, crate::ui_commands::open_forms_folder, crate::ui_commands::forms_custom_delete`.
  - In `setup`, schedule the boot sweep (Task 8).

- [ ] **Step 6: Commit** — `feat(forms): enumerate .htm + register import commands/state (tuxlink-z0le)`.

---

## TASK GROUP D — Frontend

### Task 12: `importApi.ts` — typed bindings + TS mirror types

**Files:** Create `src/compose/importApi.ts`.

- [ ] **Step 1: Failing test** — `src/compose/importApi.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { importPreview, importCommit, importCancel } from './importApi';

describe('importApi', () => {
  beforeEach(() => vi.clearAllMocks());
  it('importPreview passes sources and returns the plan', async () => {
    (invoke as any).mockResolvedValue({ stagingToken: 'abc', entries: [], summary: {} });
    const plan = await importPreview(['/x/org.zip']);
    expect(invoke).toHaveBeenCalledWith('forms_import_preview', { sources: ['/x/org.zip'] });
    expect(plan.stagingToken).toBe('abc');
  });
  it('importCommit passes token + approved ids', async () => {
    (invoke as any).mockResolvedValue({ installed: [], skippedUpdates: [], entries: [] });
    await importCommit('abc', ['Foo Initial']);
    expect(invoke).toHaveBeenCalledWith('forms_import_commit',
      { stagingToken: 'abc', approvedOverwriteIds: ['Foo Initial'] });
  });
  it('importCancel never throws on a bad token', async () => {
    (invoke as any).mockResolvedValue(undefined);
    await expect(importCancel('abc')).resolves.toBeUndefined();
  });
});
```

- [ ] **Step 2: FAIL. Step 3: Implement** — mirror the Rust serde shapes exactly (camelCase). `ImportKind = 'added' | 'update' | 'overridesStandard' | 'companion' | 'skip' | 'reject'`. Export `importPreview(sources)`, `importCommit(token, approvedIds)`, `importCancel(token)`, `openFormsFolder()`, `formsCustomDelete(ids)`.

- [ ] **Step 4: green. Step 5: Commit** — `feat(forms): import IPC bindings + TS types (tuxlink-z0le)`.

---

### Task 13: `ImportSheet.tsx` — source picker → report → confirm → commit

**Files:** Create `src/compose/ImportSheet.tsx`, `src/compose/ImportSheet.css`. Modify `src-tauri/capabilities/default.json`.

- [ ] **Step 0: Grant the picker capability (REQUIRED — verified missing).** `default.json` grants `dialog:allow-save` but NOT `dialog:allow-open`; `@tauri-apps/plugin-dialog`'s `open()` (file/folder/zip picker) will fail at runtime without it. Add `"dialog:allow-open"` to the `permissions` array in `src-tauri/capabilities/default.json`. (Do NOT touch the URL-scoped `shell:allow-open` block — it is correctly scoped to winlink/github URLs; the folder-reveal goes through the backend `open_forms_folder` command spawning `xdg-open`, which is why a frontend `shell.open(localPath)` would be rejected by that scope.) This is a security-relevant grant — note it in the PR body.

- [ ] **Step 1: Failing tests** — `src/compose/ImportSheet.test.tsx` (mock `importApi` + `@tauri-apps/plugin-dialog`):

```ts
// key behaviors to assert:
// 1. Renders three source choices; empty-state leads with "Choose ZIP…" (it is first / primary).
// 2. After a preview resolves, renders one row per entry with its kind label and reason.
// 3. Update rows render a checkbox DEFAULT UNCHECKED; Added/OverridesStandard rows have no checkbox.
// 4. Commit is disabled until the user acknowledges (when Update rows exist, the confirm step gates it).
// 5. Clicking Import (commit) calls importCommit with ONLY the checked Update ids.
// 6. OverridesStandard rows show the amber "Replaces the standard …" warning.
// 7. "no viewer" authoring rows show the reworded actionable note.
// 8. Unmount fires importCancel(token).
```

Write these as concrete RTL tests (`render`, `screen.getByRole`, `fireEvent.click`, `await waitFor`). Example for #5:

```ts
it('commits only the checked overwrites', async () => {
  (importPreview as any).mockResolvedValue({
    stagingToken: 'tok',
    entries: [
      { relPath: 'A/New Initial.html', id: 'New Initial', folder: 'A', kind: 'added', reason: null, hasViewer: true },
      { relPath: 'A/Old Initial.html', id: 'Old Initial', folder: 'A', kind: 'update', reason: null, hasViewer: true },
    ],
    summary: { added: 1, updated: 1, overridesStandard: 0, skipped: 0, rejected: 0, companions: 0 },
  });
  (importCommit as any).mockResolvedValue({ installed: ['New Initial'], skippedUpdates: ['Old Initial'], entries: [] });
  render(<ImportSheet onDone={vi.fn()} onCancel={vi.fn()} />);
  fireEvent.click(screen.getByTestId('import-choose-zip'));
  await screen.findByText('Old Initial');
  // leave the Update checkbox UNCHECKED → commit with []
  fireEvent.click(screen.getByTestId('import-commit'));
  await waitFor(() => expect(importCommit).toHaveBeenCalledWith('tok', []));
});
```

- [ ] **Step 2: FAIL. Step 3: Implement `ImportSheet`.** State machine: `idle → previewing → report → committing → result`. Use `@tauri-apps/plugin-dialog`'s `open({ multiple:false, directory:false })` for file/zip and `open({ directory:true })` for folder. Render the report grouped by kind; amber styling for `overridesStandard` + `no viewer`; per-`update` checkbox (controlled, default unchecked). `onDone(result)` after commit; `onCancel()` + `importCancel(token)` on cancel/unmount (use a `useEffect` cleanup that fires `importCancel` if a token exists and commit hasn't consumed it).

> **WebKitGTK / jsdom caveat (memory `chromium_not_webkitgtk_proxy`):** jsdom can't see CSS. The amber-warning + custom-badge styling is NOT verifiable in vitest — assert on `data-kind`/`aria` attributes and text, not computed color. Visual correctness is an opportunistic post-merge grim/WebKitGTK smoke, not a gate.

- [ ] **Step 4: green. Step 5: Commit** — `feat(forms): ImportSheet preview/confirm/commit UI (tuxlink-z0le)`.

---

### Task 14: Wire `CatalogBrowser` — entry points, custom-first sort, Remove, Escape state machine, post-commit refresh

**Files:** Modify `src/compose/CatalogBrowser.tsx`.

- [ ] **Step 1: Failing tests** — extend `src/compose/CatalogBrowser.test.tsx`:

```ts
// 1. Header renders an "Import group forms…" control (distinct label from "Update standard forms…").
// 2. Empty-custom state renders the CTA ("bring in your group's forms…") leading with ZIP.
// 3. Footer renders "Open forms folder" → calls openFormsFolder().
// 4. buildFolderTree sorts custom categories FIRST (was last).
// 5. After ImportSheet onDone, fetchCatalog is re-invoked (refresh) and new ids are highlighted.
// 6. Each custom-form row has a Remove affordance → confirm → formsCustomDelete([id]) → refresh.
// 7. Escape unwinds: import-confirming → import-sheet-open → idle (close picker); committing ignores Escape.
```

Concrete example for #4 (pure-function test, no render):

```ts
it('sorts custom categories before bundled', () => {
  const buckets = buildFolderTree([
    { id: 'Z', label: 'Z', folder: 'ICS Forms', source: 'Bundled', path: '' } as any,
    { id: 'A', label: 'A', folder: 'AAMRON', source: 'Custom', path: '' } as any,
  ]);
  expect(buckets[0].name).toBe('AAMRON');       // custom first
});
```

- [ ] **Step 2: FAIL. Step 3: Implement.**
  - **`buildFolderTree`:** after building buckets, sort so custom buckets (any bucket whose members are `source === 'Custom'`, incl. the synthetic `CUSTOM_FOLDER_LABEL`) sort before bundled, then alphabetical within each group. (Add an `isCustom` flag to `FolderBucket`.)
  - **Header control:** an `Import group forms…` button next to the existing refresh control; opens the `ImportSheet` (a new `RefreshStep`-sibling state — extend the discriminated union with `{ kind: 'importing' }` and `{ kind: 'import-confirming' }`, OR a parallel `importStep` state; keep import and refresh **mutually exclusive** per §11.4).
  - **Empty-custom CTA:** when no custom forms exist, render the prominent CTA leading with ZIP.
  - **Footer:** `Open forms folder` ghost button → `openFormsFolder()` with a toast on the typed error.
  - **Remove:** per custom-form row, a Remove button → inline confirm → `formsCustomDelete([id])` → `fetchCatalog()`.
  - **Escape state machine (extend the existing handler at ~160):** precedence — if importing/committing: `committing` ignores Escape (like `refreshing`); `import-confirming` → back to sheet-open; `importing` (sheet open) → close sheet (fires `importCancel`); else existing refresh/idle behavior. Import and refresh are mutually exclusive (opening one disables the other's entry control).
  - **Post-commit:** on `ImportSheet onDone`, call `fetchCatalog()` and briefly highlight the `installed` ids (reuse any existing highlight mechanism, or add a transient `highlightedIds` set with a timeout-cleared CSS class).

> **Production-mount test (memory `test_production_mount_path_not_just_units`):** also add/confirm an AppShell-level test that opens the Catalog from the Tools menu and mounts the real provider tree, so the import entry points are exercised on the production path, not just in an isolated wrapper.

- [ ] **Step 4: green (run `pnpm -C . vitest run src/compose`). Step 5: Commit** — `feat(forms): wire import entry points + custom-first sort + uninstall into CatalogBrowser (tuxlink-z0le, tuxlink-fwob)`.

---

## TASK GROUP E — Discoverability + docs + gates

### Task 15: G11 in-app help entry + discoverability

**Files:** Confirm the project's in-app help surface first (`grep -rn "help" src/ --include=*.tsx -l` and check `docs/help/` or a Help menu). Create `docs/help/forms-import.md` (or the established help-bundle format) and surface a "Help: importing forms" link from the ImportSheet header.

- [ ] **Step 1: Identify the existing help mechanism.** If there is an in-app help/docs panel, add an entry there (test that the entry renders + is reachable). If help is markdown-bundled, add the file + a link.
- [ ] **Step 2: Write the help content** — declarative voice, no first person, no temporal hedging (memories `writing_voice_*`): what import does, the three source types (ZIP is how orgs distribute), the validate-before-write report meaning of each status, where forms land, how to remove them. Frame for the stuck AAMRON member.
- [ ] **Step 3: Wire + test the link/entry.**
- [ ] **Step 4: Commit** — `docs(forms): in-app help for form import (tuxlink-48uc)`.

> **Definition of done (spec §10):** a stuck onboarding member can open Forms → Import group forms… → pick their org's ZIP → read a clear report → confirm → see their org's forms in the catalog and open one, IN A REAL BUILD, with the help entry findable. The realbuild verification is an operator grim/WebKitGTK smoke post-merge (not a pre-merge gate, memory `browser_smoke_before_ship`).

---

### Task 16: Full gates + implementation-log entry

- [ ] **Step 1:** `cargo test --manifest-path src-tauri/Cargo.toml` (full backend) → green.
- [ ] **Step 2:** `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` → re-run until exit 0 (it hides later-target lints; memory `scoped_vitest_misses_contract_tests`).
- [ ] **Step 3:** `pnpm -C . vitest run` (full) → green. Do NOT `pkill -f vitest`.
- [ ] **Step 4:** `pnpm -C . typecheck` → clean.
- [ ] **Step 5:** Add the top entry to `dev/implementation-log.md` (date + topic, what shipped, gates, the Codex-deferred note).
- [ ] **Step 6:** Commit — `chore(forms): gates green + implementation-log for import (tuxlink-z0le)`.

---

## Cross-task dependency / ordering

```
A1 → A2 → B3 → B4 → B5 → C6 → C7        (backend pipeline; strictly sequential — each builds on prior types)
C8, C9, C10, C11   depend on C7's types but are independent of each other (can interleave)
D12 → D13 → D14    (frontend; D14 depends on D12+D13 + C11's registered commands)
E15, E16           last (E16 is the whole-suite gate)
```

**Files touched by >1 task (must be sequenced, never parallel-edited):**
- `src-tauri/src/forms/import.rs` — Tasks 1–9 (all sequential).
- `src-tauri/src/ui_commands.rs` — Tasks 6, 7, 8, 9 (append-only; sequence them).
- `src-tauri/src/lib.rs` — Task 8 + Task 11 (sequence: 11 does the bulk registration).
- `src-tauri/src/forms/wle_templates.rs` — Task 11 only.
- `src/compose/CatalogBrowser.tsx` — Task 14 only.

---

## Out of scope (do NOT build here)

- Aggregation / common-operating-picture (G1–G4) — deferred behind `tuxlink-sobm`.
- Catalog identity `(folder, id)` engine change — separate issue `tuxlink-8v3l` (§11.6).
- A form authoring/editor.
- Drag-and-drop OS integration (stretch).
- Any tuxlink-added transmit safeguard (memory `no_tuxlink_added_safeguards`) — not applicable here; import never transmits.

## The merge gate (carry into the PR body)

This plan's PR is **NOT merge-ready** until the deferred cross-provider Codex adversarial round (`tuxlink-yqo4`) runs on the diff after Codex quota returns (Jun 13) and its findings are dispositioned. The 4 Claude rounds in spec §11 are not a substitute (memory `no_carveout_on_cross_provider_adrev`). Open the PR, mark it blocked-on-`yqo4`, do not merge until Codex clears.
