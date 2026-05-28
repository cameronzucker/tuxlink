//! ARDOP data-socket transport: `DataSocket`.
//!
//! Wraps the ARDOP data TCP socket (default 8516) as a `std::io::Read +
//! Write` byte-stream of the inbound ARQ payload — exactly what the sync
//! B2F `run_exchange<R: Read, W: Write>` consumes.
//!
//! **Framing contract:**
//! - **Inbound:** `[u16 BE length][3-byte type tag][payload]` frames arrive
//!   from the TNC. Only `DataKind::Arq` frames carry B2F session data;
//!   FEC/ERR/IDF frames are silently skipped. The `Read` impl decodes frames
//!   on the fly and presents the concatenated ARQ payloads as a flat byte
//!   stream to the caller.
//! - **Outbound:** raw bytes are written straight to the socket. The TNC
//!   handles framing for TX — no framing wrapper is applied here.
//!
//! **Read-loop design (no busy-loop, no spin):**
//! The `read` impl drains any queued payload bytes from `leftover` first.
//! When `leftover` is empty it calls `TcpStream::read` to fetch the next
//! raw chunk, pushes it into `DataDecoder`, and pulls frames.  If a socket
//! `read` returns 0 bytes the socket is at EOF and `Ok(0)` is returned to
//! the caller immediately — no re-read, no spin.

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};

use super::frame::{DataDecoder, DataKind};

// ─── DataSocket ────────────────────────────────────────────────────────────

/// A `Read + Write` view over the ARDOP data TCP socket.
///
/// `Read` surfaces the concatenated payload of all inbound `ARQ`-typed
/// frames as a flat byte stream.  `Write` forwards raw bytes directly to
/// the socket (the TNC frames them for TX).
pub struct DataSocket {
    stream: TcpStream,
    decoder: DataDecoder,
    /// Decoded ARQ payload bytes not yet consumed by a `read` call.
    leftover: VecDeque<u8>,
}

impl DataSocket {
    /// Open the ARDOP data socket at `addr`.
    pub fn connect(addr: SocketAddr) -> io::Result<Self> {
        let stream = TcpStream::connect(addr)?;
        Ok(DataSocket {
            stream,
            decoder: DataDecoder::default(),
            leftover: VecDeque::new(),
        })
    }

    /// Drain `leftover` queue bytes into `buf`, up to `buf.len()` bytes.
    ///
    /// Returns the number of bytes written.
    fn drain_leftover(&mut self, buf: &mut [u8]) -> usize {
        let n = buf.len().min(self.leftover.len());
        for (dst, src) in buf[..n].iter_mut().zip(self.leftover.drain(..n)) {
            *dst = src;
        }
        n
    }

    /// Pull decoded ARQ-payload bytes from the decoder into `leftover`.
    fn pump_decoder(&mut self) {
        while let Some(frame) = self.decoder.next_frame() {
            if frame.kind == DataKind::Arq {
                self.leftover.extend(frame.payload);
            }
            // FEC / ERR / IDF / Other frames are not B2F session data — skip.
        }
    }
}

impl Read for DataSocket {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        // 1. Drain any already-decoded leftover payload first.
        if !self.leftover.is_empty() {
            return Ok(self.drain_leftover(buf));
        }

        // 2. Loop: read from socket → push to decoder → pump until we either
        //    get payload bytes or hit EOF.
        let mut raw = [0u8; 4096];
        loop {
            let n = self.stream.read(&mut raw)?;
            if n == 0 {
                // Real socket EOF — signal EOF to the B2F engine.
                return Ok(0);
            }
            self.decoder.push(&raw[..n]);
            self.pump_decoder();

            if !self.leftover.is_empty() {
                return Ok(self.drain_leftover(buf));
            }
            // No complete ARQ frame decoded yet (could be partial frame or
            // only non-ARQ frames).  Block on the next socket read — no spin.
        }
    }
}

impl Write for DataSocket {
    /// Forward raw bytes directly to the data socket.
    ///
    /// The TNC handles ARDOP framing for TX — we write unframed payload.
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stream.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as IoWrite;
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;

    // ── Mock data-socket server helper ────────────────────────────────────

    /// Bind a loopback listener, spawn a server thread, return (addr, handle).
    ///
    /// The `handler` receives the accepted `TcpStream`.  A 2-second read timeout
    /// is set on the accepted connection so the server exits promptly.
    fn spawn_mock_data_server<F>(handler: F) -> (SocketAddr, thread::JoinHandle<()>)
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

    /// Build the wire bytes for one ARQ data frame.
    fn arq_frame(payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        let length = (3 + payload.len()) as u16;
        v.extend_from_slice(&length.to_be_bytes());
        v.extend_from_slice(b"ARQ");
        v.extend_from_slice(payload);
        v
    }

    // ── Test 1: Read yields ARQ payload from one frame ────────────────────

    #[test]
    fn read_yields_arq_payload_from_one_frame() {
        // Server sends one framed ARQ frame: [u16 len][ARQ][HELLO]
        let (addr, server) = spawn_mock_data_server(|mut conn| {
            conn.write_all(&arq_frame(b"HELLO")).unwrap();
            // Let the server thread exit cleanly: the 2s read-timeout on the
            // conn plus the close of the TcpStream on drop is sufficient.
        });

        let mut ds = DataSocket::connect(addr).unwrap();
        let mut buf = vec![0u8; 64];
        let n = ds.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"HELLO");

        drop(ds);
        server.join().unwrap();
    }

    // ── Test 2: Read across two frames, small buf ─────────────────────────

    #[test]
    fn read_across_two_frames_and_small_buf() {
        // Server sends two ARQ frames back-to-back in one TCP write.
        let (addr, server) = spawn_mock_data_server(|mut conn| {
            let mut wire = Vec::new();
            wire.extend_from_slice(&arq_frame(b"FOO"));
            wire.extend_from_slice(&arq_frame(b"BAR"));
            conn.write_all(&wire).unwrap();
        });

        let mut ds = DataSocket::connect(addr).unwrap();

        // Use a 3-byte buffer so we definitely exercise the leftover drain path.
        let mut all = Vec::new();
        let mut buf = [0u8; 3];

        // Read until we have 6 bytes (the full "FOOBAR").
        // We don't loop to EOF because the server holds the socket open until
        // the 2-second read timeout, causing a block; just read until 6.
        while all.len() < 6 {
            let n = ds.read(&mut buf).unwrap();
            assert!(n > 0, "should not get EOF before 6 bytes");
            all.extend_from_slice(&buf[..n]);
        }
        assert_eq!(&all[..6], b"FOOBAR");

        drop(ds);
        server.join().unwrap();
    }

    // ── Test 3: Non-ARQ frames (FEC) are silently skipped ────────────────

    #[test]
    fn non_arq_frames_are_skipped() {
        // Server sends FEC frame (should be ignored) then ARQ frame.
        let (addr, server) = spawn_mock_data_server(|mut conn| {
            let mut wire = Vec::new();
            // FEC frame (payload = "NOISE")
            let fec_len = (3u16 + 5).to_be_bytes();
            wire.extend_from_slice(&fec_len);
            wire.extend_from_slice(b"FEC");
            wire.extend_from_slice(b"NOISE");
            // ARQ frame (payload = "GOOD")
            wire.extend_from_slice(&arq_frame(b"GOOD"));
            conn.write_all(&wire).unwrap();
        });

        let mut ds = DataSocket::connect(addr).unwrap();
        let mut buf = vec![0u8; 64];
        let n = ds.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"GOOD", "FEC frame must be skipped");

        drop(ds);
        server.join().unwrap();
    }

    // ── Test 4: Write sends raw bytes without framing ──────────────────────

    #[test]
    fn write_sends_raw_bytes_without_framing() {
        use std::io::Read as IoRead;
        use std::sync::{Arc, Mutex};

        let received: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let received_clone = received.clone();

        let (addr, server) = spawn_mock_data_server(move |mut conn| {
            let mut buf = [0u8; 64];
            // Read until timeout (2s) or EOF.
            loop {
                match conn.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        received_clone.lock().unwrap().extend_from_slice(&buf[..n]);
                    }
                }
            }
        });

        let mut ds = DataSocket::connect(addr).unwrap();
        ds.write_all(b"WORLD").unwrap();
        ds.flush().unwrap();
        // Close write side so server exits before the 2s timeout.
        let _ = ds.stream.shutdown(std::net::Shutdown::Write);

        server.join().unwrap();
        assert_eq!(*received.lock().unwrap(), b"WORLD");
    }
}
