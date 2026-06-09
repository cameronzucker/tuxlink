//! `tiles::serve` — the serving mechanism behind the bespoke `tile` URI scheme.
//!
//! ## SSRF (§8.3) boundary
//!
//! [`serve_tile`] is the webview→backend entry point for tile bytes. The
//! webview requests a tile via a Leaflet `TileLayer` whose template is
//! `tile://localhost/{z}/{x}/{y}` (Linux/WebKitGTK form — see
//! `docs/plans/dyop-phase0-csp-spike.md`). The async URI-scheme handler in
//! `lib.rs` extracts the URL path and hands it here as a `{z}/{x}/{y}` string.
//!
//! This function NEVER accepts a caller-supplied full URL. It only parses three
//! integer path segments, validates them against the stored source's `max_zoom`
//! via [`TileCoord::from_parts`] (which rejects non-integers, negatives, and
//! out-of-range zoom before any `2^z` is computed), and delegates to the
//! SSRF-guarded [`fetch_tile_single_flight`]. Only integer `{z}/{x}/{y}` and the
//! operator-configured [`TileSource`] reach the network egress; the gatekeeper
//! in `fetch.rs` remains the sole tile network egress.

use super::coord::TileCoord;
use super::fetch::{fetch_tile_single_flight, FetchError};
use super::TileGatekeeper;

/// Errors surfaced by [`serve_tile`].
///
/// Maps the parse / no-source / fetch cases into a small enum the `tile`-scheme
/// handler in `lib.rs` translates to an HTTP status:
///
/// | Variant       | HTTP status | Meaning |
/// |---------------|-------------|---------|
/// | [`NoSource`]   | 404 Not Found | No source configured — nothing to serve. |
/// | [`BadPath`]    | 400 Bad Request | Path was not a valid `{z}/{x}/{y}` triple, or the coordinate failed `TileCoord` validation (out-of-range / non-integer / over-max-zoom). |
/// | [`NotFound`]   | 404 Not Found | Upstream returned 404 for this tile. |
/// | [`Upstream`]   | 502 Bad Gateway | Upstream error: host denied by SSRF policy, redirect, non-image body, size cap, bad URL, network error, or a non-success status. The webview cannot distinguish these and must not learn SSRF-internal detail; they collapse to one "upstream failed" status. |
///
/// [`NoSource`]: ServeError::NoSource
/// [`BadPath`]: ServeError::BadPath
/// [`NotFound`]: ServeError::NotFound
/// [`Upstream`]: ServeError::Upstream
#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    /// No active source is configured on the gatekeeper.
    #[error("no tile source configured")]
    NoSource,
    /// The request path was not a valid `{z}/{x}/{y}` triple, or the parsed
    /// coordinate failed validation.
    #[error("bad tile path: {0}")]
    BadPath(String),
    /// The upstream returned 404 for this specific tile.
    #[error("tile not found")]
    NotFound,
    /// Any other upstream / gatekeeper failure. The detail is for logs only;
    /// the webview sees an opaque 502.
    #[error("upstream tile fetch failed: {0}")]
    Upstream(String),
}

/// Parse a request path into its three `{z}/{x}/{y}` string segments.
///
/// Accepts a leading `/` (the URI handler passes `request.uri().path()` which
/// begins with `/`). The final segment may be either `{y}` or `{y}.png` (Leaflet
/// templates commonly append `.png`); a trailing `.png` is stripped. Exactly
/// three non-empty segments are required — anything else is a `BadPath`. The
/// segments are returned as raw strings; integer validation is `TileCoord`'s job.
fn parse_zxy(path: &str) -> Result<(String, String, String), ServeError> {
    let trimmed = path.strip_prefix('/').unwrap_or(path);
    let parts: Vec<&str> = trimmed.split('/').collect();
    if parts.len() != 3 || parts.iter().any(|p| p.is_empty()) {
        return Err(ServeError::BadPath(format!(
            "expected z/x/y, got {path:?}"
        )));
    }
    let z = parts[0].to_string();
    let x = parts[1].to_string();
    // Strip an optional `.png` suffix on the y segment.
    let y = parts[2].strip_suffix(".png").unwrap_or(parts[2]).to_string();
    if y.is_empty() {
        return Err(ServeError::BadPath(format!("empty y in {path:?}")));
    }
    Ok((z, x, y))
}

/// Map a [`FetchError`] from the gatekeeper into a [`ServeError`].
///
/// Only `NotFound` keeps its identity (→ 404); every other variant — including
/// `HostDenied` (SSRF) — collapses to [`ServeError::Upstream`] so the webview
/// never learns which SSRF defense fired or any other internal detail.
fn map_fetch_error(e: FetchError) -> ServeError {
    match e {
        FetchError::NotFound => ServeError::NotFound,
        other => ServeError::Upstream(other.to_string()),
    }
}

/// Serve a single tile: parse the path → validate the coordinate against the
/// active source's `max_zoom` → run the SSRF-guarded fetch/cache pipeline.
///
/// `path` is the `{z}/{x}/{y}` (or `{z}/{x}/{y}.png`) segment of the `tile://`
/// URL, as extracted from the request URI by the handler in `lib.rs`. A leading
/// `/` is tolerated.
///
/// `allow_loopback` is `false` in production (the handler passes `false`); tests
/// pass `true` to exercise the happy path against a loopback-bound mock server.
///
/// On success returns `(body_bytes, image_mime)` where `image_mime` is derived
/// from the validated magic bytes (NOT the upstream `Content-Type`).
///
// SSRF (§8.3) boundary: this is the webview→backend tile entry. Only integer
// z/x/y (validated by TileCoord) and the stored TileSource reach the gatekeeper;
// no caller-supplied URL is ever accepted here.
pub async fn serve_tile(
    gk: &TileGatekeeper,
    path: &str,
    allow_loopback: bool,
) -> Result<(Vec<u8>, &'static str), ServeError> {
    let source = gk.active_source().ok_or(ServeError::NoSource)?;
    let (z, x, y) = parse_zxy(path)?;
    let coord = TileCoord::from_parts(&z, &x, &y, source.max_zoom).map_err(ServeError::BadPath)?;
    fetch_tile_single_flight(gk.cache_root(), &source, &coord, allow_loopback)
        .await
        .map_err(map_fetch_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tiles::{Crs, TileScheme, TileSource};
    use tempfile::TempDir;

    fn png_bytes() -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
        v.extend_from_slice(&[0u8; 32]);
        v
    }

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

    /// A gatekeeper with a fresh temp cache root + an optional active source.
    /// Returns the `TempDir` guard so the cache root survives the test body.
    fn gatekeeper(src: Option<TileSource>) -> (TileGatekeeper, TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let gk = TileGatekeeper::new(dir.path());
        gk.set_source(src);
        (gk, dir)
    }

    #[tokio::test]
    async fn happy_path_returns_bytes_and_mime() {
        // A configured source pointing at a mock server that returns a PNG.
        // allow_loopback=true exercises the happy IP-literal path (mockito binds
        // loopback). serve_tile runs parse → from_parts → fetch_tile_single_flight.
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("GET", "/3/5/2.png")
            .with_status(200)
            .with_header("content-type", "image/png")
            .with_body(png_bytes())
            .create_async()
            .await;
        let (gk, _dir) = gatekeeper(Some(source(&server.url())));
        let (bytes, mime) = serve_tile(&gk, "/3/5/2", true).await.unwrap();
        m.assert_async().await;
        assert_eq!(mime, "image/png");
        assert!(bytes.starts_with(b"\x89PNG"));
    }

    #[tokio::test]
    async fn happy_path_accepts_png_suffix() {
        // The Leaflet template may append `.png`; serve must accept `{y}.png`.
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/3/5/2.png")
            .with_status(200)
            .with_body(png_bytes())
            .create_async()
            .await;
        let (gk, _dir) = gatekeeper(Some(source(&server.url())));
        let (bytes, mime) = serve_tile(&gk, "/3/5/2.png", true).await.unwrap();
        assert_eq!(mime, "image/png");
        assert!(bytes.starts_with(b"\x89PNG"));
    }

    #[tokio::test]
    async fn no_source_configured_is_no_source_error() {
        // No active source → documented NoSource error, never a panic.
        let (gk, _dir) = gatekeeper(None);
        let err = serve_tile(&gk, "/3/5/2", true).await.unwrap_err();
        assert!(matches!(err, ServeError::NoSource), "got {err:?}");
    }

    #[tokio::test]
    async fn traversal_path_is_bad_path_not_a_read() {
        // A hostile path must map to BadPath, never a panic and never a traversal.
        let (gk, _dir) = gatekeeper(Some(source("http://192.168.1.5:8080/tiles/")));
        let err = serve_tile(&gk, "/../../etc/passwd", true)
            .await
            .unwrap_err();
        assert!(matches!(err, ServeError::BadPath(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn too_few_segments_is_bad_path() {
        let (gk, _dir) = gatekeeper(Some(source("http://192.168.1.5:8080/tiles/")));
        let err = serve_tile(&gk, "/3/5", true).await.unwrap_err();
        assert!(matches!(err, ServeError::BadPath(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn non_integer_segment_is_bad_path() {
        // `3/5/x` parses into 3 segments but TileCoord::from_parts rejects `x`.
        let (gk, _dir) = gatekeeper(Some(source("http://192.168.1.5:8080/tiles/")));
        let err = serve_tile(&gk, "/3/5/x", true).await.unwrap_err();
        assert!(matches!(err, ServeError::BadPath(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn zoom_over_source_max_is_bad_path() {
        // z=40 is over the source max_zoom (19) → TileCoord rejects → BadPath,
        // not a panic and not a fetch.
        let (gk, _dir) = gatekeeper(Some(source("http://192.168.1.5:8080/tiles/")));
        let err = serve_tile(&gk, "/40/0/0", true).await.unwrap_err();
        assert!(matches!(err, ServeError::BadPath(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn upstream_404_maps_to_not_found() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/3/5/2.png")
            .with_status(404)
            .create_async()
            .await;
        let (gk, _dir) = gatekeeper(Some(source(&server.url())));
        let err = serve_tile(&gk, "/3/5/2", true).await.unwrap_err();
        assert!(matches!(err, ServeError::NotFound), "got {err:?}");
    }

    #[tokio::test]
    async fn denied_host_maps_to_upstream_error() {
        // A public IP-literal source → fetch HostDenied → collapses to Upstream
        // (the webview must not learn the SSRF detail). allow_loopback irrelevant;
        // a public IP is denied regardless. No network I/O occurs.
        let (gk, _dir) = gatekeeper(Some(source("http://8.8.8.8:8080/")));
        let err = serve_tile(&gk, "/3/5/2", false).await.unwrap_err();
        assert!(matches!(err, ServeError::Upstream(_)), "got {err:?}");
    }
}
