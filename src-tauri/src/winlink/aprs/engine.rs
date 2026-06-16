//! Synchronous, testable core of the APRS tactical-chat engine.
//!
//! `handle_inbound_bytes` does **promiscuous** decode — there is NO destination
//! filter on received frames, because that is the whole point of APRS RX: every
//! station on the channel hears every UI frame. EVERY decoded text message is
//! routed to the UI — the channel is a party line, so the feed shows all traffic
//! (directed-to-anyone plus blank-addressee broadcasts), each carrying its
//! `addressee`. Non-message traffic (position beacons, telemetry, status,
//! objects) is ALSO surfaced — as a raw feed row carrying its verbatim info
//! field (tuxlink-8tz1: an operator diagnostic to confirm native RX sees the
//! whole channel; positions additionally emit to the map). A UI-side frame-type
//! filter is the eventual home for "what to show" (tuxlink-l0z5 follow-up); the
//! backend no longer DROPS heard frames. Auto-ACK still fires ONLY for a message
//! addressed to our exact call (never a broadcast or another station's traffic —
//! that would be SPAM).
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

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

use tauri::Emitter;

use crate::winlink::ax25::frame::Frame;
use crate::winlink::ax25::kiss::{kiss_data_frame, KissDecoder};

use super::dedupe::{DedupeCache, DedupeKey};
use super::framebuild::{build_ui_frame, extract_inbound, fmt_callsign, to_tnc2};
use super::identity::AprsIdentity;
use super::message::{encode_ack, encode_message, parse_info, AprsPayload};
use super::position::{parse_mice, parse_object_or_item, parse_position};
use super::tx::TxQueue;

/// Dev-only raw-frame capture (tuxlink-iehg). When the env var
/// `TUXLINK_APRS_RAW_CAPTURE` is set to a writable file path, append each
/// received frame's literal TNC2 string to that file AND echo it to stderr
/// (visible in the `tauri dev` console). Off by default — zero production noise.
/// RX-side only: this path observes, it never transmits. The closure defers
/// formatting so the disabled (no env var) path costs only one env lookup.
fn raw_capture(line: impl FnOnce() -> String) {
    let Ok(path) = std::env::var("TUXLINK_APRS_RAW_CAPTURE") else {
        return;
    };
    let line = line();
    eprintln!("APRS-RX-RAW {line}");
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{line}");
    }
}

/// Compact space-separated hex for an undecodable frame's bytes.
fn hex_bytes(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ")
}

/// A decoded inbound text message heard on the channel, destined for the UI.
///
/// `addressee` is the message's 9-char addressee field, trimmed — an empty
/// string is a no-recipient **broadcast** heard by all (ground-truthed on air
/// 2026-06-13: the BTECH/UV-Pro app sends broadcasts as `:` + 9 spaces). The
/// engine emits EVERY heard message because APRS is a party line; the UI renders
/// `sender → addressee` (or `→ all` when blank). Auto-ACK is a separate concern
/// and fires only for our own exact call (see `ingest_ax25`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboundMsg {
    pub sender: String,
    pub addressee: String,
    pub text: String,
    pub msgid: Option<String>,
}

/// Delivery lifecycle of one of OUR outgoing messages.
///
/// Wire forms (camelCase) are exactly `"sent"`, `"acked"`, `"timedOut"`,
/// `"rejected"` — the UI matches on these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateChange {
    pub msgid: String,
    pub state: DeliveryState,
}

/// A position report decoded from a frame HEARD on the channel (RX-only).
///
/// Mirrors [`InboundMsg`]'s shape: `sender` is the transmitting station's
/// callsign (`CALL-SSID`), and lat/lon/symbol/comment come straight off the
/// wire — RF-honesty: only what was actually decoded, no estimated location.
/// Serializes camelCase so the UI's `aprs-position:new` payload reads
/// `symbolTable` / `symbolCode`. Emitted whenever a heard frame carries a
/// well-formed (uncompressed / compressed / Mic-E) position report — separate
/// from, and in addition to, any message decode of the same frame.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboundPos {
    pub sender: String,
    /// For an OBJECT (`;`) / ITEM (`)`) report, the named entity this position
    /// describes — the map labels the pin by this, not by the reporting
    /// `sender`. `None` for a station's own beacon (the pin is labeled by
    /// `sender`). Skipped on the wire when absent so a beacon payload is
    /// unchanged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub lat: f64,
    pub lon: f64,
    pub symbol_table: char,
    pub symbol_code: char,
    pub comment: String,
    /// APRS position-ambiguity level (0–4) decoded off the wire. `0` is a
    /// full-precision fix; higher means the sender masked low-order minute
    /// digits, so the UI must plot a region, not a false-exact pin (RF-honesty).
    pub ambiguity: u8,
}

/// Side-effect sink the engine emits into. The async driver implements this with
/// a Tauri event emitter; tests implement it with a recording sink.
pub trait EventSink: Send {
    fn emit_message(&self, ev: InboundMsg);
    fn emit_state(&self, ev: StateChange);
    fn emit_listening(&self, on: bool);
    /// Emit a position report decoded from a heard frame (`aprs-position:new`).
    fn emit_position(&self, ev: InboundPos);
}

/// Display dedupe window (ms): suppress re-showing ANY retransmitted/digipeated copy.
const DEDUPE_WINDOW_MS: u64 = 300_000;
/// Auto-ACK throttle window (ms): re-ACK every received copy EXCEPT collapse a burst.
const ACK_THROTTLE_MS: u64 = 5_000;
/// Position dedupe window (ms): suppress re-emitting an IDENTICAL position
/// (same station, same coordinates+symbol) within the window. A station that
/// MOVES (new coordinates) re-emits immediately — the dedupe key includes the
/// position, so latest-position-wins is preserved while a re-beacon of the same
/// fix (or a digipeated copy) is collapsed. Matches the display-dedupe window.
const POS_DEDUPE_WINDOW_MS: u64 = 300_000;

pub struct AprsEngine {
    identity: AprsIdentity,
    sink: Box<dyn EventSink>,
    decoder: KissDecoder,
    dedupe: DedupeCache,
    ack_throttle: DedupeCache,
    pos_dedupe: DedupeCache,
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
            pos_dedupe: DedupeCache::new(POS_DEDUPE_WINDOW_MS),
            tx: TxQueue::new(),
        }
    }

    /// Feed raw bytes read from a KISS link. Returns KISS-ready frames to write back
    /// (auto-acks). Auto-ACKs are intentionally OUTSIDE the abort/TxQueue path: each is a
    /// single fire-once short frame with no retransmit timer, rate-limited by `ack_throttle`
    /// — RADIO-1-safe. KISS-deframes, then KISS-wraps each raw auto-ACK `ingest_ax25` returns.
    pub fn handle_inbound_bytes(&mut self, bytes: &[u8], now_ms: u64) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        for body in self.decoder.push(bytes) {
            for raw in self.ingest_ax25(&body, now_ms) {
                out.push(kiss_data_frame(&raw));
            }
        }
        out
    }

    /// Feed ONE raw AX.25 frame already deframed by a non-KISS transport (the native
    /// Benshi reassembler). Returns auto-ACK responses as RAW AX.25 frames (no KISS wrap)
    /// for the native driver to fragment via `send_aprs_frame`. Same promiscuous decode +
    /// throttled auto-ACK as the KISS path; only the framing differs. Live caller:
    /// the native driver (`NativeDriver::ingest_inbound`), spawned by `start_native`.
    pub fn handle_inbound_frame(&mut self, ax25: &[u8], now_ms: u64) -> Vec<Vec<u8>> {
        self.ingest_ax25(ax25, now_ms)
    }

    /// Promiscuous decode + throttled auto-ACK for one already-deframed AX.25 frame.
    /// Returns auto-ACK responses as RAW AX.25 frame bytes; the caller wraps for its
    /// transport (`handle_inbound_bytes` KISS-wraps, `handle_inbound_frame` returns raw).
    /// Early returns of the (empty-so-far) `out` mirror the per-frame `continue` the KISS
    /// loop used: a non-addressed / undecodable frame yields no auto-ACK.
    fn ingest_ax25(&mut self, body: &[u8], now_ms: u64) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let frame = match Frame::decode(body) {
            Ok(f) => {
                // tuxlink-iehg: capture the literal wire form of EVERY decoded
                // frame BEFORE any addressed-to-us / payload filtering, so the
                // on-air format of e.g. a no-recipient packet (blank addressee)
                // is observable for ground-truthing. No-op unless opted in.
                raw_capture(|| to_tnc2(&f));
                f
            }
            Err(_) => {
                // Undecodable frames are still interesting during capture — record
                // their length + hex rather than discarding silently.
                raw_capture(|| format!("[undecodable {} bytes] {}", body.len(), hex_bytes(body)));
                return out;
            }
        };
        let (sender, info) = match extract_inbound(&frame) {
            Some(x) => x,
            None => return out,
        };
        // Position decode runs on EVERY heard UI frame, independent of the
        // message decode below: a position beacon is NOT a message-type payload
        // (`parse_info` returns None for it), so this must precede the message
        // early-return. The AX.25 destination callsign (base call, SSID
        // stripped) carries Mic-E latitude, so it is passed to `parse_mice`.
        self.try_emit_position(&sender, &frame.path.dest.call, &info, now_ms);

        let payload = match parse_info(&info) {
            Some(p) => p,
            None => {
                // tuxlink-8tz1 (operator-directed diagnostic slice): the feed used
                // to DROP every non-message frame here — positions, status,
                // telemetry, objects, weather, bulletins. On a live channel
                // dominated by position beacons that made the feed look nearly
                // empty even when native RX was healthy ("some, far fewer than I
                // hear"). To verify native RX end-to-end (and expose any upstream
                // GAIA-reassembly loss), surface EVERY heard frame's raw info as a
                // feed row instead of returning early. Positions ALSO still go to
                // the map via try_emit_position above. Eventual design is a UI-side
                // frame-type filter (tuxlink-l0z5 follow-up), NOT a backend drop —
                // the operator wants all traffic visible while validating the path.
                let raw_text = String::from_utf8_lossy(&info).trim_end().to_string();
                if !raw_text.is_empty() {
                    let dkey = DedupeKey {
                        src: sender.clone(),
                        kind: "raw".into(),
                        id: text_hash(&raw_text),
                    };
                    if !self.dedupe.seen(dkey, now_ms) {
                        self.sink.emit_message(InboundMsg {
                            sender,
                            // No APRS addressee on a non-message frame → render as a
                            // broadcast row (the UI maps "" → "→ all").
                            addressee: String::new(),
                            text: raw_text,
                            msgid: None,
                        });
                    }
                }
                return out;
            }
        };
        match payload {
            AprsPayload::Message {
                addressee,
                text,
                msgid,
            } => {
                // APRS is a party line: EMIT every heard message (deduped),
                // regardless of addressee, so the channel feed shows all traffic
                // — directed-to-anyone plus blank-addressee broadcasts. The
                // addressed-to-us check now gates ONLY the auto-ACK below, not
                // display (it used to drop everything not for us).
                let for_us = self.addressed_to_us(&addressee);
                let dkey = DedupeKey {
                    src: sender.clone(),
                    kind: "msg".into(),
                    id: msgid.clone().unwrap_or_else(|| text_hash(&text)),
                };
                if !self.dedupe.seen(dkey, now_ms) {
                    self.sink.emit_message(InboundMsg {
                        sender: sender.clone(),
                        addressee,
                        text,
                        msgid: msgid.clone(),
                    });
                }
                // Auto-ACK ONLY a message addressed to our exact call that carries
                // a msgid. NEVER ack a broadcast (blank addressee) or another
                // station's traffic — that is documented network SPAM. The
                // `.filter(|_| for_us)` collapses the gate to a single `if let`.
                if let Some(id) = msgid.filter(|_| for_us) {
                    let akey = DedupeKey {
                        src: sender.clone(),
                        kind: "ackout".into(),
                        id: id.clone(),
                    };
                    if !self.ack_throttle.seen(akey, now_ms) {
                        let ack = encode_ack(&sender, &id);
                        let frame = build_ui_frame(&self.identity, &ack);
                        if let Ok(b) = frame.encode() {
                            out.push(b);
                        }
                    }
                }
            }
            AprsPayload::Ack { addressee, msgid } => {
                if !self.addressed_to_us(&addressee) {
                    return out;
                }
                let key = DedupeKey {
                    src: sender,
                    kind: "ack".into(),
                    id: msgid.clone(),
                };
                if self.dedupe.seen(key, now_ms) {
                    return out;
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
                    return out;
                }
                let key = DedupeKey {
                    src: sender,
                    kind: "rej".into(),
                    id: msgid.clone(),
                };
                if self.dedupe.seen(key, now_ms) {
                    return out;
                }
                if self.tx.on_ack(&msgid) {
                    self.sink.emit_state(StateChange {
                        msgid,
                        state: DeliveryState::Rejected,
                    });
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
        // Store the RAW AX.25 frame; the transport-specific drain wraps it (KISS via
        // `tick`, raw via `tick_frames`). Keeping the TxQueue transport-neutral lets one
        // engine serve both the KISS link and the native-Benshi path.
        let bytes = match frame.encode() {
            Ok(b) => b,
            Err(_) => {
                // Couldn't build the frame — emit a terminal state so the in-flight
                // capacity slot reserved upstream in AprsState::send is released, not leaked.
                self.sink.emit_state(StateChange {
                    msgid: msgid.to_string(),
                    state: DeliveryState::TimedOut,
                });
                return;
            }
        };
        if self.tx.enqueue(msgid.to_string(), bytes, now_ms).is_ok() {
            self.sink.emit_state(StateChange {
                msgid: msgid.to_string(),
                state: DeliveryState::Sent,
            });
        } else {
            // CapacityFull at the queue (defense-in-depth backstop; AprsState gates first).
            // Release the slot rather than leak it.
            self.sink.emit_state(StateChange {
                msgid: msgid.to_string(),
                state: DeliveryState::TimedOut,
            });
        }
    }

    /// Queue a FIRE-ONCE broadcast (no recipient): **blank 9-space addressee, NO
    /// msgno**, transmitted exactly once with no retransmit and no ACK expected.
    /// Ground-truthed against the BTECH/UV-Pro app on air (2026-06-13): a
    /// no-recipient message is `:` + 9 spaces + `:` + text. `local_id` is a
    /// UI-only tracking handle (there is no wire msgid); we emit `Sent` once and
    /// never a terminal state — a broadcast has no delivery confirmation on a
    /// party line. Does NOT consume an in-flight ACK slot (nothing to await).
    pub fn enqueue_broadcast(&mut self, text: &str, local_id: &str, now_ms: u64) {
        let info = encode_message("", text, None);
        let frame = build_ui_frame(&self.identity, &info);
        let bytes = match frame.encode() {
            Ok(b) => b,
            Err(_) => return, // unencodable — nothing queued, nothing to release
        };
        let state = if self.tx.enqueue_once(local_id.to_string(), bytes, now_ms).is_ok() {
            DeliveryState::Sent
        } else {
            // Queue full (8 fire-once frames in one tick — rare). Signal non-send.
            DeliveryState::TimedOut
        };
        self.sink.emit_state(StateChange {
            msgid: local_id.to_string(),
            state,
        });
    }

    /// Drive the retransmit clock; returns due TX frames as RAW AX.25 (no KISS wrap) for
    /// non-KISS transports — the native Benshi path fragments these via `send_aprs_frame`.
    /// Emits TimedOut for messages whose retransmit budget is spent. `tick` is the
    /// KISS-wrapping sibling for the KISS link path. Live caller: the native driver
    /// (`NativeDriver::drain_due`), spawned by `start_native`.
    pub fn tick_frames(&mut self, now_ms: u64) -> Vec<Vec<u8>> {
        let due: Vec<Vec<u8>> = self.tx.tick(now_ms).into_iter().map(|d| d.bytes).collect();
        for msgid in self.tx.take_timed_out() {
            self.sink.emit_state(StateChange {
                msgid,
                state: DeliveryState::TimedOut,
            });
        }
        due
    }

    /// Drive the retransmit clock; returns KISS-ready frames to write now. Emits TimedOut.
    /// The KISS-wrapping wrapper over `tick_frames` for the KISS link path.
    pub fn tick(&mut self, now_ms: u64) -> Vec<Vec<u8>> {
        self.tick_frames(now_ms)
            .iter()
            .map(|raw| kiss_data_frame(raw))
            .collect()
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

    /// Attempt a position decode on a heard frame and emit it (deduped) if one
    /// is present. Tries the standard uncompressed/compressed parser first;
    /// failing that, the Mic-E parser (which needs the AX.25 destination
    /// callsign, where Mic-E packs latitude). `dest` is the base destination
    /// call (SSID already stripped by the AX.25 `Address`). No-op when the frame
    /// carries no well-formed position (message/ack/telemetry/etc.).
    ///
    /// Dedupe is keyed by `sender` + the decoded coordinates+symbol, so a
    /// re-beacon of an unchanged fix (or a digipeated duplicate) is suppressed
    /// while a MOVE (new coordinates) emits immediately — latest-position-wins.
    fn try_emit_position(&mut self, sender: &str, dest: &str, info: &[u8], now_ms: u64) {
        // An OBJECT (`;`) / ITEM (`)`) report carries the position of a NAMED
        // entity (a weather object, event marker, ARES asset, …), not the
        // sender's own location. A killed (`_`) object/item is a tombstone — not
        // plotted; the map's TTL sweep retires any prior pin. Otherwise fall back
        // to the station-own beacon DTIs (uncompressed/compressed/Mic-E).
        let (pos, name) = match parse_object_or_item(info) {
            Some(obj) => {
                if !obj.alive {
                    return;
                }
                (obj.position, Some(obj.name))
            }
            None => {
                let pos = match parse_position(info) {
                    Some(p) => p,
                    None => match parse_mice(dest, info) {
                        Some(p) => p,
                        None => return,
                    },
                };
                (pos, None)
            }
        };
        // Dedupe + map identity is the ENTITY: an object's name, else the sender.
        // Two distinct objects reported by the same sender must each get a pin.
        let identity = name.clone().unwrap_or_else(|| sender.to_string());
        let key = DedupeKey {
            src: identity,
            kind: "pos".into(),
            // Round to ~1e-4 deg (~11 m) so float jitter does not defeat dedupe,
            // while a genuine move still produces a distinct key. The comment
            // hash is part of the key so a station that re-beacons from the SAME
            // spot with a CHANGED comment/status (e.g. "/A=001234 QRT") is not
            // suppressed — that is a real update the map popup should reflect.
            id: format!(
                "{:.4},{:.4},{}{},a{},{}",
                pos.lat,
                pos.lon,
                pos.symbol_table,
                pos.symbol_code,
                pos.ambiguity,
                text_hash(&pos.comment)
            ),
        };
        if self.pos_dedupe.seen(key, now_ms) {
            return;
        }
        self.sink.emit_position(InboundPos {
            sender: sender.to_string(),
            name,
            lat: pos.lat,
            lon: pos.lon,
            symbol_table: pos.symbol_table,
            symbol_code: pos.symbol_code,
            comment: pos.comment,
            ambiguity: pos.ambiguity,
        });
    }
}

fn text_hash(text: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    format!("h{:x}", h.finish())
}

// ---------------------------------------------------------------------------
// Async lifecycle (Task 10): managed `AprsState`, the blocking driver, and the
// Tauri event sink.
//
// **The driver is a sync `fn run` on `spawn_blocking`, NOT a plain async task.**
// `ByteLink::read` is blocking link I/O; running it directly on the async
// executor starves the tokio runtime (the same rule the packet path follows —
// see `winlink_backend.rs:868`). So `run` uses `std::sync::mpsc`,
// `std::thread::sleep`, and a `std::time::Instant`-derived `now_ms`.
// ---------------------------------------------------------------------------

/// A command crossing the channel from a Tauri command into the blocking driver.
pub enum TxCommand {
    Send {
        dest: String,
        text: String,
        msgid: String,
    },
    /// A no-recipient broadcast: blank addressee, fire-once, no ACK. `local_id`
    /// is a UI tracking handle, not a wire msgid.
    Broadcast {
        text: String,
        local_id: String,
    },
    Abort,
}

/// Driver handle stored in `AprsState` while listening.
struct AprsHandle {
    cmd_tx: mpsc::Sender<TxCommand>,
    abort: Arc<AtomicBool>,
}

/// Tauri-managed APRS engine lifecycle. `start` opens the link and spawns the
/// blocking driver; `send`/`abort` forward commands; `stop` signals the driver
/// to exit. In-flight capacity is gated synchronously in `send` before the
/// command crosses the channel.
#[derive(Default)]
pub struct AprsState {
    inner: std::sync::Mutex<Option<AprsHandle>>,
    listening: Arc<AtomicBool>,
    counter: AtomicU64,
    in_flight: Arc<AtomicUsize>,
}

impl AprsState {
    pub fn is_listening(&self) -> bool {
        self.listening.load(Ordering::SeqCst)
    }

    /// Open the link, build the engine + sink, spawn the blocking driver, store the handle.
    ///
    /// `cfg` is any directly-connectable KISS byte-pipe — `Bluetooth` RFCOMM, `Tcp`
    /// (Dire Wolf / SoundModem), or `Serial` (USB TNC). `connect_link_with_abort`
    /// opens whichever variant the operator configured (tuxlink-a20f multi-transport);
    /// the native UV-Pro GAIA path is a separate entry (`start_native`).
    pub fn start(
        &self,
        app: tauri::AppHandle,
        cfg: crate::winlink::ax25::link::KissLinkConfig,
        identity: AprsIdentity,
    ) -> Result<(), String> {
        let abort = Arc::new(AtomicBool::new(false));
        let (link, _abort_sock) = crate::winlink::ax25::connect_link_with_abort(&cfg, abort.clone())
            .map_err(|e| {
                format!(
                    "could not open the radio link ({e}). Is the packet session using it, or the radio off?"
                )
            })?;
        let sink: Box<dyn EventSink> = Box::new(TauriEventSink {
            app,
            in_flight: self.in_flight.clone(),
        });
        let engine = AprsEngine::new(identity, sink);
        let (cmd_tx, cmd_rx) = mpsc::channel::<TxCommand>();
        let listening = self.listening.clone();
        let abort_for_task = abort.clone();
        tokio::task::spawn_blocking(move || run(link, engine, cmd_rx, listening, abort_for_task));
        *self.inner.lock().unwrap() = Some(AprsHandle { cmd_tx, abort });
        Ok(())
    }

    /// Start APRS over the UV-Pro **native** path: instead of opening a KISS link,
    /// reuse the already-connected `UvproSession` (control + chat share the one
    /// connection). Takes the session's inbound-APRS receiver and spawns the native
    /// driver (the analogue of `start`'s `run`). The session must already be
    /// connected (the control connection is brought up first); stopping APRS aborts
    /// the driver but leaves the session connected — control stays live.
    pub fn start_native(
        &self,
        app: tauri::AppHandle,
        session: Arc<crate::winlink::ax25::uvpro::session::UvproSession>,
        identity: AprsIdentity,
    ) -> Result<(), String> {
        if !session.is_connected() {
            return Err(
                "connect the UV-Pro first — native APRS shares its control connection".to_string(),
            );
        }
        // One receiver per connection: the native driver owns the inbound channel
        // the session's event pump feeds. A second start (without reconnect) finds
        // it already taken — surfaced as "already listening".
        let aprs_rx = session
            .take_aprs_receiver()
            .ok_or_else(|| "native APRS is already listening on the UV-Pro".to_string())?;
        let abort = Arc::new(AtomicBool::new(false));
        let sink: Box<dyn EventSink> = Box::new(TauriEventSink {
            app,
            in_flight: self.in_flight.clone(),
        });
        let engine = AprsEngine::new(identity, sink);
        let (cmd_tx, cmd_rx) = mpsc::channel::<TxCommand>();
        let tx: Box<dyn crate::winlink::aprs::native_driver::AprsFrameTx> = Box::new(session);
        let listening = self.listening.clone();
        let abort_for_task = abort.clone();
        tokio::task::spawn_blocking(move || {
            crate::winlink::aprs::native_driver::run_native(
                aprs_rx,
                tx,
                engine,
                cmd_rx,
                listening,
                abort_for_task,
            )
        });
        *self.inner.lock().unwrap() = Some(AprsHandle { cmd_tx, abort });
        Ok(())
    }

    pub fn stop(&self) {
        if let Some(h) = self.inner.lock().unwrap().take() {
            h.abort.store(true, Ordering::SeqCst);
        }
    }

    /// Send an APRS message. `dest = Some(callsign)` is a **directed** message
    /// (mint msgid, gate in-flight capacity, bounded retransmit + ACK). `dest =
    /// None` or an empty/whitespace string is a **broadcast** (blank addressee,
    /// fire-once, no msgid, no ACK, no in-flight slot). Returns the tracking id
    /// the UI uses to follow delivery state (a real wire msgid for directed; a
    /// `b`-prefixed UI-only handle for broadcast).
    pub fn send(&self, dest: Option<String>, text: String) -> Result<String, String> {
        let guard = self.inner.lock().unwrap();
        let handle = guard
            .as_ref()
            .ok_or_else(|| "not listening — start APRS first".to_string())?;
        match dest {
            Some(call) if !call.trim().is_empty() => {
                if self.in_flight.load(Ordering::SeqCst) >= crate::winlink::aprs::tx::CONCURRENT_CAP {
                    return Err("too many messages pending — wait for acks or timeouts".into());
                }
                let n = self.counter.fetch_add(1, Ordering::SeqCst);
                let msgid = mint_msgid(n);
                self.in_flight.fetch_add(1, Ordering::SeqCst);
                handle
                    .cmd_tx
                    .send(TxCommand::Send {
                        dest: call,
                        text,
                        msgid: msgid.clone(),
                    })
                    .map_err(|_| "APRS driver stopped".to_string())?;
                Ok(msgid)
            }
            _ => {
                // Broadcast: no wire msgid, no ACK, no in-flight gating. The
                // `b`-prefix keeps the UI handle distinct from base-36 msgids.
                let n = self.counter.fetch_add(1, Ordering::SeqCst);
                let local_id = format!("b{}", mint_msgid(n));
                handle
                    .cmd_tx
                    .send(TxCommand::Broadcast {
                        text,
                        local_id: local_id.clone(),
                    })
                    .map_err(|_| "APRS driver stopped".to_string())?;
                Ok(local_id)
            }
        }
    }

    pub fn abort(&self) {
        if let Some(h) = self.inner.lock().unwrap().as_ref() {
            let _ = h.cmd_tx.send(TxCommand::Abort);
        }
    }
}

/// 1-5 char alphanumeric msgid (base-36 of a monotonic counter, wraps within 5 chars).
fn mint_msgid(n: u64) -> String {
    const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut n = n % 36u64.pow(5);
    if n == 0 {
        return "0".into();
    }
    let mut s = Vec::new();
    while n > 0 {
        s.push(ALPHABET[(n % 36) as usize]);
        n /= 36;
    }
    s.reverse();
    String::from_utf8(s).unwrap()
}

/// The blocking driver. Runs on `spawn_blocking`; owns the link + engine; polls
/// the link, drains commands, drives the retransmit clock; exits on abort/EOF/error.
fn run(
    mut link: Box<dyn crate::winlink::ax25::link::ByteLink>,
    mut engine: AprsEngine,
    cmd_rx: mpsc::Receiver<TxCommand>,
    listening: Arc<AtomicBool>,
    abort: Arc<AtomicBool>,
) {
    let started = std::time::Instant::now();
    let now_ms = || started.elapsed().as_millis() as u64;
    listening.store(true, Ordering::SeqCst);
    engine.set_listening(true);
    let mut buf = [0u8; 1024];
    loop {
        if abort.load(Ordering::SeqCst) {
            break;
        }
        match link.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                for frame in engine.handle_inbound_bytes(&buf[..n], now_ms()) {
                    let _ = link.write_all(&frame);
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => break,
        }
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                TxCommand::Send { dest, text, msgid } => {
                    engine.enqueue_send(&dest, &text, &msgid, now_ms())
                }
                TxCommand::Broadcast { text, local_id } => {
                    engine.enqueue_broadcast(&text, &local_id, now_ms())
                }
                TxCommand::Abort => engine.abort(),
            }
        }
        for frame in engine.tick(now_ms()) {
            let _ = link.write_all(&frame);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    // On teardown (stop / link close / error), drain any pending retransmits to terminal
    // states so their in-flight capacity slots are released. Without this, a Stop-with-pending
    // (the routine listening toggle) permanently leaks slots and eventually wedges `send`.
    engine.abort();
    listening.store(false, Ordering::SeqCst);
    engine.set_listening(false);
}

/// `EventSink` that forwards engine events to the UI via Tauri events, and
/// releases an in-flight slot on each terminal delivery transition.
pub struct TauriEventSink {
    pub app: tauri::AppHandle,
    pub in_flight: Arc<AtomicUsize>,
}

impl EventSink for TauriEventSink {
    fn emit_message(&self, ev: InboundMsg) {
        let _ = self.app.emit("aprs-message:new", &ev);
    }
    fn emit_state(&self, ev: StateChange) {
        if ev.state.is_terminal() {
            let _ = self
                .in_flight
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| {
                    Some(v.saturating_sub(1))
                });
        }
        let _ = self.app.emit("aprs-message:state", &ev);
    }
    fn emit_listening(&self, on: bool) {
        let _ = self.app.emit("aprs-listening:change", on);
    }
    fn emit_position(&self, ev: InboundPos) {
        let _ = self.app.emit("aprs-position:new", &ev);
    }
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
        positions: Arc<Mutex<Vec<InboundPos>>>,
    }
    impl EventSink for RecSink {
        fn emit_message(&self, ev: InboundMsg) {
            self.msgs.lock().unwrap().push(ev);
        }
        fn emit_state(&self, ev: StateChange) {
            self.states.lock().unwrap().push(ev);
        }
        fn emit_listening(&self, _on: bool) {}
        fn emit_position(&self, ev: InboundPos) {
            self.positions.lock().unwrap().push(ev);
        }
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

    fn inbound_with(src: &str, info: &[u8]) -> Vec<u8> {
        let f = Frame {
            path: Path {
                dest: Address { call: "APZTUX".into(), ssid: 0 },
                src: Address { call: src.into(), ssid: 0 },
                digis: vec![],
            },
            control: Control::Ui { pf: false },
            info: info.to_vec(),
        };
        kiss_data_frame(&f.encode().unwrap())
    }

    #[test]
    fn heard_message_for_another_station_is_emitted_but_not_acked() {
        // Party line (tuxlink-iehg): a message addressed to ANOTHER station is
        // shown in the feed (with its addressee) but never auto-ACKed — acking
        // another station's traffic is documented network SPAM.
        let sink = RecSink::default();
        let mut engine = AprsEngine::new(identity(), Box::new(sink.clone()));
        let tx = engine.handle_inbound_bytes(&inbound_with("KK6XYZ", b":W7OTHER  :hi{05"), 1000);
        let msgs = sink.msgs.lock().unwrap();
        assert_eq!(msgs.len(), 1, "heard message must reach the feed");
        assert_eq!(msgs[0].sender, "KK6XYZ");
        assert_eq!(msgs[0].addressee, "W7OTHER");
        assert_eq!(msgs[0].text, "hi");
        assert!(tx.is_empty(), "must NOT auto-ACK a message for another station");
    }

    #[test]
    fn heard_broadcast_blank_addressee_is_emitted_not_acked() {
        // A no-recipient broadcast (blank 9-space addressee) appears in the feed
        // with an empty addressee and is never ACKed — even though the BTECH app
        // wastefully attaches a msgno, nobody acks a blank addressee.
        let sink = RecSink::default();
        let mut engine = AprsEngine::new(identity(), Box::new(sink.clone()));
        let tx = engine.handle_inbound_bytes(&inbound_with("KK6XYZ", b":         :net up{2"), 1000);
        let msgs = sink.msgs.lock().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].addressee, "", "blank addressee == broadcast");
        assert_eq!(msgs[0].text, "net up");
        assert!(tx.is_empty(), "a broadcast is never auto-ACKed");
    }

    #[test]
    fn broadcast_send_is_blank_addressee_no_msgno_and_fires_exactly_once() {
        // Ground-truthed wire form: a no-recipient broadcast = `:` + 9 spaces +
        // `:` + text, NO `{msgno`, transmitted once with no retransmit/timeout.
        let sink = RecSink::default();
        let mut engine = AprsEngine::new(identity(), Box::new(sink.clone()));
        engine.enqueue_broadcast("ALCON test", "bX", 0);
        let due = engine.tick_frames(0);
        assert_eq!(due.len(), 1, "broadcast transmits exactly once");
        let decoded = Frame::decode(&due[0]).unwrap();
        assert_eq!(decoded.info, b":         :ALCON test", "blank addressee, no msgno");
        assert!(engine.tick_frames(30_000).is_empty(), "no retransmit");
        assert!(engine.tick_frames(200_000).is_empty(), "broadcast never times out");
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

    #[test]
    fn abort_emits_a_terminal_for_each_pending_to_release_in_flight() {
        // The driver calls engine.abort() on teardown; every pending message must hit a
        // terminal state so its in-flight capacity slot is released (no stop()-leak).
        let sink = RecSink::default();
        let mut engine = AprsEngine::new(identity(), Box::new(sink.clone()));
        engine.enqueue_send("KK6XYZ", "a", "01", 0);
        engine.enqueue_send("KK6XYZ", "b", "02", 0);
        engine.abort();
        let states = sink.states.lock().unwrap();
        let timed_out: Vec<&str> = states
            .iter()
            .filter(|s| s.state == DeliveryState::TimedOut)
            .map(|s| s.msgid.as_str())
            .collect();
        assert!(timed_out.contains(&"01"));
        assert!(timed_out.contains(&"02"));
    }

    /// A raw AX.25 (non-KISS) inbound frame routes through the same promiscuous decode +
    /// auto-ACK as the KISS path, but the auto-ACK comes back RAW — the native transport
    /// fragments it itself, no KISS wrap.
    #[test]
    fn handle_inbound_frame_routes_raw_ax25_and_acks_raw() {
        let sink = RecSink::default();
        let mut engine = AprsEngine::new(identity(), Box::new(sink.clone()));
        // Same content as inbound_message_bytes(), but RAW (KISS framing stripped).
        let raw = strip_kiss(&inbound_message_bytes());
        let out = engine.handle_inbound_frame(&raw, 1000);
        let msgs = sink.msgs.lock().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].sender, "KK6XYZ");
        assert_eq!(msgs[0].text, "ping");
        // The auto-ACK is returned RAW: it decodes directly as an AX.25 frame (no KISS strip).
        assert_eq!(out.len(), 1);
        let decoded = Frame::decode(&out[0]).unwrap();
        assert_eq!(decoded.path.dest.call, "APZTUX");
        assert_eq!(decoded.info, b":KK6XYZ   :ack04");
    }

    /// `tick_frames` drains due retransmits as RAW AX.25; `tick` returns the byte-identical
    /// frame KISS-wrapped. Both draw from the one transport-neutral TxQueue, so
    /// `tick` == `kiss_data_frame(tick_frames)`.
    #[test]
    fn tick_frames_returns_raw_while_tick_kiss_wraps() {
        let sink = RecSink::default();
        let mut e_raw = AprsEngine::new(identity(), Box::new(sink.clone()));
        e_raw.enqueue_send("KK6XYZ", "hello", "07", 0);
        let raw = e_raw.tick_frames(0);
        assert_eq!(raw.len(), 1);
        // Raw form decodes directly (no KISS framing bytes).
        assert_eq!(Frame::decode(&raw[0]).unwrap().info, b":KK6XYZ   :hello{07");

        // Identical frame drained via tick() comes back KISS-wrapped — exactly the wrap of raw.
        let mut e_kiss = AprsEngine::new(identity(), Box::new(sink.clone()));
        e_kiss.enqueue_send("KK6XYZ", "hello", "07", 0);
        let wrapped = e_kiss.tick(0);
        assert_eq!(wrapped.len(), 1);
        assert_eq!(wrapped[0], kiss_data_frame(&raw[0]));
        assert_eq!(
            Frame::decode(&strip_kiss(&wrapped[0])).unwrap().info,
            b":KK6XYZ   :hello{07"
        );
    }

    fn strip_kiss(b: &[u8]) -> Vec<u8> {
        let mut d = crate::winlink::ax25::kiss::KissDecoder::new();
        d.push(b).into_iter().next().unwrap()
    }

    #[test]
    fn aprs_state_starts_not_listening() {
        let st = AprsState::default();
        assert!(!st.is_listening());
    }

    // -- Position RX (tuxlink-6vgt) -----------------------------------------

    /// Build a KISS-wrapped inbound UI frame with an arbitrary destination call
    /// (Mic-E packs latitude into the dest, so position tests need to set it).
    fn inbound_with_dest(src: &str, dest: &str, info: &[u8]) -> Vec<u8> {
        let f = Frame {
            path: Path {
                dest: Address { call: dest.into(), ssid: 0 },
                src: Address { call: src.into(), ssid: 0 },
                digis: vec![],
            },
            control: Control::Ui { pf: false },
            info: info.to_vec(),
        };
        kiss_data_frame(&f.encode().unwrap())
    }

    #[test]
    fn heard_object_emits_position_labeled_by_object_name() {
        let sink = RecSink::default();
        let mut engine = AprsEngine::new(identity(), Box::new(sink.clone()));
        // A digi (DIGI1) reports an OBJECT "LEADER" at 49.058,-72.029. The map
        // pin must be labeled by the object NAME, not the reporting station.
        engine.handle_inbound_bytes(
            &inbound_with("DIGI1", b";LEADER   *092345z4903.50N/07201.75W>"),
            1000,
        );
        let pos = sink.positions.lock().unwrap();
        assert_eq!(pos.len(), 1);
        assert_eq!(pos[0].sender, "DIGI1");
        assert_eq!(pos[0].name.as_deref(), Some("LEADER"));
        assert!((pos[0].lat - 49.058333).abs() < 1e-3);
        assert_eq!(pos[0].symbol_code, '>');
    }

    #[test]
    fn killed_object_is_not_plotted() {
        let sink = RecSink::default();
        let mut engine = AprsEngine::new(identity(), Box::new(sink.clone()));
        // A KILLED object ('_') is a tombstone — it must not emit a pin.
        engine.handle_inbound_bytes(
            &inbound_with("DIGI1", b";LEADER   _092345z4903.50N/07201.75W>"),
            1000,
        );
        assert_eq!(sink.positions.lock().unwrap().len(), 0);
    }

    #[test]
    fn heard_uncompressed_position_emits_position_with_latlon_and_symbol() {
        let sink = RecSink::default();
        let mut engine = AprsEngine::new(identity(), Box::new(sink.clone()));
        // "!4903.50N/07201.75W-Hello" → 49.058, -72.029, table '/', code '-'.
        engine.handle_inbound_bytes(&inbound_with("KK6XYZ", b"!4903.50N/07201.75W-Hello"), 1000);
        let pos = sink.positions.lock().unwrap();
        assert_eq!(pos.len(), 1);
        assert_eq!(pos[0].sender, "KK6XYZ");
        assert!((pos[0].lat - 49.058333).abs() < 1e-3);
        assert!((pos[0].lon - (-72.029167)).abs() < 1e-3);
        assert_eq!(pos[0].symbol_table, '/');
        assert_eq!(pos[0].symbol_code, '-');
        assert_eq!(pos[0].comment, "Hello");
        assert_eq!(pos[0].ambiguity, 0, "a full-precision fix reports ambiguity 0");
        // tuxlink-8tz1: a position beacon emits to the map (asserted above) AND
        // now ALSO surfaces as a raw feed row (operator diagnostic — confirm all
        // heard traffic reaches the chat). The row carries the verbatim info field
        // and renders as a broadcast (blank addressee).
        let msgs = sink.msgs.lock().unwrap();
        assert_eq!(msgs.len(), 1, "non-message frame now surfaces as a raw feed row");
        assert_eq!(msgs[0].sender, "KK6XYZ");
        assert_eq!(msgs[0].addressee, "", "raw frame renders as broadcast");
        assert_eq!(msgs[0].text, "!4903.50N/07201.75W-Hello");
        assert!(msgs[0].msgid.is_none(), "a raw frame is not ACK-tracked");
    }

    #[test]
    fn heard_status_frame_surfaces_as_raw_feed_row() {
        // tuxlink-8tz1: a non-message, non-position frame (APRS status `>`) is not
        // a `parse_info` payload and not a position — historically dropped. It must
        // now surface as a raw feed row so the operator sees the full channel.
        let sink = RecSink::default();
        let mut engine = AprsEngine::new(identity(), Box::new(sink.clone()));
        engine.handle_inbound_bytes(&inbound_with("KK6XYZ", b">EOC active, monitoring 146.52"), 1000);
        let msgs = sink.msgs.lock().unwrap();
        assert_eq!(msgs.len(), 1, "status frame surfaces as a raw feed row");
        assert_eq!(msgs[0].sender, "KK6XYZ");
        assert_eq!(msgs[0].addressee, "");
        assert_eq!(msgs[0].text, ">EOC active, monitoring 146.52");
        assert!(
            sink.positions.lock().unwrap().is_empty(),
            "a status frame is not a position"
        );
    }

    #[test]
    fn heard_ambiguous_position_reports_ambiguity_level() {
        let sink = RecSink::default();
        let mut engine = AprsEngine::new(identity(), Box::new(sink.clone()));
        // One masked hundredths-of-minute digit in each coordinate => level 1.
        engine.handle_inbound_bytes(&inbound_with("KK6XYZ", b"!4903.5 N/07201.7 W-"), 1000);
        let pos = sink.positions.lock().unwrap();
        assert_eq!(pos.len(), 1);
        assert_eq!(pos[0].ambiguity, 1, "the masked-digit fix must surface as ambiguous");
    }

    #[test]
    fn heard_compressed_position_emits_position() {
        let sink = RecSink::default();
        let mut engine = AprsEngine::new(identity(), Box::new(sink.clone()));
        // aprslib reference compressed report "/5L!!<*e7>" → 49.5, -72.75.
        engine.handle_inbound_bytes(&inbound_with("KK6XYZ", b"!/5L!!<*e7>  T"), 1000);
        let pos = sink.positions.lock().unwrap();
        assert_eq!(pos.len(), 1);
        assert!((pos[0].lat - 49.5).abs() < 1e-3);
        assert!((pos[0].lon - (-72.75)).abs() < 1e-3);
    }

    #[test]
    fn heard_mice_frame_emits_position_using_dest_latitude() {
        let sink = RecSink::default();
        let mut engine = AprsEngine::new(identity(), Box::new(sink.clone()));
        // Mic-E: dest "332UVT" carries lat; info carries lon+symbol (position.rs vector).
        let mice_info: &[u8] = b"\x60\x28\x60\x6e\x1c\x1c\x1c\x3e\x2f";
        engine.handle_inbound_bytes(&inbound_with_dest("KK6XYZ", "332UVT", mice_info), 1000);
        let pos = sink.positions.lock().unwrap();
        assert_eq!(pos.len(), 1, "a Mic-E frame must emit a position");
        assert_eq!(pos[0].sender, "KK6XYZ");
        assert!((pos[0].lat - 33.427333).abs() < 1e-3);
        assert!((pos[0].lon - (-112.147)).abs() < 1e-3);
        assert_eq!(pos[0].symbol_code, '>');
        assert_eq!(pos[0].symbol_table, '/');
    }

    #[test]
    fn heard_message_frame_emits_message_not_position() {
        let sink = RecSink::default();
        let mut engine = AprsEngine::new(identity(), Box::new(sink.clone()));
        engine.handle_inbound_bytes(&inbound_message_bytes(), 1000);
        assert_eq!(sink.msgs.lock().unwrap().len(), 1, "message frame routes to chat");
        assert!(
            sink.positions.lock().unwrap().is_empty(),
            "a message frame carries no position"
        );
    }

    #[test]
    fn identical_position_rebeacon_is_deduped_within_window() {
        let sink = RecSink::default();
        let mut engine = AprsEngine::new(identity(), Box::new(sink.clone()));
        let frame = inbound_with("KK6XYZ", b"!4903.50N/07201.75W-Hello");
        engine.handle_inbound_bytes(&frame, 1_000);
        engine.handle_inbound_bytes(&frame, 60_000); // same fix within window
        assert_eq!(
            sink.positions.lock().unwrap().len(),
            1,
            "an identical re-beacon must not re-emit"
        );
    }

    #[test]
    fn moved_station_emits_new_position_immediately() {
        let sink = RecSink::default();
        let mut engine = AprsEngine::new(identity(), Box::new(sink.clone()));
        engine.handle_inbound_bytes(&inbound_with("KK6XYZ", b"!4903.50N/07201.75W-"), 1_000);
        // Different coordinates from the same station => not a dup, emits again.
        engine.handle_inbound_bytes(&inbound_with("KK6XYZ", b"!4905.00N/07205.00W-"), 2_000);
        assert_eq!(
            sink.positions.lock().unwrap().len(),
            2,
            "a moved station must emit the new fix"
        );
    }

    #[test]
    fn same_spot_changed_comment_re_emits() {
        let sink = RecSink::default();
        let mut engine = AprsEngine::new(identity(), Box::new(sink.clone()));
        // Same coordinates + symbol, but the station updates its status/comment.
        // That is a real change (e.g. "QRT", a new altitude) the map must reflect,
        // so it must NOT be collapsed by the position dedupe.
        engine.handle_inbound_bytes(&inbound_with("KK6XYZ", b"!4903.50N/07201.75W-In service"), 1_000);
        engine.handle_inbound_bytes(&inbound_with("KK6XYZ", b"!4903.50N/07201.75W-QRT"), 60_000);
        let pos = sink.positions.lock().unwrap();
        assert_eq!(pos.len(), 2, "a changed comment at the same spot must re-emit");
        assert_eq!(pos[1].comment, "QRT");
    }

    #[test]
    fn precision_change_at_same_spot_re_emits() {
        let sink = RecSink::default();
        let mut engine = AprsEngine::new(identity(), Box::new(sink.clone()));
        // Same rounded coords + symbol + comment, but the station drops from a
        // full-precision fix to an ambiguous one — a real precision change the
        // map must reflect, so the dedupe (which now keys on ambiguity) re-emits.
        engine.handle_inbound_bytes(&inbound_with("KK6XYZ", b"!4903.50N/07201.50W-"), 1_000);
        engine.handle_inbound_bytes(&inbound_with("KK6XYZ", b"!4903.5 N/07201.5 W-"), 60_000);
        let pos = sink.positions.lock().unwrap();
        assert_eq!(pos.len(), 2, "a precision change at the same spot must re-emit");
        assert_eq!(pos[1].ambiguity, 1);
    }
}
