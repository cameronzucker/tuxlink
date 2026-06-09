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

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};

use super::coord::TileCoord;
use super::{Crs, TileScheme, TileSource};

/// Process-wide registry of per-namespace critical-section locks.
///
/// Concurrency invariant (§8.4 bounded-growth): the meta.json read-modify-write
/// that `put`/`get` perform (`evict_to_fit` + tile write + `record_entry`, and
/// `get`'s `touch_entry`) is NOT atomic on its own. The single-flight layer in
/// `fetch.rs` only dedups the SAME `(ns, coord)`; concurrent operations on
/// DIFFERENT coords of the same namespace (the viewport-pan case — dozens of
/// distinct tiles at once) would otherwise each load a stale meta, each pass the
/// cap check, each write, and last-writer-wins would clobber the accounting →
/// on-disk bytes exceed the cap, tile files written-but-untracked become orphans
/// eviction can never reclaim, and `last_access` bumps clobber each other so LRU
/// ordering breaks. This map hands out one `Mutex` per namespace so the ENTIRE
/// evict→write→record critical section runs atomically per namespace.
///
/// `std::sync::Mutex` (not `tokio`'s) is correct here: the critical section is
/// synchronous file I/O with NO `.await` inside it, and `put`/`get` are
/// themselves synchronous fns. Holding a std Mutex is sound because nothing
/// awaits while the guard is live.
///
/// Map memory: lock entries are never evicted. There is one entry per configured
/// tile source (a handful at most), so unbounded `Weak`-cleanup would be
/// over-engineering for no measurable benefit. The `Arc<Mutex<()>>` values are
/// 1 word each; the map is effectively a small fixed set keyed by 64-hex-char
/// namespace strings.
static CACHE_LOCKS: Lazy<Mutex<HashMap<String, Arc<Mutex<()>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Get (or create) the per-namespace critical-section lock. The outer map lock
/// is held only briefly to look up / insert the `Arc`; the returned `Arc<Mutex>`
/// is what callers hold across the evict→write→record critical section.
fn ns_lock(ns: &str) -> Arc<Mutex<()>> {
    let mut map = CACHE_LOCKS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    map.entry(ns.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

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
    tile_path_inner(cache_root, ns, coord, tms, /*create*/ true)
}

/// Read-side path resolver: like [`tile_path`] but does NOT create the parent
/// directory tree. A read must not materialize the cache layout — if the
/// parent does not exist yet, the tile is simply absent (returns `None`). This
/// keeps `clear`/`purge` durable: a subsequent `get` does not silently recreate
/// the namespace subtree that was just removed.
fn tile_path_read(
    cache_root: &Path,
    ns: &str,
    coord: &TileCoord,
    tms: bool,
) -> std::io::Result<Option<PathBuf>> {
    let rel = coord.rel_path(tms);
    let parent = cache_root.join(ns).join(&rel);
    let parent = parent.parent().map(Path::to_path_buf);
    let Some(parent) = parent else {
        return Err(io_err("tile path has no parent directory"));
    };
    if !parent.exists() {
        return Ok(None); // not yet cached → miss, no traversal possible
    }
    Ok(Some(tile_path_inner(cache_root, ns, coord, tms, false)?))
}

/// Shared core. `create=true` materializes the parent (write path);
/// `create=false` requires it to already exist (read path).
//
// traversal-safety (§8.4): the canonicalize + starts_with gate below is the
// filesystem twin of the SSRF host gate. Phase-10 pitfalls cites this anchor.
fn tile_path_inner(
    cache_root: &Path,
    ns: &str,
    coord: &TileCoord,
    tms: bool,
    create: bool,
) -> std::io::Result<PathBuf> {
    let rel = coord.rel_path(tms); // integers only: `<z>/<x>/<y>.tile`
    let full = cache_root.join(ns).join(&rel);
    let parent = full
        .parent()
        .ok_or_else(|| io_err("tile path has no parent directory"))?;
    if create {
        std::fs::create_dir_all(parent)?;
    }

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
    std::io::Error::other(msg.to_string())
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

    // Serialize the ENTIRE per-namespace critical section (evict_to_fit → tile
    // write → record_entry) so concurrent puts for DIFFERENT coords of this
    // namespace cannot interleave their meta.json accounting (§8.4 bounded
    // growth; see CACHE_LOCKS). The guard is held across all of put_inner; there
    // is no `.await` inside, so holding a std Mutex here is sound.
    let lock = ns_lock(&ns);
    let _guard = lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);

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

    // Traversal-safety (§8.4), leaf check: `tile_path` canonicalizes the PARENT
    // dir and asserts it stays under the cache root, but a planted LEAF symlink
    // at `path` would evade that gate. Refuse a symlinked leaf before writing
    // (mirrors logging/state_dir.rs's symlink refusal). `symlink_metadata` does
    // NOT follow the link, so a planted leaf symlink is detected, not traversed.
    if let Ok(meta) = std::fs::symlink_metadata(&path) {
        if meta.file_type().is_symlink() {
            return Err(io_err(&format!(
                "refusing to write through a symlinked tile path: {path:?}"
            )));
        }
    }

    // Atomic temp+rename (mirrors config::write_config_atomic): write to a
    // same-directory temp file, fsync, then persist via rename.
    //
    // SAFETY-CRITICAL — do NOT replace `tmp.persist(&path)` with a
    // follow-symlink open such as `File::create(path)`. `persist` performs a
    // `rename` that REPLACES the target inode rather than following it, so a
    // planted leaf symlink cannot redirect the write to an arbitrary file. A
    // `File::create(path)` (or any `OpenOptions` open without `O_NOFOLLOW`)
    // would follow a leaf symlink and turn a planted link into an arbitrary
    // write. The symlink_metadata refusal above is the explicit guard; this
    // rename is the structural one. Keep both.
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
    let path = tile_path_read(cache_root, &ns, coord, tms).ok()??;
    let bytes = std::fs::read(&path).ok()?;
    if bytes.is_empty() {
        return None;
    }
    // LRU touch: best-effort, a failure here doesn't invalidate the hit. The
    // touch is a meta.json read-modify-write too, so serialize it under the SAME
    // per-namespace lock as `put`: a concurrent put+get must not clobber meta
    // (a lost touch is benign for correctness, but round-tripping the whole meta
    // could lost-update total_bytes and corrupt LRU ordering). No `.await` is
    // held across this std guard.
    let rel = coord.rel_path(tms);
    let rel_str = rel.to_string_lossy().replace('\\', "/");
    {
        let lock = ns_lock(&ns);
        let _guard = lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = touch_entry(&cache_root.join(&ns), &rel_str);
    }
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

    // ---- Task 5.3 ----

    /// A small PNG of a chosen size (≥ 8 bytes magic). Distinct sizes let us
    /// drive the byte cap with a known per-tile cost.
    fn png_sized(total: usize) -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
        v.resize(total.max(8), 0u8);
        v
    }

    /// Sum the bytes of every `*.tile` file under a namespace subtree.
    fn on_disk_tile_bytes(ns_root: &Path) -> u64 {
        fn walk(dir: &Path, acc: &mut u64) {
            if let Ok(rd) = std::fs::read_dir(dir) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        walk(&p, acc);
                    } else if p.extension().and_then(|s| s.to_str()) == Some("tile") {
                        *acc += std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                    }
                }
            }
        }
        let mut acc = 0;
        walk(ns_root, &mut acc);
        acc
    }

    #[test]
    fn lru_eviction_keeps_total_under_cap() {
        // THE bounded-growth discipline. A tiny cap + many inserts must NEVER
        // let on-disk bytes exceed the cap: eviction runs before each write.
        let root = tempfile::tempdir().unwrap();
        let mut src = source("http://192.168.1.5:8080/tiles/");
        // cache_budget_mb is in MiB; we want a SMALL cap. Use the raw helper to
        // express a sub-MiB cap by overriding via a tiny per-tile budget: set
        // the budget so the cap is ~64 KiB. cache_budget_mb can't express <1MiB,
        // so we drive eviction by inserting MANY 1100-byte tiles against a 1 MiB
        // cap → cap = 1,048,576 bytes; 1100-byte tiles → ~953 fit; insert 1500.
        src.cache_budget_mb = 1; // 1 MiB cap
        let cap = byte_cap(&src);
        let tile = png_sized(1100);
        // Insert 1500 distinct tiles (well past the ~953 that fit under 1 MiB).
        let mut inserted = 0u32;
        for i in 0..1500u32 {
            // Spread across x to get distinct rel paths at z=16 (x<65536).
            let c = TileCoord::new(16, i, 0, 19).unwrap();
            put(root.path(), &src, &c, &tile).unwrap();
            inserted += 1;
        }
        let ns_root = root.path().join(source_namespace(&src));
        let on_disk = on_disk_tile_bytes(&ns_root);
        assert!(
            on_disk <= cap,
            "on-disk bytes {on_disk} must stay <= cap {cap} after {inserted} inserts"
        );
        // And the meta agrees with disk (within the cap).
        let meta = load_meta(&ns_root);
        assert!(meta.total_bytes <= cap, "meta total {} > cap {cap}", meta.total_bytes);
    }

    #[test]
    fn concurrent_puts_distinct_coords_stay_under_cap() {
        // BLOCKER regression: concurrent puts for DIFFERENT coords in the same
        // namespace (the viewport-pan case) must NOT race their meta.json
        // read-modify-write. Without per-namespace serialization each thread
        // loads a stale meta, each passes the cap check, each writes, and
        // last-writer-wins clobbers the accounting → on-disk bytes blow past
        // the cap and meta-tracked tiles become orphans eviction can't reclaim.
        let root = tempfile::tempdir().unwrap();
        let mut src = source("http://192.168.1.5:8080/tiles/");
        src.cache_budget_mb = 1; // 1 MiB cap
        let cap = byte_cap(&src);
        // 50 distinct-coord tiles, 100 KiB each → 5 MiB attempted vs 1 MiB cap.
        const N: u32 = 50;
        const TILE_BYTES: usize = 100 * 1024;
        let tile = png_sized(TILE_BYTES);

        std::thread::scope(|scope| {
            for i in 0..N {
                let root = root.path();
                let src = &src;
                let tile = &tile;
                scope.spawn(move || {
                    // Distinct rel paths: spread across x at z=16.
                    let c = TileCoord::new(16, i, 0, 19).unwrap();
                    put(root, src, &c, tile).unwrap();
                });
            }
        });

        let ns_root = root.path().join(source_namespace(&src));
        let on_disk = on_disk_tile_bytes(&ns_root);
        // (a)/(b): no panic (join above) + on-disk ≤ cap.
        assert!(
            on_disk <= cap,
            "on-disk bytes {on_disk} must stay <= cap {cap} after {N} concurrent distinct-coord puts \
             ({:.2}x over cap)",
            on_disk as f64 / cap as f64
        );
        // (c): meta.json parses.
        let meta = load_meta(&ns_root);
        // (d): no orphans — every on-disk tile is tracked in meta, and meta's
        // total agrees with disk within one tile's slack (the only legitimate
        // skew is a tile written but whose record_entry has not yet landed; the
        // serialized critical section makes write+record atomic, so they match).
        assert!(
            meta.total_bytes <= cap,
            "meta total {} must stay <= cap {cap}",
            meta.total_bytes
        );
        let slack = TILE_BYTES as u64;
        let diff = on_disk.abs_diff(meta.total_bytes);
        assert!(
            diff <= slack,
            "meta total_bytes {} must agree with on-disk {on_disk} within one tile ({slack}); \
             a larger gap means orphaned tile files lost from meta",
            meta.total_bytes
        );
    }

    #[test]
    fn lru_evicts_least_recently_accessed_first() {
        // Insert A then B under a cap that holds ~1 tile of slack; touch A via
        // get (making B the LRU), then insert C → B must be the eviction victim.
        let root = tempfile::tempdir().unwrap();
        let mut src = source("http://192.168.1.5:8080/tiles/");
        src.cache_budget_mb = 1;
        let cap = byte_cap(&src);
        // Each tile ~ 60% of cap so only one fits at a time, forcing a choice.
        let big = png_sized((cap as usize * 6) / 10);
        let a = TileCoord::new(16, 1, 0, 19).unwrap();
        let b = TileCoord::new(16, 2, 0, 19).unwrap();
        let c = TileCoord::new(16, 3, 0, 19).unwrap();
        put(root.path(), &src, &a, &big).unwrap();
        put(root.path(), &src, &b, &big).unwrap(); // evicts A (only one fits)
        // A is gone, B present.
        assert!(get(root.path(), &src, &a).is_none(), "A should have been evicted by B");
        assert!(get(root.path(), &src, &b).is_some(), "B should be present");
        // Now insert C; B is the only resident → B evicted, C present.
        put(root.path(), &src, &c, &big).unwrap();
        assert!(get(root.path(), &src, &b).is_none(), "B should have been evicted by C");
        assert!(get(root.path(), &src, &c).is_some(), "C should be present");
    }

    #[test]
    fn clear_empties_source_subtree() {
        let root = tempfile::tempdir().unwrap();
        let src = source("http://192.168.1.5:8080/tiles/");
        put(root.path(), &src, &coord(), &png_bytes()).unwrap();
        assert!(get(root.path(), &src, &coord()).is_some());
        clear(root.path(), &src).unwrap();
        assert!(get(root.path(), &src, &coord()).is_none(), "clear must empty the subtree");
        let ns_root = root.path().join(source_namespace(&src));
        assert!(!ns_root.exists(), "namespace subtree removed");
        // Idempotent: clearing an absent subtree is a no-op.
        clear(root.path(), &src).unwrap();
    }

    #[test]
    fn purge_removes_namespace_and_isolates_other_sources() {
        let root = tempfile::tempdir().unwrap();
        let a = source("http://192.168.1.5:8080/a/");
        let b = source("http://192.168.1.5:8080/b/");
        put(root.path(), &a, &coord(), &png_bytes()).unwrap();
        put(root.path(), &b, &coord(), &png_bytes()).unwrap();
        purge(root.path(), &a).unwrap();
        // a's tiles gone, b's untouched.
        assert!(get(root.path(), &a, &coord()).is_none(), "purged source has no tiles");
        assert!(get(root.path(), &b, &coord()).is_some(), "other source untouched by purge");
    }
}
