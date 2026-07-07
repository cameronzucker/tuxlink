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
}

impl Default for VaraConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            cmd_port: 8300,
            data_port: 8301,
            connect_timeout: Duration::from_secs(5),
            read_timeout: Some(Duration::from_secs(2)),
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
        data_stream.set_read_timeout(cfg.read_timeout)?;

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
                    || e.kind() == io::ErrorKind::Interrupted =>
            {
                RecvOutcome::Idle
            }
            Err(e) => RecvOutcome::Err(e),
        }
    }

    /// Bounded, consuming liveness drain for the idle-open heartbeat
    /// (tuxlink-6urh2 v2). The heartbeat owns the transport EXCLUSIVELY during
    /// this call (`TransportOwner::Heartbeat`), so it temporarily lowers the
    /// cmd read timeout to `probe_timeout` (restored before returning): an
    /// "open but idle" verdict then costs ~`probe_timeout` instead of the full
    /// 2s command `read_timeout`, bounding how long the heartbeat holds the
    /// borrowed transport away from a would-be exchange. The drain consumes
    /// buffered `IAMALIVE` / setter-echo `OK` lines (each proves liveness); a
    /// peer FIN surfaces as `Eof` IMMEDIATELY regardless of the timeout. Reads
    /// at most `cap` lines. Returns `true` = alive, `false` = dead (Eof/Err).
    ///
    /// Safe re: the socket-level `SO_RCVTIMEO` shared across the dup'd cmd fds
    /// — exclusive heartbeat ownership means no concurrent reader observes the
    /// lowered timeout, and it is restored before the transport is re-installed.
    pub fn probe_liveness_draining(&mut self, probe_timeout: Duration, cap: usize) -> bool {
        let restore = self.cfg.read_timeout;
        // Best-effort: if lowering the timeout fails the drain still works, it
        // just falls back to the original (longer) per-read wait.
        let _ = self.cmd_writer.set_read_timeout(Some(probe_timeout));
        let mut alive = true;
        for _ in 0..cap {
            match self.recv_line_distinguishing_eof() {
                RecvOutcome::Line(_) => continue, // buffered keepalive/echo — alive
                RecvOutcome::Idle => break,        // drained, socket still open — alive
                RecvOutcome::Eof | RecvOutcome::Err(_) => {
                    alive = false; // peer FIN or hard error — dead
                    break;
                }
            }
        }
        let _ = self.cmd_writer.set_read_timeout(restore);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

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
}
