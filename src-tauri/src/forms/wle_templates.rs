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
            // 2026-06-04 Codex adrev P2.1: skip non-authoring templates.
            // The WLE catalog co-locates an Initial/authoring template,
            // a Viewer template, and a SendReply template per form in
            // the same folder. Only the authoring template should
            // appear in CatalogBrowser's "Compose form…" list. The
            // Viewer template is opened from the receive side
            // (open_webview_viewer) and the SendReply template is
            // generated on reply, not picked from the catalog.
            if !is_authoring_template_stem(&id) {
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

/// Determine if an HTML filename stem represents an authoring (compose-side)
/// template, as opposed to a Viewer (receive-side) or SendReply (reply-side)
/// template that the catalog walker would otherwise expose as a compose
/// option.
///
/// Case-insensitive substring match on "Viewer" and "SendReply" — the two
/// load-bearing categories in the bundled WLE catalog. This dropped the
/// observed bundle count from ~258 → ~137 (2026-06-04, current bundle).
///
/// Limitations: this is a heuristic. A legitimately-named operator-custom
/// form that happens to contain `Viewer` in its stem would be filtered.
/// The risk is low (operators don't typically name authoring templates
/// `Viewer`), and the alternative — letting Viewer pages render as
/// compose options — is the worse defect Codex flagged.
pub(crate) fn is_authoring_template_stem(stem: &str) -> bool {
    let lowered = stem.to_lowercase();
    !lowered.contains("viewer") && !lowered.contains("sendreply")
}

/// Resolve the viewer filename that pairs with the given authoring template
/// (2026-06-04 Codex adrev P1.3). The bundled WLE catalog is INCONSISTENT
/// about how authoring + viewer templates relate by filename — every
/// pattern below appears in the current bundle:
///
/// 1. Underscore swap: `ICS213_Initial.html` ↔ `ICS213_Viewer.html`,
///    `IARU_Message_Form_Initial.html` ↔ `IARU_Message_Form_Viewer.html`
/// 2. Space swap: `Bulletin Initial.html` ↔ `Bulletin Viewer.html`,
///    `Race Tracker Initial.html` ↔ `Race Tracker Viewer.html`
/// 3. Append " Viewer": `Hawaii Siren Report.html` ↔
///    `Hawaii Siren Report Viewer.html` (no `Initial` suffix at all on
///    the authoring template)
/// 4. Underscored Viewer with space-separated authoring:
///    `ARC Disaster Receipt 6409-B.html` ↔ `… Viewer.html` — these are
///    same as case 3 but they have `_` mixed in
/// 5. Mixed-case `Viewer` / `viewer` (WLE catalog is inconsistent —
///    `Field Situation Report Initial.html` ↔
///    `Field Situation Report viewer.html`)
///
/// Resolution strategy: walk the sibling files in the authoring
/// template's folder, find the one whose stem looks like the authoring
/// stem with "Initial" optionally replaced by "Viewer" or "Viewer"
/// appended. The check is case-insensitive on the comparison side; we
/// return the actual on-disk filename (preserving case) so the loopback
/// HTTP server / Path::join works on case-sensitive filesystems.
///
/// Returns the basename of the resolved viewer file (NOT the full path)
/// — the caller composes the full path via `form_parent.join(&name)`.
/// Returns `None` if no viewer match is found; the caller is expected to
/// surface `viewer template not found` (open_webview_viewer) or use
/// `<form_id>_Viewer.html` as a best-effort default (send_webview_form's
/// `display_form` field).
pub fn resolve_viewer_for(authoring_template: &Path) -> Option<String> {
    let folder = authoring_template.parent()?;
    let authoring_stem = authoring_template.file_stem()?.to_str()?;
    let authoring_lower = authoring_stem.to_lowercase();

    // Compute the candidate stems we expect to see, in priority order.
    // The first match against an actual on-disk file wins.
    let candidates = expected_viewer_stems(&authoring_lower);

    // Read the folder once; check each candidate against the lowered
    // sibling stems. Preserve the on-disk case in the return value.
    let entries: Vec<(String, String)> = std::fs::read_dir(folder)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter_map(|e| {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()).map(|x| x.eq_ignore_ascii_case("html")) != Some(true) {
                return None;
            }
            let stem = path.file_stem().and_then(|s| s.to_str())?.to_string();
            let name = path.file_name().and_then(|s| s.to_str())?.to_string();
            Some((stem.to_lowercase(), name))
        })
        .collect();

    for candidate in &candidates {
        if let Some((_, name)) = entries.iter().find(|(stem, _)| stem == candidate) {
            return Some(name.clone());
        }
    }

    // Last-resort fallback: any sibling .html whose stem contains
    // "viewer" (case-insensitive). This covers the case where the
    // authoring + viewer stems diverge in a way the explicit candidates
    // don't cover. If multiple match, prefer the one that starts with
    // the same prefix as the authoring stem (best-effort similarity).
    let viewer_candidates: Vec<&(String, String)> = entries
        .iter()
        .filter(|(stem, _)| stem.contains("viewer"))
        .collect();
    if viewer_candidates.len() == 1 {
        return Some(viewer_candidates[0].1.clone());
    }
    // Prefer the one that shares the longest common prefix with the
    // authoring stem (so e.g. authoring "ARC 213 Message Initial" picks
    // "ARC 213 Message Initial Viewer.html" over "ARC 213 Message SendReply
    // Viewer.html" — though SendReply Viewer would be filtered by
    // is_authoring_template_stem anyway).
    let best = viewer_candidates
        .iter()
        .max_by_key(|(stem, _)| common_prefix_len(&authoring_lower, stem))?;
    Some(best.1.clone())
}

/// Generate the ordered list of expected viewer stems (lowercased) for
/// the given authoring stem. The list reflects the WLE bundle
/// conventions documented on [`resolve_viewer_for`]; the first match
/// against an actual on-disk file wins.
fn expected_viewer_stems(authoring_lower: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    // Case 1 + 2: replace "Initial" suffix with "Viewer" (preserving
    // the separator that was in front of "initial" — space or underscore).
    if let Some(rest) = authoring_lower.strip_suffix("_initial") {
        out.push(format!("{rest}_viewer"));
    }
    if let Some(rest) = authoring_lower.strip_suffix(" initial") {
        out.push(format!("{rest} viewer"));
    }
    // Case 3: append " Viewer" to the full authoring stem (no Initial
    // suffix — e.g. Hawaii Siren Report → Hawaii Siren Report Viewer).
    out.push(format!("{authoring_lower} viewer"));
    // Case 4: append "_Viewer" — uncommon in the bundle but possible
    // in operator-custom forms.
    out.push(format!("{authoring_lower}_viewer"));
    out
}

/// Count the number of leading characters two strings share.
fn common_prefix_len(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
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

    /// 2026-06-04 Codex adrev P2.1: Viewer + SendReply templates must NOT
    /// appear in the authoring catalog. Operators clicking
    /// "Compose form → Bulletin Viewer" loaded a Viewer-only template
    /// in compose context, and submission went sideways.
    #[test]
    fn list_filters_viewer_and_sendreply_templates() {
        let td = TempDir::new().unwrap();
        let root = td.path().join("Standard_Forms");
        std::fs::create_dir_all(root.join("General Forms")).unwrap();

        // Authoring templates (must survive):
        std::fs::write(
            root.join("General Forms/Bulletin Initial.html"),
            "<html><!-- authoring --></html>",
        )
        .unwrap();
        std::fs::write(
            root.join("General Forms/ICS213_Initial.html"),
            "<html><!-- authoring --></html>",
        )
        .unwrap();
        std::fs::write(
            root.join("General Forms/Quick Message Initial.html"),
            "<html><!-- authoring --></html>",
        )
        .unwrap();

        // Companion Viewer + SendReply templates (must be filtered):
        std::fs::write(
            root.join("General Forms/Bulletin Viewer.html"),
            "<html><!-- viewer --></html>",
        )
        .unwrap();
        std::fs::write(
            root.join("General Forms/ICS213_Viewer.html"),
            "<html><!-- viewer --></html>",
        )
        .unwrap();
        std::fs::write(
            root.join("General Forms/ICS213_SendReply.html"),
            "<html><!-- sendreply --></html>",
        )
        .unwrap();

        let cat = list(&root, None).unwrap();
        let ids: Vec<&str> = cat.iter().map(|t| t.id.as_str()).collect();

        // Authoring survives:
        assert!(ids.contains(&"Bulletin Initial"), "Bulletin Initial survives");
        assert!(ids.contains(&"ICS213_Initial"), "ICS213_Initial survives");
        assert!(ids.contains(&"Quick Message Initial"), "Quick Message Initial survives");

        // Viewer + SendReply filtered:
        assert!(!ids.contains(&"Bulletin Viewer"), "Bulletin Viewer filtered");
        assert!(!ids.contains(&"ICS213_Viewer"), "ICS213_Viewer filtered");
        assert!(!ids.contains(&"ICS213_SendReply"), "ICS213_SendReply filtered");
    }

    /// The Viewer/SendReply detector is case-insensitive — WLE has
    /// `Field Situation Report viewer.html` (lowercase v) alongside
    /// `Bulletin Viewer.html` (uppercase V). Both must be filtered.
    #[test]
    fn is_authoring_template_stem_is_case_insensitive() {
        assert!(is_authoring_template_stem("ICS213_Initial"));
        assert!(is_authoring_template_stem("Bulletin Initial"));
        assert!(is_authoring_template_stem("ICS309 Initial"));

        // Various Viewer casings — all rejected:
        assert!(!is_authoring_template_stem("Bulletin Viewer"));
        assert!(!is_authoring_template_stem("Field Situation Report viewer"));
        assert!(!is_authoring_template_stem("HH_Daily_Shelter_Report_Viewer"));

        // Various SendReply casings — all rejected:
        assert!(!is_authoring_template_stem("ICS213_SendReply"));
        assert!(!is_authoring_template_stem("ICS213_sendreply"));
        assert!(!is_authoring_template_stem("Some Form SendReply"));
    }

    /// 2026-06-04 Codex adrev P1.3: resolve_viewer_for walks the
    /// authoring template's sibling folder and returns the actual viewer
    /// filename, accommodating WLE's inconsistent naming conventions.
    #[test]
    fn resolve_viewer_for_underscore_swap() {
        let td = TempDir::new().unwrap();
        let folder = td.path().join("ICS Forms");
        std::fs::create_dir_all(&folder).unwrap();
        let authoring = folder.join("ICS213_Initial.html");
        std::fs::write(&authoring, "<html></html>").unwrap();
        std::fs::write(folder.join("ICS213_Viewer.html"), "<html></html>").unwrap();

        let resolved = resolve_viewer_for(&authoring).expect("viewer resolved");
        assert_eq!(resolved, "ICS213_Viewer.html");
    }

    #[test]
    fn resolve_viewer_for_space_swap() {
        // Bulletin Initial.html ↔ Bulletin Viewer.html
        let td = TempDir::new().unwrap();
        let folder = td.path().join("General Forms");
        std::fs::create_dir_all(&folder).unwrap();
        let authoring = folder.join("Bulletin Initial.html");
        std::fs::write(&authoring, "<html></html>").unwrap();
        std::fs::write(folder.join("Bulletin Viewer.html"), "<html></html>").unwrap();

        let resolved = resolve_viewer_for(&authoring).expect("viewer resolved");
        assert_eq!(resolved, "Bulletin Viewer.html");
    }

    #[test]
    fn resolve_viewer_for_append_viewer_no_initial_suffix() {
        // Hawaii Siren Report.html (no "Initial" anywhere) ↔
        // Hawaii Siren Report Viewer.html
        let td = TempDir::new().unwrap();
        let folder = td.path().join("HI State forms");
        std::fs::create_dir_all(&folder).unwrap();
        let authoring = folder.join("Hawaii Siren Report.html");
        std::fs::write(&authoring, "<html></html>").unwrap();
        std::fs::write(
            folder.join("Hawaii Siren Report Viewer.html"),
            "<html></html>",
        )
        .unwrap();

        let resolved = resolve_viewer_for(&authoring).expect("viewer resolved");
        assert_eq!(resolved, "Hawaii Siren Report Viewer.html");
    }

    #[test]
    fn resolve_viewer_for_handles_case_drift() {
        // Field Situation Report Initial.html ↔
        // Field Situation Report viewer.html (lowercase v on disk)
        let td = TempDir::new().unwrap();
        let folder = td.path().join("General Forms");
        std::fs::create_dir_all(&folder).unwrap();
        let authoring = folder.join("Field Situation Report Initial.html");
        std::fs::write(&authoring, "<html></html>").unwrap();
        std::fs::write(
            folder.join("Field Situation Report viewer.html"),
            "<html></html>",
        )
        .unwrap();

        let resolved = resolve_viewer_for(&authoring).expect("viewer resolved");
        // The returned filename preserves on-disk case (lowercase v).
        assert_eq!(resolved, "Field Situation Report viewer.html");
    }

    #[test]
    fn resolve_viewer_for_returns_none_when_no_viewer_present() {
        let td = TempDir::new().unwrap();
        let folder = td.path().join("Custom");
        std::fs::create_dir_all(&folder).unwrap();
        let authoring = folder.join("OnlyAuthoring_Initial.html");
        std::fs::write(&authoring, "<html></html>").unwrap();

        // No viewer file in the folder.
        assert!(resolve_viewer_for(&authoring).is_none());
    }

    #[test]
    fn resolve_viewer_for_prefers_initial_swap_over_append_when_both_exist() {
        // Defensive: if a folder somehow has BOTH "X_Viewer.html"
        // (Initial-swap form) AND "X_Initial Viewer.html" (append form),
        // the Initial-swap candidate wins because it's the first in
        // `expected_viewer_stems`.
        let td = TempDir::new().unwrap();
        let folder = td.path().join("Custom");
        std::fs::create_dir_all(&folder).unwrap();
        let authoring = folder.join("Form_Initial.html");
        std::fs::write(&authoring, "<html></html>").unwrap();
        std::fs::write(folder.join("Form_Viewer.html"), "<html></html>").unwrap();
        std::fs::write(folder.join("Form_Initial Viewer.html"), "<html></html>").unwrap();

        let resolved = resolve_viewer_for(&authoring).expect("viewer resolved");
        assert_eq!(resolved, "Form_Viewer.html");
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
