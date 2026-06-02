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
use std::sync::{Arc, Mutex};
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

/// Operator-side handle that disarms a live KISS link at the OS layer (tuxlink-0ja).
///
/// The orchestration layer stashes one of these in its abort slot at connect time
/// (mirroring how telnet stashes a try-cloned `TcpStream`); the operator's Cancel
/// invokes `abort()`, which performs the strongest disarm the underlying transport
/// supports:
///
/// * `TcpStream` / `RfcommAbort` (socket transports) — `shutdown(SHUT_RDWR)` on a
///   try-cloned fd from another thread DETERMINISTICALLY interrupts an in-flight
///   read or write at the kernel layer. This is the same mechanism the existing
///   TCP-KISS arm uses; `RfcommAbort` ports it to the Bluetooth path.
/// * `SerialAbort` — serial ports have no socket-shutdown equivalent, so abort drops
///   the underlying `serialport` handle (sets the shared slot to `None`). All
///   subsequent reads/writes through `DisarmableLink` see the empty slot and return
///   `ConnectionAborted` synchronously; an in-flight write that holds the slot lock
///   completes first (bounded — ≤ one ~20-byte SABM at 1200 baud ≈ 166 ms — and the
///   datalink connect loop is hard-capped at ≤ 2 SABMs). This closes the
///   check-then-write TOCTOU the previous flag-based gate left open.
pub trait LinkAbort: Send + Sync {
    fn abort(&self);
}

impl LinkAbort for TcpStream {
    fn abort(&self) {
        let _ = self.shutdown(std::net::Shutdown::Both);
    }
}

/// Shared slot for `DisarmableLink` + `SerialAbort`. `None` means the operator has
/// aborted; reads and writes through the link observe that synchronously and
/// return `ConnectionAborted` — the kernel-shutdown equivalent for transports that
/// have no socket to `shutdown()`.
type DisarmSlot = Arc<Mutex<Option<Box<dyn ByteLink>>>>;

/// A `ByteLink` whose inner transport can be torn down from another thread by
/// clearing the shared slot (serial / `/dev/rfcommN` paths — see `LinkAbort`).
struct DisarmableLink {
    slot: DisarmSlot,
}

impl Read for DisarmableLink {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut guard = self
            .slot
            .lock()
            .map_err(|_| std::io::Error::other("disarm slot poisoned"))?;
        match guard.as_mut() {
            None => Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionAborted,
                "packet listen aborted",
            )),
            Some(inner) => inner.read(buf),
        }
    }
}

impl Write for DisarmableLink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        // tuxlink-2y4 / tuxlink-0ja: gate every transmit on the disarm slot. The lock
        // makes the slot check + `inner.write` happen-before-atomic w.r.t. an aborting
        // thread setting the slot to `None`, so the previous AbortableByteLink TOCTOU
        // (check-then-write — a Cancel landing between the flag load and `inner.write`
        // could still leak one in-flight SABM) is closed for the SUBSEQUENT-write case.
        // The IN-FLIGHT case (Cancel arriving while a write call holds the lock)
        // remains bounded: the lock-blocked abort waits ≤ one short serial write
        // (~166 ms for a 20-byte SABM at 1200 baud); the datalink connect loop is
        // hard-capped at ≤ 2 SABMs total. Bluetooth gets the strict in-flight
        // interrupt via the socket-shutdown path (RfcommAbort), not this slot.
        let mut guard = self
            .slot
            .lock()
            .map_err(|_| std::io::Error::other("disarm slot poisoned"))?;
        match guard.as_mut() {
            None => Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionAborted,
                "packet transmit aborted",
            )),
            Some(inner) => inner.write(buf),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let mut guard = self
            .slot
            .lock()
            .map_err(|_| std::io::Error::other("disarm slot poisoned"))?;
        match guard.as_mut() {
            None => Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionAborted,
                "packet flush aborted",
            )),
            Some(inner) => inner.flush(),
        }
    }
}

/// `LinkAbort` for a serial / RFCOMM-TTY transport: dropping the inner port (by
/// setting the shared slot to `None`) closes the underlying fd. The next read or
/// write through the paired `DisarmableLink` sees the empty slot and returns
/// `ConnectionAborted`.
struct SerialAbort {
    slot: DisarmSlot,
}

impl LinkAbort for SerialAbort {
    fn abort(&self) {
        // Lock-poisoned is fine — the next caller will recover, and the slot is
        // already in a state where it's about to be cleared anyway. Use `Ok` so a
        // poisoned lock doesn't strand the abort.
        if let Ok(mut guard) = self.slot.lock() {
            *guard = None;
        }
    }
}

/// Wrap an inner `ByteLink` so it can be disarmed from another thread (the serial
/// pattern). Returns the wrapper (hand to `datalink::connect`/`answer`) and the
/// `LinkAbort` handle (stash in the orchestration's abort slot).
fn disarmable(inner: Box<dyn ByteLink>) -> (DisarmableLink, SerialAbort) {
    let slot: DisarmSlot = Arc::new(Mutex::new(Some(inner)));
    let link = DisarmableLink { slot: slot.clone() };
    let abort = SerialAbort { slot };
    (link, abort)
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
        KissLinkConfig::Bluetooth { mac } => Ok(Box::new(connect_bluetooth(mac)?)),
    }
}

/// Open a Bluetooth RFCOMM **socket** to `mac` (tuxlink-nx2). Resolves the SPP
/// channel from SDP at connect time (it rotates), then connects an
/// `AF_BLUETOOTH`/`BTPROTO_RFCOMM` socket — no `rfcomm bind`, no root, no TTY.
fn connect_bluetooth(mac: &str) -> std::io::Result<crate::winlink::ax25::rfcomm::RfcommSocket> {
    let channel = crate::winlink::ax25::rfcomm::resolve_spp_channel(mac);
    crate::winlink::ax25::rfcomm::RfcommSocket::connect(
        mac,
        channel,
        LINK_POLL_TIMEOUT,
        LINK_WRITE_TIMEOUT,
    )
}

/// Like `connect_link`, but wires the orchestration layer's abort handle into the
/// link so a blocked connect/answer can be Stopped at the OS layer (tuxlink-0ja).
///
/// Every transport now yields a `Box<dyn LinkAbort>` — the orchestration's abort
/// slot stashes it and calls `.abort()` on Cancel:
///
/// * TCP / RFCOMM socket — `shutdown(SHUT_RDWR)` on a try-cloned fd from another
///   thread interrupts an in-flight read/write at the kernel layer (the strongest
///   disarm available, mirroring what telnet already does).
/// * Serial — drops the held `serialport` handle (closes the underlying fd). The
///   paired `DisarmableLink`'s next read/write sees the empty slot and returns
///   `ConnectionAborted`. An in-flight write holding the slot lock completes first
///   (bounded — see `DisarmableLink::write`).
pub fn connect_link_with_abort(
    cfg: &KissLinkConfig,
) -> std::io::Result<(Box<dyn ByteLink>, Box<dyn LinkAbort>)> {
    match cfg {
        KissLinkConfig::Tcp { host, port } => {
            let stream = TcpStream::connect((host.as_str(), *port))?;
            stream.set_read_timeout(Some(LINK_POLL_TIMEOUT)).ok();
            stream.set_write_timeout(Some(LINK_WRITE_TIMEOUT)).ok();
            // A `shutdown()` on the clone makes the boxed original's `read` return 0
            // (FIN), which `recv_frame` maps to ConnectionAborted, unwinding a blocked
            // answer()/connect(). It also makes a subsequent `write` fail at the kernel
            // — the strict in-flight disarm.
            let abort_sock = stream.try_clone()?;
            Ok((Box::new(stream), Box::new(abort_sock)))
        }
        // Serial has no socket-shutdown equivalent (the `serialport` crate exposes no
        // `try_clone`+shutdown analog), so route abort through the slot pattern
        // (tuxlink-0ja): clearing the shared `Option<Box<dyn ByteLink>>` from
        // SerialAbort::abort() makes the next DisarmableLink read/write fail with
        // ConnectionAborted, unwinding a blocked answer()/connect(). Strictly better
        // than the previous AbortableByteLink check-then-write flag because the slot
        // check + `inner.write` are atomic under the lock.
        KissLinkConfig::Serial { .. } => {
            let inner = connect_serial(cfg)?;
            let (link, abort) = disarmable(inner);
            Ok((Box::new(link), Box::new(abort)))
        }
        // RFCOMM socket — same family as TCP (real socket fd), so use the same
        // `shutdown(SHUT_RDWR)` pattern: try_clone the fd, hand the original back as
        // the ByteLink, return RfcommAbort holding the clone (tuxlink-0ja). Strict
        // in-flight disarm — the previous flag-based AbortableByteLink wrapper here
        // had the same TOCTOU class as the serial path.
        KissLinkConfig::Bluetooth { mac } => {
            let sock = connect_bluetooth(mac)?;
            let abort = sock.try_clone_abort()?;
            Ok((Box::new(sock), Box::new(abort)))
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

    // tuxlink-0ja: invoking the SerialAbort handle must make the DisarmableLink's
    // next write FAIL WITHOUT forwarding to the inner link. The previous
    // AbortableByteLink check-then-write flag-gate left a sub-microsecond window
    // where one in-flight SABM could still key the radio after Cancel; the slot
    // pattern closes that. No hardware, no RF.
    #[test]
    fn disarmable_link_write_refuses_to_key_after_abort() {
        use std::sync::Mutex as StdMutex;
        struct Recording(Arc<StdMutex<Vec<u8>>>);
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
        let written = Arc::new(StdMutex::new(Vec::new()));
        let inner: Box<dyn ByteLink> = Box::new(Recording(written.clone()));
        let (mut link, abort) = disarmable(inner);
        // Disarm before any write — the slot is now empty.
        abort.abort();
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

    // tuxlink-0ja: before abort, DisarmableLink is fully transparent — bytes flow
    // both ways. After abort, the slot is permanently empty: every subsequent
    // operation fails with ConnectionAborted. There is no "rearm" path; the disarm
    // is one-shot per connection (the orchestration drops the link+abort at
    // exchange end and opens fresh handles for the next connect).
    #[test]
    fn disarmable_link_passes_bytes_until_aborted_then_stays_disarmed() {
        use std::sync::Mutex as StdMutex;
        struct Recording {
            buf: Arc<StdMutex<Vec<u8>>>,
            to_read: Arc<StdMutex<Vec<u8>>>,
        }
        impl Read for Recording {
            fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
                let mut src = self.to_read.lock().unwrap();
                let n = src.len().min(b.len());
                b[..n].copy_from_slice(&src[..n]);
                src.drain(..n);
                Ok(n)
            }
        }
        impl Write for Recording {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.buf.lock().unwrap().extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let written = Arc::new(StdMutex::new(Vec::new()));
        let to_read = Arc::new(StdMutex::new(vec![0xC0u8, 0x42, 0xC0]));
        let inner: Box<dyn ByteLink> = Box::new(Recording {
            buf: written.clone(),
            to_read: to_read.clone(),
        });
        let (mut link, abort) = disarmable(inner);

        // Pre-abort: write passes through, read passes through.
        link.write_all(b"hello").unwrap();
        assert_eq!(written.lock().unwrap().as_slice(), b"hello");
        let mut buf = [0u8; 3];
        assert_eq!(link.read(&mut buf).unwrap(), 3);
        assert_eq!(buf, [0xC0, 0x42, 0xC0]);

        // Disarm.
        abort.abort();

        // Post-abort: writes and reads BOTH fail, even after the first failure (no
        // rearm — the slot is permanently empty).
        for _ in 0..3 {
            assert_eq!(
                link.write(b"x").unwrap_err().kind(),
                std::io::ErrorKind::ConnectionAborted
            );
            assert_eq!(
                link.read(&mut buf).unwrap_err().kind(),
                std::io::ErrorKind::ConnectionAborted
            );
        }
        // And no bytes leaked past the first hello.
        assert_eq!(written.lock().unwrap().as_slice(), b"hello");
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
    fn connect_link_with_abort_tcp_arm_disarms_at_the_kernel() {
        // A loopback KISS modem stand-in that holds the connection open until the
        // client side is shut down (no RF, 127.0.0.1 only).
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut sink = Vec::new();
            let _ = sock.read_to_end(&mut sink);
        });

        let cfg = KissLinkConfig::Tcp { host: addr.ip().to_string(), port: addr.port() };
        let (mut link, abort) = connect_link_with_abort(&cfg).unwrap();

        link.write_all(&[0xC0]).unwrap(); // ensure the link is live before aborting
        abort.abort(); // LinkAbort — under the hood: shutdown(SHUT_RDWR) on the cloned fd

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
        let cfg = KissLinkConfig::Serial {
            device: "/dev/tuxlink-no-such-device".into(),
            baud: 9600,
        };
        let err = connect_link_with_abort(&cfg)
            .err()
            .expect("expected a clean open error, got Ok");
        assert!(
            matches!(err.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::Other),
            "expected a clean open error, got {err:?}"
        );
    }
}
