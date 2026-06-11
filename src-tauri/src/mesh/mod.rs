//! AREDN mesh Post Office discovery (tuxlink-1w7t).
//!
//! Client-side discovery of Winlink RMS Relay / Post Office services advertised
//! on the LOCAL AREDN mesh, so the operator selects a relay instead of
//! hand-typing host:port. Source-grounded against AREDN firmware
//! (`aredn/aredn` @ d3f7282): `sysinfo.json` 307-redirects to `/a/sysinfo`,
//! whose `services=1` handler reads `/var/run/arednlink/services/` — populated
//! by the Babel-era **arednlink** daemon, NOT the deprecated OLSR nameservice —
//! emitting one `{ name, ip, link, protocol }` record per advertised service.
//!
//! NETWORK-CITIZENSHIP BOUNDARY (tuxlink-1w7t P2 — operator decision 2026-06-10):
//! discovery is a SINGLE LOCAL HTTP GET to the operator's own node. That fetch
//! generates ZERO mesh traffic — the node already learned services via arednlink
//! pub/sub. The ONLY wire traffic is the liveness probe, which is bounded
//! (low concurrency, short timeout), on-demand only (never a timer), and aimed
//! at advertised PO nodes on the LOCAL mesh only. DO NOT add background polling
//! or supernode-wide search: amplifying traffic across the (already overloaded /
//! mis-cross-linked) supernode network to save one station a manual lookup is a
//! bad trade. Local-mesh only, by design.

use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::config;
use crate::ui_commands::UiError;

/// AREDN node web server port serving sysinfo (verified in firmware).
const SYSINFO_PORT: u16 = 8080;
/// Default mesh node when the operator has not configured a master-node host.
const DEFAULT_MASTER_NODE: &str = "localnode.local.mesh";
/// RMS Relay plaintext-B2F default port (used when a link advertises no port).
const DEFAULT_B2F_PORT: u16 = 8772;
/// Cap on the (single, local) sysinfo fetch.
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
/// Per-relay liveness-probe timeout. Short — this is a TCP connect, not a session.
const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);
/// Bounded probe fan-out (good-citizen: a handful at a time, local mesh only).
const PROBE_CONCURRENCY: usize = 4;

/// A discovered Post Office / RMS Relay on the local mesh. Serialized snake_case
/// to the frontend (matches the project's DTO convention, e.g. `DialResult`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MeshPostOffice {
    /// Advertised service name, trimmed of cruft (field reports show trailing " - ").
    pub name: String,
    /// Numeric node IP — DIAL THIS, not the node name: `.local.mesh` names
    /// frequently fail to resolve/connect (Winlink Programs Group corpus).
    pub ip: String,
    /// B2F port parsed from the advertised link URL (default 8772).
    pub port: u16,
    /// Raw advertised link URL (display / debugging).
    pub link: String,
    /// TCP-reachable at probe time. NOT a verified B2F handshake — an open port
    /// only (a dead-but-listening process still shows reachable).
    pub reachable: bool,
    /// Round-trip time of the successful TCP connect, milliseconds.
    pub rtt_ms: Option<u32>,
}

/// AREDN `sysinfo.json?services=1` response. Partial — we read only `services`;
/// AREDN emits many other top-level keys which serde ignores (no
/// `deny_unknown_fields` here, unlike our own config types).
#[derive(Debug, Deserialize)]
struct SysinfoResponse {
    #[serde(default)]
    services: Vec<SysinfoService>,
}

/// One advertised service record. AREDN emits `{ name, ip, link, protocol }`;
/// `link` is `""` for info-only (`:0/`) advertisements per the firmware parser.
#[derive(Debug, Deserialize)]
struct SysinfoService {
    #[serde(default)]
    name: String,
    #[serde(default)]
    ip: String,
    #[serde(default)]
    link: String,
}

/// Phil W4PHS's default Relay filter (verified, Winlink Programs Group corpus):
/// a service marks a Winlink Post Office iff its name contains "WINLINK" or
/// "POST OFFICE" (case-insensitive).
fn is_post_office(name: &str) -> bool {
    let up = name.to_uppercase();
    up.contains("WINLINK") || up.contains("POST OFFICE")
}

/// Trim advertised-name cruft: field reports show names like
/// `"K0DZ-10-Winlink-Gateway - "` with a trailing separator. Strip trailing
/// whitespace and dangling separator dashes (internal dashes are preserved).
fn trim_name(name: &str) -> String {
    name.trim()
        .trim_end_matches([' ', '\t', '-'])
        .trim()
        .to_string()
}

/// Parse the B2F port from an advertised link URL, defaulting to 8772.
/// Links look like `http://10.5.3.2:8772/` or `winlink://10.5.3.2:8772`.
fn parse_port(link: &str) -> u16 {
    let after_scheme = link.split("://").nth(1).unwrap_or(link);
    let authority = after_scheme.split('/').next().unwrap_or(after_scheme);
    authority
        .rsplit(':')
        .next()
        .and_then(|p| p.parse::<u16>().ok())
        .filter(|&p| p > 0)
        .unwrap_or(DEFAULT_B2F_PORT)
}

/// Classify advertised services into Post Office candidates. Keeps services that
/// (a) mark a PO by name AND (b) have a non-empty link (AREDN sets link="" for
/// info-only `:0/` advertisements, which are non-connectable) AND (c) carry an
/// IP to dial. Reachability is filled in later by the probe.
fn classify(services: &[SysinfoService]) -> Vec<MeshPostOffice> {
    services
        .iter()
        .filter(|s| {
            is_post_office(&s.name) && !s.link.trim().is_empty() && !s.ip.trim().is_empty()
        })
        .map(|s| MeshPostOffice {
            name: trim_name(&s.name),
            ip: s.ip.trim().to_string(),
            port: parse_port(&s.link),
            link: s.link.clone(),
            reachable: false,
            rtt_ms: None,
        })
        .collect()
}

/// Rank for display: reachable first, then lowest RTT, then name (stable, total).
fn rank(list: &mut [MeshPostOffice]) {
    list.sort_by(|a, b| {
        b.reachable
            .cmp(&a.reachable)
            .then(
                a.rtt_ms
                    .unwrap_or(u32::MAX)
                    .cmp(&b.rtt_ms.unwrap_or(u32::MAX)),
            )
            .then_with(|| a.name.cmp(&b.name))
    });
}

/// Resolve the sysinfo host: explicit param → configured master node (P3a:
/// honor the operator's setting, unlike WLE which stores but ignores it) →
/// default `localnode.local.mesh`.
fn resolve_host(param: Option<String>) -> String {
    let norm = |s: String| {
        let t = s.trim().to_string();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    };
    param
        .and_then(norm)
        .or_else(|| {
            config::read_config()
                .ok()
                .and_then(|c| c.aredn_master_node_host)
                .and_then(norm)
        })
        .unwrap_or_else(|| DEFAULT_MASTER_NODE.to_string())
}

/// Fetch + parse the sysinfo services list. reqwest follows the 307 redirect to
/// `/a/sysinfo` by default. Plain HTTP (mesh-internal); no TLS.
async fn fetch_sysinfo(host: &str) -> Result<SysinfoResponse, UiError> {
    let url = format!("http://{host}:{SYSINFO_PORT}/cgi-bin/sysinfo.json?services=1");
    let client = reqwest::Client::builder()
        .user_agent("tuxlink-mesh-discovery")
        .timeout(FETCH_TIMEOUT)
        .build()
        .map_err(|e| UiError::Internal {
            detail: e.to_string(),
        })?;
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| UiError::Unavailable {
            reason: e.to_string(),
        })?;
    if !resp.status().is_success() {
        return Err(UiError::Unavailable {
            reason: format!("mesh node returned HTTP {}", resp.status()),
        });
    }
    resp.json::<SysinfoResponse>()
        .await
        .map_err(|e| UiError::Transport {
            reason: format!("malformed sysinfo response: {e}"),
        })
}

/// Bounded TCP liveness probe. Returns RTT millis on a successful connect within
/// the timeout, else None. Connect-only — does NOT verify B2F.
async fn probe_one(ip: &str, port: u16) -> Option<u32> {
    let addr = format!("{ip}:{port}");
    let start = std::time::Instant::now();
    match tokio::time::timeout(PROBE_TIMEOUT, tokio::net::TcpStream::connect(&addr)).await {
        Ok(Ok(_stream)) => Some(start.elapsed().as_millis().min(u128::from(u32::MAX)) as u32),
        _ => None,
    }
}

/// Discover Post Office / RMS Relay nodes on the local AREDN mesh.
///
/// One local sysinfo GET (zero mesh traffic) → classify → bounded on-demand
/// liveness probe (local mesh only) → rank. See the module-level
/// network-citizenship note: this never crawls the supernet.
#[tauri::command]
pub async fn mesh_discover_post_offices(
    master_node_host: Option<String>,
) -> Result<Vec<MeshPostOffice>, UiError> {
    use futures::stream::{self, StreamExt};

    let host = resolve_host(master_node_host);
    let resp = fetch_sysinfo(&host).await?;
    let candidates = classify(&resp.services);

    let mut probed: Vec<MeshPostOffice> = stream::iter(candidates)
        .map(|mut po| async move {
            po.rtt_ms = probe_one(&po.ip, po.port).await;
            po.reachable = po.rtt_ms.is_some();
            po
        })
        .buffer_unordered(PROBE_CONCURRENCY)
        .collect()
        .await;

    rank(&mut probed);
    Ok(probed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn svc(name: &str, ip: &str, link: &str) -> SysinfoService {
        SysinfoService {
            name: name.to_string(),
            ip: ip.to_string(),
            link: link.to_string(),
        }
    }

    #[test]
    fn is_post_office_matches_winlink_or_post_office_case_insensitive() {
        assert!(is_post_office("Winlink Post Office"));
        assert!(is_post_office("K0DZ-10 WINLINK Gateway"));
        assert!(is_post_office("some post office node"));
        assert!(is_post_office("WL2K Winlink")); // contains WINLINK
        assert!(!is_post_office("Generic Web Server"));
        assert!(!is_post_office("wl2k")); // 'wl2k' alone is NOT the Relay filter
        assert!(!is_post_office("Camera"));
    }

    #[test]
    fn trim_name_strips_trailing_cruft_keeps_internal_dashes() {
        assert_eq!(trim_name("K0DZ-10-Winlink-Gateway - "), "K0DZ-10-Winlink-Gateway");
        assert_eq!(trim_name("WINLINK POST OFFICE - "), "WINLINK POST OFFICE");
        assert_eq!(trim_name("  Winlink Relay  "), "Winlink Relay");
        assert_eq!(trim_name("WL2K-MESH"), "WL2K-MESH");
    }

    #[test]
    fn parse_port_extracts_or_defaults_8772() {
        assert_eq!(parse_port("http://10.5.3.2:8772/"), 8772);
        assert_eq!(parse_port("winlink://10.5.7.9:8080"), 8080);
        assert_eq!(parse_port("http://10.5.3.2/"), DEFAULT_B2F_PORT); // no port
        assert_eq!(parse_port("10.5.3.2:9999"), 9999);
        assert_eq!(parse_port(""), DEFAULT_B2F_PORT);
        assert_eq!(parse_port("http://10.5.3.2:0/"), DEFAULT_B2F_PORT); // :0 → default
    }

    #[test]
    fn classify_keeps_post_offices_with_links_drops_info_only_and_non_po() {
        // Mirrors the AREDN services[] shape from sysinfo.ut.
        let services = vec![
            svc("W7ABC-10 Winlink Post Office", "10.5.3.2", "http://10.5.3.2:8772/"),
            svc("Info Only Winlink Link", "10.5.4.4", ""), // link "" (was :0/) → dropped
            svc("Generic Web Camera", "10.5.5.5", "http://10.5.5.5/"), // not a PO → dropped
            svc("KD7ZWV Winlink Gateway - ", "10.5.7.9", "winlink://10.5.7.9:8772"),
            svc("No IP Winlink", "", "http://x:8772/"), // no ip → dropped
        ];
        let out = classify(&services);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].name, "W7ABC-10 Winlink Post Office");
        assert_eq!(out[0].ip, "10.5.3.2");
        assert_eq!(out[0].port, 8772);
        assert_eq!(out[1].name, "KD7ZWV Winlink Gateway"); // cruft trimmed
        assert_eq!(out[1].port, 8772);
        assert!(out.iter().all(|p| !p.reachable && p.rtt_ms.is_none()));
    }

    #[test]
    fn deserialize_real_sysinfo_envelope_shape() {
        // Shape per aredn/aredn files/app/main/sysinfo.ut: top-level object with
        // api_version/node/... plus services[]. We must ignore the extra keys.
        let json = r#"{
            "api_version": "2.0",
            "node": "W7ABC-10",
            "services": [
                {"name": "W7ABC-10 Winlink Post Office", "ip": "10.5.3.2", "link": "http://10.5.3.2:8772/", "protocol": "tcp"},
                {"name": "Web", "ip": "10.5.3.2", "link": "", "protocol": ""}
            ]
        }"#;
        let resp: SysinfoResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.services.len(), 2);
        let pos = classify(&resp.services);
        assert_eq!(pos.len(), 1);
        assert_eq!(pos[0].ip, "10.5.3.2");
    }

    #[test]
    fn deserialize_missing_services_yields_empty() {
        let json = r#"{"api_version":"2.0","node":"X"}"#;
        let resp: SysinfoResponse = serde_json::from_str(json).unwrap();
        assert!(resp.services.is_empty());
        assert!(classify(&resp.services).is_empty());
    }

    #[test]
    fn rank_orders_reachable_first_then_rtt_then_name() {
        let mut list = vec![
            MeshPostOffice { name: "C".into(), ip: "10.0.0.3".into(), port: 8772, link: String::new(), reachable: false, rtt_ms: None },
            MeshPostOffice { name: "A".into(), ip: "10.0.0.1".into(), port: 8772, link: String::new(), reachable: true, rtt_ms: Some(31) },
            MeshPostOffice { name: "B".into(), ip: "10.0.0.2".into(), port: 8772, link: String::new(), reachable: true, rtt_ms: Some(12) },
        ];
        rank(&mut list);
        assert_eq!(list[0].name, "B"); // reachable, lowest RTT
        assert_eq!(list[1].name, "A"); // reachable, higher RTT
        assert_eq!(list[2].name, "C"); // unreachable last
    }

    #[test]
    fn resolve_host_prefers_param_then_default() {
        assert_eq!(resolve_host(Some("node.local.mesh".into())), "node.local.mesh");
        assert_eq!(resolve_host(Some("  ".into())), DEFAULT_MASTER_NODE); // blank → default (or config)
        // None → config-or-default; in a clean test env config read is absent → default.
        // (We don't assert the None case strictly to avoid coupling to a config file.)
    }
}
