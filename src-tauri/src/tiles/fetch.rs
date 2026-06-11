//! `tiles::fetch` — the SSRF gatekeeper that fetches a single tile from a
//! LAN tile server.
//!
//! ## SSRF boundary (§8.3)
//!
//! This module is the network-egress deputy. The webview can influence the
//! tile coordinate (`z`/`x`/`y`, validated upstream by [`crate::tiles::coord`])
//! and the operator configures the source URL, but neither may steer the
//! backend into fetching an arbitrary internet host. The defenses, in order:
//!
//! 1. **No caller-supplied full URL.** [`fetch_tile_bytes`] takes a stored
//!    [`TileSource`] plus a validated [`TileCoord`] and builds the upstream URL
//!    itself, using `Url` path-segment APIs (never string interpolation of
//!    webview input).
//! 2. **URL-shape validation** via [`crate::tiles::host::validate_source_url`]
//!    (http/https only, no embedded creds, host present).
//! 3. **Fetch-time resolved-IP pinning** (the rebinding defense):
//!    - If the URL host is an **IP literal**, there is no DNS to rebind — the
//!      literal is vetted directly with [`crate::tiles::host::ip_is_permitted`].
//!    - If the URL host is a **name**, the name is resolved *at fetch time* and
//!      EVERY resolved address must pass `ip_is_permitted`; the per-fetch client
//!      then pins the connection to exactly that vetted address set via
//!      `reqwest`'s `resolve_to_addrs`, so the socket can only reach the IPs we
//!      validated (a TOCTOU rebind to a public IP between our lookup and
//!      reqwest's connect cannot occur — reqwest does not re-resolve).
//! 4. **`redirect::Policy::none()`** — a 3xx is a hard error, never followed
//!    (a redirect is a classic SSRF pivot).
//! 5. **`.no_proxy()`** on every tile/probe client — `resolve_to_addrs` pins the
//!    target HOST's connection, NOT a proxy. If reqwest honored
//!    `HTTP(S)_PROXY`/system proxy, a permitted LAN source's TCP connection would
//!    be opened to a public proxy instead, defeating the LAN-only egress
//!    guarantee. `.no_proxy()` forces a direct connection to the pinned address.
//! 6. **Short timeout** (5 s).
//! 7. **Response size cap** ([`MAX_TILE_BYTES`]) enforced via both the
//!    `Content-Length` pre-check AND a streaming running-total abort (the
//!    server may lie about / omit `Content-Length`).
//! 8. **Image magic-byte validation** — the leading bytes must be a real
//!    PNG/JPEG/WebP signature; the upstream `Content-Type` is NOT trusted.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use futures::future::{BoxFuture, FutureExt, Shared};
use once_cell::sync::Lazy;
use reqwest::Url;

use super::cache;
use super::coord::TileCoord;
use super::host::{ip_is_permitted, validate_source_url};
use super::{TileScheme, TileSource};

/// Hard cap on a single tile's body size. A 256×256 tile is well under this;
/// the cap exists to bound peak memory against a hostile / misconfigured
/// server that streams an unbounded body.
pub const MAX_TILE_BYTES: u64 = 2 * 1024 * 1024;

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
    /// The body exceeded [`MAX_TILE_BYTES`] (declared or streamed).
    #[error("tile body exceeds size cap of {MAX_TILE_BYTES} bytes")]
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

/// MIME type of a fetched tile, derived from the validated magic bytes (NOT
/// from the upstream `Content-Type`, which is not trusted).
///
/// `pub(crate)` so the cache layer reuses the SAME magic check when deciding
/// whether a byte slice is cacheable (cache-only-good, §8.4) — a single source
/// of truth for "is this a real image."
pub(crate) fn image_mime_from_magic(bytes: &[u8]) -> Option<&'static str> {
    // PNG: 89 50 4E 47 0D 0A 1A 0A
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
    // JPEG: FF D8 FF
    const JPEG: &[u8] = b"\xFF\xD8\xFF";
    if bytes.starts_with(PNG) {
        return Some("image/png");
    }
    if bytes.starts_with(JPEG) {
        return Some("image/jpeg");
    }
    // WebP: "RIFF" <4-byte size> "WEBP"
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    None
}

/// Build the shared no-redirect, short-timeout, no-proxy tile client.
///
/// Used directly for the IP-literal host case (no DNS pinning needed). For
/// named hosts, a *per-fetch* client is built with the same options plus
/// `resolve_to_addrs` pinning (see [`build_vetted_client`]).
///
/// `.no_proxy()` is load-bearing for the SSRF guarantee — see the module-level
/// defense (4a) and [`build_vetted_client`].
pub fn build_tile_client() -> Result<reqwest::Client, FetchError> {
    reqwest::Client::builder()
        .user_agent(TILE_USER_AGENT)
        .timeout(TILE_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        // SSRF (§8.3): never honor an ambient HTTP(S)_PROXY / system proxy. A
        // proxy would open the connection to the proxy host, not the vetted LAN
        // IP, so the LAN-only egress gate would be bypassed entirely.
        .no_proxy()
        .build()
        .map_err(|e| FetchError::Network(format!("client build: {e}")))
}

/// Vet a tile source's host against the SSRF policy and build the reqwest client
/// that will reach it — the SINGLE shared egress chokepoint for the tile fetch
/// ([`fetch_tile_bytes_with_resolver`]).
///
/// Vetting branches on the source URL's host type (§8.3):
/// - **IP literal** (`http://192.168.1.5:8080/`, `http://[fd00::1]:8080/`): there
///   is no DNS to rebind — the literal is vetted directly via
///   [`ip_is_permitted`]. The shared no-redirect/no-proxy client is returned.
/// - **Named host** (`https://tiles.lan/`): resolved via `resolve` AT THIS POINT,
///   EVERY resolved address must pass [`ip_is_permitted`] (reject mixed/any-public
///   — no single-addr cherry-pick), and the returned client is PINNED to exactly
///   that vetted address set via `resolve_to_addrs`. reqwest does not re-resolve a
///   pinned host, so the TOCTOU rebind window between our lookup and reqwest's
///   connect is closed.
///
/// Every returned client carries `redirect::none()`, the short timeout, AND
/// `.no_proxy()` (Findings 1+2): the probe and the fetch share the identical
/// egress discipline, so a host that would be denied for a tile fetch is denied
/// for the metadata probe too — the probe can no longer connect to a public host
/// ahead of the gate.
pub(crate) async fn build_vetted_client<R, Fut>(
    source: &TileSource,
    allow_loopback: bool,
    resolve: R,
) -> Result<reqwest::Client, FetchError>
where
    R: Fn(String, u16) -> Fut,
    Fut: std::future::Future<Output = std::io::Result<Vec<SocketAddr>>>,
{
    // Shape-validate the source URL (scheme, no creds, host present) and extract
    // the host + effective port for the gate.
    let url = validate_source_url(&source.url).map_err(FetchError::BadUrl)?;
    let host = url
        .host_str()
        .ok_or_else(|| FetchError::BadUrl("source URL has no host".into()))?
        .to_string();
    let port = url
        .port_or_known_default()
        .ok_or_else(|| FetchError::BadUrl("source URL has no known port".into()))?;

    // `Url::host_str` returns IPv6 literals bracketed (`[fd00::1]`), which does
    // not parse as `IpAddr`; strip the brackets so BOTH v4 and v6 literals take
    // the direct-vet branch (a v6 literal must not be misrouted to the resolver).
    let host_for_ip = host
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(host.as_str());

    match host_for_ip.parse::<std::net::IpAddr>() {
        Ok(ip) => {
            // IP-literal host: no DNS to rebind, vet the literal directly.
            if !ip_is_permitted(ip, allow_loopback) {
                return Err(FetchError::HostDenied(format!(
                    "IP literal {ip} is not a permitted LAN destination"
                )));
            }
            build_tile_client()
        }
        Err(_) => {
            // Named host: resolve at this point, require EVERY resolved address to
            // pass the policy, then PIN the connection to that vetted set.
            let resolved = resolve(host.clone(), port)
                .await
                .map_err(|e| FetchError::Network(format!("DNS resolution of {host:?}: {e}")))?;
            if resolved.is_empty() {
                return Err(FetchError::HostDenied(format!(
                    "host {host:?} resolved to no addresses"
                )));
            }
            for addr in &resolved {
                if !ip_is_permitted(addr.ip(), allow_loopback) {
                    return Err(FetchError::HostDenied(format!(
                        "host {host:?} resolved to non-LAN address {}",
                        addr.ip()
                    )));
                }
            }
            reqwest::Client::builder()
                .user_agent(TILE_USER_AGENT)
                .timeout(TILE_TIMEOUT)
                .redirect(reqwest::redirect::Policy::none())
                // SSRF (§8.3) — see build_tile_client: a proxy would defeat the
                // resolve_to_addrs pin (it pins the host, not the proxy).
                .no_proxy()
                .resolve_to_addrs(&host, &resolved)
                .build()
                .map_err(|e| FetchError::Network(format!("client build: {e}")))
        }
    }
}

/// Build the upstream tile URL from the stored source + validated coordinate.
///
/// The integer `z`/`x`/`y` segments are appended via `Url` path-segment APIs,
/// never string interpolation of webview-influenced input (§8.4). The source
/// URL's existing path is treated as a base directory: a trailing-slash
/// difference is normalized so `…/tiles` and `…/tiles/` both yield
/// `…/tiles/{z}/{x}/{y}.png`.
fn build_tile_url(source: &TileSource, coord: &TileCoord) -> Result<Url, FetchError> {
    // Shape-validate the source URL (scheme, no creds, host present).
    let mut url = validate_source_url(&source.url).map_err(FetchError::BadUrl)?;

    let tms = matches!(source.scheme, TileScheme::Tms);
    let y = coord.upstream_y(tms);

    {
        let mut segs = url
            .path_segments_mut()
            .map_err(|()| FetchError::BadUrl("source URL cannot be a base".into()))?;
        // Drop a trailing empty segment (from a trailing slash) so we don't get
        // an empty path component before the z/x/y triple.
        segs.pop_if_empty();
        segs.push(&coord.z.to_string());
        segs.push(&coord.x.to_string());
        segs.push(&format!("{y}.png"));
    }
    Ok(url)
}

/// Production resolver: resolve `host:port` to a list of `SocketAddr` via the
/// platform resolver (blocking `to_socket_addrs`, run on the blocking pool).
async fn system_resolve(host: &str, port: u16) -> std::io::Result<Vec<SocketAddr>> {
    let target = format!("{host}:{port}");
    tokio::net::lookup_host(target).await.map(|it| it.collect())
}

/// Fetch a single tile, building the upstream URL from the stored source and
/// validated coordinate. Public entry point; uses the system resolver.
///
/// On success returns `(body_bytes, image_mime)` where `image_mime` is derived
/// from the validated magic bytes.
pub async fn fetch_tile_bytes(
    source: &TileSource,
    coord: &TileCoord,
    allow_loopback: bool,
) -> Result<(Vec<u8>, &'static str), FetchError> {
    fetch_tile_bytes_with_resolver(source, coord, allow_loopback, |host, port| async move {
        system_resolve(&host, port).await
    })
    .await
}

/// Core fetch with an injectable resolver seam (for tests).
///
/// `resolve` maps `(host, port)` to candidate `SocketAddr`s. Production passes
/// the system resolver; tests inject a fake to prove a name that resolves to a
/// PUBLIC IP is rejected (the DNS-rebind defense).
pub async fn fetch_tile_bytes_with_resolver<R, Fut>(
    source: &TileSource,
    coord: &TileCoord,
    allow_loopback: bool,
    resolve: R,
) -> Result<(Vec<u8>, &'static str), FetchError>
where
    R: Fn(String, u16) -> Fut,
    Fut: std::future::Future<Output = std::io::Result<Vec<SocketAddr>>>,
{
    let url = build_tile_url(source, coord)?;

    // SSRF (§8.3): the resolved-IP gate. The same vet-and-build logic the CRS
    // probe uses (Findings 1+2) — IP-literal → direct vet; named → resolve, vet
    // EVERY address, pin via resolve_to_addrs; always redirect::none + no_proxy.
    // Because the tile URL is built from the source URL (same host/port), vetting
    // the source is equivalent to vetting the tile URL's destination.
    let client = build_vetted_client(source, allow_loopback, resolve).await?;

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

    // Size cap — pre-check the declared Content-Length …
    if let Some(len) = resp.content_length() {
        if len > MAX_TILE_BYTES {
            return Err(FetchError::TooLarge);
        }
    }

    // … then stream the body and abort on the running total (don't trust the
    // declared length — a hostile server may omit or under-report it; mirrors
    // forms::updater::download_archive's defense-in-depth).
    let mut body: Vec<u8> = Vec::new();
    let mut total: u64 = 0;
    use futures::StreamExt;
    let mut stream = resp.bytes_stream();
    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res.map_err(|e| FetchError::Network(format!("read chunk: {e}")))?;
        total = total.saturating_add(chunk.len() as u64);
        if total > MAX_TILE_BYTES {
            return Err(FetchError::TooLarge);
        }
        body.extend_from_slice(&chunk);
    }

    // Image magic-byte validation — do NOT trust the upstream Content-Type.
    let mime = image_mime_from_magic(&body).ok_or(FetchError::NotAnImage)?;
    Ok((body, mime))
}

// ===========================================================================
// Single-flight de-duplication (Task 5.4)
// ===========================================================================

/// The shared result of one in-flight fetch. `Arc` because `FetchError` is not
/// `Clone` and `futures::future::Shared` requires a `Clone` output: every
/// waiter clones the cheap `Arc`, not the body. The body is cloned once per
/// caller only when each unwraps its copy.
type FetchResult = Arc<Result<(Vec<u8>, &'static str), FetchError>>;

/// A shared, cloneable handle to an in-flight fetch future.
type SharedFetch = Shared<BoxFuture<'static, FetchResult>>;

/// Key for the in-flight map: per-source namespace + the validated coordinate.
/// Two callers asking for the SAME tile of the SAME source coalesce; different
/// sources (different namespace) never collide even at identical z/x/y.
type FlightKey = (String, TileCoord);

/// Process-wide in-flight registry: maps a tile key to its shared fetch future.
/// Guarded by an async mutex so the join-or-launch decision is atomic across
/// concurrent callers (mirrors `forms::updater::INSTALL_LOCK`, but keyed per
/// tile so independent tiles still fetch concurrently). The leader removes its
/// own entry on completion, so the map cannot grow unbounded.
static FLIGHTS: Lazy<tokio::sync::Mutex<HashMap<FlightKey, SharedFetch>>> =
    Lazy::new(|| tokio::sync::Mutex::new(HashMap::new()));

/// Fetch a tile with single-flight de-duplication AND cache integration.
///
/// Behavior:
/// 1. **Cache hit** → return the cached bytes immediately (no upstream, no
///    flight). The `cache_root` is resolved by Phase 6 and passed in.
/// 2. **Cache miss** → coalesce concurrent callers for the SAME
///    `(namespace, coord)` onto ONE upstream fetch via a `Shared` future. The
///    leader performs the fetch and the cache `put`; every waiter awaits the
///    same result. Exactly one upstream request and one cache write occur.
///
/// The in-flight entry is removed when the fetch completes so the registry does
/// not grow unbounded.
pub async fn fetch_tile_single_flight(
    cache_root: &std::path::Path,
    source: &TileSource,
    coord: &TileCoord,
    allow_loopback: bool,
) -> Result<(Vec<u8>, &'static str), FetchError> {
    // 1. Cache-first: a hit short-circuits before any flight bookkeeping.
    if let Some(bytes) = cache::get(cache_root, source, coord) {
        let mime = image_mime_from_magic(&bytes).unwrap_or("image/png");
        return Ok((bytes, mime));
    }

    let ns = cache::source_namespace(source);
    let key: FlightKey = (ns, *coord);

    // 2. Upgrade-or-insert the shared flight under the registry lock.
    let shared: SharedFetch = {
        let mut flights = FLIGHTS.lock().await;
        if let Some(existing) = flights.get(&key) {
            // A fetch for this exact tile is already in flight: join it.
            existing.clone()
        } else {
            // Become the leader: build the fetch+cache future, share it, store
            // it so concurrent callers join, then drop the lock and drive it.
            let cache_root = cache_root.to_path_buf();
            let source = source.clone();
            let coord = *coord;
            let key_for_cleanup = key.clone();
            let fut = async move {
                let result = fetch_tile_bytes(&source, &coord, allow_loopback).await;
                // Cache only a verified success (cache-only-good is enforced
                // again inside `put`; degrades silently on write failure).
                if let Ok((ref bytes, _mime)) = result {
                    let _ = cache::put(&cache_root, &source, &coord, bytes);
                }
                // Remove our own in-flight entry so the registry stays bounded.
                // A late joiner that already cloned the Shared still completes;
                // a NEW caller after this point re-fetches (correct: the result
                // is now cached, so it short-circuits at step 1 anyway).
                {
                    let mut flights = FLIGHTS.lock().await;
                    flights.remove(&key_for_cleanup);
                }
                Arc::new(result)
            }
            .boxed()
            .shared();
            flights.insert(key.clone(), fut.clone());
            fut
        }
    };

    // Await the shared result (leader and all waiters land here).
    let shared_result: FetchResult = shared.await;
    match Arc::try_unwrap(shared_result) {
        // We were the last holder — take ownership without cloning the body.
        Ok(r) => r,
        // Other waiters still hold the Arc — clone our copy out of it.
        Err(arc) => match &*arc {
            Ok((bytes, mime)) => Ok((bytes.clone(), mime)),
            Err(e) => Err(clone_fetch_error(e)),
        },
    }
}

/// `FetchError` is not `Clone` (it wraps non-Clone payloads), but a waiter that
/// shares a failed result must surface its own owned error. Reconstruct an
/// equivalent variant. Lossless for the unit variants; string variants clone
/// their message.
fn clone_fetch_error(e: &FetchError) -> FetchError {
    match e {
        FetchError::Redirect => FetchError::Redirect,
        FetchError::HostDenied(s) => FetchError::HostDenied(s.clone()),
        FetchError::NotAnImage => FetchError::NotAnImage,
        FetchError::TooLarge => FetchError::TooLarge,
        FetchError::NotFound => FetchError::NotFound,
        FetchError::Status(c) => FetchError::Status(*c),
        FetchError::Network(s) => FetchError::Network(s.clone()),
        FetchError::BadUrl(s) => FetchError::BadUrl(s.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tiles::{TileScheme, TileSource};
    use std::net::SocketAddr;

    fn source(url: &str) -> TileSource {
        TileSource {
            url: url.into(),
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
        // PNG signature + a little filler.
        let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
        v.extend_from_slice(&[0u8; 32]);
        v
    }

    // A resolver that always returns the given fixed addresses (test seam).
    fn fixed_resolver(
        addrs: Vec<SocketAddr>,
    ) -> impl Fn(String, u16) -> std::future::Ready<std::io::Result<Vec<SocketAddr>>> + Clone {
        move |_host, _port| std::future::ready(Ok(addrs.clone()))
    }

    // ---- Task 3.1 ----

    #[test]
    fn client_builds() {
        // Smoke: the no-redirect / short-timeout client constructs cleanly.
        assert!(build_tile_client().is_ok());
    }

    // ---- Finding 2: no proxy on tile clients ----

    #[tokio::test]
    #[serial_test::serial]
    async fn fetch_ignores_ambient_proxy_env() {
        // Finding 2 regression: reqwest honors HTTP(S)_PROXY / system proxy by
        // default; `resolve_to_addrs` pins the TARGET host, NOT the proxy, so a
        // permitted LAN source's connection could be opened to a PUBLIC proxy.
        // We point the proxy env at a dead address — if the tile client honored
        // it, the fetch would route to the (refused) proxy and fail. `.no_proxy()`
        // forces a direct connection to the vetted loopback address, so the fetch
        // SUCCEEDS. (serial + env save/restore: env is process-global.)
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/3/5/2.png")
            .with_status(200)
            .with_header("content-type", "image/png")
            .with_body(png_bytes())
            .create_async()
            .await;

        // A dead proxy on a port nothing listens on. If honored, the fetch fails.
        let dead_proxy = "http://127.0.0.1:9";
        let prior_http = std::env::var("HTTP_PROXY").ok();
        let prior_https = std::env::var("HTTPS_PROXY").ok();
        let prior_all = std::env::var("ALL_PROXY").ok();
        // SAFETY: single-threaded (serial) test; no concurrent env access.
        unsafe {
            std::env::set_var("HTTP_PROXY", dead_proxy);
            std::env::set_var("HTTPS_PROXY", dead_proxy);
            std::env::set_var("ALL_PROXY", dead_proxy);
        }

        let src = source(&server.url());
        let result = fetch_tile_bytes(&src, &coord(), true).await;

        // SAFETY: symmetric restore; single-threaded test.
        unsafe {
            match prior_http {
                Some(v) => std::env::set_var("HTTP_PROXY", v),
                None => std::env::remove_var("HTTP_PROXY"),
            }
            match prior_https {
                Some(v) => std::env::set_var("HTTPS_PROXY", v),
                None => std::env::remove_var("HTTPS_PROXY"),
            }
            match prior_all {
                Some(v) => std::env::set_var("ALL_PROXY", v),
                None => std::env::remove_var("ALL_PROXY"),
            }
        }

        let (bytes, mime) = result.expect("no_proxy client must bypass the dead proxy and fetch directly");
        assert_eq!(mime, "image/png");
        assert!(bytes.starts_with(b"\x89PNG"));
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
        // mockito binds loopback; allow_loopback=true exercises the happy IP-literal path.
        let src = source(&server.url());
        let err = fetch_tile_bytes(&src, &coord(), true).await.unwrap_err();
        m.assert_async().await;
        assert!(matches!(err, FetchError::Redirect), "got {err:?}");
    }

    // ---- Task 3.2 ----

    #[tokio::test]
    async fn loopback_optin_fetch_succeeds() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("GET", "/3/5/2.png")
            .with_status(200)
            .with_header("content-type", "image/png")
            .with_body(png_bytes())
            .create_async()
            .await;
        let src = source(&server.url());
        let (bytes, mime) = fetch_tile_bytes(&src, &coord(), true).await.unwrap();
        m.assert_async().await;
        assert_eq!(mime, "image/png");
        assert!(bytes.starts_with(b"\x89PNG"));
    }

    #[tokio::test]
    async fn loopback_denied_without_optin() {
        // Same loopback server, but allow_loopback=false → IP-literal gate denies.
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/3/5/2.png")
            .with_status(200)
            .with_body(png_bytes())
            .create_async()
            .await;
        let src = source(&server.url());
        let err = fetch_tile_bytes(&src, &coord(), false).await.unwrap_err();
        assert!(matches!(err, FetchError::HostDenied(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn named_host_resolving_to_public_ip_is_denied() {
        // THE DNS-rebind test: a named host whose injected resolution returns a
        // PUBLIC IP must be HostDenied, even though the URL string looked fine.
        let src = source("https://tiles.lan/");
        let public: SocketAddr = "8.8.8.8:443".parse().unwrap();
        let err = fetch_tile_bytes_with_resolver(
            &src,
            &coord(),
            false,
            fixed_resolver(vec![public]),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, FetchError::HostDenied(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn named_host_with_mixed_addrs_is_denied() {
        // Any-public in the resolved set → reject (we do not cherry-pick a
        // private address out of a mixed set).
        let src = source("https://tiles.lan/");
        let private: SocketAddr = "192.168.1.5:443".parse().unwrap();
        let public: SocketAddr = "8.8.8.8:443".parse().unwrap();
        let err = fetch_tile_bytes_with_resolver(
            &src,
            &coord(),
            false,
            fixed_resolver(vec![private, public]),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, FetchError::HostDenied(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn ip_literal_public_host_is_denied_before_connect() {
        // A public IP-literal source URL must be denied without any network I/O.
        let src = source("http://8.8.8.8:8080/");
        let err = fetch_tile_bytes(&src, &coord(), false).await.unwrap_err();
        assert!(matches!(err, FetchError::HostDenied(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn ipv6_literal_public_host_is_denied() {
        // A bracketed public IPv6 literal must be recognized as an IP literal
        // (brackets stripped) and DENIED by the direct-vet branch — not misrouted
        // through the resolver. (ULA v6 literals like [fd00::1] take the same
        // branch and pass vetting; we assert the deny direction here since it
        // needs no live server.)
        let src = source("http://[2001:4860:4860::8888]:8080/");
        let err = fetch_tile_bytes(&src, &coord(), false).await.unwrap_err();
        assert!(matches!(err, FetchError::HostDenied(_)), "got {err:?}");
    }

    // ---- Task 3.3 ----

    #[tokio::test]
    async fn non_image_content_type_is_not_an_image() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/3/5/2.png")
            .with_status(200)
            .with_header("content-type", "text/html")
            .with_body("<html>not a tile</html>")
            .create_async()
            .await;
        let src = source(&server.url());
        let err = fetch_tile_bytes(&src, &coord(), true).await.unwrap_err();
        assert!(matches!(err, FetchError::NotAnImage), "got {err:?}");
    }

    #[tokio::test]
    async fn declared_content_length_over_cap_is_too_large() {
        // An honest over-cap Content-Length (body length == declared length, so
        // hyper doesn't reject the response) must be caught by the pre-check
        // before the body is streamed. The body starts with PNG magic so the
        // ONLY thing that can reject it is the size cap.
        let mut server = mockito::Server::new_async().await;
        let mut body = png_bytes();
        body.resize((MAX_TILE_BYTES + 1) as usize, 0u8);
        let _m = server
            .mock("GET", "/3/5/2.png")
            .with_status(200)
            .with_header("content-type", "image/png")
            .with_body(body)
            .create_async()
            .await;
        let src = source(&server.url());
        let err = fetch_tile_bytes(&src, &coord(), true).await.unwrap_err();
        assert!(matches!(err, FetchError::TooLarge), "got {err:?}");
    }

    #[tokio::test]
    async fn streamed_body_over_cap_is_too_large() {
        // Server omits Content-Length entirely (chunked transfer) but streams a
        // body over the cap → the streaming running-total abort must fire. This
        // is the "server lies/omits Content-Length" production case the pre-check
        // alone cannot cover. mockito uses chunked encoding (no Content-Length)
        // when a body stream is supplied without a length header.
        let mut server = mockito::Server::new_async().await;
        let body = vec![0xABu8; (MAX_TILE_BYTES + 1024) as usize];
        let _m = server
            .mock("GET", "/3/5/2.png")
            .with_status(200)
            .with_header("content-type", "image/png")
            // chunked transfer encoding → no Content-Length, so the pre-check
            // cannot fire and the streaming running-total guard is the only
            // thing that can reject the over-cap body.
            .with_chunked_body(move |w| w.write_all(&body))
            .create_async()
            .await;
        let src = source(&server.url());
        let err = fetch_tile_bytes(&src, &coord(), true).await.unwrap_err();
        assert!(matches!(err, FetchError::TooLarge), "got {err:?}");
    }

    #[tokio::test]
    async fn valid_png_magic_is_ok() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/3/5/2.png")
            .with_status(200)
            // Deliberately mislabel as octet-stream: magic, not Content-Type, decides.
            .with_header("content-type", "application/octet-stream")
            .with_body(png_bytes())
            .create_async()
            .await;
        let src = source(&server.url());
        let (bytes, mime) = fetch_tile_bytes(&src, &coord(), true).await.unwrap();
        assert_eq!(mime, "image/png");
        assert!(bytes.starts_with(b"\x89PNG"));
    }

    #[tokio::test]
    async fn status_404_is_not_found() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/3/5/2.png")
            .with_status(404)
            .create_async()
            .await;
        let src = source(&server.url());
        let err = fetch_tile_bytes(&src, &coord(), true).await.unwrap_err();
        assert!(matches!(err, FetchError::NotFound), "got {err:?}");
    }

    #[tokio::test]
    async fn other_non_success_is_status() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/3/5/2.png")
            .with_status(503)
            .create_async()
            .await;
        let src = source(&server.url());
        let err = fetch_tile_bytes(&src, &coord(), true).await.unwrap_err();
        assert!(matches!(err, FetchError::Status(503)), "got {err:?}");
    }

    #[tokio::test]
    async fn tms_scheme_flips_y_in_url() {
        // TMS source: y in the requested path must be the flipped upstream_y.
        let mut server = mockito::Server::new_async().await;
        let c = TileCoord::new(2, 1, 0, 19).unwrap();
        let flipped = c.upstream_y(true); // (1<<2)-1-0 = 3
        let path = format!("/2/1/{flipped}.png");
        let m = server
            .mock("GET", path.as_str())
            .with_status(200)
            .with_body(png_bytes())
            .create_async()
            .await;
        let mut src = source(&server.url());
        src.scheme = TileScheme::Tms;
        let (_bytes, mime) = fetch_tile_bytes(&src, &c, true).await.unwrap();
        m.assert_async().await;
        assert_eq!(mime, "image/png");
    }

    // ---- Task 5.4 ----

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn single_flight_dedupes_concurrent_requests_for_same_tile() {
        // N concurrent callers for the SAME (source, coord) must cause EXACTLY
        // ONE upstream fetch and ONE cache write. The mock's hit counter is the
        // ground truth: `.expect(1)` + assert proves de-duplication.
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("GET", "/3/5/2.png")
            .with_status(200)
            .with_header("content-type", "image/png")
            // A slow (chunked, sleep-between-chunks) body widens the in-flight
            // window so every concurrent caller joins the SAME flight before the
            // leader completes — exercising the coalescing path, not just a
            // post-completion cache hit.
            .with_chunked_body(|w| {
                w.write_all(b"\x89PNG\r\n\x1a\n")?;
                std::thread::sleep(std::time::Duration::from_millis(150));
                w.write_all(&[0u8; 64])
            })
            .expect(1) // <-- exactly one upstream request, no matter how many callers
            .create_async()
            .await;

        let cache_dir = tempfile::tempdir().unwrap();
        let src = Arc::new(source(&server.url()));
        let cache_root = Arc::new(cache_dir.path().to_path_buf());
        let c = coord();

        // Launch 16 concurrent callers for the same tile.
        let mut handles = Vec::new();
        for _ in 0..16 {
            let src = src.clone();
            let cache_root = cache_root.clone();
            handles.push(tokio::spawn(async move {
                fetch_tile_single_flight(&cache_root, &src, &c, true).await
            }));
        }
        let mut bodies = Vec::new();
        for h in handles {
            let (bytes, mime) = h.await.unwrap().expect("each caller gets the result");
            assert_eq!(mime, "image/png");
            bodies.push(bytes);
        }

        // Exactly one upstream request occurred.
        m.assert_async().await;
        // Every caller observed the identical body.
        assert!(bodies.iter().all(|b| *b == bodies[0]));
        assert!(bodies[0].starts_with(b"\x89PNG"));

        // The single cache write landed: a subsequent get is a hit with no new
        // upstream request (the mock would fail `.expect(1)` on a 2nd hit).
        let cached = cache::get(cache_root.as_path(), &src, &c).expect("tile cached");
        assert_eq!(cached, bodies[0]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn single_flight_cache_hit_skips_upstream() {
        // After one fetch populates the cache, a later call serves from cache
        // with NO upstream request (mock expects exactly the single fetch).
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("GET", "/3/5/2.png")
            .with_status(200)
            .with_header("content-type", "image/png")
            .with_body(png_bytes())
            .expect(1)
            .create_async()
            .await;
        let cache_dir = tempfile::tempdir().unwrap();
        let src = source(&server.url());
        let c = coord();
        let (b1, _) = fetch_tile_single_flight(cache_dir.path(), &src, &c, true)
            .await
            .unwrap();
        // Second call: cache hit, no upstream.
        let (b2, _) = fetch_tile_single_flight(cache_dir.path(), &src, &c, true)
            .await
            .unwrap();
        m.assert_async().await; // still exactly 1 upstream
        assert_eq!(b1, b2);
    }
}
