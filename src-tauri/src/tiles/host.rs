//! `tiles::host` — URL-shape validation and resolved-IP allow/deny policy.
//!
//! ## SSRF boundary (§8.3)
//!
//! The gatekeeper is a backend deputy fetching an operator/webview-influenced
//! host. Config-time string validation is insufficient — DNS rebinding defeats
//! it. This module ships two pure predicates:
//!
//! 1. `validate_source_url` — URL-shape checks (scheme, no embedded creds,
//!    host present). Returns a parsed `reqwest::Url` on success.
//!
//! 2. `ip_is_permitted` — post-DNS allow/deny on a *resolved* `IpAddr`.
//!    Allows RFC 1918 + IPv6 ULA (LAN tile servers live there). Denies
//!    everything else: public internet, loopback (unless `allow_loopback`
//!    is set for dev/test), link-local (covers the 169.254.169.254
//!    cloud-metadata oracle), multicast, and unspecified.
//!
//! **The resolve-then-vet wiring is Phase 3's job.** This file ships only
//! pure predicates + unit tests — no DNS, no network I/O.

use std::net::IpAddr;

use reqwest::Url;

/// Validate the URL shape of a tile-source URL.
///
/// Checks:
/// - Scheme is `http` or `https`.
/// - No embedded credentials (username or password in the URL).
/// - `host` component is present and non-empty.
///
/// Returns the parsed `Url` on success, or a human-readable error string.
pub fn validate_source_url(url_str: &str) -> Result<Url, String> {
    let url = Url::parse(url_str)
        .map_err(|e| format!("invalid URL {url_str:?}: {e}"))?;

    match url.scheme() {
        "http" | "https" => {}
        other => {
            return Err(format!(
                "tile source URL must use http or https scheme, got {other:?}"
            ))
        }
    }

    if !url.username().is_empty() || url.password().is_some() {
        return Err(
            "tile source URL must not contain embedded credentials (user:password@…)".into(),
        );
    }

    // Reject empty authority: `http:///path` is normalized by the url crate into
    // `http://path/` (treating the path component as the host), but the raw
    // string's authority section is empty — no actual host was specified.
    // Detect by checking that `://` is followed immediately by `/` or `?` (i.e.,
    // the authority block is absent or empty) in the original input.
    if let Some(after_scheme) = url_str.find("://").map(|i| &url_str[i + 3..]) {
        let authority_end = after_scheme
            .find('/')
            .unwrap_or(after_scheme.len());
        let authority = &after_scheme[..authority_end];
        // Strip credentials if present to get just the host:port
        let host_part = authority
            .rsplit_once('@')
            .map(|(_, h)| h)
            .unwrap_or(authority);
        // Strip port
        let host_only = if let Some(bracket_end) = host_part.strip_prefix('[').and_then(|s| s.find(']')) {
            &host_part[..bracket_end + 2] // IPv6 literal including brackets
        } else {
            host_part.split(':').next().unwrap_or("")
        };
        if host_only.is_empty() {
            return Err("tile source URL must include a host".into());
        }
    } else {
        return Err("tile source URL must include a host".into());
    }

    Ok(url)
}

/// Allow/deny a *resolved* IP address for use as a tile-server destination.
///
/// ## Allow policy
/// - IPv4 RFC 1918: `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`
/// - IPv6 ULA: `fc00::/7`
/// - Loopback (any family) — ONLY when `allow_loopback` is `true`
///   (intended for dev/test setups only; never set in production).
///
/// ## Deny policy (everything else)
/// - Public internet — the tile server MUST be on the LAN.
/// - Loopback (`127.0.0.0/8`, `::1`) when `allow_loopback = false`.
/// - Unspecified (`0.0.0.0`, `::`).
/// - IPv4 link-local `169.254.0.0/16` — covers the cloud-metadata oracle
///   `169.254.169.254` that cloud SSRF attacks target.
/// - IPv6 link-local `fe80::/10`.
/// - Multicast (`224.0.0.0/4`, `ff00::/8`).
/// - IPv4-mapped IPv6 (e.g. `::ffff:127.0.0.1`) — canonicalized via
///   `IpAddr::to_canonical()` so the mapped-loopback case is caught by
///   the loopback deny above. This is the `to_canonical` regression guard.
///
/// ## Boundary note
/// This function is **pure**: it operates on an already-resolved `IpAddr`
/// and performs no DNS resolution or network I/O. The resolve-then-vet
/// wiring (Phase 3) calls this after tokio's `lookup_host`; config-time
/// validation may call it as a courtesy warning, but it is NEVER the
/// security control there.
pub fn ip_is_permitted(addr: IpAddr, allow_loopback: bool) -> bool {
    // Normalize IPv4-mapped IPv6 addresses (`::ffff:a.b.c.d`) to their IPv4
    // form so all subsequent checks operate on a canonical representation.
    // `to_canonical()` is stable since Rust 1.75 (repo MSRV).
    let canonical = addr.to_canonical();

    // Deny loopback (unless explicitly opted-in for dev/test).
    if canonical.is_loopback() {
        return allow_loopback;
    }

    // Deny unspecified (0.0.0.0, ::).
    if canonical.is_unspecified() {
        return false;
    }

    // Deny multicast.
    if canonical.is_multicast() {
        return false;
    }

    match canonical {
        IpAddr::V4(v4) => {
            let octets = v4.octets();

            // Deny IPv4 link-local 169.254.0.0/16.
            // This range covers the cloud-metadata oracle 169.254.169.254 —
            // the canonical SSRF target in cloud environments.
            if octets[0] == 169 && octets[1] == 254 {
                return false;
            }

            // Deny 0.0.0.0/8 (handled by is_unspecified above for 0.0.0.0
            // exactly, but the broader /8 needs an explicit check).
            if octets[0] == 0 {
                return false;
            }

            // Allow RFC 1918 private ranges only.
            // 10.0.0.0/8
            if octets[0] == 10 {
                return true;
            }
            // 172.16.0.0/12 (172.16.0.0 – 172.31.255.255)
            if octets[0] == 172 && (16..=31).contains(&octets[1]) {
                return true;
            }
            // 192.168.0.0/16
            if octets[0] == 192 && octets[1] == 168 {
                return true;
            }

            // Everything else (public internet, etc.) is denied.
            false
        }
        IpAddr::V6(v6) => {
            // Deny IPv6 link-local fe80::/10.
            // fe80:: has the top 10 bits 1111 1110 10, i.e. the first two
            // bytes are 0xfe and the top 6 bits of the second byte are 10xxxx,
            // which means the second byte is in 0x80..=0xbf.
            let seg0 = v6.segments()[0];
            if (seg0 & 0xffc0) == 0xfe80 {
                return false;
            }

            // Allow ULA fc00::/7 (fc00:: and fd00:: ranges).
            // Top 7 bits of the first 16-bit segment are 1111 110, i.e.
            // seg0 & 0xfe00 == 0xfc00.
            if (seg0 & 0xfe00) == 0xfc00 {
                return true;
            }

            // Everything else denied.
            false
        }
    }
}

#[cfg(test)]
mod url_tests {
    use super::*;

    #[test]
    fn rejects_non_http_schemes() {
        assert!(validate_source_url("file:///etc/passwd").is_err());
        assert!(validate_source_url("gopher://x/").is_err());
        assert!(validate_source_url("ftp://x/").is_err());
    }

    #[test]
    fn rejects_embedded_credentials() {
        assert!(validate_source_url("http://user:pass@192.168.1.5:8080/").is_err());
    }

    #[test]
    fn accepts_plain_http_and_https_with_host() {
        assert!(validate_source_url("http://192.168.1.5:8080/tiles/").is_ok());
        assert!(validate_source_url("https://tiles.lan/").is_ok());
    }

    #[test]
    fn rejects_missing_host() {
        assert!(validate_source_url("http:///x").is_err());
    }
}

#[cfg(test)]
mod ip_tests {
    use super::*;
    use std::net::IpAddr;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn allows_rfc1918_and_ula() {
        for s in ["10.0.0.1", "172.16.5.4", "192.168.1.50", "fd00::1"] {
            assert!(ip_is_permitted(ip(s), false), "{s} should be permitted");
        }
    }

    #[test]
    fn denies_public_loopback_linklocal_metadata_multicast_unspecified() {
        for s in [
            "8.8.8.8",
            "1.1.1.1",
            "127.0.0.1",
            "::1",
            "169.254.169.254",
            "169.254.1.1",
            "fe80::1",
            "224.0.0.1",
            "0.0.0.0",
            "::",
            "::ffff:127.0.0.1",
        ] {
            assert!(!ip_is_permitted(ip(s), false), "{s} should be denied");
        }
    }

    #[test]
    fn loopback_allowed_only_with_dev_optin() {
        assert!(ip_is_permitted(ip("127.0.0.1"), true));
        assert!(!ip_is_permitted(ip("127.0.0.1"), false));
    }
}
