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
                Ok(cmd) => Ok(Some(cmd)),
                Err(_e) => {
                    // Surface unknown / malformed as Unknown so the
                    // caller can decide whether to log + continue.
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
}
