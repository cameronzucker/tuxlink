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

// ============================================================================
// Detection (§11.1) — the import unit is the `.txt` template, NOT the HTML.
// ============================================================================

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// A single file discovered during detection, pre-classification against the
/// live catalog. `Authoring` candidates become catalog-surfaced forms;
/// `Companion`s (viewer/.txt/assets) are copied but never surfaced; `Reject`s
/// are never written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CandidateKind {
    Authoring,
    Companion,
    Reject,
}

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

/// Decode any file as text without panicking on Windows-1252 byte sequences
/// (the real WLE bundle is cp1252, not UTF-8). NEVER use `read_to_string`.
fn read_text_lossy(p: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(p)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Org-package metadata that should be silently ignored on import (§11.2) —
/// never surfaced as a `reject` row.
fn is_metadata_file(name_lower: &str) -> bool {
    name_lower.ends_with(".dat")
        || name_lower == "changelog.txt"
        || name_lower.ends_with("_version.dat")
}

/// Tolerant orphan-fallback probe: does this HTML look like a Winlink
/// authoring form? Only used when no `.txt` governs the file. Real forms are
/// frequently malformed HTML, so this is a lowercased substring scan, NOT a
/// strict parser (a strict parser rejects valid forms).
fn html_looks_like_form(lowered: &str) -> bool {
    if !lowered.contains("<form") {
        return false;
    }
    let has_post = lowered.contains("method=post")
        || lowered.contains("method =post")
        || lowered.contains("method= post")
        || lowered.contains("method = post")
        || lowered.contains("method=\"post\"")
        || lowered.contains("method='post'");
    let has_multipart = lowered.contains("multipart/form-data");
    let targets_local = ["{formserver}", "{formport}", "localhost", "127.0.0.1"]
        .iter()
        .any(|t| lowered.contains(t));
    has_post && has_multipart && targets_local
}

/// Lowercased extension of a path ("" if none).
fn ext_lower(p: &Path) -> String {
    p.extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn is_html_ext(ext: &str) -> bool {
    ext == "html" || ext == "htm"
}

/// Resolve a directive-named filename against the actual on-disk siblings in
/// `folder`, case-insensitively (the WLE catalog drifts on case). Returns the
/// real path preserving on-disk case, or `None` if absent.
fn resolve_on_disk(folder: &Path, filename: &str) -> Option<PathBuf> {
    let want = filename.trim().to_lowercase();
    std::fs::read_dir(folder)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.to_lowercase() == want)
                .unwrap_or(false)
        })
}

/// Parse a `.txt` template's `Form:` directive → (input_filename, optional
/// display_filename). Returns `None` if there's no `Form:` line. Filenames may
/// contain spaces; the separator is the first comma.
fn parse_form_directive(txt: &str) -> Option<(String, Option<String>)> {
    for line in txt.lines() {
        let trimmed = line.trim_start();
        let lower = trimmed.to_lowercase();
        if let Some(rest) = lower.strip_prefix("form:") {
            // Recover the original-case value at the same byte offset.
            let val = &trimmed[trimmed.len() - rest.len()..];
            let mut parts = val.splitn(2, ',');
            let input = parts.next()?.trim().to_string();
            if input.is_empty() {
                return None;
            }
            let display = parts.next().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
            return Some((input, display));
        }
    }
    None
}

/// Compute (rel_path, folder, id) for a file under `root`. rel_path uses
/// forward slashes; folder is the parent (relative; "" = root); id is the stem.
fn rel_parts(root: &Path, abs: &Path) -> Option<(String, String, String)> {
    let rel = abs.strip_prefix(root).ok()?;
    let rel_str = rel.to_str()?.replace('\\', "/");
    let folder = rel
        .parent()
        .and_then(|p| p.to_str())
        .map(|s| s.replace('\\', "/"))
        .unwrap_or_default();
    let id = abs.file_stem().and_then(|s| s.to_str())?.to_string();
    Some((rel_str, folder, id))
}

/// Two-pass detection over a staged source tree (§11.1):
/// pass 1 parses every `.txt` `Form:` directive (input → Authoring, display →
/// Companion); pass 2 classifies orphan HTML (viewer/sendreply stems →
/// Companion via `is_authoring_template_stem`; otherwise the fallback HTML
/// probe → Authoring or Reject). All other non-metadata files → Companion
/// (assets). Metadata files are skipped silently.
pub(crate) fn detect_candidates(root: &Path) -> std::io::Result<Vec<Candidate>> {
    use crate::forms::wle_templates::{is_authoring_template_stem, resolve_viewer_for};

    let mut candidates: Vec<Candidate> = Vec::new();
    let mut governed: HashSet<PathBuf> = HashSet::new();

    // Collect every regular file once.
    let files: Vec<PathBuf> = walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .collect();

    // --- Pass 1: .txt directives bind input (authoring) + display (companion).
    for abs in &files {
        if ext_lower(abs) != "txt" {
            continue;
        }
        let name_lower = abs
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();
        if is_metadata_file(&name_lower) {
            continue;
        }
        let folder_path = match abs.parent() {
            Some(p) => p,
            None => continue,
        };
        let txt = read_text_lossy(abs)?;
        if let Some((input_name, display_name)) = parse_form_directive(&txt) {
            let input_abs = resolve_on_disk(folder_path, &input_name);
            let display_abs = display_name
                .as_deref()
                .and_then(|d| resolve_on_disk(folder_path, d));
            if let Some(input_abs) = input_abs {
                governed.insert(input_abs.clone());
                if let Some(ref d) = display_abs {
                    governed.insert(d.clone());
                }
                // has_viewer: the directive named a display that exists, OR a
                // sibling viewer resolves by the catalog's own heuristic.
                let has_viewer =
                    display_abs.is_some() || resolve_viewer_for(&input_abs).is_some();
                if let Some((rel_path, folder, id)) = rel_parts(root, &input_abs) {
                    candidates.push(Candidate {
                        id,
                        folder,
                        rel_path,
                        abs_path: input_abs,
                        kind: CandidateKind::Authoring,
                        reason: None,
                        has_viewer,
                    });
                }
            }
        }
    }

    // --- Pass 2: everything not already governed-as-authoring.
    for abs in &files {
        // Skip the authoring forms we already emitted in pass 1.
        if governed.contains(abs)
            && candidates
                .iter()
                .any(|c| &c.abs_path == abs && c.kind == CandidateKind::Authoring)
        {
            continue;
        }
        let name_lower = abs
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();
        if is_metadata_file(&name_lower) {
            continue; // org-package cruft, silently ignored (§11.2)
        }
        let (rel_path, folder, id) = match rel_parts(root, abs) {
            Some(t) => t,
            None => continue,
        };
        let ext = ext_lower(abs);

        if is_html_ext(&ext) {
            // A display HTML governed by a .txt is a companion.
            if governed.contains(abs) || !is_authoring_template_stem(&id) {
                candidates.push(companion(id, folder, rel_path, abs.clone()));
                continue;
            }
            // Orphan authoring-stem HTML → fallback probe.
            let lowered = read_text_lossy(abs)?.to_lowercase();
            if html_looks_like_form(&lowered) {
                let has_viewer = resolve_viewer_for(abs).is_some();
                candidates.push(Candidate {
                    id,
                    folder,
                    rel_path,
                    abs_path: abs.clone(),
                    kind: CandidateKind::Authoring,
                    reason: None,
                    has_viewer,
                });
            } else {
                candidates.push(Candidate {
                    id,
                    folder,
                    rel_path,
                    abs_path: abs.clone(),
                    kind: CandidateKind::Reject,
                    reason: Some("not a Winlink form".to_string()),
                    has_viewer: false,
                });
            }
        } else {
            // .txt template bodies + css/js/image assets → companions (copied,
            // never catalog-surfaced).
            candidates.push(companion(id, folder, rel_path, abs.clone()));
        }
    }

    Ok(candidates)
}

fn companion(id: String, folder: String, rel_path: String, abs_path: PathBuf) -> Candidate {
    Candidate {
        id,
        folder,
        rel_path,
        abs_path,
        kind: CandidateKind::Companion,
        reason: None,
        has_viewer: false,
    }
}

// ============================================================================
// Path safety + archive caps (§11.5). Import is a WRITE path, so it validates
// every path component strictly — stricter than `is_valid_form_id`, whose
// space/dot/`&` relaxation is a receive-path concession.
// ============================================================================

/// Entry-count cap applied uniformly to folder + zip sources (the existing
/// updater extractor lacks one — §11.5).
const MAX_IMPORT_ENTRIES: usize = 5_000;
/// Per-file uncompressed cap.
const MAX_SINGLE_FILE_BYTES: u64 = 16 * 1_048_576;
/// Aggregate uncompressed cap across a whole import.
const MAX_TOTAL_BYTES: u64 = 300 * 1_048_576;
/// Per-entry compression-ratio guard (zip-bomb defense).
const MAX_COMPRESSION_RATIO: u64 = 200;

/// Windows reserved device names (rejected as path components — these can
/// behave specially even on Linux if the tree is later moved to Windows, and
/// they signal a hostile archive).
const RESERVED_WIN: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Validate a relative path destined for `custom_root`. Rejects `..`/`.`,
/// absolute paths, empty/leading-dot components, control/NUL chars, and
/// reserved Windows device names — on EVERY component, not just the stem.
pub(crate) fn is_safe_rel_path(rel: &str) -> bool {
    if rel.is_empty() || rel.starts_with('/') || rel.starts_with('\\') {
        return false;
    }
    for c in rel.split(['/', '\\']) {
        if c.is_empty() || c == "." || c == ".." {
            return false;
        }
        if c.starts_with('.') {
            return false; // leading-dot component (.hidden, .prev-*)
        }
        if c.bytes().any(|b| b < 0x20 || b == 0x7f) {
            return false; // control / NUL / DEL
        }
        let stem_lower = c.split('.').next().unwrap_or("").to_ascii_lowercase();
        if RESERVED_WIN.contains(&stem_lower.as_str()) {
            return false;
        }
    }
    true
}

/// Per-entry zip-bomb ratio check. Stored/empty entries (compressed == 0)
/// can't be ratio bombs and avoid division by zero.
pub(crate) fn exceeds_ratio(compressed: u64, uncompressed: u64) -> bool {
    if compressed == 0 {
        return false;
    }
    uncompressed / compressed > MAX_COMPRESSION_RATIO
}

// ============================================================================
// Staging (§11.4/§11.5) — copy sources into a fresh 0700 temp dir, validated.
// Nothing reaches `custom_root` until commit. Sources are file / folder / zip.
// ============================================================================

/// A validated staging tree owned by its `TempDir` guard. Dropping `Staged`
/// (on cancel, commit, or registry reap) `rm -rf`s the dir.
#[derive(Debug)]
pub(crate) struct Staged {
    /// Canonicalized staging root.
    pub dir: PathBuf,
    _guard: tempfile::TempDir,
}

impl Staged {
    #[cfg(test)]
    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }
}

/// Running totals enforced uniformly across all sources of one import.
struct Counters {
    entries: usize,
    total: u64,
}

impl Counters {
    fn new() -> Self {
        Counters { entries: 0, total: 0 }
    }

    /// Account one staged file; reject if any cap is exceeded.
    fn enforce(&mut self, file_size: u64) -> Result<(), ImportError> {
        self.entries += 1;
        if self.entries > MAX_IMPORT_ENTRIES {
            return Err(ImportError::StagingFailed {
                reason: format!("too many entries (> {MAX_IMPORT_ENTRIES})"),
            });
        }
        if file_size > MAX_SINGLE_FILE_BYTES {
            return Err(ImportError::StagingFailed {
                reason: format!("file exceeds {MAX_SINGLE_FILE_BYTES} bytes"),
            });
        }
        self.total = self.total.saturating_add(file_size);
        if self.total > MAX_TOTAL_BYTES {
            return Err(ImportError::StagingFailed {
                reason: format!("total uncompressed size exceeds {MAX_TOTAL_BYTES} bytes"),
            });
        }
        Ok(())
    }
}

/// Stage every source into a fresh owner-only temp dir. The dir lives until
/// the returned [`Staged`] drops.
pub(crate) fn stage_sources(sources: &[String]) -> Result<Staged, ImportError> {
    let guard = tempfile::Builder::new()
        .prefix("tuxlink-formimport-")
        .tempdir()
        .map_err(|e| ImportError::StagingFailed {
            reason: format!("mkdir staging: {e}"),
        })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(guard.path(), std::fs::Permissions::from_mode(0o700)).map_err(
            |e| ImportError::StagingFailed {
                reason: format!("chmod staging: {e}"),
            },
        )?;
    }
    let dir = guard.path().canonicalize().map_err(|e| ImportError::StagingFailed {
        reason: format!("canonicalize staging: {e}"),
    })?;

    let mut counters = Counters::new();
    for src in sources {
        let sp = Path::new(src);
        let md = std::fs::symlink_metadata(sp).map_err(|e| ImportError::StagingFailed {
            reason: format!("stat {src}: {e}"),
        })?;
        if md.file_type().is_symlink() {
            return Err(ImportError::StagingFailed {
                reason: format!("source is a symlink: {src}"),
            });
        }
        if md.is_dir() {
            stage_folder(sp, &dir, &mut counters)?;
        } else if md.is_file() && ext_lower(sp) == "zip" {
            stage_zip(sp, &dir, &mut counters)?;
        } else if md.is_file() {
            stage_single_file(sp, &dir, &mut counters)?;
        } else {
            return Err(ImportError::StagingFailed {
                reason: format!("unsupported source type: {src}"),
            });
        }
    }
    Ok(Staged { dir, _guard: guard })
}

/// Copy a vetted set of (absolute source, relative dest) pairs into the
/// staging dir: strip a uniform `Standard_Forms/` wrapper, drop metadata,
/// validate each rel path, enforce caps.
fn stage_pairs(
    pairs: Vec<(PathBuf, String)>,
    dir: &Path,
    counters: &mut Counters,
) -> Result<(), ImportError> {
    let all_wrapped =
        !pairs.is_empty() && pairs.iter().all(|(_, r)| r.starts_with("Standard_Forms/"));
    for (abs, rel) in pairs {
        let rel = if all_wrapped {
            rel.strip_prefix("Standard_Forms/").unwrap_or(&rel).to_string()
        } else {
            rel
        };
        if rel.is_empty() {
            continue;
        }
        let name_lower = rel.rsplit('/').next().unwrap_or("").to_lowercase();
        if is_metadata_file(&name_lower) {
            continue;
        }
        if !is_safe_rel_path(&rel) {
            return Err(ImportError::StagingFailed {
                reason: format!("unsafe path: {rel}"),
            });
        }
        let size = std::fs::metadata(&abs)
            .map_err(|e| ImportError::StagingFailed {
                reason: format!("stat {abs:?}: {e}"),
            })?
            .len();
        counters.enforce(size)?;
        let dest = dir.join(&rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ImportError::StagingFailed {
                reason: format!("mkdir {parent:?}: {e}"),
            })?;
        }
        std::fs::copy(&abs, &dest).map_err(|e| ImportError::StagingFailed {
            reason: format!("copy {abs:?}: {e}"),
        })?;
    }
    Ok(())
}

/// Folder source: walk (NOT following symlinks), reject any symlink, collect
/// regular files as (abs, rel-to-folder).
fn stage_folder(src: &Path, dir: &Path, counters: &mut Counters) -> Result<(), ImportError> {
    let mut pairs: Vec<(PathBuf, String)> = Vec::new();
    for entry in walkdir::WalkDir::new(src).follow_links(false) {
        let entry = entry.map_err(|e| ImportError::StagingFailed {
            reason: format!("walk {src:?}: {e}"),
        })?;
        let ft = entry.file_type();
        if ft.is_symlink() {
            return Err(ImportError::StagingFailed {
                reason: format!("symlink in source folder: {:?}", entry.path()),
            });
        }
        if !ft.is_file() {
            continue; // dirs created implicitly; non-regular skipped
        }
        let rel = entry
            .path()
            .strip_prefix(src)
            .map_err(|_| ImportError::StagingFailed {
                reason: "path escapes source root".into(),
            })?
            .to_string_lossy()
            .replace('\\', "/");
        pairs.push((entry.path().to_path_buf(), rel));
    }
    stage_pairs(pairs, dir, counters)
}

/// Single-file source: copy the HTML plus its governing `.txt` (the one whose
/// `Form:` directive names this file) and that directive's display companion,
/// or a sibling viewer as a fallback.
fn stage_single_file(src: &Path, dir: &Path, counters: &mut Counters) -> Result<(), ImportError> {
    use crate::forms::wle_templates::resolve_viewer_for;
    let folder = src.parent().unwrap_or_else(|| Path::new("."));
    let file_name = src
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| ImportError::StagingFailed {
            reason: "source has no filename".into(),
        })?;

    let mut pairs: Vec<(PathBuf, String)> = vec![(src.to_path_buf(), file_name.to_string())];

    // Find a sibling .txt whose Form: directive names this html as input.
    let mut governing_txt: Option<PathBuf> = None;
    if let Ok(rd) = std::fs::read_dir(folder) {
        for e in rd.filter_map(|e| e.ok()) {
            let p = e.path();
            if ext_lower(&p) != "txt" {
                continue;
            }
            if let Ok(txt) = read_text_lossy(&p) {
                if let Some((input, display)) = parse_form_directive(&txt) {
                    if input.trim().eq_ignore_ascii_case(file_name) {
                        governing_txt = Some(p.clone());
                        if let Some(n) = p.file_name().and_then(|n| n.to_str()) {
                            pairs.push((p.clone(), n.to_string()));
                        }
                        if let Some(d) = display {
                            if let Some(dp) = resolve_on_disk(folder, &d) {
                                if let Some(n) = dp.file_name().and_then(|n| n.to_str()) {
                                    pairs.push((dp.clone(), n.to_string()));
                                }
                            }
                        }
                        break;
                    }
                }
            }
        }
    }
    // No governing .txt → best-effort sibling viewer.
    if governing_txt.is_none() {
        if let Some(viewer_name) = resolve_viewer_for(src) {
            let vp = folder.join(&viewer_name);
            if vp.is_file() {
                pairs.push((vp, viewer_name));
            }
        }
    }
    stage_pairs(pairs, dir, counters)
}

/// Zip source: hardened extraction — `enclosed_name` + caps + ratio guard +
/// `Standard_Forms/` unwrap. Mirrors `updater::extract_zip`'s traversal guard.
fn stage_zip(src: &Path, dir: &Path, counters: &mut Counters) -> Result<(), ImportError> {
    let file = std::fs::File::open(src).map_err(|e| ImportError::StagingFailed {
        reason: format!("open zip {src:?}: {e}"),
    })?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| ImportError::StagingFailed {
        reason: format!("read zip: {e}"),
    })?;
    if archive.len() > MAX_IMPORT_ENTRIES {
        return Err(ImportError::StagingFailed {
            reason: format!("archive has too many entries (> {MAX_IMPORT_ENTRIES})"),
        });
    }

    // Pass 1: detect whether every entry is wrapped under Standard_Forms/.
    let mut all_wrapped = !archive.is_empty();
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|e| ImportError::StagingFailed {
            reason: format!("zip entry {i}: {e}"),
        })?;
        let p = match entry.enclosed_name() {
            Some(p) => p.to_path_buf(),
            None => {
                return Err(ImportError::StagingFailed {
                    reason: format!("zip entry {i} path traversal: {}", entry.name()),
                })
            }
        };
        if !p.starts_with("Standard_Forms") {
            all_wrapped = false;
        }
    }

    // Pass 2: extract.
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| ImportError::StagingFailed {
            reason: format!("zip entry {i}: {e}"),
        })?;
        let entry_path = match entry.enclosed_name() {
            Some(p) => p.to_path_buf(),
            None => {
                return Err(ImportError::StagingFailed {
                    reason: format!("zip entry {i} path traversal: {}", entry.name()),
                })
            }
        };
        if entry.is_dir() {
            continue;
        }
        let stripped = if all_wrapped {
            entry_path.strip_prefix("Standard_Forms").unwrap_or(&entry_path).to_path_buf()
        } else {
            entry_path.clone()
        };
        let rel = stripped.to_string_lossy().replace('\\', "/");
        if rel.is_empty() {
            continue;
        }
        let name_lower = rel.rsplit('/').next().unwrap_or("").to_lowercase();
        if is_metadata_file(&name_lower) {
            continue;
        }
        if !is_safe_rel_path(&rel) {
            return Err(ImportError::StagingFailed {
                reason: format!("unsafe zip path: {rel}"),
            });
        }
        if exceeds_ratio(entry.compressed_size(), entry.size()) {
            return Err(ImportError::StagingFailed {
                reason: format!("entry {rel} exceeds compression-ratio guard (zip bomb?)"),
            });
        }
        counters.enforce(entry.size())?;
        let dest = dir.join(&rel);
        // Defense in depth: the canonical parent must stay under the staging dir.
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ImportError::StagingFailed {
                reason: format!("mkdir {parent:?}: {e}"),
            })?;
        }
        if !dest.starts_with(dir) {
            return Err(ImportError::StagingFailed {
                reason: format!("zip entry escapes staging: {rel}"),
            });
        }
        let mut out = std::fs::File::create(&dest).map_err(|e| ImportError::StagingFailed {
            reason: format!("create {dest:?}: {e}"),
        })?;
        std::io::copy(&mut entry, &mut out).map_err(|e| ImportError::StagingFailed {
            reason: format!("extract {rel}: {e}"),
        })?;
    }
    Ok(())
}

// ============================================================================
// Classification (§11.2/§6) — detected candidates → ImportEntry, against the
// live custom + bundled catalog, with folder-aware cross-stem dupe detection.
// ============================================================================

/// Classify staged candidates against the existing custom-form ids and the
/// bundled-form ids. Cross-folder stem collisions (intra-batch) become `Skip`
/// (first wins) — never silently collapsed (the catalog is keyed by stem;
/// the `(folder,id)` engine fix is the separate issue tuxlink-8v3l).
pub(crate) fn classify(
    cands: Vec<Candidate>,
    existing_custom: &HashSet<String>,
    bundled: &HashSet<String>,
) -> Vec<ImportEntry> {
    let mut out: Vec<ImportEntry> = Vec::with_capacity(cands.len());
    // stem → folder of the first authoring candidate that claimed it this batch.
    let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    for c in cands {
        let entry = match c.kind {
            CandidateKind::Reject => ImportEntry {
                rel_path: c.rel_path,
                id: c.id,
                folder: c.folder,
                kind: ImportKind::Reject,
                reason: c.reason,
                has_viewer: c.has_viewer,
            },
            CandidateKind::Companion => ImportEntry {
                rel_path: c.rel_path,
                id: c.id,
                folder: c.folder,
                kind: ImportKind::Companion,
                reason: None,
                has_viewer: c.has_viewer,
            },
            CandidateKind::Authoring => {
                if let Some(first_folder) = seen.get(&c.id) {
                    ImportEntry {
                        reason: Some(format!(
                            "duplicate stem \"{}\" in {} (already importing from {})",
                            c.id, c.folder, first_folder
                        )),
                        kind: ImportKind::Skip,
                        rel_path: c.rel_path,
                        id: c.id,
                        folder: c.folder,
                        has_viewer: c.has_viewer,
                    }
                } else {
                    seen.insert(c.id.clone(), c.folder.clone());
                    let (kind, reason) = if existing_custom.contains(&c.id) {
                        (ImportKind::Update, None)
                    } else if bundled.contains(&c.id) {
                        (
                            ImportKind::OverridesStandard,
                            Some(format!("Replaces the standard {}", c.id)),
                        )
                    } else {
                        (ImportKind::Added, None)
                    };
                    ImportEntry {
                        rel_path: c.rel_path,
                        id: c.id,
                        folder: c.folder,
                        kind,
                        reason,
                        has_viewer: c.has_viewer,
                    }
                }
            }
        };
        out.push(entry);
    }
    out
}

/// Tally an [`ImportSummary`] from classified entries.
pub(crate) fn summarize(entries: &[ImportEntry]) -> ImportSummary {
    let mut s = ImportSummary::default();
    for e in entries {
        match e.kind {
            ImportKind::Added => s.added += 1,
            ImportKind::Update => s.updated += 1,
            ImportKind::OverridesStandard => s.overrides_standard += 1,
            ImportKind::Companion => s.companions += 1,
            ImportKind::Skip => s.skipped += 1,
            ImportKind::Reject => s.rejected += 1,
        }
    }
    s
}

/// The set of authoring-form ids surfaced from a forms root — the same
/// predicate the catalog walker applies (`is_authoring_template_stem`,
/// `.html`/`.htm`). Used to detect collisions during classification.
pub(crate) fn stem_set(root: &Path) -> HashSet<String> {
    use crate::forms::wle_templates::is_authoring_template_stem;
    let mut s = HashSet::new();
    if !root.exists() {
        return s;
    }
    for e in walkdir::WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        if !e.file_type().is_file() {
            continue;
        }
        if !is_html_ext(&ext_lower(e.path())) {
            continue;
        }
        if let Some(stem) = e.path().file_stem().and_then(|s| s.to_str()) {
            if is_authoring_template_stem(stem) {
                s.insert(stem.to_string());
            }
        }
    }
    s
}

// ============================================================================
// Staging registry (§11.4) — owns staged dirs by opaque token; commit is
// single-shot. Tokens die with the process (no cross-session replay).
// ============================================================================

struct StagedEntry {
    staged: Staged,
    created_at: std::time::SystemTime,
}

/// Token → staged-dir map. Tauri-`manage`d as `Arc<ImportStagingRegistry>`.
pub struct ImportStagingRegistry {
    inner: std::sync::Mutex<std::collections::HashMap<String, StagedEntry>>,
}

impl Default for ImportStagingRegistry {
    fn default() -> Self {
        ImportStagingRegistry {
            inner: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }
}

impl ImportStagingRegistry {
    /// Register a staged tree; returns its opaque 16-hex token.
    pub fn insert(&self, staged: Staged) -> String {
        let mut g = self.inner.lock().unwrap();
        let mut tok = mint_import_token();
        while g.contains_key(&tok) {
            tok = mint_import_token();
        }
        g.insert(
            tok.clone(),
            StagedEntry {
                staged,
                created_at: std::time::SystemTime::now(),
            },
        );
        tok
    }

    /// Single-shot consume — remove + return the staged tree (or `None` if the
    /// token is unknown/already used).
    pub fn take(&self, token: &str) -> Option<Staged> {
        self.inner.lock().unwrap().remove(token).map(|e| e.staged)
    }

    /// Drop a staged tree (cancel). Removal → `Staged`'s `TempDir` drop `rm -rf`s it.
    pub fn cancel(&self, token: &str) {
        let _ = self.inner.lock().unwrap().remove(token);
    }

    /// Reap entries older than `ttl_secs`.
    pub fn reap(&self, ttl_secs: u64) {
        let now = std::time::SystemTime::now();
        self.inner.lock().unwrap().retain(|_, e| {
            now.duration_since(e.created_at)
                .map(|d| d.as_secs() < ttl_secs)
                .unwrap_or(true)
        });
    }

    #[cfg(test)]
    pub fn insert_with_age(&self, staged: Staged, secs_ago: u64) -> String {
        let mut g = self.inner.lock().unwrap();
        let mut tok = mint_import_token();
        while g.contains_key(&tok) {
            tok = mint_import_token();
        }
        let created_at =
            std::time::SystemTime::now() - std::time::Duration::from_secs(secs_ago);
        g.insert(tok.clone(), StagedEntry { staged, created_at });
        tok
    }
}

/// 16 hex chars from `rand::random` — identical shape to
/// `http_server::mint_session_token`. Process-lifetime only.
fn mint_import_token() -> String {
    (0..16)
        .map(|_| {
            let n: u8 = rand::random::<u8>() & 0xF;
            std::char::from_digit(n as u32, 16).unwrap()
        })
        .collect()
}

/// Stage + validate + classify sources and register the staging dir. Writes
/// NOTHING to `custom_root` (the security boundary — §11.4). Synchronous; the
/// Tauri command runs it under `spawn_blocking`.
pub(crate) fn preview_sources(
    sources: &[String],
    custom_root: &Path,
    bundle_root: &Path,
    reg: &ImportStagingRegistry,
) -> Result<ImportPlan, ImportError> {
    let staged = stage_sources(sources)?;
    let cands = detect_candidates(&staged.dir).map_err(|e| ImportError::Io {
        reason: format!("detect: {e}"),
    })?;
    let existing_custom = stem_set(custom_root);
    let bundled = stem_set(bundle_root);
    let entries = classify(cands, &existing_custom, &bundled);
    let summary = summarize(&entries);
    let staging_token = reg.insert(staged);
    Ok(ImportPlan {
        staging_token,
        entries,
        summary,
    })
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

    // ---- Task 2: detection ----

    fn write_bytes(p: &std::path::Path, b: &[u8]) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b).unwrap();
    }

    fn kinds_for<'a>(c: &'a [Candidate], rel_suffix: &str) -> Vec<&'a Candidate> {
        c.iter().filter(|x| x.rel_path.ends_with(rel_suffix)).collect()
    }

    #[test]
    fn detects_authoring_form_via_txt_form_directive() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path();
        write_bytes(
            &root.join("AAMRON/Net Check-in.txt"),
            b"Form: Net Check-in Initial.html, Net Check-in Viewer.html\r\nMsg Type: ...\r\n",
        );
        // authoring HTML: Windows-1252 (0x92 = right single quote), unsubstituted action
        write_bytes(
            &root.join("AAMRON/Net Check-in Initial.html"),
            b"<html><body><form method=post enctype=multipart/form-data \
              action=\"http://{FormServer}:{FormPort}\">It\x92s here</form></body></html>",
        );
        write_bytes(&root.join("AAMRON/Net Check-in Viewer.html"), b"<html>viewer</html>");

        let cands = detect_candidates(root).unwrap();
        let authoring: Vec<_> = cands
            .iter()
            .filter(|c| c.kind == CandidateKind::Authoring)
            .collect();
        assert_eq!(authoring.len(), 1, "exactly one authoring form");
        assert_eq!(authoring[0].id, "Net Check-in Initial");
        assert_eq!(authoring[0].folder, "AAMRON");
        assert!(authoring[0].has_viewer, "viewer named in the .txt directive");
        // The viewer is a companion, never an authoring candidate.
        assert!(cands.iter().any(|c| c.kind == CandidateKind::Companion
            && c.rel_path.ends_with("Net Check-in Viewer.html")));
    }

    #[test]
    fn authoring_form_with_zero_form_tag_still_detected_via_txt() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path();
        write_bytes(&root.join("ARC 213 Message.txt"), b"Form: ARC 213 Message Initial.html\r\n");
        write_bytes(
            &root.join("ARC 213 Message Initial.html"),
            b"<html><body>no form element here, JS builds it</body></html>",
        );
        let cands = detect_candidates(root).unwrap();
        assert!(
            cands.iter().any(|c| c.kind == CandidateKind::Authoring
                && c.id == "ARC 213 Message Initial"),
            "trust the .txt directive even when the <form> probe is inconclusive"
        );
    }

    #[test]
    fn orphan_html_with_form_action_placeholder_is_authoring() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path();
        write_bytes(
            &root.join("Loose Initial.html"),
            b"<form METHOD=POST EncType=Multipart/Form-Data action='HTTP://LOCALHOST:8001'>x</form>",
        );
        let cands = detect_candidates(root).unwrap();
        assert!(cands
            .iter()
            .any(|c| c.kind == CandidateKind::Authoring && c.id == "Loose Initial"));
    }

    #[test]
    fn orphan_non_form_html_is_rejected_not_added() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path();
        write_bytes(&root.join("readme.html"), b"<html><body>About our group</body></html>");
        let cands = detect_candidates(root).unwrap();
        let r = kinds_for(&cands, "readme.html");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].kind, CandidateKind::Reject);
        assert!(r[0].reason.as_deref().unwrap().contains("not a Winlink form"));
    }

    #[test]
    fn viewer_and_sendreply_stems_are_companions_never_authoring() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path();
        // orphan viewer/sendreply (no .txt) that DO contain <form> — must not
        // import as compose options.
        write_bytes(
            &root.join("Foo Viewer.html"),
            b"<form method=post enctype=multipart/form-data action=\"http://{FormServer}:{FormPort}\">x</form>",
        );
        write_bytes(
            &root.join("Foo SendReply.html"),
            b"<form method=post enctype=multipart/form-data action=\"http://{FormServer}:{FormPort}\">x</form>",
        );
        let cands = detect_candidates(root).unwrap();
        assert!(
            cands.iter().all(|c| c.kind != CandidateKind::Authoring),
            "Viewer/SendReply stems are companions even when they contain a form"
        );
    }

    #[test]
    fn reads_windows1252_without_panicking() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path();
        write_bytes(&root.join("W.txt"), b"Form: W Initial.html\r\n");
        // lone 0x92/0xA0 bytes are invalid UTF-8 — read_to_string().unwrap() panics
        write_bytes(
            &root.join("W Initial.html"),
            &[b'<', b'f', b'o', b'r', b'm', b' ', 0x92, 0xA0, b'>'],
        );
        let cands = detect_candidates(root).unwrap(); // must not panic
        assert!(cands.iter().any(|c| c.kind == CandidateKind::Authoring && c.id == "W Initial"));
    }

    #[test]
    fn metadata_files_are_silently_ignored() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path();
        write_bytes(&root.join("Standard_Forms_Version.dat"), b"1.2.3");
        write_bytes(&root.join("Changelog.txt"), b"changes...");
        let cands = detect_candidates(root).unwrap();
        assert!(cands.is_empty(), "metadata produces no candidates, not reject rows");
    }

    // ---- Task 3: path safety + ratio guard ----

    #[test]
    fn rejects_dotdot_and_absolute_and_reserved_components() {
        assert!(!is_safe_rel_path("a/../b"));
        assert!(!is_safe_rel_path("../b"));
        assert!(!is_safe_rel_path("/etc/passwd"));
        assert!(!is_safe_rel_path("a/./b"));
        assert!(!is_safe_rel_path("a//b"));
        assert!(!is_safe_rel_path(".hidden/b"));
        assert!(!is_safe_rel_path("a/\u{0}b"));
        assert!(!is_safe_rel_path("a/b\tc"));
        assert!(!is_safe_rel_path("CON/x.html"));
        assert!(!is_safe_rel_path("a/PRN"));
        assert!(!is_safe_rel_path("com1.html"));
        // happy path:
        assert!(is_safe_rel_path("AAMRON/Net Check-in Initial.html"));
        assert!(is_safe_rel_path("Foo & Bar/Form.v1.html"));
    }

    #[test]
    fn ratio_guard_flags_zip_bomb_entry() {
        assert!(exceeds_ratio(1024, 1024 * 1024 * 1024)); // ~1e6 ratio
        assert!(!exceeds_ratio(1_000_000, 5_000_000)); // 5x, fine
        assert!(!exceeds_ratio(0, 0)); // empty entry, no div-by-zero
        assert!(!exceeds_ratio(0, 9_999)); // stored entry, ratio undefined → allow
    }

    // ---- Task 4: staging ----

    fn build_zip(path: &std::path::Path, entries: &[(&str, &[u8])]) {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        let f = std::fs::File::create(path).unwrap();
        let mut w = zip::ZipWriter::new(f);
        let opts = SimpleFileOptions::default();
        for (name, data) in entries {
            w.start_file(*name, opts).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap();
    }

    const FORM_HTML: &[u8] =
        b"<form method=post enctype=multipart/form-data action=\"http://{FormServer}:{FormPort}\">x</form>";

    #[test]
    fn caps_reject_oversize_single_file() {
        let mut c = Counters::new();
        assert!(c.enforce(MAX_SINGLE_FILE_BYTES + 1).is_err());
    }

    #[test]
    fn caps_reject_entry_count() {
        let mut c = Counters::new();
        c.entries = MAX_IMPORT_ENTRIES;
        assert!(c.enforce(1).is_err());
    }

    #[test]
    fn caps_reject_total_bytes() {
        let mut c = Counters::new();
        c.total = MAX_TOTAL_BYTES;
        assert!(c.enforce(1).is_err());
    }

    #[test]
    fn stage_folder_copies_tree_into_0700_staging() {
        let td = tempfile::tempdir().unwrap();
        let src = td.path().join("org");
        write_bytes(&src.join("A/Form.txt"), b"Form: Form Initial.html\r\n");
        write_bytes(&src.join("A/Form Initial.html"), FORM_HTML);
        let staged = stage_sources(&[src.to_string_lossy().into_owned()]).unwrap();
        assert!(staged.dir().join("A/Form Initial.html").exists());
        assert!(staged.dir().join("A/Form.txt").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(staged.dir()).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700, "staging dir must be owner-only");
        }
    }

    #[test]
    #[cfg(unix)]
    fn stage_rejects_symlink_in_folder_source() {
        let td = tempfile::tempdir().unwrap();
        let src = td.path().join("org");
        std::fs::create_dir_all(&src).unwrap();
        write_bytes(&src.join("real.html"), FORM_HTML);
        std::os::unix::fs::symlink("/etc/passwd", src.join("evil.html")).unwrap();
        let err = stage_sources(&[src.to_string_lossy().into_owned()]).unwrap_err();
        assert!(matches!(err, ImportError::StagingFailed { .. }));
    }

    #[test]
    fn stage_zip_unwraps_standard_forms_wrapper() {
        let td = tempfile::tempdir().unwrap();
        let zip = td.path().join("org.zip");
        build_zip(
            &zip,
            &[
                ("Standard_Forms/AAMRON/Form.txt", b"Form: Form Initial.html\r\n"),
                ("Standard_Forms/AAMRON/Form Initial.html", FORM_HTML),
            ],
        );
        let staged = stage_sources(&[zip.to_string_lossy().into_owned()]).unwrap();
        assert!(
            staged.dir().join("AAMRON/Form Initial.html").exists(),
            "leading Standard_Forms/ wrapper stripped"
        );
        assert!(!staged.dir().join("Standard_Forms").exists());
    }

    #[test]
    fn stage_zip_rejects_path_traversal() {
        let td = tempfile::tempdir().unwrap();
        let zip = td.path().join("evil.zip");
        build_zip(&zip, &[("../escape.html", b"x")]);
        let err = stage_sources(&[zip.to_string_lossy().into_owned()]).unwrap_err();
        assert!(matches!(err, ImportError::StagingFailed { .. }));
    }

    #[test]
    fn stage_single_file_pulls_governing_txt_and_viewer() {
        let td = tempfile::tempdir().unwrap();
        let src = td.path().join("loose");
        write_bytes(
            &src.join("Net Check-in.txt"),
            b"Form: Net Check-in Initial.html, Net Check-in Viewer.html\r\n",
        );
        write_bytes(&src.join("Net Check-in Initial.html"), FORM_HTML);
        write_bytes(&src.join("Net Check-in Viewer.html"), b"<html>viewer</html>");
        let html = src.join("Net Check-in Initial.html");
        let staged = stage_sources(&[html.to_string_lossy().into_owned()]).unwrap();
        assert!(staged.dir().join("Net Check-in Initial.html").exists());
        assert!(staged.dir().join("Net Check-in.txt").exists(), "governing .txt pulled");
        assert!(staged.dir().join("Net Check-in Viewer.html").exists(), "display companion pulled");
    }

    // ---- Task 5: classification ----

    fn auth_cand(id: &str, folder: &str) -> Candidate {
        Candidate {
            id: id.to_string(),
            folder: folder.to_string(),
            rel_path: format!("{folder}/{id}.html"),
            abs_path: PathBuf::from(format!("/staging/{folder}/{id}.html")),
            kind: CandidateKind::Authoring,
            reason: None,
            has_viewer: true,
        }
    }
    fn comp_cand(id: &str, folder: &str) -> Candidate {
        Candidate {
            kind: CandidateKind::Companion,
            ..auth_cand(id, folder)
        }
    }
    fn kind_of(entries: &[ImportEntry], id: &str) -> ImportKind {
        entries.iter().find(|e| e.id == id).unwrap().kind.clone()
    }
    fn set(items: &[&str]) -> HashSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn classifies_added_update_override_skip() {
        let existing_custom = set(&["Net Check-in Initial"]);
        let bundled = set(&["ICS213_Initial"]);
        let cands = vec![
            auth_cand("Brand New Initial", "AAMRON"),
            auth_cand("Net Check-in Initial", "AAMRON"), // collides custom → Update
            auth_cand("ICS213_Initial", "ICS Forms"),    // collides bundled → OverridesStandard
            auth_cand("Dup Initial", "FolderX"),
            auth_cand("Dup Initial", "FolderY"), // same stem twice → 2nd Skip
        ];
        let entries = classify(cands, &existing_custom, &bundled);
        assert_eq!(kind_of(&entries, "Brand New Initial"), ImportKind::Added);
        assert_eq!(kind_of(&entries, "Net Check-in Initial"), ImportKind::Update);
        assert_eq!(kind_of(&entries, "ICS213_Initial"), ImportKind::OverridesStandard);
        let dups: Vec<_> = entries.iter().filter(|e| e.id == "Dup Initial").collect();
        assert_eq!(dups.iter().filter(|e| e.kind == ImportKind::Skip).count(), 1);
        assert!(dups
            .iter()
            .find(|e| e.kind == ImportKind::Skip)
            .unwrap()
            .reason
            .as_deref()
            .unwrap()
            .contains("duplicate stem"));
    }

    #[test]
    fn override_standard_carries_amber_warning_reason() {
        let entries = classify(
            vec![auth_cand("ICS213_Initial", "ICS Forms")],
            &HashSet::new(),
            &set(&["ICS213_Initial"]),
        );
        let e = &entries[0];
        assert_eq!(e.kind, ImportKind::OverridesStandard);
        assert!(e.reason.as_deref().unwrap().to_lowercase().contains("replaces the standard"));
    }

    #[test]
    fn companions_pass_through_as_companion_kind() {
        let entries = classify(
            vec![comp_cand("Net Check-in Viewer", "AAMRON")],
            &HashSet::new(),
            &HashSet::new(),
        );
        assert_eq!(entries[0].kind, ImportKind::Companion);
    }

    #[test]
    fn summarize_counts_each_kind() {
        let existing_custom = set(&["U Initial"]);
        let bundled = set(&["O Initial"]);
        let entries = classify(
            vec![
                auth_cand("A Initial", "x"),
                auth_cand("U Initial", "x"),
                auth_cand("O Initial", "x"),
                comp_cand("A Viewer", "x"),
            ],
            &existing_custom,
            &bundled,
        );
        let s = summarize(&entries);
        assert_eq!(s.added, 1);
        assert_eq!(s.updated, 1);
        assert_eq!(s.overrides_standard, 1);
        assert_eq!(s.companions, 1);
    }

    // ---- Task 6: registry + preview ----

    fn make_staged_fixture() -> Staged {
        let td = tempfile::tempdir().unwrap();
        let src = td.path().join("f");
        write_bytes(&src.join("X Initial.html"), FORM_HTML);
        stage_sources(&[src.to_string_lossy().into_owned()]).unwrap()
        // `td` drops here; the source is gone but the staged copy persists.
    }

    #[test]
    fn registry_mint_resolve_consume_is_single_shot() {
        let reg = ImportStagingRegistry::default();
        let token = reg.insert(make_staged_fixture());
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
        assert!(path.exists());
        reg.cancel(&token);
        assert!(reg.take(&token).is_none());
        assert!(!path.exists(), "cancel rm -rf'd the staging dir");
    }

    #[test]
    fn registry_reaps_entries_older_than_ttl() {
        let reg = ImportStagingRegistry::default();
        let token = reg.insert_with_age(make_staged_fixture(), 7200);
        reg.reap(3600);
        assert!(reg.take(&token).is_none(), "stale staging reaped");
    }

    #[test]
    fn preview_classifies_without_writing_to_custom_root() {
        let td = tempfile::tempdir().unwrap();
        let custom_root = td.path().join("custom");
        std::fs::create_dir_all(&custom_root).unwrap();
        let bundle_root = td.path().join("bundle");
        std::fs::create_dir_all(&bundle_root).unwrap();
        let src = td.path().join("org");
        write_bytes(
            &src.join("AAMRON/Net Check-in.txt"),
            b"Form: Net Check-in Initial.html, Net Check-in Viewer.html\r\n",
        );
        write_bytes(&src.join("AAMRON/Net Check-in Initial.html"), FORM_HTML);
        write_bytes(&src.join("AAMRON/Net Check-in Viewer.html"), b"<html>v</html>");
        let reg = ImportStagingRegistry::default();
        let plan = preview_sources(
            &[src.to_string_lossy().into_owned()],
            &custom_root,
            &bundle_root,
            &reg,
        )
        .unwrap();
        assert!(plan
            .entries
            .iter()
            .any(|e| e.kind == ImportKind::Added && e.id == "Net Check-in Initial"));
        assert_eq!(plan.staging_token.len(), 16);
        assert_eq!(
            std::fs::read_dir(&custom_root).unwrap().count(),
            0,
            "preview must not write custom_root"
        );
    }
}
