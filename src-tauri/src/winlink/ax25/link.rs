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

/// Which KISS byte-pipe to open. Bluetooth uses the `Serial` variant with an
/// rfcomm device path (e.g. `/dev/rfcomm0`); there is no in-app BlueZ dependency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum KissLinkConfig {
    /// KISS-over-TCP, e.g. Dire Wolf / SoundModem listening on `127.0.0.1:8001`.
    Tcp { host: String, port: u16 },
    /// KISS-over-serial: a USB COM device (`/dev/ttyUSB0`) OR a Bluetooth RFCOMM
    /// device (`/dev/rfcomm0`). `baud` is the host↔modem link rate (distinct from
    /// the 1200-baud over-air rate).
    Serial { device: String, baud: u32 },
}

/// A bidirectional, thread-movable byte stream — the KISS pipe the AX.25 state
/// machine reads framed bytes from and writes framed bytes to. Blanket-implemented
/// for any `Read + Write + Send` (so `TcpStream`, a `serialport` handle, and the
/// in-memory test peer all qualify).
pub trait ByteLink: Read + Write + Send {}
impl<T: Read + Write + Send> ByteLink for T {}

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
}

/// Open a KISS-over-serial byte-pipe (USB COM port or Bluetooth RFCOMM device).
/// `serialport` returns its own error type; map it to `std::io::Error` so the
/// `connect_link` signature stays `io::Result`.
fn connect_serial(cfg: &KissLinkConfig) -> std::io::Result<Box<dyn ByteLink>> {
    let (device, baud) = match cfg {
        KissLinkConfig::Serial { device, baud } => (device, *baud),
        // connect_link only routes the Serial variant here.
        KissLinkConfig::Tcp { .. } => unreachable!("connect_serial called with a Tcp config"),
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
}
