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
    /// Peer-flow-control flag: set by an inbound RNR (remote receiver busy), cleared
    /// by RR. While true, `write` must not send new I-frames (AX.25 v2.2 §6.4.6;
    /// reference `Connection.cs` `remoteBusy`).
    remote_busy: bool,
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
        remote_busy: false,
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
                // Fix D/H: only accept UA/DM from the station we dialed; a foreign
                // frame addressed to our call must not complete or refuse this connect.
                if frame.path.src.call != stream.peer.call
                    || frame.path.src.ssid != stream.peer.ssid
                {
                    continue;
                }
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
            // A short read-poll timeout (TCP `set_read_timeout` / serialport timeout)
            // surfaces as WouldBlock (Unix `EAGAIN`) or TimedOut depending on the
            // transport — both mean "no frame yet", not a broken link (fix H).
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                Ok(None)
            }
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
///
/// Defect L: unlike `connect`, `answer` does NOT push KISS TNC params (TXdelay /
/// persistence / slot) — the P3 listen lifecycle pushes them once when it arms the
/// link, so re-pushing on every accepted SABM would be redundant.
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
        remote_busy: false,
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
            // Fix D: on a shared RF channel any station may emit a frame addressed to
            // our call; processing a foreign station's S/I-frame would corrupt our
            // window and inbound state. Only frames from our connected peer count.
            // (The connect/answer handshakes route by their own peer checks; this
            // guard governs the CONNECTED data path. The reference routes by the
            // reversed connection path.)
            if frame.path.src.call != self.peer.call || frame.path.src.ssid != self.peer.ssid {
                continue;
            }
            match frame.control {
                // Fix I: RNR is remote-busy backpressure — ack through, but mark the
                // peer busy so `write` stops sending new I-frames until an RR clears it.
                Control::Rnr { nr, .. } => {
                    self.ack_through(nr);
                    self.remote_busy = true;
                }
                Control::Rr { nr, .. } => {
                    self.ack_through(nr);
                    self.remote_busy = false;
                }
                Control::Rej { nr, .. } => {
                    self.ack_through(nr);
                    self.remote_busy = false;
                    self.retransmit_from(nr)?;
                }
                Control::I { ns, nr, pf } => {
                    self.ack_through(nr);
                    self.accept_inbound_i(ns, pf, &frame.info)?;
                }
                // Fix H: the peer hung up. Reply UA and mark the link closed so the
                // owner notices (reference `DoConnectedStateRemote`: DISC ⇒ reply UA,
                // DisconnectIndication, state Disconnected).
                Control::Disc { pf } => {
                    let ua = Frame {
                        path: self.out_path(),
                        control: Control::Ua { pf },
                        info: vec![],
                    };
                    self.send_frame(&ua)?;
                    self.closed = true;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Mark all I-frames with N(S) < nr (mod 8) as acknowledged; advance V(A).
    ///
    /// A valid cumulative N(R) acknowledges no more than the count outstanding:
    /// `(nr - va) mod 8 <= (vs - va) mod 8`. An out-of-window N(R) (a buggy or
    /// foreign frame) is ignored entirely — we neither walk the window nor move
    /// V(A) (fix B; reference `Connection.cs::ErrorRecovery`, which restarts the
    /// link on this condition; we conservatively drop the frame).
    fn ack_through(&mut self, nr: u8) {
        let acked = (nr + 8 - self.va) % 8;
        let outstanding = (self.vs + 8 - self.va) % 8;
        if acked > outstanding {
            return; // out-of-window N(R): ignore
        }
        let mut s = self.va;
        while s != nr {
            self.unacked.remove(&s);
            s = (s + 1) % 8;
        }
        self.va = nr;
    }

    /// Retransmit every still-unacked I-frame from N(S)=nr forward through V(S),
    /// walking mod-8 so a window that has wrapped past 7→0 resends in N(S) order
    /// (REJ recovery; AX.25 v2.2 §6.4.7). The prior `.range(nr..)` body silently
    /// dropped the wrapped frames — fix A.
    fn retransmit_from(&mut self, nr: u8) -> std::io::Result<()> {
        let mut s = nr;
        while s != self.vs {
            if let Some(info) = self.unacked.get(&s).cloned() {
                let f = Frame {
                    path: self.out_path(),
                    control: Control::I { ns: s, nr: self.vr, pf: false },
                    info,
                };
                self.send_frame(&f)?;
            }
            s = (s + 1) % 8;
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
    fn accept_inbound_i(&mut self, ns: u8, pf: bool, info: &[u8]) -> std::io::Result<()> {
        // Fix G: a polled (P=1) I-frame requires an F=1 reply; echo the incoming poll
        // bit into the Final bit of our RR/REJ (reference calls `EnquiryResponse(1)`
        // when pfBit == 1).
        if ns == self.vr {
            self.inbound.extend(info.iter().copied());
            self.vr = (self.vr + 1) % 8;
            let rr = Frame {
                path: self.out_path(),
                control: Control::Rr { nr: self.vr, pf },
                info: vec![],
            };
            self.send_frame(&rr)?;
        } else {
            // Sequence gap: reject, asking the peer to resend from V(R). REJ is sent
            // per out-of-order frame here — no REJSent dedup (acceptable for v0.1;
            // future optimization, defect K).
            let rej = Frame {
                path: self.out_path(),
                control: Control::Rej { nr: self.vr, pf },
                info: vec![],
            };
            self.send_frame(&rej)?;
        }
        Ok(())
    }
}

impl Read for Ax25Stream {
    /// Read reassembled inbound bytes.
    ///
    /// **Contract (defect J — settled at P4 wiring):** `Ok(0)` here means "no data
    /// available right now" (the link is still open), NOT end-of-stream. The P4 B2F
    /// driver (`run_exchange`) MUST NOT treat `Ok(0)` as EOF; it must loop/retry
    /// until the session completes (peer DISC, our disconnect, or a higher-layer
    /// end-of-message marker). A genuinely closed link is signalled by `self.closed`
    /// (set on an inbound DISC) plus an empty inbound queue, not by `Ok(0)` alone.
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
        // Fix F: clamp the effective window to the mod-8 ceiling. MAXFRAME > 7 would
        // alias N(S) keys in the `unacked` BTreeMap and break `in_flight()`.
        let maxframe = (self.params.maxframe as usize).clamp(1, 7);
        for chunk in buf.chunks(paclen) {
            // Only enter the (pumping) wait when the window is actually full or the
            // peer is flow-controlling us (RNR). Pumping unconditionally before the
            // first send would consume an ack the peer cannot legitimately have sent
            // yet (it acks frames not yet transmitted), and fix B's out-of-window
            // guard would correctly drop it — losing the ack. Fix E + I: when blocked,
            // `await_window` drives T1 recovery (retransmit + N2 cap → TimedOut), so a
            // lost RR or a stuck-busy peer fails legibly, never hangs.
            if self.remote_busy || self.in_flight() >= maxframe {
                self.await_window(maxframe)?;
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
    /// Wait for the in-flight count to drain to `target_in_flight_drained_to`,
    /// performing T1 recovery: each round we pump acks, and if frames remain, sleep
    /// one T1 then retransmit ALL outstanding I-frames from V(A) forward (mod-8).
    /// Returns `TimedOut` once N2 retransmit rounds are exhausted without progress
    /// (spec §5 — "no answer" must fail legibly, never hang).
    ///
    /// Fix C: the prior body retransmitted only `unacked.iter().next()` (lowest map
    /// key — the wrong frame across a mod-8 wrap) and retransmitted *before* any T1
    /// wait. Now the wait precedes the retransmit, and `retransmit_from(self.va)`
    /// resends the whole outstanding window in N(S) order.
    ///
    /// `write` drives its stall recovery through `await_window` (which also honours
    /// the RNR busy flag). `await_ack` is the narrower "drain the in-flight window"
    /// primitive — retained as the directly-tested T1-recovery routine (the fix-C
    /// regression test + `recovery_tests`) and as the explicit flush/teardown drain
    /// path; hence `allow(dead_code)` for non-test builds where only `write` calls in.
    #[allow(dead_code)]
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
            // T1 expired with frames still unacked: wait one T1, re-pump (an ack may
            // have arrived), then retransmit the whole outstanding window.
            std::thread::sleep(self.params.t1);
            self.pump_acks()?;
            if self.in_flight() <= target_in_flight_drained_to {
                return Ok(());
            }
            self.retransmit_from(self.va)?;
            retries += 1;
        }
    }

    /// Block until the send window has room for a new I-frame: the in-flight count is
    /// below `maxframe` AND the peer is not flow-controlling us (RNR / `remote_busy`).
    /// Bounded by N2 rounds of T1 — a lost RR or a peer stuck in RNR surfaces as
    /// `TimedOut` rather than a silent hang (spec §5; fix E + I). Each T1 round
    /// re-pumps acks (which may clear busy or advance V(A)) and, if there are still
    /// outstanding frames, retransmits the whole window from V(A) (mod-8).
    fn await_window(&mut self, maxframe: usize) -> std::io::Result<()> {
        let mut retries = 0u8;
        loop {
            self.pump_acks()?;
            if !self.remote_busy && self.in_flight() < maxframe {
                return Ok(());
            }
            if retries >= self.params.n2_retries {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "send window stalled — peer busy or no acknowledgement (N2 exceeded)",
                ));
            }
            std::thread::sleep(self.params.t1);
            self.pump_acks()?;
            if !self.remote_busy && self.in_flight() < maxframe {
                return Ok(());
            }
            // Outstanding frames may have been lost; retransmit them. (When the stall
            // is pure RNR with nothing outstanding, this is a no-op and we simply wait
            // out the N2 rounds for the busy condition to lift.)
            if self.in_flight() > 0 {
                self.retransmit_from(self.va)?;
            }
            retries += 1;
        }
    }
}

impl Ax25Stream {
    /// Tear down the link: flush pending acks, send DISC (P=1), await UA (best-effort,
    /// bounded by one T1). Idempotent — a second call after the link is closed is a no-op.
    pub fn disconnect(&mut self) -> std::io::Result<()> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        let _ = self.pump_acks(); // best-effort drain; teardown proceeds regardless
        let disc = Frame {
            path: self.out_path(),
            control: Control::Disc { pf: true },
            info: vec![],
        };
        self.send_frame(&disc)?;
        // Await a teardown response for one T1; a peer that has already vanished must
        // not hang teardown. Fix D/H: accept UA OR DM (with F=1) from the intended
        // peer only (reference `DoDisconnectPendingStateRemote`: UA or DM ⇒ Disconnected).
        let deadline = Instant::now() + self.params.t1;
        while Instant::now() < deadline {
            if let Some(frame) = self.recv_frame()? {
                let from_peer = frame.path.src.call == self.peer.call
                    && frame.path.src.ssid == self.peer.ssid;
                if from_peer && matches!(frame.control, Control::Ua { .. } | Control::Dm { .. }) {
                    return Ok(());
                }
            }
            std::thread::sleep(POLL_INTERVAL);
        }
        Ok(()) // best-effort: DISC sent even if no teardown response came back
    }
}

impl Drop for Ax25Stream {
    fn drop(&mut self) {
        // A dropped stream always tries to release the link; ignore teardown errors.
        let _ = self.disconnect();
    }
}

#[cfg(test)]
mod tcp_integration_tests {
    use super::*;
    use crate::winlink::ax25::link::{connect_link, KissLinkConfig};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn call(c: &str, ssid: u8) -> Address { Address { call: c.into(), ssid } }

    #[test]
    fn connect_over_loopback_tcp_completes_sabm_ua() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);

        // The loopback "modem": read until it has a full KISS frame containing a SABM,
        // then reply a KISS-wrapped UA addressed back to N7CPZ-7. 127.0.0.1 only.
        let mine_s = mine.clone();
        let target_s = target.clone();
        let server = thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut decoder = KissDecoder::new();
            let mut buf = [0u8; 256];
            loop {
                let n = sock.read(&mut buf).unwrap();
                if n == 0 { return; }
                for body in decoder.push(&buf[..n]) {
                    if let Ok(f) = Frame::decode(&body) {
                        if matches!(f.control, Control::Sabm { .. }) {
                            let ua = Frame {
                                path: Path { dest: mine_s.clone(), src: target_s.clone(), digis: vec![] },
                                control: Control::Ua { pf: true },
                                info: vec![],
                            };
                            sock.write_all(&kiss_data_frame(&ua.encode().unwrap())).unwrap();
                            return;
                        }
                    }
                }
            }
        });

        let cfg = KissLinkConfig::Tcp { host: addr.ip().to_string(), port: addr.port() };
        let link = connect_link(&cfg).unwrap();
        // Non-blocking-ish: the loopback server replies promptly; a generous T1 covers scheduling.
        let params = Ax25Params { t1: Duration::from_secs(2), n2_retries: 1, ..Ax25Params::default() };
        let stream = connect(link, mine, target.clone(), &[], &params).unwrap();
        assert_eq!(stream.peer, target);
        server.join().unwrap();
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::test_peer::ScriptedPeer;
    use super::*;

    fn call(c: &str, ssid: u8) -> Address { Address { call: c.into(), ssid } }

    fn wrap(f: &Frame) -> Vec<u8> { kiss_data_frame(&f.encode().unwrap()) }

    #[test]
    fn full_session_direct_path() {
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        let back = |dest: &Address, src: &Address, c: Control, info: Vec<u8>| {
            wrap(&Frame { path: Path { dest: dest.clone(), src: src.clone(), digis: vec![] }, control: c, info })
        };
        // Pre-script the peer's whole side: UA (connect), RR(1) (ack our I-frame),
        // one inbound I-frame, then UA (disconnect).
        peer.feed(&back(&mine, &target, Control::Ua { pf: true }, vec![]));
        peer.feed(&back(&mine, &target, Control::Rr { nr: 1, pf: false }, vec![]));
        peer.feed(&back(&mine, &target, Control::I { ns: 0, nr: 1, pf: false }, b"HI".to_vec()));
        peer.feed(&back(&mine, &target, Control::Ua { pf: true }, vec![]));

        let mut s = connect(Box::new(peer.clone()), mine.clone(), target.clone(), &[], &Ax25Params::default()).unwrap();
        assert_eq!(s.write(b"PING").unwrap(), 4);
        let mut buf = [0u8; 16];
        let n = s.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"HI");
        s.disconnect().unwrap();

        // Our side, decoded: SABM, then an I-frame "PING", an RR for the inbound, a DISC.
        let frames = { let mut d = KissDecoder::new(); d.push(&peer.drain_tx()) };
        let controls: Vec<Control> = frames.iter().map(|b| Frame::decode(b).unwrap().control).collect();
        assert!(controls.iter().any(|c| matches!(c, Control::Sabm { .. })));
        assert!(controls.iter().any(|c| matches!(c, Control::I { ns: 0, .. })));
        assert!(controls.iter().any(|c| matches!(c, Control::Disc { .. })));
    }

    #[test]
    fn connect_via_one_digipeater_carries_the_relay() {
        // Spec §6: the digipeated (≥1-relay) path must be exercised before release.
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        let digi = call("W7RPT", 1);
        // The UA comes back addressed to us (the modem strips the path on the reply
        // surface for our purposes); recv_frame only checks dest == mycall.
        let ua = Frame { path: Path { dest: mine.clone(), src: target.clone(), digis: vec![] }, control: Control::Ua { pf: true }, info: vec![] };
        peer.feed(&wrap(&ua));
        let s = connect(Box::new(peer.clone()), mine.clone(), target.clone(), std::slice::from_ref(&digi), &Ax25Params::default()).unwrap();
        assert_eq!(s.digis, vec![digi.clone()]);
        // Our SABM must carry the digi in its path.
        let frames = { let mut d = KissDecoder::new(); d.push(&peer.drain_tx()) };
        let sabm = frames.iter().map(|b| Frame::decode(b).unwrap()).find(|f| matches!(f.control, Control::Sabm { .. })).unwrap();
        assert_eq!(sabm.path.digis, vec![digi]);
    }
}

#[cfg(test)]
mod disconnect_tests {
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
            params: Ax25Params { t1: Duration::from_millis(20), ..Ax25Params::default() },
            vs: 0, vr: 0, va: 0,
            remote_busy: false,
            pending_frames: std::collections::VecDeque::new(),
            inbound: std::collections::VecDeque::new(),
            unacked: std::collections::BTreeMap::new(),
            closed: false,
        }
    }

    #[test]
    fn disconnect_sends_disc_and_returns_on_ua() {
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        let ua = Frame {
            path: Path { dest: mine.clone(), src: target.clone(), digis: vec![] },
            control: Control::Ua { pf: true },
            info: vec![],
        };
        peer.feed(&kiss_data_frame(&ua.encode().unwrap()));
        let mut s = connected(&peer);
        s.disconnect().unwrap();
        let frames = { let mut d = KissDecoder::new(); d.push(&peer.drain_tx()) };
        assert!(
            frames.iter().any(|b| matches!(Frame::decode(b).unwrap().control, Control::Disc { pf: true })),
            "expected a DISC frame"
        );
    }

    #[test]
    fn disconnect_is_best_effort_when_peer_is_gone() {
        let peer = ScriptedPeer::new(); // never replies UA
        let mut s = connected(&peer);
        // Must not hang — bounded by one (tiny) T1.
        let start = Instant::now();
        s.disconnect().unwrap();
        assert!(start.elapsed() < Duration::from_secs(1), "teardown must be bounded");
    }

    #[test]
    fn disconnect_is_idempotent() {
        let peer = ScriptedPeer::new();
        let mut s = connected(&peer);
        s.disconnect().unwrap();
        let _ = peer.drain_tx();
        s.disconnect().unwrap(); // second call: no-op, sends nothing
        assert!(peer.drain_tx().is_empty(), "a closed link sends no further DISC");
    }

    #[test]
    fn drop_sends_disc() {
        let peer = ScriptedPeer::new();
        {
            let _s = connected(&peer);
            // dropped at end of scope ⇒ Drop calls disconnect
        }
        let frames = { let mut d = KissDecoder::new(); d.push(&peer.drain_tx()) };
        assert!(
            frames.iter().any(|b| matches!(Frame::decode(b).unwrap().control, Control::Disc { .. })),
            "Drop must send DISC"
        );
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
            remote_busy: false,
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
            remote_busy: false,
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
            remote_busy: false,
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
        // V(A) lands at 2, not 3: when the window stalls after frames 0,1, the pump
        // drains both pre-fed RRs in one pass — RR(2) is valid (va 0→2), but RR(3) is
        // out-of-window at that instant (frame 2 isn't sent yet), so fix B's guard
        // correctly drops it. Frame 2 then sends and stays unacked. (Previously this
        // asserted 3, encoding the pre-fix mass-ack behavior that walked past V(S).)
        assert_eq!(s.va, 2, "RR(2) advanced V(A); the premature RR(3) is out-of-window when pumped");
    }
}

#[cfg(test)]
mod hardening_tests {
    //! Regression tests for the P2 correctness defects A–I found by code review +
    //! the cross-provider Codex adversarial round. Each test reproduces a concrete
    //! bug (mod-8 wrap, out-of-window ack, T1 ordering, foreign-station corruption,
    //! window stall without recovery, MAXFRAME aliasing, P/F echo, DISC/DM teardown,
    //! RNR backpressure). Cross-checked vs `TNCKissInterface.dll` (`Connection.cs`).
    use super::test_peer::ScriptedPeer;
    use super::*;

    fn call(c: &str, ssid: u8) -> Address {
        Address { call: c.into(), ssid }
    }

    /// A connected stream with explicit window state, bypassing the handshake.
    fn connected(peer: &ScriptedPeer, params: Ax25Params) -> Ax25Stream {
        Ax25Stream {
            link: Box::new(peer.clone()),
            decoder: KissDecoder::new(),
            mycall: call("N7CPZ", 7),
            peer: call("W7AUX", 10),
            digis: vec![],
            params,
            vs: 0,
            vr: 0,
            va: 0,
            remote_busy: false,
            pending_frames: std::collections::VecDeque::new(),
            inbound: std::collections::VecDeque::new(),
            unacked: std::collections::BTreeMap::new(),
            closed: false,
        }
    }

    /// Build a KISS-wrapped S-frame the peer sends to us (src defaults to the peer).
    fn peer_s(mycall: &Address, src: &Address, control: Control) -> Vec<u8> {
        let f = Frame {
            path: Path { dest: mycall.clone(), src: src.clone(), digis: vec![] },
            control,
            info: vec![],
        };
        kiss_data_frame(&f.encode().unwrap())
    }

    /// Build a KISS-wrapped I-frame the peer sends to us.
    fn peer_i(mycall: &Address, src: &Address, ns: u8, nr: u8, pf: bool, info: &[u8]) -> Vec<u8> {
        let f = Frame {
            path: Path { dest: mycall.clone(), src: src.clone(), digis: vec![] },
            control: Control::I { ns, nr, pf },
            info: info.to_vec(),
        };
        kiss_data_frame(&f.encode().unwrap())
    }

    /// Decode every KISS frame the stream wrote and return the controls.
    fn tx_controls(peer: &ScriptedPeer) -> Vec<Control> {
        let frames = { let mut d = KissDecoder::new(); d.push(&peer.drain_tx()) };
        frames.iter().map(|b| Frame::decode(b).unwrap().control).collect()
    }

    /// Decode every KISS frame the stream wrote and return the I-frame info payloads
    /// in N(S) wire order.
    fn tx_i_infos(peer: &ScriptedPeer) -> Vec<(u8, Vec<u8>)> {
        let frames = { let mut d = KissDecoder::new(); d.push(&peer.drain_tx()) };
        frames
            .iter()
            .filter_map(|b| {
                let f = Frame::decode(b).unwrap();
                match f.control {
                    Control::I { ns, .. } => Some((ns, f.info)),
                    _ => None,
                }
            })
            .collect()
    }

    // ── A: retransmit_from must walk mod-8 through the wrap ────────────────────

    #[test]
    fn rej_retransmits_all_unacked_across_mod8_wrap() {
        // va=6, vs=2, window {6:A,7:B,0:C,1:D}. REJ(6) ⇒ resend A,B,C,D in N(S)
        // order 6,7,0,1. The old `.range(nr..)` body only resent 6,7 (dropped the
        // wrapped 0,1) — the bug this test pins.
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        let mut s = connected(&peer, Ax25Params { paclen: 1, maxframe: 7, ..Ax25Params::default() });
        s.va = 6;
        s.vs = 2;
        s.unacked.insert(6, b"A".to_vec());
        s.unacked.insert(7, b"B".to_vec());
        s.unacked.insert(0, b"C".to_vec());
        s.unacked.insert(1, b"D".to_vec());

        peer.feed(&peer_s(&mine, &target, Control::Rej { nr: 6, pf: false }));
        s.pump_acks().unwrap();

        let resent = tx_i_infos(&peer);
        assert_eq!(
            resent,
            vec![
                (6u8, b"A".to_vec()),
                (7u8, b"B".to_vec()),
                (0u8, b"C".to_vec()),
                (1u8, b"D".to_vec()),
            ],
            "REJ(6) must resend the full wrapped window in 6,7,0,1 order"
        );
    }

    #[test]
    fn rej_acks_then_retransmits_across_wrap() {
        // Same window; REJ(7) ⇒ acks 6 (va→7), resends 7,0,1.
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        let mut s = connected(&peer, Ax25Params { paclen: 1, maxframe: 7, ..Ax25Params::default() });
        s.va = 6;
        s.vs = 2;
        s.unacked.insert(6, b"A".to_vec());
        s.unacked.insert(7, b"B".to_vec());
        s.unacked.insert(0, b"C".to_vec());
        s.unacked.insert(1, b"D".to_vec());

        peer.feed(&peer_s(&mine, &target, Control::Rej { nr: 7, pf: false }));
        s.pump_acks().unwrap();

        assert_eq!(s.va, 7, "REJ(7) acknowledged frame 6");
        assert!(!s.unacked.contains_key(&6), "frame 6 is acked, dropped from window");
        let resent = tx_i_infos(&peer);
        assert_eq!(
            resent,
            vec![(7u8, b"B".to_vec()), (0u8, b"C".to_vec()), (1u8, b"D".to_vec())],
        );
    }

    // ── B: ack_through must ignore out-of-window N(R) ──────────────────────────

    #[test]
    fn ack_through_ignores_out_of_window_nr() {
        // va=4, vs=6, window {4,5}. RR(3) acks 7 frames (4→3 mod-8) which is more
        // than the 2 outstanding ⇒ out of window ⇒ IGNORE. The old walk-from-va
        // loop would have walked 4,5,6,7,0,1,2 removing live keys and set va=3.
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        let mut s = connected(&peer, Ax25Params { paclen: 1, maxframe: 7, ..Ax25Params::default() });
        s.va = 4;
        s.vs = 6;
        s.unacked.insert(4, b"A".to_vec());
        s.unacked.insert(5, b"B".to_vec());

        peer.feed(&peer_s(&mine, &target, Control::Rr { nr: 3, pf: false }));
        s.pump_acks().unwrap();

        assert_eq!(s.va, 4, "out-of-window RR(3) must not advance V(A)");
        assert!(s.unacked.contains_key(&4) && s.unacked.contains_key(&5), "window unchanged");
        assert_eq!(s.unacked.len(), 2);
    }

    #[test]
    fn ack_through_accepts_in_window_nr() {
        // va=4, vs=6; RR(5) acks 1 frame ⇒ valid ⇒ va→5, key 4 removed.
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        let mut s = connected(&peer, Ax25Params { paclen: 1, maxframe: 7, ..Ax25Params::default() });
        s.va = 4;
        s.vs = 6;
        s.unacked.insert(4, b"A".to_vec());
        s.unacked.insert(5, b"B".to_vec());

        peer.feed(&peer_s(&mine, &target, Control::Rr { nr: 5, pf: false }));
        s.pump_acks().unwrap();

        assert_eq!(s.va, 5);
        assert!(!s.unacked.contains_key(&4));
        assert!(s.unacked.contains_key(&5));
    }

    // ── C: await_ack retransmits ALL outstanding from V(A) in mod-8 order ──────

    #[test]
    fn await_ack_retransmits_all_outstanding_across_wrap() {
        // Wrapped window {6:A,7:B,0:C}, va=6, vs=1, no acks, n2=1, tiny T1.
        // await_ack must retransmit A,B,C (all three, in 6,7,0 order) exactly once,
        // then TimedOut. The old code resent only `unacked.iter().next()` (lowest
        // map key = 0 ⇒ "C"), which is the wrong frame across the wrap.
        let peer = ScriptedPeer::new();
        let mut s = connected(
            &peer,
            Ax25Params { paclen: 1, maxframe: 7, t1: Duration::from_millis(10), n2_retries: 1, ..Ax25Params::default() },
        );
        s.va = 6;
        s.vs = 1;
        s.unacked.insert(6, b"A".to_vec());
        s.unacked.insert(7, b"B".to_vec());
        s.unacked.insert(0, b"C".to_vec());

        let err = s.await_ack(0).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
        let resent = tx_i_infos(&peer);
        assert_eq!(
            resent,
            vec![(6u8, b"A".to_vec()), (7u8, b"B".to_vec()), (0u8, b"C".to_vec())],
            "one retransmit round resends the full wrapped window in 6,7,0 order"
        );
    }

    // ── D: connected stream must only process frames from its peer ─────────────

    #[test]
    fn foreign_station_ack_is_ignored() {
        // Connected to W7AUX with unacked {0,1}; a DIFFERENT station W1AAA sends an
        // RR(2) addressed to us. recv_frame's dest==mycall filter would let it in
        // and corrupt the window; pump_acks must reject it on src != peer.
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let foreign = call("W1AAA", 0);
        let mut s = connected(&peer, Ax25Params { paclen: 1, maxframe: 7, ..Ax25Params::default() });
        s.send_i(b"A").unwrap();
        s.send_i(b"B").unwrap();
        let _ = peer.drain_tx();
        assert_eq!(s.unacked.len(), 2);

        peer.feed(&peer_s(&mine, &foreign, Control::Rr { nr: 2, pf: false }));
        s.pump_acks().unwrap();

        assert_eq!(s.va, 0, "foreign RR must not advance V(A)");
        assert_eq!(s.unacked.len(), 2, "foreign RR must not clear the window");
    }

    #[test]
    fn foreign_station_i_frame_is_not_delivered() {
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let foreign = call("W1AAA", 0);
        let mut s = connected(&peer, Ax25Params::default());
        peer.feed(&peer_i(&mine, &foreign, 0, 0, false, b"INTRUDER"));
        let mut buf = [0u8; 16];
        let n = s.read(&mut buf).unwrap();
        assert_eq!(n, 0, "a foreign I-frame must not be delivered to read()");
        assert_eq!(s.vr, 0, "V(R) unchanged by a foreign I-frame");
    }

    // ── E: write() window-stall must drive T1 recovery ─────────────────────────

    #[test]
    fn write_window_stall_retransmits_then_times_out() {
        // paclen 1, maxframe 2, tiny T1, n2=3; RR is lost (nothing fed). write must
        // retransmit the stalled frame(s) and ultimately return TimedOut — the old
        // stall loop only pumped+slept (never retransmitted) and the `as u8` cast
        // could mis-cap N2.
        let peer = ScriptedPeer::new();
        let params = Ax25Params { paclen: 1, maxframe: 2, t1: Duration::from_millis(10), n2_retries: 3, ..Ax25Params::default() };
        let mut s = connected(&peer, params);
        let err = s.write(b"XYZ").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut, "lost RR must surface as TimedOut, not hang");
        // We must have retransmitted (more than the 2 original sends present).
        let resent = tx_i_infos(&peer);
        assert!(resent.len() > 2, "stall must retransmit, got {} I-frames", resent.len());
    }

    #[test]
    fn write_completes_when_rrs_arrive_late() {
        // Positive case: feed RR(2) so the window drains and the 3rd segment sends;
        // write completes Ok. Note: with fix B's out-of-window guard, the pre-fed
        // RR(3) is *correctly* dropped when pumped — at that moment only frames 0,1
        // are in flight, so an N(R)=3 acks more than is outstanding. The write still
        // succeeds (RR(2) freed the window); frame 2 stays unacked until a later RR.
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        peer.feed(&peer_s(&mine, &target, Control::Rr { nr: 2, pf: false }));
        peer.feed(&peer_s(&mine, &target, Control::Rr { nr: 3, pf: false }));
        let params = Ax25Params { paclen: 1, maxframe: 2, t1: Duration::from_millis(20), n2_retries: 3, ..Ax25Params::default() };
        let mut s = connected(&peer, params);
        assert_eq!(s.write(b"XYZ").unwrap(), 3);
        assert_eq!(s.va, 2, "RR(2) advanced V(A); the premature RR(3) is out-of-window when pumped");
        assert_eq!(s.vs, 3, "all three segments were sent");
    }

    // ── F: clamp maxframe to ≤7 so N(S) keys never alias ───────────────────────

    #[test]
    fn maxframe_clamped_to_seven() {
        // params with maxframe=9; writing 8 one-byte segments with no acks must
        // block at 7 in flight (mod-8 ceiling), never alias keys in the BTreeMap.
        let peer = ScriptedPeer::new();
        let params = Ax25Params { paclen: 1, maxframe: 9, t1: Duration::from_millis(5), n2_retries: 0, ..Ax25Params::default() };
        let mut s = connected(&peer, params);
        // 8 bytes, no acks: the 8th cannot fit a mod-8 window ⇒ stall ⇒ TimedOut.
        let err = s.write(b"ABCDEFGH").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
        assert!(s.in_flight() <= 7, "never more than 7 unacked in flight, got {}", s.in_flight());
    }

    // ── G: a polled (P=1) inbound I-frame requires an F=1 reply ────────────────

    #[test]
    fn in_order_i_frame_with_poll_gets_final_reply() {
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        let mut s = connected(&peer, Ax25Params::default());
        peer.feed(&peer_i(&mine, &target, 0, 0, true, b"HI"));
        let mut buf = [0u8; 16];
        let _ = s.read(&mut buf).unwrap();
        let controls = tx_controls(&peer);
        assert!(
            controls.iter().any(|c| matches!(c, Control::Rr { pf: true, .. })),
            "P=1 I-frame must get an RR with F=1, got {controls:?}"
        );
    }

    #[test]
    fn in_order_i_frame_without_poll_gets_non_final_reply() {
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        let mut s = connected(&peer, Ax25Params::default());
        peer.feed(&peer_i(&mine, &target, 0, 0, false, b"HI"));
        let mut buf = [0u8; 16];
        let _ = s.read(&mut buf).unwrap();
        let controls = tx_controls(&peer);
        let rr = controls.iter().find(|c| matches!(c, Control::Rr { .. })).unwrap();
        assert!(matches!(rr, Control::Rr { pf: false, .. }), "P=0 ⇒ RR F=0, got {rr:?}");
    }

    // ── H: inbound DISC + DM teardown ──────────────────────────────────────────

    #[test]
    fn inbound_disc_replies_ua_and_closes() {
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        let mut s = connected(&peer, Ax25Params::default());
        peer.feed(&peer_s(&mine, &target, Control::Disc { pf: true }));
        s.pump_acks().unwrap();
        assert!(s.closed, "an inbound DISC must close the stream");
        let controls = tx_controls(&peer);
        assert!(
            controls.iter().any(|c| matches!(c, Control::Ua { .. })),
            "we must reply UA to an inbound DISC, got {controls:?}"
        );
    }

    #[test]
    fn disconnect_accepts_dm_as_teardown() {
        // Peer replies DM (not UA) to our DISC ⇒ disconnect returns Ok, bounded.
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        let dm = Frame {
            path: Path { dest: mine.clone(), src: target.clone(), digis: vec![] },
            control: Control::Dm { pf: true },
            info: vec![],
        };
        peer.feed(&kiss_data_frame(&dm.encode().unwrap()));
        let mut s = connected(&peer, Ax25Params { t1: Duration::from_millis(50), ..Ax25Params::default() });
        let start = Instant::now();
        s.disconnect().unwrap();
        assert!(start.elapsed() < Duration::from_secs(1), "DM teardown must be bounded");
    }

    // ── I: RNR remote-busy backpressure ─────────────────────────────────────────

    #[test]
    fn rnr_sets_remote_busy_on_pump() {
        // An inbound RNR(0) marks the peer busy; a subsequent RR(0) clears it. This is
        // the receive-path half of fix I (the flag `write` then consults).
        let peer = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        let mut s = connected(&peer, Ax25Params::default());
        peer.feed(&peer_s(&mine, &target, Control::Rnr { nr: 0, pf: false }));
        s.pump_acks().unwrap();
        assert!(s.remote_busy, "RNR must set remote_busy");
        peer.feed(&peer_s(&mine, &target, Control::Rr { nr: 0, pf: false }));
        s.pump_acks().unwrap();
        assert!(!s.remote_busy, "RR must clear remote_busy");
    }

    #[test]
    fn rnr_blocks_writes_until_rr_clears_busy() {
        // `remote_busy` is persistent stream state (set by an earlier pump on an
        // inbound RNR). While busy, write must send NO new I-frame and — since no RR
        // arrives to lift it — eventually TimedOut (a stuck-busy peer fails legibly).
        let peer = ScriptedPeer::new();
        let params = Ax25Params { paclen: 1, maxframe: 4, t1: Duration::from_millis(10), n2_retries: 1, ..Ax25Params::default() };
        let mut s = connected(&peer, params);
        s.remote_busy = true; // carried over from a prior RNR pump
        let err = s.write(b"Z").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut, "a stuck-busy peer must fail legibly");
        let infos = tx_i_infos(&peer);
        assert!(infos.is_empty(), "no new I-frame may be sent while remote is busy, got {infos:?}");

        // Positive case: an RR is pending when write runs ⇒ the wait's pump clears
        // busy and the write proceeds, sending exactly one I-frame.
        let peer2 = ScriptedPeer::new();
        let mine = call("N7CPZ", 7);
        let target = call("W7AUX", 10);
        peer2.feed(&peer_s(&mine, &target, Control::Rr { nr: 0, pf: false }));
        let params2 = Ax25Params { paclen: 1, maxframe: 4, t1: Duration::from_millis(10), n2_retries: 5, ..Ax25Params::default() };
        let mut s2 = connected(&peer2, params2);
        s2.remote_busy = true;
        let _ = s2.write(b"Q").unwrap();
        assert!(!s2.remote_busy, "the pending RR cleared remote_busy");
        let infos2 = tx_i_infos(&peer2);
        assert_eq!(infos2.len(), 1, "one I-frame sent after busy cleared");
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
