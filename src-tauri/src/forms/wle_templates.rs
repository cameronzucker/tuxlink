//! WLE Standard Forms + custom-form template enumeration.
//!
//! Walks the bundled snapshot tree (extracted from `resources/wle-forms/` at
//! build time per the P1 plan Task 0) and the operator's custom-forms
//! directory (default `~/.local/share/tuxlink/forms/custom/`) and returns a
//! flat catalog of every HTML template with id, folder, source kind, and
//! path.
//!
//! Custom forms with the same `id` as a bundled form WIN (the operator's
//! file shadows the bundled one).
//!
//! The catalog is consumed by:
//! - `forms::http_server` to look up the file when a webview requests
//!   `/forms/<token>/<id>` (P1 Task 6)
//! - React `CatalogBrowser` to render the picker tree (P1 Task 10) via the
//!   `forms_list_catalog` Tauri command
//!
//! Spec: docs/superpowers/specs/2026-05-31-html-forms-full-parity-design.md
//!       §7 (component table), §8.2 (data flow).
//! Plan: docs/superpowers/plans/2026-06-01-html-forms-p1-webview-infra.md
//!       Task 3.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

/// Where the template came from. Custom forms override bundled forms of
/// the same id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TemplateSource {
    Bundled,
    Custom,
}

/// One catalog entry — a single HTML form template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Template {
    /// Form id derived from the filename stem (e.g. `ICS213_Initial.html`
    /// → `ICS213_Initial`).
    pub id: String,
    /// Display label — for now, same as `id`. Spec §13 may revisit
    /// (human-readable labels are out of scope for P1).
    pub label: String,
    /// Folder path relative to the bundle / custom root. Used for
    /// `{FormFolder}` substitution in the WLE template.
    pub folder: String,
    pub source: TemplateSource,
    /// Absolute path on disk; the http_server uses this to load + serve.
    pub path: PathBuf,
}

/// Enumerate every HTML template visible to tuxlink: bundled snapshot + the
/// operator's custom-forms directory. Custom forms with the same `id` as a
/// bundled form override the bundled entry. Returns a deterministically-
/// sorted-by-id list.
pub fn list(
    bundle_root: &Path,
    custom_root: Option<&Path>,
) -> std::io::Result<Vec<Template>> {
    let bundled = walk_html(bundle_root, TemplateSource::Bundled);
    let custom = custom_root.map(|p| walk_html(p, TemplateSource::Custom)).unwrap_or_default();

    let mut by_id: std::collections::HashMap<String, Template> = bundled
        .into_iter()
        .map(|t| (t.id.clone(), t))
        .collect();
    for t in custom {
        by_id.insert(t.id.clone(), t);
    }

    let mut templates: Vec<Template> = by_id.into_values().collect();
    templates.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(templates)
}

fn walk_html(root: &Path, source: TemplateSource) -> Vec<Template> {
    if !root.exists() {
        return Vec::new();
    }
    WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().map(|x| x == "html").unwrap_or(false))
        .filter_map(|e| {
            let path = e.path().to_path_buf();
            let id = path.file_stem().and_then(|s| s.to_str())?.to_string();
            if id.is_empty() {
                return None;
            }
            let folder = path
                .strip_prefix(root)
                .ok()
                .and_then(|p| p.parent())
                .and_then(|p| p.to_str())
                .unwrap_or_default()
                .to_string();
            Some(Template {
                id: id.clone(),
                label: id,
                folder,
                source,
                path,
            })
        })
        .collect()
}

/// Resolve the bundled snapshot's on-disk root from the Tauri AppHandle.
/// Reads from the resource bundle at `resources/wle-forms/Standard_Forms/`.
///
/// In P3, this changes: the live snapshot moves to a writable data-dir and
/// the resource becomes the seed. Until P3 lands, this returns the
/// read-only resource path.
pub fn bundle_root_for_app(app: &tauri::AppHandle) -> Result<PathBuf, tauri::Error> {
    use tauri::Manager;
    app.path()
        .resolve("resources/wle-forms/Standard_Forms", tauri::path::BaseDirectory::Resource)
}

/// Resolve the operator's custom-forms directory.
///
/// Default: `<data_dir>/tuxlink/forms/custom/`. Operator-override is a P3
/// feature; until then, this is the canonical location.
pub fn custom_root_for_app(app: &tauri::AppHandle) -> PathBuf {
    use tauri::Manager;
    app.path()
        .data_dir()
        .map(|d| d.join("tuxlink/forms/custom"))
        .unwrap_or_else(|_| PathBuf::from("forms/custom"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn fake_bundle(td: &TempDir) -> PathBuf {
        let root = td.path().join("Standard_Forms");
        std::fs::create_dir_all(root.join("ICS Forms")).unwrap();
        std::fs::write(
            root.join("ICS Forms/ICS213_Initial.html"),
            "<html><!-- ICS-213 --></html>",
        )
        .unwrap();
        std::fs::write(
            root.join("ICS Forms/ICS213_Reply.html"),
            "<html><!-- ICS-213 reply --></html>",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("ARC Forms")).unwrap();
        std::fs::write(
            root.join("ARC Forms/ARC213.html"),
            "<html><!-- ARC213 --></html>",
        )
        .unwrap();
        root
    }

    fn fake_custom(td: &TempDir) -> PathBuf {
        let root = td.path().join("custom");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("MyCustom.html"), "<html><!-- custom --></html>").unwrap();
        root
    }

    #[test]
    fn list_includes_bundled_forms_with_folder_metadata() {
        let td = TempDir::new().unwrap();
        let bundle = fake_bundle(&td);
        let custom = fake_custom(&td);
        let cat = list(&bundle, Some(&custom)).unwrap();
        let by_id: std::collections::HashMap<_, _> =
            cat.iter().map(|t| (t.id.clone(), t)).collect();
        assert!(by_id.contains_key("ICS213_Initial"));
        assert!(by_id.contains_key("ICS213_Reply"));
        assert!(by_id.contains_key("ARC213"));
        assert!(by_id.contains_key("MyCustom"));
        let ics = by_id.get("ICS213_Initial").unwrap();
        assert_eq!(ics.source, TemplateSource::Bundled);
        assert_eq!(ics.folder, "ICS Forms");
        let custom_t = by_id.get("MyCustom").unwrap();
        assert_eq!(custom_t.source, TemplateSource::Custom);
        // Custom forms can live at the root of the custom dir → folder = "".
        assert_eq!(custom_t.folder, "");
    }

    #[test]
    fn list_returns_empty_when_paths_dont_exist() {
        let td = TempDir::new().unwrap();
        let cat = list(&td.path().join("nope"), None).unwrap();
        assert!(cat.is_empty());
    }

    #[test]
    fn list_skips_non_html_files() {
        let td = TempDir::new().unwrap();
        let root = td.path();
        std::fs::create_dir_all(root.join("ICS Forms")).unwrap();
        std::fs::write(root.join("ICS Forms/notes.txt"), "ignore me").unwrap();
        std::fs::write(root.join("ICS Forms/Real.html"), "<html></html>").unwrap();
        let cat = list(root, None).unwrap();
        assert_eq!(cat.len(), 1);
        assert_eq!(cat[0].id, "Real");
    }

    #[test]
    fn list_custom_overrides_bundled_by_id() {
        // If the operator drops a custom form with the same id as a bundled
        // one, the custom takes precedence (per design §6 P1 — custom forms
        // pickup-able). The catalog returns ONE entry per id.
        let td = TempDir::new().unwrap();
        let bundle = fake_bundle(&td);
        let custom = fake_custom(&td);
        std::fs::write(
            custom.join("ICS213_Initial.html"),
            "<html><!-- OPERATOR OVERRIDE --></html>",
        )
        .unwrap();
        let cat = list(&bundle, Some(&custom)).unwrap();
        let ics: Vec<_> = cat.iter().filter(|t| t.id == "ICS213_Initial").collect();
        assert_eq!(ics.len(), 1, "exactly one entry expected after override");
        assert_eq!(ics[0].source, TemplateSource::Custom);
    }

    #[test]
    fn list_sorts_by_id() {
        // Determinism is load-bearing for CatalogBrowser snapshot tests and
        // for any cross-process assertion that the catalog order is stable.
        let td = TempDir::new().unwrap();
        let bundle = fake_bundle(&td);
        let cat = list(&bundle, None).unwrap();
        let ids: Vec<_> = cat.iter().map(|t| t.id.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted, "catalog order must be sorted-by-id");
    }

    #[test]
    fn list_handles_nested_subfolders() {
        // WLE's Standard_Forms tree has only one level of subfolder per
        // category (e.g. `ICS Forms/ICS213_Initial.html`); but the design
        // allows deeper nesting in custom-forms dirs.
        let td = TempDir::new().unwrap();
        let root = td.path();
        std::fs::create_dir_all(root.join("Cat1/Sub1")).unwrap();
        std::fs::write(root.join("Cat1/Sub1/Nested.html"), "<html></html>").unwrap();
        let cat = list(root, None).unwrap();
        assert_eq!(cat.len(), 1);
        assert_eq!(cat[0].id, "Nested");
        assert_eq!(cat[0].folder, "Cat1/Sub1");
    }
}
