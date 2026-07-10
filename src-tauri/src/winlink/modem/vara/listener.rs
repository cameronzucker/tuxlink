//! VARA P2P inbound-listener wiring (bd: tuxlink-9ls2).
//!
//! Consumes the shared listener-arms foundation (`crate::winlink::listener`)
//! and exposes the same surface as the ARDOP listener:
//!
//! - [`parse_peer_call`] — translate the verbatim `target` field from VARA's
//!   `CONNECTED <mycall> <target> [bw]` async event into an
//!   [`crate::winlink::ax25::frame::Address`]. VARA's CONNECTED arg is the
//!   bare peer callsign (no SSID syntax in the host protocol), but we
//!   defensively handle a trailing `-N` SSID — operator might paste a call
//!   from another tool, and the same parser used for ARDOP is shape-compatible
//!   with the foundation's allowlist comparisons.
//! - [`decide_for_vara_event`] — runs the allowlist + arms-TTL gate against
//!   a parsed peer, returning the [`ListenerDecision`] the caller acts on.
//!   Like ARDOP, VARA has no station-password layer (clean-sheet decision:
//!   plaintext shared secrets over RF are worse than no secret; allowlist +
//!   TCP-layer access are the actual gates).
//! - [`allowed_stations_path`] — config-dir-relative location for the VARA
//!   allowlist JSON, namespaced under `listener/vara/` so it lands beside
//!   the Telnet / Packet / ARDOP allowlists without colliding.
//! - [`set_listen`] — runtime toggle that sends `LISTEN ON` / `LISTEN OFF`
//!   over the cmd socket. Best-effort: VARA's host parser maps the echoed
//!   setter back as `InboundCommand::Unknown("LISTEN ON")` (no first-class
//!   Listen-echo variant in the inbound enum), so we send + drain pending
//!   events for a short window rather than blocking on a specific echo
//!   shape. Same posture as `commands::vara_open_session_inner` for the
//!   MYCALL/BW setters.
//! - [`serve_inbound_one`] — single-shot wait for an inbound `CONNECTED`
//!   event, run the gate, and either return [`InboundOutcome::Accepted`]
//!   (caller proceeds to B2F answer over `transport.data_stream()`) or
//!   send `DISCONNECT` and surface the reject class.
//!
//! ## Wire divergence from ARDOP
//!
//! - LISTEN setter: VARA uses `LISTEN ON` / `LISTEN OFF`; ARDOP uses
//!   `LISTEN TRUE` / `LISTEN FALSE`. See [`OutboundCommand::Listen`].
//! - CONNECTED shape: VARA emits `CONNECTED <mycall> <target> [bw]` —
//!   the PEER is the `target` field, not the first token like ARDOP's
//!   `CONNECTED <call> <bw>`. The transport-layer parser at
//!   `super::command::InboundCommand::parse` already extracts `target`,
//!   so the listener just consumes `InboundCommand::Connected { target, .. }`.
//! - No EchoBack ack: VARA doesn't have ARDOP's explicit EchoBack
//!   message type — setter echoes arrive as the same-line text mapped to
//!   `InboundCommand::Unknown`. The listener doesn't attempt to match
//!   them; the operator-visible state is the transport remaining Open.
//! - Two TCP sockets: VARA's cmd socket carries the LISTEN / CONNECTED /
//!   DISCONNECT signaling; the data socket carries raw payload bytes for
//!   B2F. The listener only touches the cmd socket; B2F answer touches
//!   both via `transport.data_stream()`.
//!
//! Spec: `docs/design/2026-06-03-multi-transport-listener-architecture.md` §4
//! Pattern source: `super::super::ardop::listener` (tuxlink-dhbl /
//! tuxlink-61yg shipped behavior).

use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::winlink::ax25::frame::Address;
use crate::winlink::credentials::EntryLike;
use crate::winlink::listener::{
    listener_decide, AllowedStations, ListenerArmsRecord, ListenerDecision, PeerId,
    StationPassword,
};

use super::command::{InboundCommand, OutboundCommand};
use super::transport::{RecvOutcome, VaraTransport};

// ──────────────────────────────────────────────────────────────
// Peer call parsing
// ──────────────────────────────────────────────────────────────

/// Parse the verbatim peer-call string from VARA's
/// `CONNECTED <mycall> <target> [bw]` event into an [`Address`].
///
/// VARA's host protocol does not carry an SSID — the peer call is the
/// bare callsign — but we defensively handle a trailing `-N` SSID anyway
/// because an operator paste from another tool (Pat, Winlink Express,
/// ARDOP) might include one and the foundation's allowlist compares
/// the parsed Address directly. Mirrors `ardop::listener::parse_peer_call`:
///
/// - `"W4PHS"` → `Address { call: "W4PHS", ssid: 0 }`.
/// - `"W4PHS-7"` → `Address { call: "W4PHS", ssid: 7 }`.
/// - `"W4PHS-0"` → `Address { call: "W4PHS", ssid: 0 }` (strip the
///   redundant trailing `-0`; allowlist comparisons against `"W4PHS"`
///   match cleanly).
/// - Out-of-range SSID (≥ 16) → defensively fall back to SSID 0 with
///   the original token preserved as the call.
///
/// Inputs are trimmed and upper-cased on the call side. Same posture as
/// `winlink_backend::parse_call_ssid`'s strict variant minus the error
/// propagation — we are downstream of an already-on-air event; tolerate
/// and route to the gate's reject path rather than panicking.
///
/// NOTE (spec §4 write boundary): this parser deliberately applies NO
/// charset filter — accept policy belongs to `AllowedStations`, and WLE
/// parity means a malformed claimed callsign may still get a session.
/// The PEER ROSTER is protected downstream: `PeersStore::apply_observation`
/// drops any presented callsign failing `callsign::validate_presented_callsign`
/// [R2-S2], and keyring accounts are id-keyed, never callsign-keyed
/// [R2-S10]. Render surfaces escape everything (frontend hostile-callsign
/// tests).
pub fn parse_peer_call(s: &str) -> Address {
    let trimmed = s.trim();
    if let Some((call, ssid_str)) = trimmed.rsplit_once('-') {
        if let Ok(n) = ssid_str.parse::<u8>() {
            if n <= 15 {
                return Address {
                    call: call.to_uppercase(),
                    ssid: n,
                };
            }
        }
    }
    Address {
        call: trimmed.to_uppercase(),
        ssid: 0,
    }
}

// ──────────────────────────────────────────────────────────────
// No-keyring StationPassword adapter
// ──────────────────────────────────────────────────────────────

/// Construct a [`StationPassword`] that always reports "no password
/// configured" — used by transports that don't have a station-password
/// layer (VARA, like ARDOP, per the same clean-sheet rationale: plaintext
/// shared secrets over RF are worse than no secret).
///
/// The shared `listener_decide` function takes a `StationPassword` by
/// reference. VARA must still call `listener_decide` to keep the
/// per-transport gate semantics identical; this adapter lets the
/// password branch short-circuit cleanly without VARA-specific code in
/// the foundation. `StationPassword::is_set()` returns FALSE because the
/// backing keyring entry always returns `NoEntry`.
///
/// Functionally identical to `super::super::ardop::listener::station_password_no_keyring`
/// and to the foundation's `StationPassword::no_keyring()` — duplicated
/// here as a local namespaced helper so the test surface mirrors ARDOP's
/// 1:1, and so a future change to one transport's posture (e.g. adding
/// a VARA-specific keyring layer) can be done without touching the
/// other.
pub fn station_password_no_keyring() -> StationPassword {
    use crate::winlink::listener::station_password::EntryFactory;
    let factory: EntryFactory = Box::new(|_service: &str, _account: &str| {
        Box::new(NoKeyringEntry) as Box<dyn EntryLike>
    });
    StationPassword::with_factory(factory)
}

/// Always-empty keyring entry — every operation behaves as if no value is
/// stored. Read/delete return `NoEntry`; writes are silently dropped (a
/// future change that sets a VARA password should add a real keyring
/// adapter rather than reusing this).
struct NoKeyringEntry;

impl EntryLike for NoKeyringEntry {
    fn get_password(&self) -> Result<String, keyring::Error> {
        Err(keyring::Error::NoEntry)
    }
    fn set_password(&self, _password: &str) -> Result<(), keyring::Error> {
        // No-op: VARA has no station-password layer.
        Ok(())
    }
    fn delete_password(&self) -> Result<(), keyring::Error> {
        Err(keyring::Error::NoEntry)
    }
}

// ──────────────────────────────────────────────────────────────
// Allowed-stations on-disk location
// ──────────────────────────────────────────────────────────────

/// Resolve the on-disk path for the VARA allowed-stations JSON file,
/// given the tuxlink config directory.
///
/// Layout: `<config-dir>/listener/vara/allowed_stations.json`. The
/// `listener/<transport>/` subtree namespaces VARA's allowlist alongside
/// Telnet (`listener/telnet/`), Packet (`listener/packet/`), and ARDOP
/// (`listener/ardop/`) so they share the same `AllowedStations` JSON
/// shape without colliding on a single flat file.
pub fn allowed_stations_path(config_dir: &std::path::Path) -> PathBuf {
    config_dir
        .join("listener")
        .join("vara")
        .join("allowed_stations.json")
}

// ──────────────────────────────────────────────────────────────
// Gate
// ──────────────────────────────────────────────────────────────

/// Run the shared listener-arms gate against a parsed peer.
///
/// Returns the [`ListenerDecision`] the caller acts on:
/// - `Accept` → hand the data stream to `run_exchange_with_role(Answer)`.
/// - `RejectAllowlist`, `RejectPassword`, `RejectExpired` → send
///   `DISCONNECT` over the cmd socket and append the reject to the
///   forensics log.
///
/// `password_input` is hard-wired to `None` because VARA has no
/// challenge-response layer — see module docs. The shared
/// `listener_decide` skips the password branch via `StationPassword::is_set()
/// == false`.
///
/// Pure over its inputs — no I/O. Production callers compose this with
/// the on-disk allowlist loaded once at arm time.
pub fn decide_for_vara_event(
    peer: &Address,
    allowed: &AllowedStations,
    arms: &ListenerArmsRecord,
) -> ListenerDecision {
    let peer_id = PeerId::Callsign(peer.clone());
    let password = station_password_no_keyring();
    listener_decide(&peer_id, None, allowed, &password, &arms.clone())
}

// ──────────────────────────────────────────────────────────────
// LISTEN state-machine
// ──────────────────────────────────────────────────────────────

/// How long to drain pending cmd-socket events after a setter is sent.
///
/// VARA's parser maps a setter echo (`LISTEN ON\r` reflected back) to
/// `InboundCommand::Unknown(...)` because the parser doesn't have a
/// first-class Listen-echo variant. Rather than block on a specific
/// echo, we drain whatever's in the socket for a short window so a
/// stale async event from a previous command doesn't pile up against
/// the next read. Bounded so a wedged VARA cannot stall the toggle.
const LISTEN_DRAIN_BUDGET: Duration = Duration::from_millis(500);

/// Toggle VARA's `LISTEN` flag at runtime.
///
/// Sends `LISTEN ON` / `LISTEN OFF` over the cmd socket and best-effort
/// drains any in-flight async events for ~500 ms. **Does NOT** block on
/// a specific echo because VARA doesn't expose an EchoBack message
/// shape comparable to ARDOP's — setter acknowledgement is implicit.
/// Matches the existing VARA setter pattern in
/// `commands::vara_open_session_inner` (which sends MYCALL/BW without
/// awaiting an echo).
///
/// Errors propagate from the underlying TCP write. Drain failures are
/// swallowed — they don't change whether the LISTEN setter reached the
/// modem.
pub fn set_listen(transport: &mut VaraTransport, enabled: bool) -> io::Result<()> {
    transport.send(&OutboundCommand::Listen(enabled))?;

    // Drain any in-flight async events for a bounded window. The transport's
    // recv() returns Ok(None) on read timeout (per VaraConfig.read_timeout)
    // OR on EOF; we treat both as "nothing more to drain" and stop early.
    let start = Instant::now();
    while start.elapsed() < LISTEN_DRAIN_BUDGET {
        match transport.recv() {
            Ok(Some(_)) => continue, // absorbed
            Ok(None) => break,       // timeout/EOF — done draining
            Err(_) => break,         // I/O error: surface on the next operation
        }
    }
    Ok(())
}

// ──────────────────────────────────────────────────────────────
// serve_inbound_one
// ──────────────────────────────────────────────────────────────

/// Outcome of one [`serve_inbound_one`] call.
///
/// `Accepted` carries the parsed peer [`Address`] and the verbatim
/// `peer_call` from the CONNECTED event (the consumer task hands this
/// to the B2F answerer as the `targetcall`). `RejectedAllowlist` /
/// `RejectedExpired` are terminal for this inbound — the listener loop
/// should record the reject (in the forensics log) and continue
/// waiting for the next CONNECTED event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InboundOutcome {
    /// Peer passed the gate — the caller proceeds to B2F answer over
    /// `transport.data_stream()`.
    Accepted {
        /// Parsed peer Address (uppercased call, SSID if present).
        peer: Address,
        /// Verbatim peer-call string from the CONNECTED event — what
        /// the B2F handshake uses as `targetcall`. Preserved separately
        /// from `peer` because the on-air callsign string may carry
        /// formatting the foundation Address normalizes away.
        peer_call: String,
    },
    /// Peer rejected on the allowlist.
    RejectedAllowlist { peer: Address },
    /// The arms TTL has elapsed (or the operator disarmed).
    RejectedExpired { peer: Address },
}

/// Why a [`serve_inbound_one`] call failed.
#[derive(Debug, thiserror::Error)]
pub enum VaraListenerError {
    /// I/O error on the cmd socket (write failed, recv returned an error,
    /// etc.).
    #[error("vara listener I/O: {0}")]
    Io(#[from] io::Error),
    /// No inbound CONNECTED arrived within the supplied deadline.
    #[error("vara listener timeout waiting for inbound CONNECTED")]
    Timeout,
    /// Rejected a peer at the gate but the DISCONNECT couldn't be sent —
    /// the modem may still be connected to the rejected peer. Caller
    /// MUST reset the transport. Mirrors the Codex 2026-06-03 P2 fix in
    /// `ardop/listener.rs` (promoting DISCONNECT failures from a
    /// swallowed `let _ =` to an explicit error so the consumer task
    /// isn't left in a half-rejected state).
    #[error("vara listener: reject DISCONNECT failed: {0}")]
    DisconnectFailed(String),
    /// The peer disconnected mid-listen (DISCONNECTED arrived before
    /// CONNECTED). Surface as an error so the caller can decide to
    /// retry or disarm — same posture as ARDOP's RemoteDisconnect path.
    #[error("vara listener: remote disconnect before CONNECTED")]
    RemoteDisconnect,
    /// The cmd-socket TCP connection reached EOF (peer sent FIN) while
    /// waiting for an inbound CONNECTED (Codex P1 #3 — tuxlink-6urh2 v2).
    /// Distinct from [`Self::RemoteDisconnect`]: a `DISCONNECTED` command
    /// is a normal VARA protocol event for a SINGLE peer (the listener
    /// keeps running); an EOF means the cmd socket itself is gone (the
    /// VARA process died, or the TCP link dropped) — the consumer task
    /// cannot keep serving inbound events over a closed socket, so this is
    /// terminal. Prior to this fix, `serve_inbound_one` folded EOF into
    /// the same `Ok(None)` bucket as an ordinary read timeout, so the
    /// consumer's poll loop treated a dead cmd socket as just another
    /// timeout tick and spun on it forever instead of noticing the
    /// transport was gone.
    #[error("vara listener: cmd-socket EOF — transport is gone")]
    TransportClosed,
}

/// Wait for ONE inbound `CONNECTED <mycall> <target> [bw]` event, run
/// the gate, and either accept (caller proceeds to B2F answer) or send
/// `DISCONNECT` and surface the reject class.
///
/// `deadline` is the overall budget: relative time from "now" to give
/// up waiting for a CONNECTED event with [`VaraListenerError::Timeout`]
/// (caller's loop re-issues if the arms window still has room).
///
/// Async-event interleaving: PTT / BUFFER / PENDING / CANCELPENDING /
/// LINK REGISTERED / IAMALIVE / Unknown are absorbed and the wait
/// continues. A `DISCONNECTED` arriving without a prior `CONNECTED` is
/// surfaced as [`VaraListenerError::RemoteDisconnect`].
///
/// On a Reject decision, the function sends `DISCONNECT` via the
/// transport. Codex 2026-06-03 [P2] (ardop): DISCONNECT failure must
/// NOT be silently swallowed — the rejected peer's ARQ link might
/// remain held by the modem. We surface
/// [`VaraListenerError::DisconnectFailed`] so the caller is forced to
/// reset.
pub fn serve_inbound_one(
    transport: &mut VaraTransport,
    allowed: &AllowedStations,
    arms: &ListenerArmsRecord,
    deadline: Duration,
) -> Result<InboundOutcome, VaraListenerError> {
    let start = Instant::now();
    loop {
        let elapsed = start.elapsed();
        if elapsed >= deadline {
            return Err(VaraListenerError::Timeout);
        }
        match transport.recv_line_distinguishing_eof() {
            RecvOutcome::Line(InboundCommand::Connected { target, .. }) => {
                let peer = parse_peer_call(&target);
                let decision = decide_for_vara_event(&peer, allowed, arms);
                match decision {
                    ListenerDecision::Accept => {
                        return Ok(InboundOutcome::Accepted {
                            peer,
                            peer_call: target,
                        });
                    }
                    ListenerDecision::RejectAllowlist => {
                        if let Err(e) = transport.send(&OutboundCommand::Disconnect) {
                            return Err(VaraListenerError::DisconnectFailed(format!(
                                "reject-allowlist disconnect failed for {target}: {e}; \
                                 modem may still be connected to the rejected peer"
                            )));
                        }
                        return Ok(InboundOutcome::RejectedAllowlist { peer });
                    }
                    ListenerDecision::RejectExpired => {
                        if let Err(e) = transport.send(&OutboundCommand::Disconnect) {
                            return Err(VaraListenerError::DisconnectFailed(format!(
                                "reject-expired disconnect failed for {target}: {e}; \
                                 modem may still be connected to the rejected peer"
                            )));
                        }
                        return Ok(InboundOutcome::RejectedExpired { peer });
                    }
                    // VARA has no password layer → the foundation never
                    // returns RejectPassword in this code path. Defensive
                    // arm: route to allowlist-reject + still send DISCONNECT.
                    ListenerDecision::RejectPassword => {
                        if let Err(e) = transport.send(&OutboundCommand::Disconnect) {
                            return Err(VaraListenerError::DisconnectFailed(format!(
                                "reject-password (defensive) disconnect failed for {target}: {e}; \
                                 modem may still be connected to the rejected peer"
                            )));
                        }
                        return Ok(InboundOutcome::RejectedAllowlist { peer });
                    }
                }
            }
            // Terminal mid-listen: peer hung up before completing the
            // CONNECTED handshake (rare; mostly happens on a race
            // between the operator disarming and a peer call arriving).
            RecvOutcome::Line(InboundCommand::Disconnected) => {
                return Err(VaraListenerError::RemoteDisconnect);
            }
            // Absorb every other async event (PTT / BUFFER / PENDING /
            // CANCELPENDING / LINK REGISTERED / IAMALIVE / OFFLINE /
            // MissingSoundcard / WrongCallsign / Unknown) and keep
            // waiting for a CONNECTED.
            RecvOutcome::Line(_) => continue,
            // recv timeout (per VaraConfig.read_timeout): treat as a tick
            // — continue the loop until the overall deadline expires. The
            // caller's read_timeout (default 2s) plus this loop give a
            // reasonable polling cadence without burning CPU.
            RecvOutcome::Idle => continue,
            // Codex P1 #3 (tuxlink-6urh2 v2): the cmd socket itself is
            // gone (peer FIN). Prior to this fix, `recv()` folded this
            // into the same `Ok(None)` bucket as an ordinary timeout,
            // so this loop kept polling a dead socket as if it were
            // merely idle. Surface as terminal instead — the consumer
            // task's match on this fn's `Err` falls through to its
            // generic "transport error; stopping" arm for any variant
            // it doesn't special-case (only `Timeout` and
            // `RemoteDisconnect` get bespoke non-terminal handling
            // there), so this correctly ends the consumer's poll loop
            // rather than spinning on a closed socket.
            RecvOutcome::Eof => return Err(VaraListenerError::TransportClosed),
            RecvOutcome::Err(e) => return Err(VaraListenerError::Io(e)),
        }
    }
}

// ──────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::winlink::listener::transport::TransportKind;
    use crate::winlink::listener::DEFAULT_TTL;
    use std::io::{BufRead, BufReader, Write};
    use std::net::{SocketAddr, TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::SystemTime;

    fn addr(call: &str, ssid: u8) -> Address {
        Address {
            call: call.into(),
            ssid,
        }
    }

    fn allowed_with(call: &str, ssid: u8) -> AllowedStations {
        // Restrict-mode so the allowlist gates on the callsign list.
        // (Foundation default since tuxlink-7vea is allow_all=TRUE; tests
        // exercising the allowlist gate must opt back into restrict-mode.)
        let mut a = AllowedStations::new().with_allow_all(false);
        a.add_callsign(addr(call, ssid));
        a
    }

    fn arms_fresh_vara() -> ListenerArmsRecord {
        let mut r = ListenerArmsRecord::arm(TransportKind::VaraHf, DEFAULT_TTL);
        r.armed_at = SystemTime::now();
        r
    }

    fn arms_disarmed_vara() -> ListenerArmsRecord {
        let mut r = ListenerArmsRecord::arm(TransportKind::VaraHf, DEFAULT_TTL);
        r.armed_at = SystemTime::now();
        r.disarm();
        r
    }

    // ── parse_peer_call ──────────────────────────────────────────

    #[test]
    fn parse_peer_call_bare_callsign() {
        let a = parse_peer_call("W4PHS");
        assert_eq!(a, addr("W4PHS", 0));
    }

    #[test]
    fn parse_peer_call_ssid_present() {
        let a = parse_peer_call("W4PHS-7");
        assert_eq!(a, addr("W4PHS", 7));
    }

    #[test]
    fn parse_peer_call_ssid_zero_strips_to_bare() {
        let a = parse_peer_call("W4PHS-0");
        assert_eq!(a, addr("W4PHS", 0));
    }

    #[test]
    fn parse_peer_call_lowercase_upcased() {
        let a = parse_peer_call("w4phs");
        assert_eq!(a, addr("W4PHS", 0));
    }

    #[test]
    fn parse_peer_call_whitespace_trimmed() {
        let a = parse_peer_call("  W4PHS-3  ");
        assert_eq!(a, addr("W4PHS", 3));
    }

    #[test]
    fn parse_peer_call_invalid_ssid_falls_back_to_bare() {
        let a = parse_peer_call("FOO-99");
        assert_eq!(a, addr("FOO-99", 0));
    }

    // ── station_password_no_keyring ──────────────────────────────

    #[test]
    fn station_password_no_keyring_is_never_set() {
        let p = station_password_no_keyring();
        assert!(
            !p.is_set(),
            "VARA no-keyring StationPassword must always report unset"
        );
    }

    #[test]
    fn station_password_no_keyring_verify_returns_false() {
        let p = station_password_no_keyring();
        assert!(!p.verify("anything"));
        assert!(!p.verify(""));
    }

    // ── allowed_stations_path ───────────────────────────────────

    #[test]
    fn allowed_stations_path_namespaces_under_listener_vara() {
        let root = std::path::Path::new("/var/lib/tuxlink");
        let p = allowed_stations_path(root);
        assert!(p.ends_with("listener/vara/allowed_stations.json"));
        assert!(p.starts_with(root));
    }

    // ── decide_for_vara_event ──────────────────────────────────

    #[test]
    fn decide_peer_not_in_allowlist_rejects_allowlist() {
        let peer = addr("W4PHS", 0);
        let allowed = allowed_with("N7CPZ", 0);
        let arms = arms_fresh_vara();
        let decision = decide_for_vara_event(&peer, &allowed, &arms);
        assert_eq!(decision, ListenerDecision::RejectAllowlist);
    }

    #[test]
    fn decide_peer_in_allowlist_armed_accepts_without_password() {
        // Same posture as ARDOP: no password layer for VARA. Gate flow:
        // allowlist OK + arms OK + password.is_set()=false → skip
        // password branch → Accept.
        let peer = addr("N7CPZ", 0);
        let allowed = allowed_with("N7CPZ", 0);
        let arms = arms_fresh_vara();
        let decision = decide_for_vara_event(&peer, &allowed, &arms);
        assert_eq!(decision, ListenerDecision::Accept);
    }

    #[test]
    fn decide_disarmed_arms_rejects_expired() {
        let peer = addr("N7CPZ", 0);
        let allowed = allowed_with("N7CPZ", 0);
        let arms = arms_disarmed_vara();
        let decision = decide_for_vara_event(&peer, &allowed, &arms);
        assert_eq!(decision, ListenerDecision::RejectExpired);
    }

    #[test]
    fn decide_allow_all_accepts_any_peer() {
        // Operator opt-in: allow_all=TRUE. Any peer that completes the
        // ARQ handshake gets accepted (the WLE-compatible permissive
        // posture).
        let peer = addr("UNKNOWN", 0);
        let mut allowed = AllowedStations::new();
        allowed.set_allow_all(true);
        let arms = arms_fresh_vara();
        let decision = decide_for_vara_event(&peer, &allowed, &arms);
        assert_eq!(decision, ListenerDecision::Accept);
    }

    // ── set_listen / serve_inbound_one wire tests ───────────────

    /// Spawn a pair of TCP listeners — one for the VARA cmd socket,
    /// one for the data socket — and return both addresses. The handler
    /// closure is invoked with the cmd-side TcpStream once the client
    /// connects; the data socket is accepted but silent (the listener
    /// path doesn't touch the data socket; B2F-answer does, which is
    /// out of scope for these wire tests).
    ///
    /// Mirrors ARDOP's `spawn_mock_tnc` helper but two-socket-shaped
    /// to satisfy `VaraTransport::connect`.
    fn spawn_mock_vara<F>(handler: F) -> (SocketAddr, SocketAddr, thread::JoinHandle<()>, thread::JoinHandle<()>)
    where
        F: FnOnce(TcpStream) + Send + 'static,
    {
        let cmd_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let cmd_addr = cmd_listener.local_addr().unwrap();
        let data_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let data_addr = data_listener.local_addr().unwrap();

        let cmd_handle = thread::spawn(move || {
            let (conn, _) = cmd_listener.accept().unwrap();
            conn.set_read_timeout(Some(Duration::from_secs(2))).ok();
            handler(conn);
        });
        let data_handle = thread::spawn(move || {
            // Accept the data socket so VaraTransport::connect's second
            // TCP connect succeeds; then hold it open so the connection
            // doesn't race-close while the cmd-side test runs.
            let (_conn, _) = data_listener.accept().unwrap();
            // Keep the data socket alive for ~1s — enough for the cmd
            // test to complete; then drop closes it cleanly.
            thread::sleep(Duration::from_secs(1));
        });
        (cmd_addr, data_addr, cmd_handle, data_handle)
    }

    fn read_cmd_line(conn: &mut BufReader<TcpStream>) -> String {
        let mut buf = Vec::new();
        match conn.read_until(b'\r', &mut buf) {
            Ok(0) | Err(_) => return String::new(),
            Ok(_) => {}
        }
        if buf.last() == Some(&b'\r') {
            buf.pop();
        }
        String::from_utf8(buf).unwrap_or_default()
    }

    fn write_reply(conn: &mut TcpStream, line: &str) {
        let _ = conn.write_all(format!("{line}\r").as_bytes());
    }

    fn connect_transport(cmd_addr: SocketAddr, data_addr: SocketAddr) -> VaraTransport {
        let cfg = super::super::transport::VaraConfig {
            host: cmd_addr.ip().to_string(),
            cmd_port: cmd_addr.port(),
            data_port: data_addr.port(),
            connect_timeout: Duration::from_secs(2),
            // Short read timeout so recv() returns None promptly when
            // the mock is idle — keeps the wire tests fast.
            read_timeout: Some(Duration::from_millis(100)),
            data_read_timeout: Some(Duration::from_millis(100)),
        };
        VaraTransport::connect(cfg).unwrap()
    }

    #[test]
    #[allow(non_snake_case)]
    fn set_listen_true_sends_LISTEN_ON() {
        let recorded: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let rec = recorded.clone();
        let (cmd_addr, data_addr, cmd_handle, data_handle) = spawn_mock_vara(move |conn| {
            let mut reader = BufReader::new(conn);
            // Read one setter line, record it, then linger so the
            // client's drain loop can complete.
            let line = read_cmd_line(&mut reader);
            if !line.is_empty() {
                rec.lock().unwrap().push(line);
            }
            thread::sleep(Duration::from_millis(200));
        });

        let mut transport = connect_transport(cmd_addr, data_addr);
        set_listen(&mut transport, true).expect("set_listen(true) must succeed");
        drop(transport);
        cmd_handle.join().unwrap();
        data_handle.join().unwrap();

        let lines = recorded.lock().unwrap().clone();
        assert_eq!(lines, vec!["LISTEN ON"]);
    }

    #[test]
    #[allow(non_snake_case)]
    fn set_listen_false_sends_LISTEN_OFF() {
        let recorded: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let rec = recorded.clone();
        let (cmd_addr, data_addr, cmd_handle, data_handle) = spawn_mock_vara(move |conn| {
            let mut reader = BufReader::new(conn);
            let line = read_cmd_line(&mut reader);
            if !line.is_empty() {
                rec.lock().unwrap().push(line);
            }
            thread::sleep(Duration::from_millis(200));
        });

        let mut transport = connect_transport(cmd_addr, data_addr);
        set_listen(&mut transport, false).expect("set_listen(false) must succeed");
        drop(transport);
        cmd_handle.join().unwrap();
        data_handle.join().unwrap();

        let lines = recorded.lock().unwrap().clone();
        assert_eq!(lines, vec!["LISTEN OFF"]);
    }

    #[test]
    fn serve_inbound_one_accept_returns_peer() {
        // VARA emits CONNECTED <mycall> <target> <bw>; the gate accepts
        // because the peer is on the allowlist; serve_inbound_one returns
        // Accepted with the parsed peer + verbatim peer_call.
        let (cmd_addr, data_addr, cmd_handle, data_handle) = spawn_mock_vara(|mut conn| {
            // Wait briefly so the client is registered; then emit.
            thread::sleep(Duration::from_millis(50));
            write_reply(&mut conn, "CONNECTED N0CALL W4PHS 2300");
            thread::sleep(Duration::from_millis(300));
        });

        let mut transport = connect_transport(cmd_addr, data_addr);
        let allowed = allowed_with("W4PHS", 0);
        let arms = arms_fresh_vara();
        let outcome =
            serve_inbound_one(&mut transport, &allowed, &arms, Duration::from_secs(2)).unwrap();

        match outcome {
            InboundOutcome::Accepted { peer, peer_call } => {
                assert_eq!(peer, addr("W4PHS", 0));
                assert_eq!(peer_call, "W4PHS");
            }
            other => panic!("expected Accepted, got: {other:?}"),
        }
        drop(transport);
        cmd_handle.join().unwrap();
        data_handle.join().unwrap();
    }

    #[test]
    #[allow(non_snake_case)]
    fn serve_inbound_one_reject_allowlist_sends_DISCONNECT() {
        // VARA emits CONNECTED for a peer NOT on the allowlist; the gate
        // rejects with RejectAllowlist; serve_inbound_one sends DISCONNECT
        // and surfaces the reject.
        let recorded: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let rec = recorded.clone();
        let (cmd_addr, data_addr, cmd_handle, data_handle) = spawn_mock_vara(move |conn| {
            let mut writer = conn.try_clone().unwrap();
            let mut reader = BufReader::new(conn);
            // First emit the CONNECTED.
            write_reply(&mut writer, "CONNECTED N0CALL INTRUDER 2300");
            // Then read whatever the client sends back (expected: DISCONNECT).
            let line = read_cmd_line(&mut reader);
            if !line.is_empty() {
                rec.lock().unwrap().push(line);
            }
            thread::sleep(Duration::from_millis(100));
        });

        let mut transport = connect_transport(cmd_addr, data_addr);
        let allowed = allowed_with("N7CPZ", 0); // INTRUDER NOT on list
        let arms = arms_fresh_vara();
        let outcome =
            serve_inbound_one(&mut transport, &allowed, &arms, Duration::from_secs(2)).unwrap();

        match outcome {
            InboundOutcome::RejectedAllowlist { peer } => {
                assert_eq!(peer, addr("INTRUDER", 0));
            }
            other => panic!("expected RejectedAllowlist, got: {other:?}"),
        }
        drop(transport);
        cmd_handle.join().unwrap();
        data_handle.join().unwrap();

        let lines = recorded.lock().unwrap().clone();
        assert!(
            lines.iter().any(|l| l.starts_with("DISCONNECT")),
            "must have sent DISCONNECT on reject; saw {lines:?}"
        );
    }

    #[test]
    fn serve_inbound_one_reject_expired_when_arms_disarmed() {
        // Operator disarmed the listener; an inbound CONNECTED arrives
        // anyway (race between disarm UI and modem event). The gate
        // rejects with RejectExpired and we send DISCONNECT.
        let recorded: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let rec = recorded.clone();
        let (cmd_addr, data_addr, cmd_handle, data_handle) = spawn_mock_vara(move |conn| {
            let mut writer = conn.try_clone().unwrap();
            let mut reader = BufReader::new(conn);
            write_reply(&mut writer, "CONNECTED N0CALL W4PHS 2300");
            let line = read_cmd_line(&mut reader);
            if !line.is_empty() {
                rec.lock().unwrap().push(line);
            }
            thread::sleep(Duration::from_millis(100));
        });

        let mut transport = connect_transport(cmd_addr, data_addr);
        let allowed = allowed_with("W4PHS", 0);
        let arms = arms_disarmed_vara();
        let outcome =
            serve_inbound_one(&mut transport, &allowed, &arms, Duration::from_secs(2)).unwrap();

        match outcome {
            InboundOutcome::RejectedExpired { peer } => {
                assert_eq!(peer, addr("W4PHS", 0));
            }
            other => panic!("expected RejectedExpired, got: {other:?}"),
        }
        drop(transport);
        cmd_handle.join().unwrap();
        data_handle.join().unwrap();

        let lines = recorded.lock().unwrap().clone();
        assert!(
            lines.iter().any(|l| l.starts_with("DISCONNECT")),
            "must have sent DISCONNECT on reject; saw {lines:?}"
        );
    }

    #[test]
    fn serve_inbound_one_timeout_when_no_connected_arrives() {
        // VARA is connected but stays quiet (no peer connects); the
        // call returns VaraListenerError::Timeout after the deadline.
        let (cmd_addr, data_addr, cmd_handle, data_handle) = spawn_mock_vara(|_conn| {
            // Stay idle so the client's recv() times out cleanly each
            // poll and the loop eventually exceeds the deadline.
            thread::sleep(Duration::from_millis(500));
        });

        let mut transport = connect_transport(cmd_addr, data_addr);
        let allowed = allowed_with("N7CPZ", 0);
        let arms = arms_fresh_vara();
        let err = serve_inbound_one(
            &mut transport,
            &allowed,
            &arms,
            Duration::from_millis(200),
        )
        .expect_err("must time out");
        assert!(matches!(err, VaraListenerError::Timeout));
        drop(transport);
        cmd_handle.join().unwrap();
        data_handle.join().unwrap();
    }

    /// Codex P1 #3 (tuxlink-6urh2 v2): an EOF on the cmd socket (peer FIN)
    /// must surface as `VaraListenerError::TransportClosed`, a DISTINCT
    /// terminal error from `Timeout` — prior to the fix, `recv()` folded
    /// EOF into the same `Ok(None)` bucket as an ordinary read timeout, so
    /// this loop would have kept "timing out" forever against a socket
    /// that was actually gone, never returning to the consumer task's
    /// generic transport-error handling.
    #[test]
    fn serve_inbound_one_surfaces_transport_closed_on_eof() {
        let (cmd_addr, data_addr, cmd_handle, data_handle) = spawn_mock_vara(|conn| {
            // Immediate close — sends FIN before any CONNECTED arrives.
            drop(conn);
        });

        let mut transport = connect_transport(cmd_addr, data_addr);
        let allowed = allowed_with("N7CPZ", 0);
        let arms = arms_fresh_vara();
        let err = serve_inbound_one(
            &mut transport,
            &allowed,
            &arms,
            // Generous relative to connect_transport's 100ms read_timeout —
            // the EOF should be observed on the first or second poll, well
            // before this deadline, proving it's terminal rather than a
            // repeated timeout tick.
            Duration::from_millis(500),
        )
        .expect_err("EOF must be terminal, not a timeout");
        assert!(
            matches!(err, VaraListenerError::TransportClosed),
            "expected TransportClosed, got {err:?}"
        );
        drop(transport);
        cmd_handle.join().unwrap();
        data_handle.join().unwrap();
    }
}
