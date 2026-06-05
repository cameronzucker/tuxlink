//! `forms::updater` — runtime refresh of the WLE Standard Forms snapshot.
//!
//! Spec: docs/superpowers/specs/2026-05-31-html-forms-full-parity-design.md
//!       §6 Phase 3 ("forms::updater — winlink.org Standard Forms zip pull,
//!       integrity check, atomic snapshot swap with rollback on bad zip").
//!
//! ## URL + JSON contract
//!
//! Mirrors Pat (`internal/forms/forms.go` — `formsVersionInfoURL`):
//!
//! - `GET https://api.getpat.io/v1/forms/standard-templates/latest`
//!   returns `{"version": "1.0.0", "archive_url": "https://..."}`.
//! - The `archive_url` is a redirected URL to the WLE Standard Forms zip
//!   (typically the Vienna RSGB build).
//!
//! ## Atomic swap pattern
//!
//! Given a runtime root at `<data_dir>/tuxlink/forms/standard/`:
//!
//! 1. Download zip to `<root>/staging/dl-<random>.zip`.
//! 2. Extract to `<root>/staging/<version>/Standard_Forms/...`.
//! 3. Validate the extracted snapshot (must contain `Standard_Forms/` with
//!    at least one `.html` template).
//! 4. If `<root>/active/` exists, rename it to `<root>/.prev-<timestamp>/`.
//! 5. Rename `<root>/staging/<version>/` to `<root>/active/`.
//! 6. Write `<root>/active/VERSION` with the version string.
//! 7. On step 5 failure: rename `.prev-*` back to `active` (rollback).
//! 8. On success: leave `.prev-*` on disk for one cycle (operator can
//!    manually revert if the new snapshot misbehaves). Future invocations
//!    that succeed clean up older `.prev-*` directories. This is the
//!    spec's "rollback on bad zip" — bad extraction fails before step 4
//!    leaves `active/` untouched; bad swap (step 5) rolls back via rename.
//!
//! ## Precedence with the bundled snapshot
//!
//! `wle_templates::bundle_root_for_app` checks the runtime path FIRST.
//! If `<root>/active/Standard_Forms/` exists, that wins; otherwise the
//! function falls back to the resource path (`resources/wle-forms/...`
//! baked into the bundle at build time). A fresh install with no
//! refresh ever performed reads from the bundle. After one successful
//! refresh, all subsequent reads come from the runtime snapshot.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Process-wide install lock. Codex xipa adrev P2 #5: two concurrent
/// `forms_refresh` IPCs can race on `staging/<version>/` (extraction
/// path) and `active/` (atomic-swap path). The mutex serializes the
/// full install body so the second caller waits for the first to
/// finish (or fail) before touching the runtime root. Acceptable
/// trade-off: refresh is operator-triggered, so the second caller
/// just sees a brief delay rather than a corrupted snapshot.
static INSTALL_LOCK: Lazy<tokio::sync::Mutex<()>> =
    Lazy::new(|| tokio::sync::Mutex::new(()));

/// JSON shape returned by `https://api.getpat.io/v1/forms/standard-templates/latest`.
/// Mirrors Pat's `formsInfo` struct exactly so we consume the same endpoint
/// the wider Winlink-client ecosystem already trusts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteFormsInfo {
    pub version: String,
    pub archive_url: String,
}

/// Outcome of a successful `install`. Returned via the `forms_refresh` IPC,
/// so the JSON shape is camelCased to match the rest of the frontend's IPC
/// surface (see `ui_commands::OpenFormResult` for the same convention).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallReport {
    pub installed_version: String,
    pub form_count: usize,
    pub prev_version: Option<String>,
}

#[derive(Debug, Error)]
pub enum UpdaterError {
    #[error("http error fetching metadata: {0}")]
    HttpMetadata(String),
    #[error("metadata json decode failed: {0}")]
    JsonDecode(String),
    #[error("http error downloading archive: {0}")]
    HttpArchive(String),
    #[error("io: {0}")]
    Io(String),
    #[error("zip: {0}")]
    Zip(String),
    #[error("bad archive: {0}")]
    BadArchive(String),
}

/// Default WLE Standard Forms metadata endpoint. Mirrors Pat's
/// `formsVersionInfoURL`. Override via `fetch_latest_info(custom_url)`
/// for testing or proxy setups.
pub const DEFAULT_METADATA_URL: &str =
    "https://api.getpat.io/v1/forms/standard-templates/latest";

/// User-Agent string sent on outbound HTTP. Identifies tuxlink to the
/// metadata service so operators can be reached if a server-side change
/// breaks our consumer. Mirrors the rest of the project's reqwest UA
/// convention.
const HTTP_USER_AGENT: &str = "tuxlink-forms-updater/0.0.1";

/// Per-request timeout for the metadata + download fetches. 60s is
/// generous for the WLE zip (~5 MB on HF satellite links the operator
/// might use offline → bridged). Below that the cap would force false
/// timeouts on legitimate slow networks; above that a stalled connection
/// hangs the UI.
const HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// Maximum size of the WLE forms archive. Defensive cap to prevent a
/// compromised metadata server from pointing us at an arbitrary-size
/// download. Current Standard Forms zip is ~5 MB; 100 MB headroom is
/// generous-but-bounded.
const MAX_ARCHIVE_BYTES: u64 = 100 * 1024 * 1024;

/// Maximum aggregate decompressed size of all extracted ZIP entries.
/// Codex xipa adrev P1 #2: without a decompression cap, a small deflated
/// zip-bomb (~200:1 typical, ~1000:1 worst-case) could expand from a
/// 100 MB download into 20–100 GB of writes, filling the runtime disk
/// before the post-extract validation runs. 300 MB is ~60× the current
/// Standard_Forms expanded size (~5 MB) — plenty of headroom for legit
/// growth, well under "fill the operator's disk."
const MAX_EXTRACT_BYTES: u64 = 300 * 1024 * 1024;

/// File inside the active snapshot recording the installed version. Read
/// by `current_version` to compare against `RemoteFormsInfo.version`.
pub const VERSION_FILENAME: &str = "VERSION";

/// Maximum length of a `version` string used as a path component. 64
/// chars accommodates semver + arbitrary release tags while bounding the
/// blast radius of a malicious metadata response.
const MAX_VERSION_LEN: usize = 64;

/// Validate that a version string is safe to use as a filesystem path
/// component. The `version` value comes from the metadata HTTP response —
/// an external source whose contents tuxlink does NOT control. Without
/// this check, a malicious or compromised metadata server could return
/// `{"version": "../../etc/passwd"}` and cause `install()` to write into
/// arbitrary filesystem locations via `staging.join(version)` and
/// `format!("download-{version}.zip")`. Restrict to `[A-Za-z0-9._-]`
/// (semver + common release-tag characters) and reject empty / oversized.
fn is_safe_version(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= MAX_VERSION_LEN
        // Reject `..` substring outright — `.` is in the per-char whitelist
        // (semver dots), so `..` (parent-traversal) would otherwise slip
        // through. Also reject a leading `.` (covers `.`, `..`, `.hidden`).
        && !v.contains("..")
        && !v.starts_with('.')
        && v.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// Classify a URL for transport-policy decisions. Codex xipa adrev
/// P2 #8: production must use https; tests/dev proxies on loopback are
/// permitted on http because an on-path attacker can't intercept
/// loopback. Returns `Ok((is_loopback,))` on accept, `Err(_)` on reject.
fn classify_transport(url_str: &str) -> Result<bool, String> {
    let parsed = reqwest::Url::parse(url_str).map_err(|e| format!("invalid URL {url_str:?}: {e}"))?;
    let is_loopback = matches!(
        parsed.host_str(),
        Some("127.0.0.1") | Some("localhost") | Some("::1") | Some("[::1]")
    );
    match parsed.scheme() {
        "https" => Ok(is_loopback),
        "http" if is_loopback => Ok(true),
        "http" => Err(format!(
            "refusing plaintext http on non-loopback host: {url_str:?}"
        )),
        other => Err(format!("refusing scheme {other:?}: {url_str:?}")),
    }
}

/// GET the metadata endpoint + decode the JSON response. Pure I/O; no
/// side effects on the local snapshot.
pub async fn fetch_latest_info(metadata_url: &str) -> Result<RemoteFormsInfo, UpdaterError> {
    // Codex xipa adrev P2 #8: require HTTPS on the metadata endpoint
    // (loopback http exempt for tests + local proxies). The metadata
    // response shapes the install entirely (version string + archive_url);
    // a plaintext fetch lets an on-path attacker rewrite either field.
    // Pair with `https_only(true)` so redirects can't downgrade — skipped
    // on loopback so mockito-backed tests work.
    let is_loopback = classify_transport(metadata_url)
        .map_err(UpdaterError::HttpMetadata)?;
    let client = reqwest::Client::builder()
        .user_agent(HTTP_USER_AGENT)
        .timeout(HTTP_TIMEOUT)
        .https_only(!is_loopback)
        .build()
        .map_err(|e| UpdaterError::HttpMetadata(format!("client build: {e}")))?;
    let resp = client
        .get(metadata_url)
        .send()
        .await
        .map_err(|e| UpdaterError::HttpMetadata(format!("send: {e}")))?;
    if !resp.status().is_success() {
        return Err(UpdaterError::HttpMetadata(format!(
            "non-success status: {}",
            resp.status()
        )));
    }
    let body = resp
        .text()
        .await
        .map_err(|e| UpdaterError::HttpMetadata(format!("read body: {e}")))?;
    let info: RemoteFormsInfo = serde_json::from_str(&body)
        .map_err(|e| UpdaterError::JsonDecode(format!("{e}: body={body}")))?;
    Ok(info)
}

/// Read the active snapshot's version file. Returns None when no refresh
/// has ever populated the runtime root (caller should fall back to the
/// bundled resource path's version, which is hard-coded at build).
pub fn current_version(runtime_root: &Path) -> Option<String> {
    let path = runtime_root.join("active").join(VERSION_FILENAME);
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

/// Returns true iff the runtime root has an active snapshot with at least
/// one HTML template under `active/Standard_Forms/`. Used by
/// `wle_templates::bundle_root_for_app` to decide between runtime and
/// bundle precedence.
pub fn runtime_snapshot_present(runtime_root: &Path) -> bool {
    let active = runtime_root.join("active").join("Standard_Forms");
    if !active.is_dir() {
        return false;
    }
    walkdir::WalkDir::new(&active)
        .into_iter()
        .filter_map(|e| e.ok())
        .any(|e| {
            e.file_type().is_file()
                && e.path()
                    .extension()
                    .and_then(|x| x.to_str())
                    .map(|x| x.eq_ignore_ascii_case("html"))
                    .unwrap_or(false)
        })
}

/// Download `archive_url` to a temporary file inside `<runtime_root>/staging/`,
/// extract, validate, and atomically swap into `<runtime_root>/active/`. On
/// any step failure before the swap, the existing `active/` is untouched.
/// On swap failure, the prior `active/` is restored via the `.prev-*`
/// rename.
pub async fn install(
    archive_url: &str,
    version: &str,
    runtime_root: &Path,
) -> Result<InstallReport, UpdaterError> {
    // Codex xipa adrev P2 #5: serialize concurrent installs so two
    // simultaneous `forms_refresh` IPCs can't race on staging dirs or
    // the active/ swap. Held for the entire install body — released on
    // every return path (including error).
    let _install_guard = INSTALL_LOCK.lock().await;

    // Defense against a malicious or compromised metadata server: `version`
    // is used as a path component (staging/<version>/) and inside a
    // filename (download-<version>.zip). A response like `"../../etc/passwd"`
    // would otherwise let install() write outside the runtime root.
    if !is_safe_version(version) {
        return Err(UpdaterError::BadArchive(format!(
            "unsafe version string (rejected: must be [A-Za-z0-9._-]{{1,{MAX_VERSION_LEN}}}): {version:?}"
        )));
    }
    let prev_version = current_version(runtime_root);
    let staging = runtime_root.join("staging");
    std::fs::create_dir_all(&staging).map_err(|e| UpdaterError::Io(format!("mkdir staging: {e}")))?;

    // 1. Download to staging/<version>.zip (versioned filename eases
    //    debugging mid-install if extraction fails).
    let dl_path = staging.join(format!("download-{version}.zip"));
    download_archive(archive_url, &dl_path).await?;

    // 2. Extract to staging/<version>/. The zip's expected top-level entry
    //    is "Standard_Forms/" per WLE's archive convention; if a future
    //    zip ships content at the root, we wrap it under Standard_Forms/
    //    during extraction (see `extract_zip`).
    let extract_dest = staging.join(version);
    if extract_dest.exists() {
        std::fs::remove_dir_all(&extract_dest)
            .map_err(|e| UpdaterError::Io(format!("clear stale staging/{version}/: {e}")))?;
    }
    let form_count = extract_zip(&dl_path, &extract_dest)?;
    let _ = std::fs::remove_file(&dl_path); // best-effort cleanup

    // 3. Validate — must have Standard_Forms/ with at least one HTML.
    let std_forms_dir = extract_dest.join("Standard_Forms");
    if !std_forms_dir.is_dir() {
        return Err(UpdaterError::BadArchive(
            "extracted archive missing Standard_Forms/ directory".into(),
        ));
    }
    if form_count == 0 {
        return Err(UpdaterError::BadArchive(
            "extracted archive contains no HTML templates".into(),
        ));
    }

    // 4. Save the old active snapshot away (if any), then 5. promote
    //    staging → active. The rename pair is non-atomic across two
    //    operations, but the rollback at the end restores the prior
    //    state if step 5 fails.
    let active = runtime_root.join("active");
    let ts = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let backup = runtime_root.join(format!(".prev-{ts}"));
    let had_active = active.exists();
    if had_active {
        std::fs::rename(&active, &backup)
            .map_err(|e| UpdaterError::Io(format!("rename active → backup: {e}")))?;
    }
    if let Err(e) = std::fs::rename(&extract_dest, &active) {
        // Rollback: restore the prior active snapshot.
        if had_active {
            let _ = std::fs::rename(&backup, &active);
        }
        return Err(UpdaterError::Io(format!(
            "rename staging → active (rolled back): {e}"
        )));
    }

    // 6. Write VERSION file.
    std::fs::write(active.join(VERSION_FILENAME), version)
        .map_err(|e| UpdaterError::Io(format!("write VERSION: {e}")))?;

    // 8. Clean up older .prev-* directories. Keep ONE generation behind
    //    so the operator has a manual escape hatch ("the new snapshot
    //    broke X, restore active from .prev-<latest>"). Older ones go.
    cleanup_old_backups(runtime_root, &backup);

    Ok(InstallReport {
        installed_version: version.to_string(),
        form_count,
        prev_version,
    })
}

async fn download_archive(url: &str, dest: &Path) -> Result<(), UpdaterError> {
    // Codex xipa adrev P2 #8: archive_url is trusted-from-metadata, but
    // metadata could return an http:// URL or a 30x redirect from https
    // to http. Require https on the initial URL + ban downgrade-to-http
    // redirects so an on-path attacker can't substitute the archive.
    // Loopback http exempt (test + local-proxy use).
    let is_loopback = classify_transport(url).map_err(UpdaterError::HttpArchive)?;
    let client = reqwest::Client::builder()
        .user_agent(HTTP_USER_AGENT)
        .timeout(HTTP_TIMEOUT)
        .https_only(!is_loopback)
        .build()
        .map_err(|e| UpdaterError::HttpArchive(format!("client build: {e}")))?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| UpdaterError::HttpArchive(format!("send: {e}")))?;
    if !resp.status().is_success() {
        return Err(UpdaterError::HttpArchive(format!(
            "non-success status: {}",
            resp.status()
        )));
    }
    if let Some(len) = resp.content_length() {
        if len > MAX_ARCHIVE_BYTES {
            return Err(UpdaterError::BadArchive(format!(
                "archive too large: Content-Length {len} > cap {MAX_ARCHIVE_BYTES}"
            )));
        }
    }

    // Codex xipa adrev P1 #1: a hostile server can OMIT Content-Length and
    // stream past the cap. Stream chunks to disk, check the running total
    // every chunk; abort the moment we exceed MAX_ARCHIVE_BYTES so we
    // bound peak memory + disk usage even when the server lies.
    let mut total: u64 = 0;
    let mut file = std::fs::File::create(dest)
        .map_err(|e| UpdaterError::Io(format!("create {dest:?}: {e}")))?;
    use futures::StreamExt;
    let mut stream = resp.bytes_stream();
    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res
            .map_err(|e| UpdaterError::HttpArchive(format!("read chunk: {e}")))?;
        total = total.saturating_add(chunk.len() as u64);
        if total > MAX_ARCHIVE_BYTES {
            // Drop the partial file so we don't leave attacker-controlled
            // bytes behind on the runtime filesystem.
            let _ = std::fs::remove_file(dest);
            return Err(UpdaterError::BadArchive(format!(
                "archive too large mid-stream: {total} bytes > cap {MAX_ARCHIVE_BYTES}"
            )));
        }
        use std::io::Write;
        file.write_all(&chunk)
            .map_err(|e| UpdaterError::Io(format!("write chunk to {dest:?}: {e}")))?;
    }
    Ok(())
}

/// Extract `zip_path` into `dest_dir`. Returns the count of `.html` files
/// extracted (used for the InstallReport's `form_count` + the post-extract
/// validation that the archive isn't empty).
///
/// If the zip's top-level entries are NOT under `Standard_Forms/`, the
/// function wraps everything under `dest_dir/Standard_Forms/`. WLE's
/// current zip ships with `Standard_Forms/` already at the root; this
/// wrap is defensive against a future structural change.
///
/// Path traversal: each entry's destination is computed relative to
/// `dest_dir` and rejected if it escapes (matching the same defense in
/// `http_server::folder_handler`).
fn extract_zip(zip_path: &Path, dest_dir: &Path) -> Result<usize, UpdaterError> {
    let file = std::fs::File::open(zip_path)
        .map_err(|e| UpdaterError::Io(format!("open zip {zip_path:?}: {e}")))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| UpdaterError::Zip(format!("open archive: {e}")))?;

    // Probe the first entry's path to decide whether to wrap under
    // Standard_Forms/. WLE's current zip starts entries with that prefix;
    // a future zip without it gets wrapped.
    let needs_wrap = if archive.is_empty() {
        false
    } else {
        let first = archive
            .by_index(0)
            .map_err(|e| UpdaterError::Zip(format!("read first entry: {e}")))?;
        !first.name().starts_with("Standard_Forms/")
    };

    std::fs::create_dir_all(dest_dir)
        .map_err(|e| UpdaterError::Io(format!("mkdir dest: {e}")))?;
    let canonical_dest = dest_dir
        .canonicalize()
        .map_err(|e| UpdaterError::Io(format!("canonicalize dest: {e}")))?;

    let mut html_count = 0;
    let mut total_decompressed: u64 = 0;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| UpdaterError::Zip(format!("entry {i}: {e}")))?;
        let entry_path = match entry.enclosed_name() {
            Some(p) => p.to_path_buf(),
            None => {
                return Err(UpdaterError::Zip(format!(
                    "entry {i} has an invalid path (likely path traversal): {}",
                    entry.name()
                )))
            }
        };

        let dest_path = if needs_wrap {
            canonical_dest.join("Standard_Forms").join(entry_path.clone())
        } else {
            canonical_dest.join(entry_path.clone())
        };

        // Defense in depth: even though enclosed_name() rejects ".." paths,
        // verify the canonical destination is under dest_dir.
        if !dest_path.starts_with(&canonical_dest) {
            return Err(UpdaterError::Zip(format!(
                "entry {i} escapes dest dir: {}",
                entry.name()
            )));
        }

        if entry.is_dir() {
            std::fs::create_dir_all(&dest_path)
                .map_err(|e| UpdaterError::Io(format!("mkdir {dest_path:?}: {e}")))?;
            continue;
        }

        // Codex xipa adrev P1 #2: zip-bomb defense. Cap per-entry size
        // by the remaining aggregate budget — `entry.size()` reports the
        // declared uncompressed size from the central directory; we
        // still need to enforce the budget during the copy because zip
        // entries can lie about their size (or report 0 when "unknown").
        let declared = entry.size();
        let remaining = MAX_EXTRACT_BYTES.saturating_sub(total_decompressed);
        if declared > remaining {
            return Err(UpdaterError::BadArchive(format!(
                "extracted size cap exceeded (declared {} for {:?} + already-extracted {} > cap {})",
                declared, entry_path, total_decompressed, MAX_EXTRACT_BYTES
            )));
        }

        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| UpdaterError::Io(format!("mkdir parent {parent:?}: {e}")))?;
        }
        let mut out = std::fs::File::create(&dest_path)
            .map_err(|e| UpdaterError::Io(format!("create {dest_path:?}: {e}")))?;
        // Copy at most `remaining` bytes; if the entry actually produces
        // more (a deflated zip-bomb that lies in its central-directory
        // size), the take-limited copy returns short + we error out.
        let mut limited = std::io::Read::take(&mut entry, remaining);
        let written = std::io::copy(&mut limited, &mut out)
            .map_err(|e| UpdaterError::Io(format!("copy entry to {dest_path:?}: {e}")))?;
        total_decompressed = total_decompressed.saturating_add(written);
        // If we filled the budget AND the source still has more bytes,
        // the entry is over the cap. Probe by reading one more byte.
        if written == remaining {
            let mut peek = [0u8; 1];
            use std::io::Read;
            if entry.read(&mut peek).map(|n| n > 0).unwrap_or(false) {
                // Drop the partial file before erroring.
                let _ = std::fs::remove_file(&dest_path);
                return Err(UpdaterError::BadArchive(format!(
                    "extracted size cap exceeded mid-copy on entry {:?} (cap {})",
                    entry_path, MAX_EXTRACT_BYTES
                )));
            }
        }
        // Codex xipa adrev P2 #4: only HTML files that land UNDER the
        // Standard_Forms/ subtree count toward form_count, since that's
        // the directory `runtime_snapshot_present` actually consults. A
        // malicious zip with a root-level README.html plus a junk
        // Standard_Forms/notes.txt could otherwise pass `form_count > 0`
        // even though no usable template ends up where the catalog reads.
        let standard_forms_root = canonical_dest.join("Standard_Forms");
        if dest_path.starts_with(&standard_forms_root)
            && dest_path
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| x.eq_ignore_ascii_case("html"))
                .unwrap_or(false)
        {
            html_count += 1;
        }
    }

    Ok(html_count)
}

/// Remove all `.prev-*` directories EXCEPT the one passed (current backup,
/// kept for one cycle as a manual rollback escape hatch).
fn cleanup_old_backups(runtime_root: &Path, keep: &Path) {
    let Ok(entries) = std::fs::read_dir(runtime_root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path == keep {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with(".prev-") {
                let _ = std::fs::remove_dir_all(&path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;
    use zip::write::SimpleFileOptions;

    /// Build a minimal in-memory zip with the given (relative-path, contents)
    /// pairs. Used as the standard test fixture for install + extract paths.
    fn make_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            for (path, contents) in entries {
                zip.start_file(*path, options).unwrap();
                zip.write_all(contents).unwrap();
            }
            zip.finish().unwrap();
        }
        buf
    }

    fn write_zip_to(dir: &Path, entries: &[(&str, &[u8])]) -> PathBuf {
        let path = dir.join("test.zip");
        std::fs::write(&path, make_zip(entries)).unwrap();
        path
    }

    #[test]
    fn extract_zip_writes_files_under_dest_and_counts_html() {
        let td = TempDir::new().unwrap();
        let zip_path = write_zip_to(
            td.path(),
            &[
                ("Standard_Forms/ICS Forms/ICS213_Initial.html", b"<html>1</html>"),
                ("Standard_Forms/ICS Forms/ICS213_Viewer.html", b"<html>2</html>"),
                ("Standard_Forms/General/Bulletin Initial.html", b"<html>3</html>"),
                ("Standard_Forms/Changelog.txt", b"v1"),
            ],
        );
        let dest = td.path().join("extracted");
        let count = extract_zip(&zip_path, &dest).unwrap();
        assert_eq!(count, 3, "should count three .html entries");
        assert!(dest.join("Standard_Forms/ICS Forms/ICS213_Initial.html").is_file());
        assert!(dest.join("Standard_Forms/Changelog.txt").is_file());
    }

    #[test]
    fn extract_zip_wraps_unwrapped_archives_under_standard_forms() {
        let td = TempDir::new().unwrap();
        let zip_path = write_zip_to(
            td.path(),
            &[
                ("ICS213_Initial.html", b"<html>1</html>"),
                ("Bulletin_Initial.html", b"<html>2</html>"),
            ],
        );
        let dest = td.path().join("extracted");
        let count = extract_zip(&zip_path, &dest).unwrap();
        assert_eq!(count, 2);
        // Defensive wrap: since the zip didn't start with Standard_Forms/,
        // the extractor puts everything under it.
        assert!(dest.join("Standard_Forms/ICS213_Initial.html").is_file());
        assert!(dest.join("Standard_Forms/Bulletin_Initial.html").is_file());
    }

    #[test]
    fn extract_zip_rejects_path_traversal() {
        let td = TempDir::new().unwrap();
        // zip's enclosed_name() rejects entries with .. or absolute paths.
        // Constructing a malicious entry directly via zip-rs to verify the
        // defense triggers.
        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            zip.start_file("../escape.txt", options).unwrap();
            zip.write_all(b"pwned").unwrap();
            zip.finish().unwrap();
        }
        let zip_path = td.path().join("bad.zip");
        std::fs::write(&zip_path, &buf).unwrap();
        let dest = td.path().join("extracted");
        let err = extract_zip(&zip_path, &dest).unwrap_err();
        match err {
            UpdaterError::Zip(msg) => {
                assert!(
                    msg.contains("path traversal") || msg.contains("invalid path"),
                    "unexpected zip error: {msg}"
                );
            }
            other => panic!("expected Zip error, got {other:?}"),
        }
        // And NO file got written outside dest.
        assert!(!td.path().join("escape.txt").exists());
    }

    #[test]
    fn install_writes_active_with_version_file() {
        let td = TempDir::new().unwrap();
        let runtime = td.path().to_path_buf();
        // Prepare a mock archive locally + serve via a temp file URL.
        // For unit test we bypass HTTP by calling extract_zip directly via
        // a tested helper path; the install() function's HTTP is exercised
        // in the install_works_end_to_end_with_mock_server test below.
        let zip_path = write_zip_to(
            td.path(),
            &[("Standard_Forms/ICS213_Initial.html", b"<html>1</html>")],
        );
        // Place the zip where install would put it after download, then
        // call into the post-download code path by extracting + running
        // the swap manually. (install() doesn't expose its phases; for the
        // pure swap-with-no-prior-active path we test through install
        // proper using the mock server below.)
        let staging = runtime.join("staging");
        std::fs::create_dir_all(&staging).unwrap();
        let extract_dest = staging.join("1.0.0");
        let count = extract_zip(&zip_path, &extract_dest).unwrap();
        assert_eq!(count, 1);
        // Manual swap (mirrors install()'s steps 4-6).
        let active = runtime.join("active");
        std::fs::rename(&extract_dest, &active).unwrap();
        std::fs::write(active.join(VERSION_FILENAME), "1.0.0").unwrap();

        assert_eq!(current_version(&runtime), Some("1.0.0".to_string()));
        assert!(runtime_snapshot_present(&runtime));
    }

    #[test]
    fn current_version_returns_none_when_no_active() {
        let td = TempDir::new().unwrap();
        assert!(current_version(td.path()).is_none());
    }

    #[test]
    fn runtime_snapshot_present_false_for_empty_dir() {
        let td = TempDir::new().unwrap();
        assert!(!runtime_snapshot_present(td.path()));
    }

    #[test]
    fn runtime_snapshot_present_false_when_active_has_no_html() {
        let td = TempDir::new().unwrap();
        std::fs::create_dir_all(td.path().join("active/Standard_Forms")).unwrap();
        std::fs::write(td.path().join("active/Standard_Forms/notes.txt"), "hi").unwrap();
        assert!(!runtime_snapshot_present(td.path()));
    }

    #[test]
    fn runtime_snapshot_present_true_with_one_html() {
        let td = TempDir::new().unwrap();
        std::fs::create_dir_all(td.path().join("active/Standard_Forms/General Forms")).unwrap();
        std::fs::write(
            td.path().join("active/Standard_Forms/General Forms/Test.html"),
            "<html></html>",
        )
        .unwrap();
        assert!(runtime_snapshot_present(td.path()));
    }

    #[tokio::test]
    async fn fetch_latest_info_parses_pat_json() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/latest")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"version":"1.2.3","archive_url":"https://example.com/forms.zip"}"#)
            .create_async()
            .await;
        let url = format!("{}/latest", server.url());
        let info = fetch_latest_info(&url).await.unwrap();
        mock.assert_async().await;
        assert_eq!(info.version, "1.2.3");
        assert_eq!(info.archive_url, "https://example.com/forms.zip");
    }

    #[tokio::test]
    async fn fetch_latest_info_errors_on_non_success() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/latest")
            .with_status(503)
            .with_body("maintenance")
            .create_async()
            .await;
        let url = format!("{}/latest", server.url());
        let err = fetch_latest_info(&url).await.unwrap_err();
        match err {
            UpdaterError::HttpMetadata(msg) => assert!(msg.contains("503")),
            other => panic!("expected HttpMetadata, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn fetch_latest_info_errors_on_garbage_body() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/latest")
            .with_status(200)
            .with_body("not json")
            .create_async()
            .await;
        let url = format!("{}/latest", server.url());
        let err = fetch_latest_info(&url).await.unwrap_err();
        match err {
            UpdaterError::JsonDecode(_) => {}
            other => panic!("expected JsonDecode, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn install_end_to_end_with_mock_archive() {
        let td = TempDir::new().unwrap();
        let runtime = td.path().to_path_buf();
        let zip_bytes = make_zip(&[
            ("Standard_Forms/ICS Forms/ICS213_Initial.html", b"<html>i</html>"),
            ("Standard_Forms/ICS Forms/ICS213_Viewer.html", b"<html>v</html>"),
            ("Standard_Forms/General Forms/Bulletin Initial.html", b"<html>b</html>"),
        ]);
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/forms.zip")
            .with_status(200)
            .with_header("content-type", "application/zip")
            .with_body(zip_bytes.clone())
            .create_async()
            .await;
        let url = format!("{}/forms.zip", server.url());
        let report = install(&url, "1.0.0", &runtime).await.unwrap();
        mock.assert_async().await;
        assert_eq!(report.installed_version, "1.0.0");
        assert_eq!(report.form_count, 3);
        assert_eq!(report.prev_version, None);
        assert_eq!(current_version(&runtime).as_deref(), Some("1.0.0"));
        assert!(runtime
            .join("active/Standard_Forms/ICS Forms/ICS213_Initial.html")
            .is_file());
    }

    #[tokio::test]
    async fn install_replaces_prior_active_and_reports_prev_version() {
        let td = TempDir::new().unwrap();
        let runtime = td.path().to_path_buf();
        // Seed a prior active snapshot.
        let prior = runtime.join("active/Standard_Forms/Old");
        std::fs::create_dir_all(&prior).unwrap();
        std::fs::write(prior.join("Old.html"), "<html>old</html>").unwrap();
        std::fs::write(runtime.join("active").join(VERSION_FILENAME), "0.9.0").unwrap();

        let zip_bytes = make_zip(&[(
            "Standard_Forms/New/New.html",
            b"<html>new</html>",
        )]);
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/forms.zip")
            .with_status(200)
            .with_body(zip_bytes)
            .create_async()
            .await;
        let url = format!("{}/forms.zip", server.url());
        let report = install(&url, "1.0.0", &runtime).await.unwrap();
        assert_eq!(report.prev_version.as_deref(), Some("0.9.0"));
        assert_eq!(report.installed_version, "1.0.0");
        assert_eq!(current_version(&runtime).as_deref(), Some("1.0.0"));
        // New snapshot in place.
        assert!(runtime.join("active/Standard_Forms/New/New.html").is_file());
        // Old snapshot gone from active/, preserved in .prev-*.
        assert!(!runtime.join("active/Standard_Forms/Old").exists());
        let prev_dirs: Vec<_> = std::fs::read_dir(&runtime)
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .map(|n| n.starts_with(".prev-"))
                    .unwrap_or(false)
            })
            .collect();
        assert_eq!(prev_dirs.len(), 1, "exactly one .prev-* should be kept");
    }

    #[tokio::test]
    async fn install_rejects_archive_with_no_standard_forms_dir() {
        // A zip whose first entry IS at the Standard_Forms/ prefix, so the
        // wrap-defense doesn't kick in, but the directory itself is absent
        // because all entries are siblings of it. Build with first-entry =
        // "Standard_Forms/notes.txt" (a file at the right prefix) then…
        // actually simpler: an empty archive (no entries) trips the
        // "no HTML templates" check.
        let zip_bytes = make_zip(&[]);
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/empty.zip")
            .with_status(200)
            .with_body(zip_bytes)
            .create_async()
            .await;
        let td = TempDir::new().unwrap();
        let url = format!("{}/empty.zip", server.url());
        let err = install(&url, "1.0.0", td.path()).await.unwrap_err();
        match err {
            UpdaterError::BadArchive(msg) => {
                assert!(
                    msg.contains("Standard_Forms") || msg.contains("no HTML templates"),
                    "unexpected msg: {msg}"
                );
            }
            other => panic!("expected BadArchive, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn install_rejects_empty_version_string() {
        let td = TempDir::new().unwrap();
        let err = install("https://example.com/nope.zip", "", td.path())
            .await
            .unwrap_err();
        match err {
            UpdaterError::BadArchive(msg) => assert!(msg.contains("version")),
            other => panic!("expected BadArchive, got {other:?}"),
        }
    }

    /// Path-traversal defense: the `version` string is untrusted (comes
    /// from the metadata HTTP response). Reject any value that contains
    /// path separators, parent-directory traversal, NUL bytes, or anything
    /// outside the `[A-Za-z0-9._-]` whitelist. Without this guard, a
    /// malicious metadata server could write into arbitrary filesystem
    /// locations via `staging.join(version)`.
    #[tokio::test]
    async fn install_rejects_path_traversal_in_version() {
        let td = TempDir::new().unwrap();
        let cases = [
            "../../etc/passwd",
            "..",
            "/absolute/path",
            "v1.0.0/../escape",
            "v1.0.0\0nul",
            "with space",
            "with;semi",
            "1.0|pipe",
            "back\\slash",
        ];
        for bad in cases {
            let err = install("https://example.com/nope.zip", bad, td.path())
                .await
                .unwrap_err();
            match err {
                UpdaterError::BadArchive(msg) => {
                    assert!(
                        msg.contains("unsafe version"),
                        "version {bad:?} should trip is_safe_version; got: {msg}"
                    );
                }
                other => panic!("version {bad:?}: expected BadArchive, got {other:?}"),
            }
        }
        // Sanity: the regex DOES accept legitimate version strings.
        assert!(is_safe_version("1.0.0"));
        assert!(is_safe_version("2.3.4-rc.1"));
        assert!(is_safe_version("v5_alpha"));
        // Length cap.
        let oversize = "a".repeat(MAX_VERSION_LEN + 1);
        assert!(!is_safe_version(&oversize));
    }

    // ========================================================================
    // Codex xipa adrev — P2 #8: HTTPS-only with loopback exemption.
    // ========================================================================

    #[test]
    fn classify_transport_accepts_https_anywhere() {
        assert_eq!(classify_transport("https://api.example.com/x").unwrap(), false);
        assert_eq!(classify_transport("https://127.0.0.1/x").unwrap(), true);
    }

    #[test]
    fn classify_transport_accepts_http_loopback_only() {
        assert_eq!(classify_transport("http://127.0.0.1/x").unwrap(), true);
        assert_eq!(classify_transport("http://localhost:8080/x").unwrap(), true);
    }

    #[test]
    fn classify_transport_rejects_http_non_loopback() {
        let err = classify_transport("http://api.example.com/x").unwrap_err();
        assert!(err.contains("plaintext http") && err.contains("non-loopback"), "got: {err}");
    }

    #[test]
    fn classify_transport_rejects_other_schemes() {
        let err = classify_transport("file:///etc/passwd").unwrap_err();
        assert!(err.contains("refusing scheme"), "got: {err}");
        let err = classify_transport("ftp://example.com/x").unwrap_err();
        assert!(err.contains("refusing scheme"), "got: {err}");
    }

    #[test]
    fn classify_transport_rejects_loopback_lookalike_hosts() {
        // Defense: ensure a host like `127.0.0.1.evil.com` doesn't slip
        // through. The classifier should reject because the host is NOT
        // literally `127.0.0.1`.
        let err = classify_transport("http://127.0.0.1.evil.com/x").unwrap_err();
        assert!(err.contains("plaintext http"), "got: {err}");
        let err = classify_transport("http://localhost.evil.com/x").unwrap_err();
        assert!(err.contains("plaintext http"), "got: {err}");
    }

    #[tokio::test]
    async fn fetch_latest_info_rejects_plain_http_non_loopback() {
        let err = fetch_latest_info("http://example.com/forms/latest").await.unwrap_err();
        match err {
            UpdaterError::HttpMetadata(msg) => {
                assert!(msg.contains("plaintext http"), "got: {msg}");
            }
            other => panic!("expected HttpMetadata, got {other:?}"),
        }
    }

    // ========================================================================
    // Codex xipa adrev — P1 #2: zip-bomb defense.
    // ========================================================================

    #[test]
    fn extract_zip_rejects_oversized_archive() {
        // Build a zip whose declared-size on a single entry exceeds the
        // MAX_EXTRACT_BYTES cap. zip-rs's declared_size comes from the
        // central directory, so this is the cheap-to-construct path.
        let td = TempDir::new().unwrap();
        // Single entry, 301 MiB of zeros — over the 300 MiB cap.
        // Use Stored (no compression) so the central-directory size matches.
        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            zip.start_file("Standard_Forms/giant.html", options).unwrap();
            let payload = vec![0u8; (MAX_EXTRACT_BYTES + 1024) as usize];
            zip.write_all(&payload).unwrap();
            zip.finish().unwrap();
        }
        let zip_path = td.path().join("bomb.zip");
        std::fs::write(&zip_path, &buf).unwrap();
        let dest = td.path().join("extracted");
        let err = extract_zip(&zip_path, &dest).unwrap_err();
        match err {
            UpdaterError::BadArchive(msg) => {
                assert!(
                    msg.contains("extracted size cap"),
                    "expected size-cap error, got: {msg}"
                );
            }
            other => panic!("expected BadArchive, got {other:?}"),
        }
    }

    // ========================================================================
    // Codex xipa adrev — P2 #4: HTML-count under Standard_Forms only.
    // ========================================================================

    #[test]
    fn extract_zip_only_counts_html_under_standard_forms_subtree() {
        // A malicious archive: legit-looking Standard_Forms/ with a
        // non-html "form", plus a root-level README.html that would
        // otherwise satisfy a naive "any HTML" count.
        let td = TempDir::new().unwrap();
        let zip_path = write_zip_to(
            td.path(),
            &[
                ("Standard_Forms/General/notes.txt", b"not a form"),
                ("README.html", b"<html>root readme</html>"),
            ],
        );
        let dest = td.path().join("extracted");
        let count = extract_zip(&zip_path, &dest).unwrap();
        // README.html is OUTSIDE Standard_Forms/, so it doesn't count.
        // notes.txt isn't html. → 0.
        assert_eq!(
            count, 0,
            "form_count must reject HTML files outside Standard_Forms/"
        );
        // But the install-level validation (form_count == 0) will then
        // reject this archive.
        assert!(dest.join("README.html").exists(), "extraction still happens");
    }
}
