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
//! 5. **Short timeout** (5 s).
//! 6. **Response size cap** ([`MAX_TILE_BYTES`]) enforced via both the
//!    `Content-Length` pre-check AND a streaming running-total abort (the
//!    server may lie about / omit `Content-Length`).
//! 7. **Image magic-byte validation** — the leading bytes must be a real
//!    PNG/JPEG/WebP signature; the upstream `Content-Type` is NOT trusted.

use std::net::SocketAddr;
use std::time::Duration;

use reqwest::Url;

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
fn image_mime_from_magic(bytes: &[u8]) -> Option<&'static str> {
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

    let host = url
        .host_str()
        .ok_or_else(|| FetchError::BadUrl("tile URL has no host".into()))?
        .to_string();
    // Effective port for connect-pinning (scheme default when absent).
    let port = url
        .port_or_known_default()
        .ok_or_else(|| FetchError::BadUrl("tile URL has no known port".into()))?;

    // SSRF (§8.3): the resolved-IP gate. Branch on host type.
    // `Url::host_str` returns IPv6 literals in bracketed form (`[fd00::1]`),
    // which does not parse as `IpAddr`; strip the brackets so BOTH IPv4 and
    // IPv6 literals are recognized and take the direct-vet branch below (a v6
    // literal must not be misrouted through the resolver path). Domains never
    // carry brackets, so this is a no-op for them.
    let host_for_ip = host
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(host.as_str());
    let client = match host_for_ip.parse::<std::net::IpAddr>() {
        Ok(ip) => {
            // IP-literal host (common LAN case, e.g. http://192.168.1.5:8080/ or
            // http://[fd00::1]:8080/). There is no DNS to rebind: vet the literal
            // directly and connect normally with the shared no-redirect client.
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
}
