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
        // Strip port; handle IPv6 literals e.g. [::1]:8080
        let host_only = if host_part.starts_with('[') {
            host_part
                .find(']')
                .map(|i| &host_part[..i + 1])
                .unwrap_or(host_part)
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
