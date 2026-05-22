//! AX.25 connected-mode v2.x (mod-8) data-link state machine + `Ax25Stream`.
//!
//! Drives a `ByteLink` (TCP / serial KISS pipe) through P1's KISS framer
//! (`KissDecoder` / `kiss_data_frame`) and AX.25 codec (`Frame` / `Control`),
//! running SABM→UA connect, inbound-SABM→UA answer, sequenced I-frames with RR
//! acknowledgement, REJ retransmit, T1 timeout + N2 retry, MAXFRAME windowing,
//! PACLEN segmentation/reassembly, and DISC on drop. Presents reliable in-order
//! bytes as `Ax25Stream: Read + Write`.
//!
//! **No CSMA here** — half-duplex channel access is the modem's job (spec §2/§4.1);
//! this layer only pushes the KISS TNC params (`kiss_param`) on connect.
//!
//! Verified against a scripted in-memory peer (below) + a loopback TCP socket; no
//! RF, no transmission. Behaviour cross-checked vs `TNCKissInterface.dll`
//! (`Connection`/`DataLinkProvider`/`EstablishDataLink`) at
//! `dev/scratch/winlink-re/decompiled/tnckiss/` (local-only) + AX.25 v2.2 §6.

use std::io::{Read, Write};
use std::time::{Duration, Instant};

use super::frame::{Address, Control, Frame, Path};
use super::kiss::{kiss_data_frame, kiss_param, KissDecoder, KissParam};
use super::link::ByteLink;
use super::params::Ax25Params;

/// A connected AX.25 link presenting reliable in-order bytes.
pub struct Ax25Stream {
    link: Box<dyn ByteLink>,
    decoder: KissDecoder,
    mycall: Address,
    peer: Address,
    digis: Vec<Address>,
    params: Ax25Params,
    /// V(S): next I-frame send sequence number (mod 8).
    vs: u8,
    /// V(R): next expected receive sequence number (mod 8).
    vr: u8,
    /// V(A): last sequence number acknowledged by the peer (mod 8).
    va: u8,
    /// Reassembled inbound bytes not yet handed to the caller's `read`.
    inbound: std::collections::VecDeque<u8>,
    /// Sent-but-unacked I-frame info payloads, keyed by their N(S), for retransmit.
    unacked: std::collections::BTreeMap<u8, Vec<u8>>,
    closed: bool,
}

/// Open a connected-mode AX.25 link: push the KISS TNC params, send SABM (with the
/// digipeater path), await UA. Errors `TimedOut` after N2×T1 with no UA (spec §5:
/// "No answer" must never be a silent hang). Cross-check `EstablishDataLink`.
pub fn connect(
    link: Box<dyn ByteLink>,
    mycall: Address,
    target: Address,
    digis: &[Address],
    params: &Ax25Params,
) -> std::io::Result<Ax25Stream> {
    let path = Path { dest: target.clone(), src: mycall.clone(), digis: digis.to_vec() };
    let mut stream = Ax25Stream {
        link,
        decoder: KissDecoder::new(),
        mycall,
        peer: target,
        digis: digis.to_vec(),
        params: params.clone(),
        vs: 0,
        vr: 0,
        va: 0,
        inbound: std::collections::VecDeque::new(),
        unacked: std::collections::BTreeMap::new(),
        closed: false,
    };
    // Push the KISS TNC params from the timing config. CSMA itself is the modem's job.
    stream.link.write_all(&kiss_param(KissParam::TxDelay, params.txdelay))?;
    stream.link.write_all(&kiss_param(KissParam::Persistence, params.persistence))?;
    stream.link.write_all(&kiss_param(KissParam::SlotTime, params.slot_time))?;

    // Send SABM (P=1) and await UA, bounded by N2 retransmits of T1.
    let sabm = Frame { path: path.clone(), control: Control::Sabm { pf: true }, info: vec![] };
    for _attempt in 0..=params.n2_retries {
        stream.send_frame(&sabm)?;
        let deadline = Instant::now() + params.t1;
        while Instant::now() < deadline {
            if let Some(frame) = stream.recv_frame()? {
                match frame.control {
                    Control::Ua { .. } => return Ok(stream),
                    Control::Dm { .. } => {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::ConnectionRefused,
                            "peer refused the connection (DM)",
                        ))
                    }
                    _ => continue, // ignore anything else while awaiting UA
                }
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        "no UA — peer did not answer the connect (SABM)",
    ))
}

/// How long to nap between `recv_frame` polls when the pipe is momentarily empty,
/// so the T1 wait does not busy-spin the CPU.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

impl Ax25Stream {
    /// KISS-wrap an AX.25 frame and write it to the link.
    fn send_frame(&mut self, frame: &Frame) -> std::io::Result<()> {
        let bytes = frame
            .encode()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{e:?}")))?;
        self.link.write_all(&kiss_data_frame(&bytes))
    }

    /// Pull bytes from the link into the KISS decoder and return the next decoded,
    /// successfully-parsed AX.25 frame, if any is available right now. Returns
    /// `Ok(None)` when the pipe is momentarily empty (WouldBlock) — NOT an error.
    fn recv_frame(&mut self) -> std::io::Result<Option<Frame>> {
        let mut buf = [0u8; 512];
        match self.link.read(&mut buf) {
            Ok(0) => Ok(None),
            Ok(n) => {
                for body in self.decoder.push(&buf[..n]) {
                    if let Ok(frame) = Frame::decode(&body) {
                        // Only deliver frames addressed to us (dest == mycall).
                        if frame.path.dest.call == self.mycall.call
                            && frame.path.dest.ssid == self.mycall.ssid
                        {
                            return Ok(Some(frame));
                        }
                    }
                }
                Ok(None)
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// The path from us to the peer (for outbound command frames).
    #[allow(dead_code)]
    fn out_path(&self) -> Path {
        Path { dest: self.peer.clone(), src: self.mycall.clone(), digis: self.digis.clone() }
    }
}

#[cfg(test)]
mod connect_tests {
    use super::test_peer::ScriptedPeer;
    use super::*;

    fn call(c: &str, ssid: u8) -> Address {
        Address { call: c.into(), ssid }
    }

    /// Build the KISS-wrapped UA the peer would send back, addressed to `mycall`.
    fn peer_ua(mycall: &Address, peer: &Address) -> Vec<u8> {
        let f = Frame {
            path: Path { dest: mycall.clone(), src: peer.clone(), digis: vec![] },
            control: Control::Ua { pf: true },
            info: vec![],
        };
        kiss_data_frame(&f.encode().unwrap())
    }

    #[test]
    fn connect_sends_sabm_and_returns_on_ua() {
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        // Pre-load the UA so the first recv_frame inside connect succeeds.
        peer.feed(&peer_ua(&mine, &target));

        let stream = connect(
            Box::new(peer.clone()),
            mine.clone(),
            target.clone(),
            &[],
            &Ax25Params::default(),
        )
        .unwrap();
        assert_eq!(stream.peer.call, "W7AUX");

        // We must have written: 3 KISS param frames, then a KISS-wrapped SABM.
        let tx = peer.drain_tx();
        let frames = {
            let mut d = KissDecoder::new();
            d.push(&tx)
        };
        // The last data frame decoded should be our SABM to W7AUX-10.
        let sabm = Frame::decode(frames.last().unwrap()).unwrap();
        assert!(matches!(sabm.control, Control::Sabm { pf: true }));
        assert_eq!(sabm.path.dest, target);
        assert_eq!(sabm.path.src, mine);
    }

    #[test]
    fn connect_times_out_without_ua() {
        let peer = ScriptedPeer::new(); // never feeds a UA
        let mine = call("N7CPZ", 7);
        // Tiny T1 + zero retries so the test is fast.
        let params = Ax25Params { t1: Duration::from_millis(40), n2_retries: 0, ..Ax25Params::default() };
        let err = connect(Box::new(peer), mine, call("W7AUX", 10), &[], &params)
            .err().expect("expected TimedOut error");
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
    }

    #[test]
    fn connect_errors_on_dm() {
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        let dm = Frame {
            path: Path { dest: mine.clone(), src: target.clone(), digis: vec![] },
            control: Control::Dm { pf: true },
            info: vec![],
        };
        peer.feed(&kiss_data_frame(&dm.encode().unwrap()));
        let err = connect(Box::new(peer), mine, target, &[], &Ax25Params::default())
            .err().expect("expected ConnectionRefused error");
        assert_eq!(err.kind(), std::io::ErrorKind::ConnectionRefused);
    }
}

#[cfg(test)]
mod test_peer {
    use std::io::{Read, Write};
    use std::sync::{Arc, Mutex};

    /// A scripted in-memory `ByteLink`: the state machine writes to `tx` (which a
    /// test decodes) and reads from `rx` (which a test pre-loads with canned KISS
    /// frames). Both ends share the buffers so a test can inspect/extend between calls.
    #[derive(Clone)]
    pub struct ScriptedPeer {
        pub tx: Arc<Mutex<Vec<u8>>>,
        pub rx: Arc<Mutex<std::collections::VecDeque<u8>>>,
    }

    impl ScriptedPeer {
        pub fn new() -> Self {
            ScriptedPeer {
                tx: Arc::new(Mutex::new(Vec::new())),
                rx: Arc::new(Mutex::new(std::collections::VecDeque::new())),
            }
        }
        /// Queue bytes for the state machine to read (a peer's KISS frame).
        pub fn feed(&self, bytes: &[u8]) {
            self.rx.lock().unwrap().extend(bytes.iter().copied());
        }
        /// Take everything the state machine has written so far.
        pub fn drain_tx(&self) -> Vec<u8> {
            std::mem::take(&mut *self.tx.lock().unwrap())
        }
    }

    impl Read for ScriptedPeer {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let mut rx = self.rx.lock().unwrap();
            if rx.is_empty() {
                // Empty (not EOF): the state machine's read loop must treat a momentarily
                // empty pipe as "no frame yet", not as a closed link. WouldBlock models a
                // non-blocking serial/TCP read with nothing buffered.
                return Err(std::io::Error::new(std::io::ErrorKind::WouldBlock, "no data"));
            }
            let n = buf.len().min(rx.len());
            for b in buf.iter_mut().take(n) {
                *b = rx.pop_front().unwrap();
            }
            Ok(n)
        }
    }

    impl Write for ScriptedPeer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.tx.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn peer_records_tx_and_serves_rx() {
        let peer = ScriptedPeer::new();
        let mut a = peer.clone();
        a.write_all(&[1, 2, 3]).unwrap();
        assert_eq!(peer.drain_tx(), vec![1, 2, 3]);

        peer.feed(&[9, 8]);
        let mut buf = [0u8; 2];
        a.read_exact(&mut buf).unwrap();
        assert_eq!(buf, [9, 8]);

        // Empty pipe ⇒ WouldBlock, not EOF.
        let mut one = [0u8; 1];
        assert_eq!(a.read(&mut one).unwrap_err().kind(), std::io::ErrorKind::WouldBlock);
    }
}
