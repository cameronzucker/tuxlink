//! Region-pack download + validate + atomic install + orphan sweep
//! (tuxlink-ndi4, phase 4, plan A4/R5).
//!
//! Design: docs/design/2026-06-13-ndi4-d1-region-pack-distribution.md
//!
//! This is the R5 critical path: "the one place a silent failure could persist
//! corrupt state". The install sequence is **temp → validate → atomic rename →
//! parent-dir fsync → manifest write (AFTER the rename)** so a crash at any step
//! never leaves a half-written archive registered or read. The actual byte
//! transfer is done by the go-pmtiles sidecar (it Range-reads the planet); that
//! one runtime dependency is abstracted behind [`Extractor`] so the whole
//! orchestration — free-space gate, validation reject, atomic install, manifest
//! write, orphan sweep — is unit-testable without spawning a process.
//!
//! Mirrors the established `tiles::cache` atomic temp+rename and
//! `forms::import::sweep_stale_staging` patterns (plan A4).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use super::packs::{is_safe_pack_id, Bbox, InstalledPack, PacksManifest};
use super::validate::{self, ValidationError};
use super::PmtilesArchive;

/// File extension for installed packs and the in-progress temp files.
const PACK_EXT: &str = "pmtiles";
const TMP_SUFFIX: &str = ".part";

/// Abstraction over `pmtiles extract <planet_url> <dest> --bbox=… --maxzoom=…`.
/// The runtime impl spawns the bundled go-pmtiles sidecar; tests use a fake that
/// writes known bytes, so the install orchestration is exercised without a process.
pub trait Extractor {
    /// Extract `bbox` (z0..=`maxzoom`) from `planet_url` into `dest`. Blocking.
    /// `planet_url` has already passed [`super::region_manifest::validate_planet_url`].
    ///
    /// `cancel` is polled while the transfer runs; when set the extractor stops
    /// ASAP (killing the sidecar child) and returns [`DownloadError::Cancelled`].
    /// `on_progress(bytes_written_so_far)` is invoked as bytes accumulate so the
    /// command layer can emit a progress event. Both are wired through from
    /// [`install_pack`]; tests may ignore them.
    fn extract(
        &self,
        planet_url: &str,
        bbox: &Bbox,
        maxzoom: u8,
        dest: &Path,
        cancel: &Arc<AtomicBool>,
        on_progress: &dyn Fn(u64),
    ) -> Result<(), DownloadError>;
}

/// A validated request to install one pack.
#[derive(Debug, Clone)]
pub struct PackRequest {
    /// Filesystem-safe id (asserted again here defensively).
    pub id: String,
    pub label: String,
    /// Already-allowlisted https planet URL.
    pub planet_url: String,
    pub bbox: Bbox,
    pub maxzoom: u8,
    pub source_build: String,
    /// The manifest's `typical_bytes` size estimate for this tier/continent — the
    /// progress bar's denominator (`total`). Distinct from `needed_bytes` (which
    /// carries free-space headroom) so the UI shows the honest expected size.
    pub typical_bytes: u64,
    /// Reject the download up front unless this many bytes are free.
    pub needed_bytes: u64,
    /// Reject the *downloaded* archive if it exceeds this (validate.rs size budget).
    pub size_budget: u64,
    /// RFC3339 UTC timestamp recorded on the installed entry (caller supplies the
    /// clock, keeping install deterministic/testable).
    pub installed_at: String,
}

/// Why an install failed. No variant leaves state behind — the temp file is
/// always cleaned up before returning an error.
#[derive(Debug)]
pub enum DownloadError {
    /// Pack id was not `[a-z0-9-]+` (defence in depth; the id is derived safely).
    UnsafeId(String),
    /// Pre-flight free-space check failed.
    InsufficientSpace { needed: u64, available: u64 },
    /// The go-pmtiles sidecar failed / was killed.
    ExtractFailed(String),
    /// The operator cancelled the download mid-extract. Like every other error
    /// variant, the temp `.part` is cleaned up so no partial pack persists.
    Cancelled,
    /// The downloaded archive failed PMTiles/schema/size validation.
    Validation(ValidationError),
    /// A filesystem step (temp create, rename, fsync, manifest write) failed.
    Io(String),
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadError::UnsafeId(id) => write!(f, "unsafe pack id {id:?}"),
            DownloadError::InsufficientSpace { needed, available } => write!(
                f,
                "not enough free space: need ~{needed} bytes, {available} available"
            ),
            DownloadError::ExtractFailed(e) => write!(f, "map extraction failed: {e}"),
            DownloadError::Cancelled => write!(f, "download cancelled"),
            DownloadError::Validation(e) => write!(f, "downloaded pack is invalid: {e}"),
            DownloadError::Io(e) => write!(f, "pack install I/O error: {e}"),
        }
    }
}

impl std::error::Error for DownloadError {}

impl From<ValidationError> for DownloadError {
    fn from(e: ValidationError) -> Self {
        DownloadError::Validation(e)
    }
}

/// Final installed path for a pack id within `packs_dir`.
pub fn pack_path(packs_dir: &Path, id: &str) -> PathBuf {
    packs_dir.join(format!("{id}.{PACK_EXT}"))
}

/// Path to the app-data packs manifest.
pub fn manifest_path(packs_dir: &Path) -> PathBuf {
    packs_dir.join("manifest.json")
}

/// Load the packs manifest, returning an empty one if absent or unreadable
/// (a corrupt manifest must not block the whole pack subsystem; the orphan sweep
/// + re-register on startup rebuilds usable state).
pub fn load_manifest(packs_dir: &Path) -> PacksManifest {
    let path = manifest_path(packs_dir);
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Atomically write the packs manifest (same-dir temp + fsync + rename), mirroring
/// `tiles::cache` / `config::write_config_atomic`.
fn write_manifest_atomic(packs_dir: &Path, manifest: &PacksManifest) -> Result<(), DownloadError> {
    let json = serde_json::to_vec_pretty(manifest).map_err(|e| DownloadError::Io(e.to_string()))?;
    let tmp = tempfile::Builder::new()
        .prefix(".manifest-")
        .suffix(".json.part")
        .tempfile_in(packs_dir)
        .map_err(|e| DownloadError::Io(e.to_string()))?;
    {
        use std::io::Write;
        let mut f = tmp.as_file();
        f.write_all(&json).map_err(|e| DownloadError::Io(e.to_string()))?;
        f.sync_all().map_err(|e| DownloadError::Io(e.to_string()))?;
    }
    tmp.persist(manifest_path(packs_dir))
        .map_err(|e| DownloadError::Io(e.error.to_string()))?;
    fsync_dir(packs_dir);
    Ok(())
}

/// Best-effort fsync of a directory so a rename is durable. A failure here is not
/// fatal (the rename already happened); logged by the caller in the runtime layer.
fn fsync_dir(dir: &Path) {
    if let Ok(d) = fs::File::open(dir) {
        let _ = d.sync_all();
    }
}

/// Download, validate, and atomically install one pack. On success the archive is
/// at `pack_path(packs_dir, req.id)` and a manifest entry is written; the caller
/// then registers it in the `PmtilesRegistry`. On any failure NOTHING is left
/// behind (the temp `.part` is removed) and the manifest is untouched.
///
/// `available_bytes` is the free space on the packs filesystem (injected so the
/// free-space gate is testable; the runtime layer supplies it via statvfs).
///
/// `cancel` is polled during the extract; if the operator cancels, the sidecar
/// is killed and `Err(DownloadError::Cancelled)` is returned — the temp `.part`
/// is removed by the same cleanup guard as any other error, so no partial pack
/// persists. `on_progress(bytes)` is invoked with the growing temp-file size for
/// the UI's progress bar.
pub fn install_pack(
    extractor: &dyn Extractor,
    packs_dir: &Path,
    available_bytes: u64,
    req: &PackRequest,
    cancel: &Arc<AtomicBool>,
    on_progress: &dyn Fn(u64),
) -> Result<InstalledPack, DownloadError> {
    if !is_safe_pack_id(&req.id) {
        return Err(DownloadError::UnsafeId(req.id.clone()));
    }
    if available_bytes < req.needed_bytes {
        return Err(DownloadError::InsufficientSpace {
            needed: req.needed_bytes,
            available: available_bytes,
        });
    }
    fs::create_dir_all(packs_dir).map_err(|e| DownloadError::Io(e.to_string()))?;

    // A uniquely-named same-dir temp the sidecar writes into. `.part` so the
    // orphan sweep recognizes and removes it if we die mid-extract.
    let tmp_path = packs_dir.join(format!("{}.{}{}", req.id, PACK_EXT, TMP_SUFFIX));
    // Clear any leftover from a previous interrupted attempt.
    let _ = fs::remove_file(&tmp_path);

    // Cleanup guard: remove the temp on any error from the install sequence
    // (extract failure, validation reject, AND cancel — a cancelled download
    // leaves no installed pack, only the temp, removed here).
    let result = install_into_temp(extractor, packs_dir, &tmp_path, req, cancel, on_progress);

    if result.is_err() {
        let _ = fs::remove_file(&tmp_path);
    }
    result
}

/// The install sequence after the safe-id + free-space gates: extract into the
/// temp file, validate, atomically rename into place, fsync the dir, then write
/// the manifest entry. Any error leaves the caller to remove the temp. Split out
/// of [`install_pack`] so the cleanup-on-error is a plain call, not an IIFE.
fn install_into_temp(
    extractor: &dyn Extractor,
    packs_dir: &Path,
    tmp_path: &Path,
    req: &PackRequest,
    cancel: &Arc<AtomicBool>,
    on_progress: &dyn Fn(u64),
) -> Result<InstalledPack, DownloadError> {
    extractor.extract(
        &req.planet_url,
        &req.bbox,
        req.maxzoom,
        tmp_path,
        cancel,
        on_progress,
    )?;

    // A cancel can land in the window AFTER extract returns but BEFORE the
    // irreversible install (validate → atomic rename → manifest). Re-check here so
    // a click in that gap still aborts: returning Cancelled lets the caller's
    // cleanup guard drop the temp, so no pack is installed and success is never
    // reported for a cancelled download (Codex review 2026-06-13, P2).
    if cancel.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(DownloadError::Cancelled);
    }

    let archive = PmtilesArchive::open(tmp_path).map_err(|e| DownloadError::Io(e.to_string()))?;
    let v = validate::validate(&archive, req.size_budget)?;
    // Drop the read handle before renaming.
    drop(archive);

    let final_path = pack_path(packs_dir, &req.id);
    // Atomic replace: rename over any existing same-id pack.
    fs::rename(tmp_path, &final_path).map_err(|e| DownloadError::Io(e.to_string()))?;
    fsync_dir(packs_dir);

    let entry = InstalledPack {
        id: req.id.clone(),
        label: req.label.clone(),
        bbox: [req.bbox.west, req.bbox.south, req.bbox.east, req.bbox.north],
        minzoom: v.min_zoom,
        maxzoom: v.max_zoom,
        schema_version: v.schema_version.clone(),
        bytes: v.len,
        source_build: req.source_build.clone(),
        installed_at: req.installed_at.clone(),
    };

    // Manifest write happens AFTER the archive is in place + dir fsync'd, so a
    // crash before this leaves an unreferenced archive (swept on next startup),
    // never a manifest entry pointing at a missing/partial file.
    let mut manifest = load_manifest(packs_dir);
    manifest.upsert(entry.clone());
    write_manifest_atomic(packs_dir, &manifest)?;
    Ok(entry)
}

/// Delete an installed pack: remove the archive + drop the manifest entry. Returns
/// true if a manifest entry was removed. The caller unregisters it from the
/// `PmtilesRegistry`.
pub fn delete_pack(packs_dir: &Path, id: &str) -> Result<bool, DownloadError> {
    if !is_safe_pack_id(id) {
        return Err(DownloadError::UnsafeId(id.to_string()));
    }
    // Manifest-first, mirroring install's manifest-last discipline in reverse: drop
    // the entry (and durably persist that) BEFORE deleting the archive, so a crash
    // mid-delete leaves an unreferenced archive (swept on next startup) rather than
    // a manifest entry pointing at a deleted file (which would 404 every read until
    // a restart re-derived state). The archive removal is best-effort; if it fails
    // after the manifest write, the orphan sweep reclaims the file later.
    let mut manifest = load_manifest(packs_dir);
    let removed = manifest.remove(id).is_some();
    if removed {
        write_manifest_atomic(packs_dir, &manifest)?;
    }
    let _ = fs::remove_file(pack_path(packs_dir, id));
    Ok(removed)
}

/// Startup orphan sweep: delete any `*.part` (interrupted downloads) and any
/// `*.pmtiles` whose id is not in the manifest (partials renamed-but-not-recorded,
/// or stale packs). Mirrors `forms::import::sweep_stale_staging`. Returns the
/// number of files removed.
pub fn sweep_orphans(packs_dir: &Path, manifest: &PacksManifest) -> usize {
    let known: std::collections::HashSet<&str> = manifest.packs.iter().map(|p| p.id.as_str()).collect();
    let mut removed = 0;
    let Ok(entries) = fs::read_dir(packs_dir) else {
        return 0;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // Always sweep interrupted temp files.
        if name.ends_with(TMP_SUFFIX) {
            if fs::remove_file(&path).is_ok() {
                removed += 1;
            }
            continue;
        }
        // Sweep installed archives not present in the manifest.
        if let Some(stem) = name.strip_suffix(&format!(".{PACK_EXT}")) {
            if !known.contains(stem) && fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }
    }
    removed
}

/// Throttle gate for progress emissions: emit if at least `interval` has elapsed
/// since `last` (or if `last` is `None`, i.e. the first sample). Factored out as a
/// pure function so the command layer's per-event throttle is unit-testable
/// without a Tauri runtime. Returns `true` when the caller should emit + update
/// its `last` cursor.
pub fn should_emit(
    last: Option<std::time::Instant>,
    now: std::time::Instant,
    interval: std::time::Duration,
) -> bool {
    match last {
        None => true,
        Some(prev) => now.duration_since(prev) >= interval,
    }
}

/// Stall-watchdog predicate (tuxlink-k9pg): the in-flight download is considered
/// hung when the sidecar has emitted NO output for at least `timeout`. The caller
/// resets the clock whenever go-pmtiles writes any stdout (a log line or a progress
/// update), so this fires only on a genuinely silent (dead-connection) sidecar — at
/// which point it is killed so the blocking thread unwinds and its in-flight guard
/// clears. This replaces the tuxlink-8g28 file-growth watchdog, which was FATALLY
/// WRONG: go-pmtiles pre-allocates the output file to its final size within ~2s and
/// fills it in place, so the file never "grows" during the real multi-minute
/// download — the old watchdog killed every extract longer than its timeout. Pure so
/// the policy is unit-tested without spawning a process.
pub fn is_stalled(since_last_output: std::time::Duration, timeout: std::time::Duration) -> bool {
    since_last_output >= timeout
}

/// Parse a human-readable byte count as emitted by go-pmtiles' progress output,
/// which uses `dustin/go-humanize` `Bytes` (decimal SI: `B`, `kB`, `MB`, `GB`,
/// `TB`, `PB`, `EB`). e.g. `"76 MB"` → `76_000_000`, `"2.0 GB"` → `2_000_000_000`,
/// `"149 B"` → `149`. Returns `None` for anything that is not `<number> <unit>`.
/// Pure → unit-tested (tuxlink-k9pg).
pub fn parse_humanize_bytes(s: &str) -> Option<u64> {
    let s = s.trim();
    let split = s.find(|c: char| c.is_ascii_alphabetic())?;
    let (num, unit) = s.split_at(split);
    let num: f64 = num.trim().parse().ok()?;
    if !num.is_finite() || num < 0.0 {
        return None;
    }
    let mult: f64 = match unit.trim() {
        "B" => 1.0,
        "kB" => 1e3,
        "MB" => 1e6,
        "GB" => 1e9,
        "TB" => 1e12,
        "PB" => 1e15,
        "EB" => 1e18,
        _ => return None,
    };
    Some((num * mult) as u64)
}

/// Extract `(transferred_bytes, total_bytes)` from a single go-pmtiles `extract`
/// progress line (written to STDOUT, `\r`-updated in place), e.g.
/// `"fetching chunks   3% |…| (76 MB/2.0 GB, 38 MB/s) [1s:51s]"` →
/// `Some((76_000_000, 2_000_000_000))`. The pair lives inside `(…/…` before the
/// rate. Returns `None` for every non-progress line (log lines, blanks), so the
/// caller can feed the whole stdout stream and keep only the matches. This is the
/// REAL progress signal — the output file size is not (go-pmtiles pre-sizes it).
/// Pure → unit-tested (tuxlink-k9pg).
pub fn parse_pmtiles_progress(line: &str) -> Option<(u64, u64)> {
    let open = line.find('(')?;
    let close = line[open..].find(')').map(|i| i + open)?;
    let inner = &line[open + 1..close]; // e.g. "76 MB/2.0 GB, 38 MB/s" or "42/42 MB, …"
    let pair = inner.split(',').next()?; // "76 MB/2.0 GB" or "42/42 MB"
    let (left, right) = pair.split_once('/')?;
    let total = parse_humanize_bytes(right)?;
    // The transferred half may SHARE the total's unit (go-pmtiles 1.30.3 prints e.g.
    // "(42/42 MB, …)" / "(4.3/9.3 GB, …)" once both values are the same magnitude).
    // If the left half is unitless, borrow the unit from the right (Codex P2 — else
    // the bar freezes exactly when transferred catches up to total, near the end).
    let transferred = parse_humanize_bytes(left).or_else(|| {
        let unit: String = right.chars().filter(|c| c.is_ascii_alphabetic()).collect();
        if unit.is_empty() {
            return None;
        }
        parse_humanize_bytes(&format!("{} {}", left.trim(), unit))
    })?;
    Some((transferred, total))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basemap::packs::tier_bbox;
    use crate::basemap::validate::testutil::TestPmtiles;
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    /// A never-cancelled flag for tests that don't exercise cancellation.
    fn no_cancel() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }

    /// A no-op progress sink for tests that don't assert on progress.
    fn noop_progress(_: u64) {}

    fn req(id: &str, needed: u64, budget: u64) -> PackRequest {
        PackRequest {
            id: id.to_string(),
            label: "Wide".to_string(),
            planet_url: "https://build.protomaps.com/20260608.pmtiles".to_string(),
            bbox: tier_bbox(-112.0, 33.5, 7.5, 6.0).unwrap(),
            maxzoom: 14,
            source_build: "20260608".to_string(),
            typical_bytes: needed,
            needed_bytes: needed,
            size_budget: budget,
            installed_at: "2026-06-13T00:00:00Z".to_string(),
        }
    }

    /// Extractor that writes a well-formed (default TestPmtiles) archive. Reports
    /// the final byte count via `on_progress` so happy-path callers still exercise
    /// the progress channel.
    struct GoodExtractor;
    impl Extractor for GoodExtractor {
        fn extract(
            &self,
            _url: &str,
            _b: &Bbox,
            _z: u8,
            dest: &Path,
            _cancel: &Arc<AtomicBool>,
            on_progress: &dyn Fn(u64),
        ) -> Result<(), DownloadError> {
            let bytes = TestPmtiles::default().build();
            fs::write(dest, &bytes).map_err(|e| DownloadError::Io(e.to_string()))?;
            on_progress(bytes.len() as u64);
            Ok(())
        }
    }

    /// Extractor that writes a raster (invalid) archive — exercises the validation
    /// reject + cleanup path.
    struct RasterExtractor;
    impl Extractor for RasterExtractor {
        fn extract(
            &self,
            _url: &str,
            _b: &Bbox,
            _z: u8,
            dest: &Path,
            _cancel: &Arc<AtomicBool>,
            _on_progress: &dyn Fn(u64),
        ) -> Result<(), DownloadError> {
            let bytes = TestPmtiles { tile_type: 2, ..Default::default() }.build();
            fs::write(dest, bytes).map_err(|e| DownloadError::Io(e.to_string()))
        }
    }

    /// Extractor that fails (sidecar error / killed).
    struct FailingExtractor;
    impl Extractor for FailingExtractor {
        fn extract(
            &self,
            _url: &str,
            _b: &Bbox,
            _z: u8,
            _dest: &Path,
            _cancel: &Arc<AtomicBool>,
            _on_progress: &dyn Fn(u64),
        ) -> Result<(), DownloadError> {
            Err(DownloadError::ExtractFailed("killed".into()))
        }
    }

    /// Extractor that writes the temp in growing chunks, calling `on_progress`
    /// after each, and honors the cancel flag (mirrors the SidecarExtractor poll
    /// loop without a child process). If cancelled it leaves a partial temp and
    /// returns `Cancelled`, so the install cleanup guard is exercised.
    struct ChunkedExtractor {
        chunks: usize,
        cancel_after: Option<usize>,
    }
    impl Extractor for ChunkedExtractor {
        fn extract(
            &self,
            _url: &str,
            _b: &Bbox,
            _z: u8,
            dest: &Path,
            cancel: &Arc<AtomicBool>,
            on_progress: &dyn Fn(u64),
        ) -> Result<(), DownloadError> {
            use std::io::Write;
            let mut f = fs::File::create(dest).map_err(|e| DownloadError::Io(e.to_string()))?;
            let mut written = 0u64;
            for i in 0..self.chunks {
                if let Some(n) = self.cancel_after {
                    if i == n {
                        cancel.store(true, Ordering::SeqCst);
                    }
                }
                if cancel.load(Ordering::SeqCst) {
                    return Err(DownloadError::Cancelled);
                }
                let chunk = [0u8; 16];
                f.write_all(&chunk).map_err(|e| DownloadError::Io(e.to_string()))?;
                f.flush().map_err(|e| DownloadError::Io(e.to_string()))?;
                written += chunk.len() as u64;
                on_progress(written);
            }
            Ok(())
        }
    }

    #[test]
    fn install_happy_path_writes_archive_and_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let entry = install_pack(&GoodExtractor, dir.path(), u64::MAX, &req("tier-wide-n34-w112", 1, 10_000_000), &no_cancel(), &noop_progress).unwrap();
        assert_eq!(entry.id, "tier-wide-n34-w112");
        assert!(entry.bytes > 0);
        assert_eq!(entry.maxzoom, 6); // TestPmtiles default
        // Archive present, no temp left.
        assert!(pack_path(dir.path(), "tier-wide-n34-w112").exists());
        assert!(!dir.path().join("tier-wide-n34-w112.pmtiles.part").exists());
        // Manifest records it.
        let m = load_manifest(dir.path());
        assert_eq!(m.packs.len(), 1);
        assert_eq!(m.packs[0].id, "tier-wide-n34-w112");
    }

    /// Writes a VALID archive, then flips the cancel flag and returns Ok —
    /// simulating a cancel that lands AFTER extract completes but BEFORE install.
    /// Without the post-extract re-check this pack would validate + install + report
    /// success despite the cancel (Codex review 2026-06-13, P2).
    struct ValidThenCancelExtractor;
    impl Extractor for ValidThenCancelExtractor {
        fn extract(
            &self,
            _url: &str,
            _b: &Bbox,
            _z: u8,
            dest: &Path,
            cancel: &Arc<AtomicBool>,
            on_progress: &dyn Fn(u64),
        ) -> Result<(), DownloadError> {
            let bytes = TestPmtiles::default().build();
            fs::write(dest, &bytes).map_err(|e| DownloadError::Io(e.to_string()))?;
            on_progress(bytes.len() as u64);
            // The cancel arrives just as the transfer finishes.
            cancel.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn cancel_after_extract_but_before_install_aborts_and_installs_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let err = install_pack(
            &ValidThenCancelExtractor,
            dir.path(),
            u64::MAX,
            &req("tier-wide-n34-w112", 1, 10_000_000),
            &no_cancel(),
            &noop_progress,
        )
        .unwrap_err();
        assert!(matches!(err, DownloadError::Cancelled));
        // The valid archive was NOT installed — cancel won the race.
        assert!(!pack_path(dir.path(), "tier-wide-n34-w112").exists());
        assert!(!dir.path().join("tier-wide-n34-w112.pmtiles.part").exists());
        assert!(load_manifest(dir.path()).packs.is_empty());
    }

    #[test]
    fn insufficient_space_rejects_before_extract() {
        let dir = tempfile::tempdir().unwrap();
        let err = install_pack(&GoodExtractor, dir.path(), 100, &req("tier-wide-n34-w112", 1_000_000, 10_000_000), &no_cancel(), &noop_progress).unwrap_err();
        assert!(matches!(err, DownloadError::InsufficientSpace { .. }));
        // Nothing written.
        assert!(!pack_path(dir.path(), "tier-wide-n34-w112").exists());
        assert!(load_manifest(dir.path()).packs.is_empty());
    }

    #[test]
    fn invalid_download_is_rejected_and_leaves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let err = install_pack(&RasterExtractor, dir.path(), u64::MAX, &req("tier-wide-n34-w112", 1, 10_000_000), &no_cancel(), &noop_progress).unwrap_err();
        assert!(matches!(err, DownloadError::Validation(ValidationError::NotVectorTiles(_))));
        // No archive, no temp, no manifest entry — corrupt state never persisted.
        assert!(!pack_path(dir.path(), "tier-wide-n34-w112").exists());
        assert!(!dir.path().join("tier-wide-n34-w112.pmtiles.part").exists());
        assert!(load_manifest(dir.path()).packs.is_empty());
    }

    #[test]
    fn over_budget_download_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        // Budget smaller than the (small) test archive → SizeBudgetExceeded.
        let err = install_pack(&GoodExtractor, dir.path(), u64::MAX, &req("tier-wide-n34-w112", 1, 10), &no_cancel(), &noop_progress).unwrap_err();
        assert!(matches!(err, DownloadError::Validation(ValidationError::SizeBudgetExceeded { .. })));
        assert!(load_manifest(dir.path()).packs.is_empty());
    }

    #[test]
    fn extractor_failure_cleans_temp() {
        let dir = tempfile::tempdir().unwrap();
        let err = install_pack(&FailingExtractor, dir.path(), u64::MAX, &req("tier-wide-n34-w112", 1, 10_000_000), &no_cancel(), &noop_progress).unwrap_err();
        assert!(matches!(err, DownloadError::ExtractFailed(_)));
        assert!(!dir.path().join("tier-wide-n34-w112.pmtiles.part").exists());
    }

    #[test]
    fn unsafe_id_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let err = install_pack(&GoodExtractor, dir.path(), u64::MAX, &req("../etc/passwd", 1, 10_000_000), &no_cancel(), &noop_progress).unwrap_err();
        assert!(matches!(err, DownloadError::UnsafeId(_)));
    }

    #[test]
    fn reinstall_same_id_replaces_not_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        install_pack(&GoodExtractor, dir.path(), u64::MAX, &req("tier-wide-n34-w112", 1, 10_000_000), &no_cancel(), &noop_progress).unwrap();
        install_pack(&GoodExtractor, dir.path(), u64::MAX, &req("tier-wide-n34-w112", 1, 10_000_000), &no_cancel(), &noop_progress).unwrap();
        assert_eq!(load_manifest(dir.path()).packs.len(), 1);
    }

    #[test]
    fn delete_removes_archive_and_entry() {
        let dir = tempfile::tempdir().unwrap();
        install_pack(&GoodExtractor, dir.path(), u64::MAX, &req("tier-wide-n34-w112", 1, 10_000_000), &no_cancel(), &noop_progress).unwrap();
        assert!(delete_pack(dir.path(), "tier-wide-n34-w112").unwrap());
        assert!(!pack_path(dir.path(), "tier-wide-n34-w112").exists());
        assert!(load_manifest(dir.path()).packs.is_empty());
        // Deleting a missing pack is a no-op (false), not an error.
        assert!(!delete_pack(dir.path(), "tier-wide-n34-w112").unwrap());
    }

    #[test]
    fn delete_removes_manifest_entry_even_if_archive_already_gone() {
        // Manifest-first delete ordering: the entry is dropped + persisted before the
        // archive removal, so an already-missing archive (e.g. a crash that removed
        // the file but not the entry on a prior run) still cleanly drops the entry —
        // no manifest entry is left pointing at a deleted file.
        let dir = tempfile::tempdir().unwrap();
        install_pack(&GoodExtractor, dir.path(), u64::MAX, &req("tier-wide-n34-w112", 1, 10_000_000), &no_cancel(), &noop_progress).unwrap();
        // Simulate the archive already gone (manifest still references it).
        fs::remove_file(pack_path(dir.path(), "tier-wide-n34-w112")).unwrap();
        assert!(delete_pack(dir.path(), "tier-wide-n34-w112").unwrap());
        assert!(load_manifest(dir.path()).packs.is_empty());
    }

    #[test]
    fn sweep_removes_orphans_keeps_registered() {
        let dir = tempfile::tempdir().unwrap();
        install_pack(&GoodExtractor, dir.path(), u64::MAX, &req("tier-wide-n34-w112", 1, 10_000_000), &no_cancel(), &noop_progress).unwrap();
        // A stray interrupted temp + an unreferenced archive.
        fs::write(dir.path().join("tier-old-n10-e10.pmtiles.part"), b"partial").unwrap();
        fs::write(dir.path().join("continent-na.pmtiles"), b"orphan").unwrap();
        let manifest = load_manifest(dir.path());
        let removed = sweep_orphans(dir.path(), &manifest);
        assert_eq!(removed, 2);
        // Registered pack + manifest survive.
        assert!(pack_path(dir.path(), "tier-wide-n34-w112").exists());
        assert!(manifest_path(dir.path()).exists());
        assert!(!dir.path().join("continent-na.pmtiles").exists());
        assert!(!dir.path().join("tier-old-n10-e10.pmtiles.part").exists());
    }

    #[test]
    fn corrupt_manifest_loads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(manifest_path(dir.path()), b"{ this is not json").unwrap();
        assert!(load_manifest(dir.path()).packs.is_empty());
    }

    #[test]
    fn progress_callback_is_invoked_with_growing_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let seen = std::sync::Mutex::new(Vec::<u64>::new());
        let ex = ChunkedExtractor { chunks: 4, cancel_after: None };
        let _ = install_pack(
            &ex,
            dir.path(),
            u64::MAX,
            &req("tier-wide-n34-w112", 1, 10_000_000),
            &no_cancel(),
            &|b| seen.lock().unwrap().push(b),
        );
        let seen = seen.into_inner().unwrap();
        // One progress sample per chunk, strictly increasing.
        assert_eq!(seen.len(), 4);
        assert!(seen.windows(2).all(|w| w[1] > w[0]), "bytes must grow: {seen:?}");
        assert_eq!(seen[0], 16);
        assert_eq!(seen[3], 64);
    }

    #[test]
    fn cancel_returns_cancelled_and_leaves_no_pack() {
        let dir = tempfile::tempdir().unwrap();
        // Cancel after the 2nd chunk: a partial temp exists when Cancelled fires.
        let ex = ChunkedExtractor { chunks: 8, cancel_after: Some(2) };
        let err = install_pack(
            &ex,
            dir.path(),
            u64::MAX,
            &req("tier-wide-n34-w112", 1, 10_000_000),
            &no_cancel(),
            &noop_progress,
        )
        .unwrap_err();
        assert!(matches!(err, DownloadError::Cancelled));
        // No installed archive, no leftover temp, no manifest entry.
        assert!(!pack_path(dir.path(), "tier-wide-n34-w112").exists());
        assert!(!dir.path().join("tier-wide-n34-w112.pmtiles.part").exists());
        assert!(load_manifest(dir.path()).packs.is_empty());
    }

    #[test]
    fn cancel_flag_set_before_extract_aborts_immediately() {
        let dir = tempfile::tempdir().unwrap();
        let cancel = Arc::new(AtomicBool::new(true));
        let ex = ChunkedExtractor { chunks: 8, cancel_after: None };
        let err = install_pack(
            &ex,
            dir.path(),
            u64::MAX,
            &req("tier-wide-n34-w112", 1, 10_000_000),
            &cancel,
            &noop_progress,
        )
        .unwrap_err();
        assert!(matches!(err, DownloadError::Cancelled));
        assert!(!dir.path().join("tier-wide-n34-w112.pmtiles.part").exists());
    }

    #[test]
    fn should_emit_gates_on_interval() {
        let interval = Duration::from_millis(400);
        let t0 = Instant::now();
        // First sample (no prior cursor) always emits.
        assert!(should_emit(None, t0, interval));
        // Too soon after the last emit → suppressed.
        let soon = t0 + Duration::from_millis(100);
        assert!(!should_emit(Some(t0), soon, interval));
        // At/after the interval → emit.
        let later = t0 + Duration::from_millis(400);
        assert!(should_emit(Some(t0), later, interval));
        let much_later = t0 + Duration::from_secs(2);
        assert!(should_emit(Some(t0), much_later, interval));
    }

    #[test]
    fn is_stalled_trips_only_after_timeout() {
        let timeout = Duration::from_secs(120);
        // Recent output → not stalled.
        assert!(!is_stalled(Duration::ZERO, timeout));
        assert!(!is_stalled(Duration::from_secs(119), timeout));
        // At/after the timeout with no output → stalled (boundary is inclusive).
        assert!(is_stalled(Duration::from_secs(120), timeout));
        assert!(is_stalled(Duration::from_secs(600), timeout));
    }

    #[test]
    fn parse_humanize_bytes_handles_go_humanize_units() {
        assert_eq!(parse_humanize_bytes("0 B"), Some(0));
        assert_eq!(parse_humanize_bytes("149 B"), Some(149));
        assert_eq!(parse_humanize_bytes("14 kB"), Some(14_000));
        assert_eq!(parse_humanize_bytes("2.3 MB"), Some(2_300_000));
        assert_eq!(parse_humanize_bytes("76 MB"), Some(76_000_000));
        assert_eq!(parse_humanize_bytes("2.0 GB"), Some(2_000_000_000));
        assert_eq!(parse_humanize_bytes(" 17 GB "), Some(17_000_000_000));
        // Not a byte count.
        assert_eq!(parse_humanize_bytes("8236 tiles"), None);
        assert_eq!(parse_humanize_bytes("38 MB/s"), None); // unit "MB/s" not allowed
        assert_eq!(parse_humanize_bytes(""), None);
        assert_eq!(parse_humanize_bytes("garbage"), None);
    }

    #[test]
    fn parse_pmtiles_progress_extracts_transferred_and_total() {
        // Real go-pmtiles `extract` progress lines (observed on pmtiles 1.30.3).
        assert_eq!(
            parse_pmtiles_progress("fetching chunks   3% |   | (76 MB/2.0 GB, 38 MB/s) [1s:51s]"),
            Some((76_000_000, 2_000_000_000))
        );
        assert_eq!(
            parse_pmtiles_progress("fetching chunks   0% |   | ( 0 B/13 MB) [0s:0s]"),
            Some((0, 13_000_000))
        );
        assert_eq!(
            parse_pmtiles_progress("fetching chunks  47% |   | (4.3 GB/9.3 GB, 24 MB/s) [3m:4m]"),
            Some((4_300_000_000, 9_300_000_000))
        );
        // SHARED-UNIT form (go-pmtiles 1.30.3 once both values are the same
        // magnitude): the left half is unitless and borrows the right's unit.
        assert_eq!(
            parse_pmtiles_progress("fetching chunks 100% |   | (42/42 MB, 327 MB/s) [0s:0s]"),
            Some((42_000_000, 42_000_000))
        );
        assert_eq!(
            parse_pmtiles_progress("fetching chunks  46% |   | (4.3/9.3 GB, 24 MB/s) [3m:4m]"),
            Some((4_300_000_000, 9_300_000_000))
        );
    }

    #[test]
    fn parse_pmtiles_progress_ignores_non_progress_lines() {
        // Log lines + the completion line must NOT be mistaken for progress.
        assert_eq!(parse_pmtiles_progress("extract.go:373: fetching 14 dirs, 14 chunks, 2 requests"), None);
        assert_eq!(parse_pmtiles_progress("Region tiles 852343, result tile entries 418331"), None);
        assert_eq!(
            parse_pmtiles_progress("Completed in 50.45s with 4 download threads (8236.56 tiles/s)."),
            None
        );
        assert_eq!(parse_pmtiles_progress(""), None);
        assert_eq!(parse_pmtiles_progress("                    "), None);
    }
}
