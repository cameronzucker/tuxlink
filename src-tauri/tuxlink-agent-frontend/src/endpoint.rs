//! SEC-5 — loopback/UDS-only endpoint enforcement for the model adapter.
//!
//! The `OpenAiProvider` may only POST to a *loopback* HTTP(S) endpoint by
//! default. A non-loopback host — anything on the LAN, the public internet, or
//! the cloud-metadata / link-local ranges — is REJECTED unless the operator
//! explicitly opts in with `--allow-remote` (an advanced, disclosed flag). This
//! is the d3zwe-side half of the "the model can read tainted mailbox content,
//! so the model endpoint must be trusted" invariant: a remote endpoint is a data
//! exfiltration sink for everything the loop puts in the prompt.
//!
//! Critically, the endpoint is supplied ONLY from the CLI / config; a tool
//! result can never set it (the loop has no path to mutate it). This module is
//! pure (no I/O) so the whole accept/reject table is unit-tested in CI.
//!
//! ## What counts as loopback
//!
//! * IPv4 `127.0.0.0/8` (the entire loopback block, not just `127.0.0.1`).
//! * IPv6 `::1`.
//! * The literal host `localhost` (resolves to loopback on a sane system; we do
//!   NOT resolve DNS here — an attacker who can poison `localhost` already owns
//!   the box, and refusing the canonical loopback name would be hostile).
//!
//! ## What is ALWAYS rejected (even with `--allow-remote`-shaped opt-in handled
//! by the caller — this function reports the classification; the caller decides)
//!
//! * IPv4 link-local `169.254.0.0/16` — this is the cloud metadata range
//!   (`169.254.169.254`); exfiltration / SSRF magnet. Rejected as non-loopback;
//!   with `--allow-remote` the caller still refuses it (see [`validate_endpoint`]).
//! * IPv6 link-local `fe80::/10` and the IPv4-mapped form of the metadata IP.
//!
//! ## Threat model + the DNS-rebinding boundary (cf. implementation-pitfalls
//! SSRF-1)
//!
//! SSRF-1 warns that *config-time string validation is defeated by DNS
//! rebinding*: a hostname validated as "safe" can resolve to a loopback /
//! metadata IP at fetch time. That pitfall targets an **adversary-supplied** URL
//! (a malicious tile server). d3zwe's endpoint is **operator-supplied** (CLI /
//! config only; a tool result can NEVER set it), so the adversary-controls-the-URL
//! premise does not hold here, and a fetch-time resolved-IP gate (the tile path's
//! `ip_is_permitted`) is disproportionate for this surface.
//!
//! Two properties make the string check sufficient for d3zwe's default posture:
//!
//! 1. **Default-deny on names.** Only the literal `localhost` is treated as
//!    loopback; EVERY other domain classifies as `Remote` and is refused without
//!    `--allow-remote`. We never resolve DNS to *grant* loopback, so a
//!    `http://attacker.example/` that rebinds to `127.0.0.1` is rejected by the
//!    name check before any resolution — the naive rebind-to-loopback bypass
//!    cannot reach the accept path.
//! 2. **IP literals are range-checked directly** (incl. IPv4-mapped IPv6), so a
//!    `http://169.254.169.254/` or `http://[::ffff:169.254.169.254]/` literal is
//!    refused regardless of the flag.
//!
//! KNOWN, ACCEPTED LIMITATION: with the explicit `--allow-remote` opt-in, a
//! *named* host that resolves to a metadata / loopback IP is NOT socket-layer
//! gated (we do not resolve-then-gate). At that point the operator has
//! deliberately disclosed-accepted remote egress for this run; the flag is the
//! consent boundary. If d3zwe ever accepts a non-operator endpoint source, this
//! must be upgraded to the SSRF-1 fetch-time resolved-IP gate.

use std::net::{Ipv4Addr, Ipv6Addr};

use url::{Host, Url};

/// The classification of an endpoint host, independent of policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostClass {
    /// `127.0.0.0/8`, `::1`, or the literal `localhost`.
    Loopback,
    /// IPv4 `169.254.0.0/16` or IPv6 `fe80::/10` — link-local / cloud-metadata.
    /// ALWAYS rejected: never reachable as a legitimate local model endpoint and
    /// a classic SSRF/exfil target.
    LinkLocalOrMetadata,
    /// Any other host: LAN, public internet, cloud. Allowed only behind the
    /// explicit `--allow-remote` opt-in.
    Remote,
}

/// Why an endpoint was rejected (SEC-5). Carries an operator-facing message.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EndpointError {
    /// The string did not parse as an absolute URL.
    #[error("endpoint is not a valid URL: {0}")]
    Unparseable(String),
    /// The scheme was not http/https.
    #[error("endpoint scheme `{0}` is not supported (expected http or https)")]
    UnsupportedScheme(String),
    /// The URL had no host component (e.g. a bare path).
    #[error("endpoint has no host: {0}")]
    MissingHost(String),
    /// A non-loopback host without `--allow-remote`.
    #[error(
        "endpoint host `{host}` is not loopback; refusing by default (SEC-5). \
         Pass --allow-remote to send the model your prompt content over a \
         non-loopback connection (advanced — the endpoint becomes a data sink)."
    )]
    RemoteNotAllowed { host: String },
    /// A link-local / metadata host, rejected even with `--allow-remote`.
    #[error(
        "endpoint host `{host}` is in the link-local / cloud-metadata range \
         (169.254.0.0/16 or fe80::/10) and is ALWAYS refused (SEC-5)"
    )]
    LinkLocalAlwaysRefused { host: String },
}

/// Classify the host of a parsed URL. Pure; no DNS, no I/O.
pub fn classify_host(host: &Host<&str>) -> HostClass {
    match host {
        Host::Domain(name) => {
            if name.eq_ignore_ascii_case("localhost") {
                HostClass::Loopback
            } else {
                HostClass::Remote
            }
        }
        Host::Ipv4(addr) => classify_ipv4(*addr),
        Host::Ipv6(addr) => classify_ipv6(*addr),
    }
}

fn classify_ipv4(addr: Ipv4Addr) -> HostClass {
    // Link-local / metadata FIRST: 169.254.0.0/16 must never be treated as
    // anything but refused, even though it is not loopback.
    if addr.octets()[0] == 169 && addr.octets()[1] == 254 {
        return HostClass::LinkLocalOrMetadata;
    }
    // Entire 127.0.0.0/8 loopback block.
    if addr.octets()[0] == 127 {
        return HostClass::Loopback;
    }
    HostClass::Remote
}

fn classify_ipv6(addr: Ipv6Addr) -> HostClass {
    // ::1 loopback.
    if addr == Ipv6Addr::LOCALHOST {
        return HostClass::Loopback;
    }
    // fe80::/10 link-local.
    let first = addr.segments()[0];
    if (first & 0xffc0) == 0xfe80 {
        return HostClass::LinkLocalOrMetadata;
    }
    // An IPv4-mapped (::ffff:a.b.c.d) or IPv4-compatible address can smuggle the
    // metadata / loopback IPv4 ranges through an IPv6 literal. Re-classify via
    // the embedded IPv4 so `::ffff:169.254.169.254` and `::ffff:127.0.0.1` are
    // caught by the same rules rather than slipping through as Remote.
    if let Some(v4) = addr.to_ipv4_mapped() {
        return classify_ipv4(v4);
    }
    if let Some(v4) = addr.to_ipv4() {
        // `to_ipv4()` also yields the mapped form; guard against the unspecified
        // `::` / `::1` we already handled by only re-classifying real v4 ranges.
        if v4 != Ipv4Addr::UNSPECIFIED {
            return classify_ipv4(v4);
        }
    }
    HostClass::Remote
}

/// Validate a model endpoint string against SEC-5.
///
/// * Loopback hosts are accepted regardless of `allow_remote`.
/// * Link-local / metadata hosts are ALWAYS rejected (even with `allow_remote`).
/// * Any other host is accepted ONLY when `allow_remote` is `true`.
///
/// Returns the parsed [`Url`] on success so the caller does not re-parse.
pub fn validate_endpoint(raw: &str, allow_remote: bool) -> Result<Url, EndpointError> {
    let url = Url::parse(raw).map_err(|e| EndpointError::Unparseable(format!("{raw}: {e}")))?;

    match url.scheme() {
        "http" | "https" => {}
        other => return Err(EndpointError::UnsupportedScheme(other.to_string())),
    }

    let host = url
        .host()
        .ok_or_else(|| EndpointError::MissingHost(raw.to_string()))?;

    match classify_host(&host) {
        HostClass::Loopback => Ok(url),
        HostClass::LinkLocalOrMetadata => Err(EndpointError::LinkLocalAlwaysRefused {
            host: host.to_string(),
        }),
        HostClass::Remote => {
            if allow_remote {
                Ok(url)
            } else {
                Err(EndpointError::RemoteNotAllowed {
                    host: host.to_string(),
                })
            }
        }
    }
}

/// A pre-validated loopback-only endpoint URL.
///
/// Wraps a [`Url`] that has been accepted by [`validate_endpoint`] with
/// `allow_remote = false`. Callers that only ever talk to a local model shim
/// (the common case) construct this instead of calling `validate_endpoint`
/// directly to make the loopback constraint explicit in the type.
pub struct LoopbackEndpoint(pub Url);

impl LoopbackEndpoint {
    /// Parse and validate `raw` as a loopback-only endpoint (SEC-5,
    /// `allow_remote = false`).
    pub fn parse(raw: &str) -> Result<Self, EndpointError> {
        validate_endpoint(raw, false).map(LoopbackEndpoint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The SEC-5 acceptance table from the plan: loopback accepted; LAN +
    // metadata rejected without the flag.
    #[test]
    fn loopback_v4_accepted_without_flag() {
        assert!(validate_endpoint("http://127.0.0.1:8080/v1/chat/completions", false).is_ok());
        // The whole /8, not just .0.1.
        assert!(validate_endpoint("http://127.5.6.7:1234/v1/chat/completions", false).is_ok());
    }

    #[test]
    fn loopback_v6_accepted_without_flag() {
        assert!(validate_endpoint("http://[::1]:8080/v1/chat/completions", false).is_ok());
    }

    #[test]
    fn localhost_name_accepted_without_flag() {
        assert!(validate_endpoint("http://localhost:8080/v1/chat/completions", false).is_ok());
        assert!(validate_endpoint("http://LOCALHOST:8080/v1", false).is_ok());
    }

    #[test]
    fn https_loopback_accepted() {
        assert!(validate_endpoint("https://127.0.0.1/v1/chat/completions", false).is_ok());
    }

    #[test]
    fn lan_rejected_without_flag() {
        let err = validate_endpoint("http://192.168.1.50:8080/v1", false).unwrap_err();
        assert!(
            matches!(err, EndpointError::RemoteNotAllowed { .. }),
            "got {err:?}"
        );
        // And the other common private ranges.
        assert!(validate_endpoint("http://10.0.0.5:8080/v1", false).is_err());
        assert!(validate_endpoint("http://172.16.4.4:8080/v1", false).is_err());
    }

    #[test]
    fn lan_accepted_with_flag() {
        assert!(validate_endpoint("http://192.168.1.50:8080/v1", true).is_ok());
    }

    #[test]
    fn metadata_ip_rejected_without_flag() {
        let err = validate_endpoint("http://169.254.169.254/latest/meta-data", false).unwrap_err();
        assert!(
            matches!(err, EndpointError::LinkLocalAlwaysRefused { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn metadata_ip_rejected_even_with_flag() {
        // The whole 169.254.0.0/16 link-local block is refused regardless of the
        // remote opt-in — it is never a legitimate model endpoint.
        let err = validate_endpoint("http://169.254.169.254/latest/meta-data", true).unwrap_err();
        assert!(
            matches!(err, EndpointError::LinkLocalAlwaysRefused { .. }),
            "got {err:?}"
        );
        let err2 = validate_endpoint("http://169.254.1.1/v1", true).unwrap_err();
        assert!(matches!(err2, EndpointError::LinkLocalAlwaysRefused { .. }));
    }

    #[test]
    fn ipv6_link_local_rejected_even_with_flag() {
        let err = validate_endpoint("http://[fe80::1]/v1", true).unwrap_err();
        assert!(
            matches!(err, EndpointError::LinkLocalAlwaysRefused { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn ipv4_mapped_metadata_rejected() {
        // ::ffff:169.254.169.254 must not slip through as a Remote IPv6 literal.
        let err = validate_endpoint("http://[::ffff:169.254.169.254]/v1", true).unwrap_err();
        assert!(
            matches!(err, EndpointError::LinkLocalAlwaysRefused { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn ipv4_mapped_loopback_accepted() {
        // ::ffff:127.0.0.1 is loopback.
        assert!(validate_endpoint("http://[::ffff:127.0.0.1]:8080/v1", false).is_ok());
    }

    #[test]
    fn public_domain_rejected_without_flag() {
        let err = validate_endpoint("https://api.openai.com/v1/chat/completions", false)
            .unwrap_err();
        assert!(
            matches!(err, EndpointError::RemoteNotAllowed { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn public_domain_accepted_with_flag() {
        assert!(
            validate_endpoint("https://api.openai.com/v1/chat/completions", true).is_ok()
        );
    }

    #[test]
    fn unsupported_scheme_rejected() {
        let err = validate_endpoint("ftp://127.0.0.1/v1", false).unwrap_err();
        assert!(matches!(err, EndpointError::UnsupportedScheme(_)), "got {err:?}");
        // file:// has no host and an unsupported scheme — scheme check fires first.
        assert!(validate_endpoint("file:///etc/passwd", false).is_err());
    }

    #[test]
    fn unparseable_rejected() {
        assert!(matches!(
            validate_endpoint("not a url", false).unwrap_err(),
            EndpointError::Unparseable(_)
        ));
        assert!(validate_endpoint("", false).is_err());
    }

    #[test]
    fn classify_table() {
        let cases = [
            ("http://127.0.0.1/", HostClass::Loopback),
            ("http://127.255.255.255/", HostClass::Loopback),
            ("http://[::1]/", HostClass::Loopback),
            ("http://localhost/", HostClass::Loopback),
            ("http://169.254.169.254/", HostClass::LinkLocalOrMetadata),
            ("http://[fe80::abcd]/", HostClass::LinkLocalOrMetadata),
            ("http://8.8.8.8/", HostClass::Remote),
            ("http://192.168.0.1/", HostClass::Remote),
            ("http://example.com/", HostClass::Remote),
        ];
        for (raw, want) in cases {
            let url = Url::parse(raw).unwrap();
            let host = url.host().unwrap();
            assert_eq!(classify_host(&host), want, "host of {raw}");
        }
    }

    #[test]
    fn loopback_endpoint_parse_accepts_loopback() {
        let ep = LoopbackEndpoint::parse("http://127.0.0.1:11434/v1/chat/completions");
        assert!(ep.is_ok());
    }

    #[test]
    fn loopback_endpoint_parse_rejects_remote() {
        let err = LoopbackEndpoint::parse("https://api.openai.com/v1/chat/completions");
        assert!(err.is_err());
    }
}
