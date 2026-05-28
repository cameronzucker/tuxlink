//! `ArdopTransport` — implements [`ModemTransport`] for ardopcf (and any
//! ARDOP-compatible TNC) over the standard dual-TCP-socket host protocol.
//!
//! `with_addrs` constructs an unconnected transport; `init` opens both the
//! command socket and the data socket and runs the ARDOP init sequence.
//! After a successful `connect_arq` the `data_stream` accessor exposes the
//! `DataSocket` as `&mut dyn ReadWrite` for consumption by the sync B2F
//! `run_exchange`.

use std::io;
use std::net::SocketAddr;
use std::time::Duration;

use super::data::DataSocket;
use super::session::{arq_connect, arq_disconnect, init_tnc, CmdSocket, ConnectInfo, InitConfig, SessionError};
use crate::winlink::modem::{ModemTransport, ReadWrite};

// ─── ArdopTransport ─────────────────────────────────────────────────────────

/// [`ModemTransport`] implementation for ardopcf (ARDOP TNC).
///
/// Drives ardopcf's dual-TCP host protocol:
/// - cmd socket (typically 8515): `\r`-terminated ASCII command lines.
/// - data socket (typically 8516): ARQ-framed inbound, raw bytes outbound.
///
/// # Lifecycle
///
/// ```text
/// with_addrs(cmd_addr, data_addr)   ← no I/O; sockets not yet open
///   .init(cfg)                      ← opens CmdSocket + DataSocket, runs init sequence
///   .connect_arq(target, n, t)      ← ARQCALL handshake → ConnectInfo
///   .data_stream()                  ← Read + Write for B2F exchange
///   .disconnect(t)                  ← DISCONNECT command + confirmation
/// ```
pub struct ArdopTransport {
    cmd_addr: SocketAddr,
    data_addr: SocketAddr,
    cmd: Option<CmdSocket>,
    data: Option<DataSocket>,
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
        }
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
        let cmd = CmdSocket::connect(self.cmd_addr)?;
        let data = DataSocket::connect(self.data_addr)?;
        self.cmd = Some(cmd);
        self.data = Some(data);
        init_tnc(self.cmd.as_mut().unwrap(), cfg)
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
}
