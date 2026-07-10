//! VARA TCP transport — open the cmd + data socket pair, send / receive
//! commands, expose the connected-mode data byte stream.
//!
//! Synchronous `std::io` + `std::thread` per the modem-subtree's
//! concurrency posture (ADR 0015). The data socket is held as a
//! `TcpStream` and exposed via `data_stream()` for the B2F session
//! layer to read/write directly.
//!
//! ## RADIO-1
//!
//! Opening these sockets DOES NOT transmit by itself. CONNECT does.
//! The smoke probe and unit tests only exercise the TCP layer +
//! command roundtrips; no CONNECT is issued without operator intent.

use std::io::{self, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use super::command::{InboundCommand, OutboundCommand};
use super::wire::{write_line, LineReader};
use crate::modem_status::{ShutdownableStream, ABORT_WRITE_TIMEOUT};

/// Configuration for connecting to a VARA TCP instance.
#[derive(Debug, Clone)]
pub struct VaraConfig {
    /// Host (hostname or IP) of the VARA modem.
    pub host: String,
    /// Command socket port (VARA default: 8300).
    pub cmd_port: u16,
    /// Data socket port (VARA default: cmd_port + 1 = 8301).
    pub data_port: u16,
    /// TCP connect timeout.
    pub connect_timeout: Duration,
    /// Per-read timeout on the command socket. None = blocking
    /// indefinitely.
    pub read_timeout: Option<Duration>,
    /// Per-read timeout on the DATA socket. None = blocking
    /// indefinitely.
    ///
    /// Deliberately independent of `read_timeout`: the cmd socket ticks
    /// at event cadence (a short timeout keeps status loops responsive),
    /// but the data socket carries the B2F byte stream **paced by the RF
    /// link**. At VARA 500 the observed throughput is tens of bps — a
    /// gateway's SID banner takes 10–20+ s to arrive, and multi-second
    /// inter-byte gaps are normal mid-transfer. The B2F reader maps any
    /// read error (including a timeout tick) to ConnectionClosed, so a
    /// cmd-scale timeout here tears down a healthy link: on 2026-07-10
    /// the first-ever on-air gateway answer (KD6OAT, BW500, S/N −11.8 dB)
    /// was disconnected 4 s after link-up by exactly this (tuxlink-xzxk1).
    pub data_read_timeout: Option<Duration>,
}

impl Default for VaraConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            cmd_port: 8300,
            data_port: 8301,
            connect_timeout: Duration::from_secs(5),
            read_timeout: Some(Duration::from_secs(2)),
            // RF-scale (see the field doc): bounds a dead-link read at the
            // same order as VARA's own ARQ timeout regime while never
            // expiring on a healthy slow link.
            data_read_timeout: Some(Duration::from_secs(120)),
        }
    }
}

/// VARA TCP transport. Holds the cmd + data socket pair plus a
/// line-buffered reader over the cmd socket.
pub struct VaraTransport {
    cfg: VaraConfig,
    cmd_writer: TcpStream,
    cmd_reader: LineReader<TcpStream>,
    /// Connected-mode data byte stream. `Read + Write` for the
    /// session layer.
    data_stream: TcpStream,
}

impl VaraTransport {
    /// Open the cmd + data socket pair and return the transport. Does
    /// NOT issue any VARA commands — caller is responsible for the
    /// `MYCALL` + `BW` + `LISTEN` initialization sequence.
    pub fn connect(cfg: VaraConfig) -> io::Result<Self> {
        tracing::info!(
            target: "tuxlink::winlink::modem::vara",
            host = %cfg.host,
            cmd_port = cfg.cmd_port,
            data_port = cfg.data_port,
            "VARA transport connecting",
        );
        let cmd_addr = (cfg.host.as_str(), cfg.cmd_port)
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("no addresses for {}:{}", cfg.host, cfg.cmd_port),
                )
            })?;
        let data_addr = (cfg.host.as_str(), cfg.data_port)
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("no addresses for {}:{}", cfg.host, cfg.data_port),
                )
            })?;

        let cmd_stream = TcpStream::connect_timeout(&cmd_addr, cfg.connect_timeout)?;
        cmd_stream.set_read_timeout(cfg.read_timeout)?;
        let cmd_writer = cmd_stream.try_clone()?;
        let cmd_reader = LineReader::new(cmd_stream);

        let data_stream = TcpStream::connect_timeout(&data_addr, cfg.connect_timeout)?;
        // SO_RCVTIMEO lives on the socket, not the fd, so this also governs
        // every `try_clone` handed to the B2F exchange (writer + reader).
        data_stream.set_read_timeout(cfg.data_read_timeout)?;

        tracing::info!(
            target: "tuxlink::winlink::modem::vara",
            host = %cfg.host,
            cmd_port = cfg.cmd_port,
            data_port = cfg.data_port,
            "VARA transport connected",
        );

        Ok(Self {
            cfg,
            cmd_writer,
            cmd_reader,
            data_stream,
        })
    }

    /// Send one outbound command (auto-appends the `\r` terminator).
    pub fn send(&mut self, cmd: &OutboundCommand) -> io::Result<()> {
        let line = cmd.as_wire();
        tracing::debug!(
            target: "tuxlink::winlink::modem::vara",
            command = %line,
            "VARA command sent",
        );
        write_line(&mut self.cmd_writer, &line)
    }

    /// Send a raw command line (no parsing). Use this for variants
    /// the [`OutboundCommand`] enum doesn't cover yet.
    pub fn send_raw(&mut self, line: &str) -> io::Result<()> {
        write_line(&mut self.cmd_writer, line)
    }

    /// Read one inbound command line. Returns `Ok(None)` on read
    /// timeout (when [`VaraConfig::read_timeout`] is set) or EOF.
    ///
    /// **Not suitable for liveness detection (tuxlink-6urh2 v2).** This fn
    /// collapses "peer closed" (EOF) and "peer idle" (timeout) to the same
    /// `Ok(None)`, which is correct for the command-exchange callers that
    /// only care "nothing more to read right now" — but a caller trying to
    /// distinguish "socket still open" from "socket dead" needs
    /// [`Self::recv_line_distinguishing_eof`] instead. See that fn's docs
    /// for why a non-consuming peek can't make this distinction reliably on
    /// VARA's cmd socket (unsolicited `IAMALIVE` / setter-echo `OK` lines
    /// buffer unread during the idle-open window and make a peek report
    /// "alive" forever, even after the peer's FIN).
    pub fn recv(&mut self) -> io::Result<Option<InboundCommand>> {
        match self.cmd_reader.read_line() {
            Ok(None) => Ok(None),
            Ok(Some(line)) => match InboundCommand::parse(&line) {
                Ok(cmd) => {
                    tracing::debug!(
                        target: "tuxlink::winlink::modem::vara",
                        command = %line,
                        "VARA command received",
                    );
                    Ok(Some(cmd))
                }
                Err(_e) => {
                    // Surface unknown / malformed as Unknown so the
                    // caller can decide whether to log + continue.
                    tracing::debug!(
                        target: "tuxlink::winlink::modem::vara",
                        raw_line = %line,
                        "VARA unknown command received",
                    );
                    Ok(Some(InboundCommand::Unknown(line)))
                }
            },
            Err(e) if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut => {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    /// Consuming, EOF-aware read for liveness detection (tuxlink-6urh2 v2 —
    /// replaces the non-consuming `peek_liveness` design). Distinguishes
    /// three outcomes that [`Self::recv`] collapses to `Ok(None)`:
    ///
    /// - [`RecvOutcome::Line`] — a full line was read (parsed, or wrapped
    ///   as [`InboundCommand::Unknown`] on a parse miss). Receiving ANY
    ///   byte-terminated line — even an unsolicited `IAMALIVE` keepalive or
    ///   a setter echo — proves the peer is alive.
    /// - [`RecvOutcome::Idle`] — the read timed out
    ///   ([`VaraConfig::read_timeout`]) with nothing buffered. The socket
    ///   is still open; the peer just hasn't sent anything this tick.
    /// - [`RecvOutcome::Eof`] — [`super::wire::LineReader::read_line`]
    ///   returned `Ok(None)` because the underlying `read` returned
    ///   `Ok(0)`: the peer sent FIN. **This is the case a peek cannot
    ///   distinguish from `Idle`** — VARA's idle-open window has
    ///   unsolicited `IAMALIVE` / `OK` lines sitting in the kernel receive
    ///   buffer unread by any consumer, so a `MSG_PEEK` always finds
    ///   *something* buffered (or, once drained, `WouldBlock`) and reports
    ///   "alive" even after the peer has actually closed the socket — the
    ///   flaw this fn's consuming design fixes.
    /// - [`RecvOutcome::Err`] — the read failed with an I/O error other
    ///   than a timeout (e.g. `ECONNRESET`).
    pub fn recv_line_distinguishing_eof(&mut self) -> RecvOutcome {
        match self.cmd_reader.read_line() {
            Ok(None) => RecvOutcome::Eof,
            Ok(Some(line)) => match InboundCommand::parse(&line) {
                Ok(cmd) => RecvOutcome::Line(cmd),
                Err(_e) => {
                    // Garbage/unparseable content is still a byte the peer
                    // sent — it proves liveness just as much as a
                    // recognized command does. Wrap as Unknown so the
                    // caller always sees a Line variant either way (mirrors
                    // `recv`'s Unknown-wrapping posture above).
                    RecvOutcome::Line(InboundCommand::Unknown(line))
                }
            },
            Err(e)
                if e.kind() == io::ErrorKind::WouldBlock
                    || e.kind() == io::ErrorKind::TimedOut
                    // EINTR: a signal interrupted the read — not a peer close.
                    // std normally retries EINTR internally, so this is
                    // belt-and-suspenders, but a false `Dead` here would be a
                    // spurious SocketLost, so treat it as a live idle tick.
                    || e.kind() == io::ErrorKind::Interrupted
                    // InvalidData = LineReader read a full line but it wasn't
                    // valid UTF-8. The bytes were CONSUMED — a byte arrived, so
                    // the peer is alive. This honors the "any received byte
                    // proves liveness" contract (parse failures already wrap as
                    // Line(Unknown); a non-UTF8 line must NOT be a false FIN).
                    || e.kind() == io::ErrorKind::InvalidData =>
            {
                RecvOutcome::Idle
            }
            Err(e) => RecvOutcome::Err(e),
        }
    }

    /// Bounded, consuming liveness drain for the idle-open heartbeat
    /// (tuxlink-6urh2 v2). The heartbeat owns the transport EXCLUSIVELY during
    /// this call (`TransportOwner::Heartbeat`).
    ///
    /// The drain runs the shared cmd socket **non-blocking** for its duration:
    /// buffered `IAMALIVE` / setter-echo `OK` lines (each proves liveness) are
    /// consumed until the kernel buffer is momentarily empty, which surfaces as
    /// `WouldBlock` -> [`RecvOutcome::Idle`] -> alive IMMEDIATELY — it does NOT
    /// block waiting for the *next* keepalive. A peer FIN surfaces as a 0-byte
    /// read -> [`RecvOutcome::Eof`] -> dead, also immediately. Reads at most
    /// `cap` lines as a backstop against a pathological in-buffer stream.
    /// Returns `true` = alive, `false` = dead (Eof/Err).
    ///
    /// Why non-blocking rather than a lowered blocking timeout: a peer that
    /// streams keepalives *faster* than any blocking `probe_timeout` never
    /// yields an idle gap, so a blocking drain would consume line-by-line up to
    /// `cap` — holding the borrow (and, called from the async heartbeat task,
    /// the runtime thread) for `cap x inter-line-gap`, and eventually reading a
    /// finite peer's natural FIN as a false drop. Non-blocking bounds the
    /// borrow to the buffered backlog (microseconds), independent of peer
    /// chattiness.
    ///
    /// `O_NONBLOCK` is a file-status flag on the shared open file description,
    /// so toggling it via `cmd_writer` also governs `cmd_reader`'s reads (the
    /// two are dup'd fds of one socket). Exclusive heartbeat ownership means no
    /// concurrent reader observes the non-blocking window, and blocking mode is
    /// restored before the transport is re-installed. The `_probe_timeout` arg
    /// is retained for call-site/signature stability but is unused: `SO_RCVTIMEO`
    /// is irrelevant while the socket is non-blocking.
    pub fn probe_liveness_draining(&mut self, _probe_timeout: Duration, cap: usize) -> bool {
        // Best-effort: if the mode toggle fails the drain still classifies
        // correctly, it just falls back to the socket's blocking read timeout.
        let _ = self.cmd_writer.set_nonblocking(true);
        let mut alive = true;
        for _ in 0..cap {
            match self.recv_line_distinguishing_eof() {
                RecvOutcome::Line(_) => continue, // buffered keepalive/echo — alive
                RecvOutcome::Idle => break,        // buffer empty, no FIN — alive
                RecvOutcome::Eof | RecvOutcome::Err(_) => {
                    alive = false; // peer FIN or hard error — dead
                    break;
                }
            }
        }
        // Restore blocking mode so the exchange path's reads honor SO_RCVTIMEO.
        let _ = self.cmd_writer.set_nonblocking(false);
        alive
    }

    /// Borrowed access to the connected-mode data byte stream.
    /// Read/write directly for the B2F session layer.
    pub fn data_stream(&mut self) -> &mut TcpStream {
        &mut self.data_stream
    }

    /// Borrowed access to the cmd-socket writer for advanced uses
    /// (the session layer normally calls [`send`] instead).
    pub fn cmd_writer(&mut self) -> &mut TcpStream {
        &mut self.cmd_writer
    }

    /// Configuration the transport was opened with.
    pub fn config(&self) -> &VaraConfig {
        &self.cfg
    }

    /// Flush + close both sockets. Best-effort; errors are logged
    /// upstream.
    pub fn close(mut self) -> io::Result<()> {
        let _ = self.cmd_writer.flush();
        let _ = self.data_stream.flush();
        Ok(())
    }

    /// Return a side-channel writer + hard-close stream pair pointed at the
    /// VARA cmd port (tuxlink-0ye6 Task 4.1 — spec §9 + Codex Round 1 P1 #4).
    ///
    /// The cooperative writer is a clone of the cmd-port write half with a
    /// bounded [`ABORT_WRITE_TIMEOUT`] so a wedged VARA modem cannot stall
    /// the abort budget. The stream is a separate clone of the same socket
    /// used by [`VaraSession::abort_in_flight`]'s fallback path to call
    /// `shutdown_both` when the cooperative `ABORT\r` write fails — a
    /// TCP RST forces VARA to notice the teardown and halt TX on its end
    /// even when the cmd port itself is unresponsive (Codex Round 4 P1 #3).
    ///
    /// Returns `Err` if either `try_clone` fails — the caller can fall
    /// through to the graceful `OutboundCommand::Disconnect` path.
    ///
    /// **`ABORT` vs `DISCONNECT`:** the VARA cmd codec models both
    /// commands distinctly (see [`super::command::OutboundCommand`]):
    /// `ABORT` is hard tear-down (interrupts in-flight TX within ~2s);
    /// `DISCONNECT` is graceful (waits for the current burst to finish,
    /// can be slow on weak-signal modes). This handle's `ABORT\r` is the
    /// only path that fits spec §2's interrupt contract.
    pub fn try_clone_abort_writer(
        &self,
    ) -> io::Result<(Box<dyn Write + Send>, Box<dyn ShutdownableStream>)> {
        let writer = self.cmd_writer.try_clone()?;
        // Bound the cooperative write so a wedged peer can't stall the
        // abort path past spec §2's ~2s contract. Best-effort: if the
        // socket rejects the timeout, fall through unbounded — the
        // hard-close fallback still bounds the overall budget.
        let _ = writer.set_write_timeout(Some(ABORT_WRITE_TIMEOUT));
        let stream_clone = writer.try_clone()?;
        Ok((
            Box::new(writer) as Box<dyn Write + Send>,
            Box::new(stream_clone) as Box<dyn ShutdownableStream>,
        ))
    }
}

/// Outcome of [`VaraTransport::recv_line_distinguishing_eof`] — a
/// consuming, EOF-aware read used for liveness detection (tuxlink-6urh2 v2).
/// See that fn's doc comment for the full outcome mapping.
#[derive(Debug)]
pub enum RecvOutcome {
    /// A full command line was read — parsed, or wrapped as
    /// [`InboundCommand::Unknown`] on a parse miss. Any line at all proves
    /// the peer is alive.
    Line(InboundCommand),
    /// The read timed out with nothing buffered. Socket still open.
    Idle,
    /// The peer sent FIN (`read` returned `Ok(0)`). Socket is dead.
    Eof,
    /// The read failed with an I/O error other than a timeout.
    Err(io::Error),
}

/// Read-only TCP reachability touch on VARA's COMMAND port (tuxlink-7ppfq,
/// Contract 1). Opens a socket, then immediately shuts it down — issues NO
/// VARA command, so it never mutates modem state (unlike MYCALL/BW/LISTEN).
///
/// `cmd`-reachable is NOT "ready to send": 8300 can accept while 8301 (data)
/// still lags on a WINE restart. Callers name/describe this as cmd-port
/// reachability, not "usable session."
pub fn cmd_port_reachable(host: &str, cmd_port: u16, timeout: Duration) -> bool {
    let Ok(mut addrs) = (host, cmd_port).to_socket_addrs() else {
        return false;
    };
    let Some(addr) = addrs.next() else {
        return false;
    };
    match TcpStream::connect_timeout(&addr, timeout) {
        Ok(stream) => {
            // Explicit shutdown so we never leave a half-open connection on
            // VARA's single-App acceptor.
            let _ = stream.shutdown(std::net::Shutdown::Both);
            true
        }
        Err(_) => false,
    }
}

/// Outcome of the read-only deep probe ([`deep_probe`]). A transport-layer type
/// (the MCP `VaraProbeDto` is built from it in `mcp_ports`) so this module does
/// not depend upward on the MCP DTO crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaraProbeResult {
    /// `"down"` (no TCP), `"socket-not-vara"` (something answered but is not
    /// VARA), or `"vara-ok"` (a real VARA banner / VERSION reply).
    pub classification: String,
    /// The trimmed banner / VERSION reply text, when any bytes were read.
    pub banner: Option<String>,
}

/// READ-ONLY deep probe (tuxlink-7ppfq, Contract 1): connect the cmd port, read
/// VARA's startup banner, and — only if that banner does not already identify
/// VARA — send a single `VERSION` query (a pure read; it does NOT mutate modem
/// state, unlike MYCALL/BW/LISTEN) and read the reply. Mirrors the setup
/// engine's `wv_wait_ports` verify handshake. Never opens the data port, never
/// sends a stateful setter, never keys a radio.
pub fn deep_probe(cfg: &VaraConfig) -> VaraProbeResult {
    use std::io::Read;
    let addr = match (cfg.host.as_str(), cfg.cmd_port)
        .to_socket_addrs()
        .ok()
        .and_then(|mut a| a.next())
    {
        Some(a) => a,
        None => {
            return VaraProbeResult {
                classification: "down".into(),
                banner: None,
            }
        }
    };
    let mut stream = match TcpStream::connect_timeout(&addr, cfg.connect_timeout) {
        Ok(s) => s,
        Err(_) => {
            return VaraProbeResult {
                classification: "down".into(),
                banner: None,
            }
        }
    };
    let _ = stream.set_read_timeout(
        cfg.read_timeout
            .or_else(|| Some(Duration::from_millis(500))),
    );
    let mut acc = String::new();
    let mut buf = [0u8; 512];
    // Drain any startup banner first (read-only).
    if let Ok(n) = stream.read(&mut buf) {
        acc.push_str(&String::from_utf8_lossy(&buf[..n]));
    }
    if !acc.to_uppercase().contains("VARA") {
        // Single read-only VERSION query (CR terminator matches VARA's codec).
        let _ = stream.write_all(b"VERSION\r");
        if let Ok(n) = stream.read(&mut buf) {
            acc.push_str(&String::from_utf8_lossy(&buf[..n]));
        }
    }
    let _ = stream.shutdown(std::net::Shutdown::Both);
    let banner = acc.trim().to_string();
    let classification = if banner.to_uppercase().contains("VARA") {
        "vara-ok"
    } else {
        "socket-not-vara"
    };
    VaraProbeResult {
        classification: classification.into(),
        banner: if banner.is_empty() {
            None
        } else {
            Some(banner)
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn cmd_port_reachable_true_when_listener_bound() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
        let port = listener.local_addr().unwrap().port();
        assert!(cmd_port_reachable(
            "127.0.0.1",
            port,
            Duration::from_secs(5)
        ));
    }

    #[test]
    fn cmd_port_reachable_false_when_no_listener() {
        // Bind then drop to obtain a port nothing is listening on.
        let port = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        assert!(!cmd_port_reachable(
            "127.0.0.1",
            port,
            Duration::from_millis(500)
        ));
    }

    /// Fake VARA cmd-port acceptor: replies `reply` as a startup banner, then
    /// records every byte it subsequently receives so a test can assert the
    /// read-only probe sent NO stateful setter.
    fn spawn_fake_vara(reply: &'static str) -> (u16, std::sync::mpsc::Receiver<String>) {
        use std::io::{Read, Write};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = std::sync::mpsc::channel();
        thread::spawn(move || {
            if let Ok((mut sock, _)) = listener.accept() {
                // Short per-read timeout, but keep looping past timeouts up to an
                // overall deadline: with a SILENT banner the probe only sends
                // VERSION after ITS own banner-read timeout, so the fake must not
                // give up on the first idle read (else it misses the VERSION).
                sock.set_read_timeout(Some(Duration::from_millis(100))).ok();
                let _ = sock.write_all(reply.as_bytes());
                let mut buf = [0u8; 512];
                let mut seen = String::new();
                let start = std::time::Instant::now();
                while start.elapsed() < Duration::from_secs(2) {
                    match sock.read(&mut buf) {
                        Ok(0) => break, // peer (the probe) shut down
                        Ok(n) => seen.push_str(&String::from_utf8_lossy(&buf[..n])),
                        Err(e)
                            if e.kind() == std::io::ErrorKind::WouldBlock
                                || e.kind() == std::io::ErrorKind::TimedOut =>
                        {
                            continue
                        }
                        Err(_) => break,
                    }
                }
                let _ = tx.send(seen);
            }
        });
        (port, rx)
    }

    fn probe_cfg(port: u16) -> VaraConfig {
        VaraConfig {
            host: "127.0.0.1".into(),
            cmd_port: port,
            data_port: port,
            connect_timeout: Duration::from_secs(5),
            read_timeout: Some(Duration::from_millis(300)),
            data_read_timeout: Some(Duration::from_millis(300)),
        }
    }

    #[test]
    fn deep_probe_classifies_vara_ok_and_sends_no_setter() {
        let (port, rx) = spawn_fake_vara("VARA HF v4.8.6 Ready\r");
        let result = deep_probe(&probe_cfg(port));
        assert_eq!(result.classification, "vara-ok");
        assert!(result
            .banner
            .unwrap_or_default()
            .to_uppercase()
            .contains("VARA"));
        // The banner already identified VARA, so the probe must not have sent
        // VERSION or any setter. Assert NOTHING mutating crossed the wire.
        let seen = rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap_or_default();
        let up = seen.to_uppercase();
        assert!(!up.contains("MYCALL"), "probe must not send MYCALL");
        assert!(!up.contains("BW"), "probe must not send BW");
        assert!(!up.contains("LISTEN"), "probe must not send LISTEN");
    }

    #[test]
    fn deep_probe_sends_version_only_when_banner_silent() {
        // Empty banner → probe falls back to a single VERSION query (read-only).
        let (port, rx) = spawn_fake_vara("");
        let _ = deep_probe(&probe_cfg(port));
        let seen = rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap_or_default();
        let up = seen.to_uppercase();
        assert!(up.contains("VERSION"), "expected the read-only VERSION query");
        assert!(!up.contains("MYCALL") && !up.contains("LISTEN"), "no setter");
    }

    #[test]
    fn deep_probe_socket_not_vara() {
        let (port, _rx) = spawn_fake_vara("gibberish\r");
        assert_eq!(deep_probe(&probe_cfg(port)).classification, "socket-not-vara");
    }

    #[test]
    fn deep_probe_down_when_no_listener() {
        let port = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let cfg = VaraConfig {
            connect_timeout: Duration::from_millis(500),
            ..probe_cfg(port)
        };
        assert_eq!(deep_probe(&cfg).classification, "down");
    }

    /// Connect a real `VaraTransport` against loopback cmd + data acceptors,
    /// returning the transport plus the server-side cmd-socket handle (so
    /// the test can write lines / close it to drive the client's read
    /// outcomes). Mirrors the loopback pattern used throughout
    /// `commands.rs`'s test module — a real TCP pair, not a mock trait, so
    /// the EOF-vs-timeout distinction is exercised at the actual socket
    /// layer rather than an in-memory stand-in that could paper over a
    /// kernel-level subtlety.
    fn loopback_pair(read_timeout: Duration) -> (VaraTransport, TcpStream) {
        let cmd_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let cmd_port = cmd_l.local_addr().unwrap().port();
        let data_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let data_port = data_l.local_addr().unwrap().port();

        let cmd_accept = thread::spawn(move || cmd_l.accept().unwrap().0);
        let data_accept = thread::spawn(move || data_l.accept().unwrap().0);

        let cfg = VaraConfig {
            host: "127.0.0.1".into(),
            cmd_port,
            data_port,
            connect_timeout: Duration::from_secs(2),
            read_timeout: Some(read_timeout),
            data_read_timeout: Some(read_timeout),
        };
        let transport = VaraTransport::connect(cfg).expect("connect must succeed");
        let cmd_server = cmd_accept.join().unwrap();
        let _data_server = data_accept.join().unwrap();
        (transport, cmd_server)
    }

    /// tuxlink-6urh2 v2: the three outcomes a liveness-checking caller
    /// needs distinguished, in sequence against ONE live socket pair —
    /// Idle (nothing sent, timeout fires), Line (a buffered `IAMALIVE`
    /// keepalive, the exact line class the flawed peek design mistook for
    /// "forever alive"), then Eof once the peer actually closes.
    #[test]
    fn recv_line_distinguishing_eof_classifies_idle_line_then_eof() {
        let (mut transport, mut server) = loopback_pair(Duration::from_millis(100));

        // Idle: nothing sent yet — the bounded read times out with nothing
        // buffered.
        match transport.recv_line_distinguishing_eof() {
            RecvOutcome::Idle => {}
            other => panic!("expected Idle, got {other:?}"),
        }

        // Line: an unsolicited IAMALIVE keepalive arrives — the exact
        // buffered-line class that a non-consuming peek would also report
        // as "alive," but here it's CONSUMED (this is the load-bearing
        // difference from the old design: recv() drains it so a
        // subsequent read can observe the peer's eventual FIN instead of
        // perpetually re-peeking the same buffered bytes).
        server.write_all(b"IAMALIVE\r").unwrap();
        server.flush().unwrap();
        match transport.recv_line_distinguishing_eof() {
            RecvOutcome::Line(InboundCommand::IAmAlive) => {}
            other => panic!("expected Line(IAmAlive), got {other:?}"),
        }

        // Eof: the peer closes — must be distinguished from Idle, not
        // folded into it.
        drop(server);
        match transport.recv_line_distinguishing_eof() {
            RecvOutcome::Eof => {}
            other => panic!("expected Eof, got {other:?}"),
        }
    }

    /// A garbage/unparseable line still counts as proof of liveness — it's
    /// wrapped as `Unknown` rather than surfacing as an error, so a
    /// liveness-checking drain loop doesn't misclassify a malformed-but-
    /// present byte stream as "dead."
    #[test]
    fn recv_line_distinguishing_eof_wraps_unparseable_as_unknown_line() {
        let (mut transport, mut server) = loopback_pair(Duration::from_millis(200));
        server.write_all(b"SOMETHING NOVEL\r").unwrap();
        server.flush().unwrap();
        match transport.recv_line_distinguishing_eof() {
            RecvOutcome::Line(InboundCommand::Unknown(s)) => {
                assert_eq!(s, "SOMETHING NOVEL");
            }
            other => panic!("expected Line(Unknown), got {other:?}"),
        }
    }

    /// tuxlink-xzxk1: the cmd and data sockets get INDEPENDENT read
    /// timeouts — cmd at event cadence, data at RF scale. Asserted via the
    /// kernel (`TcpStream::read_timeout`), not the config struct, so a
    /// regression in `connect()`'s plumbing (e.g. reverting the data socket
    /// to `cfg.read_timeout`) fails even if the config fields are right.
    #[test]
    fn data_socket_gets_its_own_rf_scale_read_timeout() {
        let cmd_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let data_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let cfg = VaraConfig {
            host: "127.0.0.1".into(),
            cmd_port: cmd_l.local_addr().unwrap().port(),
            data_port: data_l.local_addr().unwrap().port(),
            connect_timeout: Duration::from_secs(2),
            read_timeout: Some(Duration::from_millis(250)),
            data_read_timeout: Some(Duration::from_secs(90)),
        };
        let cmd_accept = thread::spawn(move || cmd_l.accept().unwrap().0);
        let data_accept = thread::spawn(move || data_l.accept().unwrap().0);
        let transport = VaraTransport::connect(cfg).expect("connect must succeed");
        let _cmd_server = cmd_accept.join().unwrap();
        let _data_server = data_accept.join().unwrap();

        assert_eq!(
            transport.data_stream().read_timeout().unwrap(),
            Some(Duration::from_secs(90)),
            "data socket must carry data_read_timeout, not the cmd cadence"
        );
        // And the clone the B2F exchange actually reads from shares it
        // (SO_RCVTIMEO is a socket option, not per-fd state).
        let clone = transport.data_stream().try_clone().unwrap();
        assert_eq!(clone.read_timeout().unwrap(), Some(Duration::from_secs(90)));
    }
}
