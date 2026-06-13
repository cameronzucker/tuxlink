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
    fn extract(
        &self,
        planet_url: &str,
        bbox: &Bbox,
        maxzoom: u8,
        dest: &Path,
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
pub fn install_pack(
    extractor: &dyn Extractor,
    packs_dir: &Path,
    available_bytes: u64,
    req: &PackRequest,
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

    // Cleanup guard: remove the temp on any early return.
    let result = (|| {
        extractor.extract(&req.planet_url, &req.bbox, req.maxzoom, &tmp_path)?;

        let archive = PmtilesArchive::open(&tmp_path).map_err(|e| DownloadError::Io(e.to_string()))?;
        let v = validate::validate(&archive, req.size_budget)?;
        // Drop the read handle before renaming (Windows-friendliness + clarity).
        drop(archive);

        let final_path = pack_path(packs_dir, &req.id);
        // Atomic replace: rename over any existing same-id pack.
        fs::rename(&tmp_path, &final_path).map_err(|e| DownloadError::Io(e.to_string()))?;
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
    })();

    if result.is_err() {
        let _ = fs::remove_file(&tmp_path);
    }
    result
}

/// Delete an installed pack: remove the archive + drop the manifest entry. Returns
/// true if a manifest entry was removed. The caller unregisters it from the
/// `PmtilesRegistry`.
pub fn delete_pack(packs_dir: &Path, id: &str) -> Result<bool, DownloadError> {
    if !is_safe_pack_id(id) {
        return Err(DownloadError::UnsafeId(id.to_string()));
    }
    let _ = fs::remove_file(pack_path(packs_dir, id));
    let mut manifest = load_manifest(packs_dir);
    let removed = manifest.remove(id).is_some();
    if removed {
        write_manifest_atomic(packs_dir, &manifest)?;
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basemap::packs::tier_bbox;
    use crate::basemap::validate::testutil::TestPmtiles;
    use std::cell::Cell;

    fn req(id: &str, needed: u64, budget: u64) -> PackRequest {
        PackRequest {
            id: id.to_string(),
            label: "Wide".to_string(),
            planet_url: "https://build.protomaps.com/20260608.pmtiles".to_string(),
            bbox: tier_bbox(-112.0, 33.5, 7.5, 6.0).unwrap(),
            maxzoom: 14,
            source_build: "20260608".to_string(),
            needed_bytes: needed,
            size_budget: budget,
            installed_at: "2026-06-13T00:00:00Z".to_string(),
        }
    }

    /// Extractor that writes a well-formed (default TestPmtiles) archive.
    struct GoodExtractor;
    impl Extractor for GoodExtractor {
        fn extract(&self, _url: &str, _b: &Bbox, _z: u8, dest: &Path) -> Result<(), DownloadError> {
            fs::write(dest, TestPmtiles::default().build()).map_err(|e| DownloadError::Io(e.to_string()))
        }
    }

    /// Extractor that writes a raster (invalid) archive — exercises the validation
    /// reject + cleanup path.
    struct RasterExtractor;
    impl Extractor for RasterExtractor {
        fn extract(&self, _url: &str, _b: &Bbox, _z: u8, dest: &Path) -> Result<(), DownloadError> {
            let bytes = TestPmtiles { tile_type: 2, ..Default::default() }.build();
            fs::write(dest, bytes).map_err(|e| DownloadError::Io(e.to_string()))
        }
    }

    /// Extractor that fails (sidecar error / killed).
    struct FailingExtractor;
    impl Extractor for FailingExtractor {
        fn extract(&self, _url: &str, _b: &Bbox, _z: u8, _dest: &Path) -> Result<(), DownloadError> {
            Err(DownloadError::ExtractFailed("killed".into()))
        }
    }

    #[test]
    fn install_happy_path_writes_archive_and_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let entry = install_pack(&GoodExtractor, dir.path(), u64::MAX, &req("tier-wide-n34-w112", 1, 10_000_000)).unwrap();
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

    #[test]
    fn insufficient_space_rejects_before_extract() {
        let dir = tempfile::tempdir().unwrap();
        let err = install_pack(&GoodExtractor, dir.path(), 100, &req("tier-wide-n34-w112", 1_000_000, 10_000_000)).unwrap_err();
        assert!(matches!(err, DownloadError::InsufficientSpace { .. }));
        // Nothing written.
        assert!(!pack_path(dir.path(), "tier-wide-n34-w112").exists());
        assert!(load_manifest(dir.path()).packs.is_empty());
    }

    #[test]
    fn invalid_download_is_rejected_and_leaves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let err = install_pack(&RasterExtractor, dir.path(), u64::MAX, &req("tier-wide-n34-w112", 1, 10_000_000)).unwrap_err();
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
        let err = install_pack(&GoodExtractor, dir.path(), u64::MAX, &req("tier-wide-n34-w112", 1, 10)).unwrap_err();
        assert!(matches!(err, DownloadError::Validation(ValidationError::SizeBudgetExceeded { .. })));
        assert!(load_manifest(dir.path()).packs.is_empty());
    }

    #[test]
    fn extractor_failure_cleans_temp() {
        let dir = tempfile::tempdir().unwrap();
        let err = install_pack(&FailingExtractor, dir.path(), u64::MAX, &req("tier-wide-n34-w112", 1, 10_000_000)).unwrap_err();
        assert!(matches!(err, DownloadError::ExtractFailed(_)));
        assert!(!dir.path().join("tier-wide-n34-w112.pmtiles.part").exists());
    }

    #[test]
    fn unsafe_id_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let err = install_pack(&GoodExtractor, dir.path(), u64::MAX, &req("../etc/passwd", 1, 10_000_000)).unwrap_err();
        assert!(matches!(err, DownloadError::UnsafeId(_)));
    }

    #[test]
    fn reinstall_same_id_replaces_not_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        install_pack(&GoodExtractor, dir.path(), u64::MAX, &req("tier-wide-n34-w112", 1, 10_000_000)).unwrap();
        install_pack(&GoodExtractor, dir.path(), u64::MAX, &req("tier-wide-n34-w112", 1, 10_000_000)).unwrap();
        assert_eq!(load_manifest(dir.path()).packs.len(), 1);
    }

    #[test]
    fn delete_removes_archive_and_entry() {
        let dir = tempfile::tempdir().unwrap();
        install_pack(&GoodExtractor, dir.path(), u64::MAX, &req("tier-wide-n34-w112", 1, 10_000_000)).unwrap();
        assert!(delete_pack(dir.path(), "tier-wide-n34-w112").unwrap());
        assert!(!pack_path(dir.path(), "tier-wide-n34-w112").exists());
        assert!(load_manifest(dir.path()).packs.is_empty());
        // Deleting a missing pack is a no-op (false), not an error.
        assert!(!delete_pack(dir.path(), "tier-wide-n34-w112").unwrap());
    }

    #[test]
    fn sweep_removes_orphans_keeps_registered() {
        let dir = tempfile::tempdir().unwrap();
        install_pack(&GoodExtractor, dir.path(), u64::MAX, &req("tier-wide-n34-w112", 1, 10_000_000)).unwrap();
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
}
