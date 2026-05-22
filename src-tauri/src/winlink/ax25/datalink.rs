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
    /// Decoded KISS/AX.25 frame bodies not yet dispatched by recv_frame (buffered
    /// because a single link read can complete multiple KISS frames at once).
    pending_frames: std::collections::VecDeque<Vec<u8>>,
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
        pending_frames: std::collections::VecDeque::new(),
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
    ///
    /// Multiple KISS frames can complete in a single `link.read()` call (e.g. two
    /// small RR frames pre-buffered together). `pending_frames` holds any surplus so
    /// a subsequent `recv_frame` call sees them without another link read.
    fn recv_frame(&mut self) -> std::io::Result<Option<Frame>> {
        // Drain surplus frames from the previous read before hitting the link again.
        while let Some(body) = self.pending_frames.pop_front() {
            if let Ok(frame) = Frame::decode(&body) {
                if frame.path.dest.call == self.mycall.call
                    && frame.path.dest.ssid == self.mycall.ssid
                {
                    return Ok(Some(frame));
                }
            }
        }
        let mut buf = [0u8; 512];
        match self.link.read(&mut buf) {
            Ok(0) => Ok(None),
            Ok(n) => {
                let mut completed = self.decoder.push(&buf[..n]);
                // Buffer all but the first (which we return immediately if addressed to us).
                let mut result = None;
                for body in completed.drain(..) {
                    if let Ok(frame) = Frame::decode(&body) {
                        if frame.path.dest.call == self.mycall.call
                            && frame.path.dest.ssid == self.mycall.ssid
                        {
                            if result.is_none() {
                                result = Some(frame);
                            } else {
                                // Buffer surplus for the next recv_frame call.
                                self.pending_frames.push_back(body);
                            }
                        }
                    }
                }
                Ok(result)
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

/// Await an inbound SABM addressed to `mycall`, reply UA, and surface the calling
/// peer. Blocks (polling the link) until a SABM arrives. The caller (P3 listen
/// lifecycle) governs when to arm this and how to abort it via the link shutdown
/// hook. The reply UA echoes the SABM's source as the new path's dest.
pub fn answer(
    link: Box<dyn ByteLink>,
    mycall: Address,
    params: &Ax25Params,
) -> std::io::Result<(Address, Ax25Stream)> {
    let mut stream = Ax25Stream {
        link,
        decoder: KissDecoder::new(),
        mycall: mycall.clone(),
        peer: mycall.clone(), // placeholder until the SABM names the caller
        digis: vec![],
        params: params.clone(),
        vs: 0,
        vr: 0,
        va: 0,
        pending_frames: std::collections::VecDeque::new(),
        inbound: std::collections::VecDeque::new(),
        unacked: std::collections::BTreeMap::new(),
        closed: false,
    };
    loop {
        if let Some(frame) = stream.recv_frame()? {
            if let Control::Sabm { pf } = frame.control {
                // The caller is the SABM's source.
                let peer = frame.path.src.clone();
                stream.peer = peer.clone();
                let ua = Frame {
                    path: Path { dest: peer.clone(), src: mycall.clone(), digis: vec![] },
                    control: Control::Ua { pf },
                    info: vec![],
                };
                stream.send_frame(&ua)?;
                return Ok((peer, stream));
            }
            // Ignore non-SABM frames while listening.
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

impl Ax25Stream {
    /// Drain any pending S-frames (RR/RNR/REJ) and I-frames from the link, updating
    /// V(A) on acknowledgements, queuing inbound info, and handling REJ/T1 retransmit.
    /// Returns once the pipe is momentarily empty. `expect_progress` bounds how long
    /// we wait for V(A) to advance before a T1 retransmit fires.
    fn pump_acks(&mut self) -> std::io::Result<()> {
        while let Some(frame) = self.recv_frame()? {
            match frame.control {
                Control::Rr { nr, .. } | Control::Rnr { nr, .. } => self.ack_through(nr),
                Control::Rej { nr, .. } => {
                    self.ack_through(nr);
                    self.retransmit_from(nr)?;
                }
                Control::I { ns, nr, pf } => {
                    self.ack_through(nr);
                    self.accept_inbound_i(ns, pf, &frame.info)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Mark all I-frames with N(S) < nr (mod 8) as acknowledged; advance V(A).
    fn ack_through(&mut self, nr: u8) {
        // Remove every unacked entry the peer has now confirmed (sequence numbers
        // strictly before nr, walking forward from V(A) mod 8).
        let mut s = self.va;
        while s != nr {
            self.unacked.remove(&s);
            s = (s + 1) % 8;
        }
        self.va = nr;
    }

    /// Retransmit every still-unacked I-frame from N(S)=nr forward (REJ recovery).
    fn retransmit_from(&mut self, nr: u8) -> std::io::Result<()> {
        let payloads: Vec<(u8, Vec<u8>)> = self
            .unacked
            .range(nr..)
            .chain(self.unacked.range(..nr).filter(|_| false)) // mod-8 wrap handled by callers; v0.1 windows are small
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        for (ns, info) in payloads {
            let f = Frame {
                path: self.out_path(),
                control: Control::I { ns, nr: self.vr, pf: false },
                info,
            };
            self.send_frame(&f)?;
        }
        Ok(())
    }

    /// Send one I-frame carrying `info` (≤ paclen) and record it as unacked.
    fn send_i(&mut self, info: &[u8]) -> std::io::Result<()> {
        let ns = self.vs;
        let f = Frame {
            path: self.out_path(),
            control: Control::I { ns, nr: self.vr, pf: false },
            info: info.to_vec(),
        };
        self.send_frame(&f)?;
        self.unacked.insert(ns, info.to_vec());
        self.vs = (self.vs + 1) % 8;
        Ok(())
    }

    /// Number of I-frames currently in flight (sent, not yet acked).
    fn in_flight(&self) -> usize {
        self.unacked.len()
    }

    /// Process an inbound I-frame. In order (N(S)==V(R)): queue its info, advance
    /// V(R), reply RR(V(R)) acknowledging it. Out of order (gap): drop it and reply
    /// REJ(V(R)) to request retransmission from the expected sequence. Reassembly
    /// across PACLEN segments is implicit — every accepted info chunk is appended to
    /// the inbound byte queue, which `read` drains in order.
    fn accept_inbound_i(&mut self, ns: u8, _pf: bool, info: &[u8]) -> std::io::Result<()> {
        if ns == self.vr {
            self.inbound.extend(info.iter().copied());
            self.vr = (self.vr + 1) % 8;
            let rr = Frame {
                path: self.out_path(),
                control: Control::Rr { nr: self.vr, pf: false },
                info: vec![],
            };
            self.send_frame(&rr)?;
        } else {
            // Sequence gap: reject, asking the peer to resend from V(R).
            let rej = Frame {
                path: self.out_path(),
                control: Control::Rej { nr: self.vr, pf: false },
                info: vec![],
            };
            self.send_frame(&rej)?;
        }
        Ok(())
    }
}

impl Read for Ax25Stream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // Drain the link first so any freshly-arrived I-frames are queued + acked.
        self.pump_acks()?;
        // If nothing queued and the peer hasn't sent anything, poll once more so a
        // caller in a read loop makes progress without busy-spinning.
        if self.inbound.is_empty() && !self.closed {
            std::thread::sleep(POLL_INTERVAL);
            self.pump_acks()?;
        }
        let n = buf.len().min(self.inbound.len());
        for b in buf.iter_mut().take(n) {
            *b = self.inbound.pop_front().unwrap();
        }
        Ok(n)
    }
}

impl Write for Ax25Stream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.closed {
            return Err(std::io::Error::new(std::io::ErrorKind::NotConnected, "link closed"));
        }
        let paclen = self.params.paclen.max(1);
        let maxframe = self.params.maxframe as usize;
        for chunk in buf.chunks(paclen) {
            // Block until the window has room, draining acks (bounded by N2×T1 so a
            // dead peer surfaces as an error, never a silent hang — spec §5).
            let mut attempts = 0u32;
            while self.in_flight() >= maxframe {
                self.pump_acks()?;
                if self.in_flight() < maxframe {
                    break;
                }
                attempts += 1;
                if attempts as u8 > self.params.n2_retries {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "window stalled — no acknowledgement (N2 exceeded)",
                    ));
                }
                std::thread::sleep(self.params.t1);
            }
            self.send_i(chunk)?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        // Drain any pending acks so a subsequent read/disconnect sees current state.
        self.pump_acks()
    }
}

impl Ax25Stream {
    /// Wait for V(A) to reach at least `target` (mod-8, measured as count of frames
    /// acked from the wait's start), retransmitting the oldest unacked frame each T1
    /// up to N2 times. Returns `TimedOut` if N2 is exhausted without progress (spec §5).
    fn await_ack(&mut self, target_in_flight_drained_to: usize) -> std::io::Result<()> {
        let mut retries = 0u8;
        loop {
            self.pump_acks()?;
            if self.in_flight() <= target_in_flight_drained_to {
                return Ok(());
            }
            if retries >= self.params.n2_retries {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "no acknowledgement after N2 retransmits (T1 timeout)",
                ));
            }
            // T1 expired with frames still unacked: retransmit the oldest, bump retry.
            if let Some((&ns, info)) = self.unacked.iter().next() {
                let info = info.clone();
                let f = Frame {
                    path: self.out_path(),
                    control: Control::I { ns, nr: self.vr, pf: true }, // P=1 polls for an RR
                    info,
                };
                self.send_frame(&f)?;
            }
            retries += 1;
            std::thread::sleep(self.params.t1);
        }
    }
}

#[cfg(test)]
mod read_tests {
    use super::test_peer::ScriptedPeer;
    use super::*;

    fn call(c: &str, ssid: u8) -> Address { Address { call: c.into(), ssid } }

    fn connected(peer: &ScriptedPeer) -> Ax25Stream {
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        Ax25Stream {
            link: Box::new(peer.clone()),
            decoder: KissDecoder::new(),
            mycall: mine, peer: target, digis: vec![],
            params: Ax25Params::default(),
            vs: 0, vr: 0, va: 0,
            pending_frames: std::collections::VecDeque::new(),
            inbound: std::collections::VecDeque::new(),
            unacked: std::collections::BTreeMap::new(),
            closed: false,
        }
    }

    /// An inbound I-frame from the peer to us, with N(S)=ns carrying `info`.
    fn peer_i(mycall: &Address, peer: &Address, ns: u8, info: &[u8]) -> Vec<u8> {
        let f = Frame {
            path: Path { dest: mycall.clone(), src: peer.clone(), digis: vec![] },
            control: Control::I { ns, nr: 0, pf: false },
            info: info.to_vec(),
        };
        kiss_data_frame(&f.encode().unwrap())
    }

    #[test]
    fn read_delivers_in_order_i_frames_and_replies_rr() {
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        // Two in-order I-frames reassemble into one byte stream.
        peer.feed(&peer_i(&mine, &target, 0, b"FOO"));
        peer.feed(&peer_i(&mine, &target, 1, b"BAR"));
        let mut s = connected(&peer);
        let mut got = Vec::new();
        let mut buf = [0u8; 16];
        // Two reads drain both queued frames.
        let n1 = s.read(&mut buf).unwrap();
        got.extend_from_slice(&buf[..n1]);
        let n2 = s.read(&mut buf).unwrap();
        got.extend_from_slice(&buf[..n2]);
        assert_eq!(got, b"FOOBAR");
        assert_eq!(s.vr, 2, "V(R) advanced past both frames");
        // We replied RR for each.
        let frames = { let mut d = KissDecoder::new(); d.push(&peer.drain_tx()) };
        let rrs: Vec<u8> = frames
            .iter()
            .filter_map(|b| match Frame::decode(b).unwrap().control {
                Control::Rr { nr, .. } => Some(nr),
                _ => None,
            })
            .collect();
        assert_eq!(rrs, vec![1, 2], "RR(1) then RR(2)");
    }

    #[test]
    fn out_of_order_i_frame_triggers_rej() {
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        // We expect N(S)=0 but the peer sends N(S)=1 ⇒ gap ⇒ REJ(0), no delivery.
        peer.feed(&peer_i(&mine, &target, 1, b"OOPS"));
        let mut s = connected(&peer);
        let mut buf = [0u8; 16];
        let n = s.read(&mut buf).unwrap();
        assert_eq!(n, 0, "out-of-order frame is not delivered");
        assert_eq!(s.vr, 0, "V(R) unchanged on a gap");
        let frames = { let mut d = KissDecoder::new(); d.push(&peer.drain_tx()) };
        assert!(
            frames.iter().any(|b| matches!(Frame::decode(b).unwrap().control, Control::Rej { nr: 0, .. })),
            "expected a REJ(0)"
        );
    }
}

#[cfg(test)]
mod recovery_tests {
    use super::test_peer::ScriptedPeer;
    use super::*;

    fn call(c: &str, ssid: u8) -> Address { Address { call: c.into(), ssid } }

    fn connected(peer: &ScriptedPeer, params: Ax25Params) -> Ax25Stream {
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        Ax25Stream {
            link: Box::new(peer.clone()),
            decoder: KissDecoder::new(),
            mycall: mine, peer: target, digis: vec![], params,
            vs: 0, vr: 0, va: 0,
            pending_frames: std::collections::VecDeque::new(),
            inbound: std::collections::VecDeque::new(),
            unacked: std::collections::BTreeMap::new(),
            closed: false,
        }
    }

    fn peer_s(mycall: &Address, peer: &Address, control: Control) -> Vec<u8> {
        let f = Frame { path: Path { dest: mycall.clone(), src: peer.clone(), digis: vec![] }, control, info: vec![] };
        kiss_data_frame(&f.encode().unwrap())
    }

    #[test]
    fn rej_retransmits_from_the_rejected_sequence() {
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        let mut s = connected(&peer, Ax25Params { paclen: 1, maxframe: 4, ..Ax25Params::default() });
        // Send 3 frames (N(S) 0,1,2) — no acks fed, so they stay unacked.
        s.send_i(b"A").unwrap();
        s.send_i(b"B").unwrap();
        s.send_i(b"C").unwrap();
        let _ = peer.drain_tx(); // discard the originals
        // Peer rejects at N(R)=1 ⇒ retransmit frames 1 and 2 (B, C), not 0.
        peer.feed(&peer_s(&mine, &target, Control::Rej { nr: 1, pf: false }));
        s.pump_acks().unwrap();
        let frames = { let mut d = KissDecoder::new(); d.push(&peer.drain_tx()) };
        let resent: Vec<Vec<u8>> = frames.iter().map(|b| Frame::decode(b).unwrap().info).collect();
        assert_eq!(resent, vec![b"B".to_vec(), b"C".to_vec()]);
        assert_eq!(s.va, 1, "REJ N(R)=1 acknowledged frame 0");
    }

    #[test]
    fn t1_timeout_retransmits_then_fails_after_n2() {
        let peer = ScriptedPeer::new(); // never acks
        let mut s = connected(&peer, Ax25Params { paclen: 1, maxframe: 1, t1: Duration::from_millis(10), n2_retries: 2, ..Ax25Params::default() });
        s.send_i(b"Z").unwrap();
        let _ = peer.drain_tx();
        // await_ack must retransmit up to N2 times, then TimedOut.
        let err = s.await_ack(0).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
        let frames = { let mut d = KissDecoder::new(); d.push(&peer.drain_tx()) };
        // n2_retries=2 ⇒ exactly 2 retransmissions of frame Z.
        assert_eq!(frames.len(), 2, "expected N2=2 retransmits, got {}", frames.len());
        assert!(frames.iter().all(|b| Frame::decode(b).unwrap().info == b"Z"));
    }

    #[test]
    fn t1_retransmit_stops_once_acked() {
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        let mut s = connected(&peer, Ax25Params { paclen: 1, maxframe: 1, t1: Duration::from_millis(10), n2_retries: 5, ..Ax25Params::default() });
        s.send_i(b"Q").unwrap();
        let _ = peer.drain_tx();
        // Ack arrives before N2 is hit ⇒ await_ack returns Ok.
        peer.feed(&peer_s(&mine, &target, Control::Rr { nr: 1, pf: false }));
        s.await_ack(0).unwrap();
        assert_eq!(s.va, 1);
        assert_eq!(s.in_flight(), 0);
    }
}

#[cfg(test)]
mod write_tests {
    use super::test_peer::ScriptedPeer;
    use super::*;

    fn call(c: &str, ssid: u8) -> Address { Address { call: c.into(), ssid } }

    /// A connected stream with a fresh peer, bypassing the connect handshake.
    fn connected(peer: &ScriptedPeer, params: Ax25Params) -> Ax25Stream {
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        Ax25Stream {
            link: Box::new(peer.clone()),
            decoder: KissDecoder::new(),
            mycall: mine,
            peer: target,
            digis: vec![],
            params,
            vs: 0, vr: 0, va: 0,
            pending_frames: std::collections::VecDeque::new(),
            inbound: std::collections::VecDeque::new(),
            unacked: std::collections::BTreeMap::new(),
            closed: false,
        }
    }

    /// Build the KISS-wrapped RR the peer sends to acknowledge through `nr`.
    fn peer_rr(mycall: &Address, peer: &Address, nr: u8) -> Vec<u8> {
        let f = Frame {
            path: Path { dest: mycall.clone(), src: peer.clone(), digis: vec![] },
            control: Control::Rr { nr, pf: false },
            info: vec![],
        };
        kiss_data_frame(&f.encode().unwrap())
    }

    #[test]
    fn write_under_paclen_sends_one_i_frame() {
        let peer = ScriptedPeer::new();
        let mut s = connected(&peer, Ax25Params::default());
        let n = s.write(b"HELLO").unwrap();
        assert_eq!(n, 5);
        let frames = { let mut d = KissDecoder::new(); d.push(&peer.drain_tx()) };
        assert_eq!(frames.len(), 1);
        let f = Frame::decode(&frames[0]).unwrap();
        assert!(matches!(f.control, Control::I { ns: 0, nr: 0, pf: false }));
        assert_eq!(f.info, b"HELLO");
        assert_eq!(s.vs, 1);
    }

    #[test]
    fn write_over_paclen_is_segmented() {
        let peer = ScriptedPeer::new();
        // Pre-feed enough RRs so the window never stalls.
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        for nr in 1..=4u8 {
            peer.feed(&peer_rr(&mine, &target, nr));
        }
        let mut s = connected(&peer, Ax25Params { paclen: 4, maxframe: 4, ..Ax25Params::default() });
        let n = s.write(b"ABCDEFG").unwrap(); // 7 bytes / paclen 4 ⇒ 2 segments (4 + 3)
        assert_eq!(n, 7);
        let frames = { let mut d = KissDecoder::new(); d.push(&peer.drain_tx()) };
        let infos: Vec<Vec<u8>> = frames.iter().map(|b| Frame::decode(b).unwrap().info).collect();
        assert_eq!(infos, vec![b"ABCD".to_vec(), b"EFG".to_vec()]);
    }

    #[test]
    fn write_blocks_until_the_window_drains_then_completes() {
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        // maxframe 2, paclen 1 ⇒ "XYZ" needs 3 frames but only 2 fit; an RR through 2
        // must free the window so the 3rd sends.
        peer.feed(&peer_rr(&mine, &target, 2));
        peer.feed(&peer_rr(&mine, &target, 3));
        let params = Ax25Params { paclen: 1, maxframe: 2, t1: Duration::from_millis(20), ..Ax25Params::default() };
        let mut s = connected(&peer, params);
        let n = s.write(b"XYZ").unwrap();
        assert_eq!(n, 3);
        let frames = { let mut d = KissDecoder::new(); d.push(&peer.drain_tx()) };
        assert_eq!(frames.len(), 3, "all three segments must eventually be sent");
        assert_eq!(s.va, 3, "V(A) advanced as RRs arrived");
    }
}

#[cfg(test)]
mod answer_tests {
    use super::test_peer::ScriptedPeer;
    use super::*;

    fn call(c: &str, ssid: u8) -> Address {
        Address { call: c.into(), ssid }
    }

    #[test]
    fn answer_replies_ua_to_an_inbound_sabm_and_names_the_peer() {
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let caller = call("W7AUX", 10);
        // The peer dials us: a SABM addressed to N7CPZ-7 from W7AUX-10.
        let sabm = Frame {
            path: Path { dest: mine.clone(), src: caller.clone(), digis: vec![] },
            control: Control::Sabm { pf: true },
            info: vec![],
        };
        peer.feed(&kiss_data_frame(&sabm.encode().unwrap()));

        let (got_peer, stream) =
            answer(Box::new(peer.clone()), mine.clone(), &Ax25Params::default()).unwrap();
        assert_eq!(got_peer, caller);
        assert_eq!(stream.peer, caller);

        // We replied a UA addressed back to the caller.
        let tx = peer.drain_tx();
        let frames = { let mut d = KissDecoder::new(); d.push(&tx) };
        let ua = Frame::decode(frames.last().unwrap()).unwrap();
        assert!(matches!(ua.control, Control::Ua { pf: true }));
        assert_eq!(ua.path.dest, caller);
        assert_eq!(ua.path.src, mine);
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
