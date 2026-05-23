//! KISS byte-pipe transports for AX.25: a `ByteLink` over TCP (Dire Wolf /
//! SoundModem KISS port) or a serial device (USB COM port, or a Bluetooth RFCOMM
//! `/dev/rfcommN` opened identically). The state machine in `datalink.rs` drives a
//! `ByteLink` through the KISS framer; this layer is dumb byte plumbing.
//!
//! **No RF here.** The TCP arm is exercised against a loopback `TcpListener`. The
//! serial arm (Task 4) is exercised by the operator on hardware (RADIO-1 / spec §6);
//! the agent verifies only that it compiles and opens a device path.

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// How long a single read on the KISS link may block before returning (a short
/// poll interval, NOT a session timeout). The AX.25 state machine's poll/T1 loops
/// (`recv_frame`, `await_ack`, `await_window`, the connect/disconnect waits) call
/// `read` repeatedly and treat a timeout as "no frame yet" (WouldBlock); a real
/// `TcpStream`/serial `read` must therefore return promptly when idle, or those
/// loops would each block up to a full socket timeout per poll. The "fail legibly
/// on a hung modem" guarantee comes from the N2×T1 logic in the state machine, not
/// from this socket timeout. (Fix H — was 60 s, which defeated the poll/T1 model on
/// real links.)
const LINK_POLL_TIMEOUT: Duration = Duration::from_millis(200);

/// How long a single `write` on the KISS link may block before failing. Writes are
/// not part of the poll loop, so this stays a generous "the socket is wedged" bound
/// rather than the short read-poll interval (a normal KISS frame write completes
/// instantly; this only trips if the OS send buffer never drains).
const LINK_WRITE_TIMEOUT: Duration = Duration::from_secs(60);

/// Which KISS byte-pipe to open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum KissLinkConfig {
    /// KISS-over-TCP, e.g. Dire Wolf / SoundModem listening on `127.0.0.1:8001`.
    Tcp { host: String, port: u16 },
    /// KISS-over-serial: a USB COM device (`/dev/ttyUSB0`). `baud` is the host↔modem
    /// link rate (distinct from the 1200-baud over-air rate).
    Serial { device: String, baud: u32 },
    /// KISS over a Bluetooth RFCOMM **socket** connected directly to the radio's
    /// `mac` (tuxlink-nx2). Unlike `Serial`+`/dev/rfcommN`, this needs no `rfcomm
    /// bind`, no root, and no serialport TTY whose termios reconfiguration the
    /// radio's SPP service tears down (the "Broken pipe" on first write). The SPP
    /// channel rotates per registration, so it is read from SDP at connect time.
    Bluetooth { mac: String },
}

/// A bidirectional, thread-movable byte stream — the KISS pipe the AX.25 state
/// machine reads framed bytes from and writes framed bytes to. Blanket-implemented
/// for any `Read + Write + Send` (so `TcpStream`, a `serialport` handle, and the
/// in-memory test peer all qualify).
pub trait ByteLink: Read + Write + Send {}
impl<T: Read + Write + Send> ByteLink for T {}

/// Wraps a `ByteLink` so a shared abort flag can unwind a blocked read. A serial
/// KISS link (unlike TCP) has no socket to shut down, so `answer()`/`connect()`
/// can't be aborted by closing the pipe; instead a Stop sets the flag, and the
/// next `read` returns `ConnectionAborted` — which `recv_frame` already maps to an
/// abort, unwinding the poll loop. The serial read's short poll timeout
/// (`LINK_POLL_TIMEOUT`) bounds how soon the flag is observed (tuxlink-nj1).
struct AbortableByteLink {
    inner: Box<dyn ByteLink>,
    abort: Arc<AtomicBool>,
}

impl Read for AbortableByteLink {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.abort.load(Ordering::SeqCst) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionAborted,
                "packet listen aborted",
            ));
        }
        self.inner.read(buf)
    }
}

impl Write for AbortableByteLink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        // tuxlink-2y4 RADIO-1: gate every transmit on the abort flag, mirroring read().
        // connect()'s push_kiss_params + each SABM go through here; a Cancel set before
        // the write means the radio is NEVER keyed. The 2026-05-22 incident keyed for
        // ~110 s because only read() checked the flag — write() forwarded unconditionally.
        if self.abort.load(Ordering::SeqCst) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionAborted,
                "packet transmit aborted",
            ));
        }
        self.inner.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// Open a KISS byte-pipe per `cfg`. The returned `Box<dyn ByteLink>` is handed to
/// `datalink::connect` / `datalink::answer`.
pub fn connect_link(cfg: &KissLinkConfig) -> std::io::Result<Box<dyn ByteLink>> {
    match cfg {
        KissLinkConfig::Tcp { host, port } => {
            let stream = TcpStream::connect((host.as_str(), *port))?;
            stream.set_read_timeout(Some(LINK_POLL_TIMEOUT)).ok();
            stream.set_write_timeout(Some(LINK_WRITE_TIMEOUT)).ok();
            Ok(Box::new(stream))
        }
        KissLinkConfig::Serial { .. } => connect_serial(cfg),
        KissLinkConfig::Bluetooth { mac } => connect_bluetooth(mac),
    }
}

/// Open a Bluetooth RFCOMM **socket** to `mac` (tuxlink-nx2). Resolves the SPP
/// channel from SDP at connect time (it rotates), then connects an
/// `AF_BLUETOOTH`/`BTPROTO_RFCOMM` socket — no `rfcomm bind`, no root, no TTY.
fn connect_bluetooth(mac: &str) -> std::io::Result<Box<dyn ByteLink>> {
    let channel = crate::winlink::ax25::rfcomm::resolve_spp_channel(mac);
    let sock = crate::winlink::ax25::rfcomm::RfcommSocket::connect(
        mac,
        channel,
        LINK_POLL_TIMEOUT,
        LINK_WRITE_TIMEOUT,
    )?;
    Ok(Box::new(sock))
}

/// Like `connect_link`, but wires the orchestration layer's abort signal into the
/// link so a blocked connect/answer can be Stopped. TCP yields a try_clone'd
/// `TcpStream` whose `shutdown()` makes reads return 0 (the immediate fast path).
/// Serial has no socket, so it wraps the link in `AbortableByteLink` keyed on the
/// shared `abort` flag: setting the flag makes the next read return
/// `ConnectionAborted`, unwinding the state machine (tuxlink-nj1) — so serial yields
/// `None` for the socket handle, with abort delivered via the flag instead.
pub fn connect_link_with_abort(
    cfg: &KissLinkConfig,
    abort: Arc<AtomicBool>,
) -> std::io::Result<(Box<dyn ByteLink>, Option<std::net::TcpStream>)> {
    match cfg {
        KissLinkConfig::Tcp { host, port } => {
            let stream = TcpStream::connect((host.as_str(), *port))?;
            stream.set_read_timeout(Some(LINK_POLL_TIMEOUT)).ok();
            stream.set_write_timeout(Some(LINK_WRITE_TIMEOUT)).ok();
            // TCP keeps the immediate socket-shutdown fast path: a `shutdown()` on the
            // clone makes the boxed original's `read` return 0 (FIN), which `recv_frame`
            // maps to ConnectionAborted, unwinding a blocked answer()/connect(). The
            // `abort` flag is unused on this arm (the FIN, not the flag, does the work).
            let abort_sock = stream.try_clone()?;
            Ok((Box::new(stream), Some(abort_sock)))
        }
        // Serial has no try_clone/shutdown equivalent, so route abort through the
        // shared flag (tuxlink-nj1): AbortableByteLink turns a set flag into a
        // ConnectionAborted read on the next poll, unwinding a blocked answer()/
        // connect(). No TCP socket handle to return.
        KissLinkConfig::Serial { .. } => {
            let inner = connect_serial(cfg)?;
            Ok((Box::new(AbortableByteLink { inner, abort }), None))
        }
        // The RFCOMM socket has no try_clone/shutdown wired here, so (like Serial)
        // abort is delivered via the shared flag: the socket's SO_RCVTIMEO
        // (LINK_POLL_TIMEOUT) bounds how soon AbortableByteLink turns a set flag into
        // a ConnectionAborted read, unwinding a blocked answer()/connect().
        KissLinkConfig::Bluetooth { mac } => {
            let inner = connect_bluetooth(mac)?;
            Ok((Box::new(AbortableByteLink { inner, abort }), None))
        }
    }
}

#[cfg(test)]
mod link_serial_tests {
    use super::*;
    #[test]
    fn serial_open_of_a_nonexistent_device_errors_cleanly() {
        // No hardware, no RF: opening a device that does not exist must return a
        // clean Err, never panic or hang. A real device open is operator-only
        // (RADIO-1 / spec §6 — exercised on hardware by the licensee).
        let cfg = KissLinkConfig::Serial {
            device: "/dev/tuxlink-no-such-device".into(),
            baud: 9600,
        };
        let result = connect_link(&cfg);
        let err = result.err().expect("expected a clean open error, got Ok");
        // serialport surfaces a NotFound/Other for a missing device path.
        assert!(
            matches!(err.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::Other),
            "expected a clean open error, got {err:?}"
        );
    }

    // tuxlink-2y4 RADIO-1: a set abort flag must make write() return ConnectionAborted
    // WITHOUT forwarding to the inner link — so connect()'s SABM + push_kiss_params
    // never key the radio after Cancel. read() already gated; write() did NOT — the
    // runaway-keying hole (the 2026-05-22 ~110 s incident). No hardware, no RF.
    #[test]
    fn abortable_link_write_refuses_to_key_after_abort() {
        use std::sync::Mutex;
        struct Recording(Arc<Mutex<Vec<u8>>>);
        impl Read for Recording {
            fn read(&mut self, _b: &mut [u8]) -> std::io::Result<usize> {
                Ok(0)
            }
        }
        impl Write for Recording {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let written = Arc::new(Mutex::new(Vec::new()));
        let mut link = AbortableByteLink {
            inner: Box::new(Recording(written.clone())),
            abort: Arc::new(AtomicBool::new(true)), // already cancelled
        };
        // Both write() and write_all() (what send_frame/push_kiss_params use) must fail.
        assert_eq!(
            link.write(b"SABM").unwrap_err().kind(),
            std::io::ErrorKind::ConnectionAborted
        );
        assert!(link.write_all(b"params").is_err());
        assert!(
            written.lock().unwrap().is_empty(),
            "no bytes may reach the radio after abort"
        );
    }
}

/// Open a KISS-over-serial byte-pipe (USB COM port or Bluetooth RFCOMM device).
/// `serialport` returns its own error type; map it to `std::io::Error` so the
/// `connect_link` signature stays `io::Result`.
fn connect_serial(cfg: &KissLinkConfig) -> std::io::Result<Box<dyn ByteLink>> {
    let (device, baud) = match cfg {
        KissLinkConfig::Serial { device, baud } => (device, *baud),
        // connect_link only routes the Serial variant here.
        KissLinkConfig::Tcp { .. } | KissLinkConfig::Bluetooth { .. } => {
            unreachable!("connect_serial called with a non-Serial config")
        }
    };
    let port = serialport::new(device, baud)
        // Short read-poll timeout so `recv_frame` returns promptly when the line is
        // idle (the state machine's N2×T1 logic owns the hung-modem guarantee).
        .timeout(LINK_POLL_TIMEOUT)
        .open()
        .map_err(|e| match e.kind() {
            serialport::ErrorKind::NoDevice => {
                std::io::Error::new(std::io::ErrorKind::NotFound, e.to_string())
            }
            _ => std::io::Error::other(e.to_string()),
        })?;
    Ok(Box::new(port))
}

#[cfg(test)]
mod link_tcp_tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn tcp_link_round_trips_bytes_over_loopback() {
        // A loopback KISS modem stand-in: echoes one chunk back. 127.0.0.1 only —
        // no RF, no external network (per testing-pitfalls).
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4];
            let n = sock.read(&mut buf).unwrap();
            sock.write_all(&buf[..n]).unwrap();
        });

        let cfg = KissLinkConfig::Tcp { host: addr.ip().to_string(), port: addr.port() };
        let mut link = connect_link(&cfg).unwrap();
        link.write_all(&[0xC0, 0x00, 0x42, 0xC0]).unwrap();
        let mut back = [0u8; 4];
        link.read_exact(&mut back).unwrap();
        assert_eq!(back, [0xC0, 0x00, 0x42, 0xC0]);
        server.join().unwrap();
    }

    #[test]
    fn tcp_connect_to_a_dead_port_errors_not_hangs() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener); // nothing listening ⇒ connection refused
        let cfg = KissLinkConfig::Tcp { host: addr.ip().to_string(), port: addr.port() };
        assert!(connect_link(&cfg).is_err());
    }

    #[test]
    fn connect_link_with_abort_yields_a_tcp_abort_handle_that_closes_the_link() {
        // A loopback KISS modem stand-in that holds the connection open until the
        // client side is shut down (no RF, 127.0.0.1 only).
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            // Block until the peer's FIN arrives, then return.
            let mut sink = Vec::new();
            let _ = sock.read_to_end(&mut sink);
        });

        let cfg = KissLinkConfig::Tcp { host: addr.ip().to_string(), port: addr.port() };
        let (mut link, abort) = connect_link_with_abort(&cfg, Arc::new(AtomicBool::new(false))).unwrap();
        // TCP must yield Some(_) abort handle.
        let abort = abort.expect("TCP arm must yield a Some(TcpStream) abort handle");

        // Shutting the returned clone makes the boxed original's read return 0 (FIN).
        // Use a longer read timeout on the original so the post-shutdown read returns 0
        // (FIN) rather than the short poll-timeout TimedOut.
        link.write_all(&[0xC0]).unwrap(); // ensure the link is live before aborting
        abort.shutdown(std::net::Shutdown::Both).unwrap();
        // Read until we observe a 0-byte read (the FIN), tolerating any interim
        // timeouts from the short LINK_POLL_TIMEOUT.
        let mut buf = [0u8; 8];
        let mut saw_zero = false;
        for _ in 0..50 {
            match link.read(&mut buf) {
                Ok(0) => {
                    saw_zero = true;
                    break;
                }
                Ok(_) => continue,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::TimedOut
                            | std::io::ErrorKind::NotConnected
                            | std::io::ErrorKind::BrokenPipe
                    ) =>
                {
                    continue
                }
                Err(e) => panic!("unexpected read error after shutdown: {e:?}"),
            }
        }
        assert!(saw_zero, "expected the shut-down clone to make the original read return 0 (FIN)");
        server.join().unwrap();
    }

    #[test]
    fn connect_link_with_abort_serial_open_errors_cleanly() {
        // Serial arm: a missing device path still errors cleanly (no None panic).
        // (When it DOES open, the abort handle is None — a clean serial abort is a
        // follow-up; here we only assert the open-error path matches connect_link.)
        let cfg = KissLinkConfig::Serial {
            device: "/dev/tuxlink-no-such-device".into(),
            baud: 9600,
        };
        let err = connect_link_with_abort(&cfg, Arc::new(AtomicBool::new(false)))
            .err()
            .expect("expected a clean open error, got Ok");
        assert!(
            matches!(err.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::Other),
            "expected a clean open error, got {err:?}"
        );
    }

    #[test]
    fn abortable_link_read_returns_connectionaborted_once_the_flag_is_set() {
        // tuxlink-nj1: a serial KISS link has no socket to shut down, so a Stop
        // sets a shared flag that AbortableByteLink turns into a ConnectionAborted
        // read — which recv_frame already maps to "abort", unwinding a blocked
        // answer()/connect() poll loop. Before the flag, reads pass through.
        let flag = Arc::new(AtomicBool::new(false));
        let inner: Box<dyn ByteLink> = Box::new(std::io::Cursor::new(vec![0xC0u8, 0x00, 0x42, 0xC0]));
        let mut link = AbortableByteLink { inner, abort: flag.clone() };
        let mut buf = [0u8; 4];
        assert_eq!(link.read(&mut buf).unwrap(), 4, "reads pass through before Stop");
        flag.store(true, Ordering::SeqCst);
        let err = link
            .read(&mut buf)
            .expect_err("after Stop, read must abort (not EOF, not inner data)");
        assert_eq!(err.kind(), std::io::ErrorKind::ConnectionAborted);
    }
}
