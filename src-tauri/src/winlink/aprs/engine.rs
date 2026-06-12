//! Synchronous, testable core of the APRS tactical-chat engine.
//!
//! `handle_inbound_bytes` does **promiscuous** decode — there is NO destination
//! filter on received frames, because that is the whole point of APRS RX: every
//! station on the channel hears every UI frame. Addressed message-type packets
//! are routed to the UI and auto-ACKed; everything else (other stations' chatter,
//! position beacons, telemetry) is decoded and dropped.
//!
//! **The dedupe split is load-bearing (APRS-protocol correctness):**
//!   - A long `dedupe` window (300 s) gates UI DISPLAY: a retransmitted or
//!     digipeated copy of a message we already showed must NOT re-appear in the
//!     conversation.
//!   - A separate short `ack_throttle` window (5 s) gates the auto-ACK: the
//!     sender's retransmits (APRS spacing is ≥30 s apart) must EACH be re-ACKed so
//!     a lost ACK is recovered, while a digipeated burst (multiple copies within
//!     a second or two) collapses to a single ACK.
//!
//! A received REJ terminates our outgoing retransmit loop immediately rather than
//! riding the full timeout, so the operator learns of an explicit rejection at
//! once.
//!
//! The async driver (Task 10) wraps this core; all timing is injected via
//! `now_ms` so the engine is fully deterministic under test.

use crate::winlink::ax25::frame::Frame;
use crate::winlink::ax25::kiss::{kiss_data_frame, KissDecoder};

use super::dedupe::{DedupeCache, DedupeKey};
use super::framebuild::{build_ui_frame, extract_inbound, fmt_callsign};
use super::identity::AprsIdentity;
use super::message::{encode_ack, encode_message, parse_info, AprsPayload};
use super::tx::TxQueue;

/// A decoded, addressed-to-us inbound text message destined for the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundMsg {
    pub sender: String,
    pub text: String,
    pub msgid: Option<String>,
}

/// Delivery lifecycle of one of OUR outgoing messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryState {
    Sent,
    Acked,
    TimedOut,
    Rejected,
}

impl DeliveryState {
    /// Terminal states release an in-flight slot (see Task 10's TauriEventSink).
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            DeliveryState::Acked | DeliveryState::TimedOut | DeliveryState::Rejected
        )
    }
}

/// A delivery-state transition for one outgoing message, keyed by its msgid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateChange {
    pub msgid: String,
    pub state: DeliveryState,
}

/// Side-effect sink the engine emits into. The async driver implements this with
/// a Tauri event emitter; tests implement it with a recording sink.
pub trait EventSink: Send {
    fn emit_message(&self, ev: InboundMsg);
    fn emit_state(&self, ev: StateChange);
    fn emit_listening(&self, on: bool);
}

/// Display dedupe window (ms): suppress re-showing ANY retransmitted/digipeated copy.
const DEDUPE_WINDOW_MS: u64 = 300_000;
/// Auto-ACK throttle window (ms): re-ACK every received copy EXCEPT collapse a burst.
const ACK_THROTTLE_MS: u64 = 5_000;

pub struct AprsEngine {
    identity: AprsIdentity,
    sink: Box<dyn EventSink>,
    decoder: KissDecoder,
    dedupe: DedupeCache,
    ack_throttle: DedupeCache,
    tx: TxQueue,
}

impl AprsEngine {
    pub fn new(identity: AprsIdentity, sink: Box<dyn EventSink>) -> Self {
        Self {
            identity,
            sink,
            decoder: KissDecoder::new(),
            dedupe: DedupeCache::new(DEDUPE_WINDOW_MS),
            ack_throttle: DedupeCache::new(ACK_THROTTLE_MS),
            tx: TxQueue::new(),
        }
    }

    /// Feed raw bytes read from the link. Returns KISS-ready frames to write back (auto-acks).
    /// Auto-ACKs are intentionally OUTSIDE the abort/TxQueue path: each is a single fire-once
    /// short frame with no retransmit timer, rate-limited by `ack_throttle` — RADIO-1-safe.
    pub fn handle_inbound_bytes(&mut self, bytes: &[u8], now_ms: u64) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        for body in self.decoder.push(bytes) {
            let frame = match Frame::decode(&body) {
                Ok(f) => f,
                Err(_) => continue,
            };
            let (sender, info) = match extract_inbound(&frame) {
                Some(x) => x,
                None => continue,
            };
            let payload = match parse_info(&info) {
                Some(p) => p,
                None => continue,
            };
            match payload {
                AprsPayload::Message {
                    addressee,
                    text,
                    msgid,
                } => {
                    if !self.addressed_to_us(&addressee) {
                        continue;
                    }
                    let dkey = DedupeKey {
                        src: sender.clone(),
                        kind: "msg".into(),
                        id: msgid.clone().unwrap_or_else(|| text_hash(&text)),
                    };
                    if !self.dedupe.seen(dkey, now_ms) {
                        self.sink.emit_message(InboundMsg {
                            sender: sender.clone(),
                            text,
                            msgid: msgid.clone(),
                        });
                    }
                    if let Some(id) = msgid {
                        let akey = DedupeKey {
                            src: sender.clone(),
                            kind: "ackout".into(),
                            id: id.clone(),
                        };
                        if !self.ack_throttle.seen(akey, now_ms) {
                            let ack = encode_ack(&sender, &id);
                            let frame = build_ui_frame(&self.identity, &ack);
                            if let Ok(b) = frame.encode() {
                                out.push(kiss_data_frame(&b));
                            }
                        }
                    }
                }
                AprsPayload::Ack { addressee, msgid } => {
                    if !self.addressed_to_us(&addressee) {
                        continue;
                    }
                    let key = DedupeKey {
                        src: sender,
                        kind: "ack".into(),
                        id: msgid.clone(),
                    };
                    if self.dedupe.seen(key, now_ms) {
                        continue;
                    }
                    if self.tx.on_ack(&msgid) {
                        self.sink.emit_state(StateChange {
                            msgid,
                            state: DeliveryState::Acked,
                        });
                    }
                }
                AprsPayload::Rej { addressee, msgid } => {
                    if !self.addressed_to_us(&addressee) {
                        continue;
                    }
                    let key = DedupeKey {
                        src: sender,
                        kind: "rej".into(),
                        id: msgid.clone(),
                    };
                    if self.dedupe.seen(key, now_ms) {
                        continue;
                    }
                    if self.tx.on_ack(&msgid) {
                        self.sink.emit_state(StateChange {
                            msgid,
                            state: DeliveryState::Rejected,
                        });
                    }
                }
            }
        }
        out
    }

    /// Queue an outgoing message with an ALREADY-MINTED msgid (minting lives upstream in
    /// AprsState::send). Emits `Sent` = "queued" (NOT keyed; the frame is written by `tick`).
    pub fn enqueue_send(&mut self, dest_call: &str, text: &str, msgid: &str, now_ms: u64) {
        let info = encode_message(dest_call, text, Some(msgid));
        let frame = build_ui_frame(&self.identity, &info);
        let bytes = match frame.encode() {
            Ok(b) => kiss_data_frame(&b),
            Err(_) => return,
        };
        if self.tx.enqueue(msgid.to_string(), bytes, now_ms).is_ok() {
            self.sink.emit_state(StateChange {
                msgid: msgid.to_string(),
                state: DeliveryState::Sent,
            });
        }
    }

    /// Drive the retransmit clock; returns KISS-ready frames to write now. Emits TimedOut.
    pub fn tick(&mut self, now_ms: u64) -> Vec<Vec<u8>> {
        let due: Vec<Vec<u8>> = self.tx.tick(now_ms).into_iter().map(|d| d.bytes).collect();
        for msgid in self.tx.take_timed_out() {
            self.sink.emit_state(StateChange {
                msgid,
                state: DeliveryState::TimedOut,
            });
        }
        due
    }

    pub fn abort(&mut self) {
        for msgid in self.tx.abort() {
            self.sink.emit_state(StateChange {
                msgid,
                state: DeliveryState::TimedOut,
            });
        }
    }

    /// Emit a listening-state change (called by the Task 10 driver at start/exit).
    pub fn set_listening(&self, on: bool) {
        self.sink.emit_listening(on);
    }

    fn addressed_to_us(&self, addressee: &str) -> bool {
        addressee == fmt_callsign(&self.identity.source)
    }
}

fn text_hash(text: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    format!("h{:x}", h.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::winlink::ax25::frame::{Address, Control, Frame, Path};
    use crate::winlink::ax25::kiss::kiss_data_frame;
    use std::sync::{Arc, Mutex};

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

    fn identity() -> super::super::identity::AprsIdentity {
        super::super::identity::AprsIdentity {
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

    fn inbound_message_bytes() -> Vec<u8> {
        let f = Frame {
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
        };
        kiss_data_frame(&f.encode().unwrap())
    }

    #[test]
    fn inbound_message_is_routed_and_auto_acked() {
        let sink = RecSink::default();
        let mut engine = AprsEngine::new(identity(), Box::new(sink.clone()));
        let tx = engine.handle_inbound_bytes(&inbound_message_bytes(), 1000);
        let msgs = sink.msgs.lock().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].sender, "KK6XYZ");
        assert_eq!(msgs[0].text, "ping");
        assert_eq!(tx.len(), 1);
        let decoded = Frame::decode(&strip_kiss(&tx[0])).unwrap();
        assert_eq!(decoded.path.dest.call, "APZTUX");
        assert_eq!(decoded.info, b":KK6XYZ   :ack04");
    }

    #[test]
    fn duplicate_inbound_suppresses_display_but_re_acks_for_lost_ack_recovery() {
        let sink = RecSink::default();
        let mut engine = AprsEngine::new(identity(), Box::new(sink.clone()));
        let tx1 = engine.handle_inbound_bytes(&inbound_message_bytes(), 1_000);
        assert_eq!(sink.msgs.lock().unwrap().len(), 1);
        assert_eq!(tx1.len(), 1);
        let tx2 = engine.handle_inbound_bytes(&inbound_message_bytes(), 3_000);
        assert_eq!(sink.msgs.lock().unwrap().len(), 1);
        assert_eq!(tx2.len(), 0);
        let tx3 = engine.handle_inbound_bytes(&inbound_message_bytes(), 35_000);
        assert_eq!(sink.msgs.lock().unwrap().len(), 1);
        assert_eq!(tx3.len(), 1);
    }

    #[test]
    fn inbound_rej_stops_retransmit_and_reports_rejected() {
        let sink = RecSink::default();
        let mut engine = AprsEngine::new(identity(), Box::new(sink.clone()));
        engine.enqueue_send("KK6XYZ", "hello", "07", 0);
        let rej = Frame {
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
            info: b":N0CALL   :rej07".to_vec(),
        };
        engine.handle_inbound_bytes(&kiss_data_frame(&rej.encode().unwrap()), 1000);
        assert!(sink
            .states
            .lock()
            .unwrap()
            .iter()
            .any(|s| s.msgid == "07" && s.state == DeliveryState::Rejected));
        assert!(engine.tick(30_000).is_empty());
    }

    #[test]
    fn inbound_ack_transitions_outgoing_to_acked() {
        let sink = RecSink::default();
        let mut engine = AprsEngine::new(identity(), Box::new(sink.clone()));
        engine.enqueue_send("KK6XYZ", "hello", "07", 0);
        let ack = Frame {
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
            info: b":N0CALL   :ack07".to_vec(),
        };
        engine.handle_inbound_bytes(&kiss_data_frame(&ack.encode().unwrap()), 1000);
        assert!(sink
            .states
            .lock()
            .unwrap()
            .iter()
            .any(|s| s.msgid == "07" && s.state == DeliveryState::Acked));
    }

    fn strip_kiss(b: &[u8]) -> Vec<u8> {
        let mut d = crate::winlink::ax25::kiss::KissDecoder::new();
        d.push(b).into_iter().next().unwrap()
    }
}
