//! In-app form import (Forms-push G5+G6). Two-phase validate-before-write:
//! `preview` stages+classifies under a 0700 temp dir and mints a registry
//! token; `commit` consumes the token single-shot and atomically promotes
//! the validated set into the custom-forms dir. Detection is .txt-`Form:`-
//! directive-driven (spec §11.1), NOT an HTML heuristic — the real WLE
//! bundle is Windows-1252 and some authoring forms contain no `<form>`.
//!
//! Canonical design: docs/superpowers/specs/2026-06-11-forms-import-design.md
//! (§11 supersedes §1–10 where they conflict).

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportKind {
    /// New form, no collision.
    Added,
    /// Collides with an existing CUSTOM form → needs operator confirm.
    Update,
    /// Shadows a BUNDLED form (intended; warn, no confirm).
    OverridesStandard,
    /// Viewer / `.txt` / asset — copied alongside, never catalog-surfaced.
    Companion,
    /// Intra-batch or cross-folder stem duplicate; first wins.
    Skip,
    /// Failed validation; never written.
    Reject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportEntry {
    /// Path relative to the (unwrapped) source root.
    pub rel_path: String,
    /// Form id derived from the filename stem.
    pub id: String,
    /// Category folder (relative; "" = root).
    pub folder: String,
    pub kind: ImportKind,
    /// Populated for Skip/Reject + amber warnings (override-standard, no-viewer).
    pub reason: Option<String>,
    /// `false` → "no viewer" amber note on an authoring form.
    pub has_viewer: bool,
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
    /// Ids written into the custom-forms dir.
    pub installed: Vec<String>,
    /// `Update` entries the operator did NOT confirm (left untouched).
    pub skipped_updates: Vec<String>,
    /// Realized per-file outcome.
    pub entries: Vec<ImportEntry>,
}

/// Whole-operation failure (NOT per-entry — those live in [`ImportResult`]).
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
