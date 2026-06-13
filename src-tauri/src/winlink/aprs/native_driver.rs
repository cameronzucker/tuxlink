//! Native APRS driver — bridges the UV-Pro native data path to the APRS engine.
//!
//! The KISS analogue is `engine.rs`'s `run()`: a blocking loop that drains the
//! link, feeds the engine, and writes the engine's outbound frames. Here the
//! "link" is the native Benshi data path — completed inbound AX.25 frames arrive
//! on the `Receiver<Vec<u8>>` that `UvproSession::take_aprs_receiver()` hands
//! over (the session's event loop reassembles `DATA_RXD` fragments into them),
//! and outbound frames go out as `HT_SEND_DATA` fragments via
//! `UvproSession::send_aprs_frame()`. Unlike the KISS path there is no second
//! socket: control + chat share the one connection the session owns.
//!
//! RADIO-1 / ADR 0018: outbound frames are the engine's auto-ACKs (fire-once,
//! throttled) and bounded retransmits; abort drops the session link upstream and
//! flushes pending retransmits to terminal so in-flight slots are released. The
//! driver never opens hardware itself — it is handed an already-connected sink.

// Live caller: `AprsState::start_native` (engine.rs) spawns `run_native` against a
// connected `UvproSession`, which is also the live caller for the `AprsFrameTx` impl
// and the session.rs TX-path methods (`take_aprs_receiver` / `send_aprs_frame`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::engine::{AprsEngine, TxCommand};

/// Sink for raw AX.25 frames the engine emits (auto-ACKs + due retransmits),
/// transmitted over the native Benshi link. Abstracted so the driver loop is
/// unit-testable with a recording fake and runs against `UvproSession` in
/// production. A send error is non-fatal to the loop: the engine's retransmit
/// timer covers a transient miss.
pub trait AprsFrameTx: Send {
    fn send_frame(&self, ax25: &[u8]) -> Result<(), String>;
}

impl AprsFrameTx for Arc<crate::winlink::ax25::uvpro::session::UvproSession> {
    fn send_frame(&self, ax25: &[u8]) -> Result<(), String> {
        self.send_aprs_frame(ax25).map_err(|e| e.to_string())
    }
}

/// Poll cadence — matches the KISS driver's 50 ms loop sleep.
const POLL_MS: u64 = 50;

/// The synchronous, sleep-free core: bridges inbound frames into the engine and
/// routes the engine's outbound frames to the native tx. Separated from the loop +
/// cadence so each step is unit-testable without spinning a thread.
struct NativeDriver {
    engine: AprsEngine,
    tx: Box<dyn AprsFrameTx>,
}

impl NativeDriver {
    fn new(engine: AprsEngine, tx: Box<dyn AprsFrameTx>) -> Self {
        Self { engine, tx }
    }

    /// Feed one completed inbound AX.25 frame: route to the UI and send any
    /// auto-ACK back over the native tx as RAW AX.25 (the session fragments it).
    fn ingest_inbound(&mut self, frame: &[u8], now_ms: u64) {
        for ack in self.engine.handle_inbound_frame(frame, now_ms) {
            let _ = self.tx.send_frame(&ack);
        }
    }

    /// Apply a queued command (the native analogue of the KISS `run()` command arm).
    fn apply_command(&mut self, cmd: TxCommand, now_ms: u64) {
        match cmd {
            TxCommand::Send { dest, text, msgid } => {
                self.engine.enqueue_send(&dest, &text, &msgid, now_ms)
            }
            TxCommand::Broadcast { text, local_id } => {
                self.engine.enqueue_broadcast(&text, &local_id, now_ms)
            }
            TxCommand::Abort => self.engine.abort(),
        }
    }

    /// Drain due retransmits (raw AX.25) to the native tx.
    fn drain_due(&mut self, now_ms: u64) {
        for frame in self.engine.tick_frames(now_ms) {
            let _ = self.tx.send_frame(&frame);
        }
    }
}

/// The native driver loop. Runs on `spawn_blocking` (like the KISS `run`): drains
/// inbound frames, applies commands, drives the retransmit clock; exits on abort
/// or when the inbound channel closes (the session disconnected). On teardown it
/// flushes pending retransmits to terminal so their in-flight capacity slots are
/// released (matching the KISS `run`'s `engine.abort()` on exit).
pub fn run_native(
    aprs_rx: Receiver<Vec<u8>>,
    tx: Box<dyn AprsFrameTx>,
    engine: AprsEngine,
    cmd_rx: Receiver<TxCommand>,
    listening: Arc<AtomicBool>,
    abort: Arc<AtomicBool>,
) {
    let started = Instant::now();
    let now_ms = || started.elapsed().as_millis() as u64;
    let mut driver = NativeDriver::new(engine, tx);
    listening.store(true, Ordering::SeqCst);
    driver.engine.set_listening(true);
    loop {
        if abort.load(Ordering::SeqCst) {
            break;
        }
        // Inbound completed AX.25 frames. A closed channel (session gone) ends the loop.
        loop {
            match aprs_rx.try_recv() {
                Ok(frame) => driver.ingest_inbound(&frame, now_ms()),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    abort.store(true, Ordering::SeqCst);
                    break;
                }
            }
        }
        if abort.load(Ordering::SeqCst) {
            break;
        }
        while let Ok(cmd) = cmd_rx.try_recv() {
            driver.apply_command(cmd, now_ms());
        }
        driver.drain_due(now_ms());
        std::thread::sleep(Duration::from_millis(POLL_MS));
    }
    // Teardown: flush pending retransmits to terminal (release in-flight slots).
    driver.engine.abort();
    listening.store(false, Ordering::SeqCst);
    driver.engine.set_listening(false);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::winlink::aprs::engine::{EventSink, InboundMsg, StateChange};
    use crate::winlink::aprs::identity::AprsIdentity;
    use crate::winlink::ax25::frame::{Address, Control, Frame, Path};
    use std::sync::Mutex;

    #[derive(Default, Clone)]
    struct RecSink {
        msgs: Arc<Mutex<Vec<InboundMsg>>>,
        states: Arc<Mutex<Vec<StateChange>>>,
    }
    impl EventSink for RecSink {
        fn emit_message(&self, ev: InboundMsg) {
            self.msgs.lock().unwrap().push(ev);
        }
        fn emit_state(&self, ev: StateChange) {
            self.states.lock().unwrap().push(ev);
        }
        fn emit_listening(&self, _on: bool) {}
    }

    /// Recording frame tx — the in-memory stand-in for `UvproSession::send_aprs_frame`.
    #[derive(Default, Clone)]
    struct RecTx {
        sent: Arc<Mutex<Vec<Vec<u8>>>>,
    }
    impl AprsFrameTx for RecTx {
        fn send_frame(&self, ax25: &[u8]) -> Result<(), String> {
            self.sent.lock().unwrap().push(ax25.to_vec());
            Ok(())
        }
    }

    fn identity() -> AprsIdentity {
        AprsIdentity {
            source: Address {
                call: "N0CALL".into(),
                ssid: 0,
            },
            tocall: Address {
                call: "APZTUX".into(),
                ssid: 0,
            },
            path: vec![],
        }
    }

    /// A raw (non-KISS) inbound APRS message frame addressed to N0CALL from KK6XYZ.
    fn raw_inbound_message() -> Vec<u8> {
        Frame {
            path: Path {
                dest: Address {
                    call: "APZTUX".into(),
                    ssid: 0,
                },
                src: Address {
                    call: "KK6XYZ".into(),
                    ssid: 0,
                },
                digis: vec![],
            },
            control: Control::Ui { pf: false },
            info: b":N0CALL   :ping{04".to_vec(),
        }
        .encode()
        .unwrap()
    }

    #[test]
    fn inbound_frame_fires_chat_event_and_sends_raw_auto_ack() {
        let sink = RecSink::default();
        let engine = AprsEngine::new(identity(), Box::new(sink.clone()));
        let tx = RecTx::default();
        let mut nd = NativeDriver::new(engine, Box::new(tx.clone()));
        nd.ingest_inbound(&raw_inbound_message(), 1_000);
        // (a) the chat event fired
        let msgs = sink.msgs.lock().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].sender, "KK6XYZ");
        assert_eq!(msgs[0].text, "ping");
        // (b) the auto-ACK was sent via the frame tx, as RAW AX.25 (decodes directly)
        let sent = tx.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        let decoded = Frame::decode(&sent[0]).unwrap();
        assert_eq!(decoded.path.dest.call, "APZTUX");
        assert_eq!(decoded.info, b":KK6XYZ   :ack04");
    }

    #[test]
    fn queued_send_is_transmitted_raw_via_frame_tx() {
        let sink = RecSink::default();
        let engine = AprsEngine::new(identity(), Box::new(sink.clone()));
        let tx = RecTx::default();
        let mut nd = NativeDriver::new(engine, Box::new(tx.clone()));
        nd.apply_command(
            TxCommand::Send {
                dest: "KK6XYZ".into(),
                text: "hi".into(),
                msgid: "07".into(),
            },
            0,
        );
        nd.drain_due(0);
        let sent = tx.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        let decoded = Frame::decode(&sent[0]).unwrap();
        assert_eq!(decoded.info, b":KK6XYZ   :hi{07");
    }

    #[test]
    fn abort_command_flushes_pending_to_terminal() {
        let sink = RecSink::default();
        let engine = AprsEngine::new(identity(), Box::new(sink.clone()));
        let mut nd = NativeDriver::new(engine, Box::new(RecTx::default()));
        nd.apply_command(
            TxCommand::Send {
                dest: "KK6XYZ".into(),
                text: "a".into(),
                msgid: "01".into(),
            },
            0,
        );
        nd.apply_command(TxCommand::Abort, 0);
        // The abort emits a terminal state so the in-flight slot is released.
        let states = sink.states.lock().unwrap();
        assert!(states.iter().any(|s| s.msgid == "01"));
    }
}
