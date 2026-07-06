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
//!
//! ## qe6ie: TLS required for non-loopback hosts
//!
//! A non-loopback model endpoint is refused unless the scheme is `https`
//! (`EndpointError::PlaintextRemoteRefused`). The model is an
//! untrusted-instruction channel — Tuxlink executes the tool-calls it returns —
//! so a plaintext, MITM-rewritable channel to a non-loopback host is refused by
//! default. "Valid TLS" itself is reqwest's default certificate validation at
//! request time (see `egress::build_vetted_client`); this module only forbids the
//! plaintext scheme and never judges host form (named vs bare IP). Loopback is
//! exempt (same trust domain, the OAuth-2.1 loopback carve-out).

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
    /// A non-loopback host reached over plain `http` (qe6ie trust boundary). A
    /// remote model endpoint is an untrusted-instruction channel: Tuxlink
    /// executes the tool-calls the model returns, so over plaintext an on-path
    /// device can rewrite them. Require TLS for any non-loopback host. reqwest's
    /// default validation enforces a *valid* certificate at request time; this
    /// variant only refuses the plaintext scheme. Host form (named vs bare IP) is
    /// not judged here — cert matching is reqwest's job.
    #[error(
        "endpoint host `{host}` uses plain http; a remote (non-loopback) model \
         endpoint must use https. Point Tuxlink at an https:// URL backed by a \
         valid TLS certificate, or use a local (loopback) or cloud endpoint. \
         See Help > Settings > 'AI agent model endpoint'."
    )]
    PlaintextRemoteRefused { host: String },
    /// A link-local / metadata host, rejected even with `--allow-remote`.
    #[error(
        "endpoint host `{host}` is in the link-local / cloud-metadata range \
         (169.254.0.0/16 or fe80::/10) and is ALWAYS refused (SEC-5)"
    )]
    LinkLocalAlwaysRefused { host: String },
    /// The URL contained a userinfo component (username and/or password).
    ///
    /// Credentials in a URL are rejected: they appear in logs, config files,
    /// and error messages. Use the keyring (Task B1) for secret storage instead.
    #[error(
        "endpoint host `{host}` URL contains userinfo (username/password); \
         credentials in URLs are not allowed — use the keyring instead (SEC-5)"
    )]
    UserinfoNotAllowed { host: String },
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
/// * A non-loopback host over plain `http` is refused (`PlaintextRemoteRefused`);
///   remote endpoints must use `https`. Loopback is exempt.
///
/// Returns the parsed [`Url`] on success so the caller does not re-parse.
pub fn validate_endpoint(raw: &str, allow_remote: bool) -> Result<Url, EndpointError> {
    let url = Url::parse(raw).map_err(|e| EndpointError::Unparseable(format!("{raw}: {e}")))?;

    // Validate scheme and remember whether it is TLS. The scheme is constrained
    // to http|https here; the TLS rule below applies only to non-loopback hosts.
    let is_https = match url.scheme() {
        "https" => true,
        "http" => false,
        other => return Err(EndpointError::UnsupportedScheme(other.to_string())),
    };

    let host = url
        .host()
        .ok_or_else(|| EndpointError::MissingHost(raw.to_string()))?;

    match classify_host(&host) {
        HostClass::Loopback => Ok(url),
        HostClass::LinkLocalOrMetadata => Err(EndpointError::LinkLocalAlwaysRefused {
            host: host.to_string(),
        }),
        HostClass::Remote => {
            if !allow_remote {
                return Err(EndpointError::RemoteNotAllowed {
                    host: host.to_string(),
                });
            }
            // qe6ie trust boundary: a non-loopback model endpoint is an
            // untrusted-instruction channel, so it MUST use TLS. Loopback is
            // exempt (handled above): same trust domain, the OAuth-2.1 loopback
            // carve-out. Host form is deliberately not judged — reqwest decides
            // whether the cert validates at request time. NOTE precedence: this
            // fires before AgentEndpoint::parse's userinfo check, so a remote
            // `http://creds@host` reports plaintext refusal first (fails closed;
            // the message names only the host, never the credentials).
            if !is_https {
                return Err(EndpointError::PlaintextRemoteRefused {
                    host: host.to_string(),
                });
            }
            Ok(url)
        }
    }
}

/// A pre-validated loopback-only endpoint URL.
///
/// Wraps a [`Url`] that has been accepted by [`validate_endpoint`] with
/// `allow_remote = false`. Callers that only ever talk to a local model shim
/// (the common case) construct this instead of calling `validate_endpoint`
/// directly to make the loopback constraint explicit in the type.
#[derive(Debug, Clone)]
pub struct LoopbackEndpoint(pub Url);

impl LoopbackEndpoint {
    /// Parse and validate `raw` as a loopback-only endpoint (SEC-5,
    /// `allow_remote = false`).
    pub fn parse(raw: &str) -> Result<Self, EndpointError> {
        validate_endpoint(raw, false).map(LoopbackEndpoint)
    }
}

/// A pre-validated agent model endpoint URL that permits remote hosts but
/// rejects link-local/metadata ranges and credentials-in-URL.
///
/// Unlike [`LoopbackEndpoint`], this type accepts public internet hosts (e.g.
/// `api.openai.com`) — because the Elmer agent config explicitly opts the
/// operator into remote LLM egress. However:
///
/// - Link-local / cloud-metadata ranges (`169.254.0.0/16`, `fe80::/10`) are
///   ALWAYS refused even here (they are never legitimate model endpoints and
///   are classic SSRF/exfil targets).
/// - Credentials in the URL (`user:pass@host`) are refused. They appear in
///   logs and error messages; secrets must go through the keyring (Task B1).
///
/// ## Origin contract (cross-language, used as keyring account suffix in Task B1)
///
/// `origin()` returns `self.0.origin().ascii_serialization()` — the url
/// crate's canonical tuple-origin form. This string is used as the keyring
/// account suffix and as the preset-inference key in the TypeScript frontend
/// (Task G1). A Rust/TS mismatch silently desyncs the stored credential key
/// from the endpoint being configured. The following test vectors are the
/// contract; they MUST hold in both this Rust `origin()` and the TS
/// `originOf()` function in Task G1:
///
/// | URL                                              | origin()                     | notes                              |
/// |--------------------------------------------------|------------------------------|------------------------------------|
/// | `https://API.OpenAI.com:443/v1/chat/completions` | `https://api.openai.com`     | 443 is https default → omitted     |
/// | `http://127.0.0.1:11434/v1/chat/completions`     | `http://127.0.0.1:11434`     | non-default port → kept            |
/// | `https://openrouter.ai/api/v1/chat/completions`  | `https://openrouter.ai`      | default port, no subdomain         |
#[derive(Debug, Clone)]
pub struct AgentEndpoint(pub Url);

impl AgentEndpoint {
    /// Parse and validate `raw` as an agent model endpoint (SEC-5,
    /// `allow_remote = true`).
    ///
    /// Accepts loopback and remote hosts; rejects link-local/metadata ranges
    /// and any URL that carries a userinfo component (username or password).
    /// The userinfo check runs AFTER host classification so that a loopback
    /// URL with credentials is also refused.
    pub fn parse(raw: &str) -> Result<Self, EndpointError> {
        // validate_endpoint with allow_remote=true handles scheme, host
        // presence, loopback/link-local/remote classification.
        let url = validate_endpoint(raw, true)?;

        // Userinfo check runs after host-class validation so that
        // `http://u:p@127.0.0.1/v1` is refused (not silently accepted as
        // loopback). Use url.username() and url.password() directly — the url
        // crate populates them only when the authority contains a `@`.
        if !url.username().is_empty() || url.password().is_some() {
            let host = url.host_str().unwrap_or_default().to_string();
            return Err(EndpointError::UserinfoNotAllowed { host });
        }

        Ok(AgentEndpoint(url))
    }

    /// Returns `true` if the endpoint host is loopback (`127.0.0.0/8`,
    /// `::1`, or the literal `localhost`).
    pub fn is_loopback(&self) -> bool {
        match self.0.host() {
            Some(host) => matches!(classify_host(&host), HostClass::Loopback),
            None => false,
        }
    }

    /// Returns the scheme+host+port origin as a canonical ASCII string.
    ///
    /// Uses the `url` crate's `Origin::ascii_serialization()` so the result
    /// is guaranteed lowercase and includes the port only when it differs from
    /// the scheme default. This string is the keyring account suffix (Task B1)
    /// and the preset-inference key (Task G1) — do NOT hand-roll it.
    pub fn origin(&self) -> String {
        self.0.origin().ascii_serialization()
    }

    /// Returns a reference to the inner validated [`Url`].
    pub fn url(&self) -> &Url {
        &self.0
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
    fn lan_http_refused_with_flag() {
        // The hole this issue closes: a non-loopback host over plain http was
        // accepted with the remote opt-in. It is now refused for want of TLS.
        let err = validate_endpoint("http://192.168.1.50:8080/v1", true).unwrap_err();
        assert!(
            matches!(err, EndpointError::PlaintextRemoteRefused { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn lan_https_accepted_with_flag() {
        // Same host over TLS is accepted (reqwest validates the cert at request
        // time; the validator only gates the scheme). Bare-IP TLS is not
        // special-cased.
        assert!(validate_endpoint("https://192.168.1.50:8080/v1", true).is_ok());
    }

    #[test]
    fn remote_named_http_refused_with_flag() {
        // A named non-loopback host over http is refused just like a bare IP.
        let err = validate_endpoint("http://model.internal.example/v1", true).unwrap_err();
        assert!(
            matches!(err, EndpointError::PlaintextRemoteRefused { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn remote_named_https_accepted_with_flag() {
        assert!(validate_endpoint("https://model.internal.example/v1", true).is_ok());
    }

    #[test]
    fn loopback_http_still_accepted_after_tls_rule() {
        // Loopback is exempt from the TLS rule (same trust domain). Regression
        // lock for first-class local operation.
        assert!(validate_endpoint("http://127.0.0.1:11434/v1", false).is_ok());
        assert!(validate_endpoint("http://localhost:11434/v1", false).is_ok());
        assert!(validate_endpoint("http://[::1]:11434/v1", false).is_ok());
    }

    #[test]
    fn link_local_https_still_refused() {
        // The TLS rule does NOT relax the always-refuse link-local/metadata rule.
        let err = validate_endpoint("https://169.254.169.254/v1", true).unwrap_err();
        assert!(
            matches!(err, EndpointError::LinkLocalAlwaysRefused { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn plaintext_remote_refusal_is_distinct() {
        // PlaintextRemoteRefused (remote + http + allow_remote) must not be
        // confused with RemoteNotAllowed (remote + !allow_remote) or
        // UnsupportedScheme (neither http nor https).
        let plaintext = validate_endpoint("http://192.168.1.50/v1", true).unwrap_err();
        let not_allowed = validate_endpoint("http://192.168.1.50/v1", false).unwrap_err();
        let bad_scheme = validate_endpoint("ftp://192.168.1.50/v1", true).unwrap_err();
        assert!(matches!(plaintext, EndpointError::PlaintextRemoteRefused { .. }));
        assert!(matches!(not_allowed, EndpointError::RemoteNotAllowed { .. }));
        assert!(matches!(bad_scheme, EndpointError::UnsupportedScheme(_)));
        // The operator-facing message names the offending host and mentions https.
        let msg = plaintext.to_string();
        assert!(msg.contains("192.168.1.50"), "message must name the host: {msg}");
        assert!(msg.contains("https"), "message must point at https: {msg}");
    }

    #[test]
    fn loopback_lookalikes_refused_over_http() {
        // Hosts that superficially resemble loopback but are NOT must classify as
        // Remote and be refused over plain http — they cannot ride the loopback
        // plaintext exemption. Locks the boundary against future url-parser drift.
        for raw in [
            "http://localhost./v1",         // trailing dot: not literal "localhost"
            "http://127.0.0.1.evil.com/v1", // loopback IP as a subdomain label
            "http://0.0.0.0:8080/v1",       // unspecified v4, not loopback
            "http://[::]:8080/v1",          // unspecified v6, not loopback
        ] {
            let err = validate_endpoint(raw, true).unwrap_err();
            assert!(
                matches!(err, EndpointError::PlaintextRemoteRefused { .. }),
                "{raw} must be refused as plaintext remote; got {err:?}"
            );
        }
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

    // ── AgentEndpoint tests ────────────────────────────────────────────────
    //
    // These tests were written FIRST (TDD), before the AgentEndpoint
    // implementation was added, and serve as the contract for the type.
    // They cannot be run locally on this Pi (cargo won't finish a cold
    // build on contended hardware); CI verifies them.

    /// Loopback URL is accepted; is_loopback() returns true.
    #[test]
    fn agent_endpoint_accepts_loopback() {
        let ep =
            AgentEndpoint::parse("http://127.0.0.1:11434/v1/chat/completions").unwrap();
        assert!(
            ep.is_loopback(),
            "127.0.0.1 must classify as loopback"
        );
    }

    /// Public HTTPS URL is accepted (allow_remote=true); is_loopback() returns false.
    #[test]
    fn agent_endpoint_accepts_public_https() {
        let ep =
            AgentEndpoint::parse("https://api.openai.com/v1/chat/completions").unwrap();
        assert!(
            !ep.is_loopback(),
            "public domain must not classify as loopback"
        );
    }

    /// RFC-1918 LAN address over TLS is accepted; is_loopback() returns false.
    /// Bare-IP TLS is permitted, not special-cased — reqwest is the cert arbiter.
    #[test]
    fn agent_endpoint_accepts_rfc1918_https() {
        let ep =
            AgentEndpoint::parse("https://192.168.1.50:8080/v1/chat/completions").unwrap();
        assert!(
            !ep.is_loopback(),
            "RFC-1918 address must not classify as loopback"
        );
    }

    /// RFC-1918 LAN address over plain http is refused (qe6ie TLS rule).
    #[test]
    fn agent_endpoint_refuses_plaintext_rfc1918() {
        let err =
            AgentEndpoint::parse("http://192.168.1.50:8080/v1/chat/completions").unwrap_err();
        assert!(
            matches!(err, EndpointError::PlaintextRemoteRefused { .. }),
            "expected PlaintextRemoteRefused, got {err:?}"
        );
    }

    /// Cloud metadata / link-local address is ALWAYS refused, even with the
    /// allow_remote=true path that AgentEndpoint uses.
    #[test]
    fn agent_endpoint_refuses_metadata() {
        let err = AgentEndpoint::parse("http://169.254.169.254/v1").unwrap_err();
        assert!(
            matches!(err, EndpointError::LinkLocalAlwaysRefused { .. }),
            "expected LinkLocalAlwaysRefused, got {err:?}"
        );
    }

    /// URL with userinfo (user:pass@host) is refused. Uses https so the endpoint
    /// clears the TLS gate and reaches the userinfo check. (A remote *http*
    /// endpoint is refused as plaintext BEFORE userinfo is examined — see
    /// `agent_endpoint_plaintext_refused_before_userinfo`.)
    #[test]
    fn agent_endpoint_refuses_userinfo() {
        let err =
            AgentEndpoint::parse("https://user:pass@api.openai.com/v1").unwrap_err();
        assert!(
            matches!(err, EndpointError::UserinfoNotAllowed { .. }),
            "expected UserinfoNotAllowed, got {err:?}"
        );
    }

    /// Deliberate precedence: a remote http endpoint WITH userinfo reports the
    /// plaintext refusal first (validate_endpoint runs before the userinfo check
    /// in AgentEndpoint::parse). Still fails closed; the message names only the
    /// host, never the credentials.
    #[test]
    fn agent_endpoint_plaintext_refused_before_userinfo() {
        let err =
            AgentEndpoint::parse("http://user:pass@192.168.1.50/v1").unwrap_err();
        assert!(
            matches!(err, EndpointError::PlaintextRemoteRefused { .. }),
            "remote http+creds must report plaintext refusal first; got {err:?}"
        );
        assert!(!err.to_string().contains("pass"), "creds must not leak: {err}");
    }

    /// Non-http/https scheme is refused (scheme check fires before anything else).
    #[test]
    fn agent_endpoint_refuses_ftp() {
        let err = AgentEndpoint::parse("ftp://127.0.0.1/v1").unwrap_err();
        assert!(
            matches!(err, EndpointError::UnsupportedScheme(_)),
            "expected UnsupportedScheme, got {err:?}"
        );
    }

    /// origin() returns the canonical scheme+host+port string with path
    /// stripped and host lowercased.
    ///
    /// These three vectors are the cross-language contract (Rust origin() ↔
    /// TypeScript originOf() in Task G1) and the keyring account suffix
    /// (Task B1). Any mismatch silently desyncs stored credentials.
    ///
    /// | URL                                              | expected origin          |
    /// |--------------------------------------------------|--------------------------|
    /// | https://API.OpenAI.com:443/v1/chat/completions  | https://api.openai.com   |
    /// | http://127.0.0.1:11434/v1/chat/completions      | http://127.0.0.1:11434   |
    /// | https://openrouter.ai/api/v1/chat/completions   | https://openrouter.ai    |
    #[test]
    fn origin_strips_path_and_lowercases() {
        // Vector 1: default HTTPS port (443) must be omitted.
        let ep =
            AgentEndpoint::parse("https://API.OpenAI.com:443/v1/chat/completions")
                .unwrap();
        assert_eq!(
            ep.origin(),
            "https://api.openai.com",
            "443 is the https default — must not appear in origin"
        );

        // Vector 2: non-default port must be kept.
        let ep =
            AgentEndpoint::parse("http://127.0.0.1:11434/v1/chat/completions").unwrap();
        assert_eq!(
            ep.origin(),
            "http://127.0.0.1:11434",
            "non-default port 11434 must appear in origin"
        );

        // Vector 3: default HTTPS port implicitly absent, no subdomain.
        let ep =
            AgentEndpoint::parse("https://openrouter.ai/api/v1/chat/completions").unwrap();
        assert_eq!(
            ep.origin(),
            "https://openrouter.ai",
            "no port in origin when using scheme default"
        );
    }

    /// Userinfo on a loopback host is still refused — the check is
    /// unconditional and runs AFTER host classification, not before.
    /// This guards the ordering invariant in AgentEndpoint::parse.
    #[test]
    fn userinfo_check_runs_before_remote_accept() {
        let err = AgentEndpoint::parse("http://u:p@127.0.0.1/v1").unwrap_err();
        assert!(
            matches!(err, EndpointError::UserinfoNotAllowed { .. }),
            "loopback with credentials must be refused; got {err:?}"
        );
    }

    /// url() exposes the inner Url so callers can inspect path, query, etc.
    #[test]
    fn agent_endpoint_url_accessor() {
        let ep =
            AgentEndpoint::parse("https://api.openai.com/v1/chat/completions").unwrap();
        assert_eq!(ep.url().host_str(), Some("api.openai.com"));
        assert_eq!(ep.url().path(), "/v1/chat/completions");
    }

    /// Username-only (no password) in userinfo is also refused. https so it
    /// reaches the userinfo check (an http remote would refuse as plaintext first).
    /// Tests that the `!url.username().is_empty()` branch fires independently.
    #[test]
    fn agent_endpoint_refuses_username_only() {
        let err = AgentEndpoint::parse("https://user@api.openai.com/v1").unwrap_err();
        assert!(
            matches!(err, EndpointError::UserinfoNotAllowed { .. }),
            "username without password must also be refused; got {err:?}"
        );
    }
}
