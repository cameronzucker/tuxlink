//! `AllowedStations` — operator-curated allowlist of callsigns and IPs for
//! inbound P2P listeners.
//!
//! Storage: a plain config file (callsigns + IPs are not secrets). JSON via
//! `serde_json` matching the existing config-file pattern at
//! `src-tauri/src/config.rs`.
//!
//! ## Defaults (DIVERGENCE FROM WLE)
//!
//! Winlink Express defaults `Allow All Connections: TRUE` — every inbound
//! callsign is accepted unless explicitly blocklisted. Tuxlink inverts that:
//! `Allow All Connections: FALSE` is the default, and the allowlist is
//! empty, so a freshly-armed listener with no editing rejects every peer.
//! This matches the operator-feedback class encoded in the user-memory
//! entry `no-disk-creds-default` (defensive posture by default; operator
//! explicitly opts into permissiveness).
//!
//! ## Wildcard matching
//!
//! Per `dev/scratch/winlink-re/findings/telnet-p2p.md §allowed-stations`,
//! WLE supports `*` as a tail wildcard:
//!
//! - Callsign: `N7*` matches `N7CPZ`, `N7CPZ-1`, `N7ABC-15`, etc.
//! - IP: `192.168.*` matches `192.168.1.5`, `192.168.0.0/16`-class entries.
//!
//! The tuxlink semantics mirror WLE: the `*` is a tail wildcard only
//! (no glob, no regex). Callsign comparisons are case-insensitive and
//! ignore SSIDs when the pattern omits one (so `N7*` matches every SSID
//! variant; `N7CPZ-1` matches that specific SSID only).
//!
//! Spec: `docs/design/2026-06-03-multi-transport-listener-architecture.md` §2.1
//! bd: tuxlink-3o2o

use std::net::IpAddr;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::winlink::ax25::frame::Address;

use super::peer::PeerId;

// ──────────────────────────────────────────────────────────────
// Errors
// ──────────────────────────────────────────────────────────────

/// Error returned by `AllowedStations::load_from` / `save_to`.
#[derive(Debug, thiserror::Error)]
pub enum AllowedStationsError {
    #[error("io error at {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("serde error at {path}: {source}")]
    Serde {
        path: std::path::PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

// ──────────────────────────────────────────────────────────────
// Data
// ──────────────────────────────────────────────────────────────

/// The persisted allowlist + master toggle.
///
/// JSON shape (illustrative):
///
/// ```json
/// {
///   "allow_all": false,
///   "callsigns": ["N7CPZ", "W4PHS-*"],
///   "ips": ["192.168.1.5", "192.168.*"]
/// }
/// ```
///
/// `callsigns` entries are stored upper-case sans whitespace. IP entries are
/// stored as the operator typed them so wildcards survive a round trip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllowedStations {
    #[serde(default)]
    allow_all: bool,
    #[serde(default)]
    callsigns: Vec<String>,
    #[serde(default)]
    ips: Vec<String>,
}

impl Default for AllowedStations {
    /// `Allow All Connections: FALSE`, empty allowlist — DIVERGES from WLE.
    fn default() -> Self {
        Self {
            allow_all: false,
            callsigns: Vec::new(),
            ips: Vec::new(),
        }
    }
}

impl AllowedStations {
    /// Construct an empty allowlist with the master toggle off.
    pub fn new() -> Self {
        Self::default()
    }

    /// Toggle the master "allow all" flag.
    ///
    /// When TRUE, `accept` returns true for any peer (matching WLE's permissive
    /// default). When FALSE (the tuxlink default), the operator-curated
    /// callsign + IP lists are consulted.
    pub fn with_allow_all(mut self, allow: bool) -> Self {
        self.allow_all = allow;
        self
    }

    /// Returns the master "allow all" flag.
    pub fn allow_all(&self) -> bool {
        self.allow_all
    }

    /// Set the master flag in place.
    pub fn set_allow_all(&mut self, allow: bool) {
        self.allow_all = allow;
    }

    /// Add an AX.25 callsign (exact match, including SSID) to the allowlist.
    ///
    /// Duplicates are deduplicated.
    pub fn add_callsign(&mut self, addr: Address) {
        let stored = canonicalize_address(&addr);
        if !self.callsigns.iter().any(|c| c == &stored) {
            self.callsigns.push(stored);
        }
    }

    /// Add a callsign pattern (which may be a wildcard like `N7*` or
    /// `W4PHS-*`) to the allowlist. Patterns are stored upper-cased.
    pub fn add_callsign_pattern<S: Into<String>>(&mut self, pattern: S) {
        let stored = pattern.into().trim().to_uppercase();
        if !stored.is_empty() && !self.callsigns.iter().any(|c| c == &stored) {
            self.callsigns.push(stored);
        }
    }

    /// Add an IP (exact match) to the allowlist.
    pub fn add_ip(&mut self, ip: IpAddr) {
        let stored = ip.to_string();
        if !self.ips.iter().any(|i| i == &stored) {
            self.ips.push(stored);
        }
    }

    /// Add an IP pattern (which may include a `*` tail wildcard like
    /// `192.168.*`) to the allowlist.
    pub fn add_ip_pattern<S: Into<String>>(&mut self, pattern: S) {
        let stored = pattern.into().trim().to_string();
        if !stored.is_empty() && !self.ips.iter().any(|i| i == &stored) {
            self.ips.push(stored);
        }
    }

    /// Clear all entries and reset `allow_all` to FALSE.
    pub fn clear(&mut self) {
        self.allow_all = false;
        self.callsigns.clear();
        self.ips.clear();
    }

    /// Borrow the callsign patterns (read-only).
    pub fn callsigns(&self) -> &[String] {
        &self.callsigns
    }

    /// Borrow the IP patterns (read-only).
    pub fn ips(&self) -> &[String] {
        &self.ips
    }

    /// Returns TRUE if the peer is accepted by this allowlist.
    ///
    /// Semantics:
    /// 1. If `allow_all` is set, accept unconditionally (WLE-compatible
    ///    permissive mode).
    /// 2. Otherwise, accept if any callsign fragment of `peer` matches a
    ///    callsign pattern OR any socket-addr fragment of `peer` matches
    ///    an IP pattern.
    pub fn accept(&self, peer: &PeerId) -> bool {
        if self.allow_all {
            return true;
        }
        if let Some(call) = peer.callsign() {
            if self.callsigns.iter().any(|p| callsign_matches(p, call)) {
                return true;
            }
        }
        if let Some(addr) = peer.socket_addr() {
            if self.ips.iter().any(|p| ip_matches(p, &addr.ip())) {
                return true;
            }
        }
        false
    }

    /// Load from a JSON file.
    ///
    /// If the file is missing, return [`AllowedStations::default`] (NOT an
    /// error). This matches first-run semantics across the rest of tuxlink:
    /// an absent allowlist file means "you haven't configured one yet,"
    /// and the defensive default takes effect.
    pub fn load_from(path: &Path) -> Result<Self, AllowedStationsError> {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => {
                return Err(AllowedStationsError::Io {
                    path: path.to_path_buf(),
                    source,
                });
            }
        };
        serde_json::from_slice(&bytes).map_err(|source| AllowedStationsError::Serde {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Save to a JSON file. Creates the parent directory if missing; writes
    /// atomically via a sibling `.tmp` file + rename so a crash mid-write
    /// can't corrupt the on-disk allowlist.
    pub fn save_to(&self, path: &Path) -> Result<(), AllowedStationsError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| AllowedStationsError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let json = serde_json::to_vec_pretty(self).map_err(|source| AllowedStationsError::Serde {
            path: path.to_path_buf(),
            source,
        })?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &json).map_err(|source| AllowedStationsError::Io {
            path: tmp.clone(),
            source,
        })?;
        std::fs::rename(&tmp, path).map_err(|source| AllowedStationsError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(())
    }
}

// ──────────────────────────────────────────────────────────────
// Pattern matching
// ──────────────────────────────────────────────────────────────

/// Canonical "CALL-SSID" form (e.g. "N7CPZ-1" or "N7CPZ" if ssid==0).
/// Uppercased, no whitespace.
fn canonicalize_address(addr: &Address) -> String {
    let base = addr.call.trim().to_uppercase();
    if addr.ssid == 0 {
        base
    } else {
        format!("{}-{}", base, addr.ssid)
    }
}

/// Test whether a pattern (with optional `*` tail wildcard) matches a callsign.
///
/// The pattern is case-insensitive.
///
/// If the pattern omits SSID (no `-` in the pattern), only the base callsign
/// portion of the address is compared (so `N7*` matches `N7CPZ-1` and
/// `N7CPZ` alike). If the pattern includes SSID, the full canonical form
/// is matched (so `N7CPZ-1` does NOT match `N7CPZ-2`).
fn callsign_matches(pattern: &str, addr: &Address) -> bool {
    let pat_norm = pattern.trim().to_uppercase();
    let canonical = canonicalize_address(addr); // e.g. "N7CPZ" or "N7CPZ-1"

    if pat_norm.contains('-') {
        // SSID-aware match
        prefix_or_exact(&pat_norm, &canonical)
    } else {
        // SSID-agnostic match: compare against the base callsign only
        let base = addr.call.trim().to_uppercase();
        prefix_or_exact(&pat_norm, &base)
    }
}

/// Tail-wildcard match. If `pattern` ends in `*`, treat the prefix as a
/// case-insensitive prefix match against `value`; otherwise compare equal.
/// Comparison is byte-wise on already-uppercased inputs.
fn prefix_or_exact(pattern: &str, value: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('*') {
        value.starts_with(prefix)
    } else {
        pattern == value
    }
}

/// Test whether a pattern (with optional `*` tail wildcard) matches an IP.
///
/// The IP is rendered to its standard string form (`Ipv4: A.B.C.D`,
/// `Ipv6: a:b:c::d`) and tail-matched against the pattern.
fn ip_matches(pattern: &str, ip: &IpAddr) -> bool {
    let rendered = ip.to_string();
    if let Some(prefix) = pattern.strip_suffix('*') {
        rendered.starts_with(prefix)
    } else {
        pattern == rendered
    }
}

// ──────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddr};

    fn addr(call: &str, ssid: u8) -> Address {
        Address {
            call: call.into(),
            ssid,
        }
    }

    fn peer_call(call: &str, ssid: u8) -> PeerId {
        PeerId::Callsign(addr(call, ssid))
    }

    fn peer_addr(ip: [u8; 4], port: u16) -> PeerId {
        PeerId::SocketAddr(SocketAddr::new(
            std::net::IpAddr::V4(Ipv4Addr::new(ip[0], ip[1], ip[2], ip[3])),
            port,
        ))
    }

    // ── Default state ────────────────────────────────────────────

    #[test]
    fn default_rejects_every_peer() {
        let allowed = AllowedStations::default();
        assert!(!allowed.allow_all());
        assert!(allowed.callsigns().is_empty());
        assert!(allowed.ips().is_empty());

        assert!(!allowed.accept(&peer_call("N7CPZ", 0)));
        assert!(!allowed.accept(&peer_addr([192, 168, 1, 5], 8774)));
    }

    // ── add_callsign / add_ip ────────────────────────────────────

    #[test]
    fn add_callsign_accepts_listed_rejects_others() {
        let mut allowed = AllowedStations::new();
        allowed.add_callsign(addr("N7CPZ", 0));

        assert!(allowed.accept(&peer_call("N7CPZ", 0)));
        assert!(!allowed.accept(&peer_call("W4PHS", 0)));
    }

    #[test]
    fn add_callsign_is_idempotent() {
        let mut allowed = AllowedStations::new();
        allowed.add_callsign(addr("N7CPZ", 0));
        allowed.add_callsign(addr("N7CPZ", 0));
        assert_eq!(allowed.callsigns().len(), 1);
    }

    #[test]
    fn add_callsign_is_case_insensitive_in_storage() {
        let mut allowed = AllowedStations::new();
        allowed.add_callsign(addr("n7cpz", 0));
        assert_eq!(allowed.callsigns(), &["N7CPZ".to_string()]);
        assert!(allowed.accept(&peer_call("N7CPZ", 0)));
    }

    #[test]
    fn add_ip_accepts_listed_rejects_others() {
        let mut allowed = AllowedStations::new();
        allowed.add_ip(std::net::IpAddr::V4(Ipv4Addr::new(192, 168, 1, 5)));

        assert!(allowed.accept(&peer_addr([192, 168, 1, 5], 8774)));
        assert!(!allowed.accept(&peer_addr([10, 0, 0, 1], 8774)));
    }

    // ── allow_all toggle ─────────────────────────────────────────

    #[test]
    fn with_allow_all_true_accepts_everyone() {
        let allowed = AllowedStations::new().with_allow_all(true);
        assert!(allowed.accept(&peer_call("N7CPZ", 0)));
        assert!(allowed.accept(&peer_call("W4PHS", 7)));
        assert!(allowed.accept(&peer_addr([10, 0, 0, 1], 8774)));
    }

    #[test]
    fn allow_all_false_with_empty_list_rejects() {
        let allowed = AllowedStations::new();
        assert!(!allowed.accept(&peer_call("N7CPZ", 0)));
    }

    // ── Wildcard callsign matching ───────────────────────────────

    #[test]
    fn wildcard_callsign_matches_prefix_no_ssid_in_pattern() {
        let mut allowed = AllowedStations::new();
        allowed.add_callsign_pattern("N7*");
        assert!(allowed.accept(&peer_call("N7CPZ", 0)));
        assert!(allowed.accept(&peer_call("N7CPZ", 1)));
        assert!(allowed.accept(&peer_call("N7ABC", 15)));
        assert!(!allowed.accept(&peer_call("W4PHS", 0)));
    }

    #[test]
    fn wildcard_callsign_with_ssid_in_pattern_constrains_ssid() {
        let mut allowed = AllowedStations::new();
        allowed.add_callsign_pattern("N7CPZ-1");
        assert!(allowed.accept(&peer_call("N7CPZ", 1)));
        assert!(!allowed.accept(&peer_call("N7CPZ", 2)));
    }

    #[test]
    fn wildcard_callsign_lowercase_pattern_matches_uppercase_peer() {
        let mut allowed = AllowedStations::new();
        allowed.add_callsign_pattern("n7*");
        assert!(allowed.accept(&peer_call("N7CPZ", 0)));
    }

    // ── Wildcard IP matching ─────────────────────────────────────

    #[test]
    fn wildcard_ip_matches_prefix() {
        let mut allowed = AllowedStations::new();
        allowed.add_ip_pattern("192.168.*");
        assert!(allowed.accept(&peer_addr([192, 168, 1, 5], 8774)));
        assert!(allowed.accept(&peer_addr([192, 168, 0, 0], 8774)));
        assert!(!allowed.accept(&peer_addr([10, 0, 0, 1], 8774)));
    }

    #[test]
    fn exact_ip_does_not_match_neighbours() {
        let mut allowed = AllowedStations::new();
        allowed.add_ip_pattern("192.168.1.5");
        assert!(allowed.accept(&peer_addr([192, 168, 1, 5], 8774)));
        assert!(!allowed.accept(&peer_addr([192, 168, 1, 6], 8774)));
    }

    // ── clear ─────────────────────────────────────────────────────

    #[test]
    fn clear_resets_to_default() {
        let mut allowed = AllowedStations::new().with_allow_all(true);
        allowed.add_callsign(addr("N7CPZ", 0));
        allowed.add_ip(std::net::IpAddr::V4(Ipv4Addr::new(192, 168, 1, 5)));
        allowed.clear();

        assert!(!allowed.allow_all());
        assert!(allowed.callsigns().is_empty());
        assert!(allowed.ips().is_empty());
        assert!(!allowed.accept(&peer_call("N7CPZ", 0)));
    }

    // ── JSON round-trip ──────────────────────────────────────────

    #[test]
    fn json_round_trip_equals() {
        let mut original = AllowedStations::new().with_allow_all(false);
        original.add_callsign(addr("N7CPZ", 0));
        original.add_callsign_pattern("W4*");
        original.add_ip(std::net::IpAddr::V4(Ipv4Addr::new(192, 168, 1, 5)));
        original.add_ip_pattern("10.0.*");

        let json = serde_json::to_string(&original).expect("serialize");
        let back: AllowedStations = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(original, back);
    }

    // ── load_from missing file → default ─────────────────────────

    #[test]
    fn load_from_missing_path_returns_default() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nope.json");
        let loaded = AllowedStations::load_from(&path).expect("missing → default");
        assert_eq!(loaded, AllowedStations::default());
    }

    #[test]
    fn save_then_load_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("subdir").join("allowed_stations.json");

        let mut original = AllowedStations::new();
        original.add_callsign(addr("N7CPZ", 0));
        original.add_ip_pattern("192.168.*");
        original.save_to(&path).expect("save");

        let back = AllowedStations::load_from(&path).expect("load");
        assert_eq!(original, back);
    }

    // ── Both variant of PeerId ───────────────────────────────────

    #[test]
    fn both_variant_callsign_match_accepts() {
        let mut allowed = AllowedStations::new();
        allowed.add_callsign(addr("N7CPZ", 0));

        let peer = PeerId::Both {
            callsign: addr("N7CPZ", 0),
            addr: SocketAddr::new(
                std::net::IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
                8774,
            ),
        };
        assert!(allowed.accept(&peer));
    }

    #[test]
    fn both_variant_ip_match_accepts_even_if_callsign_missing() {
        let mut allowed = AllowedStations::new();
        allowed.add_ip_pattern("192.168.*");

        let peer = PeerId::Both {
            callsign: addr("UNKNOWN", 0),
            addr: SocketAddr::new(
                std::net::IpAddr::V4(Ipv4Addr::new(192, 168, 0, 1)),
                8774,
            ),
        };
        assert!(allowed.accept(&peer));
    }
}
