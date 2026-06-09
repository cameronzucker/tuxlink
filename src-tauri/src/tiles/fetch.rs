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
//! 4. **`redirect::Policy::none()`** — a 3xx is a hard error, never followed.

use std::net::SocketAddr;
use std::time::Duration;

use reqwest::Url;

use super::coord::TileCoord;
use super::host::{ip_is_permitted, validate_source_url};
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
/// Used directly for the IP-literal host case (no DNS pinning needed). For
/// named hosts, a *per-fetch* client is built with the same options plus
/// `resolve_to_addrs` pinning (see [`fetch_tile_bytes_with_resolver`]).
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
/// never string interpolation of webview-influenced input (§8.4). The source
/// URL's existing path is treated as a base directory: a trailing-slash
/// difference is normalized so `…/tiles` and `…/tiles/` both yield
/// `…/tiles/{z}/{x}/{y}.png`.
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

/// Production resolver: resolve `host:port` to a list of `SocketAddr` via the
/// platform resolver.
async fn system_resolve(host: &str, port: u16) -> std::io::Result<Vec<SocketAddr>> {
    let target = format!("{host}:{port}");
    tokio::net::lookup_host(target).await.map(|it| it.collect())
}

/// Fetch a single tile, building the upstream URL from the stored source and
/// validated coordinate. Public entry point; uses the system resolver.
pub async fn fetch_tile_bytes(
    source: &TileSource,
    coord: &TileCoord,
    allow_loopback: bool,
) -> Result<Vec<u8>, FetchError> {
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
) -> Result<Vec<u8>, FetchError>
where
    R: Fn(String, u16) -> Fut,
    Fut: std::future::Future<Output = std::io::Result<Vec<SocketAddr>>>,
{
    let url = build_tile_url(source, coord)?;

    let host = url
        .host_str()
        .ok_or_else(|| FetchError::BadUrl("tile URL has no host".into()))?
        .to_string();
    let port = url
        .port_or_known_default()
        .ok_or_else(|| FetchError::BadUrl("tile URL has no known port".into()))?;

    // SSRF (§8.3): the resolved-IP gate. Branch on host type.
    let client = match host.parse::<std::net::IpAddr>() {
        Ok(ip) => {
            // IP-literal host (common LAN case, e.g. http://192.168.1.5:8080/).
            // There is no DNS to rebind: vet the literal directly and connect
            // normally with the shared no-redirect client.
            if !ip_is_permitted(ip, allow_loopback) {
                return Err(FetchError::HostDenied(format!(
                    "IP literal {ip} is not a permitted LAN destination"
                )));
            }
            build_tile_client()?
        }
        Err(_) => {
            // Named host (e.g. https://tiles.lan/). Resolve at fetch time and
            // require EVERY resolved address to pass the policy (reject mixed /
            // any-public — do NOT use the single-addr `resolve()` convenience),
            // then PIN the per-fetch client's connection to exactly that vetted
            // address set so reqwest can only reach IPs we validated. reqwest
            // does not re-resolve a pinned host, closing the TOCTOU rebind
            // window between our lookup and reqwest's connect.
            //
            // Fully-general alternative: a custom `dns_resolver`/`Resolve` impl
            // on one shared client that vets inside `resolve()`. The per-fetch
            // `resolve_to_addrs` approach below is the primary because it keeps
            // the vetting in plain async code (testable via this seam) and does
            // not require a long-lived shared resolver object.
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
            // All vetted: pin the connection to exactly these addresses.
            reqwest::Client::builder()
                .user_agent(TILE_USER_AGENT)
                .timeout(TILE_TIMEOUT)
                .redirect(reqwest::redirect::Policy::none())
                .resolve_to_addrs(&host, &resolved)
                .build()
                .map_err(|e| FetchError::Network(format!("client build: {e}")))?
        }
    };

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| FetchError::Network(format!("send: {e}")))?;

    let status = resp.status();
    if status.is_redirection() {
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
    use std::net::SocketAddr;

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
            .with_body(png_bytes())
            .create_async()
            .await;
        let src = source(&server.url());
        let bytes = fetch_tile_bytes(&src, &coord(), true).await.unwrap();
        m.assert_async().await;
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
}
