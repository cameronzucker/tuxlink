//! ARDOP P2P inbound-listener wiring.
//!
//! Consumes the shared listener-arms foundation (`crate::winlink::listener`)
//! and exposes:
//!
//! - [`parse_peer_call`] — translate the verbatim string from
//!   ardopcf's `CONNECTED <call> <bw>` event into an
//!   [`crate::winlink::ax25::frame::Address`]. Strips the WLE-style `-0`
//!   tail (SSID 0 is implicit) per `dev/scratch/winlink-re/findings/ardop-p2p.md`.
//! - [`decide_for_ardop_event`] — runs the allowlist + arms-TTL gate against
//!   a parsed peer, returning the [`ListenerDecision`] the caller acts on.
//!   ARDOP has no station-password layer per ardop-p2p.md divergence 2 —
//!   plaintext shared secret over RF is worse than no secret. The
//!   `StationPassword::no_keyring()` adapter is built so the foundation's
//!   `is_set()` returns FALSE and the password gate is skipped.
//! - [`allowed_stations_path`] — config-dir-relative location for the ARDOP
//!   allowlist JSON, namespaced under `listener/ardop/` so future Telnet /
//!   Packet / VARA allowlists land beside it without colliding.
//! - [`set_listen`] — runtime toggle that sends `LISTEN TRUE\r` /
//!   `LISTEN FALSE\r` over the cmd socket and waits for ardopcf's echoback.
//!   Mirrors the `set_and_ack` private helper used inside `init_tnc`.
//! - [`serve_inbound_one`] — single-shot wait for an inbound `CONNECTED`
//!   event, run the gate, and EITHER record a `ConnectInfo` (Accept — the
//!   caller hands the data stream to `run_exchange_with_role(Answer)`) OR
//!   send `DISCONNECT` (Reject*).
//!
//! ## Why a "no-keyring" StationPassword
//!
//! The foundation's `decide::listener_decide` is transport-agnostic — every
//! caller passes a `StationPassword`. ARDOP doesn't have a station-password
//! layer, but we still call the same `listener_decide` so the gate's
//! ordering + audit semantics stay identical across transports. The
//! "no-keyring" `StationPassword` is a thin factory that always returns
//! `NoEntry` — `is_set()` is FALSE → the password branch in
//! `listener_decide` is a no-op for ARDOP — same result as if ARDOP had its
//! own gate, with one less code path to drift.
//!
//! Spec: `docs/design/2026-06-03-multi-transport-listener-architecture.md` §4.1
//! Wire/protocol authority: `dev/scratch/winlink-re/findings/ardop-p2p.md`
//! bd: tuxlink-dhbl

use std::io;
use std::path::PathBuf;
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

use crate::winlink::ax25::frame::Address;
use crate::winlink::credentials::EntryLike;
use crate::winlink::listener::{
    listener_decide, AllowedStations, ListenerArmsRecord, ListenerDecision, PeerId,
    StationPassword,
};

use super::command::Command;
use super::session::{arq_disconnect, CmdSocket, ConnectInfo, SessionError};

/// How long to wait for the echo-back ack on a `LISTEN` setter.
///
/// Mirrors the `SETTER_ACK_TIMEOUT` private constant in `session.rs`
/// (10 s) — keeps a wedged TNC from blocking the listener-toggle flow.
const LISTEN_ACK_TIMEOUT: Duration = Duration::from_secs(10);

// ──────────────────────────────────────────────────────────────
// Peer call parsing
// ──────────────────────────────────────────────────────────────

/// Parse the verbatim peer-call string from ardopcf's `CONNECTED <call> <bw>`
/// event into an [`Address`].
///
/// Handles three input shapes, mirroring the WLE behavior at
/// `ArdopSession.cs:2252-2256` (per `dev/scratch/winlink-re/findings/ardop-p2p.md`):
///
/// - `"W4PHS"` → `Address { call: "W4PHS", ssid: 0 }`.
/// - `"W4PHS-7"` → `Address { call: "W4PHS", ssid: 7 }`.
/// - `"W4PHS-0"` → `Address { call: "W4PHS", ssid: 0 }` (WLE strips the
///   redundant trailing `-0`; we normalize the same way so allowlist
///   comparisons against `"W4PHS"` match cleanly).
///
/// Inputs are trimmed and upper-cased on the call side. The SSID, if
/// present, must parse as 0..=15; out-of-range values silently fall back
/// to SSID 0 with the full original token preserved — defensive against a
/// malformed event rather than crashing the listener loop. Same posture as
/// `winlink_backend::parse_call_ssid`'s strict variant minus the error
/// propagation (we are downstream of an already-on-air event; the choice
/// is "tolerate and log via the gate's reject path" rather than "panic").
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
/// layer (ARDOP, per `ardop-p2p.md` divergence 2).
///
/// The shared `listener_decide` function takes a `StationPassword` by
/// reference. ARDOP must still call `listener_decide` to keep the
/// per-transport gate semantics identical; this adapter lets the
/// password branch short-circuit cleanly without ARDOP-specific code in
/// the foundation. `StationPassword::is_set()` returns FALSE because the
/// backing keyring entry always returns `NoEntry`.
pub fn station_password_no_keyring() -> StationPassword {
    use crate::winlink::listener::station_password::EntryFactory;
    let factory: EntryFactory = Box::new(|_service: &str, _account: &str| {
        Box::new(NoKeyringEntry) as Box<dyn EntryLike>
    });
    StationPassword::with_factory(factory)
}

/// Always-empty keyring entry — every operation behaves as if no value is
/// stored. Read/delete return `NoEntry`; writes are silently dropped (the
/// caller would only set if ARDOP wanted to gain a password layer, which
/// the divergence forbids).
struct NoKeyringEntry;

impl EntryLike for NoKeyringEntry {
    fn get_password(&self) -> Result<String, keyring::Error> {
        Err(keyring::Error::NoEntry)
    }
    fn set_password(&self, _password: &str) -> Result<(), keyring::Error> {
        // No-op: ARDOP has no station-password layer.
        Ok(())
    }
    fn delete_password(&self) -> Result<(), keyring::Error> {
        Err(keyring::Error::NoEntry)
    }
}

// ──────────────────────────────────────────────────────────────
// Allowed-stations on-disk location
// ──────────────────────────────────────────────────────────────

/// Resolve the on-disk path for the ARDOP allowed-stations JSON file,
/// given the tuxlink config directory (per `config::config_path`'s
/// resolution rules).
///
/// Layout: `<config-dir>/listener/ardop/allowed_stations.json`. The
/// `listener/<transport>/` subtree namespaces future Telnet / Packet /
/// VARA allowlists so they share the same `AllowedStations` JSON shape
/// without colliding on a single flat file.
pub fn allowed_stations_path(config_dir: &std::path::Path) -> PathBuf {
    config_dir
        .join("listener")
        .join("ardop")
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
/// `password_input` is hard-wired to `None` because ARDOP has no
/// challenge-response layer — see module docs. The shared
/// `listener_decide` skips the password branch via `StationPassword::is_set()
/// == false`.
///
/// Pure over its inputs — no I/O. Production callers compose this with
/// the on-disk allowlist loaded once at arm time.
pub fn decide_for_ardop_event(
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

/// Toggle the ardopcf modem's `LISTEN` flag at runtime.
///
/// Sends `LISTEN TRUE\r` or `LISTEN FALSE\r` and absorbs interleaved
/// async events (NewState / Ptt / Busy / Buffer / Status / Ping*) until
/// the matching echo-back arrives or the wait times out. Mirrors the
/// `set_and_ack` semantics used inside `init_tnc` — the same tolerance
/// for in-flight async events keeps the toggle robust on a busy link.
///
/// This is the runtime counterpart to `init_tnc`'s static `LISTEN FALSE`
/// (which sets the initial state at session boot). Use this to arm /
/// disarm the listener after init has completed.
pub fn set_listen(sock: &mut CmdSocket, enabled: bool) -> Result<(), SessionError> {
    let arg = if enabled { "TRUE" } else { "FALSE" };
    sock.send_line(&format!("LISTEN {arg}"))?;

    loop {
        match sock.recv_event(LISTEN_ACK_TIMEOUT) {
            Ok(Command::EchoBack(name)) if name.eq_ignore_ascii_case("LISTEN") => {
                return Ok(());
            }
            Ok(Command::EchoBack(other)) => {
                return Err(SessionError::Unexpected {
                    cmd: "LISTEN".into(),
                    got: other,
                });
            }
            Ok(Command::Fault(msg)) => return Err(SessionError::Fault(msg)),
            // Absorb async events — they can interleave with the echo-back
            // on a busy link.
            Ok(_) => continue,
            Err(RecvTimeoutError::Timeout) => {
                return Err(SessionError::Timeout {
                    cmd: "LISTEN".into(),
                });
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(SessionError::Io(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "cmd socket disconnected while waiting for LISTEN ack",
                )));
            }
        }
    }
}

// ──────────────────────────────────────────────────────────────
// serve_inbound_one
// ──────────────────────────────────────────────────────────────

/// Outcome of one [`serve_inbound_one`] call.
///
/// `Accepted` carries the [`ConnectInfo`] the caller hands to the
/// answerer-side B2F driver. `RejectedAllowlist` / `RejectedExpired` are
/// terminal for this inbound — the listener loop should record the
/// reject (in the forensics log) and continue waiting for the next
/// `CONNECTED` event. `Timeout` means no peer arrived inside the
/// deadline; the loop typically reissues `serve_inbound_one` until the
/// arms TTL expires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InboundOutcome {
    /// Peer passed the gate — hand `ConnectInfo` to the answerer.
    Accepted {
        info: ConnectInfo,
        peer: Address,
    },
    /// Peer rejected on the allowlist.
    RejectedAllowlist { peer: Address },
    /// The arms TTL has elapsed (or the operator disarmed).
    RejectedExpired { peer: Address },
}

/// Wait for ONE inbound `CONNECTED <peer> <bw>` event, run the gate,
/// and either accept (caller proceeds to B2F answer) or send
/// `DISCONNECT` over the cmd socket and surface the reject class.
///
/// `deadline` is the overall budget: relative time from "now" to give
/// up waiting for a CONNECTED event with `RecvTimeoutError::Timeout`
/// (caller's loop re-issues if the arms window still has room).
///
/// Async-event interleaving: NewState, Ptt, Busy, Buffer, Status,
/// PingAck, Ping, EchoBack are absorbed and the wait continues. A
/// `Fault` / `Disconnected` / `NEWSTATE DISC` arriving without a prior
/// `CONNECTED` is surfaced as `SessionError::Fault` — the listener
/// caller decides whether to retry or disarm.
///
/// On a Reject decision, the function attempts `arq_disconnect` with a
/// 5 s budget. Failures to send DISCONNECT are intentionally swallowed
/// inside this function — the reject decision is the security-relevant
/// answer; a follow-on transport error doesn't change it. The caller's
/// forensics log records the reject class.
pub fn serve_inbound_one(
    sock: &mut CmdSocket,
    allowed: &AllowedStations,
    arms: &ListenerArmsRecord,
    deadline: Duration,
) -> Result<InboundOutcome, SessionError> {
    let start = Instant::now();
    loop {
        let elapsed = start.elapsed();
        if elapsed >= deadline {
            return Err(SessionError::Timeout {
                cmd: "INBOUND-CONNECTED".into(),
            });
        }
        let remaining = deadline - elapsed;
        match sock.recv_event(remaining) {
            Ok(Command::Connected { peer_call, bandwidth_hz }) => {
                let peer = parse_peer_call(&peer_call);
                let decision = decide_for_ardop_event(&peer, allowed, arms);
                match decision {
                    ListenerDecision::Accept => {
                        return Ok(InboundOutcome::Accepted {
                            info: ConnectInfo {
                                peer_call,
                                bandwidth_hz,
                            },
                            peer,
                        });
                    }
                    ListenerDecision::RejectAllowlist => {
                        let _ = arq_disconnect(sock, Duration::from_secs(5));
                        return Ok(InboundOutcome::RejectedAllowlist { peer });
                    }
                    ListenerDecision::RejectExpired => {
                        let _ = arq_disconnect(sock, Duration::from_secs(5));
                        return Ok(InboundOutcome::RejectedExpired { peer });
                    }
                    // ARDOP has no password layer → the foundation never
                    // returns RejectPassword in this code path. If a
                    // future foundation change reorders the gate so it
                    // CAN return RejectPassword without a configured
                    // password, treat it as a defensive "reject + log
                    // allowlist class" rather than an Accept by default.
                    ListenerDecision::RejectPassword => {
                        let _ = arq_disconnect(sock, Duration::from_secs(5));
                        return Ok(InboundOutcome::RejectedAllowlist { peer });
                    }
                }
            }
            Ok(Command::Fault(msg)) => {
                return Err(SessionError::Fault(msg));
            }
            Ok(Command::Disconnected) | Ok(Command::NewState(super::command::State::Disc)) => {
                return Err(SessionError::Fault(
                    "DISC / DISCONNECTED before CONNECTED — listener path".into(),
                ));
            }
            // Absorb every other async event (NewState, Ptt, Busy, Buffer,
            // Status, PingAck, Ping, EchoBack of an earlier setter) and
            // keep waiting for a CONNECTED.
            Ok(_) => continue,
            Err(RecvTimeoutError::Timeout) => {
                return Err(SessionError::Timeout {
                    cmd: "INBOUND-CONNECTED".into(),
                });
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(SessionError::Io(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "cmd socket disconnected while waiting for inbound CONNECTED",
                )));
            }
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
        let mut a = AllowedStations::new();
        a.add_callsign(addr(call, ssid));
        a
    }

    fn arms_fresh_ardop() -> ListenerArmsRecord {
        let mut r = ListenerArmsRecord::arm(TransportKind::Ardop, DEFAULT_TTL);
        // Anchor armed_at to a fixed time so the SystemTime::now() inside
        // `listener_decide` compares against a known stamp — we don't
        // expire it inside the gate (the listener_decide uses now()), so
        // we just leave armed_at at "now" (the default arm() value).
        r.armed_at = SystemTime::now();
        r
    }

    fn arms_disarmed_ardop() -> ListenerArmsRecord {
        let mut r = ListenerArmsRecord::arm(TransportKind::Ardop, DEFAULT_TTL);
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
        // WLE-style: `-0` is the redundant default; strip so allowlist
        // entries for "W4PHS" match a peer that announced "W4PHS-0".
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
        // SSID 99 is out of range — defensively keep the original call
        // (sans the bogus tail) at SSID 0. The gate will reject if the
        // operator's allowlist doesn't include "FOO".
        let a = parse_peer_call("FOO-99");
        assert_eq!(a, addr("FOO-99", 0));
    }

    // ── station_password_no_keyring ──────────────────────────────

    #[test]
    fn station_password_no_keyring_is_never_set() {
        let p = station_password_no_keyring();
        assert!(
            !p.is_set(),
            "ARDOP no-keyring StationPassword must always report unset"
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
    fn allowed_stations_path_namespaces_under_listener_ardop() {
        let root = std::path::Path::new("/var/lib/tuxlink");
        let p = allowed_stations_path(root);
        assert!(p.ends_with("listener/ardop/allowed_stations.json"));
        assert!(p.starts_with(root));
    }

    // ── decide_for_ardop_event ──────────────────────────────────

    #[test]
    fn decide_peer_not_in_allowlist_rejects_allowlist() {
        let peer = addr("W4PHS", 0);
        let allowed = allowed_with("N7CPZ", 0);
        let arms = arms_fresh_ardop();
        let decision = decide_for_ardop_event(&peer, &allowed, &arms);
        assert_eq!(decision, ListenerDecision::RejectAllowlist);
    }

    #[test]
    fn decide_peer_in_allowlist_armed_accepts_without_password() {
        // Per ardop-p2p.md divergence 2: no password layer for ARDOP.
        // Gate flow: allowlist OK + arms OK + password.is_set()=false →
        // skip password branch → Accept.
        let peer = addr("N7CPZ", 0);
        let allowed = allowed_with("N7CPZ", 0);
        let arms = arms_fresh_ardop();
        let decision = decide_for_ardop_event(&peer, &allowed, &arms);
        assert_eq!(decision, ListenerDecision::Accept);
    }

    #[test]
    fn decide_disarmed_arms_rejects_expired() {
        let peer = addr("N7CPZ", 0);
        let allowed = allowed_with("N7CPZ", 0);
        let arms = arms_disarmed_ardop();
        let decision = decide_for_ardop_event(&peer, &allowed, &arms);
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
        let arms = arms_fresh_ardop();
        let decision = decide_for_ardop_event(&peer, &allowed, &arms);
        assert_eq!(decision, ListenerDecision::Accept);
    }

    // ── set_listen LISTEN-state-machine wire tests ───────────────

    /// Bind a local TCP listener, spawn a thread to accept one
    /// connection, and return the bound address together with a join
    /// handle. The handler runs on the server thread and receives the
    /// accepted `TcpStream` (with a 2-second read timeout already set).
    /// Mirrors the helper in `session.rs::tests`.
    fn spawn_mock_tnc<F>(handler: F) -> (SocketAddr, thread::JoinHandle<()>)
    where
        F: FnOnce(TcpStream) + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (conn, _) = listener.accept().unwrap();
            conn.set_read_timeout(Some(Duration::from_secs(2))).ok();
            handler(conn);
        });
        (addr, handle)
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

    #[test]
    fn set_listen_true_sends_listen_true_setter() {
        let recorded: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let rec = recorded.clone();
        let (sock_addr, server) = spawn_mock_tnc(move |conn| {
            let mut writer = conn.try_clone().unwrap();
            let mut reader = BufReader::new(conn);
            loop {
                let line = read_cmd_line(&mut reader);
                if line.is_empty() {
                    break;
                }
                rec.lock().unwrap().push(line.clone());
                let cmd_name = line.split_whitespace().next().unwrap_or("").to_string();
                write_reply(&mut writer, &cmd_name);
            }
        });

        let mut sock = CmdSocket::connect(sock_addr).unwrap();
        set_listen(&mut sock, true).expect("set_listen(true) must succeed");
        drop(sock);
        server.join().unwrap();

        let lines = recorded.lock().unwrap().clone();
        assert_eq!(lines, vec!["LISTEN TRUE"]);
    }

    #[test]
    fn set_listen_false_sends_listen_false_setter() {
        let recorded: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let rec = recorded.clone();
        let (sock_addr, server) = spawn_mock_tnc(move |conn| {
            let mut writer = conn.try_clone().unwrap();
            let mut reader = BufReader::new(conn);
            loop {
                let line = read_cmd_line(&mut reader);
                if line.is_empty() {
                    break;
                }
                rec.lock().unwrap().push(line.clone());
                let cmd_name = line.split_whitespace().next().unwrap_or("").to_string();
                write_reply(&mut writer, &cmd_name);
            }
        });

        let mut sock = CmdSocket::connect(sock_addr).unwrap();
        set_listen(&mut sock, false).expect("set_listen(false) must succeed");
        drop(sock);
        server.join().unwrap();

        let lines = recorded.lock().unwrap().clone();
        assert_eq!(lines, vec!["LISTEN FALSE"]);
    }

    #[test]
    fn set_listen_returns_fault_on_tnc_fault() {
        let (sock_addr, server) = spawn_mock_tnc(|conn| {
            let mut writer = conn.try_clone().unwrap();
            let mut reader = BufReader::new(conn);
            // Consume the LISTEN setter line.
            read_cmd_line(&mut reader);
            write_reply(&mut writer, "FAULT not initialized");
        });

        let mut sock = CmdSocket::connect(sock_addr).unwrap();
        let err = set_listen(&mut sock, true).expect_err("must fail on FAULT");
        assert!(matches!(err, SessionError::Fault(_)));
        drop(sock);
        server.join().unwrap();
    }

    #[test]
    fn set_listen_absorbs_interleaved_async_event_before_ack() {
        // Mock that sends a NEWSTATE async event BEFORE the LISTEN echo
        // back — the absorbed-events loop must skip it and still resolve
        // on the eventual echo-back.
        let (sock_addr, server) = spawn_mock_tnc(|conn| {
            let mut writer = conn.try_clone().unwrap();
            let mut reader = BufReader::new(conn);
            // Wait for the LISTEN setter, then send an async event, then ack.
            let _line = read_cmd_line(&mut reader);
            write_reply(&mut writer, "NEWSTATE IDLE");
            write_reply(&mut writer, "LISTEN");
        });

        let mut sock = CmdSocket::connect(sock_addr).unwrap();
        set_listen(&mut sock, true).expect("must tolerate interleaved async");
        drop(sock);
        server.join().unwrap();
    }

    // ── serve_inbound_one CONNECTED-routing tests ────────────────

    #[test]
    fn serve_inbound_one_accept_returns_connect_info() {
        // ardopcf emits CONNECTED <peer> <bw>; the gate accepts because
        // the peer is on the allowlist; serve_inbound_one returns the
        // ConnectInfo so the caller can hand the data stream to the B2F
        // answerer.
        let (sock_addr, server) = spawn_mock_tnc(|mut conn| {
            // Wait briefly so the client is registered; then emit.
            thread::sleep(Duration::from_millis(50));
            write_reply(&mut conn, "CONNECTED W4PHS 500");
            // Hold the connection open so the test's `sock` can still
            // talk to a live TCP peer (avoids a stray write-after-shutdown
            // race in the test).
            thread::sleep(Duration::from_millis(200));
        });

        let mut sock = CmdSocket::connect(sock_addr).unwrap();
        let allowed = allowed_with("W4PHS", 0);
        let arms = arms_fresh_ardop();
        let outcome =
            serve_inbound_one(&mut sock, &allowed, &arms, Duration::from_secs(2)).unwrap();

        match outcome {
            InboundOutcome::Accepted { info, peer } => {
                assert_eq!(info.peer_call, "W4PHS");
                assert_eq!(info.bandwidth_hz, 500);
                assert_eq!(peer, addr("W4PHS", 0));
            }
            other => panic!("expected Accepted, got: {other:?}"),
        }
        drop(sock);
        server.join().unwrap();
    }

    #[test]
    fn serve_inbound_one_reject_allowlist_sends_disconnect() {
        // ardopcf emits CONNECTED for a peer NOT on the allowlist; the
        // gate rejects with RejectAllowlist; serve_inbound_one issues a
        // DISCONNECT setter and surfaces the reject.
        let recorded: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let rec = recorded.clone();
        let (sock_addr, server) = spawn_mock_tnc(move |conn| {
            let mut writer = conn.try_clone().unwrap();
            let mut reader = BufReader::new(conn);
            // First emit the CONNECTED.
            write_reply(&mut writer, "CONNECTED INTRUDER 500");
            // Then wait for the DISCONNECT and ack it.
            loop {
                let line = read_cmd_line(&mut reader);
                if line.is_empty() {
                    break;
                }
                rec.lock().unwrap().push(line.clone());
                if line.starts_with("DISCONNECT") {
                    write_reply(&mut writer, "DISCONNECTED");
                    break;
                }
            }
        });

        let mut sock = CmdSocket::connect(sock_addr).unwrap();
        let allowed = allowed_with("N7CPZ", 0); // intruder NOT on list
        let arms = arms_fresh_ardop();
        let outcome =
            serve_inbound_one(&mut sock, &allowed, &arms, Duration::from_secs(2)).unwrap();

        match outcome {
            InboundOutcome::RejectedAllowlist { peer } => {
                assert_eq!(peer, addr("INTRUDER", 0));
            }
            other => panic!("expected RejectedAllowlist, got: {other:?}"),
        }
        drop(sock);
        server.join().unwrap();

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
        let (sock_addr, server) = spawn_mock_tnc(move |conn| {
            let mut writer = conn.try_clone().unwrap();
            let mut reader = BufReader::new(conn);
            write_reply(&mut writer, "CONNECTED W4PHS 500");
            loop {
                let line = read_cmd_line(&mut reader);
                if line.is_empty() {
                    break;
                }
                rec.lock().unwrap().push(line.clone());
                if line.starts_with("DISCONNECT") {
                    write_reply(&mut writer, "DISCONNECTED");
                    break;
                }
            }
        });

        let mut sock = CmdSocket::connect(sock_addr).unwrap();
        let allowed = allowed_with("W4PHS", 0);
        let arms = arms_disarmed_ardop();
        let outcome =
            serve_inbound_one(&mut sock, &allowed, &arms, Duration::from_secs(2)).unwrap();

        match outcome {
            InboundOutcome::RejectedExpired { peer } => {
                assert_eq!(peer, addr("W4PHS", 0));
            }
            other => panic!("expected RejectedExpired, got: {other:?}"),
        }
        drop(sock);
        server.join().unwrap();
    }

    #[test]
    fn serve_inbound_one_timeout_when_no_connected_arrives() {
        // ardopcf is connected but stays quiet (no peer connects); the
        // call returns SessionError::Timeout after the deadline.
        let (sock_addr, server) = spawn_mock_tnc(|_conn| {
            // Stay idle for a moment so the client's recv_event times out
            // cleanly.
            thread::sleep(Duration::from_millis(200));
        });

        let mut sock = CmdSocket::connect(sock_addr).unwrap();
        let allowed = allowed_with("N7CPZ", 0);
        let arms = arms_fresh_ardop();
        let err = serve_inbound_one(&mut sock, &allowed, &arms, Duration::from_millis(100))
            .expect_err("must time out");
        assert!(matches!(err, SessionError::Timeout { .. }));
        drop(sock);
        server.join().unwrap();
    }
}
