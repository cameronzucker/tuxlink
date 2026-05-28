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

use super::command::{encode_setter, Command, CommandParseError, State};
use super::wire::encode_cmd_line;

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
}

impl CmdSocket {
    /// Open the ARDOP cmd socket at `addr` and start the reader thread.
    ///
    /// On success returns a `CmdSocket` whose control-loop thread is already
    /// running and forwarding parsed `Command` values into the internal channel.
    pub fn connect(addr: SocketAddr) -> io::Result<Self> {
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
                match Command::parse(&line) {
                    Ok(cmd) => {
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
            // `tx` drops here → channel becomes disconnected → recv_event returns
            // `RecvError::Disconnected`.
        });

        Ok(CmdSocket {
            writer,
            rx,
            reader_thread: Some(reader_thread),
        })
    }

    /// Write `line` to the cmd socket, appending the required `\r` terminator.
    pub fn send_line(&mut self, line: &str) -> io::Result<()> {
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
/// 6. `MYCALL <call>`
/// 7. `GRIDSQUARE <grid>`
///
/// For each setter: sends the encoded line, then consumes events from the channel
/// until the matching `EchoBack(cmd)` ack arrives — tolerating interleaved async
/// events (NewState/Ptt/Busy/Buffer/etc.) — or a `Fault` (→ `SessionError::Fault`).
/// Returns `Err(SessionError::Timeout)` if the ack does not arrive within
/// [`SETTER_ACK_TIMEOUT`].
pub fn init_tnc(sock: &mut CmdSocket, cfg: &InitConfig) -> Result<(), SessionError> {
    set_and_ack(sock, "INITIALIZE", None)?;
    set_and_ack(sock, "CODEC", Some("TRUE"))?;
    set_and_ack(sock, "PROTOCOLMODE", Some("ARQ"))?;
    set_and_ack(sock, "ARQTIMEOUT", Some(&cfg.arq_timeout_s.to_string()))?;
    set_and_ack(sock, "LISTEN", Some("FALSE"))?;
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
            Ok(
                Command::NewState(_)
                | Command::Ptt(_)
                | Command::Busy(_)
                | Command::Buffer(_)
                | Command::Status(_)
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
/// Sends `ARQCALL <target> <repeat>` and waits until the TNC emits
/// `CONNECTED <peer_call> <bw>` (success), `FAULT <msg>` (error), or
/// `DISCONNECTED`/`NEWSTATE DISC` (error — link dropped before connecting).
///
/// The `deadline` is an **overall** deadline from the time the function is
/// called; per-iteration remaining time is recomputed so the loop actually
/// terminates.
pub fn arq_connect(
    sock: &mut CmdSocket,
    target: &str,
    repeat: u32,
    deadline: Duration,
) -> Result<ConnectInfo, SessionError> {
    let start = Instant::now();
    sock.send_line(&encode_setter(
        "ARQCALL",
        Some(&format!("{target} {repeat}")),
    ))?;
    loop {
        let elapsed = start.elapsed();
        if elapsed >= deadline {
            return Err(SessionError::Timeout { cmd: "ARQCALL".into() });
        }
        let remaining = deadline - elapsed;
        match sock.recv_event(remaining) {
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
        };
        init_tnc(&mut sock, &cfg).expect("init must tolerate interleaved async events");

        let _ = sock.writer.shutdown(std::net::Shutdown::Write);
        server.join().unwrap();
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
        let info = arq_connect(&mut sock, "W7ABC", 3, Duration::from_secs(10))
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
        let err = arq_connect(&mut sock, "W7ABC", 3, Duration::from_secs(10))
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
        let err = arq_connect(&mut sock, "W7ABC", 3, Duration::from_secs(10))
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
        let err = arq_connect(&mut sock, "W7ABC", 3, Duration::from_secs(10))
            .expect_err("must fail on DISCONNECTED before CONNECTED");
        assert!(
            matches!(err, SessionError::Fault(_)),
            "expected Fault, got {err:?}"
        );
        drop(sock);
        server.join().unwrap();
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
