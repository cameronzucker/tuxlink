//! ARDOP cmd-socket session layer: `CmdSocket` + `init_tnc`.
//!
//! Opens and owns the ARDOP command TCP socket (default 8515), drives the
//! documented init sequence (per wl2k-go `transport/ardop/tnc.go::init()`),
//! and delivers parsed TNC events to the caller via an `mpsc` channel.
//!
//! **Concurrency model:** synchronous `std::net::TcpStream` + `std::thread`
//! (see [ADR 0015] and the plan's CONCURRENCY ARCHITECTURE note). No Tokio.
//! A single control-loop thread reads `\r`-terminated lines from a `BufReader`,
//! parses each into a [`Command`], and sends it into an `mpsc::Sender`. The
//! `CmdSocket` holds the receiver and the write half. Channel close on EOF so
//! `recv_event` surfaces a disconnect as `RecvError::Disconnected`.
//!
//! The pattern mirrors `src-tauri/src/winlink/telnet.rs`'s `TcpStream`
//! `try_clone` / reader + writer split.

use std::io::{self, BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use super::arq_state::ArqState;
use super::command::{encode_setter, Command, CommandParseError, State};
use super::wire::encode_cmd_line;
use crate::winlink_backend::WireSink;

/// Bound on a single cmd-socket write — pairs with the per-setter ack timeout so
/// a wedged TNC can't block init indefinitely. (Code review Phase 2a.)
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);

// ─── CmdSocket ─────────────────────────────────────────────────────────────

/// An active ARDOP command-socket connection.
///
/// Owns the write half of the TCP stream and an `mpsc` channel whose sender
/// lives on the control-loop reader thread. Send lines with [`send_line`]; pull
/// parsed TNC events with [`recv_event`].
pub struct CmdSocket {
    writer: TcpStream,
    rx: mpsc::Receiver<Command>,
    /// Control-loop reader thread; joined on drop after the socket is shut down.
    reader_thread: Option<thread::JoinHandle<()>>,
    /// Optional raw-wire tap (tuxlink-ngsk). Used by [`send_line`] to log
    /// outbound cmd-port lines; the reader thread holds its own clone for the
    /// inbound side.
    wire: Option<WireSink>,
}

impl CmdSocket {
    /// Open the ARDOP cmd socket at `addr` and start the reader thread.
    ///
    /// On success returns a `CmdSocket` whose control-loop thread is already
    /// running and forwarding parsed `Command` values into the internal channel.
    pub fn connect(addr: SocketAddr) -> io::Result<Self> {
        Self::connect_with_arq_state(addr, None)
    }

    /// Like [`connect`] but also wire an [`ArqState`] that the reader thread
    /// updates on `CONNECTED` / `DISCONNECTED` / `NEWSTATE DISC` events
    /// (tuxlink-ytg). The data socket reads the same `ArqState` to decide
    /// EOF-on-DISC and pre-ARQ frame-drop. `None` skips the bookkeeping for
    /// callers that don't share state with a data socket (the existing
    /// connect tests).
    pub fn connect_with_arq_state(
        addr: SocketAddr,
        arq_state: Option<ArqState>,
    ) -> io::Result<Self> {
        Self::connect_with_arq_state_and_wire(addr, arq_state, None)
    }

    /// Like [`connect_with_arq_state`] but also installs a raw-wire tap
    /// (tuxlink-ngsk). Every inbound cmd-port line read from ardopcf — including
    /// `REJ` / `NEWSTATE` / `FAULT` and any line that does NOT parse to a known
    /// [`Command`] — is handed to `wire` BEFORE parsing, so the session log
    /// captures the verbatim cmd-port transcript. This is the alpha
    /// troubleshooting surface: uploaded logs are the source of truth, so the
    /// raw frames must be in them. Outbound lines are tapped in [`send_line`].
    /// `None` skips the tap (the existing tests + the no-wire `connect`).
    pub fn connect_with_arq_state_and_wire(
        addr: SocketAddr,
        arq_state: Option<ArqState>,
        wire: Option<WireSink>,
    ) -> io::Result<Self> {
        let stream = TcpStream::connect(addr)?;
        // The reader and writer halves share the underlying socket; `try_clone`
        // gives us two independently-closeable handles — the same split pattern
        // used in `telnet.rs`'s `ReadHalf`/`WriteHalf`.
        let reader_stream = stream.try_clone()?;
        let writer = stream;
        // Bound a single write so TCP flow-control backpressure from a wedged TNC
        // can't block init forever (pairs with the per-setter ack timeout).
        writer.set_write_timeout(Some(WRITE_TIMEOUT))?;

        let (tx, rx) = mpsc::channel::<Command>();

        // tuxlink-ngsk: the reader thread holds its own clone of the wire tap so
        // it can log inbound cmd-port lines; the struct keeps the original for
        // the outbound send_line tap.
        let reader_wire = wire.clone();
        let reader_thread = thread::spawn(move || {
            let mut reader = BufReader::new(reader_stream);
            let mut buf = Vec::new();
            loop {
                buf.clear();
                match reader.read_until(b'\r', &mut buf) {
                    Ok(0) => break, // EOF — socket closed by peer; let channel close
                    Err(_) => break, // I/O error — treat as disconnect
                    Ok(_) => {}
                }
                // Strip the trailing \r the BufReader left in the buffer.
                if buf.last() == Some(&b'\r') {
                    buf.pop();
                }
                let line = match std::str::from_utf8(&buf) {
                    Ok(s) => s.to_owned(),
                    Err(_) => continue, // non-UTF8 line; skip
                };
                if line.trim().is_empty() {
                    continue;
                }
                // tuxlink-ngsk: tap the raw inbound cmd-port line BEFORE parsing,
                // so REJ / NEWSTATE / FAULT — and any line Command::parse does NOT
                // recognize — still reach the session log. ardopcf is the sender,
                // so prefix to make direction unambiguous in the transcript.
                if let Some(ref w) = reader_wire {
                    w(&format!("cmd« {line}"));
                }
                match Command::parse(&line) {
                    Ok(cmd) => {
                        // tuxlink-ytg: book-keep the ARQ link state for the data
                        // socket's EOF-on-DISC + pre-connect drop gates. The update
                        // happens BEFORE the send so the DataSocket sees the new
                        // state by the time any blocking-recv consumer of the cmd
                        // channel reacts.
                        if let Some(ref state) = arq_state {
                            match &cmd {
                                Command::Connected { .. } => state.set_connected(),
                                Command::Disconnected => state.set_disconnected(),
                                Command::NewState(State::Disc) => state.set_disconnected(),
                                // tuxlink-ytg P1 (Codex adrev 2026-05-30): FAULT
                                // events must also clear ArqState. ardopcf can
                                // emit FAULT while the data TCP socket stays
                                // open; without this arm the data socket's
                                // EOF-on-DISC gate stays armed (`is_connected`
                                // remains true), so a blocked B2F `read_line`
                                // keeps polling forever and the command never
                                // reaches its disconnect/reset cleanup.
                                Command::Fault(_) => state.set_disconnected(),
                                _ => {}
                            }
                        }
                        // Sender drop unblocks on Err (receiver gone); exit cleanly.
                        if tx.send(cmd).is_err() {
                            break;
                        }
                    }
                    Err(_) => {
                        // Unknown/malformed command — skip rather than crash the loop.
                    }
                }
            }
            // tuxlink-ytg: if the cmd socket itself dies (EOF or I/O error), the
            // ARQ link cannot be considered live regardless of the last event.
            // Flip the flag to disconnected so any blocked data-socket read
            // unblocks promptly with EOF.
            if let Some(ref state) = arq_state {
                state.set_disconnected();
            }
            // `tx` drops here → channel becomes disconnected → recv_event returns
            // `RecvError::Disconnected`.
        });

        Ok(CmdSocket {
            writer,
            rx,
            reader_thread: Some(reader_thread),
            wire,
        })
    }

    /// Write `line` to the cmd socket, appending the required `\r` terminator.
    pub fn send_line(&mut self, line: &str) -> io::Result<()> {
        // tuxlink-ngsk: tap the outbound cmd-port line so the session log shows
        // what tuxlink SENT (ARQCALL / LISTEN / DISCONNECT / ABORT …) alongside
        // ardopcf's replies — the two halves together tell the on-air story.
        if let Some(ref w) = self.wire {
            w(&format!("cmd» {line}"));
        }
        self.writer.write_all(&encode_cmd_line(line))
    }

    /// Pull the next parsed TNC event, blocking for up to `timeout`.
    ///
    /// Returns `Err(RecvTimeoutError::Timeout)` when the deadline passes
    /// without an event, and `Err(RecvTimeoutError::Disconnected)` when the
    /// reader thread has exited (EOF / socket closed).
    pub fn recv_event(&self, timeout: Duration) -> Result<Command, RecvTimeoutError> {
        self.rx.recv_timeout(timeout)
    }

    /// Block indefinitely for the next parsed TNC event.
    ///
    /// Unlike [`recv_event`] this never returns
    /// `Err(RecvTimeoutError::Timeout)` — it blocks until either an event
    /// arrives or the reader thread closes the channel (EOF / socket
    /// closed), surfacing the latter as `Err(RecvTimeoutError::Disconnected)`
    /// so callers can use a single match arm.
    ///
    /// Used by the no-deadline `arq_connect` path (operator decision bd
    /// tuxlink-qtgg + Codex Round 1 P1 #3) instead of
    /// `recv_event(Duration::MAX)` — `mpsc::Receiver::recv_timeout`
    /// internally calls `Instant::checked_add(now, deadline)` which returns
    /// `None` on overflow, so `Duration::MAX` is not a safe stand-in for
    /// "no deadline" (Codex Round 2 P1 #2).
    pub fn recv_event_blocking(&self) -> Result<Command, RecvTimeoutError> {
        self.rx.recv().map_err(|_| RecvTimeoutError::Disconnected)
    }

    /// Get a cloneable write handle to the cmd socket. Used by `ModemSession`
    /// so a side channel (e.g. `modem_ardop_disconnect`) can send `ABORT`
    /// while `arq_connect`'s recv loop is blocking on this socket's read
    /// side (tuxlink-o3f2 — P1 abort-during-connect fix).
    ///
    /// The returned handle shares the underlying TCP write half via
    /// [`std::net::TcpStream::try_clone`]. Concurrent writes from multiple
    /// threads to the same TCP stream are POSIX-safe at the kernel level
    /// (writes are atomic up to `PIPE_BUF`), but the caller should serialize
    /// at the application layer if the protocol requires command boundaries.
    /// For the ABORT use case the host-protocol message is a single short
    /// line and the only competing writer is `init`/`arq_connect` issuing
    /// the next setter — interleaving is acceptable because ABORT is the
    /// terminal command that unblocks the entire flow.
    pub fn try_clone_writer(&self) -> io::Result<TcpStream> {
        self.writer.try_clone()
    }
}

impl Drop for CmdSocket {
    /// Shut the socket down in both directions so the reader thread's blocked
    /// `read_until` returns promptly, then join it — guaranteeing no leaked
    /// thread when the connection is abandoned without the peer closing first.
    /// The reader holds its own `try_clone`d read half, so dropping the write
    /// half alone would NOT unblock it. (Code review Phase 2a.)
    fn drop(&mut self) {
        let _ = self.writer.shutdown(std::net::Shutdown::Both);
        if let Some(handle) = self.reader_thread.take() {
            let _ = handle.join();
        }
    }
}

// ─── InitConfig ────────────────────────────────────────────────────────────

/// Configuration supplied to [`init_tnc`].
#[derive(Debug, Clone)]
pub struct InitConfig {
    /// Station call sign (e.g. `"N7CPZ"`).
    pub mycall: String,
    /// 4- or 6-character Maidenhead grid square (e.g. `"CN87"`).
    pub gridsquare: String,
    /// ARQ link timeout in seconds (wl2k-go default: 30).
    pub arq_timeout_s: u32,
    /// Optional ARQ bandwidth in Hz: 200/500/1000/2000. None = leave at
    /// ardopcf default. If set, init_tnc sends `ARQBW <hz>FORCED` between
    /// LISTEN and MYCALL (tuxlink-j0ij). Caller validates the value range
    /// before constructing the InitConfig (modem_commands.rs); init_tnc
    /// trusts the value verbatim.
    pub arq_bandwidth_hz: Option<u32>,
    /// Optional ARDOP transmit drive level (0–100). None = leave at ardopcf
    /// default. If set, init_tnc sends `DRIVELEVEL <n>` after ARQBW.
    pub drive_level: Option<u8>,
    /// Whether to send `LISTEN TRUE` (vs `LISTEN FALSE`) during init
    /// (tuxlink-dhbl). Default `false`: the modem comes up NOT listening
    /// for inbound ARDOP calls — operator arms it via the
    /// `ardop_listen` UI command, which uses
    /// [`super::listener::set_listen`] to flip the modem flag at runtime.
    ///
    /// DIVERGES from the pre-tuxlink-dhbl behavior, which hardcoded
    /// `LISTEN FALSE`. The default keeps the same surface but the field
    /// is now expressible — outbound dial paths construct `InitConfig`
    /// with `initial_listen: false` (their existing behavior); the
    /// inbound-listen path constructs with `initial_listen: true` so the
    /// modem is armed on the very first init.
    ///
    /// Per `dev/scratch/winlink-re/findings/ardop-p2p.md` row "Inbound
    /// listener (on-air)": WLE sends `LISTEN TRUE` during session
    /// activation regardless of P2P-vs-CMS mode. Tuxlink's P1-defensive
    /// posture is "default FALSE, operator opts in" — see the
    /// architecture doc §5 operator-decision defaults.
    pub initial_listen: bool,
}

// ─── SessionError ──────────────────────────────────────────────────────────

/// Why an ARDOP session operation failed.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("parse: {0}")]
    Parse(#[from] CommandParseError),
    #[error("TNC fault: {0}")]
    Fault(String),
    #[error("timeout waiting for ack of {cmd}")]
    Timeout { cmd: String },
    #[error("unexpected response to {cmd}: {got:?}")]
    Unexpected { cmd: String, got: String },
}

// ─── init_tnc ──────────────────────────────────────────────────────────────

/// How long to wait for a single setter's echo-back ack before giving up.
const SETTER_ACK_TIMEOUT: Duration = Duration::from_secs(10);

/// Drive the ARDOP init sequence (per wl2k-go `transport/ardop/tnc.go::init()`):
///
/// 1. `INITIALIZE`
/// 2. `CODEC TRUE`
/// 3. `PROTOCOLMODE ARQ`
/// 4. `ARQTIMEOUT <n>`
/// 5. `LISTEN FALSE`
/// 6. `ARQBW <hz>FORCED` — only when `cfg.arq_bandwidth_hz` is Some (tuxlink-j0ij).
/// 7. `DRIVELEVEL <n>` — only when `cfg.drive_level` is Some.
/// 8. `MYCALL <call>`
/// 9. `GRIDSQUARE <grid>`
///
/// For each setter: sends the encoded line, then consumes events from the channel
/// until the matching `EchoBack(cmd)` ack arrives — tolerating interleaved async
/// events (NewState/Ptt/Busy/Buffer/etc.) — or a `Fault` (→ `SessionError::Fault`).
/// Returns `Err(SessionError::Timeout)` if the ack does not arrive within
/// [`SETTER_ACK_TIMEOUT`].
///
/// **ARQBW placement (tuxlink-j0ij):** between LISTEN and MYCALL. ardopcf accepts
/// ARQBW at any point after INITIALIZE, but placing it before MYCALL ensures the
/// bandwidth is established before any subsequent `ARQCALL` honors it. `FORCED`
/// (rather than `MAX`) means the client's value wins over any server-side
/// preference during ARQ negotiation — operator's explicit choice for v1.
pub fn init_tnc(sock: &mut CmdSocket, cfg: &InitConfig) -> Result<(), SessionError> {
    set_and_ack(sock, "INITIALIZE", None)?;
    set_and_ack(sock, "CODEC", Some("TRUE"))?;
    set_and_ack(sock, "PROTOCOLMODE", Some("ARQ"))?;
    set_and_ack(sock, "ARQTIMEOUT", Some(&cfg.arq_timeout_s.to_string()))?;
    // tuxlink-dhbl: LISTEN flag is now operator-controlled. Default
    // `false` (no inbound listen) preserves prior behavior; the
    // `ardop_listen` UI command flips it via
    // `super::listener::set_listen` at runtime.
    let listen_arg = if cfg.initial_listen { "TRUE" } else { "FALSE" };
    set_and_ack(sock, "LISTEN", Some(listen_arg))?;
    if let Some(bw) = cfg.arq_bandwidth_hz {
        // ardopcf's ARQBW parameter is a SINGLE token — `2000FORCED` / `500MAX`,
        // no space between width and qualifier. `{bw} FORCED` (with a space)
        // parses as two params and faults "Syntax Err: ARQBW <bw> FORCED",
        // aborting init whenever a bandwidth is set (tuxlink-87uc).
        set_and_ack(sock, "ARQBW", Some(&format!("{bw}FORCED")))?;
    }
    if let Some(dl) = cfg.drive_level {
        set_and_ack(sock, "DRIVELEVEL", Some(&dl.to_string()))?;
    }
    set_and_ack(sock, "MYCALL", Some(&cfg.mycall))?;
    set_and_ack(sock, "GRIDSQUARE", Some(&cfg.gridsquare))?;
    Ok(())
}

/// Send one setter and wait for its echo-back ack.
///
/// Absorbs any interleaved async events (NewState, Ptt, Busy, Buffer, Status,
/// Connected, Disconnected) that may arrive before the ack. Returns on the first
/// matching `EchoBack`, `Fault`, or timeout.
fn set_and_ack(sock: &mut CmdSocket, cmd: &str, arg: Option<&str>) -> Result<(), SessionError> {
    sock.send_line(&encode_setter(cmd, arg))?;
    loop {
        match sock.recv_event(SETTER_ACK_TIMEOUT) {
            Ok(Command::EchoBack(name)) if name.eq_ignore_ascii_case(cmd) => return Ok(()),
            Ok(Command::EchoBack(other)) => {
                // An echo-back for a *different* command — unexpected; surface it.
                return Err(SessionError::Unexpected { cmd: cmd.into(), got: other });
            }
            Ok(Command::Fault(msg)) => return Err(SessionError::Fault(msg)),
            // Interleaved async events are normal during init; absorb and continue.
            // PingAck / Ping (tuxlink-1637) are async telemetry like Status —
            // they can arrive at any moment, including between a setter and
            // its echo-back, and must not break the ack wait.
            Ok(
                Command::NewState(_)
                | Command::Ptt(_)
                | Command::Busy(_)
                | Command::Buffer(_)
                | Command::Status(_)
                | Command::PingAck { .. }
                | Command::Ping { .. }
                | Command::Connected { .. }
                | Command::Disconnected,
            ) => continue,
            Err(RecvTimeoutError::Timeout) => {
                return Err(SessionError::Timeout { cmd: cmd.into() });
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(SessionError::Io(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "cmd socket disconnected while waiting for ack",
                )));
            }
        }
    }
}

// ─── ConnectInfo ───────────────────────────────────────────────────────────

/// Result of a successful ARQ connect handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectInfo {
    /// Peer station callsign as reported by the TNC.
    pub peer_call: String,
    /// Negotiated link bandwidth in Hz.
    pub bandwidth_hz: u32,
}

// ─── arq_connect ───────────────────────────────────────────────────────────

/// Initiate an ARQ connection to `target` with `repeat` retries.
///
/// Before sending `ARQCALL`, drains any already-queued events from the cmd
/// channel so that stale events from a previous session phase (e.g. a
/// `NEWSTATE DISC` queued before this call) cannot be misread as a
/// "DISC before CONNECTED" failure. Draining uses a zero timeout so it
/// does not block.
///
/// After draining, sends `ARQCALL <target> <repeat>` and waits until the
/// TNC emits `CONNECTED <peer_call> <bw>` (success), `FAULT <msg>` (error),
/// or `DISCONNECTED`/`NEWSTATE DISC` (error — link dropped before connecting).
///
/// The `deadline` is an `Option<Duration>` (operator decision bd tuxlink-qtgg
/// + Codex Round 1 P1 #3):
/// - `Some(d)` — **overall** deadline from the time the function is called;
///   per-iteration remaining time is recomputed so the loop actually
///   terminates.
/// - `None` — no wall-clock cap; each iteration blocks indefinitely via
///   [`CmdSocket::recv_event_blocking`] until CONNECTED / FAULT /
///   DISCONNECTED arrives or the cmd socket closes. The bound on keyed
///   airtime is the modem's own `ARQTIMEOUT` × `CONNECT_REPEAT` plus the
///   operator's ABORT side channel. Codex Round 2 P1 #2 explicitly rejects
///   `Duration::MAX` as a stand-in here — that value overflows
///   `mpsc::Receiver::recv_timeout`'s internal `Instant::checked_add`.
pub fn arq_connect(
    sock: &mut CmdSocket,
    target: &str,
    repeat: u32,
    deadline: Option<Duration>,
) -> Result<ConnectInfo, SessionError> {
    // Drain stale events (e.g. NEWSTATE DISC from a prior phase) before
    // sending ARQCALL so they cannot be misread as a connect failure.
    // Duration::ZERO makes recv_event return immediately if nothing is queued.
    while sock.recv_event(Duration::ZERO).is_ok() {}

    let start = Instant::now();
    sock.send_line(&encode_setter(
        "ARQCALL",
        Some(&format!("{target} {repeat}")),
    ))?;
    loop {
        // Branch on Some(deadline) (bounded wait) vs None (block indefinitely).
        // Avoids passing Duration::MAX through recv_timeout (Codex R2 P1 #2).
        let recv_result = match deadline {
            Some(d) => {
                let elapsed = start.elapsed();
                if elapsed >= d {
                    return Err(SessionError::Timeout { cmd: "ARQCALL".into() });
                }
                sock.recv_event(d - elapsed)
            }
            None => sock.recv_event_blocking(),
        };
        match recv_result {
            Ok(Command::Connected { peer_call, bandwidth_hz }) => {
                return Ok(ConnectInfo { peer_call, bandwidth_hz });
            }
            Ok(Command::Fault(msg)) => {
                return Err(SessionError::Fault(msg));
            }
            Ok(Command::Disconnected) | Ok(Command::NewState(State::Disc)) => {
                return Err(SessionError::Fault(
                    "disconnected/DISC before CONNECTED".into(),
                ));
            }
            // Absorb all other events (ARQCALL echo-back, NewState ISS/IRS,
            // Ptt, Busy, Buffer, Status, …) and keep waiting.
            Ok(_) => continue,
            Err(RecvTimeoutError::Timeout) => {
                return Err(SessionError::Timeout { cmd: "ARQCALL".into() });
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(SessionError::Io(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "cmd socket disconnected while waiting for CONNECTED",
                )));
            }
        }
    }
}

// ─── arq_disconnect ────────────────────────────────────────────────────────

/// Send `DISCONNECT` and wait for the TNC to confirm the link is torn down.
///
/// Resolves on `DISCONNECTED` or `NEWSTATE DISC`, bounded by `deadline`.
pub fn arq_disconnect(sock: &mut CmdSocket, deadline: Duration) -> Result<(), SessionError> {
    let start = Instant::now();
    sock.send_line("DISCONNECT")?;
    loop {
        let elapsed = start.elapsed();
        if elapsed >= deadline {
            return Err(SessionError::Timeout { cmd: "DISCONNECT".into() });
        }
        let remaining = deadline - elapsed;
        match sock.recv_event(remaining) {
            Ok(Command::Disconnected) | Ok(Command::NewState(State::Disc)) => return Ok(()),
            // Absorb interleaved events.
            Ok(_) => continue,
            Err(RecvTimeoutError::Timeout) => {
                return Err(SessionError::Timeout { cmd: "DISCONNECT".into() });
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(SessionError::Io(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "cmd socket disconnected while waiting for DISCONNECTED",
                )));
            }
        }
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    // ── Mock TNC helper ───────────────────────────────────────────────────

    /// Bind a local TCP listener, spawn a thread to accept one connection, and
    /// return the bound address together with a join handle.
    ///
    /// The `handler` closure runs on the server thread and receives the accepted
    /// `TcpStream` (with a 2-second read timeout already set, so drain loops exit
    /// promptly rather than blocking forever if the client holds the socket open).
    fn spawn_mock_tnc<F>(handler: F) -> (SocketAddr, thread::JoinHandle<()>)
    where
        F: FnOnce(std::net::TcpStream) + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (conn, _) = listener.accept().unwrap();
            // A read timeout ensures the server thread exits promptly even when
            // the client does not explicitly shut down its write half.
            conn.set_read_timeout(Some(Duration::from_secs(2))).ok();
            handler(conn);
        });
        (addr, handle)
    }

    /// Read one `\r`-terminated line from `conn` and return it (without the `\r`).
    ///
    /// Returns an empty string on EOF or timeout — callers use this to break
    /// their read loop cleanly.
    fn read_cmd_line(conn: &mut BufReader<std::net::TcpStream>) -> String {
        let mut buf = Vec::new();
        match conn.read_until(b'\r', &mut buf) {
            Ok(0) | Err(_) => return String::new(), // EOF or timeout — signal loop exit
            Ok(_) => {}
        }
        if buf.last() == Some(&b'\r') {
            buf.pop();
        }
        String::from_utf8(buf).unwrap_or_default()
    }

    /// Write `line\r` to the connection (TNC → client direction).
    fn write_reply(conn: &mut std::net::TcpStream, line: &str) {
        // Ignore errors: the client may have already closed its read side.
        let _ = conn.write_all(format!("{line}\r").as_bytes());
    }

    // ── Test 1: init_tnc sends exactly the 7 setters in order ────────────

    #[test]
    fn init_tnc_sends_exactly_7_setters_in_order() {
        // Recording mock: accepts each command line, echoes the command name back
        // as an ack, and records every line received.
        let recorded: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let rec = recorded.clone();

        let (addr, server) = spawn_mock_tnc(move |conn| {
            let mut writer = conn.try_clone().unwrap();
            let mut reader = BufReader::new(conn);
            loop {
                let line = read_cmd_line(&mut reader);
                if line.is_empty() {
                    break; // EOF or read timeout — all 7 setters processed
                }
                rec.lock().unwrap().push(line.clone());
                // Echo back the command name (first token) as ack.
                let cmd_name = line.split_whitespace().next().unwrap_or("").to_string();
                write_reply(&mut writer, &cmd_name);
            }
        });

        let mut sock = CmdSocket::connect(addr).unwrap();
        let cfg = InitConfig {
            mycall: "N7CPZ".into(),
            gridsquare: "CN87".into(),
            arq_timeout_s: 30,
            arq_bandwidth_hz: None,
            drive_level: None,
            initial_listen: false,
        };
        init_tnc(&mut sock, &cfg).expect("init should succeed");

        // Signal EOF to the server so its read loop exits via the FIN rather
        // than waiting for the 2-second read timeout.
        let _ = sock.writer.shutdown(std::net::Shutdown::Write);
        server.join().unwrap();

        let lines = recorded.lock().unwrap().clone();
        assert_eq!(
            lines,
            vec![
                "INITIALIZE",
                "CODEC TRUE",
                "PROTOCOLMODE ARQ",
                "ARQTIMEOUT 30",
                "LISTEN FALSE",
                "MYCALL N7CPZ",
                "GRIDSQUARE CN87",
            ],
            "init sequence must match wl2k-go's tnc.go::init() order"
        );
    }

    #[test]
    fn cmd_socket_wire_tap_captures_inbound_lines_verbatim() {
        // tuxlink-ngsk: the wire tap must hand each raw inbound cmd-port line to
        // the sink BEFORE parsing — so REJ / NEWSTATE / FAULT (the alpha
        // troubleshooting signal an operator uploads in a log) reach the session
        // log even when ardopcf emits a line we don't model as a Command. The
        // line is prefixed `cmd« ` to mark direction (ardopcf → tuxlink).
        let (addr, server) = spawn_mock_tnc(move |mut conn| {
            // Unsolicited TNC chatter, as ardopcf emits during a session.
            write_reply(&mut conn, "PTT TRUE");
            write_reply(&mut conn, "NEWSTATE DISC");
            // Hold briefly so the client drains both lines before EOF.
            thread::sleep(Duration::from_millis(200));
        });

        let recorded: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let rec = recorded.clone();
        let wire: WireSink = Arc::new(move |line: &str| {
            rec.lock().unwrap().push(line.to_string());
        });

        let sock =
            CmdSocket::connect_with_arq_state_and_wire(addr, None, Some(wire)).unwrap();
        // Drain parsed events until EOF/timeout — guarantees the reader thread
        // has processed (and therefore tapped) every line the mock sent.
        while sock.recv_event(Duration::from_millis(300)).is_ok() {}
        drop(sock);
        server.join().unwrap();

        let lines = recorded.lock().unwrap().clone();
        assert!(
            lines.iter().any(|l| l == "cmd« PTT TRUE"),
            "wire tap must capture the verbatim inbound line; got {lines:?}",
        );
        assert!(
            lines.iter().any(|l| l == "cmd« NEWSTATE DISC"),
            "wire tap must capture NEWSTATE DISC verbatim; got {lines:?}",
        );
    }

    // ── Test 2: init_tnc returns Err(Fault) when mock answers with FAULT ─

    #[test]
    fn init_tnc_returns_fault_when_tnc_sends_fault() {
        // Mock that answers the very first setter (INITIALIZE) with a FAULT.
        // The 2-second read timeout (set in spawn_mock_tnc) ensures the server
        // thread exits promptly without needing an explicit drain loop.
        let (addr, server) = spawn_mock_tnc(|conn| {
            let mut writer = conn.try_clone().unwrap();
            let mut reader = BufReader::new(conn);
            // Consume the INITIALIZE line.
            read_cmd_line(&mut reader);
            // Reply with a fault.
            write_reply(&mut writer, "FAULT hardware not ready");
            // Server thread exits when the read timeout fires or the client closes.
        });

        let mut sock = CmdSocket::connect(addr).unwrap();
        let cfg = InitConfig {
            mycall: "N7CPZ".into(),
            gridsquare: "CN87".into(),
            arq_timeout_s: 30,
            arq_bandwidth_hz: None,
            drive_level: None,
            initial_listen: false,
        };
        let err = init_tnc(&mut sock, &cfg).expect_err("init must fail on FAULT");
        assert!(
            matches!(err, SessionError::Fault(ref s) if s.contains("hardware not ready")),
            "expected Fault(\"hardware not ready\"), got {err:?}"
        );
        // Drop the socket before joining so any server-side read unblocks.
        drop(sock);
        server.join().unwrap();
    }

    // ── Test 3: recv_event delivers an unsolicited async event ────────────

    #[test]
    fn recv_event_delivers_unsolicited_async_event() {
        // Mock that immediately sends NEWSTATE DISC with no prior command.
        // The 2-second read timeout ensures the server thread exits without
        // requiring the client to explicitly close.
        let (addr, server) = spawn_mock_tnc(|mut conn| {
            write_reply(&mut conn, "NEWSTATE DISC");
            // Server thread exits when the read timeout fires.
        });

        let sock = CmdSocket::connect(addr).unwrap();
        let event = sock
            .recv_event(Duration::from_secs(5))
            .expect("should receive NEWSTATE DISC");
        assert!(
            matches!(event, Command::NewState(super::super::command::State::Disc)),
            "expected NewState(Disc), got {event:?}"
        );
        drop(sock);
        server.join().unwrap();
    }

    // ── Test 4: init_tnc tolerates an async event before the ack ─────────

    #[test]
    fn init_tnc_tolerates_async_event_before_ack() {
        // Mock that for each setter sends NEWSTATE DISC first, then the ack.
        // init_tnc must skip the async event and return successfully.
        let (addr, server) = spawn_mock_tnc(|conn| {
            let mut writer = conn.try_clone().unwrap();
            let mut reader = BufReader::new(conn);
            loop {
                let line = read_cmd_line(&mut reader);
                if line.is_empty() {
                    break;
                }
                let cmd_name = line.split_whitespace().next().unwrap_or("").to_string();
                // Interleave an async NEWSTATE event before the echo-back ack.
                write_reply(&mut writer, "NEWSTATE DISC");
                write_reply(&mut writer, &cmd_name);
            }
        });

        let mut sock = CmdSocket::connect(addr).unwrap();
        let cfg = InitConfig {
            mycall: "N7CPZ".into(),
            gridsquare: "CN87".into(),
            arq_timeout_s: 30,
            arq_bandwidth_hz: None,
            drive_level: None,
            initial_listen: false,
        };
        init_tnc(&mut sock, &cfg).expect("init must tolerate interleaved async events");

        let _ = sock.writer.shutdown(std::net::Shutdown::Write);
        server.join().unwrap();
    }

    // ── tuxlink-j0ij: init_tnc sends ARQBW <hz>FORCED between LISTEN and MYCALL ──

    /// When `cfg.arq_bandwidth_hz` is Some, init_tnc must send
    /// `ARQBW <hz>FORCED` AFTER `LISTEN FALSE` and BEFORE `MYCALL`.
    /// The 8-element sequence proves both presence and position.
    #[test]
    fn init_tnc_sends_arqbw_between_listen_and_mycall_when_some() {
        let recorded: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let rec = recorded.clone();

        let (addr, server) = spawn_mock_tnc(move |conn| {
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

        let mut sock = CmdSocket::connect(addr).unwrap();
        let cfg = InitConfig {
            mycall: "N7CPZ".into(),
            gridsquare: "CN87".into(),
            arq_timeout_s: 30,
            arq_bandwidth_hz: Some(500),
            drive_level: None,
            initial_listen: false,
        };
        init_tnc(&mut sock, &cfg).expect("init with bandwidth should succeed");

        let _ = sock.writer.shutdown(std::net::Shutdown::Write);
        server.join().unwrap();

        let lines = recorded.lock().unwrap().clone();
        assert_eq!(
            lines,
            vec![
                "INITIALIZE",
                "CODEC TRUE",
                "PROTOCOLMODE ARQ",
                "ARQTIMEOUT 30",
                "LISTEN FALSE",
                "ARQBW 500FORCED",
                "MYCALL N7CPZ",
                "GRIDSQUARE CN87",
            ],
            "ARQBW <hz>FORCED must be sent between LISTEN FALSE and MYCALL when bandwidth is Some (tuxlink-j0ij)"
        );
    }

    /// When `cfg.drive_level` is Some, init_tnc must send `DRIVELEVEL <n>`
    /// immediately AFTER the ARQBW setter and BEFORE `MYCALL`. The full
    /// sequence proves both presence and position.
    #[test]
    fn init_tnc_sends_drivelevel_when_some() {
        let recorded: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let rec = recorded.clone();

        let (addr, server) = spawn_mock_tnc(move |conn| {
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

        let mut sock = CmdSocket::connect(addr).unwrap();
        let cfg = InitConfig {
            mycall: "N7CPZ".into(),
            gridsquare: "CN87".into(),
            arq_timeout_s: 30,
            arq_bandwidth_hz: Some(500),
            drive_level: Some(40),
            initial_listen: false,
        };
        init_tnc(&mut sock, &cfg).expect("init with drive level should succeed");

        let _ = sock.writer.shutdown(std::net::Shutdown::Write);
        server.join().unwrap();

        let lines = recorded.lock().unwrap().clone();
        assert_eq!(
            lines,
            vec![
                "INITIALIZE",
                "CODEC TRUE",
                "PROTOCOLMODE ARQ",
                "ARQTIMEOUT 30",
                "LISTEN FALSE",
                "ARQBW 500FORCED",
                "DRIVELEVEL 40",
                "MYCALL N7CPZ",
                "GRIDSQUARE CN87",
            ],
            "DRIVELEVEL <n> must be sent immediately after ARQBW and before MYCALL when drive_level is Some"
        );
    }

    /// When `cfg.arq_bandwidth_hz` is None, init_tnc must NOT send any ARQBW
    /// setter — ardopcf's default (or the WebGUI's persistent override) takes
    /// over. Regression guard for the migration path.
    #[test]
    fn init_tnc_does_not_send_arqbw_when_none() {
        let recorded: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let rec = recorded.clone();

        let (addr, server) = spawn_mock_tnc(move |conn| {
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

        let mut sock = CmdSocket::connect(addr).unwrap();
        let cfg = InitConfig {
            mycall: "N7CPZ".into(),
            gridsquare: "CN87".into(),
            arq_timeout_s: 30,
            arq_bandwidth_hz: None,
            drive_level: None,
            initial_listen: false,
        };
        init_tnc(&mut sock, &cfg).expect("init with no bandwidth should succeed");

        let _ = sock.writer.shutdown(std::net::Shutdown::Write);
        server.join().unwrap();

        let lines = recorded.lock().unwrap().clone();
        assert!(
            !lines.iter().any(|l| l.starts_with("ARQBW")),
            "ARQBW must NOT be sent when arq_bandwidth_hz is None; got: {lines:?}"
        );
        // And the canonical 7-setter sequence is preserved.
        assert_eq!(lines.len(), 7, "expected exactly 7 setters when bandwidth is None");
    }

    // ── Test 5: arq_connect resolves to ConnectInfo on happy path ─────────

    #[test]
    fn arq_connect_resolves_on_connected() {
        // Mock emits: ARQCALL echo-back → NEWSTATE ISS → CONNECTED W7ABC 500
        let (addr, server) = spawn_mock_tnc(|conn| {
            let mut writer = conn.try_clone().unwrap();
            let mut reader = BufReader::new(conn);
            // Consume the ARQCALL line.
            let _line = read_cmd_line(&mut reader);
            write_reply(&mut writer, "ARQCALL");          // echo-back
            write_reply(&mut writer, "NEWSTATE ISS");     // async state transition
            write_reply(&mut writer, "CONNECTED W7ABC 500");
        });

        let mut sock = CmdSocket::connect(addr).unwrap();
        let info = arq_connect(&mut sock, "W7ABC", 3, Some(Duration::from_secs(10)))
            .expect("arq_connect must succeed on CONNECTED");
        assert_eq!(info.peer_call, "W7ABC");
        assert_eq!(info.bandwidth_hz, 500);
        drop(sock);
        server.join().unwrap();
    }

    // ── Test 6: arq_connect returns Err(Fault) on FAULT reply ─────────────

    #[test]
    fn arq_connect_returns_fault_on_fault_reply() {
        let (addr, server) = spawn_mock_tnc(|conn| {
            let mut writer = conn.try_clone().unwrap();
            let mut reader = BufReader::new(conn);
            let _line = read_cmd_line(&mut reader);
            write_reply(&mut writer, "FAULT not from state DISC");
        });

        let mut sock = CmdSocket::connect(addr).unwrap();
        let err = arq_connect(&mut sock, "W7ABC", 3, Some(Duration::from_secs(10)))
            .expect_err("must fail on FAULT");
        assert!(
            matches!(err, SessionError::Fault(ref s) if s.contains("not from state DISC")),
            "expected Fault, got {err:?}"
        );
        drop(sock);
        server.join().unwrap();
    }

    // ── Test 7: arq_connect returns Err on NEWSTATE DISC before CONNECTED ──

    #[test]
    fn arq_connect_returns_fault_on_disc_before_connected() {
        let (addr, server) = spawn_mock_tnc(|conn| {
            let mut writer = conn.try_clone().unwrap();
            let mut reader = BufReader::new(conn);
            let _line = read_cmd_line(&mut reader);
            write_reply(&mut writer, "NEWSTATE DISC");
        });

        let mut sock = CmdSocket::connect(addr).unwrap();
        let err = arq_connect(&mut sock, "W7ABC", 3, Some(Duration::from_secs(10)))
            .expect_err("must fail on DISC before CONNECTED");
        assert!(
            matches!(err, SessionError::Fault(_)),
            "expected Fault, got {err:?}"
        );
        drop(sock);
        server.join().unwrap();
    }

    // ── Test 7b: arq_connect returns Err on DISCONNECTED before CONNECTED ──

    #[test]
    fn arq_connect_returns_fault_on_disconnected_before_connected() {
        let (addr, server) = spawn_mock_tnc(|conn| {
            let mut writer = conn.try_clone().unwrap();
            let mut reader = BufReader::new(conn);
            let _line = read_cmd_line(&mut reader);
            write_reply(&mut writer, "DISCONNECTED");
        });

        let mut sock = CmdSocket::connect(addr).unwrap();
        let err = arq_connect(&mut sock, "W7ABC", 3, Some(Duration::from_secs(10)))
            .expect_err("must fail on DISCONNECTED before CONNECTED");
        assert!(
            matches!(err, SessionError::Fault(_)),
            "expected Fault, got {err:?}"
        );
        drop(sock);
        server.join().unwrap();
    }

    // ── Test 7c: arq_connect drains stale DISC before sending ARQCALL ──────

    /// Pre-queue a NEWSTATE DISC on the mock (simulating a stale event from a
    /// prior session phase), then script a normal successful connect sequence.
    /// arq_connect must SUCCEED — the stale DISC is drained, not misread as
    /// "DISC before CONNECTED".
    #[test]
    fn arq_connect_drains_stale_disc_before_arqcall() {
        let (addr, server) = spawn_mock_tnc(|conn| {
            let mut writer = conn.try_clone().unwrap();
            let mut reader = BufReader::new(conn);

            // Emit a stale NEWSTATE DISC *before* the client sends ARQCALL.
            // This simulates an event queued from a previous phase.
            write_reply(&mut writer, "NEWSTATE DISC");

            // Now wait for the ARQCALL and script the happy path.
            let _line = read_cmd_line(&mut reader); // consume ARQCALL
            write_reply(&mut writer, "ARQCALL");
            write_reply(&mut writer, "NEWSTATE ISS");
            write_reply(&mut writer, "CONNECTED W7ABC 500");
        });

        let mut sock = CmdSocket::connect(addr).unwrap();

        // Give the server a moment to push the stale DISC into the socket
        // before we call arq_connect so it's queued in the channel.
        std::thread::sleep(Duration::from_millis(50));

        let info = arq_connect(&mut sock, "W7ABC", 3, Some(Duration::from_secs(10)))
            .expect("arq_connect must succeed when stale DISC is drained");
        assert_eq!(info.peer_call, "W7ABC");
        assert_eq!(info.bandwidth_hz, 500);

        drop(sock);
        server.join().unwrap();
    }

    // ── Test 7d: FAULT event clears the shared ArqState (tuxlink-ytg P1) ───

    /// Codex adrev 2026-05-30 P1 #1: when ardopcf emits FAULT while the data
    /// TCP socket stays open, the reader thread must flip `ArqState` to
    /// disconnected so the DataSocket's EOF-on-DISC gate fires. Without this,
    /// a blocked B2F read_line keeps polling forever and the surrounding Tauri
    /// command never reaches its cleanup.
    #[test]
    fn fault_event_clears_arq_state() {
        // Mock that emits exactly one event: `FAULT some error`.
        let (addr, server) = spawn_mock_tnc(|mut conn| {
            write_reply(&mut conn, "FAULT hardware not ready");
            // Server thread exits when the read timeout fires.
        });

        // Pre-seed an ArqState in the connected position to make the
        // transition observable.
        let arq_state = ArqState::new();
        arq_state.set_connected();
        assert!(arq_state.is_connected(), "precondition: state starts connected");

        let sock = CmdSocket::connect_with_arq_state(addr, Some(arq_state.clone()))
            .expect("connect must succeed");

        // The reader thread should parse the FAULT and clear the flag. Wait
        // for the channel to deliver the parsed event so we know the reader
        // has processed it (the flag flip happens BEFORE the send).
        let event = sock
            .recv_event(Duration::from_secs(2))
            .expect("FAULT event must reach the channel");
        assert!(
            matches!(event, Command::Fault(ref s) if s.contains("hardware not ready")),
            "expected Fault, got {event:?}"
        );

        assert!(
            !arq_state.is_connected(),
            "FAULT must clear ArqState — the data-socket EOF-on-DISC gate \
             depends on this transition (Codex tuxlink-ytg P1)."
        );

        drop(sock);
        server.join().unwrap();
    }

    // ── Test 7e: try_clone_writer returns a usable side-channel writer ─────

    /// tuxlink-o3f2: `CmdSocket::try_clone_writer` returns a clone of the
    /// TCP write half. A write to the clone must reach the same peer the
    /// `send_line` writes reach. This is the foundation for the
    /// `ModemSession::abort_in_flight` side channel that sends `ABORT`
    /// while `arq_connect` is blocked on the recv channel.
    #[test]
    fn try_clone_writer_returns_usable_side_channel_writer() {
        // Mock that records every line received and never closes.
        let received: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let rec = received.clone();
        let (addr, server) = spawn_mock_tnc(move |conn| {
            let mut reader = BufReader::new(conn);
            loop {
                let line = read_cmd_line(&mut reader);
                if line.is_empty() {
                    break;
                }
                rec.lock().unwrap().push(line);
            }
        });

        let sock = CmdSocket::connect(addr).unwrap();
        let mut writer_clone = sock
            .try_clone_writer()
            .expect("try_clone_writer must succeed on a live socket");
        // Side-channel write — bypasses the &mut CmdSocket gate that the
        // recv-blocked thread would otherwise hold.
        writer_clone
            .write_all(b"ABORT\r")
            .expect("side-channel write must succeed");
        writer_clone.flush().ok();

        // Wait briefly for the mock thread to read the line.
        std::thread::sleep(Duration::from_millis(50));

        // Close both halves of the cmd socket so the mock's read loop exits.
        drop(sock);
        drop(writer_clone);
        server.join().unwrap();

        let lines = received.lock().unwrap().clone();
        assert!(
            lines.iter().any(|l| l == "ABORT"),
            "side-channel writer must have delivered ABORT to the peer; got: {lines:?}"
        );
    }

    // ── Test 8: arq_disconnect resolves on DISCONNECTED ───────────────────

    #[test]
    fn arq_disconnect_resolves_on_disconnected() {
        let (addr, server) = spawn_mock_tnc(|conn| {
            let mut writer = conn.try_clone().unwrap();
            let mut reader = BufReader::new(conn);
            // Consume the DISCONNECT line.
            let line = read_cmd_line(&mut reader);
            assert_eq!(line, "DISCONNECT", "must send DISCONNECT command");
            write_reply(&mut writer, "DISCONNECTED");
        });

        let mut sock = CmdSocket::connect(addr).unwrap();
        arq_disconnect(&mut sock, Duration::from_secs(10))
            .expect("arq_disconnect must succeed on DISCONNECTED");
        drop(sock);
        server.join().unwrap();
    }
}
