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
}
