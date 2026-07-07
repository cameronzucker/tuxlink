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

    /// Clone the cmd-port socket for the drop-detection heartbeat
    /// (tuxlink-6urh2). Mirrors [`Self::try_clone_abort_writer`]'s
    /// clone-the-cmd-socket pattern, but the returned handle is used PURELY
    /// as input to [`Self::peek_liveness`] — never for writing — so it
    /// carries none of the abort writer's bounded `write_timeout` setup.
    pub fn try_clone_cmd_socket(&self) -> io::Result<TcpStream> {
        self.cmd_writer.try_clone()
    }

    /// Pure liveness probe for a cmd-port socket clone (tuxlink-6urh2). Peeks
    /// (does NOT consume) one byte to distinguish "peer closed" from "peer
    /// idle" without disturbing [`Self::recv`]'s byte stream:
    ///
    /// - `Ok(0)` — the peer sent FIN with nothing buffered: [`LivenessProbe::Dead`].
    /// - `Err(WouldBlock)` — no data buffered, socket still open: this is the
    ///   steady idle-open state: [`LivenessProbe::Alive`].
    /// - `Ok(n > 0)` — data is sitting in the receive buffer unread by
    ///   [`Self::recv`]: [`LivenessProbe::Alive`]. The peek does not consume
    ///   the byte, so a subsequent `recv()` still sees it — no corruption of
    ///   the command exchange.
    /// - Any other `Err` (`ECONNRESET`, etc.): [`LivenessProbe::Dead`].
    ///
    /// **No shared-socket-option mutation (load-bearing).** `TcpStream::try_clone`
    /// dup()s the fd, so the clone shares the underlying open file description —
    /// and thus `O_NONBLOCK` — with `cmd_writer` / `cmd_reader`. We therefore do
    /// NOT call `set_nonblocking`: its flag is shared and would flip the whole
    /// transport nonblocking, silently breaking `recv()`'s blocking-with-timeout
    /// contract. Instead the probe passes `MSG_DONTWAIT` — a PER-CALL nonblocking
    /// flag that affects only this one `recv` syscall and never touches the
    /// shared file description — combined with `MSG_PEEK` (no consume). So a
    /// concurrent `recv()`/`send()` on any clone is completely undisturbed, with
    /// no transient-nonblocking window to reason about.
    pub fn peek_liveness(sock: &TcpStream) -> LivenessProbe {
        use std::os::unix::io::AsRawFd;
        let mut buf = [0u8; 1];
        // SAFETY: `sock` is a live `TcpStream`, so its fd is valid for the
        // duration of this call; `recv` writes at most `buf.len()` bytes into
        // `buf`. MSG_PEEK leaves the byte in the kernel receive buffer (no
        // consume); MSG_DONTWAIT makes only this one call nonblocking.
        let n = unsafe {
            libc::recv(
                sock.as_raw_fd(),
                buf.as_mut_ptr() as *mut libc::c_void,
                buf.len(),
                libc::MSG_PEEK | libc::MSG_DONTWAIT,
            )
        };
        if n == 0 {
            // Peer sent FIN with nothing buffered — connection closed.
            LivenessProbe::Dead
        } else if n > 0 {
            // Data sitting unread by `recv()`; the peek did not consume it.
            LivenessProbe::Alive
        } else {
            match io::Error::last_os_error().kind() {
                // No data yet, socket open — the steady idle-open state.
                io::ErrorKind::WouldBlock => LivenessProbe::Alive,
                // Interrupted by a signal — inconclusive; retry next tick.
                io::ErrorKind::Interrupted => LivenessProbe::Alive,
                // ECONNRESET / EPIPE / EBADF etc. — treat as dead.
                _ => LivenessProbe::Dead,
            }
        }
    }
}

/// Liveness classification returned by [`VaraTransport::peek_liveness`].
/// See that fn's doc comment for the exact `peek()` outcome mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LivenessProbe {
    /// Socket is idle-alive (`WouldBlock`) or has unread data buffered
    /// (`Ok(n > 0)`).
    Alive,
    /// Peer closed the connection (`Ok(0)`) or the peek failed with an
    /// error other than `WouldBlock`.
    Dead,
}
