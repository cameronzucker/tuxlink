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
//! - **Outbound:** each write is framed as `[u16 BE payload-length][payload]`.
//!   One frame carries at most 65535 payload bytes. No `D:` prefix, no CRC,
//!   no 3-byte type tag (those are inbound-only). This matches wl2k-go
//!   `transport/ardop/conn.go` `tncConn::Write`.
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
use std::time::Duration;

use super::arq_state::ArqState;
use super::frame::{DataDecoder, DataKind};

/// Poll interval for the EOF-on-DISC gate (tuxlink-ytg). With an `ArqState`
/// wired, the data socket sets this as its read timeout so a `read` blocked
/// on a quiet TCP wakes up at this cadence to check `arq_state.is_connected`.
/// Short enough to make a DISC visible promptly (sub-second), long enough to
/// keep the syscall rate negligible on a quiet link.
const ARQ_STATE_POLL_INTERVAL: Duration = Duration::from_millis(250);

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
    /// Shared cmd↔data ARQ link-state flag (tuxlink-ytg). When wired:
    ///
    /// - `read` returns `Ok(0)` (EOF) while `Disconnected` AND `leftover` is
    ///   empty — even if the TCP data socket itself is still open. The cmd
    ///   reader thread flips this on DISCONNECTED / NEWSTATE DISC events,
    ///   unblocking the B2F engine.
    /// - Inbound ARQ frames decoded while `Disconnected` are dropped, so
    ///   stale RF data emitted on the data socket before the session is up
    ///   cannot contaminate the first B2F handshake read.
    /// - `write` returns `BrokenPipe` if the ARQ link is not connected.
    ///
    /// `None` for callers that don't share state (the standalone DataSocket
    /// tests).
    arq_state: Option<ArqState>,
}

impl DataSocket {
    /// Open the ARDOP data socket at `addr`. No ARQ-state coordination.
    pub fn connect(addr: SocketAddr) -> io::Result<Self> {
        Self::connect_with_arq_state(addr, None)
    }

    /// Like [`connect`] but also share an [`ArqState`] with the cmd socket
    /// (tuxlink-ytg): the cmd reader thread updates it on CONNECTED /
    /// DISCONNECTED / NEWSTATE DISC, and this DataSocket's `read` / `write`
    /// observe it as described on the struct.
    ///
    /// When `arq_state` is `Some`, a read timeout of
    /// [`ARQ_STATE_POLL_INTERVAL`] is set on the underlying TCP stream so a
    /// `read` blocked on a quiet socket wakes up at that cadence to consult
    /// the flag. A failed `set_read_timeout` is non-fatal — the read path
    /// would still get EOF on real TCP close; only the "data socket stays
    /// open after DISC" wake-up is degraded.
    pub fn connect_with_arq_state(
        addr: SocketAddr,
        arq_state: Option<ArqState>,
    ) -> io::Result<Self> {
        let stream = TcpStream::connect(addr)?;
        if arq_state.is_some() {
            let _ = stream.set_read_timeout(Some(ARQ_STATE_POLL_INTERVAL));
        }
        Ok(DataSocket {
            stream,
            decoder: DataDecoder::default(),
            leftover: VecDeque::new(),
            arq_state,
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
    ///
    /// tuxlink-ytg: when `arq_state` is wired AND not yet connected, drop ARQ
    /// frames rather than buffer them — ardopcf can emit monitored / non-
    /// session ARQ-tagged traffic on the data socket *before* the ARQ link is
    /// up; without this gate, that stale traffic would contaminate the first
    /// B2F handshake read.
    ///
    /// **Timing edge.** The drop runs at *decode* time, which only happens when
    /// `read()` is called. Bytes that arrive on the TCP socket before any
    /// `read()` call sit in the OS receive buffer until the first `read()`
    /// post-connect. Since the operator workflow today only invokes B2F via
    /// `modem_ardop_b2f_exchange` AFTER `modem_ardop_connect` has succeeded
    /// (and `arq_state` is therefore already `Connected` by the time any
    /// `read()` happens), the gate's drop-at-decode-time is sufficient. A
    /// future "drain stale bytes at the connect transition" hook can be added
    /// if a path that reads pre-connect appears.
    fn pump_decoder(&mut self) {
        let arq_up = self
            .arq_state
            .as_ref()
            .map(|s| s.is_connected())
            .unwrap_or(true);
        while let Some(frame) = self.decoder.next_frame() {
            if frame.kind == DataKind::Arq && arq_up {
                // tuxlink-n2uz: account in-session ARQ payload toward the
                // cumulative bytes_rx counter at the same gate that admits the
                // payload to `leftover`. Bytes dropped by the gate (pre-connect
                // noise, non-ARQ frames) do NOT count — the counter represents
                // session payload the B2F engine will see.
                if let Some(ref state) = self.arq_state {
                    state.add_bytes_rx(frame.payload.len() as u64);
                }
                self.leftover.extend(frame.payload);
            }
            // - Non-ARQ frames (FEC / ERR / IDF / Other): not B2F session data.
            // - ARQ frames decoded while `!arq_up`: stale pre-connect noise.
            // Either way: skip (and do not count toward bytes_rx).
        }
    }

    /// Is the ARQ link still considered up by the cmd-socket bookkeeping?
    /// Returns `true` when no ArqState is wired (the standalone test path).
    fn arq_connected(&self) -> bool {
        self.arq_state
            .as_ref()
            .map(|s| s.is_connected())
            .unwrap_or(true)
    }

    /// Discard any bytes currently sitting in the OS receive buffer of the
    /// data socket (tuxlink-ytg P1, Codex adrev 2026-05-30).
    ///
    /// Used by [`crate::winlink::modem::ardop::transport::ArdopTransport::connect_arq`]
    /// immediately after a successful ARQ-handshake. Without this drain, any
    /// ARQ-tagged bytes that arrived between `init()` opening the data socket
    /// and the `CONNECTED` event flipping `ArqState` would be silently
    /// accepted as session payload on the first post-connect `read()` — the
    /// `pump_decoder` drop gate only fires when the flag is `Disconnected`
    /// AT DECODE TIME, not for bytes received earlier and decoded later.
    ///
    /// Returns the number of raw socket bytes consumed and discarded. The
    /// decoder is also reset, so any partial frame held mid-parse is cleared
    /// — preserving the invariant that post-drain reads only see post-connect
    /// data.
    ///
    /// **Blocking-mode discipline:** sets the underlying TCP stream to a very
    /// short read timeout for the drain, then restores the prior timeout on
    /// every exit path (success AND error). The prior timeout is whatever
    /// `connect_with_arq_state` configured ([`ARQ_STATE_POLL_INTERVAL`]) or
    /// what the caller has otherwise set; we never leave the socket with the
    /// drain-window timeout.
    pub fn drain_pending(&mut self) -> io::Result<usize> {
        // Snapshot the prior read-timeout so we can restore it on every exit.
        let prior_timeout = self.stream.read_timeout().ok().flatten();
        // A tiny read-timeout is the portable "non-blocking drain": each
        // syscall either returns bytes immediately or fails with WouldBlock /
        // TimedOut after the deadline, so the loop terminates cleanly without
        // touching the socket's nonblocking flag (which is finicky under
        // `try_clone` siblings — TimedOut/WouldBlock at OS level may differ).
        let drain_window = Duration::from_millis(1);
        if let Err(e) = self.stream.set_read_timeout(Some(drain_window)) {
            // If we can't set a short timeout, restore the prior one and
            // surface the error rather than risk a long block in the drain
            // loop below.
            let _ = self.stream.set_read_timeout(prior_timeout);
            return Err(e);
        }

        let mut total = 0;
        let mut scratch = [0u8; 8192];
        let result = loop {
            match self.stream.read(&mut scratch) {
                Ok(0) => break Ok(total), // peer closed — nothing more to drain
                Ok(n) => total += n,
                Err(e)
                    if e.kind() == io::ErrorKind::WouldBlock
                        || e.kind() == io::ErrorKind::TimedOut =>
                {
                    // No more bytes immediately available — drain is done.
                    break Ok(total);
                }
                Err(e) => break Err(e),
            }
        };

        // Restore the prior timeout regardless of result, so post-drain reads
        // observe the same blocking discipline as before the drain.
        let _ = self.stream.set_read_timeout(prior_timeout);

        // Reset the decoder so any mid-parse partial frame is discarded too —
        // bytes that landed in the decoder before the connect transition are
        // pre-session noise just like bytes still in the OS buffer.
        self.decoder = DataDecoder::default();
        self.leftover.clear();

        result
    }
}

impl Read for DataSocket {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        // 1. Drain any already-decoded leftover payload first. Leftover bytes
        //    were buffered while `arq_up` was true; even after a subsequent
        //    DISC, the B2F engine should finish processing what arrived
        //    in-session before seeing EOF.
        if !self.leftover.is_empty() {
            return Ok(self.drain_leftover(buf));
        }

        // 2. Loop: read from socket → push to decoder → pump until we either
        //    get payload bytes or hit EOF.
        //
        // tuxlink-ytg: when `arq_state` is wired, the read timeout
        // (ARQ_STATE_POLL_INTERVAL) makes a TimedOut error a normal "no data
        // yet" signal — re-check the flag and either keep reading (still
        // connected) or surface EOF (DISC observed on the cmd socket).
        //
        // Inbound ARQ frames arriving while `arq_up == false` are silently
        // dropped by `pump_decoder` (the pre-connect / post-DISC frame-drop
        // gate), so a stale ARQ-tagged frame can never surface as session
        // data even if the data TCP socket carried it before the link was up.
        let mut raw = [0u8; 4096];
        loop {
            match self.stream.read(&mut raw) {
                Ok(0) => {
                    // Real socket EOF — signal EOF to the B2F engine.
                    return Ok(0);
                }
                Ok(n) => {
                    self.decoder.push(&raw[..n]);
                    self.pump_decoder(); // drops ARQ frames if !arq_up
                    if !self.leftover.is_empty() {
                        return Ok(self.drain_leftover(buf));
                    }
                    // No payload buffered (partial frame, non-ARQ frame, or
                    // dropped stale frame). If the cmd socket has reported
                    // DISC, surface EOF — there's nothing more coming that
                    // belongs to this session.
                    if !self.arq_connected() {
                        return Ok(0);
                    }
                    // Still connected; loop and read more.
                }
                Err(e)
                    if e.kind() == io::ErrorKind::WouldBlock
                        || e.kind() == io::ErrorKind::TimedOut =>
                {
                    // tuxlink-ytg: read-timeout fired (set up when `arq_state`
                    // is wired). Re-check the ARQ-state flag — if the cmd
                    // socket has reported DISC, surface EOF; otherwise loop
                    // and read again. This is a low-rate poll (every
                    // `ARQ_STATE_POLL_INTERVAL`), not a busy spin.
                    if !self.arq_connected() {
                        return Ok(0);
                    }
                    // Still connected; loop and read again.
                }
                Err(e) => return Err(e),
            }
        }
    }
}

impl Write for DataSocket {
    /// Frame `buf` as `[u16 BE length][payload]` and write to the data socket.
    ///
    /// One frame carries at most 65535 payload bytes (the u16 maximum). If
    /// `buf` is longer than 65535 bytes, only the first 65535 bytes are sent
    /// and `Ok(65535)` is returned so that `write_all` loops correctly for
    /// larger buffers.
    ///
    /// Returns the number of **payload** bytes consumed (not counting the
    /// 2-byte length prefix).
    ///
    /// tuxlink-ytg: when `arq_state` is wired AND not connected, refuses the
    /// write with `BrokenPipe`. The structural guarantee (the Tauri command
    /// only installs the transport after `connect_arq` succeeds) keeps this
    /// unreachable from the operator flow today, but the explicit check
    /// catches a B2F engine that keeps trying to send after the cmd socket
    /// has reported DISC mid-exchange.
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if !self.arq_connected() {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "ARDOP ARQ link is not connected — refusing data-socket write",
            ));
        }
        let n = buf.len().min(65535);
        // Build the frame in a single allocation: [u16 BE length][payload]
        let mut frame = Vec::with_capacity(2 + n);
        frame.extend_from_slice(&(n as u16).to_be_bytes());
        frame.extend_from_slice(&buf[..n]);
        self.stream.write_all(&frame)?;
        Ok(n)
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

    // ── Test 4: Write sends framed bytes (u16 BE length prefix + payload) ──

    #[test]
    fn write_sends_framed_bytes() {
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
        // The server must see framed bytes: [0x00, 0x05] (length=5) followed by "WORLD"
        assert_eq!(
            *received.lock().unwrap(),
            vec![0x00, 0x05, b'W', b'O', b'R', b'L', b'D'],
            "write must send [u16 BE length][payload]"
        );
    }

    // ── Test 5: Write of >65535 bytes caps at 65535, returns Ok(65535) ────

    #[test]
    fn write_over_65535_bytes_caps_at_frame_max() {
        use std::io::Read as IoRead;
        use std::sync::{Arc, Mutex};

        let received: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let received_clone = received.clone();

        // Server reads up to 65537 + 2 bytes (frame header + max payload + some slack).
        // A large 512 KiB read buffer is used to drain a 65535-byte payload in one pass.
        let (addr, server) = spawn_mock_data_server(move |mut conn| {
            let mut buf = vec![0u8; 524288]; // 512 KiB — enough for header + full max payload
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

        // Build a payload of 65536 bytes (one over the max).
        let big = vec![0xABu8; 65536];
        // `write` (not `write_all`) must return Ok(65535) — capped at one frame.
        let n = ds.write(&big).unwrap();
        assert_eq!(n, 65535, "write must cap at 65535 payload bytes per frame");

        ds.flush().unwrap();
        let _ = ds.stream.shutdown(std::net::Shutdown::Write);

        server.join().unwrap();

        let got = received.lock().unwrap().clone();
        // The framed output must be exactly 2 (header) + 65535 (payload) = 65537 bytes.
        assert_eq!(got.len(), 65537, "framed output must be 2-byte header + 65535 payload");
        // Header must encode 0xFFFF (65535).
        assert_eq!(&got[..2], &[0xFF, 0xFF], "length prefix must be 0xFFFF");
        // Payload must be exactly 65535 repetitions of 0xAB.
        assert!(
            got[2..].iter().all(|&b| b == 0xAB),
            "payload must be the first 65535 bytes of the input"
        );
    }

    // ── Test 6: ARQ-state gate — write before connect is BrokenPipe (tuxlink-ytg) ──

    #[test]
    fn write_refused_when_arq_state_not_connected() {
        // tuxlink-ytg: an ArqState in default (disconnected) state must make
        // write fail with BrokenPipe rather than push frame bytes that the
        // peer would parse out of session.
        let (addr, _server) = spawn_mock_data_server(|mut conn: TcpStream| {
            // Server idles; this test asserts the client never even writes.
            let _ = conn.set_read_timeout(Some(Duration::from_millis(500)));
            let mut buf = [0u8; 32];
            let _ = conn.read(&mut buf);
        });

        let arq_state = super::super::arq_state::ArqState::new(); // disconnected
        let mut ds = DataSocket::connect_with_arq_state(addr, Some(arq_state)).unwrap();
        let err = ds
            .write(b"WORLD")
            .expect_err("write must be refused while ARQ disconnected");
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe, "got {err:?}");
    }

    // ── Test 7: ARQ-state gate — write allowed once connect flag flips ────

    #[test]
    fn write_allowed_after_arq_state_marks_connected() {
        // tuxlink-ytg: when the cmd reader thread flips the flag to Connected,
        // the same DataSocket starts accepting writes. Exercised via the
        // ArqState directly — production wires this through the CmdSocket
        // event loop.
        use std::io::Read as IoRead;
        use std::sync::{Arc, Mutex};

        let received: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let received_clone = received.clone();
        let (addr, server) = spawn_mock_data_server(move |mut conn| {
            let mut buf = [0u8; 64];
            loop {
                match conn.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => received_clone.lock().unwrap().extend_from_slice(&buf[..n]),
                }
            }
        });

        let arq_state = super::super::arq_state::ArqState::new();
        let mut ds = DataSocket::connect_with_arq_state(addr, Some(arq_state.clone())).unwrap();
        arq_state.set_connected();
        ds.write_all(b"HI").unwrap();
        ds.flush().unwrap();
        let _ = ds.stream.shutdown(std::net::Shutdown::Write);
        server.join().unwrap();

        assert_eq!(
            *received.lock().unwrap(),
            vec![0x00, 0x02, b'H', b'I'],
            "after ARQ connect, the wire must carry the framed bytes"
        );
    }

    // ── Test 8: ARQ-state gate — read returns EOF when flag flips ─────────

    #[test]
    fn read_returns_eof_when_arq_state_flips_to_disconnected() {
        // tuxlink-ytg: a quiet-but-still-open data TCP socket would otherwise
        // hang a blocked read_line forever after an on-air DISC. The
        // ArqState's set_disconnected (called by the cmd reader thread) must
        // make the next read return Ok(0).
        let (addr, server) = spawn_mock_data_server(|mut conn| {
            // Hold the socket open without writing anything — the OS-level
            // FIN won't fire; the wake-up has to come from the ArqState path.
            let _ = conn.set_read_timeout(Some(Duration::from_secs(3)));
            let mut buf = [0u8; 32];
            let _ = conn.read(&mut buf);
        });

        let arq_state = super::super::arq_state::ArqState::new();
        arq_state.set_connected(); // start in the connected state
        let mut ds = DataSocket::connect_with_arq_state(addr, Some(arq_state.clone())).unwrap();

        // From a separate thread, flip the state to disconnected after a beat
        // so the blocked read in the main thread can wake up via the read
        // timeout poll + flag-check and return EOF.
        let arq_state_clone = arq_state.clone();
        let flipper = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            arq_state_clone.set_disconnected();
        });

        let mut buf = vec![0u8; 64];
        let start = std::time::Instant::now();
        let n = ds.read(&mut buf).unwrap();
        let elapsed = start.elapsed();
        assert_eq!(n, 0, "read must return EOF on ARQ disconnect");
        // Sanity-check the wake-up was prompt (sub-second), not "hang till TCP closes".
        assert!(
            elapsed < Duration::from_secs(2),
            "EOF on DISC must arrive promptly; took {elapsed:?}"
        );

        flipper.join().unwrap();
        drop(ds);
        server.join().unwrap();
    }

    // ── Test 8b: drain_pending discards pre-connect OS-buffered bytes (tuxlink-ytg P1) ──

    /// Codex adrev 2026-05-30 P1 #2: when ARQ-tagged data arrives on the
    /// data socket BEFORE the ARQ-connect handshake completes, those bytes
    /// sit in the OS receive buffer until the first post-connect `read()` —
    /// by which time the flag has flipped and the `pump_decoder` drop gate
    /// no longer fires. `drain_pending` discards those bytes at the connect
    /// transition so they cannot corrupt the B2F handshake.
    #[test]
    fn drain_pending_discards_pre_connect_os_buffered_bytes() {
        // Mock writes some ARQ-frame-looking bytes immediately on connect,
        // then idles (holding the socket open) so we can observe the drain
        // and a subsequent read in the same test.
        let (addr, server) = spawn_mock_data_server(|mut conn: TcpStream| {
            // Bytes that would otherwise corrupt the post-connect B2F read.
            conn.write_all(&arq_frame(b"PRECONNECT-NOISE")).unwrap();
            // Hold the connection open so the client side can call drain
            // before the server's read timeout fires the close.
            let _ = conn.set_read_timeout(Some(Duration::from_secs(2)));
            let mut buf = [0u8; 32];
            let _ = conn.read(&mut buf);
        });

        // Start in the disconnected state — mirrors the production timing
        // where bytes accumulate before the cmd-reader flips the flag.
        let arq_state = super::super::arq_state::ArqState::new();
        let mut ds = DataSocket::connect_with_arq_state(addr, Some(arq_state.clone())).unwrap();

        // Wait long enough for the server's write to land in the client's OS
        // recv buffer.
        std::thread::sleep(Duration::from_millis(50));

        let drained = ds.drain_pending().expect("drain_pending must succeed");
        assert!(
            drained > 0,
            "drain_pending should report bytes consumed from the OS buffer; got {drained}"
        );

        // Flip to connected (production: the cmd reader does this just before
        // arq_connect returns Ok); the post-drain read must NOT see the
        // pre-connect noise.
        arq_state.set_connected();

        // Subsequent read should time out / wouldblock — no bytes remain.
        // The default read-timeout from connect_with_arq_state is
        // ARQ_STATE_POLL_INTERVAL (~250ms); we expect at least one full poll
        // cycle to fire WouldBlock + arq_connected() check → Ok(0)/loop, but
        // since no further bytes ever arrive, we just verify the leftover
        // buffer is empty post-drain.
        assert!(
            ds.leftover.is_empty(),
            "leftover must be empty after drain; got {:?}",
            ds.leftover
        );

        drop(ds);
        server.join().unwrap();
    }

    // ── Test 8c: drain_pending restores prior read-timeout on success ─────

    /// `drain_pending` uses a 1 ms read-timeout internally; it MUST restore
    /// whatever timeout was set before so post-drain reads keep their
    /// blocking discipline (in particular the `ARQ_STATE_POLL_INTERVAL`
    /// poll that backstops the EOF-on-DISC gate). Regression guard for
    /// "drain leaves the socket in a 1ms-poll state forever."
    #[test]
    fn drain_pending_restores_prior_read_timeout() {
        let (addr, server) = spawn_mock_data_server(|mut conn: TcpStream| {
            // Write some bytes so the drain has something to consume.
            let _ = conn.write_all(&arq_frame(b"X"));
            let _ = conn.set_read_timeout(Some(Duration::from_secs(2)));
            let mut buf = [0u8; 16];
            let _ = conn.read(&mut buf);
        });

        let arq_state = super::super::arq_state::ArqState::new();
        let mut ds = DataSocket::connect_with_arq_state(addr, Some(arq_state)).unwrap();

        // Sanity: connect_with_arq_state set the ARQ_STATE_POLL_INTERVAL.
        let before = ds.stream.read_timeout().unwrap();
        assert!(before.is_some(), "expected a read timeout from connect_with_arq_state");

        std::thread::sleep(Duration::from_millis(50));
        let _ = ds.drain_pending().unwrap();

        let after = ds.stream.read_timeout().unwrap();
        assert_eq!(
            after, before,
            "drain_pending must restore the prior read timeout (got {after:?}, expected {before:?})"
        );

        drop(ds);
        server.join().unwrap();
    }

    // ── Test 9: pump_decoder drops ARQ frames while ArqState disconnected ─

    #[test]
    fn pump_decoder_drops_arq_frames_while_arq_state_disconnected() {
        // tuxlink-ytg: the pre-connect / post-DISC frame-drop gate. If a
        // stale ARQ frame is decoded while `arq_up == false`, it MUST NOT
        // land in `leftover` — even if a subsequent read happens after the
        // flag flips. The protection runs at decode time.
        //
        // Tests the gate at the unit level via direct decoder push: feeds
        // ARQ-frame bytes into a DataSocket whose ArqState is disconnected,
        // calls pump_decoder, and confirms `leftover` is empty.
        let arq_state = super::super::arq_state::ArqState::new(); // disconnected

        // Build a DataSocket pointing at a closed loopback (we never call
        // `read`, only push directly into the decoder).
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server_thread = thread::spawn(move || {
            let _ = listener.accept(); // accept then immediately drop the conn
        });
        let mut ds =
            DataSocket::connect_with_arq_state(addr, Some(arq_state.clone())).unwrap();
        server_thread.join().unwrap();

        // Push an ARQ-framed payload directly into the decoder, then pump.
        let stale = arq_frame(b"STALE");
        ds.decoder.push(&stale);
        ds.pump_decoder();
        assert!(
            ds.leftover.is_empty(),
            "pre-connect ARQ frame must be dropped by pump_decoder, got {:?}",
            ds.leftover
        );

        // Same frame pushed AFTER the flag flips must be accepted.
        arq_state.set_connected();
        let live = arq_frame(b"LIVE");
        ds.decoder.push(&live);
        ds.pump_decoder();
        let buffered: Vec<u8> = ds.leftover.iter().copied().collect();
        assert_eq!(
            buffered, b"LIVE",
            "post-connect ARQ frame must be buffered for the next read"
        );
    }
}
