//! Packet-transport listener gate — wires the shared listener-arms layer
//! ([`crate::winlink::listener`]) into the AX.25 answerer path.
//!
//! ## Where this fits in the listener call graph
//!
//! 1. `winlink_backend::native_packet_connect` enters the Listen branch and
//!    calls `winlink::ax25::answer(link, mycall, params)`. The answerer waits
//!    for an inbound SABM, replies UA, and returns the connected stream + the
//!    peer's AX.25 source address.
//! 2. Before handing that stream to `native_packet_exchange` (which runs the
//!    answerer-side B2F), the backend calls [`gate_inbound_peer`] from this
//!    module.
//! 3. On `Accept`, the gate is invisible — the existing B2F path runs.
//! 4. On any reject variant the gate appends the (anonymized) reject event to
//!    the JSONL forensics log and the caller drops the stream — the
//!    `Ax25Stream::drop` impl fires a DISC (AX.25 disconnect) because the link
//!    is established (it replied UA in step 1).
//!
//! ## DIVERGENCE from WLE (per
//! `docs/design/2026-06-03-multi-transport-listener-architecture.md` §5 +
//! `dev/scratch/winlink-re/findings/packet-p2p.md` §"Allowed-stations model")
//!
//! Winlink Express has NO listener-side allowlist for Packet, because at the
//! AX.25 link layer the TNC accepts any peer that completes the SABM exchange.
//! Tuxlink overlays an application-layer allowlist on the answerer side: a
//! fresh-installed tuxlink rejects every inbound Packet-P2P session until the
//! operator curates a callsign list (or flips `allow_all`).
//!
//! The station-password layer is intentionally absent for Packet (see
//! `packet-p2p.md` §"Auth": no in-band place to challenge before B2F). The
//! gate is therefore built with [`StationPassword::no_keyring`] so the
//! decision short-circuits the password branch via `is_set() == false`.
//!
//! ## Path resolution
//!
//! - Allowlist: `<config-dir>/listener/packet/allowed_stations.json`
//! - Forensics log: `<config-dir>/listener/listener_arms.jsonl` (shared
//!   cross-transport; the entries are tagged with `transport: "packet"`).
//!
//! `<config-dir>` resolves through the same `TUXLINK_CONFIG_DIR > XDG_CONFIG_HOME > ~/.config/tuxlink`
//! precedence chain as the rest of tuxlink (see [`crate::config::config_path`]).
//!
//! bd: tuxlink-inde

use std::path::PathBuf;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::winlink::ax25::frame::Address;
use crate::winlink::listener::{
    listener_decide_at, AllowedStations, ListenerArmsRecord, ListenerDecision, PeerId,
    StationPassword, TransportKind,
};

// ──────────────────────────────────────────────────────────────
// Path helpers
// ──────────────────────────────────────────────────────────────

/// Directory that holds tuxlink's per-transport listener state.
///
/// Layout (illustrative):
///
/// ```text
/// <config-dir>/listener/
///   listener_arms.jsonl              (cross-transport forensics)
///   packet/allowed_stations.json     (Packet allowlist — this module)
///   telnet/allowed_stations.json     (future — tuxlink-xehu)
///   ...
/// ```
///
/// Resolves to `<config-dir>/listener/` where `<config-dir>` is the parent of
/// [`crate::config::config_path`].
pub fn listener_state_dir() -> PathBuf {
    let cfg_path = crate::config::config_path();
    cfg_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
        .join("listener")
}

/// Absolute path to the Packet allowlist file
/// (`<config-dir>/listener/packet/allowed_stations.json`).
pub fn packet_allowed_stations_path() -> PathBuf {
    listener_state_dir()
        .join("packet")
        .join("allowed_stations.json")
}

/// Absolute path to the shared listener forensics log
/// (`<config-dir>/listener/listener_arms.jsonl`).
pub fn listener_forensics_log_path() -> PathBuf {
    listener_state_dir().join("listener_arms.jsonl")
}

// ──────────────────────────────────────────────────────────────
// Reject-event record (forensics)
// ──────────────────────────────────────────────────────────────

/// One inbound-reject event appended to the shared forensics log when the
/// gate denies a peer.
///
/// Append-only; entries record WHO was rejected and WHY so the operator can
/// audit "rejected inbound from N7XYZ at HH:MM:SS." The shared
/// `listener_arms.jsonl` log carries arm events; reject events live in the
/// same file with a `kind` discriminator so a single read recovers the full
/// activity trail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListenerRejectEvent {
    /// Discriminator: always `"reject"` so a single read of the forensics log
    /// can distinguish arm events (which use the bare `ListenerArmsRecord`
    /// shape — no `kind` field) from reject events (this shape).
    pub kind: String,
    /// Wall-clock time of the reject.
    pub rejected_at: SystemTime,
    /// Which transport rejected.
    pub transport: TransportKind,
    /// Reject reason (allowlist / expired / password). Kebab-case for parse
    /// stability across writers + readers.
    pub reason: String,
    /// The peer's callsign (canonical "CALL-SSID" form, or "CALL" when
    /// SSID==0). `None` only if the gate ran without a callsign fragment
    /// (Packet always has one, but the type is `Option` for future
    /// transport-portable use).
    pub peer_callsign: Option<String>,
}

impl ListenerRejectEvent {
    /// Build a fresh reject event with `rejected_at = SystemTime::now()`.
    pub fn new(transport: TransportKind, reason: &'static str, peer: &PeerId) -> Self {
        Self {
            kind: "reject".to_string(),
            rejected_at: SystemTime::now(),
            transport,
            reason: reason.to_string(),
            peer_callsign: peer.callsign().map(canonical_callsign),
        }
    }

    /// Append this event to the JSONL forensics log at `path`.
    pub fn append_to_log(&self, path: &std::path::Path) -> std::io::Result<()> {
        use std::io::Write;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut line = serde_json::to_vec(self).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
        })?;
        line.push(b'\n');
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        file.write_all(&line)
    }
}

/// Canonical "CALL-SSID" rendering (matches `AllowedStations::canonicalize_address`).
fn canonical_callsign(addr: &Address) -> String {
    let base = addr.call.trim().to_uppercase();
    if addr.ssid == 0 {
        base
    } else {
        format!("{}-{}", base, addr.ssid)
    }
}

/// Map a `ListenerDecision::Reject*` to its kebab-case reason for the
/// forensics log. Returns `None` for `Accept` (the gate logs only rejects).
pub fn reject_reason(decision: &ListenerDecision) -> Option<&'static str> {
    match decision {
        ListenerDecision::Accept => None,
        ListenerDecision::RejectAllowlist => Some("allowlist"),
        ListenerDecision::RejectPassword => Some("password"),
        ListenerDecision::RejectExpired => Some("expired"),
    }
}

// ──────────────────────────────────────────────────────────────
// Gate entry point
// ──────────────────────────────────────────────────────────────

/// Build the `PeerId` a Packet listener presents to the gate from the
/// peer-address fragment that `winlink::ax25::answer` returns.
pub fn peer_id_from_ax25(addr: Address) -> PeerId {
    PeerId::Callsign(addr)
}

/// Run the listener-arms gate for one inbound Packet peer.
///
/// Builds a `StationPassword::no_keyring()` (per packet-p2p.md §"Auth": no
/// in-band place to challenge; the password layer is intentionally absent
/// for Packet) and calls [`listener_decide_at`] against the supplied
/// allowlist + arms record.
///
/// Returns the decision verbatim so the caller can drop-on-reject (the
/// `Ax25Stream::drop` impl fires DISC since the link is established) +
/// surface the result via the session log.
pub fn gate_inbound_peer(
    peer: &PeerId,
    allowed: &AllowedStations,
    arms: &ListenerArmsRecord,
    now: SystemTime,
) -> ListenerDecision {
    let password = StationPassword::no_keyring();
    listener_decide_at(peer, None, allowed, &password, arms, now)
}

/// Wrapper around [`gate_inbound_peer`] that uses `SystemTime::now()`. The
/// `_at` variant is the testing entrypoint.
pub fn gate_inbound_peer_now(
    peer: &PeerId,
    allowed: &AllowedStations,
    arms: &ListenerArmsRecord,
) -> ListenerDecision {
    gate_inbound_peer(peer, allowed, arms, SystemTime::now())
}

// ──────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::winlink::listener::arms_record::DEFAULT_TTL;
    use std::time::Duration;

    fn addr(call: &str, ssid: u8) -> Address {
        Address { call: call.into(), ssid }
    }

    fn allowed_with(call: &str, ssid: u8) -> AllowedStations {
        // Restrict-mode so the allowlist gates on the callsign list.
        // (Foundation default since tuxlink-7vea is allow_all=TRUE; tests
        // exercising the allowlist gate must opt back into restrict-mode.)
        let mut a = AllowedStations::new().with_allow_all(false);
        a.add_callsign(addr(call, ssid));
        a
    }

    fn fresh_arms() -> ListenerArmsRecord {
        let mut r = ListenerArmsRecord::arm(TransportKind::Packet, DEFAULT_TTL);
        // Pin armed_at so test runs are deterministic.
        r.armed_at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        r
    }

    fn within_window(arms: &ListenerArmsRecord) -> SystemTime {
        arms.armed_at + Duration::from_secs(30 * 60)
    }

    fn past_window(arms: &ListenerArmsRecord) -> SystemTime {
        arms.armed_at + Duration::from_secs(2 * 60 * 60)
    }

    // ── Allowlist gate ───────────────────────────────────────────

    #[test]
    fn allowlist_rejects_peer_not_on_list() {
        let peer = peer_id_from_ax25(addr("W4PHS", 0));
        let allowed = allowed_with("N7CPZ", 0);
        let arms = fresh_arms();
        let now = within_window(&arms);

        let decision = gate_inbound_peer(&peer, &allowed, &arms, now);
        assert_eq!(decision, ListenerDecision::RejectAllowlist);
    }

    #[test]
    fn allowlist_accepts_peer_on_list() {
        let peer = peer_id_from_ax25(addr("N7CPZ", 0));
        let allowed = allowed_with("N7CPZ", 0);
        let arms = fresh_arms();
        let now = within_window(&arms);

        let decision = gate_inbound_peer(&peer, &allowed, &arms, now);
        assert_eq!(decision, ListenerDecision::Accept);
    }

    #[test]
    fn allow_all_permits_any_peer() {
        // Foundation default since tuxlink-7vea is allow_all=TRUE (WLE-parity).
        // This test is a positive control: a fresh AllowedStations accepts.
        let peer = peer_id_from_ax25(addr("RANDOM", 7));
        let allowed = AllowedStations::new().with_allow_all(true);
        let arms = fresh_arms();
        let now = within_window(&arms);

        let decision = gate_inbound_peer(&peer, &allowed, &arms, now);
        assert_eq!(decision, ListenerDecision::Accept);
    }

    #[test]
    fn default_allowlist_accepts_known_callsigns_wle_parity() {
        // Tuxlink-7vea: AllowedStations::default() is allow_all=TRUE (WLE-parity).
        // Empty list + allow_all=TRUE accepts every peer — fresh tuxlink no
        // longer footguns operators by rejecting everyone until they curate.
        let peer = peer_id_from_ax25(addr("N7CPZ", 0));
        let allowed = AllowedStations::default();
        let arms = fresh_arms();
        let now = within_window(&arms);

        let decision = gate_inbound_peer(&peer, &allowed, &arms, now);
        assert_eq!(decision, ListenerDecision::Accept);
    }

    // ── Arms TTL gate ────────────────────────────────────────────

    #[test]
    fn arms_disarmed_rejects_even_listed_peer() {
        let peer = peer_id_from_ax25(addr("N7CPZ", 0));
        let allowed = allowed_with("N7CPZ", 0);
        let mut arms = fresh_arms();
        arms.disarm();
        let now = within_window(&arms);

        let decision = gate_inbound_peer(&peer, &allowed, &arms, now);
        assert_eq!(decision, ListenerDecision::RejectExpired);
    }

    #[test]
    fn arms_ttl_expired_rejects_listed_peer() {
        let peer = peer_id_from_ax25(addr("N7CPZ", 0));
        let allowed = allowed_with("N7CPZ", 0);
        let arms = fresh_arms();
        let now = past_window(&arms);

        let decision = gate_inbound_peer(&peer, &allowed, &arms, now);
        assert_eq!(decision, ListenerDecision::RejectExpired);
    }

    // ── No password layer for Packet ─────────────────────────────

    #[test]
    fn packet_gate_skips_password_layer() {
        // The whole point of no_keyring() for Packet: a peer who passes the
        // allowlist + TTL gates is accepted without any password input.
        // This is the inverse of Telnet (which prompts at the transport
        // layer); Packet has no in-band place to ask.
        let peer = peer_id_from_ax25(addr("N7CPZ", 0));
        let allowed = allowed_with("N7CPZ", 0);
        let arms = fresh_arms();
        let now = within_window(&arms);

        let decision = gate_inbound_peer(&peer, &allowed, &arms, now);
        assert_eq!(decision, ListenerDecision::Accept);
    }

    // ── SSID matching ────────────────────────────────────────────

    #[test]
    fn ssid_exact_match_required_when_pattern_has_ssid() {
        // AllowedStations stores N7CPZ-7 as exact; the inbound N7CPZ-0
        // (different SSID) does NOT match.
        let peer = peer_id_from_ax25(addr("N7CPZ", 0));
        let allowed = allowed_with("N7CPZ", 7);
        let arms = fresh_arms();
        let now = within_window(&arms);

        let decision = gate_inbound_peer(&peer, &allowed, &arms, now);
        assert_eq!(decision, ListenerDecision::RejectAllowlist);
    }

    // ── reject_reason mapping ────────────────────────────────────

    #[test]
    fn reject_reason_maps_each_decision_variant() {
        assert_eq!(reject_reason(&ListenerDecision::Accept), None);
        assert_eq!(
            reject_reason(&ListenerDecision::RejectAllowlist),
            Some("allowlist"),
        );
        assert_eq!(
            reject_reason(&ListenerDecision::RejectPassword),
            Some("password"),
        );
        assert_eq!(
            reject_reason(&ListenerDecision::RejectExpired),
            Some("expired"),
        );
    }

    // ── Forensics-log round trip ─────────────────────────────────

    #[test]
    fn reject_event_appends_to_forensics_log() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("subdir").join("listener_arms.jsonl");

        let peer = peer_id_from_ax25(addr("W4PHS", 3));
        let ev =
            ListenerRejectEvent::new(TransportKind::Packet, "allowlist", &peer);
        ev.append_to_log(&path).expect("append");

        let read = std::fs::read_to_string(&path).expect("read");
        // One JSON line ending in newline.
        assert_eq!(read.lines().count(), 1);
        let parsed: ListenerRejectEvent =
            serde_json::from_str(read.lines().next().unwrap()).expect("parse");
        assert_eq!(parsed.kind, "reject");
        assert_eq!(parsed.transport, TransportKind::Packet);
        assert_eq!(parsed.reason, "allowlist");
        assert_eq!(parsed.peer_callsign.as_deref(), Some("W4PHS-3"));
    }

    // ── Path resolution ──────────────────────────────────────────

    #[test]
    fn paths_resolve_under_config_dir() {
        // We don't assert the exact absolute path (it depends on env), only
        // the structural relationship: allowlist path lives at
        // <state-dir>/packet/allowed_stations.json and the forensics log at
        // <state-dir>/listener_arms.jsonl.
        let state = listener_state_dir();
        let allow = packet_allowed_stations_path();
        let log = listener_forensics_log_path();

        assert!(allow.starts_with(&state));
        assert!(log.starts_with(&state));
        assert_eq!(
            allow.file_name().and_then(|s| s.to_str()),
            Some("allowed_stations.json"),
        );
        assert_eq!(
            log.file_name().and_then(|s| s.to_str()),
            Some("listener_arms.jsonl"),
        );
        // Sanity: state dir is named "listener".
        assert_eq!(
            state.file_name().and_then(|s| s.to_str()),
            Some("listener"),
        );
    }
}
