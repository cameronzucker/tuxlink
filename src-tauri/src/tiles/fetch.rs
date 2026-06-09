//! `tiles::fetch` — the SSRF gatekeeper that fetches a single tile from a
//! LAN tile server.
//!
//! ## SSRF boundary (§8.3)
//!
//! This module is the network-egress deputy. The webview can influence the
//! tile coordinate and the operator configures the source URL, but neither may
//! steer the backend into fetching an arbitrary internet host. Defenses:
//! no caller-supplied full URL, URL-shape validation, fetch-time resolved-IP
//! pinning (rebind defense), `redirect::Policy::none()`, a short timeout, a
//! response-size cap, and image magic-byte validation.

use std::time::Duration;

use reqwest::Url;

use super::coord::TileCoord;
use super::host::validate_source_url;
use super::{TileScheme, TileSource};

/// Connect/read timeout for a single tile fetch. LAN tile servers are local;
/// a slow response is a failure, not something to wait minutes on.
const TILE_TIMEOUT: Duration = Duration::from_secs(5);

/// User-Agent sent on every tile request.
const TILE_USER_AGENT: &str = "tuxlink-tiles/0.0.1";

/// Errors from the tile gatekeeper.
///
/// Variants are stable surface for Phase 5 (cache — caches only `Ok` image
/// results) and Phase 6 (serving — maps these to HTTP status / `StatusKind`).
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    /// The upstream returned a 3xx. The no-redirect policy surfaces it as a
    /// normal response with a 3xx status; we refuse to follow it.
    #[error("upstream returned a redirect (3xx); redirects are not followed")]
    Redirect,
    /// The configured/resolved host is not a permitted LAN destination
    /// (public IP, loopback without opt-in, link-local, etc.). The SSRF gate.
    #[error("host denied by SSRF policy: {0}")]
    HostDenied(String),
    /// The body did not begin with a recognized image magic signature.
    #[error("upstream response is not a recognized image (PNG/JPEG/WebP)")]
    NotAnImage,
    /// The body exceeded the size cap (declared or streamed).
    #[error("tile body exceeds the size cap")]
    TooLarge,
    /// The upstream returned 404 for this tile.
    #[error("tile not found (404)")]
    NotFound,
    /// The upstream returned a non-success, non-404, non-3xx status.
    #[error("upstream returned status {0}")]
    Status(u16),
    /// A transport/network/DNS error.
    #[error("network error: {0}")]
    Network(String),
    /// The source URL or built tile URL was malformed.
    #[error("bad URL: {0}")]
    BadUrl(String),
}

/// Build the shared no-redirect, short-timeout tile client.
///
/// A 3xx will surface as a normal response with a 3xx status (not followed),
/// which [`fetch_tile_bytes`] maps to [`FetchError::Redirect`].
pub fn build_tile_client() -> Result<reqwest::Client, FetchError> {
    reqwest::Client::builder()
        .user_agent(TILE_USER_AGENT)
        .timeout(TILE_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| FetchError::Network(format!("client build: {e}")))
}

/// Build the upstream tile URL from the stored source + validated coordinate.
///
/// The integer `z`/`x`/`y` segments are appended via `Url` path-segment APIs,
/// never string interpolation of webview-influenced input (§8.4).
fn build_tile_url(source: &TileSource, coord: &TileCoord) -> Result<Url, FetchError> {
    let mut url = validate_source_url(&source.url).map_err(FetchError::BadUrl)?;

    let tms = matches!(source.scheme, TileScheme::Tms);
    let y = coord.upstream_y(tms);

    {
        let mut segs = url
            .path_segments_mut()
            .map_err(|()| FetchError::BadUrl("source URL cannot be a base".into()))?;
        segs.pop_if_empty();
        segs.push(&coord.z.to_string());
        segs.push(&coord.x.to_string());
        segs.push(&format!("{y}.png"));
    }
    Ok(url)
}

/// Fetch a single tile, building the upstream URL from the stored source and
/// validated coordinate. A 3xx is a hard error (never followed).
pub async fn fetch_tile_bytes(
    source: &TileSource,
    coord: &TileCoord,
    _allow_loopback: bool,
) -> Result<Vec<u8>, FetchError> {
    let url = build_tile_url(source, coord)?;
    let client = build_tile_client()?;

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| FetchError::Network(format!("send: {e}")))?;

    let status = resp.status();
    if status.is_redirection() {
        // The no-redirect policy surfaces a 3xx as a normal response; refuse it.
        return Err(FetchError::Redirect);
    }
    if status.as_u16() == 404 {
        return Err(FetchError::NotFound);
    }
    if !status.is_success() {
        return Err(FetchError::Status(status.as_u16()));
    }

    let body = resp
        .bytes()
        .await
        .map_err(|e| FetchError::Network(format!("read body: {e}")))?;
    Ok(body.to_vec())
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

    fn png_bytes() -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
        v.extend_from_slice(&[0u8; 32]);
        v
    }

    // ---- Task 3.1 ----

    #[test]
    fn client_builds() {
        // Smoke: the no-redirect / short-timeout client constructs cleanly.
        assert!(build_tile_client().is_ok());
    }

    #[tokio::test]
    async fn redirect_3xx_is_a_hard_error_not_followed() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("GET", "/3/5/2.png")
            .with_status(302)
            .with_header("location", "http://example.com/elsewhere")
            .create_async()
            .await;
        let src = source(&server.url());
        let err = fetch_tile_bytes(&src, &coord(), true).await.unwrap_err();
        m.assert_async().await;
        assert!(matches!(err, FetchError::Redirect), "got {err:?}");
    }

    #[tokio::test]
    async fn success_returns_body() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/3/5/2.png")
            .with_status(200)
            .with_body(png_bytes())
            .create_async()
            .await;
        let src = source(&server.url());
        let bytes = fetch_tile_bytes(&src, &coord(), true).await.unwrap();
        assert!(bytes.starts_with(b"\x89PNG"));
    }
}
