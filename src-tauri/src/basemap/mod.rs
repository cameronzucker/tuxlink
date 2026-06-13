//! `basemap` — self-hosted vector OSM basemap serving (tuxlink-ndi4).
//!
//! Plan: docs/superpowers/plans/2026-06-13-vector-basemap-maplibre-swap.md
//! Design: docs/design/2026-06-13-self-hosted-vector-osm-basemap-design.md
//!
//! This module is the Rust seam for the MapLibre-GL vector basemap: it serves a
//! bundled (and, later, downloaded) PMTiles archive's raw bytes to the webview
//! over HTTP-206 `Range` requests on the bespoke `tile://pmtiles/<archive>` URI,
//! consumed by the `pmtiles` JS library's native `FetchSource` (plan A1).
//!
//! Deliberately SEPARATE from [`crate::tiles`] (the LAN raster transport, parked
//! for imagery): that path is HTTP-tile / image-magic shaped and runs SSRF +
//! `MAX_TILE_BYTES` checks per tile. The basemap path serves RAW bytes with zero
//! content decoding (plan A1) — PMTiles internal compression is decoded by the JS
//! client, not here — and validates an archive once (header + schema), not per read.
//!
//! Concurrency (plan A3): one long-lived [`PmtilesArchive`] per archive id holds a
//! shared `Arc<File>`; reads use lock-free positioned `pread` (no per-read open, no
//! mutex, no handle cap, no mmap — mmap would thrash the page cache against the
//! tight GPU/WebKit budget on the Pi). The registry's `RwLock` is held only long
//! enough to clone the `Arc` out; the hot read path is lock-free.

pub mod download;
pub mod packs;
pub mod region_manifest;
pub mod validate;

use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::os::unix::fs::FileExt;
use std::path::Path;
use std::sync::{Arc, RwLock};

/// MIME type for the raw PMTiles byte stream. The bytes are an opaque archive
/// slice (the JS `pmtiles` lib parses + decompresses), so `application/octet-stream`
/// is correct — NOT `application/x-protobuf` (that is the *decoded* tile payload).
pub const PMTILES_CONTENT_TYPE: &str = "application/octet-stream";

/// A single PMTiles archive opened once and read concurrently via `pread`.
///
/// Holds an `Arc<File>` so clones share the same underlying descriptor; positioned
/// reads (`read_at`) take no lock and never seek, so any number of webview tile
/// requests can read disjoint ranges in parallel without contention (plan A3).
#[derive(Debug)]
pub struct PmtilesArchive {
    file: Arc<File>,
    len: u64,
}

impl PmtilesArchive {
    /// Open `path` read-only and record its length. The descriptor stays open for
    /// the archive's lifetime (process-lifetime for the bundled `world` archive).
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        Ok(Self {
            file: Arc::new(file),
            len,
        })
    }

    /// Total archive length in bytes.
    pub fn len(&self) -> u64 {
        self.len
    }

    /// True when the archive is empty (defensive; a real PMTiles is never empty).
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Lock-free positioned read of up to `length` bytes starting at `offset`.
    ///
    /// Clamps to EOF: a read that starts in-bounds but runs past the end returns
    /// only the bytes that exist (a short read), so the caller forms a clamped 206
    /// with the real length rather than erroring (plan A3 "short final read at EOF
    /// → clamped 206 with real length, not an error"). A read whose `offset` is at
    /// or past EOF returns an empty buffer (the caller maps that to 416).
    pub fn read_at(&self, offset: u64, length: usize) -> io::Result<Vec<u8>> {
        if offset >= self.len {
            return Ok(Vec::new());
        }
        let available = (self.len - offset) as usize;
        let n = length.min(available);
        let mut buf = vec![0u8; n];
        // `Arc<File>` derefs to `File`, which implements unix `FileExt::read_exact_at`
        // (pread(2)) — positioned, no shared cursor, safe to call concurrently.
        self.file.read_exact_at(&mut buf, offset)?;
        Ok(buf)
    }
}

/// Process-lifetime registry of opened PMTiles archives, keyed by archive id
/// (`"world"` for the bundled overview; region-pack ids once phase 4 lands).
///
/// Managed by Tauri (`.manage(Arc::new(PmtilesRegistry::new()))`); the `tile://`
/// handler clones the per-archive `Arc` out under a short read lock, then reads
/// lock-free.
#[derive(Debug, Default)]
pub struct PmtilesRegistry {
    archives: RwLock<HashMap<String, Arc<PmtilesArchive>>>,
}

impl PmtilesRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register (or replace) the archive served under `id`.
    pub fn register(&self, id: impl Into<String>, archive: Arc<PmtilesArchive>) {
        self.archives
            .write()
            .expect("PmtilesRegistry lock poisoned")
            .insert(id.into(), archive);
    }

    /// Open `path` and register it under `id`. Returns the archive's length.
    pub fn register_path(&self, id: impl Into<String>, path: &Path) -> io::Result<u64> {
        let archive = Arc::new(PmtilesArchive::open(path)?);
        let len = archive.len();
        self.register(id, archive);
        Ok(len)
    }

    /// Clone the `Arc` for `id`, if registered. The read lock is released before
    /// the caller performs any `read_at`, keeping the read path lock-free.
    pub fn get(&self, id: &str) -> Option<Arc<PmtilesArchive>> {
        self.archives
            .read()
            .expect("PmtilesRegistry lock poisoned")
            .get(id)
            .cloned()
    }
}

/// A parsed HTTP `Range` request (single byte range only — all that `pmtiles`
/// `FetchSource` issues).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RangeSpec {
    /// Inclusive start byte offset.
    pub start: u64,
    /// Inclusive end byte offset, or `None` for an open-ended `bytes=start-`.
    pub end_inclusive: Option<u64>,
}

/// Parse a single-range `Range` header value of the form `bytes=START-END` or
/// `bytes=START-`. Returns `None` for anything we don't serve (multi-range,
/// suffix `bytes=-N`, malformed) — the caller then serves the full body (200).
pub fn parse_range_header(value: &str) -> Option<RangeSpec> {
    let spec = value.trim().strip_prefix("bytes=")?;
    // Reject multi-range ("a-b,c-d") — pmtiles never sends it and we don't serve it.
    if spec.contains(',') {
        return None;
    }
    let (start_s, end_s) = spec.split_once('-')?;
    let start_s = start_s.trim();
    let end_s = end_s.trim();
    // Suffix range (`bytes=-N`) has an empty start; not served.
    if start_s.is_empty() {
        return None;
    }
    let start: u64 = start_s.parse().ok()?;
    let end_inclusive = if end_s.is_empty() {
        None
    } else {
        Some(end_s.parse().ok()?)
    };
    if let Some(end) = end_inclusive {
        if end < start {
            return None;
        }
    }
    Some(RangeSpec {
        start,
        end_inclusive,
    })
}

/// A fully-formed range/full response ready to map onto a `tauri::http::Response`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeResponse {
    /// HTTP status: 200 (full), 206 (partial), or 416 (unsatisfiable).
    pub status: u16,
    /// Response body (empty for 416).
    pub body: Vec<u8>,
    /// `Content-Range` header value, present for 206 and the 416 unsatisfiable case.
    pub content_range: Option<String>,
    /// The archive's total length (for `Content-Range` `/total` and `Content-Length`).
    pub total_len: u64,
}

/// Build the response for `range` against `archive`, reading the requested bytes.
///
/// - No `range` → 200 with the full archive body.
/// - In-bounds `range` → 206, clamped to EOF, with `Content-Range: bytes s-e/total`.
/// - `range.start >= total` → 416 with `Content-Range: bytes */total`.
pub fn read_range(
    archive: &PmtilesArchive,
    range: Option<RangeSpec>,
) -> io::Result<RangeResponse> {
    let total = archive.len();
    let Some(spec) = range else {
        // Full-body request (e.g. a non-range GET). Serve everything.
        let body = archive.read_at(0, total as usize)?;
        return Ok(RangeResponse {
            status: 200,
            body,
            content_range: None,
            total_len: total,
        });
    };

    if spec.start >= total {
        // Unsatisfiable: start beyond EOF.
        return Ok(RangeResponse {
            status: 416,
            body: Vec::new(),
            content_range: Some(format!("bytes */{total}")),
            total_len: total,
        });
    }

    // Clamp the inclusive end to the last byte; open-ended runs to EOF.
    let last = total - 1;
    let end_inclusive = spec.end_inclusive.unwrap_or(last).min(last);
    let length = (end_inclusive - spec.start + 1) as usize;
    let body = archive.read_at(spec.start, length)?;
    // `read_at` may short-read at EOF; reflect the bytes actually returned so the
    // Content-Range end and Content-Length agree with the body.
    let actual_end = spec.start + body.len() as u64 - 1;
    Ok(RangeResponse {
        status: 206,
        content_range: Some(format!("bytes {}-{actual_end}/{total}", spec.start)),
        total_len: total,
        body,
    })
}

/// Extract the archive id from a `tile://pmtiles/<archive>` request's `host` and
/// `path` components.
///
/// wry/WebKitGTK may surface the `pmtiles` segment as the URI host
/// (`host="pmtiles"`, `path="/world"`) or fold it into the path
/// (`path="/pmtiles/world"`); accept both so the branch is robust to scheme-URI
/// normalization. Returns `None` when this is not a pmtiles request (so the
/// caller falls through to the LAN-raster `serve_tile` path).
pub fn parse_pmtiles_uri(host: Option<&str>, path: &str) -> Option<String> {
    if host == Some("pmtiles") {
        let id = path.trim_start_matches('/');
        return (!id.is_empty()).then(|| id.to_string());
    }
    if let Some(rest) = path.trim_start_matches('/').strip_prefix("pmtiles/") {
        return (!rest.is_empty()).then(|| rest.to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    /// Write `bytes` to a temp file and open it as an archive.
    fn archive_of(bytes: &[u8]) -> (NamedTempFile, PmtilesArchive) {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(bytes).unwrap();
        f.flush().unwrap();
        let archive = PmtilesArchive::open(f.path()).unwrap();
        (f, archive)
    }

    #[test]
    fn read_at_returns_exact_bytes_for_in_bounds_range() {
        let data: Vec<u8> = (0..=255u8).collect();
        let (_f, archive) = archive_of(&data);
        assert_eq!(archive.len(), 256);
        assert_eq!(archive.read_at(10, 4).unwrap(), vec![10, 11, 12, 13]);
        assert_eq!(archive.read_at(0, 1).unwrap(), vec![0]);
    }

    #[test]
    fn read_at_clamps_short_read_at_eof() {
        let data: Vec<u8> = (0..10u8).collect();
        let (_f, archive) = archive_of(&data);
        // Ask for 5 bytes starting at 8 — only 2 exist.
        assert_eq!(archive.read_at(8, 5).unwrap(), vec![8, 9]);
    }

    #[test]
    fn read_at_past_eof_returns_empty() {
        let data: Vec<u8> = (0..10u8).collect();
        let (_f, archive) = archive_of(&data);
        assert!(archive.read_at(10, 4).unwrap().is_empty());
        assert!(archive.read_at(99, 4).unwrap().is_empty());
    }

    #[test]
    fn read_at_is_concurrent_safe() {
        use std::thread;
        let data: Vec<u8> = (0..=255u8).collect();
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(&data).unwrap();
        f.flush().unwrap();
        let archive = Arc::new(PmtilesArchive::open(f.path()).unwrap());
        let mut handles = Vec::new();
        for off in 0..16u64 {
            let a = Arc::clone(&archive);
            handles.push(thread::spawn(move || {
                let got = a.read_at(off, 4).unwrap();
                assert_eq!(got, vec![off as u8, off as u8 + 1, off as u8 + 2, off as u8 + 3]);
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
    }

    #[test]
    fn registry_register_and_get() {
        let data: Vec<u8> = (0..10u8).collect();
        let (_f, archive) = archive_of(&data);
        let reg = PmtilesRegistry::new();
        reg.register("world", Arc::new(archive));
        assert!(reg.get("world").is_some());
        assert_eq!(reg.get("world").unwrap().len(), 10);
        assert!(reg.get("missing").is_none());
    }

    #[test]
    fn parse_range_header_variants() {
        assert_eq!(
            parse_range_header("bytes=0-99"),
            Some(RangeSpec {
                start: 0,
                end_inclusive: Some(99)
            })
        );
        assert_eq!(
            parse_range_header("bytes=100-"),
            Some(RangeSpec {
                start: 100,
                end_inclusive: None
            })
        );
        // multi-range, suffix, malformed, end<start → None (fall back to full body)
        assert_eq!(parse_range_header("bytes=0-10,20-30"), None);
        assert_eq!(parse_range_header("bytes=-500"), None);
        assert_eq!(parse_range_header("kbytes=0-1"), None);
        assert_eq!(parse_range_header("bytes=abc-def"), None);
        assert_eq!(parse_range_header("bytes=50-10"), None);
    }

    #[test]
    fn read_range_no_range_serves_full_body_200() {
        let data: Vec<u8> = (0..20u8).collect();
        let (_f, archive) = archive_of(&data);
        let resp = read_range(&archive, None).unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, data);
        assert_eq!(resp.content_range, None);
        assert_eq!(resp.total_len, 20);
    }

    #[test]
    fn read_range_partial_emits_206_with_content_range() {
        let data: Vec<u8> = (0..20u8).collect();
        let (_f, archive) = archive_of(&data);
        let resp = read_range(
            &archive,
            Some(RangeSpec {
                start: 5,
                end_inclusive: Some(9),
            }),
        )
        .unwrap();
        assert_eq!(resp.status, 206);
        assert_eq!(resp.body, vec![5, 6, 7, 8, 9]);
        assert_eq!(resp.content_range.as_deref(), Some("bytes 5-9/20"));
    }

    #[test]
    fn read_range_open_ended_runs_to_eof() {
        let data: Vec<u8> = (0..20u8).collect();
        let (_f, archive) = archive_of(&data);
        let resp = read_range(
            &archive,
            Some(RangeSpec {
                start: 18,
                end_inclusive: None,
            }),
        )
        .unwrap();
        assert_eq!(resp.status, 206);
        assert_eq!(resp.body, vec![18, 19]);
        assert_eq!(resp.content_range.as_deref(), Some("bytes 18-19/20"));
    }

    #[test]
    fn read_range_end_past_eof_is_clamped() {
        let data: Vec<u8> = (0..20u8).collect();
        let (_f, archive) = archive_of(&data);
        let resp = read_range(
            &archive,
            Some(RangeSpec {
                start: 15,
                end_inclusive: Some(999),
            }),
        )
        .unwrap();
        assert_eq!(resp.status, 206);
        assert_eq!(resp.body, vec![15, 16, 17, 18, 19]);
        assert_eq!(resp.content_range.as_deref(), Some("bytes 15-19/20"));
    }

    #[test]
    fn read_range_start_past_eof_is_416() {
        let data: Vec<u8> = (0..20u8).collect();
        let (_f, archive) = archive_of(&data);
        let resp = read_range(
            &archive,
            Some(RangeSpec {
                start: 20,
                end_inclusive: Some(25),
            }),
        )
        .unwrap();
        assert_eq!(resp.status, 416);
        assert!(resp.body.is_empty());
        assert_eq!(resp.content_range.as_deref(), Some("bytes */20"));
    }

    #[test]
    fn parse_pmtiles_uri_host_form() {
        assert_eq!(
            parse_pmtiles_uri(Some("pmtiles"), "/world"),
            Some("world".to_string())
        );
    }

    #[test]
    fn parse_pmtiles_uri_path_form() {
        assert_eq!(
            parse_pmtiles_uri(Some("localhost"), "/pmtiles/world"),
            Some("world".to_string())
        );
        assert_eq!(
            parse_pmtiles_uri(None, "/pmtiles/region-cascadia"),
            Some("region-cascadia".to_string())
        );
    }

    #[test]
    fn parse_pmtiles_uri_rejects_non_pmtiles() {
        // LAN raster tile request — must fall through to serve_tile.
        assert_eq!(parse_pmtiles_uri(Some("localhost"), "/8/137/89"), None);
        assert_eq!(parse_pmtiles_uri(Some("pmtiles"), "/"), None);
        assert_eq!(parse_pmtiles_uri(Some("pmtiles"), ""), None);
    }
}
