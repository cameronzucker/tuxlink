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

use std::time::Instant;

use super::breaker::Outcome;
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
/// | [`SourceDegraded`] | 503 Service Unavailable | The §8.5 circuit breaker is tripped + cooling: the source failed K consecutive host fetches, so per-tile fetches are short-circuited (no network) until the cooldown expires. The webview serves the bundled raster for these tiles. |
/// | [`Upstream`]   | 502 Bad Gateway | Upstream error: host denied by SSRF policy, redirect, non-image body, size cap, bad URL, network error, or a non-success status. The webview cannot distinguish these and must not learn SSRF-internal detail; they collapse to one "upstream failed" status. |
///
/// [`NoSource`]: ServeError::NoSource
/// [`BadPath`]: ServeError::BadPath
/// [`NotFound`]: ServeError::NotFound
/// [`SourceDegraded`]: ServeError::SourceDegraded
/// [`Upstream`]: ServeError::Upstream
#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    /// No active source is configured on the gatekeeper.
    #[error("no tile source configured")]
    NoSource,
    /// The §8.5 circuit breaker is tripped + cooling. Per-tile fetches are
    /// short-circuited (NO network) so a dead source can't storm timeouts; the
    /// webview serves the bundled raster for the affected tiles until the
    /// cooldown expires and a re-probe re-arms the source.
    #[error("tile source degraded; serving bundled (circuit breaker open)")]
    SourceDegraded,
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
//
// §8.5 circuit breaker: BEFORE any network I/O the breaker is consulted. A
// degraded+cooling source short-circuits to [`ServeError::SourceDegraded`] (no
// fetch — the webview serves bundled). Otherwise the fetch outcome feeds the
// breaker: a success resets it, a host-failure increments it (tripping at K),
// and a 404 marks the source partial WITHOUT incrementing (§8.5: a 404 above
// raster-native zoom is a coverage gap, not a source-health failure).
pub async fn serve_tile(
    gk: &TileGatekeeper,
    path: &str,
    allow_loopback: bool,
) -> Result<(Vec<u8>, &'static str), ServeError> {
    serve_tile_with(gk, path, Instant::now(), |cache_root, source, coord| async move {
        fetch_tile_single_flight(&cache_root, &source, &coord, allow_loopback).await
    })
    .await
}

/// `serve_tile` with the clock and the fetch step injected (the test seam for
/// Task 9.1). Production [`serve_tile`] passes `Instant::now()` and the real
/// single-flight fetch; tests pass a hand-advanced instant and a counting /
/// failing fetch closure to prove the breaker short-circuits without fetching.
///
/// The `fetch` closure receives OWNED `(cache_root, source, coord)` so it can be
/// a simple `FnOnce(…) -> impl Future` without higher-ranked-lifetime gymnastics
/// (borrowing across the `.await` would force an HRTB bound the closure cannot
/// satisfy). The clones are cheap relative to a network fetch.
pub(crate) async fn serve_tile_with<F, Fut>(
    gk: &TileGatekeeper,
    path: &str,
    now: Instant,
    fetch: F,
) -> Result<(Vec<u8>, &'static str), ServeError>
where
    F: FnOnce(std::path::PathBuf, super::TileSource, TileCoord) -> Fut,
    Fut: std::future::Future<Output = Result<(Vec<u8>, &'static str), FetchError>>,
{
    let source = gk.active_source().ok_or(ServeError::NoSource)?;
    let (z, x, y) = parse_zxy(path)?;
    let coord = TileCoord::from_parts(&z, &x, &y, source.max_zoom).map_err(ServeError::BadPath)?;

    // §8.5: consult the breaker BEFORE fetching. Degraded+cooling → no network.
    if !gk.breaker_should_attempt(now) {
        return Err(ServeError::SourceDegraded);
    }

    let result = fetch(gk.cache_root().to_path_buf(), source, coord).await;

    // Feed the outcome back to the breaker, classifying 404-vs-host (§8.5).
    let outcome = match &result {
        Ok(_) => Outcome::Success,
        // A 404 above coverage is a coverage gap, NOT a host-health failure:
        // it marks the source partial but does not increment the breaker.
        Err(FetchError::NotFound) => Outcome::Coverage,
        // Everything else (HostDenied/Redirect/Status/Network/TooLarge/
        // NotAnImage/BadUrl) is a host-level failure that counts toward the trip.
        Err(_) => Outcome::HostFailure,
    };
    gk.breaker_record(outcome, now);

    result.map_err(map_fetch_error)
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

    // ── Task 9.1: circuit-breaker integration via the injected seam ──────────

    use crate::tiles::breaker::{BreakerHealth, COOLDOWN, FAILURE_THRESHOLD};
    use std::cell::Cell;
    use std::time::Instant;

    /// The result shape a fetch seam produces.
    type FetchOutcome = Result<(Vec<u8>, &'static str), FetchError>;

    /// A fetch seam that returns a fixed result and counts how many times it was
    /// invoked. The count is the ground truth for "did serve_tile short-circuit
    /// without fetching?" (`Cell` is fine: these tests are single-threaded).
    struct CountingFetch {
        calls: Cell<u32>,
        result: fn() -> FetchOutcome,
    }
    impl CountingFetch {
        fn new(result: fn() -> FetchOutcome) -> Self {
            CountingFetch {
                calls: Cell::new(0),
                result,
            }
        }
        async fn serve(
            &self,
            gk: &TileGatekeeper,
            now: Instant,
        ) -> Result<(Vec<u8>, &'static str), ServeError> {
            serve_tile_with(gk, "/3/5/2", now, |_root, _src, _coord| {
                self.calls.set(self.calls.get() + 1);
                let r = (self.result)();
                async move { r }
            })
            .await
        }
    }

    /// A fetch seam that PANICS if invoked — proves a degraded breaker
    /// short-circuits with NO fetch attempted.
    async fn panicking_serve(
        gk: &TileGatekeeper,
        now: Instant,
    ) -> Result<(Vec<u8>, &'static str), ServeError> {
        serve_tile_with(gk, "/3/5/2", now, |_root, _src, _coord| async move {
            panic!("breaker degraded: serve_tile must NOT fetch")
        })
        .await
    }

    fn ok_png() -> Result<(Vec<u8>, &'static str), FetchError> {
        Ok((png_bytes(), "image/png"))
    }
    fn host_fail() -> Result<(Vec<u8>, &'static str), FetchError> {
        Err(FetchError::Network("simulated host failure".into()))
    }
    fn not_found() -> Result<(Vec<u8>, &'static str), FetchError> {
        Err(FetchError::NotFound)
    }

    #[tokio::test]
    async fn three_host_failures_trip_breaker_then_short_circuit_without_fetching() {
        let (gk, _dir) = gatekeeper(Some(source("http://192.168.1.5:8080/")));
        let now = Instant::now();
        let f = CountingFetch::new(host_fail);
        // 3 consecutive host failures: each DOES fetch (and fails).
        for _ in 0..FAILURE_THRESHOLD {
            let _ = f.serve(&gk, now).await;
        }
        assert_eq!(f.calls.get(), FAILURE_THRESHOLD, "first K attempts all fetch");
        assert_eq!(gk.breaker_health(now), BreakerHealth::Degraded);

        // Now degraded + cooling: the NEXT serve must short-circuit WITHOUT
        // calling the fetch seam. Use a seam that would PANIC if invoked to
        // prove no fetch happens.
        let before = f.calls.get();
        let err = panicking_serve(&gk, now).await.unwrap_err();
        assert!(matches!(err, ServeError::SourceDegraded), "got {err:?}");
        assert_eq!(f.calls.get(), before, "no new fetch while degraded");
    }

    #[tokio::test]
    async fn success_resets_counter_so_two_plus_one_plus_two_does_not_trip() {
        let (gk, _dir) = gatekeeper(Some(source("http://192.168.1.5:8080/")));
        let now = Instant::now();
        let fail = CountingFetch::new(host_fail);
        let ok = CountingFetch::new(ok_png);
        // 2 host failures …
        let _ = fail.serve(&gk, now).await;
        let _ = fail.serve(&gk, now).await;
        // … 1 success (resets the consecutive run) …
        ok.serve(&gk, now).await.unwrap();
        // … 2 more host failures: total 4 failures, but never 3 in a row.
        let _ = fail.serve(&gk, now).await;
        let _ = fail.serve(&gk, now).await;
        assert_eq!(
            gk.breaker_health(now),
            BreakerHealth::Live,
            "interleaved success prevents the trip"
        );
        // Proof it still fetches (not degraded):
        let probe = CountingFetch::new(ok_png);
        probe.serve(&gk, now).await.unwrap();
        assert_eq!(probe.calls.get(), 1, "live source still fetches");
    }

    #[tokio::test]
    async fn a_404_does_not_increment_the_breaker() {
        let (gk, _dir) = gatekeeper(Some(source("http://192.168.1.5:8080/")));
        let now = Instant::now();
        let f = CountingFetch::new(not_found);
        // A flood of 404s: every one returns NotFound and NONE trips the breaker.
        for _ in 0..(FAILURE_THRESHOLD + 5) {
            let err = f.serve(&gk, now).await.unwrap_err();
            assert!(matches!(err, ServeError::NotFound), "got {err:?}");
        }
        assert_eq!(
            gk.breaker_health(now),
            BreakerHealth::Live,
            "404s are coverage gaps, never trip the breaker"
        );
        assert!(gk.is_partial_coverage(), "404 marks the source partial");
        // And the fetch seam was hit every time (no short-circuit).
        assert_eq!(f.calls.get(), FAILURE_THRESHOLD + 5);
    }

    #[tokio::test]
    async fn after_cooldown_one_reprobe_is_allowed_then_recovers_on_success() {
        let (gk, _dir) = gatekeeper(Some(source("http://192.168.1.5:8080/")));
        let now = Instant::now();
        let fail = CountingFetch::new(host_fail);
        for _ in 0..FAILURE_THRESHOLD {
            let _ = fail.serve(&gk, now).await;
        }
        assert_eq!(gk.breaker_health(now), BreakerHealth::Degraded);

        // After the cooldown, exactly one re-probe is authorized.
        let after = now + COOLDOWN;
        let probe = CountingFetch::new(ok_png);
        probe.serve(&gk, after).await.unwrap();
        assert_eq!(probe.calls.get(), 1, "one re-probe fetched after cooldown");
        assert_eq!(
            gk.breaker_health(after),
            BreakerHealth::Live,
            "successful re-probe resets to live"
        );
    }

    #[tokio::test]
    async fn changing_source_resets_a_degraded_breaker() {
        let (gk, _dir) = gatekeeper(Some(source("http://192.168.1.5:8080/")));
        let now = Instant::now();
        let fail = CountingFetch::new(host_fail);
        for _ in 0..FAILURE_THRESHOLD {
            let _ = fail.serve(&gk, now).await;
        }
        assert_eq!(gk.breaker_health(now), BreakerHealth::Degraded);
        // Reconfiguring the source clears the stale degraded state.
        gk.set_source(Some(source("http://192.168.1.9:8080/")));
        assert_eq!(
            gk.breaker_health(now),
            BreakerHealth::Live,
            "a new source starts with a fresh breaker"
        );
        assert!(!gk.is_partial_coverage(), "partial flag cleared on source change");
    }
}
