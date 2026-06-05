//! Network environment probe — DNS resolution + TCP-connect-only CMS
//! reachability + CMS health cache read (spec §9.3).
//!
//! RADIO-1: read-only. TCP-connect-and-immediate-drop only (SYN/RST).
//! NO banner read. NO protocol write. DNS resolution is not transmission.
//!
//! CmsHealthState is accessed via the re-export at crate::winlink::cms_health.
//! That path does not reference the session module, satisfying the RADIO-1
//! probe isolation contract (tests/probes_no_tx_apis.rs).

use crate::logging::env_probes::{safe_env_value, ProbeGate, ProbeSnapshot};
use chrono::Utc;
use serde_json::json;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

pub static GATE: ProbeGate = ProbeGate::new();

const CONNECT_TIMEOUT: Duration = Duration::from_millis(500);

/// Default CMS host for reachability probes.
const DEFAULT_CMS_HOST: &str = "cms-z.winlink.org";

pub fn run(trigger: &str) -> ProbeSnapshot {
    let timestamp = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    let cms_host = safe_env_value("TUXLINK_CMS_HOST")
        .unwrap_or_else(|| DEFAULT_CMS_HOST.to_string());

    // DNS resolution (sync via getaddrinfo — RADIO-1-safe, not a transmission)
    let dns_addrs: Vec<SocketAddr> = format!("{cms_host}:8772")
        .to_socket_addrs()
        .ok()
        .map(|iter| iter.collect())
        .unwrap_or_default();
    let dns_a_records_count = dns_addrs.len();

    // TCP connect-and-immediate-drop — SYN/RST only, no payload
    let port_8772_reachable = tcp_connect_only(&format!("{cms_host}:8772"));
    let port_8773_reachable = tcp_connect_only(&format!("{cms_host}:8773"));

    // CMS health cache — accessed via crate::winlink::cms_health re-export.
    // This path satisfies the RADIO-1 probe isolation constraint.
    let cms_health_snapshot = crate::winlink::cms_health::CMS_HEALTH.snapshot();

    let result = json!({
        "trigger": trigger,
        "cms_host": cms_host,
        "dns_a_records_count": dns_a_records_count,
        "port_8772_reachable": port_8772_reachable,
        "port_8773_reachable": port_8773_reachable,
        "cms_health_snapshot": cms_health_snapshot,
    });

    ProbeSnapshot {
        probe: "network".into(),
        timestamp,
        trigger: trigger.into(),
        result,
    }
}

/// TCP connect-and-immediate-drop. Returns true if the TCP handshake
/// completes. No bytes are written or read after the connection is
/// established — the stream is dropped immediately.
fn tcp_connect_only(addr: &str) -> bool {
    addr.to_socket_addrs()
        .ok()
        .and_then(|mut addrs| addrs.next())
        .map(|sa| {
            TcpStream::connect_timeout(&sa, CONNECT_TIMEOUT)
                .map(|stream| { drop(stream); true })
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_produces_non_empty_json() {
        let snap = run("test");
        assert_eq!(snap.probe, "network");
        let r = &snap.result;
        assert!(r.get("dns_a_records_count").is_some());
        assert!(r.get("port_8772_reachable").is_some());
        assert!(r.get("cms_health_snapshot").is_some());
    }
}
