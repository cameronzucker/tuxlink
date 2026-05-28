//! `ArdopTransport` — implements [`ModemTransport`] for ardopcf (and any
//! ARDOP-compatible TNC) over the standard dual-TCP-socket host protocol.
//!
//! `with_addrs` constructs an unconnected transport; `init` opens both the
//! command socket and the data socket and runs the ARDOP init sequence.
//! After a successful `connect_arq` the `data_stream` accessor exposes the
//! `DataSocket` as `&mut dyn ReadWrite` for consumption by the sync B2F
//! `run_exchange`.
//!
//! Phase 5 adds `with_managed_modem` and `shutdown` for the full
//! tuxlink-owns-the-process lifecycle (ADR 0015 decision #2).

use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::super::process::{ManagedModem, ProcessError};
use super::data::DataSocket;
use super::session::{arq_connect, arq_disconnect, init_tnc, CmdSocket, ConnectInfo, InitConfig, SessionError};
use super::ArdopConfig;
use crate::winlink::modem::{ModemTransport, ReadWrite};

/// How long to wait (total) for ardopcf to bind both TCP ports after spawn.
const BIND_WAIT_TIMEOUT: Duration = Duration::from_secs(5);
/// Interval between retry attempts while waiting for ports to open.
const BIND_WAIT_POLL_INTERVAL: Duration = Duration::from_millis(100);

// ─── ArdopTransport ─────────────────────────────────────────────────────────

/// [`ModemTransport`] implementation for ardopcf (ARDOP TNC).
///
/// Drives ardopcf's dual-TCP host protocol:
/// - cmd socket (typically 8515): `\r`-terminated ASCII command lines.
/// - data socket (typically 8516): ARQ-framed inbound, raw bytes outbound.
///
/// # Lifecycle (external TNC — `with_addrs`)
///
/// ```text
/// with_addrs(cmd_addr, data_addr)   ← no I/O; sockets not yet open
///   .init(cfg)                      ← opens CmdSocket + DataSocket, runs init sequence
///   .connect_arq(target, n, t)      ← ARQCALL handshake → ConnectInfo
///   .data_stream()                  ← Read + Write for B2F exchange
///   .disconnect(t)                  ← DISCONNECT command + confirmation
/// ```
///
/// # Lifecycle (managed TNC — `with_managed_modem`)
///
/// ```text
/// with_managed_modem(cfg)           ← spawns ardopcf, bind-waits for both ports
///   .init(cfg)                      ← same as above
///   ...
///   .shutdown()                     ← disconnect + close sockets + stop process + audio-release check
/// ```
pub struct ArdopTransport {
    cmd_addr: SocketAddr,
    data_addr: SocketAddr,
    cmd: Option<CmdSocket>,
    data: Option<DataSocket>,
    /// Present only when tuxlink spawned and owns the TNC process.
    /// Tuple: (supervisor, optional audio-device path for release check).
    managed: Option<(ManagedModem, Option<PathBuf>)>,
}

impl ArdopTransport {
    /// Construct an `ArdopTransport` pointing at `cmd_addr` and `data_addr`.
    ///
    /// No I/O happens here — sockets are opened lazily in [`ModemTransport::init`].
    pub fn with_addrs(cmd_addr: SocketAddr, data_addr: SocketAddr) -> Self {
        ArdopTransport {
            cmd_addr,
            data_addr,
            cmd: None,
            data: None,
            managed: None,
        }
    }

    /// Spawn the ardopcf binary described by `cfg`, wait for both TCP ports to
    /// accept connections, then return a transport ready for `init`.
    ///
    /// `cfg.extra_args` is passed verbatim to the binary — the caller packs all
    /// needed arguments (including cmd_port, capture, and playback device names).
    ///
    /// # Bind-wait
    ///
    /// After spawning, the function loops trying `TcpStream::connect` to both
    /// `cmd_port` and `data_port` (loopback). Both must accept before
    /// [`BIND_WAIT_TIMEOUT`] elapses; otherwise returns
    /// `SessionError::Io(ErrorKind::TimedOut)`.
    ///
    /// # RADIO-1
    ///
    /// The caller must obtain per-invocation operator consent before calling
    /// this function — spawning ardopcf can eventually key the radio.
    pub fn with_managed_modem(cfg: ArdopConfig) -> Result<Self, SessionError> {
        Self::with_managed_modem_timeout(cfg, BIND_WAIT_TIMEOUT)
    }

    /// Like `with_managed_modem` but with a caller-specified bind-wait timeout.
    /// Exposed for tests that need a short timeout to keep the test suite fast.
    pub fn with_managed_modem_timeout(
        cfg: ArdopConfig,
        bind_wait: Duration,
    ) -> Result<Self, SessionError> {
        // Pass extra_args verbatim to the binary. The caller is responsible for
        // packing ardopcf's positional args (cmd_port, capture, playback) into
        // extra_args — the CLI example does exactly this. cmd_port and data_port
        // fields on ArdopConfig are used exclusively for the bind-wait and the
        // transport socket addresses.
        let binary_str = cfg.binary.to_string_lossy().into_owned();
        let args_refs: Vec<&str> = cfg.extra_args.iter().map(|s| s.as_str()).collect();

        let modem = ManagedModem::spawn(&binary_str, &args_refs)
            .map_err(|e: ProcessError| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("failed to spawn modem: {e}"),
                )
            })?;

        let cmd_addr: SocketAddr = format!("127.0.0.1:{}", cfg.cmd_port)
            .parse()
            .expect("cmd_addr parse is infallible for valid u16");
        let data_addr: SocketAddr = format!("127.0.0.1:{}", cfg.data_port)
            .parse()
            .expect("data_addr parse is infallible for valid u16");

        // Bind-wait: loop until both ports are bound by the ardopcf process, or timeout.
        //
        // Detection strategy: attempt to bind a new loopback socket to the same
        // port. If binding fails with EADDRINUSE (AddressInUse), ardopcf is already
        // listening on that port. This avoids consuming a connection slot (unlike
        // TcpStream::connect), which would prevent the real `init` connection from
        // being accepted.
        let start = Instant::now();
        loop {
            let cmd_ok = std::net::TcpListener::bind(cmd_addr).is_err();
            let data_ok = std::net::TcpListener::bind(data_addr).is_err();
            if cmd_ok && data_ok {
                break;
            }
            if start.elapsed() >= bind_wait {
                return Err(SessionError::Io(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!(
                        "ardopcf did not bind ports {} and {} within {:?}",
                        cfg.cmd_port, cfg.data_port, bind_wait
                    ),
                )));
            }
            std::thread::sleep(BIND_WAIT_POLL_INTERVAL);
        }

        Ok(ArdopTransport {
            cmd_addr,
            data_addr,
            cmd: None,
            data: None,
            managed: Some((modem, cfg.audio_device_path)),
        })
    }

    /// Tear down the full transport + process lifecycle.
    ///
    /// Steps (each best-effort; errors are accumulated but the sequence
    /// always completes):
    ///
    /// 1. Best-effort `DISCONNECT` on the cmd socket (ignores errors — the TNC
    ///    process is about to be killed regardless).
    /// 2. Drop both sockets (their `Drop` implementations close the TCP streams
    ///    and join background threads).
    /// 3. If a managed process is held: `ManagedModem::stop(~3s)`.
    /// 4. If an `audio_device_path` was configured:
    ///    `confirm_audio_device_released(path, ~2s)`. Returns
    ///    `Err(SessionError::Io(WouldBlock))` if the device is still held
    ///    after the deadline — the ADR-0015 swap invariant.
    ///
    /// # Idempotent
    ///
    /// Safe to call on a partially-initialized transport (e.g., `with_addrs`
    /// without `init` — all `Option` fields are just `None`-checked).
    pub fn shutdown(&mut self) -> Result<(), SessionError> {
        // Step 1: best-effort ARQ disconnect.
        if let Some(ref mut cmd) = self.cmd {
            let _ = arq_disconnect(cmd, Duration::from_secs(5));
        }

        // Step 2: drop sockets.
        self.cmd = None;
        self.data = None;

        // Step 3 + 4: stop process and check audio release.
        if let Some((ref mut modem, ref audio_path)) = self.managed {
            modem.stop(Duration::from_secs(3)).map_err(|e| {
                io::Error::new(io::ErrorKind::Other, format!("modem stop failed: {e}"))
            })?;

            if let Some(path) = audio_path {
                if !ManagedModem::confirm_audio_device_released(path, Duration::from_secs(2)) {
                    return Err(SessionError::Io(io::Error::new(
                        io::ErrorKind::WouldBlock,
                        format!(
                            "audio device {:?} still held after shutdown — swap invariant violated",
                            path
                        ),
                    )));
                }
            }
        }

        Ok(())
    }

    /// Return a reference to the live `CmdSocket`, or an `Err` if `init` has
    /// not been called.
    fn cmd_or_err(&mut self) -> Result<&mut CmdSocket, SessionError> {
        self.cmd.as_mut().ok_or_else(|| {
            SessionError::Io(io::Error::new(
                io::ErrorKind::NotConnected,
                "ArdopTransport: init() has not been called",
            ))
        })
    }
}

impl ModemTransport for ArdopTransport {
    /// Open the cmd and data sockets, then run the ARDOP TNC init sequence.
    ///
    /// Replaces any previously-open sockets (idempotent re-init).
    fn init(&mut self, cfg: &InitConfig) -> Result<(), SessionError> {
        // Hold the sockets as locals and run init_tnc on the local cmd socket: if
        // any step fails, the locals drop (CmdSocket::Drop shuts down + joins its
        // reader thread; DataSocket closes its TcpStream), leaving `self` in a
        // clean uninit state for an idempotent re-init — and avoiding an unwrap on
        // a just-stored Option. (Code review Phase 3.)
        let mut cmd = CmdSocket::connect(self.cmd_addr)?;
        let data = DataSocket::connect(self.data_addr)?;
        init_tnc(&mut cmd, cfg)?;
        self.cmd = Some(cmd);
        self.data = Some(data);
        Ok(())
    }

    /// Initiate an ARQ connection to `target` with `repeat` retries, bounded by
    /// `deadline`.
    ///
    /// Returns `Err` if [`init`] was not called first.
    fn connect_arq(
        &mut self,
        target: &str,
        repeat: u32,
        deadline: Duration,
    ) -> Result<ConnectInfo, SessionError> {
        let cmd = self.cmd_or_err()?;
        arq_connect(cmd, target, repeat, deadline)
    }

    /// Send `DISCONNECT` and wait for the TNC to confirm the link is torn down.
    ///
    /// Returns `Err` if [`init`] was not called first.
    fn disconnect(&mut self, deadline: Duration) -> Result<(), SessionError> {
        let cmd = self.cmd_or_err()?;
        arq_disconnect(cmd, deadline)
    }

    /// Return the data byte stream for the connected ARQ session.
    ///
    /// Returns `Err(NotConnected)` if [`init`] was not called, so callers get
    /// a clear error rather than a panic.
    fn data_stream(&mut self) -> io::Result<&mut dyn ReadWrite> {
        self.data
            .as_mut()
            .map(|d| d as &mut dyn ReadWrite)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotConnected,
                    "ArdopTransport: init() has not been called — data socket not open",
                )
            })
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};
    use std::thread;

    // ── Mock server helpers ───────────────────────────────────────────────

    /// Bind a loopback listener, spawn a server thread, return (addr, handle).
    /// The accepted connection gets a 2-second read timeout so server threads
    /// exit promptly instead of blocking forever.
    fn spawn_server<F>(handler: F) -> (SocketAddr, thread::JoinHandle<()>)
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

    // ── Mock CMD server ───────────────────────────────────────────────────

    /// Read one `\r`-terminated line from `conn` (strips the `\r`).
    /// Returns an empty string on EOF or timeout.
    fn read_cmd_line(reader: &mut BufReader<TcpStream>) -> String {
        let mut buf = Vec::new();
        match reader.read_until(b'\r', &mut buf) {
            Ok(0) | Err(_) => return String::new(),
            Ok(_) => {}
        }
        if buf.last() == Some(&b'\r') {
            buf.pop();
        }
        String::from_utf8(buf).unwrap_or_default()
    }

    /// Write `line\r` to the connection (TNC → client direction).
    fn write_reply(conn: &mut TcpStream, line: &str) {
        let _ = conn.write_all(format!("{line}\r").as_bytes());
    }

    /// Spawn a mock CMD server that:
    /// 1. Echoes the command name for each of the 7 init setters.
    /// 2. On `ARQCALL ...` replies: echo-back → `NEWSTATE ISS` → `CONNECTED <peer> <bw>`.
    /// 3. On `DISCONNECT` replies: `DISCONNECTED`.
    ///
    /// `peer_call` and `bandwidth_hz` are baked into the `CONNECTED` reply.
    fn spawn_mock_cmd_server(
        peer_call: &'static str,
        bandwidth_hz: u32,
    ) -> (SocketAddr, thread::JoinHandle<()>) {
        spawn_server(move |conn| {
            let mut writer = conn.try_clone().unwrap();
            let mut reader = BufReader::new(conn);
            loop {
                let line = read_cmd_line(&mut reader);
                if line.is_empty() {
                    break; // EOF or read timeout
                }
                let cmd_name = line
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_ascii_uppercase();
                match cmd_name.as_str() {
                    "ARQCALL" => {
                        write_reply(&mut writer, "ARQCALL");
                        write_reply(&mut writer, "NEWSTATE ISS");
                        write_reply(
                            &mut writer,
                            &format!("CONNECTED {peer_call} {bandwidth_hz}"),
                        );
                    }
                    "DISCONNECT" => {
                        write_reply(&mut writer, "DISCONNECTED");
                        break; // session is done
                    }
                    other => {
                        // For all init setters: echo the command name back.
                        write_reply(&mut writer, other);
                    }
                }
            }
        })
    }

    // ── Mock DATA server ──────────────────────────────────────────────────

    /// Build the wire bytes for one ARQ data frame:
    /// `[u16 BE length = 3 + payload.len()][ARQ][payload]`
    fn arq_frame(payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        let length = (3 + payload.len()) as u16;
        v.extend_from_slice(&length.to_be_bytes());
        v.extend_from_slice(b"ARQ");
        v.extend_from_slice(payload);
        v
    }

    /// Spawn a mock DATA server that:
    /// - Immediately sends one ARQ frame with `inbound_payload`.
    /// - Collects all raw bytes written by the client into `received`.
    ///
    /// Returns `(addr, join_handle, received_arc)`.
    fn spawn_mock_data_server(
        inbound_payload: Vec<u8>,
        received: Arc<Mutex<Vec<u8>>>,
    ) -> (SocketAddr, thread::JoinHandle<()>) {
        spawn_server(move |mut conn| {
            // Send the framed ARQ payload to the client.
            let frame = arq_frame(&inbound_payload);
            let _ = conn.write_all(&frame);
            // Collect what the client writes (raw bytes, no framing).
            let mut buf = [0u8; 256];
            loop {
                match conn.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => received.lock().unwrap().extend_from_slice(&buf[..n]),
                }
            }
        })
    }

    // ── Test 1: Full happy-path session through Box<dyn ModemTransport> ───

    #[test]
    fn full_session_happy_path_via_boxed_trait() {
        // — CMD mock
        let (cmd_addr, cmd_server) = spawn_mock_cmd_server("W7ABC", 500);

        // — DATA mock: sends "HELLO" ARQ frame, collects raw client writes
        let received: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let (data_addr, data_server) = spawn_mock_data_server(b"HELLO".to_vec(), received.clone());

        // Exercise through Box<dyn ModemTransport> — this is the object-safety
        // test; if the trait isn't object-safe this line won't compile.
        let mut transport: Box<dyn ModemTransport> =
            Box::new(ArdopTransport::with_addrs(cmd_addr, data_addr));

        // init
        let cfg = InitConfig {
            mycall: "N7CPZ".into(),
            gridsquare: "CN87".into(),
            arq_timeout_s: 30,
        };
        transport.init(&cfg).expect("init must succeed");

        // connect_arq
        let info = transport
            .connect_arq("W7ABC", 3, Duration::from_secs(5))
            .expect("connect_arq must succeed");
        assert_eq!(info.peer_call, "W7ABC");
        assert_eq!(info.bandwidth_hz, 500);

        // write through data_stream — assert raw bytes arrive at mock server
        {
            let ds = transport.data_stream().expect("data_stream must be available after init");
            ds.write_all(b"WORLD").expect("write to data socket");
            ds.flush().ok();
        }

        // read through data_stream — should get the ARQ payload "HELLO"
        {
            let ds = transport.data_stream().expect("data_stream still available");
            let mut buf = vec![0u8; 64];
            let n = ds.read(&mut buf).expect("read from data socket");
            assert_eq!(&buf[..n], b"HELLO", "must read back the ARQ payload");
        }

        // disconnect
        transport
            .disconnect(Duration::from_secs(5))
            .expect("disconnect must succeed");

        // Give the mock data server a moment to drain writes then close
        drop(transport);
        cmd_server.join().unwrap();
        data_server.join().unwrap();

        // The mock data server received the raw "WORLD" bytes (no framing added)
        let got = received.lock().unwrap().clone();
        assert_eq!(got, b"WORLD", "data server must see raw write bytes");
    }

    // ── Test 2: connect_arq before init returns Err, not panic ───────────

    #[test]
    fn connect_arq_before_init_returns_err() {
        // Addresses don't matter — we never connect.
        let addr: SocketAddr = "127.0.0.1:1".parse().unwrap();
        let mut t = ArdopTransport::with_addrs(addr, addr);
        let err = t
            .connect_arq("W7ABC", 3, Duration::from_millis(100))
            .expect_err("connect_arq before init must return Err");
        // Should be a NotConnected or similar I/O error wrapped in SessionError.
        assert!(
            matches!(err, SessionError::Io(ref e) if e.kind() == io::ErrorKind::NotConnected),
            "expected SessionError::Io(NotConnected), got {err:?}"
        );
    }

    // ── Test 3: data_stream before init returns Err, not panic ───────────

    #[test]
    fn data_stream_before_init_returns_err() {
        let addr: SocketAddr = "127.0.0.1:1".parse().unwrap();
        let mut t = ArdopTransport::with_addrs(addr, addr);
        // io::Result<&mut dyn ReadWrite>: the Ok arm is `&mut dyn ReadWrite`
        // which doesn't implement Debug, so unwrap_err()/expect_err() won't compile.
        // Use match to extract the Err branch manually.
        match t.data_stream() {
            Ok(_) => panic!("data_stream before init must return Err"),
            Err(e) => assert_eq!(
                e.kind(),
                io::ErrorKind::NotConnected,
                "expected NotConnected, got {e}"
            ),
        }
    }

    // ── Test 4: disconnect before init returns Err, not panic ────────────

    #[test]
    fn disconnect_before_init_returns_err() {
        let addr: SocketAddr = "127.0.0.1:1".parse().unwrap();
        let mut t = ArdopTransport::with_addrs(addr, addr);
        let err = t
            .disconnect(Duration::from_millis(100))
            .expect_err("disconnect before init must return Err");
        assert!(
            matches!(err, SessionError::Io(ref e) if e.kind() == io::ErrorKind::NotConnected),
            "expected SessionError::Io(NotConnected), got {err:?}"
        );
    }

    // ── Test 5: object safety — explicit Box<dyn ModemTransport> compile check

    /// This test primarily exists to confirm the trait is object-safe:
    /// constructing a `Box<dyn ModemTransport>` and calling all four methods
    /// through the vtable.  The mock servers used here are identical to test 1
    /// but we do a minimal round-trip to keep the test light and focused.
    #[test]
    fn object_safety_box_dyn_modem_transport() {
        let (cmd_addr, cmd_server) = spawn_mock_cmd_server("K7XYZ", 200);
        let received: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let (data_addr, data_server) =
            spawn_mock_data_server(b"PING".to_vec(), received.clone());

        // The explicit type annotation is the load-bearing part of this test:
        // if `ModemTransport` were not object-safe, this line would fail to compile.
        let mut t: Box<dyn ModemTransport> =
            Box::new(ArdopTransport::with_addrs(cmd_addr, data_addr));

        let cfg = InitConfig {
            mycall: "K7XYZ".into(),
            gridsquare: "CN87".into(),
            arq_timeout_s: 30,
        };
        t.init(&cfg).unwrap();
        let info = t.connect_arq("K7XYZ", 1, Duration::from_secs(5)).unwrap();
        assert_eq!(info.peer_call, "K7XYZ");

        // Read one payload through the trait object.
        let ds = t.data_stream().unwrap();
        let mut buf = vec![0u8; 32];
        let n = ds.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"PING");

        t.disconnect(Duration::from_secs(5)).unwrap();

        drop(t);
        cmd_server.join().unwrap();
        data_server.join().unwrap();
    }

    // ── Phase 5 helper: Python stub that mimics ardopcf's TCP ports ──────

    /// Write a Python stub script to a temp file and return its path.
    ///
    /// The stub:
    /// - Binds `cmd_port` and `data_port` immediately on startup (so bind-wait
    ///   succeeds quickly).
    /// - On the cmd port: accepts one connection, reads `\r`-terminated command
    ///   lines, and echoes back the command name as the ack (matching the
    ///   `init_tnc` sequence). On DISCONNECT it replies DISCONNECTED and exits.
    /// - On the data port: accepts one connection and idles (reads + discards).
    /// - Exits cleanly on SIGINT.
    ///
    /// The script path is unique per process-id + thread-id to avoid collisions
    /// when tests run in parallel.
    fn write_ardopcf_stub(cmd_port: u16, data_port: u16) -> std::path::PathBuf {
        use std::fmt::Write as FmtWrite;
        let pid = std::process::id();
        // Use a unique filename per invocation so parallel test runs don't collide.
        let path = std::env::temp_dir().join(format!(
            "tuxlink-ardopcf-stub-{}-{}.py",
            pid,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos()
        ));

        let mut script = String::new();
        write!(
            &mut script,
            r#"#!/usr/bin/env python3
import socket
import threading
import signal
import sys

CMD_PORT = {cmd_port}
DATA_PORT = {data_port}

# Bind both sockets immediately so the bind-wait succeeds.
cmd_srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
cmd_srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
cmd_srv.bind(('127.0.0.1', CMD_PORT))
cmd_srv.listen(1)

data_srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
data_srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
data_srv.bind(('127.0.0.1', DATA_PORT))
data_srv.listen(1)

stop_event = threading.Event()

def handle_data(conn):
    conn.settimeout(1.0)
    while not stop_event.is_set():
        try:
            data = conn.recv(256)
            if not data:
                break
        except socket.timeout:
            continue
        except Exception:
            break
    conn.close()

def handle_cmd(conn):
    conn.settimeout(1.0)
    buf = b''
    while not stop_event.is_set():
        try:
            chunk = conn.recv(256)
            if not chunk:
                break
            buf += chunk
        except socket.timeout:
            continue
        except Exception:
            break
        while b'\r' in buf:
            line, buf = buf.split(b'\r', 1)
            line_str = line.decode('ascii', errors='replace').strip()
            if not line_str:
                continue
            cmd_name = line_str.split()[0].upper() if line_str.split() else ''
            if cmd_name == 'DISCONNECT':
                conn.sendall(b'DISCONNECTED\r')
                conn.close()
                stop_event.set()
                return
            else:
                conn.sendall((cmd_name + '\r').encode('ascii'))
    conn.close()

def sigint_handler(sig, frame):
    stop_event.set()
    sys.exit(0)

signal.signal(signal.SIGINT, sigint_handler)
signal.signal(signal.SIGTERM, sigint_handler)

cmd_srv.settimeout(10.0)
data_srv.settimeout(10.0)

try:
    cmd_conn, _ = cmd_srv.accept()
    data_conn, _ = data_srv.accept()
except socket.timeout:
    sys.exit(1)

cmd_srv.close()
data_srv.close()

data_thread = threading.Thread(target=handle_data, args=(data_conn,), daemon=True)
data_thread.start()

handle_cmd(cmd_conn)
stop_event.set()
data_thread.join(timeout=2.0)
"#,
            cmd_port = cmd_port,
            data_port = data_port,
        )
        .unwrap();

        std::fs::write(&path, script.as_bytes()).expect("write stub script");
        path
    }

    /// Pick two free loopback ports without holding them open.
    ///
    /// Binds to :0, reads the OS-assigned port, then drops the listener (releases
    /// the port). There is a narrow TOCTOU window between drop and the stub's
    /// bind, but in practice the OS does not immediately reuse ephemeral ports, so
    /// this is reliable in tests.
    fn free_ports() -> (u16, u16) {
        let l1 = TcpListener::bind("127.0.0.1:0").unwrap();
        let p1 = l1.local_addr().unwrap().port();
        let l2 = TcpListener::bind("127.0.0.1:0").unwrap();
        let p2 = l2.local_addr().unwrap().port();
        drop(l1);
        drop(l2);
        (p1, p2)
    }

    // ── Test 6: with_managed_modem → init → shutdown happy path ──────────

    /// Spawn the Python stub, drive init, then call shutdown.
    ///
    /// Verifies:
    /// - `with_managed_modem` succeeds (both ports come up).
    /// - `init` completes the 7-setter sequence.
    /// - `shutdown` returns Ok and the stub process is reaped
    ///   (`ManagedModem` no longer running).
    #[test]
    fn managed_modem_spawn_init_shutdown_happy_path() {
        let (cmd_port, data_port) = free_ports();
        let stub_path = write_ardopcf_stub(cmd_port, data_port);

        let cfg = ArdopConfig {
            binary: "python3".into(),
            extra_args: vec![stub_path.to_string_lossy().into_owned()],
            cmd_port,
            data_port,
            audio_device_path: None,
        };

        // Use a generous bind-wait because Python startup can be slow on a
        // loaded CI host.
        let mut transport =
            ArdopTransport::with_managed_modem_timeout(cfg, Duration::from_secs(10))
                .expect("with_managed_modem must succeed");

        let init_cfg = InitConfig {
            mycall: "N7TST".into(),
            gridsquare: "CN87".into(),
            arq_timeout_s: 30,
        };
        transport.init(&init_cfg).expect("init must succeed");

        // After init, cmd socket is open; shutdown should close it and stop the stub.
        transport.shutdown().expect("shutdown must return Ok");

        // The managed ManagedModem should have been stopped.
        if let Some((ref mut modem, _)) = transport.managed {
            assert!(
                !modem.is_running(),
                "stub process must be reaped after shutdown"
            );
        } else {
            panic!("managed field must be Some after with_managed_modem");
        }

        // Cleanup stub script.
        let _ = std::fs::remove_file(&stub_path);
    }

    // ── Test 7: with_managed_modem returns TimedOut if ports never bind ───

    /// Spawn a no-op process that never binds the ports. Verify that
    /// `with_managed_modem_timeout` returns `Err(SessionError::Io(TimedOut))`
    /// within the (short) timeout.
    #[test]
    fn managed_modem_times_out_when_ports_never_bind() {
        let (cmd_port, data_port) = free_ports();

        let cfg = ArdopConfig {
            binary: "/bin/sh".into(),
            // `-c "sleep 30"` — binds no ports
            extra_args: vec!["-c".into(), "sleep 30".into()],
            cmd_port,
            data_port,
            audio_device_path: None,
        };

        // Very short bind-wait so the test completes quickly.
        let result =
            ArdopTransport::with_managed_modem_timeout(cfg, Duration::from_millis(500));

        match result {
            Ok(_) => panic!("must return Err when ports never bind"),
            Err(SessionError::Io(ref e)) => {
                assert_eq!(
                    e.kind(),
                    io::ErrorKind::TimedOut,
                    "expected TimedOut, got {e}"
                );
            }
            Err(other) => panic!("expected SessionError::Io(TimedOut), got {other:?}"),
        }
    }
}
