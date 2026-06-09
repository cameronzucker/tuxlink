//! `tiles::cache` — bounded, traversal-safe, single-flight-friendly on-disk
//! tile cache.
//!
//! ## §8.4 cache discipline
//!
//! The cache is the filesystem twin of the SSRF gate. Every path it builds is
//! derived from validated-integer [`TileCoord`] components and a per-source
//! namespace that is a SHA-256 hex digest — never raw operator/webview strings.
//! Concretely:
//!
//! - **Per-source namespace.** A source's cache subtree is keyed by
//!   `sha256(normalized_url + crs + scheme)` so two sources that differ only in
//!   CRS or scheme cannot collide, and rotating a source's URL transparently
//!   re-namespaces (the old subtree becomes orphan and is reclaimed by
//!   [`purge`]).
//! - **Traversal-safety.** [`tile_path`] builds `cache_root/<ns>/<rel_path>`
//!   from validated integers, then canonicalizes the *parent* directory (after
//!   creating it — `canonicalize` requires existence) and asserts the result
//!   stays under the canonical `cache_root`. A coordinate cannot escape the
//!   cache root.
//! - **Cache-only-good.** [`put`] writes ONLY non-empty, image-magic-validated
//!   bytes, via a same-directory temp file + atomic rename, so a concurrent
//!   reader never observes a partial file. A failed/ENOSPC write degrades
//!   silently to "served-but-uncached" (`Ok`), never a user-facing error.
//! - **Bounded growth.** A per-namespace `meta.json` tracks total bytes + an
//!   LRU index. [`put`] evicts least-recently-accessed entries *before* writing
//!   so total bytes never exceed the source's byte cap.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::coord::TileCoord;
use super::{Crs, TileScheme, TileSource};

/// Default per-source cache budget (MiB) when a source does not specify one
/// (§8.7). Mirrors `TileSource::cache_budget_mb`'s configured default.
pub const DEFAULT_CACHE_BUDGET_MB: u64 = 384;

/// Compute the per-source cache namespace: `sha256(normalized_url + crs +
/// scheme)` rendered as lowercase hex.
///
/// The URL is normalized by trimming a single trailing `/` (so `…/tiles` and
/// `…/tiles/` share a namespace, matching `fetch::build_tile_url`'s
/// trailing-slash normalization) and lowercasing — the latter folds host-case
/// differences without changing path semantics for the integer tile segments
/// the cache actually keys on. CRS and scheme are appended as stable discriminant
/// strings so two sources differing only in projection/scheme never collide.
pub fn source_namespace(source: &TileSource) -> String {
    let normalized_url = source.url.trim_end_matches('/').to_ascii_lowercase();
    let crs = match source.crs {
        Crs::Geodetic => "geodetic",
    };
    let scheme = match source.scheme {
        TileScheme::Xyz => "xyz",
        TileScheme::Tms => "tms",
    };
    let mut hasher = Sha256::new();
    hasher.update(normalized_url.as_bytes());
    hasher.update(b"\0");
    hasher.update(crs.as_bytes());
    hasher.update(b"\0");
    hasher.update(scheme.as_bytes());
    let digest = hasher.finalize();
    hex_lower(&digest)
}

/// Lowercase-hex encode a byte slice (avoids a hex-crate dependency).
fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Resolve the on-disk path for a tile, asserting it stays under `cache_root`.
///
/// Builds `cache_root/<ns>/<coord.rel_path(tms)>` from validated integers, then
/// — because `std::fs::canonicalize` requires the path to EXIST — creates and
/// canonicalizes the *parent* directory and joins the integer filename. The
/// canonical parent MUST start with the canonical `cache_root`; otherwise the
/// path escaped (symlink / traversal) and the call errors.
//
// traversal-safety (§8.4): the canonicalize + starts_with gate below is the
// filesystem twin of the SSRF host gate. Phase-10 pitfalls cites this anchor.
pub fn tile_path(
    cache_root: &Path,
    ns: &str,
    coord: &TileCoord,
    tms: bool,
) -> std::io::Result<PathBuf> {
    let rel = coord.rel_path(tms); // integers only: `<z>/<x>/<y>.tile`
    let full = cache_root.join(ns).join(&rel);
    let parent = full
        .parent()
        .ok_or_else(|| io_err("tile path has no parent directory"))?;
    std::fs::create_dir_all(parent)?;

    let canon_root = std::fs::canonicalize(cache_root)?;
    let canon_parent = std::fs::canonicalize(parent)?;
    if !canon_parent.starts_with(&canon_root) {
        return Err(io_err(&format!(
            "tile path {canon_parent:?} escapes cache root {canon_root:?}"
        )));
    }

    let file_name = full
        .file_name()
        .ok_or_else(|| io_err("tile path has no file name"))?;
    Ok(canon_parent.join(file_name))
}

fn io_err(msg: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, msg.to_string())
}

/// Write a verified tile body into the cache for `source`+`coord`.
///
/// Contract (§8.4 cache-only-good):
/// - `bytes` MUST be a non-empty, image-magic-validated body (the same magic
///   check the fetch layer applies). A failed/non-image slice is rejected with
///   `Err(InvalidData)` so a `NotAnImage`/`NotFound`/empty result is NEVER
///   persisted. Callers only reach `put` with an upstream-200 image body, so
///   this is defense-in-depth.
/// - The write is atomic: a same-directory temp file is written + fsync'd, then
///   `rename`d over the final path, so a concurrent [`get`] never observes a
///   partial file.
/// - A write failure (ENOSPC, permission, etc.) AFTER validation passes
///   degrades SILENTLY to "served-but-uncached": logged and returned as
///   `Ok(())`, never surfaced as a user-facing error. (Validation failure is
///   the lone `Err` path because it signals a caller bug, not a disk
///   condition.)
pub fn put(cache_root: &Path, source: &TileSource, coord: &TileCoord, bytes: &[u8]) -> std::io::Result<()> {
    // Cache-only-good: reject empty + non-image up front. This is the ONLY
    // hard-error path; anything past here degrades silently.
    if bytes.is_empty() || crate::tiles::fetch::image_mime_from_magic(bytes).is_none() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "refusing to cache a non-image / empty tile body",
        ));
    }

    let tms = matches!(source.scheme, TileScheme::Tms);
    let ns = source_namespace(source);

    // Everything below this point is best-effort: degrade to uncached on error.
    if let Err(e) = put_inner(cache_root, source, &ns, coord, tms, bytes) {
        tracing::warn!(
            "tile cache write degraded to served-but-uncached for {coord:?}: {e}"
        );
    }
    Ok(())
}

/// The fallible body of [`put`] (eviction + atomic write + meta update). Any
/// error here is swallowed by `put` into served-but-uncached.
fn put_inner(
    cache_root: &Path,
    source: &TileSource,
    ns: &str,
    coord: &TileCoord,
    tms: bool,
    bytes: &[u8],
) -> std::io::Result<()> {
    let path = tile_path(cache_root, ns, coord, tms)?;
    let parent = path
        .parent()
        .ok_or_else(|| io_err("resolved tile path has no parent"))?;

    // Bounded growth (§8.4): evict LRU entries until this write fits under the
    // byte cap, BEFORE writing the new bytes.
    let ns_root = cache_root.join(ns);
    let cap = byte_cap(source);
    evict_to_fit(&ns_root, cap, bytes.len() as u64)?;

    // Atomic temp+rename (mirrors config::write_config_atomic): write to a
    // same-directory temp file, fsync, then persist via rename.
    let tmp = tempfile::NamedTempFile::new_in(parent)?;
    {
        use std::io::Write;
        tmp.as_file().write_all(bytes)?;
        tmp.as_file().sync_all()?;
    }
    tmp.persist(&path).map_err(|e| e.error)?;
    // Parent-dir fsync: rename(2) is atomic but not durable until parent
    // metadata flushes (cf. config::write_config_atomic).
    std::fs::File::open(parent)?.sync_all()?;

    // Record the entry in the namespace meta (LRU index + total bytes).
    let rel = coord.rel_path(tms);
    let rel_str = rel.to_string_lossy().replace('\\', "/");
    record_entry(&ns_root, &rel_str, bytes.len() as u64)?;
    Ok(())
}

/// Read a cached tile body for `source`+`coord`, if present. Updates the entry's
/// last-access timestamp (LRU bookkeeping). Returns `None` on any miss or read
/// error (the caller fetches through).
pub fn get(cache_root: &Path, source: &TileSource, coord: &TileCoord) -> Option<Vec<u8>> {
    let tms = matches!(source.scheme, TileScheme::Tms);
    let ns = source_namespace(source);
    let path = tile_path(cache_root, &ns, coord, tms).ok()?;
    let bytes = std::fs::read(&path).ok()?;
    if bytes.is_empty() {
        return None;
    }
    // LRU touch: best-effort, a failure here doesn't invalidate the hit.
    let rel = coord.rel_path(tms);
    let rel_str = rel.to_string_lossy().replace('\\', "/");
    let _ = touch_entry(&cache_root.join(&ns), &rel_str);
    Some(bytes)
}

// ===========================================================================
// Bounded-growth meta + LRU eviction (Task 5.3)
// ===========================================================================

/// Per-namespace cache index, persisted as `<ns_root>/meta.json`.
///
/// Tracks the running total + an entry list so [`put`] can evict the
/// least-recently-accessed tiles BEFORE writing, keeping total bytes ≤ the
/// source's byte cap. `last_access` is a monotonic logical counter (not a wall
/// clock) so eviction ordering is deterministic and clock-skew-immune.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct CacheMeta {
    total_bytes: u64,
    /// Monotonic counter; each access bumps it. Newest = highest.
    clock: u64,
    entries: Vec<MetaEntry>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct MetaEntry {
    /// Forward-slash relative path under the namespace (e.g. `3/5/2.tile`).
    rel: String,
    bytes: u64,
    last_access: u64,
}

/// The source's hard byte cap, derived from `cache_budget_mb` (default
/// [`DEFAULT_CACHE_BUDGET_MB`] when zero/unset). §8.7.
fn byte_cap(source: &TileSource) -> u64 {
    let mb = if source.cache_budget_mb == 0 {
        DEFAULT_CACHE_BUDGET_MB
    } else {
        source.cache_budget_mb
    };
    mb.saturating_mul(1024 * 1024)
}

fn meta_path(ns_root: &Path) -> PathBuf {
    ns_root.join("meta.json")
}

fn load_meta(ns_root: &Path) -> CacheMeta {
    match std::fs::read(meta_path(ns_root)) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => CacheMeta::default(),
    }
}

/// Persist meta atomically (same temp+rename discipline as tile bodies) so a
/// crash mid-write never leaves a half-written index.
fn store_meta(ns_root: &Path, meta: &CacheMeta) -> std::io::Result<()> {
    std::fs::create_dir_all(ns_root)?;
    let tmp = tempfile::NamedTempFile::new_in(ns_root)?;
    serde_json::to_writer(tmp.as_file(), meta)?;
    tmp.as_file().sync_all()?;
    tmp.persist(meta_path(ns_root)).map_err(|e| e.error)?;
    std::fs::File::open(ns_root)?.sync_all()?;
    Ok(())
}

/// Evict least-recently-accessed entries until `incoming_bytes` fits under
/// `cap`. Runs BEFORE the new tile is written, so the namespace total never
/// exceeds the cap. If a single tile is larger than the whole cap, eviction
/// drains everything and the write still proceeds (best-effort; the next call
/// will evict it in turn — a degenerate cap is an operator misconfig, not a
/// reason to refuse service).
fn evict_to_fit(ns_root: &Path, cap: u64, incoming_bytes: u64) -> std::io::Result<()> {
    let mut meta = load_meta(ns_root);
    // Sort a working index by last_access ascending (oldest first).
    while meta.total_bytes.saturating_add(incoming_bytes) > cap && !meta.entries.is_empty() {
        // Find the LRU entry.
        let (lru_idx, _) = meta
            .entries
            .iter()
            .enumerate()
            .min_by_key(|(_, e)| e.last_access)
            .expect("non-empty checked above");
        let victim = meta.entries.swap_remove(lru_idx);
        meta.total_bytes = meta.total_bytes.saturating_sub(victim.bytes);
        // Remove the file (best-effort; a missing file just means the meta was
        // ahead of disk — still reclaim the bytes).
        let _ = std::fs::remove_file(ns_root.join(&victim.rel));
    }
    store_meta(ns_root, &meta)
}

/// Insert/replace an entry for `rel` and bump the clock. Called after a
/// successful tile write.
fn record_entry(ns_root: &Path, rel: &str, bytes: u64) -> std::io::Result<()> {
    let mut meta = load_meta(ns_root);
    meta.clock = meta.clock.saturating_add(1);
    let access = meta.clock;
    if let Some(existing) = meta.entries.iter_mut().find(|e| e.rel == rel) {
        meta.total_bytes = meta.total_bytes.saturating_sub(existing.bytes).saturating_add(bytes);
        existing.bytes = bytes;
        existing.last_access = access;
    } else {
        meta.total_bytes = meta.total_bytes.saturating_add(bytes);
        meta.entries.push(MetaEntry { rel: rel.to_string(), bytes, last_access: access });
    }
    store_meta(ns_root, &meta)
}

/// Bump an entry's last-access on a cache hit (LRU touch). No-op if the entry
/// is absent from meta.
fn touch_entry(ns_root: &Path, rel: &str) -> std::io::Result<()> {
    let mut meta = load_meta(ns_root);
    meta.clock = meta.clock.saturating_add(1);
    let access = meta.clock;
    if let Some(existing) = meta.entries.iter_mut().find(|e| e.rel == rel) {
        existing.last_access = access;
        store_meta(ns_root, &meta)
    } else {
        Ok(())
    }
}

/// Empty a source's cache subtree (delete all tiles + meta) but leave the
/// namespace directory in place. Idempotent; a missing subtree is a no-op.
pub fn clear(cache_root: &Path, source: &TileSource) -> std::io::Result<()> {
    let ns_root = cache_root.join(source_namespace(source));
    if ns_root.exists() {
        std::fs::remove_dir_all(&ns_root)?;
    }
    Ok(())
}

/// Purge a source's namespace entirely (the subtree is removed). Used when a
/// source is deleted/rotated so its orphaned tiles are reclaimed. Currently
/// identical to [`clear`] (both remove the namespace subtree); kept as a
/// distinct entry point so Phase 6 can call the intent it means.
pub fn purge(cache_root: &Path, source: &TileSource) -> std::io::Result<()> {
    clear(cache_root, source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tiles::{Crs, TileScheme, TileSource};

    fn source(url: &str) -> TileSource {
        TileSource {
            url: url.into(),
            crs: Crs::Geodetic,
            scheme: TileScheme::Xyz,
            min_zoom: 0,
            max_zoom: 19,
            cache_budget_mb: 384,
            attribution: None,
            label: "test".into(),
        }
    }

    fn coord() -> TileCoord {
        TileCoord::new(3, 5, 2, 19).unwrap()
    }

    // ---- Task 5.1 ----

    #[test]
    fn namespace_is_stable_hex_sha256() {
        let ns = source_namespace(&source("http://192.168.1.5:8080/tiles/"));
        // 32-byte digest → 64 hex chars, lowercase.
        assert_eq!(ns.len(), 64);
        assert!(ns.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        // Deterministic.
        assert_eq!(ns, source_namespace(&source("http://192.168.1.5:8080/tiles/")));
    }

    #[test]
    fn namespace_normalizes_trailing_slash() {
        // `…/tiles` and `…/tiles/` must share a namespace (matches fetch URL
        // normalization).
        let a = source_namespace(&source("http://192.168.1.5:8080/tiles"));
        let b = source_namespace(&source("http://192.168.1.5:8080/tiles/"));
        assert_eq!(a, b);
    }

    #[test]
    fn namespace_separates_crs_and_scheme() {
        let base = source("http://192.168.1.5:8080/tiles/");
        let mut tms = base.clone();
        tms.scheme = TileScheme::Tms;
        assert_ne!(
            source_namespace(&base),
            source_namespace(&tms),
            "scheme must change the namespace"
        );
    }

    #[test]
    fn tile_path_canonicalizes_under_root() {
        let root = tempfile::tempdir().unwrap();
        let ns = source_namespace(&source("http://192.168.1.5:8080/tiles/"));
        let p = tile_path(root.path(), &ns, &coord(), false).unwrap();
        let canon_root = std::fs::canonicalize(root.path()).unwrap();
        assert!(
            p.starts_with(&canon_root),
            "{p:?} must be under {canon_root:?}"
        );
        // Filename is the integer y + `.tile` (XYZ → y unchanged).
        assert_eq!(p.file_name().unwrap(), std::ffi::OsStr::new("2.tile"));
    }

    #[test]
    fn tile_path_namespace_isolates_sources() {
        let root = tempfile::tempdir().unwrap();
        let a = source_namespace(&source("http://192.168.1.5:8080/a/"));
        let b = source_namespace(&source("http://192.168.1.5:8080/b/"));
        let pa = tile_path(root.path(), &a, &coord(), false).unwrap();
        let pb = tile_path(root.path(), &b, &coord(), false).unwrap();
        assert_ne!(pa, pb, "different sources must land in different subtrees");
    }

    fn png_bytes() -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
        v.extend_from_slice(&[0u8; 32]);
        v
    }

    // ---- Task 5.2 ----

    #[test]
    fn put_then_get_round_trips() {
        let root = tempfile::tempdir().unwrap();
        let src = source("http://192.168.1.5:8080/tiles/");
        let body = png_bytes();
        put(root.path(), &src, &coord(), &body).unwrap();
        let got = get(root.path(), &src, &coord()).expect("cached tile must read back");
        assert_eq!(got, body);
    }

    #[test]
    fn put_rejects_empty_and_non_image() {
        let root = tempfile::tempdir().unwrap();
        let src = source("http://192.168.1.5:8080/tiles/");
        // Empty body: rejected (a NotFound/empty upstream result is never cached).
        let e = put(root.path(), &src, &coord(), &[]).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::InvalidData);
        // Non-image body (HTML): rejected (mirrors fetch's magic check).
        let html = b"<html>not a tile</html>";
        let e = put(root.path(), &src, &coord(), html).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::InvalidData);
        // And nothing was written.
        assert!(get(root.path(), &src, &coord()).is_none());
    }

    #[test]
    fn put_is_atomic_no_partial_file_visible() {
        // After put, the final file exists and equals the full body; no leftover
        // temp file is visible in the tile's directory (atomic rename, not a
        // streaming append a reader could catch mid-write).
        let root = tempfile::tempdir().unwrap();
        let src = source("http://192.168.1.5:8080/tiles/");
        let body = png_bytes();
        put(root.path(), &src, &coord(), &body).unwrap();
        let ns = source_namespace(&src);
        let path = tile_path(root.path(), &ns, &coord(), false).unwrap();
        let dir = path.parent().unwrap();
        let stray: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path() != path)
            .collect();
        assert!(
            stray.is_empty(),
            "no temp/partial files should remain beside the tile: {stray:?}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), body);
    }

    #[test]
    fn failed_write_degrades_to_uncached_not_err() {
        // Simulate a write failure by making the cache_root a non-writable path:
        // point cache_root at a FILE (not a dir) so create_dir_all under it
        // fails. put must still return Ok (served-but-uncached), NOT Err.
        let dir = tempfile::tempdir().unwrap();
        let file_as_root = dir.path().join("not-a-dir");
        std::fs::write(&file_as_root, b"x").unwrap();
        let src = source("http://192.168.1.5:8080/tiles/");
        let body = png_bytes();
        // Validation passes (real PNG), but the write cannot land under a file.
        let r = put(&file_as_root, &src, &coord(), &body);
        assert!(r.is_ok(), "failed write must degrade to Ok(()), got {r:?}");
        // And the read is a clean miss.
        assert!(get(&file_as_root, &src, &coord()).is_none());
    }
}
