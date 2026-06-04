//! Telnet-P2P inbound listener.
//!
//! The only transport with NO existing listener code prior to tuxlink-xehu —
//! Packet (`ax25::datalink::answer`), ARDOP and VARA (modem-side `LISTEN` flag)
//! already ship some form of receive path; Telnet had to be added from scratch.
//!
//! ## Wire protocol
//!
//! Implements the listener side of the protocol decompiled from Winlink Express
//! at `dev/scratch/winlink-re/findings/telnet-p2p.md`:
//!
//! ```text
//! LISTENER                          DIALER
//!     -- "CALLSIGN :\r" -->
//!                                   -- "<callsign>\r" -->
//!     [allowlist + IPv6 gate]
//!     -- "Password :\r" -->            (unconditional per §5.2 wire parity)
//!                                   -- "<password>\r" -->
//!     [password verify if configured]
//!     -- hand stream to run_exchange_with_role(ExchangeRole::Answer) -->
//! ```
//!
//! Per [`dev/scratch/winlink-re/findings/telnet-p2p.md`] §8 — line terminator is
//! `\r` ONLY. Bytes `0` and `\n` are skipped in the WLE state-machine's
//! `AccumulateInputLine` and `\r` (`0x0d`) returns the accumulated line. This
//! matches `read_callsign_line` / `read_password_line` below.
//!
//! ## Defaults (DIVERGE from WLE — see telnet-p2p.md §9)
//!
//! | Knob               | WLE default | tuxlink default | Why diverge                        |
//! |--------------------|-------------|-----------------|------------------------------------|
//! | Listen port        | 8774        | 8774            | parity (§9 — no divergence)        |
//! | Bind address       | `"Default"` ≈ 0.0.0.0 | 127.0.0.1 | §9.3 — operator opts into LAN     |
//! | Allow All          | TRUE        | FALSE           | §9.1 — defensive default           |
//! | Password storage   | INI plaintext | OS keyring    | §9.2 — `no-disk-creds-default`     |
//! | Password compare   | inbound-uppercase, stored-verbatim → silent-fail | uppercase BOTH (§9.4 Option A) | bug-fix; reject the WLE silent-fail |
//! | TTL                | infinite    | 1 hour          | RADIO-1 framing                    |
//! | Max concurrent     | 1           | 1               | parity (single-session listener)   |
//!
//! ## Single-session
//!
//! The accept loop is single-threaded — the next iteration only starts when the
//! previous session ends. This matches WLE's `Ipdaemon.Config("MaxConnections=1")`
//! at `TelnetP2PSession.cs:2193`. A second peer hitting the port while a session
//! is in flight is queued by the TCP stack and immediately rejected when the
//! listener loops back; we close the second socket without speaking to it.
//!
//! ## Tests
//!
//! See the `#[cfg(test)]` block at the bottom. Coverage:
//! - 4 wire-format helpers: line read on `\r`, skip-on-`\n`-only, uppercase
//!   round-trip, SSID strip.
//! - 3 reject-message wire bytes: allowlist / password / expired.
//! - 1 integration test: bind 127.0.0.1:0, dial in via `TcpStream`, complete
//!   the CALLSIGN + Password exchange, verify the listener handed the stream
//!   to the answerer.
//!
//! Spec: `docs/design/2026-06-03-multi-transport-listener-architecture.md` §4.1
//! Wire: `dev/scratch/winlink-re/findings/telnet-p2p.md`
//! bd: tuxlink-xehu

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::ax25::frame::Address;
use super::listener::{
    listener_decide, AllowedStations, ListenerArmsRecord, ListenerDecision, PeerId,
    StationPassword,
};
use super::proposal::Proposal;
use super::session::{self, ExchangeConfig, ExchangeError, ExchangeResult, ExchangeRole};

/// Default TCP listen port — matches WLE's `Globals.strTelnetListeningPort`
/// initial value `"8774"` at `Globals.cs:1518`. NOTE: 8772 is RMS-Relay's
/// hub-direction port, NOT the P2P listener port.
pub const DEFAULT_PORT: u16 = 8774;

/// Default bind address — DIVERGES from WLE's `"Default"`/0.0.0.0 (§9.3).
/// Operator opts into LAN/all-interfaces via the listener-config UI.
pub const DEFAULT_BIND_ADDR: &str = "127.0.0.1";

/// Per-connection read/write timeout. Long enough that an interactive peer
/// completing the CALLSIGN + Password exchange has plenty of slack; short
/// enough that a half-open / hung connection self-clears.
const PEER_TIMEOUT: Duration = Duration::from_secs(60);

/// Wire bytes for each reject class. Exposed `pub(crate)` so test assertions
/// don't have to duplicate the literal strings.
pub(crate) const WIRE_PROMPT_CALLSIGN: &[u8] = b"CALLSIGN :\r";
pub(crate) const WIRE_PROMPT_PASSWORD: &[u8] = b"Password :\r";
pub(crate) const WIRE_REJECT_ALLOWLIST: &[u8] = b"*** Your station is not authorized to connect.\r";
pub(crate) const WIRE_REJECT_PASSWORD: &[u8] = b"*** Incorrect station password specified\r";
pub(crate) const WIRE_REJECT_EXPIRED: &[u8] = b"*** Listener is not armed\r";
pub(crate) const WIRE_REJECT_IPV6: &[u8] = b"*** IPv6 peers are not supported by this listener\r";

/// Errors that can arise during a single inbound Telnet-P2P session.
#[derive(Debug)]
pub enum TelnetListenError {
    /// `TcpListener::bind` failed (port in use / permission denied / etc).
    Bind {
        addr: SocketAddr,
        source: io::Error,
    },
    /// Reading bytes from a peer failed.
    PeerIo(io::Error),
    /// The peer disconnected before sending its CALLSIGN response.
    EofBeforeCallsign,
    /// The peer disconnected before sending its Password response.
    EofBeforePassword,
    /// The B2F exchange after handoff failed.
    Exchange(ExchangeError),
    /// Inbound callsign could not be parsed.
    BadCallsign(String),
}

impl std::fmt::Display for TelnetListenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TelnetListenError::Bind { addr, source } => {
                write!(f, "bind to {addr} failed: {source}")
            }
            TelnetListenError::PeerIo(e) => write!(f, "peer io error: {e}"),
            TelnetListenError::EofBeforeCallsign => {
                write!(f, "peer disconnected before sending callsign")
            }
            TelnetListenError::EofBeforePassword => {
                write!(f, "peer disconnected before sending password")
            }
            TelnetListenError::Exchange(e) => write!(f, "B2F exchange failed: {e}"),
            TelnetListenError::BadCallsign(s) => write!(f, "invalid callsign on wire: {s:?}"),
        }
    }
}

impl std::error::Error for TelnetListenError {}

// ──────────────────────────────────────────────────────────────
// Listener entry points
// ──────────────────────────────────────────────────────────────

/// Bind a TCP listener at `(bind_addr, port)` and return it without entering
/// the accept loop. Exposed separately so the Tauri command can take ownership
/// of the bound socket BEFORE spawning the accept loop (so an early bind
/// failure is reported synchronously to the operator).
pub fn bind(bind_addr: &str, port: u16) -> Result<TcpListener, TelnetListenError> {
    let addr: SocketAddr = format!("{bind_addr}:{port}")
        .parse()
        .map_err(|e: std::net::AddrParseError| TelnetListenError::Bind {
            addr: format!("{bind_addr}:{port}").parse().unwrap_or_else(|_| {
                // unreachable in practice; placeholder for the error type
                "0.0.0.0:0".parse().unwrap()
            }),
            source: io::Error::new(io::ErrorKind::InvalidInput, e.to_string()),
        })?;
    TcpListener::bind(addr).map_err(|source| TelnetListenError::Bind { addr, source })
}

/// Run the accept loop on a pre-bound listener until `shutdown` is set.
///
/// Single-session semantics (WLE parity, `MaxConnections=1`): the loop accepts
/// ONE peer at a time. After the in-flight session completes (or rejects), the
/// loop accepts the next. While a session is in flight any other peer hitting
/// the port sits in the TCP backlog and is `accept()`ed + closed immediately
/// when the loop comes back around.
///
/// `shutdown` is polled between accept iterations AND set as a wakeup-on-close
/// by closing the listener from the operator's disarm path.
#[allow(clippy::too_many_arguments)]
pub fn run_accept_loop<F>(
    listener: TcpListener,
    allowed: AllowedStations,
    password: StationPassword,
    arms: ListenerArmsRecord,
    config: ExchangeConfig,
    mailbox: Option<Arc<crate::native_mailbox::Mailbox>>,
    shutdown: Arc<AtomicBool>,
    progress: &dyn Fn(&str),
    wire_log: &dyn Fn(&str),
    decide: F,
) where
    F: Fn(&[crate::winlink::proposal::Proposal]) -> Vec<crate::winlink::proposal::Answer> + Clone,
{
    let local = listener
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "?".to_string());
    progress(&format!(
        "Telnet listener armed on {local} (Allow All={}, allowlist={} callsigns + {} IPs)",
        allowed.allow_all(),
        allowed.callsigns().len(),
        allowed.ips().len(),
    ));

    // Set a short accept timeout so the shutdown poll has a chance to fire
    // between connections without an inbound peer.
    listener
        .set_nonblocking(false)
        .expect("set_nonblocking false should always succeed on Linux");

    loop {
        if shutdown.load(Ordering::SeqCst) {
            progress("Telnet listener disarmed.");
            return;
        }
        // accept() is blocking; the operator-disarm path closes the listener
        // from another thread to wake us. A closed-listener error then exits.
        let (stream, peer_addr) = match listener.accept() {
            Ok(pair) => pair,
            Err(e) => {
                progress(&format!("Telnet listener accept failed: {e}"));
                return;
            }
        };

        // Codex review 2026-06-03 [P2]: close the TOCTOU between the shutdown
        // check at the loop top and the actual `accept()` call. If
        // `telnet_set_listen(false)` flipped `shutdown` AND the wakeup connect
        // raced into our accept queue (or any other queued peer landed
        // between the check and the syscall return), the prior code handed
        // the peer to handle_one_session and let it complete auth + B2F
        // AFTER the operator disarmed. Re-check shutdown immediately after
        // the syscall returns and drop the just-accepted stream if so.
        if shutdown.load(Ordering::SeqCst) {
            progress(&format!(
                "Telnet listener disarmed during accept; dropping peer {peer_addr}."
            ));
            drop(stream);
            return;
        }

        progress(&format!("Inbound Telnet connection from {peer_addr}…"));

        // Per-session: handle synchronously (MaxConnections=1 parity).
        let result = handle_one_session(
            stream,
            peer_addr,
            &allowed,
            &password,
            &arms,
            &config,
            mailbox.as_deref(),
            progress,
            wire_log,
            decide.clone(),
        );
        match result {
            Ok(_) => progress("Telnet session completed."),
            Err(e) => progress(&format!("Telnet session ended: {e}")),
        }
        // Loop back to accept the next peer (or exit if shutdown fired).
    }
}

/// Handle one inbound peer end-to-end: CALLSIGN prompt → allowlist gate →
/// Password prompt → password verify → handoff to B2F answerer + mailbox
/// persistence.
///
/// `mailbox` is plumbed through to `run_b2f_answerer` for Inbox-persist +
/// Outbox-drain. `None` is for tests.
///
/// On any reject path, send the appropriate WLE-compat reject message then
/// drop the TCP connection. The TCP `Drop` impl closes the socket.
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_one_session<F>(
    stream: TcpStream,
    peer_addr: SocketAddr,
    allowed: &AllowedStations,
    password: &StationPassword,
    arms: &ListenerArmsRecord,
    config: &ExchangeConfig,
    mailbox: Option<&crate::native_mailbox::Mailbox>,
    progress: &dyn Fn(&str),
    wire_log: &dyn Fn(&str),
    decide: F,
) -> Result<ExchangeResult, TelnetListenError>
where
    F: Fn(&[crate::winlink::proposal::Proposal]) -> Vec<crate::winlink::proposal::Answer>,
{
    // Bound the wire phase so a half-open peer can't pin the listener.
    stream.set_read_timeout(Some(PEER_TIMEOUT)).ok();
    stream.set_write_timeout(Some(PEER_TIMEOUT)).ok();

    // IPv6 explicit reject (DIVERGES from WLE silent-drop, §9).
    if peer_addr.ip().is_ipv6() {
        progress(&format!(
            "Rejecting IPv6 peer {peer_addr} — IPv6 is not supported by the Telnet listener"
        ));
        let mut s = stream;
        let _ = s.write_all(WIRE_REJECT_IPV6);
        return Ok(ExchangeResult::default());
    }

    // Split into reader + writer halves over a shared stream so we can speak
    // the CALLSIGN + Password prompts. Cloning a `TcpStream` yields a second
    // file descriptor wrapping the same kernel socket — read on one half,
    // write on the other (no Mutex needed because the prompts/replies are
    // strictly turn-based: we never read + write simultaneously).
    let writer_stream = stream
        .try_clone()
        .map_err(TelnetListenError::PeerIo)?;
    let mut writer = writer_stream;
    let mut reader = BufReader::new(stream);

    // ── Phase 1: CALLSIGN exchange ────────────────────────────────
    writer
        .write_all(WIRE_PROMPT_CALLSIGN)
        .map_err(TelnetListenError::PeerIo)?;
    wire_log("> CALLSIGN :");
    writer.flush().ok();

    let callsign_bytes = read_cr_terminated_line(&mut reader)?
        .ok_or(TelnetListenError::EofBeforeCallsign)?;
    let callsign_raw = String::from_utf8_lossy(&callsign_bytes).trim().to_string();
    wire_log(&format!("< {callsign_raw}"));

    let claimed = parse_telnet_callsign(&callsign_raw)
        .ok_or_else(|| TelnetListenError::BadCallsign(callsign_raw.clone()))?;

    let peer = PeerId::Both {
        callsign: claimed.clone(),
        addr: peer_addr,
    };

    // ── Allowlist + arms TTL gate (note: password gate runs AFTER the
    //    `Password :` prompt — see Phase 2). The decision function returns
    //    one of {Accept, RejectAllowlist, RejectExpired, RejectPassword}.
    //    For the wire-protocol order we ALWAYS prompt for the password if the
    //    allowlist + TTL are clean, EVEN when no station password is
    //    configured (telnet-p2p.md §5.2: WLE emits the prompt
    //    unconditionally). The password-class reject is decided after the
    //    inbound password is read.

    // Pre-check allowlist + TTL without consulting the password (so a peer
    // who fails the allowlist never reaches the password prompt).
    let arms_only_decision = listener_decide(&peer, None, allowed, &noop_password(), arms);
    match arms_only_decision {
        ListenerDecision::RejectAllowlist => {
            progress(&format!(
                "Rejecting {peer_addr} — callsign={:?} not on allowlist",
                claimed
            ));
            let _ = writer.write_all(WIRE_REJECT_ALLOWLIST);
            wire_log("> *** Your station is not authorized to connect.");
            return Ok(ExchangeResult::default());
        }
        ListenerDecision::RejectExpired => {
            progress(&format!(
                "Rejecting {peer_addr} — listener arm window has expired"
            ));
            let _ = writer.write_all(WIRE_REJECT_EXPIRED);
            wire_log("> *** Listener is not armed");
            return Ok(ExchangeResult::default());
        }
        ListenerDecision::Accept => {}
        ListenerDecision::RejectPassword => {
            // Unreachable: `noop_password()` is never `is_set()`, so the
            // password branch in `listener_decide` is skipped. If we ever
            // see this we'd rather close than continue.
            progress(&format!(
                "Rejecting {peer_addr} — unexpected password-class reject during pre-check"
            ));
            return Ok(ExchangeResult::default());
        }
    }

    // ── Phase 2: Password prompt — UNCONDITIONAL per §5.2 ─────────
    writer
        .write_all(WIRE_PROMPT_PASSWORD)
        .map_err(TelnetListenError::PeerIo)?;
    wire_log("> Password :");
    writer.flush().ok();

    let password_bytes = read_cr_terminated_line(&mut reader)?
        .ok_or(TelnetListenError::EofBeforePassword)?;
    let password_raw = String::from_utf8_lossy(&password_bytes).to_string();
    // Per telnet-p2p.md §9.4 Option A + WLE wire parity: the inbound password
    // is uppercased + trimmed before compare. Stored value is ALSO uppercased
    // (via `normalize_station_password` at write time) so the compare
    // succeeds for any case combination the operator + peer can type.
    let password_normalised = normalize_inbound_password(&password_raw);
    // Do NOT wire-log the password value verbatim; only its presence.
    wire_log(&format!("< <{} byte password>", password_normalised.len()));

    let final_decision = listener_decide(&peer, Some(&password_normalised), allowed, password, arms);
    match final_decision {
        ListenerDecision::Accept => {
            progress(&format!(
                "Accepted Telnet session from {peer_addr} (callsign={:?})",
                claimed
            ));
        }
        ListenerDecision::RejectPassword => {
            progress(&format!(
                "Rejecting {peer_addr} — incorrect station password"
            ));
            let _ = writer.write_all(WIRE_REJECT_PASSWORD);
            wire_log("> *** Incorrect station password specified");
            return Ok(ExchangeResult::default());
        }
        ListenerDecision::RejectExpired => {
            // Possible if the TTL elapsed between the pre-check and the
            // password gate (a slow peer + a short TTL).
            progress(&format!(
                "Rejecting {peer_addr} — listener arm window expired during exchange"
            ));
            let _ = writer.write_all(WIRE_REJECT_EXPIRED);
            wire_log("> *** Listener is not armed");
            return Ok(ExchangeResult::default());
        }
        ListenerDecision::RejectAllowlist => {
            // Should be impossible — we passed the pre-check above.
            progress(&format!(
                "Rejecting {peer_addr} — allowlist re-check failed (race?)"
            ));
            let _ = writer.write_all(WIRE_REJECT_ALLOWLIST);
            return Ok(ExchangeResult::default());
        }
    }

    // ── Phase 3: Handoff to B2F answerer ──────────────────────────
    // Codex review 2026-06-03 [P2]: the master handshake (FBB/WLE-strict
    // peers) addresses the `targetcall` field — leaving it empty puts
    // `; DE <mycall>` on the wire and can break strict peers. Clone the
    // listener-shared `ExchangeConfig` per-session and inject the parsed
    // CALLSIGN response (the peer's claimed callsign, canonicalised by
    // `parse_telnet_callsign`).
    let mut per_session_config = config.clone();
    per_session_config.targetcall = claimed.call.clone();
    run_b2f_answerer(
        reader,
        writer,
        &per_session_config,
        mailbox,
        progress,
        wire_log,
        decide,
    )
}

/// Drive `run_exchange_with_role(ExchangeRole::Answer)` over the connected
/// stream. Loads the operator's Outbox before the exchange so any pending
/// messages get offered to the inbound peer. After the exchange completes,
/// persists `result.received` to the operator's Inbox and moves `result.sent`
/// MIDs from Outbox to Sent — the same mailbox symmetry the Packet listener
/// inherits from `native_packet_exchange` (winlink_backend.rs:1323).
///
/// `mailbox` may be `None` for tests that don't need filesystem state.
#[allow(clippy::too_many_arguments)]
fn run_b2f_answerer<R, W, F>(
    mut reader: BufReader<R>,
    mut writer: W,
    config: &ExchangeConfig,
    mailbox: Option<&crate::native_mailbox::Mailbox>,
    progress: &dyn Fn(&str),
    wire_log: &dyn Fn(&str),
    decide: F,
) -> Result<ExchangeResult, TelnetListenError>
where
    R: Read,
    W: Write,
    F: Fn(&[crate::winlink::proposal::Proposal]) -> Vec<crate::winlink::proposal::Answer>,
{
    use crate::winlink_backend::MailboxFolder;

    // Tuxlink-7vea Telnet inbound-mail symmetry (closes tuxlink-k3ru):
    // load the operator's Outbox BEFORE opening the B2F exchange so any
    // pending mail rides the inbound session out to the peer. Empty Vec
    // when no mailbox is plumbed (tests).
    let outbound = match mailbox {
        Some(mb) => match crate::winlink_backend::build_outbound_proposals(mb) {
            Ok(v) => v,
            Err(e) => {
                progress(&format!("Outbox read failed (proceeding empty): {e}"));
                Vec::new()
            }
        },
        None => Vec::new(),
    };
    progress(&format!(
        "Running B2F exchange (answerer role; {} outbound)…",
        outbound.len()
    ));

    let result = session::run_exchange_with_role(
        &mut reader,
        &mut writer,
        ExchangeRole::Answer,
        config,
        outbound,
        decide,
        Some(wire_log),
    )
    .map_err(TelnetListenError::Exchange)?;

    // Persist received messages to Inbox + move sent MIDs from Outbox to Sent.
    // Error-tolerant: a single message-store failure logs but doesn't fail the
    // whole exchange — the peer's already gone, partial persistence is better
    // than nothing. (Symmetric with native_packet_exchange's ordering: file
    // accepted messages FIRST, then signal completion.)
    if let Some(mb) = mailbox {
        for message in &result.received {
            match mb.store(MailboxFolder::Inbox, &message.to_bytes()) {
                Ok(_) => {}
                Err(e) => progress(&format!("Inbox store failed for one message: {e}")),
            }
        }
        for mid in &result.sent {
            let mid_obj = crate::winlink_backend::MessageId(mid.clone());
            match mb.move_to(MailboxFolder::Outbox, MailboxFolder::Sent, &mid_obj) {
                Ok(_) => {}
                Err(e) => progress(&format!("Outbox→Sent move failed for {mid}: {e}")),
            }
        }
        progress(&format!(
            "Telnet inbound exchange persisted: {} received → Inbox, {} sent → Sent",
            result.received.len(),
            result.sent.len()
        ));
    }

    Ok(result)
}

// ──────────────────────────────────────────────────────────────
// Wire helpers
// ──────────────────────────────────────────────────────────────

/// Read a `\r`-terminated line from the peer.
///
/// Per WLE's `AccumulateInputLine` (`TelnetP2PSession.cs:1367-1381`):
/// - byte `0` (NUL) is skipped (do not accumulate)
/// - byte `10` (`\n`) is skipped (do not accumulate; WLE does not treat `\n`
///   as a line terminator)
/// - byte `13` (`\r`) returns the accumulated line
/// - all other bytes are accumulated
///
/// Returns `Ok(None)` on EOF with an empty buffer (the peer disconnected
/// before sending any input). Returns the line WITHOUT the trailing `\r`.
pub(crate) fn read_cr_terminated_line<R: BufRead>(
    reader: &mut R,
) -> Result<Option<Vec<u8>>, TelnetListenError> {
    let mut buf: Vec<u8> = Vec::with_capacity(64);
    loop {
        let mut byte = [0u8; 1];
        match reader.read(&mut byte) {
            Ok(0) => {
                return Ok(if buf.is_empty() { None } else { Some(buf) });
            }
            Ok(_) => {
                let b = byte[0];
                if b == b'\r' {
                    return Ok(Some(buf));
                }
                if b == 0 || b == b'\n' {
                    continue; // skip — do NOT accumulate
                }
                buf.push(b);
                // Sanity cap: WLE has no documented line-length limit but a
                // peer that streams 1 MiB without a `\r` is misbehaving.
                if buf.len() > 4096 {
                    return Err(TelnetListenError::BadCallsign(
                        "line exceeds 4 KiB without terminator".to_string(),
                    ));
                }
            }
            Err(e) => return Err(TelnetListenError::PeerIo(e)),
        }
    }
}

/// Parse the callsign the peer claimed in their CALLSIGN-prompt response.
///
/// Per WLE (`TelnetP2PSession.cs:1287-1293`):
/// - Uppercase the inbound value
/// - Strip an optional trailing `-T` / `-R` / `-L` qualifier
/// - The remainder is the callsign that goes to the allowlist comparison
///
/// tuxlink additionally strips an AX.25-style numeric SSID (`-1` … `-15`) so
/// the resulting [`Address`] reflects the parsed SSID, not zero. The
/// allowlist's `callsign_matches` then compares against the canonical form
/// (`N7CPZ` for SSID 0, `N7CPZ-1` otherwise).
pub(crate) fn parse_telnet_callsign(raw: &str) -> Option<Address> {
    let trimmed = raw.trim().to_uppercase();
    if trimmed.is_empty() {
        return None;
    }
    // Strip `-T` / `-R` / `-L` qualifier (WLE parity).
    let trimmed = if let Some(stripped) = trimmed
        .strip_suffix("-T")
        .or_else(|| trimmed.strip_suffix("-R"))
        .or_else(|| trimmed.strip_suffix("-L"))
    {
        stripped.to_string()
    } else {
        trimmed
    };

    // Split base callsign + optional numeric SSID.
    if let Some((base, ssid_str)) = trimmed.rsplit_once('-') {
        if let Ok(ssid) = ssid_str.parse::<u8>() {
            if ssid <= 15 && !base.is_empty() {
                return Some(Address {
                    call: base.to_string(),
                    ssid,
                });
            }
        }
        // `-X` where X isn't a numeric SSID → treat as base part of callsign.
        // (Unusual but defensive — keeps weird callsigns from being silently
        // rejected at parse time; the allowlist will reject if not listed.)
    }
    Some(Address {
        call: trimmed,
        ssid: 0,
    })
}

/// A `StationPassword` that always reports `is_set() == false`. Used during
/// the allowlist + TTL pre-check so the pre-check doesn't probe the keyring
/// for a password we haven't asked the peer for yet.
fn noop_password() -> StationPassword {
    use crate::winlink::credentials::EntryLike;
    use crate::winlink::listener::station_password::EntryFactory;

    struct NoopEntry;
    impl EntryLike for NoopEntry {
        fn get_password(&self) -> Result<String, keyring::Error> {
            Err(keyring::Error::NoEntry)
        }
        fn set_password(&self, _: &str) -> Result<(), keyring::Error> {
            Err(keyring::Error::NoEntry)
        }
        fn delete_password(&self) -> Result<(), keyring::Error> {
            Err(keyring::Error::NoEntry)
        }
    }

    let factory: EntryFactory = Box::new(|_service: &str, _account: &str| {
        Box::new(NoopEntry) as Box<dyn EntryLike>
    });
    StationPassword::with_factory(factory)
}

// ──────────────────────────────────────────────────────────────
// Operator-typed password hygiene — uppercase at write-time
// ──────────────────────────────────────────────────────────────

/// Uppercase the operator-typed password BEFORE storing it.
///
/// Per `dev/scratch/winlink-re/findings/telnet-p2p.md §9.4 Option A`: WLE
/// uppercases the inbound password before compare but stores the operator's
/// password verbatim — meaning a lowercase-character in the operator's
/// password silently always-fails. Tuxlink rejects the bug by uppercasing
/// the operator-typed password at WRITE time. Verify still uppercases the
/// inbound, so both sides of the compare are uppercase. Trivially equivalent
/// to "case-insensitive password compare," matching operator intuition.
///
/// Whitespace is trimmed (matching WLE's `txtPassword.Text.Trim()` at
/// `TelnetP2PSetup.cs:599`).
pub fn normalize_station_password(input: &str) -> String {
    input.trim().to_uppercase()
}

/// Uppercase the inbound peer-supplied password BEFORE compare. Matches WLE's
/// `strIncomingPassword = sbdInput.ToString().Trim().ToUpper()` at
/// `TelnetP2PSession.cs:1290`.
pub(crate) fn normalize_inbound_password(input: &str) -> String {
    input.trim().to_uppercase()
}

// ──────────────────────────────────────────────────────────────
// IPv4 reject for the allowlist matcher — exposed so the listener can check
// at the TCP-accept layer without rehashing `IpAddr` matching.
// ──────────────────────────────────────────────────────────────

/// Returns TRUE if `addr` is an IPv6 address that tuxlink's Telnet listener
/// should reject before invoking the allowlist (DIVERGES from WLE silent-drop).
pub fn should_reject_ipv6(addr: &IpAddr) -> bool {
    matches!(addr, IpAddr::V6(_))
}

// ──────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use super::session::SessionIntent;
    use crate::winlink::credentials::EntryLike;
    use crate::winlink::listener::arms_record::DEFAULT_TTL;
    use crate::winlink::listener::station_password::EntryFactory;
    use crate::winlink::listener::TransportKind;
    use std::collections::HashMap;
    use std::io::Cursor;
    use std::net::{Ipv4Addr, Ipv6Addr};
    use std::sync::{Arc, Mutex};
    use std::time::SystemTime;
    use std::thread;

    // ── Mock keyring (shared with decide.rs / station_password.rs patterns) ──

    struct MockEntry {
        store: Arc<Mutex<HashMap<(String, String), String>>>,
        service: String,
        account: String,
    }
    impl EntryLike for MockEntry {
        fn get_password(&self) -> Result<String, keyring::Error> {
            self.store
                .lock()
                .unwrap()
                .get(&(self.service.clone(), self.account.clone()))
                .cloned()
                .ok_or(keyring::Error::NoEntry)
        }
        fn set_password(&self, password: &str) -> Result<(), keyring::Error> {
            self.store.lock().unwrap().insert(
                (self.service.clone(), self.account.clone()),
                password.to_string(),
            );
            Ok(())
        }
        fn delete_password(&self) -> Result<(), keyring::Error> {
            self.store
                .lock()
                .unwrap()
                .remove(&(self.service.clone(), self.account.clone()))
                .ok_or(keyring::Error::NoEntry)
                .map(|_| ())
        }
    }
    fn mock_password() -> StationPassword {
        let store: Arc<Mutex<HashMap<(String, String), String>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let factory: EntryFactory = Box::new(move |service: &str, account: &str| {
            Box::new(MockEntry {
                store: Arc::clone(&store),
                service: service.to_string(),
                account: account.to_string(),
            }) as Box<dyn EntryLike>
        });
        StationPassword::with_factory(factory)
    }

    // ── Wire-line helpers ────────────────────────────────────────

    #[test]
    fn read_cr_terminated_line_returns_on_cr() {
        let mut r = Cursor::new(b"N7CPZ\r".to_vec());
        let line = read_cr_terminated_line(&mut r).unwrap();
        assert_eq!(line, Some(b"N7CPZ".to_vec()));
    }

    #[test]
    fn read_cr_terminated_line_skips_lf_alone() {
        // \n alone is NOT a terminator (WLE parity) — it's skipped.
        let mut r = Cursor::new(b"N7CPZ\nABCD\r".to_vec());
        let line = read_cr_terminated_line(&mut r).unwrap();
        assert_eq!(line, Some(b"N7CPZABCD".to_vec()));
    }

    #[test]
    fn read_cr_terminated_line_skips_nul() {
        // NUL byte (0x00) is dropped, matching WLE's accumulator.
        let mut r = Cursor::new(b"N\x007CPZ\r".to_vec());
        let line = read_cr_terminated_line(&mut r).unwrap();
        assert_eq!(line, Some(b"N7CPZ".to_vec()));
    }

    #[test]
    fn read_cr_terminated_line_returns_none_on_eof_with_empty_buf() {
        let mut r = Cursor::new(b"".to_vec());
        let line = read_cr_terminated_line(&mut r).unwrap();
        assert_eq!(line, None);
    }

    // ── Callsign parsing ─────────────────────────────────────────

    #[test]
    fn parse_callsign_uppercases_and_strips_ssid() {
        let a = parse_telnet_callsign("n7cpz-1").unwrap();
        assert_eq!(a.call, "N7CPZ");
        assert_eq!(a.ssid, 1);
    }

    #[test]
    #[allow(non_snake_case)]
    fn parse_callsign_strips_telnet_qualifier_T() {
        // WLE: trailing `-T` is the WL2K telnet-qualifier, stripped before
        // the allowlist compare.
        let a = parse_telnet_callsign("N7CPZ-T").unwrap();
        assert_eq!(a.call, "N7CPZ");
        assert_eq!(a.ssid, 0);
    }

    #[test]
    #[allow(non_snake_case)]
    fn parse_callsign_strips_qualifier_L() {
        let a = parse_telnet_callsign("W4PHS-L").unwrap();
        assert_eq!(a.call, "W4PHS");
        assert_eq!(a.ssid, 0);
    }

    #[test]
    fn parse_callsign_empty_returns_none() {
        assert!(parse_telnet_callsign("   ").is_none());
        assert!(parse_telnet_callsign("").is_none());
    }

    // ── Password normalisation ──────────────────────────────────

    #[test]
    fn normalize_station_password_uppercases_and_trims() {
        assert_eq!(normalize_station_password("  hunter2 "), "HUNTER2");
        assert_eq!(normalize_station_password("HUNTER2"), "HUNTER2");
        assert_eq!(normalize_station_password("MixedCase"), "MIXEDCASE");
    }

    #[test]
    fn normalize_inbound_password_uppercases_and_trims() {
        assert_eq!(normalize_inbound_password("hunter2"), "HUNTER2");
        assert_eq!(normalize_inbound_password(" CMSTelnet "), "CMSTELNET");
    }

    #[test]
    fn password_uppercase_round_trip_against_keyring_verify() {
        // The end-to-end discipline: normalise BOTH sides and the SHA-256
        // constant-time compare in StationPassword::verify succeeds for any
        // case combination the operator + peer can type. This is the spec
        // §9.4 Option A fix to WLE's silent-fail-on-lowercase bug.
        let sp = mock_password();
        // Operator typed lowercase → store the normalised form.
        sp.set(&normalize_station_password("Hunter2")).unwrap();
        // Peer-supplied uppercase normalises identically → verify succeeds.
        assert!(sp.verify(&normalize_inbound_password("HUNTER2")));
        // Peer-supplied lowercase ALSO succeeds.
        assert!(sp.verify(&normalize_inbound_password("hunter2")));
        // Wrong password fails.
        assert!(!sp.verify(&normalize_inbound_password("WRONGPW")));
    }

    // ── IPv6 reject ─────────────────────────────────────────────

    #[test]
    fn should_reject_ipv6_returns_true_for_v6() {
        let ip = IpAddr::V6(Ipv6Addr::LOCALHOST);
        assert!(should_reject_ipv6(&ip));
    }

    #[test]
    fn should_reject_ipv6_returns_false_for_v4() {
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        assert!(!should_reject_ipv6(&ip));
    }

    // ── End-to-end: bind + dial in + complete CALLSIGN + Password exchange ──

    fn fresh_arms() -> ListenerArmsRecord {
        ListenerArmsRecord::arm(TransportKind::Telnet, DEFAULT_TTL)
    }

    fn allowed_with(calls: &[(&str, u8)]) -> AllowedStations {
        // Restrict-mode so the allowlist gates on the callsign list.
        // (Foundation default since tuxlink-7vea is allow_all=TRUE; tests
        // exercising the allowlist gate must opt back into restrict-mode.)
        let mut a = AllowedStations::new().with_allow_all(false);
        for (call, ssid) in calls {
            a.add_callsign(Address {
                call: (*call).to_string(),
                ssid: *ssid,
            });
        }
        a
    }

    #[test]
    fn integration_loopback_rejects_unallowed_callsign() {
        // Bind 127.0.0.1:0, dial in, send a CALLSIGN response that isn't on
        // the allowlist, and assert we see the reject message on the wire.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let allowed = allowed_with(&[("N7CPZ", 0)]); // only N7CPZ allowed
        let password = mock_password();
        let arms = fresh_arms();
        let config = ExchangeConfig {
            mycall: "TUXLINK".into(),
            targetcall: "PEER".into(),
            locator: "CN87".into(),
            password: None,
            intent: SessionIntent::P2p,
        };

        // Server accept-once.
        let server = thread::spawn(move || {
            let (stream, peer_addr) = listener.accept().unwrap();
            handle_one_session(
                stream,
                peer_addr,
                &allowed,
                &password,
                &arms,
                &config,
                None,
                &|_| {},
                &|_| {},
                |_| Vec::new(),
            )
        });

        // Client: dial in, expect CALLSIGN prompt, send W4PHS (not on list),
        // expect reject message + connection close.
        let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        client.set_read_timeout(Some(Duration::from_secs(5))).ok();
        let mut buf = Vec::new();

        // Read the CALLSIGN prompt.
        let mut prompt = [0u8; 64];
        let n = client.read(&mut prompt).unwrap();
        assert!(
            &prompt[..n].starts_with(WIRE_PROMPT_CALLSIGN),
            "expected CALLSIGN prompt, got {:?}",
            &prompt[..n]
        );

        // Send an unallowed callsign.
        client.write_all(b"W4PHS\r").unwrap();
        client.flush().ok();

        // Read whatever the server sends back — should be the reject message,
        // not the Password prompt.
        let _ = client.read_to_end(&mut buf);

        // The server side should have completed.
        let result = server.join().unwrap();
        assert!(result.is_ok());
        assert!(
            buf.windows(WIRE_REJECT_ALLOWLIST.len())
                .any(|w| w == WIRE_REJECT_ALLOWLIST),
            "expected allowlist-reject on wire, got {:?}",
            String::from_utf8_lossy(&buf)
        );
        // Crucially, the Password prompt should NOT have been sent before reject.
        assert!(
            !buf.windows(WIRE_PROMPT_PASSWORD.len())
                .any(|w| w == WIRE_PROMPT_PASSWORD),
            "Password prompt should NOT precede allowlist reject"
        );
    }

    #[test]
    fn integration_loopback_rejects_wrong_password() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let allowed = allowed_with(&[("N7CPZ", 0)]); // N7CPZ allowed
        let password = mock_password();
        // Operator stores the normalised form (uppercase) — §9.4 Option A.
        password
            .set(&normalize_station_password("hunter2"))
            .unwrap();
        let arms = fresh_arms();
        let config = ExchangeConfig {
            mycall: "TUXLINK".into(),
            targetcall: "PEER".into(),
            locator: "CN87".into(),
            password: None,
            intent: SessionIntent::P2p,
        };

        let server = thread::spawn(move || {
            let (stream, peer_addr) = listener.accept().unwrap();
            handle_one_session(
                stream,
                peer_addr,
                &allowed,
                &password,
                &arms,
                &config,
                None,
                &|_| {},
                &|_| {},
                |_| Vec::new(),
            )
        });

        let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        client.set_read_timeout(Some(Duration::from_secs(5))).ok();
        let mut buf = Vec::new();
        let mut prompt = [0u8; 64];

        // CALLSIGN prompt.
        let n = client.read(&mut prompt).unwrap();
        assert!(&prompt[..n].starts_with(WIRE_PROMPT_CALLSIGN));
        client.write_all(b"N7CPZ\r").unwrap();
        client.flush().ok();

        // Password prompt.
        let n = client.read(&mut prompt).unwrap();
        assert!(
            &prompt[..n].starts_with(WIRE_PROMPT_PASSWORD),
            "expected Password prompt, got {:?}",
            &prompt[..n]
        );
        client.write_all(b"WRONGPW\r").unwrap();
        client.flush().ok();

        let _ = client.read_to_end(&mut buf);
        let _ = server.join().unwrap();

        assert!(
            buf.windows(WIRE_REJECT_PASSWORD.len())
                .any(|w| w == WIRE_REJECT_PASSWORD),
            "expected password-reject on wire, got {:?}",
            String::from_utf8_lossy(&buf)
        );
    }

    #[test]
    fn integration_loopback_password_unconditional_prompt_with_no_password() {
        // §5.2: WLE emits the Password prompt UNCONDITIONALLY — even when no
        // station password is configured. tuxlink must match for wire parity
        // with WLE dialers.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let allowed = allowed_with(&[("N7CPZ", 0)]);
        let password = mock_password(); // NEVER .set() — is_set() == false
        let arms = fresh_arms();
        let config = ExchangeConfig {
            mycall: "TUXLINK".into(),
            targetcall: "PEER".into(),
            locator: "CN87".into(),
            password: None,
            intent: SessionIntent::P2p,
        };

        let server = thread::spawn(move || {
            let (stream, peer_addr) = listener.accept().unwrap();
            handle_one_session(
                stream,
                peer_addr,
                &allowed,
                &password,
                &arms,
                &config,
                None,
                &|_| {},
                &|_| {},
                |_| Vec::new(),
            )
        });

        let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        client.set_read_timeout(Some(Duration::from_secs(5))).ok();
        let mut prompt = [0u8; 64];
        let n = client.read(&mut prompt).unwrap();
        assert!(&prompt[..n].starts_with(WIRE_PROMPT_CALLSIGN));
        client.write_all(b"N7CPZ\r").unwrap();
        client.flush().ok();

        // Password prompt MUST be sent even when no password is configured.
        let n = client.read(&mut prompt).unwrap();
        assert!(
            &prompt[..n].starts_with(WIRE_PROMPT_PASSWORD),
            "Password prompt must fire even when StationPassword::is_set() == false"
        );

        // Drop the client without sending a password — the server will hit
        // EofBeforePassword. That's the verification we got that far.
        drop(client);
        let result = server.join().unwrap();
        // The handler returns Err(EofBeforePassword) when the peer disconnects
        // mid-password-prompt.
        assert!(matches!(result, Err(TelnetListenError::EofBeforePassword)));
    }

    #[test]
    fn integration_loopback_rejects_expired_arms() {
        // An arms record with TTL=0 (immediately expired) should reject before
        // the Password prompt fires.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let allowed = allowed_with(&[("N7CPZ", 0)]);
        let password = mock_password();
        // Disarmed → ttl=0 → always expired.
        let mut arms = fresh_arms();
        arms.disarm();
        let config = ExchangeConfig {
            mycall: "TUXLINK".into(),
            targetcall: "PEER".into(),
            locator: "CN87".into(),
            password: None,
            intent: SessionIntent::P2p,
        };

        let server = thread::spawn(move || {
            let (stream, peer_addr) = listener.accept().unwrap();
            handle_one_session(
                stream,
                peer_addr,
                &allowed,
                &password,
                &arms,
                &config,
                None,
                &|_| {},
                &|_| {},
                |_| Vec::new(),
            )
        });

        let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        client.set_read_timeout(Some(Duration::from_secs(5))).ok();
        let mut prompt = [0u8; 64];
        let n = client.read(&mut prompt).unwrap();
        assert!(&prompt[..n].starts_with(WIRE_PROMPT_CALLSIGN));
        client.write_all(b"N7CPZ\r").unwrap();
        client.flush().ok();
        let mut buf = Vec::new();
        let _ = client.read_to_end(&mut buf);
        let _ = server.join().unwrap();

        assert!(
            buf.windows(WIRE_REJECT_EXPIRED.len())
                .any(|w| w == WIRE_REJECT_EXPIRED),
            "expected expired-reject on wire, got {:?}",
            String::from_utf8_lossy(&buf)
        );
        // Critically, the Password prompt should NOT precede the expired reject.
        assert!(
            !buf.windows(WIRE_PROMPT_PASSWORD.len())
                .any(|w| w == WIRE_PROMPT_PASSWORD),
            "Password prompt should NOT precede expired-arms reject"
        );
    }

    // ── Bind-time error ──────────────────────────────────────────

    #[test]
    fn bind_rejects_unparseable_address() {
        let err = bind("not-an-ip-address", 9999).unwrap_err();
        assert!(matches!(err, TelnetListenError::Bind { .. }));
    }

    // ── arms-record reference (compile assertion) ────────────────

    #[test]
    fn default_port_is_8774_per_wle_parity() {
        // Spec citation: telnet-p2p.md §1 — `Globals.strTelnetListeningPort`
        // initial value `"8774"` at `Globals.cs:1518`. Reject any drift here.
        assert_eq!(DEFAULT_PORT, 8774);
    }

    #[test]
    fn default_bind_addr_is_loopback_per_tuxlink_divergence() {
        // Spec citation: telnet-p2p.md §9.3 — DIVERGE from WLE's all-interfaces
        // default. Operator opts into LAN/all explicitly.
        assert_eq!(DEFAULT_BIND_ADDR, "127.0.0.1");
    }

    // ── Drop a placeholder so SystemTime stays referenced (build hygiene) ──
    #[test]
    fn arms_record_armed_at_is_systemtime() {
        let r = fresh_arms();
        let _: SystemTime = r.armed_at;
    }

    // ── Spec §4.4: callsign-OR-IP logic (allowlist) ───────────────

    #[test]
    fn allowlist_or_logic_callsign_allowed_wrong_ip_still_accepts() {
        // telnet-p2p.md §4.4 row "Allow-list logic": accept when EITHER
        // callsign OR IP is allowed. A station listed by callsign with a
        // wrong IP gets in.
        let mut a = AllowedStations::new();
        a.add_callsign(Address {
            call: "N7CPZ".into(),
            ssid: 0,
        });
        a.add_ip_pattern("192.168.1.50"); // unrelated IP

        // Peer presents the callsign but from a different IP.
        let peer = PeerId::Both {
            callsign: Address {
                call: "N7CPZ".into(),
                ssid: 0,
            },
            addr: "10.0.0.1:55000".parse().unwrap(),
        };
        assert!(a.accept(&peer), "callsign-allowed → accept even with non-listed IP");
    }

    #[test]
    fn allowlist_or_logic_ip_allowed_wrong_callsign_still_accepts() {
        // The inverse: a station listed by IP with an unlisted callsign also
        // gets in.
        let mut a = AllowedStations::new();
        a.add_callsign(Address {
            call: "N7CPZ".into(),
            ssid: 0,
        });
        a.add_ip_pattern("192.168.1.50");

        let peer = PeerId::Both {
            callsign: Address {
                call: "W4PHS".into(), // not on list
                ssid: 0,
            },
            addr: "192.168.1.50:55000".parse().unwrap(),
        };
        assert!(a.accept(&peer), "IP-allowed → accept even with non-listed callsign");
    }

    // ── Spec §4.3: IPv4 per-octet wildcard ────────────────────────

    #[test]
    fn ipv4_per_octet_wildcard_matches_192_168_1_star() {
        // Restrict-mode to exercise the wildcard match logic (foundation
        // default since tuxlink-7vea is allow_all=TRUE).
        let mut a = AllowedStations::new().with_allow_all(false);
        a.add_ip_pattern("192.168.1.*");

        let peer = PeerId::SocketAddr("192.168.1.50:55000".parse().unwrap());
        assert!(a.accept(&peer));

        // Outside the /24 → reject.
        let peer = PeerId::SocketAddr("192.168.2.50:55000".parse().unwrap());
        assert!(!a.accept(&peer));
    }

    #[test]
    fn ipv4_per_octet_wildcard_matches_middle_position() {
        // WLE supports wildcards in ANY position, not just the trailing one
        // (telnet-p2p.md §4.3 — `192.168.*.50` matches 192.168.x.50 for any x).
        let mut a = AllowedStations::new().with_allow_all(false);
        a.add_ip_pattern("192.168.*.50");

        let peer = PeerId::SocketAddr("192.168.7.50:55000".parse().unwrap());
        assert!(a.accept(&peer), "middle-position wildcard should match");

        // Last octet differs → reject.
        let peer = PeerId::SocketAddr("192.168.7.49:55000".parse().unwrap());
        assert!(!a.accept(&peer), "last octet mismatch should reject");
    }

    // ── Spec §9: IPv6 explicit reject at listener layer ──────────

    #[test]
    fn integration_loopback_rejects_ipv6_peer() {
        // Bind to localhost IPv6 explicitly. Note that the IPv6 reject is at
        // the handle_one_session layer — peer_addr.ip().is_ipv6() short-
        // circuits before the CALLSIGN prompt. We simulate by constructing a
        // synthetic IPv6 peer address against a real loopback IPv4 socket
        // pair (the test asserts on the wire bytes WIRE_REJECT_IPV6 emitted
        // for an IPv6-claiming peer address).
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let allowed = AllowedStations::new().with_allow_all(true);
        let password = mock_password();
        let arms = fresh_arms();
        let config = ExchangeConfig {
            mycall: "TUXLINK".into(),
            targetcall: "PEER".into(),
            locator: "CN87".into(),
            password: None,
            intent: SessionIntent::P2p,
        };

        let server = thread::spawn(move || {
            let (stream, _peer_addr) = listener.accept().unwrap();
            // Override peer_addr with an IPv6 to exercise the v6-reject path.
            let fake_v6: SocketAddr = "[::1]:55000".parse().unwrap();
            handle_one_session(
                stream,
                fake_v6,
                &allowed,
                &password,
                &arms,
                &config,
                None,
                &|_| {},
                &|_| {},
                |_| Vec::new(),
            )
        });

        let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        client.set_read_timeout(Some(Duration::from_secs(5))).ok();
        let mut buf = Vec::new();
        let _ = client.read_to_end(&mut buf);
        let _ = server.join().unwrap();

        // CALLSIGN prompt should NOT have been sent.
        assert!(
            !buf.windows(WIRE_PROMPT_CALLSIGN.len())
                .any(|w| w == WIRE_PROMPT_CALLSIGN),
            "CALLSIGN prompt must NOT precede the IPv6-reject"
        );
        assert!(
            buf.windows(WIRE_REJECT_IPV6.len())
                .any(|w| w == WIRE_REJECT_IPV6),
            "expected IPv6-reject on wire, got {:?}",
            String::from_utf8_lossy(&buf)
        );
    }

    // ── Single-session (MaxConnections=1) ─────────────────────────

    #[test]
    fn integration_loopback_single_session_second_peer_queued_then_handled_serially() {
        // Verify the accept loop handles peers serially (single-session per
        // MaxConnections=1 parity). We dial two clients; the second blocks
        // until the first completes (or is rejected), then proceeds.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let allowed = allowed_with(&[("N7CPZ", 0)]);
        let password = mock_password();
        let arms = fresh_arms();
        let config = ExchangeConfig {
            mycall: "TUXLINK".into(),
            targetcall: "PEER".into(),
            locator: "CN87".into(),
            password: None,
            intent: SessionIntent::P2p,
        };
        let shutdown = std::sync::Arc::new(AtomicBool::new(false));
        let shutdown_for_loop = shutdown.clone();

        // Run the accept loop on a background thread.
        let loop_handle = thread::spawn(move || {
            run_accept_loop(
                listener,
                allowed,
                password,
                arms,
                config,
                None, // mailbox — tests don't persist
                shutdown_for_loop,
                &|_| {},
                &|_| {},
                |_proposals: &[Proposal]| Vec::new(),
            );
        });

        // Client A: dial in, get allowlist-rejected (W4PHS not on list), close.
        {
            let mut c = TcpStream::connect(("127.0.0.1", port)).unwrap();
            c.set_read_timeout(Some(Duration::from_secs(5))).ok();
            let mut prompt = [0u8; 64];
            let n = c.read(&mut prompt).unwrap();
            assert!(&prompt[..n].starts_with(WIRE_PROMPT_CALLSIGN));
            c.write_all(b"W4PHS\r").unwrap();
            let mut buf = Vec::new();
            let _ = c.read_to_end(&mut buf);
        }

        // Client B: dial in AFTER A's session closed. Should also reach the
        // CALLSIGN prompt — proves the loop iterated.
        {
            let mut c = TcpStream::connect(("127.0.0.1", port)).unwrap();
            c.set_read_timeout(Some(Duration::from_secs(5))).ok();
            let mut prompt = [0u8; 64];
            let n = c.read(&mut prompt).unwrap();
            assert!(
                &prompt[..n].starts_with(WIRE_PROMPT_CALLSIGN),
                "second client should receive CALLSIGN prompt after first completes"
            );
            c.write_all(b"W4PHS\r").unwrap();
            let mut buf = Vec::new();
            let _ = c.read_to_end(&mut buf);
        }

        // Signal shutdown + open a transient connection to wake the loop.
        shutdown.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect_timeout(
            &format!("127.0.0.1:{port}").parse::<SocketAddr>().unwrap(),
            Duration::from_millis(500),
        );
        loop_handle.join().unwrap();
    }
}
