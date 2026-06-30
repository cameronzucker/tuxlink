//! `egress` — the shared socket-layer SSRF / DNS-rebind guard for the Elmer
//! model adapter.
//!
//! ## Why this exists (the security headline)
//!
//! [`crate::endpoint::AgentEndpoint::parse`] (Task A1) already refuses
//! metadata / link-local *literals* and credentials-in-URL at config time. That
//! string check is NOT sufficient on its own: a *named* host (`api.model.lan`)
//! validates fine as a string and then RESOLVES, at request time, to a forbidden
//! IP. That is the classic DNS-rebinding SSRF: an endpoint that looks benign on
//! parse rebinds to `169.254.169.254` (cloud metadata) or `127.0.0.1` between
//! validation and connect, turning the model adapter into an exfiltration sink
//! for every tainted mailbox byte the loop puts in the prompt.
//!
//! This module is the fetch-time resolved-IP gate that closes that window. It is
//! the SINGLE shared egress chokepoint that BOTH the detect path and the per-turn
//! provider build their `reqwest::Client` through, so the policy cannot drift
//! between the two callers.
//!
//! ## The defenses, in order
//!
//! 1. **IP-literal host** (`http://127.0.0.1:11434/`, `http://[fd00::1]/`): there
//!    is no DNS to rebind — the literal is vetted directly via
//!    [`elmer_ip_is_permitted`] with `allow_loopback = endpoint.is_loopback()`.
//! 2. **Named host** (`https://api.model.example/`): resolved via the injected
//!    `resolve` AT THIS POINT, EVERY resolved address must pass
//!    [`elmer_ip_is_permitted`] (a mixed set with ANY refused IP is denied — no
//!    cherry-picking a permitted address out of a poisoned set), and the returned
//!    client is PINNED to exactly that vetted address set via
//!    [`reqwest::ClientBuilder::resolve_to_addrs`]. reqwest does not re-resolve a
//!    pinned host, so the TOCTOU rebind window between our lookup and reqwest's
//!    connect is closed.
//! 3. **`redirect::Policy::none()`** — unconditional. A 3xx becomes a hard error,
//!    never followed: a `302 → http://169.254.169.254/` would otherwise carry the
//!    whole conversation to the cloud-metadata endpoint past the gate.
//! 4. **`.no_proxy()`** — unconditional. The `resolve_to_addrs` pin targets the
//!    HOST's connection, not a proxy; honoring an ambient `HTTP(S)_PROXY` would
//!    open the socket to the proxy instead of the vetted IP, defeating the gate.
//! 5. **`.connect_timeout(10s)`** — unconditional; bounds the connect dial.
//!
//! ## Permit-set — INVERTED vs `tiles::host::ip_is_permitted`
//!
//! The tile fetcher default-DENIES public IPs (a tile server MUST be on the LAN):
//! it permits only RFC1918 + ULA. Elmer is the OPPOSITE — the operator may run a
//! cloud model (`api.openai.com`) OR a LAN model server (`192.168.1.50`), so BOTH
//! public and RFC1918 are permitted and there is no public-vs-private distinction
//! to draw. The deny-set is the SSRF-magnet ranges only: loopback (unless the
//! literal-loopback opt-in), link-local / cloud-metadata (`169.254.0.0/16`,
//! `fe80::/10`, and IPv4-mapped forms canonicalized via `to_canonical()`),
//! multicast, and unspecified.
//!
//! This module is pure-policy + a generic client builder: the resolver is
//! injected (no real DNS, no network in tests).

use std::net::IpAddr;
use std::time::Duration;

use crate::endpoint::AgentEndpoint;

/// Connect-dial timeout for a model-endpoint client. Bounds how long a connect
/// to the pinned address may hang; unconditional on every client this module
/// builds.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Errors from the shared egress gate.
///
/// Deliberately a small, stable surface shared by the detect path and the
/// per-turn provider. `HostDenied` carries an operator-facing reason string.
#[derive(Debug, thiserror::Error)]
pub enum EgressError {
    /// The configured/resolved host is not a permitted model destination
    /// (loopback without the literal-loopback opt-in, link-local / metadata,
    /// multicast, unspecified, or a named host that resolved to any of those).
    /// The SSRF gate.
    #[error("model endpoint host denied by egress policy: {0}")]
    HostDenied(String),
    /// The upstream returned a 3xx. The no-redirect policy surfaces it as a
    /// normal response with a 3xx status; we refuse to follow it. (Reported by
    /// callers that inspect the response; never followed by the client.)
    #[error("model endpoint returned a redirect (3xx); redirects are not followed")]
    Redirect,
    /// A transport / network / DNS error (e.g. the injected resolver failed, or
    /// the reqwest client failed to build).
    #[error("network error: {0}")]
    Network(String),
    /// The endpoint URL was malformed in a way the gate could not proceed past
    /// (e.g. no host, no known port).
    #[error("bad URL: {0}")]
    BadUrl(String),
}

/// Allow/deny a *resolved* IP address for use as an Elmer model endpoint.
///
/// ## Permit policy (INVERTED vs `tiles::host::ip_is_permitted`)
/// - **Public internet** — permitted (the operator may use a cloud model).
/// - **RFC 1918 / ULA / any other non-deny IP** — permitted (the operator may
///   run a LAN model server). There is NO public-vs-private distinction.
/// - **Loopback** (`127.0.0.0/8`, `::1`) — permitted ONLY when `allow_loopback`
///   is `true` (the literal-loopback case: the endpoint host was an IP literal
///   that [`AgentEndpoint::is_loopback`] classified as loopback).
///
/// ## Deny policy (the SSRF-magnet ranges)
/// - Loopback when `allow_loopback = false`.
/// - Unspecified (`0.0.0.0`, `::`).
/// - Multicast (`224.0.0.0/4`, `ff00::/8`).
/// - IPv4 link-local `169.254.0.0/16` — covers the cloud-metadata oracle
///   `169.254.169.254`.
/// - IPv6 link-local `fe80::/10`.
/// - IPv4-mapped IPv6 forms (e.g. `::ffff:169.254.169.254`,
///   `::ffff:127.0.0.1`) — canonicalized via [`IpAddr::to_canonical`] FIRST so
///   a mapped metadata/loopback address is caught by the same v4 rules rather
///   than slipping through as an opaque v6 literal.
///
/// Pure: operates on an already-resolved `IpAddr`, no DNS / no network I/O.
/// `to_canonical()` is stable since Rust 1.75 (the repo MSRV).
pub fn elmer_ip_is_permitted(addr: IpAddr, allow_loopback: bool) -> bool {
    // Normalize IPv4-mapped IPv6 addresses (`::ffff:a.b.c.d`) to their IPv4
    // form so all subsequent checks operate on a canonical representation.
    // This is the IPv4-mapped-metadata regression guard: without it,
    // `::ffff:169.254.169.254` would be inspected as a v6 segment pattern and
    // miss the 169.254/16 deny.
    let canonical = addr.to_canonical();

    // Loopback: the only family-agnostic case that flips on the opt-in.
    if canonical.is_loopback() {
        return allow_loopback;
    }

    // Deny unspecified (0.0.0.0, ::).
    if canonical.is_unspecified() {
        return false;
    }

    // Deny multicast (224.0.0.0/4, ff00::/8).
    if canonical.is_multicast() {
        return false;
    }

    match canonical {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            // Deny IPv4 link-local 169.254.0.0/16 — the cloud-metadata range
            // (169.254.169.254 is the canonical SSRF target).
            if octets[0] == 169 && octets[1] == 254 {
                return false;
            }
            // Everything else (public internet, RFC 1918, CGNAT, …) is permitted.
            // INVERTED vs tiles: no allow-list of private ranges — public is fine.
            true
        }
        IpAddr::V6(v6) => {
            // Deny IPv6 link-local fe80::/10. The top 10 bits are 1111 1110 10,
            // i.e. (seg0 & 0xffc0) == 0xfe80.
            let seg0 = v6.segments()[0];
            if (seg0 & 0xffc0) == 0xfe80 {
                return false;
            }
            // Everything else (public v6, ULA, …) is permitted.
            true
        }
    }
}

/// Vet an [`AgentEndpoint`]'s host against the egress policy and build the
/// `reqwest::Client` that will reach it — the SINGLE shared egress chokepoint
/// for both the detect path and the per-turn provider.
///
/// Branches on the endpoint URL's host type:
/// - **IP literal** (`http://127.0.0.1:11434/`, `http://[fd00::1]/`): no DNS to
///   rebind. The literal is vetted directly via [`elmer_ip_is_permitted`] with
///   `allow_loopback = endpoint.is_loopback()` so a literal-loopback endpoint
///   (the local llama.cpp / Ollama shim) is allowed while a literal
///   `169.254.169.254` (which `AgentEndpoint::parse` already refuses, but we
///   re-check defensively) is denied.
/// - **Named host** (`https://api.model.example/`): resolved via `resolve` AT
///   THIS POINT, EVERY resolved address must pass [`elmer_ip_is_permitted`]
///   (reject a mixed/any-forbidden set — no cherry-pick), and the returned
///   client is PINNED to exactly that vetted address set via `resolve_to_addrs`.
///   reqwest does not re-resolve a pinned host, closing the TOCTOU rebind window.
///   `allow_loopback = false` for named hosts: a name must NEVER be granted
///   loopback (only `localhost`, which `AgentEndpoint` classifies as loopback,
///   is special — but a named-host resolution to loopback is the rebind attack).
///
/// Every returned client carries `redirect::none()`, `.no_proxy()`, and the
/// connect-timeout UNCONDITIONALLY (Findings: a redirect or a proxy would each
/// defeat the gate independently).
pub async fn build_vetted_client<R, Fut>(
    endpoint: &AgentEndpoint,
    resolve: R,
) -> Result<reqwest::Client, EgressError>
where
    R: Fn(String, u16) -> Fut,
    Fut: std::future::Future<Output = std::io::Result<Vec<std::net::SocketAddr>>>,
{
    let url = endpoint.url();
    let host = url
        .host_str()
        .ok_or_else(|| EgressError::BadUrl("endpoint URL has no host".into()))?
        .to_string();
    let port = url
        .port_or_known_default()
        .ok_or_else(|| EgressError::BadUrl("endpoint URL has no known port".into()))?;

    // `Url::host_str` returns IPv6 literals bracketed (`[fd00::1]`), which does
    // not parse as `IpAddr`; strip the brackets so BOTH v4 and v6 literals take
    // the direct-vet branch (a v6 literal must not be misrouted to the resolver).
    let host_for_ip = host
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(host.as_str());

    match host_for_ip.parse::<IpAddr>() {
        Ok(ip) => {
            // IP-literal host: no DNS to rebind, vet the literal directly. The
            // literal-loopback opt-in is gated on the endpoint's own
            // classification, NOT a bare caller flag.
            if !elmer_ip_is_permitted(ip, endpoint.is_loopback()) {
                return Err(EgressError::HostDenied(format!(
                    "IP literal {ip} is not a permitted model endpoint destination"
                )));
            }
            build_client_no_pin()
        }
        Err(_) => {
            // Named host: resolve at this point, require EVERY resolved address to
            // pass the policy, then PIN the connection to that vetted set. A name
            // is never granted loopback (allow_loopback = false): a name resolving
            // to loopback is the DNS-rebind attack, not a legitimate config.
            let resolved = resolve(host.clone(), port)
                .await
                .map_err(|e| EgressError::Network(format!("DNS resolution of {host:?}: {e}")))?;
            if resolved.is_empty() {
                return Err(EgressError::HostDenied(format!(
                    "host {host:?} resolved to no addresses"
                )));
            }
            for addr in &resolved {
                if !elmer_ip_is_permitted(addr.ip(), false) {
                    return Err(EgressError::HostDenied(format!(
                        "host {host:?} resolved to forbidden address {}",
                        addr.ip()
                    )));
                }
            }
            reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .no_proxy()
                .connect_timeout(CONNECT_TIMEOUT)
                .resolve_to_addrs(&host, &resolved)
                .build()
                .map_err(|e| EgressError::Network(format!("client build: {e}")))
        }
    }
}

/// Build the shared no-redirect, no-proxy, connect-timeout client WITHOUT a DNS
/// pin — used for the IP-literal branch (there is no name to pin). The named-host
/// branch builds the same client plus `resolve_to_addrs`.
fn build_client_no_pin() -> Result<reqwest::Client, EgressError> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .map_err(|e| EgressError::Network(format!("client build: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    // A resolver that always returns the given fixed addresses (test seam) — the
    // same shape as `tiles::fetch::fixed_resolver`. No real DNS, no network.
    fn fixed_resolver(
        addrs: Vec<SocketAddr>,
    ) -> impl Fn(String, u16) -> std::future::Ready<std::io::Result<Vec<SocketAddr>>> + Clone {
        move |_host, _port| std::future::ready(Ok(addrs.clone()))
    }

    // ---- elmer_ip_is_permitted: the permit/deny table ----

    #[test]
    fn permits_public_and_rfc1918() {
        // INVERTED vs tiles: public IPs are PERMITTED (cloud model), and RFC1918
        // is permitted too (LAN model server). No public-vs-private distinction.
        for s in ["8.8.8.8", "1.1.1.1", "192.168.1.5", "10.0.0.5", "172.16.4.4"] {
            assert!(
                elmer_ip_is_permitted(ip(s), false),
                "{s} should be permitted (public + RFC1918 both pass)"
            );
        }
    }

    #[test]
    fn refuses_metadata_linklocal_multicast_unspecified() {
        // The SSRF-magnet deny-set. ::ffff:169.254.169.254 is the IPv4-mapped
        // metadata form: it MUST canonicalize and be denied, not slip through as
        // an opaque v6 literal.
        for s in [
            "169.254.169.254",
            "fe80::1",
            "::ffff:169.254.169.254",
            "224.0.0.1",
            "0.0.0.0",
            "::",
        ] {
            assert!(
                !elmer_ip_is_permitted(ip(s), false),
                "{s} should be denied (SSRF-magnet range)"
            );
        }
    }

    #[test]
    fn refuses_loopback_unless_optin() {
        // Loopback is the only case the opt-in flips. Without it: denied. With it:
        // permitted. The IPv4-mapped loopback form must also canonicalize and be
        // denied without the opt-in (the to_canonical() guard).
        assert!(!elmer_ip_is_permitted(ip("127.0.0.1"), false));
        assert!(elmer_ip_is_permitted(ip("127.0.0.1"), true));
        assert!(
            !elmer_ip_is_permitted(ip("::ffff:127.0.0.1"), false),
            "IPv4-mapped loopback must canonicalize and be denied without opt-in"
        );
    }

    // ---- build_vetted_client: the resolve-vet-pin behavior ----

    #[tokio::test]
    async fn build_vetted_client_denies_name_resolving_to_metadata() {
        // THE DNS-rebind test: a named endpoint whose injected resolution returns
        // the cloud-metadata IP must be HostDenied, even though the URL string
        // parsed fine (AgentEndpoint::parse cannot catch a NAMED host that
        // RESOLVES to metadata).
        let ep = AgentEndpoint::parse("https://api.model.example/v1/chat/completions").unwrap();
        let metadata: SocketAddr = "169.254.169.254:443".parse().unwrap();
        let err = build_vetted_client(&ep, fixed_resolver(vec![metadata]))
            .await
            .unwrap_err();
        assert!(matches!(err, EgressError::HostDenied(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn build_vetted_client_permits_name_resolving_to_public() {
        // Public IS permitted for Elmer (inverted vs tiles). A named public-https
        // endpoint resolving to a public IP must build a client (no network call;
        // just asserts the pinned client constructs).
        let ep = AgentEndpoint::parse("https://api.openai.com/v1/chat/completions").unwrap();
        let public: SocketAddr = "8.8.8.8:443".parse().unwrap();
        let client = build_vetted_client(&ep, fixed_resolver(vec![public])).await;
        assert!(client.is_ok(), "public-resolving named host must build: {client:?}");
    }

    #[tokio::test]
    async fn build_vetted_client_ip_literal_loopback_allowed_when_is_loopback() {
        // The literal-loopback branch: a 127.0.0.1 literal endpoint (the local
        // llama.cpp / Ollama shim) is allowed because endpoint.is_loopback() is
        // true. The resolver is never consulted for an IP literal — pass a
        // resolver that would FAIL the vet to prove the literal branch is taken.
        let ep = AgentEndpoint::parse("http://127.0.0.1:11434/v1/chat/completions").unwrap();
        let poison: SocketAddr = "169.254.169.254:443".parse().unwrap();
        let client = build_vetted_client(&ep, fixed_resolver(vec![poison])).await;
        assert!(
            client.is_ok(),
            "loopback IP literal must be allowed via is_loopback() opt-in: {client:?}"
        );
    }

    #[tokio::test]
    async fn build_vetted_client_named_mixed_set_denied() {
        // Any forbidden IP in the resolved set → reject the WHOLE set. We do not
        // cherry-pick the permitted 8.8.8.8 out of a set that also contains the
        // metadata IP — a poisoned resolution fails closed.
        let ep = AgentEndpoint::parse("https://api.model.example/v1/chat/completions").unwrap();
        let public: SocketAddr = "8.8.8.8:443".parse().unwrap();
        let metadata: SocketAddr = "169.254.169.254:443".parse().unwrap();
        let err = build_vetted_client(&ep, fixed_resolver(vec![public, metadata]))
            .await
            .unwrap_err();
        assert!(matches!(err, EgressError::HostDenied(_)), "got {err:?}");
    }
}
