# UV-Pro Audio Transport (SSTV component 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the net-new GAIA audio transport for the UV-Pro — open a second RFCOMM "audio" channel alongside the existing GAIA control link, frame/deframe `AudioData`/`AudioEnd`/`AudioAck` messages, stream opaque SBC payloads with a working RADIO-1 abort, and wire it into `UvproSession`. The SBC codec and SSTV codec are sibling sub-projects behind a trait seam.

**Architecture:** The existing `UvproSession` owns ONE RFCOMM socket (the GAIA control/data channel — `gaia.rs` + `Driver`). The audio path is architecturally separate: a SECOND `RfcommSocket` to the radio's audio-gateway RFCOMM service, multiplexed over the same Bluetooth ACL link (this is what the operator observed as "no connection drop" — a 2nd RFCOMM channel, NOT a BT-profile switch). On that channel the bytes are HDLC-framed (`0x7e` delimiter, `0x7d` escape) `AudioData(sbc_bytes)` frames. The SBC codec sits behind an `SbcCodec` trait so the transport is testable with a fake; the real pure-Rust codec is a sibling plan (bd: SBC codec). Keying defaults to **implicit** (benlink's working POC sends NO `c1.TX_AUDIO` — opening the channel + streaming `AudioData` keys TX; `AudioEnd` de-keys); a `c1`-over-GAIA keying hook is built but defaults OFF, to be confirmed/flipped by the operator HCI snoop.

**Tech Stack:** Rust (`src-tauri`), existing `RfcommSocket` (`AF_BLUETOOTH`/`BTPROTO_RFCOMM`, root-free), `ByteLink` trait, `sdptool` SDP query (existing pattern), `libc`. No new crates in THIS plan (SBC crate lands with the codec sub-project).

**Locked spec source:** `bd show tuxlink-bcsy` NOTES (RE'd 2026-06-13 from benlink + decompile). Reference code (gitignored, main checkout only): `dev/scratch/benshi-re/benlink/src/benlink/{protocol/audio.py,link.py,audio.py,examples/audiotransmit.py,examples/audiomonitor.py}`.

**RADIO-1 / ADR 0018:** This is RF-path code — write/test/ship freely; only the operator's on-air run is gated. The correctness bar that DOES apply: a working abort that halts TX (`AudioEnd` + drop the audio socket) and no runaway-TX. No tuxlink-added airtime cap / TOT (operator owns ~5 radios, confirms no such limit; HTCommander's 60s claim is unreliable — do NOT propagate it).

---

## Wire spec (ground truth — verified against benlink `protocol/audio.py`)

Audio-channel frame = `0x7e` ++ escape(`type_byte` ++ payload) ++ `0x7e`.
- Escape (applied to the bytes BETWEEN the delimiters): any `0x7d` or `0x7e` byte → `0x7d` then `(byte ^ 0x20)`. Unescape: `0x7d` then next byte `^ 0x20`.
- `type_byte`: `0x00` = AudioData (payload = SBC bytes), `0x01` = AudioEnd, `0x02` = AudioAck, other = AudioUnknown(type, data).
- benlink emits AudioEnd as `0x01` ++ eight `0x00` bytes, and AudioAck as `0x02` ++ eight `0x00` bytes. **On decode the trailing bytes are ignored** (the type byte alone determines the message). We MUST tolerate End/Ack with or without the 8-byte pad on RX, and we transmit End with the 8-byte pad to byte-match the app.
- The radio sends **no ack** for AudioData or AudioEnd (benlink comments confirm). AudioAck exists in the enum but is not part of the normal send loop.
- SBC payload is opaque to the framing layer (the `SbcCodec` produces/consumes it).

Hand-derived golden vectors (no radio needed — pure byte math):

| Message | Wire bytes (hex) |
|---|---|
| `AudioData([0xAB, 0xCD])` | `7e 00 ab cd 7e` |
| `AudioData([0x7e])` (delimiter in payload) | `7e 00 7d 5e 7e` |
| `AudioData([0x7d])` (escape in payload) | `7e 00 7d 5d 7e` |
| `AudioData([0x7d, 0x7e])` | `7e 00 7d 5d 7d 5e 7e` |
| `AudioData([])` (empty) | `7e 00 7e` |
| `AudioEnd` (transmit form, padded) | `7e 01 00 00 00 00 00 00 00 00 7e` |
| `AudioAck` (transmit form, padded) | `7e 02 00 00 00 00 00 00 00 00 7e` |

(`0x7e ^ 0x20 = 0x5e`; `0x7d ^ 0x20 = 0x5d`.)

---

## File structure

- Create `src-tauri/src/winlink/ax25/uvpro/audio/mod.rs` — audio submodule root; re-exports.
- Create `src-tauri/src/winlink/ax25/uvpro/audio/framing.rs` — `AudioMessage` enum, `to_bytes`/escape, `AudioDeframer` streaming parser. Pure, golden-vector tested.
- Create `src-tauri/src/winlink/ax25/uvpro/audio/codec.rs` — `SbcCodec` trait (the seam) + `NullSbcCodec` test fake (identity passthrough) + `RecordingSbcCodec` test fake.
- Create `src-tauri/src/winlink/ax25/uvpro/audio/keying.rs` — `c1` audio opcode enum + GAIA command encoders (`encode_tx_audio` / `encode_tx_audio_stop` / `encode_rx_audio` / `encode_rx_audio_stop`); used only when keying mode = Explicit.
- Create `src-tauri/src/winlink/ax25/uvpro/audio/transport.rs` — `AudioTransport`: owns the audio `ByteLink`, the codec, the deframer; `send_pcm`/`finish`/`abort` (TX) and a poll/`pump` RX path; `KeyingMode`.
- Modify `src-tauri/src/winlink/ax25/rfcomm.rs` — add `parse_audio_channel` + `resolve_audio_channel` (target the audio-gateway SDP service classes `0x1112`/`0x111f`, candidate-ranked) reusing the existing `sdptool` query.
- Modify `src-tauri/src/winlink/ax25/uvpro/mod.rs` — `pub mod audio;`.
- Modify `src-tauri/src/winlink/ax25/uvpro/session.rs` — `UvproSession::open_audio()` (resolve audio channel, connect a 2nd `RfcommSocket`, construct `AudioTransport`), `start_audio_send`/`abort_audio`; audio socket lifecycle dropped on disconnect/abort. (Detailed in Task 7 — sequenced LAST because it touches the shared session file.)

**Cross-task file ownership (avoid parallel-edit conflicts):** Tasks 1–5 each create a new file → fully parallelizable. Task 6 (rfcomm.rs) and Task 7 (session.rs) each modify ONE existing shared file and must each be a single task (not split). Task 7 depends on Tasks 1–6 (it assembles them) → sequence Task 7 last.

---

## Task 1: Audio frame codec (`framing.rs`)

**Files:**
- Create: `src-tauri/src/winlink/ax25/uvpro/audio/framing.rs`
- Create (stub): `src-tauri/src/winlink/ax25/uvpro/audio/mod.rs`
- Create (stub): make `pub mod audio;` reachable — add to `uvpro/mod.rs` (one line; see Step 6).

- [ ] **Step 1: Write the failing tests** (golden vectors are radio-free byte math — verified against benlink `protocol/audio.py`).

```rust
// in framing.rs, #[cfg(test)] mod tests
fn hex(s: &str) -> Vec<u8> {
    s.split_whitespace().map(|h| u8::from_str_radix(h, 16).unwrap()).collect()
}

#[test]
fn audio_data_to_bytes_matches_golden() {
    assert_eq!(AudioMessage::Data(vec![0xAB, 0xCD]).to_bytes(), hex("7e 00 ab cd 7e"));
}
#[test]
fn audio_data_escapes_delimiter_and_escape_bytes() {
    assert_eq!(AudioMessage::Data(vec![0x7e]).to_bytes(), hex("7e 00 7d 5e 7e"));
    assert_eq!(AudioMessage::Data(vec![0x7d]).to_bytes(), hex("7e 00 7d 5d 7e"));
    assert_eq!(AudioMessage::Data(vec![0x7d, 0x7e]).to_bytes(), hex("7e 00 7d 5d 7d 5e 7e"));
}
#[test]
fn audio_data_empty_payload() {
    assert_eq!(AudioMessage::Data(vec![]).to_bytes(), hex("7e 00 7e"));
}
#[test]
fn audio_end_transmit_form_is_padded() {
    assert_eq!(AudioMessage::End.to_bytes(), hex("7e 01 00 00 00 00 00 00 00 00 7e"));
}
#[test]
fn audio_ack_transmit_form_is_padded() {
    assert_eq!(AudioMessage::Ack.to_bytes(), hex("7e 02 00 00 00 00 00 00 00 00 7e"));
}
#[test]
fn deframer_roundtrips_data_with_escaped_bytes() {
    let mut d = AudioDeframer::new();
    let wire = AudioMessage::Data(vec![0x7e, 0x7d, 0x10]).to_bytes();
    let msgs = d.push(&wire);
    assert_eq!(msgs, vec![AudioMessage::Data(vec![0x7e, 0x7d, 0x10])]);
}
#[test]
fn deframer_tolerates_end_with_and_without_pad() {
    let mut d = AudioDeframer::new();
    assert_eq!(d.push(&hex("7e 01 7e")), vec![AudioMessage::End]); // unpadded
    assert_eq!(d.push(&hex("7e 01 00 00 00 00 00 00 00 00 7e")), vec![AudioMessage::End]); // padded
}
#[test]
fn deframer_reassembles_frame_split_across_pushes() {
    let mut d = AudioDeframer::new();
    assert!(d.push(&hex("7e 00 ab")).is_empty()); // partial (no closing delimiter)
    assert_eq!(d.push(&hex("cd 7e")), vec![AudioMessage::Data(vec![0xAB, 0xCD])]);
}
#[test]
fn deframer_yields_two_frames_from_one_buffer() {
    let mut d = AudioDeframer::new();
    let mut buf = AudioMessage::Data(vec![0x01]).to_bytes();
    buf.extend(AudioMessage::End.to_bytes());
    let msgs = d.push(&buf);
    assert_eq!(msgs, vec![AudioMessage::Data(vec![0x01]), AudioMessage::End]);
}
#[test]
fn deframer_discards_garbage_before_first_delimiter() {
    let mut d = AudioDeframer::new();
    let mut wire = hex("de ad");
    wire.extend(AudioMessage::Data(vec![0x42]).to_bytes());
    assert_eq!(d.push(&wire), vec![AudioMessage::Data(vec![0x42])]);
}
#[test]
fn deframer_bounds_buffer_against_unterminated_garbage() {
    let mut d = AudioDeframer::new();
    // A long run with no closing delimiter must not grow unbounded.
    let junk = vec![0x00u8; AudioDeframer::MAX_BUFFER + 100];
    let _ = d.push(&junk);
    assert!(d.buffered_len() <= AudioDeframer::MAX_BUFFER);
}
```

- [ ] **Step 2: Run tests, verify they fail** — `cargo test -p tuxlink --manifest-path src-tauri/Cargo.toml uvpro::audio::framing` → FAIL (types not defined). NOTE: do NOT cold-build on this Pi if it stalls; if a local build is impractical, push and let CI compile (see "Build/verify note" at the end). The failing-test step is satisfied by confirming the test code references undefined symbols.

- [ ] **Step 3: Write the implementation.**

```rust
//! UV-Pro audio-channel framing (SSTV transport, tuxlink-bcsy).
//!
//! The audio RFCOMM channel carries HDLC-style framed messages, distinct from the
//! GAIA control channel's `ff 01` framing (see `gaia.rs`). Frame layout:
//! `0x7e` ++ escape(type_byte ++ payload) ++ `0x7e`, where `0x7d`/`0x7e` in the
//! escaped region are stuffed as `0x7d` then `byte ^ 0x20`. Verified byte-for-byte
//! against benlink `protocol/audio.py` (RE source; see bd tuxlink-bcsy notes).

const DELIM: u8 = 0x7e;
const ESC: u8 = 0x7d;
const ESC_XOR: u8 = 0x20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioMessage {
    /// `0x00` — one chunk of opaque (SBC) audio payload.
    Data(Vec<u8>),
    /// `0x01` — transmit finished / de-key. Transmitted with an 8-byte zero pad to
    /// byte-match the vendor app; on RX the pad is ignored.
    End,
    /// `0x02` — acknowledgement. The radio does not ack in the normal loop; kept for
    /// completeness + RX tolerance.
    Ack,
    /// Any other type byte, preserved for diagnostics.
    Unknown(u8, Vec<u8>),
}

fn escape_into(out: &mut Vec<u8>, payload: &[u8]) {
    for &b in payload {
        if b == ESC || b == DELIM {
            out.push(ESC);
            out.push(b ^ ESC_XOR);
        } else {
            out.push(b);
        }
    }
}

impl AudioMessage {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut inner = Vec::new();
        match self {
            AudioMessage::Data(p) => { inner.push(0x00); inner.extend_from_slice(p); }
            AudioMessage::End => { inner.push(0x01); inner.extend_from_slice(&[0u8; 8]); }
            AudioMessage::Ack => { inner.push(0x02); inner.extend_from_slice(&[0u8; 8]); }
            AudioMessage::Unknown(t, d) => { inner.push(*t); inner.extend_from_slice(d); }
        }
        let mut out = Vec::with_capacity(inner.len() + 2);
        out.push(DELIM);
        escape_into(&mut out, &inner);
        out.push(DELIM);
        out
    }

    fn from_unescaped(inner: &[u8]) -> Option<AudioMessage> {
        let (&t, rest) = inner.split_first()?;
        Some(match t {
            0x00 => AudioMessage::Data(rest.to_vec()),
            0x01 => AudioMessage::End,
            0x02 => AudioMessage::Ack,
            other => AudioMessage::Unknown(other, rest.to_vec()),
        })
    }
}

/// Streaming deframer: feed arbitrary RFCOMM read chunks; yields complete messages
/// and retains any partial tail. Bounds its buffer against an unterminated stream.
#[derive(Default)]
pub struct AudioDeframer {
    buf: Vec<u8>,
    /// True once we've seen the opening delimiter and are accumulating frame body.
    in_frame: bool,
    frame: Vec<u8>,
}

impl AudioDeframer {
    /// Far larger than any real audio frame (SBC frames are tens-to-hundreds of
    /// bytes); bounds a wedged/garbage peer.
    pub const MAX_BUFFER: usize = 8192;

    pub fn new() -> Self { Self::default() }
    pub fn buffered_len(&self) -> usize { self.buf.len() + self.frame.len() }

    pub fn push(&mut self, data: &[u8]) -> Vec<AudioMessage> {
        let mut out = Vec::new();
        for &b in data {
            if b == DELIM {
                if self.in_frame {
                    // Closing delimiter: unescape the accumulated body and decode.
                    if let Some(msg) = decode_body(&self.frame) {
                        out.push(msg);
                    }
                    self.frame.clear();
                    self.in_frame = false;
                } else {
                    // Opening delimiter: start a fresh frame, drop any leading garbage.
                    self.in_frame = true;
                    self.frame.clear();
                }
            } else if self.in_frame {
                self.frame.push(b);
                if self.frame.len() > Self::MAX_BUFFER {
                    // Unterminated runaway: abandon this frame, resync on next delimiter.
                    self.frame.clear();
                    self.in_frame = false;
                }
            }
            // else: byte outside any frame (pre-first-delimiter garbage) — discard.
        }
        out
    }
}

/// Unescape `0x7d`-stuffed bytes, then decode the type byte. An empty/odd-escape
/// body yields `None` (dropped).
fn decode_body(escaped: &[u8]) -> Option<AudioMessage> {
    let mut inner = Vec::with_capacity(escaped.len());
    let mut i = 0;
    while i < escaped.len() {
        if escaped[i] == ESC {
            i += 1;
            if i >= escaped.len() { return None; } // dangling escape → drop frame
            inner.push(escaped[i] ^ ESC_XOR);
        } else {
            inner.push(escaped[i]);
        }
        i += 1;
    }
    AudioMessage::from_unescaped(&inner)
}
```

Note the deframer uses a per-frame accumulator (not the GAIA `buffered_len`-of-one-buffer model) because audio frames are delimiter-bounded on BOTH ends, unlike GAIA's length-prefixed frames. `buffered_len` returns the in-flight partial so the `MAX_BUFFER` bound test can observe it.

- [ ] **Step 4: Write the `mod.rs` stub** (`src-tauri/src/winlink/ax25/uvpro/audio/mod.rs`):

```rust
//! UV-Pro audio transport (SSTV component 1, tuxlink-bcsy): a second RFCOMM channel
//! carrying SBC-encoded audio, distinct from the GAIA control channel. See the
//! per-file docs and `docs/superpowers/plans/2026-06-13-sstv-audio-transport.md`.

pub mod framing;
// pub mod codec;     // Task 2
// pub mod keying;    // Task 5
// pub mod transport; // Task 4
```

(Uncomment each as its task lands. Keep modules behind comments until they compile so the crate stays green between tasks.)

- [ ] **Step 5: Run tests, verify pass** — `cargo test --manifest-path src-tauri/Cargo.toml uvpro::audio::framing` → PASS (or push + CI; see note).

- [ ] **Step 6: Wire the module in** — add `pub mod audio;` to `src-tauri/src/winlink/ax25/uvpro/mod.rs` (read it first; insert alphabetically among the existing `pub mod` lines).

- [ ] **Step 7: Commit.**

```bash
git add src-tauri/src/winlink/ax25/uvpro/audio/framing.rs \
        src-tauri/src/winlink/ax25/uvpro/audio/mod.rs \
        src-tauri/src/winlink/ax25/uvpro/mod.rs
git commit -m "feat(uvpro): audio-channel HDLC framing for SSTV transport (tuxlink-bcsy)"
```

---

## Task 2: `SbcCodec` trait seam + test fakes (`codec.rs`)

The transport must be testable WITHOUT the real SBC codec (a sibling sub-project). Define the seam and fakes.

**Files:** Create `src-tauri/src/winlink/ax25/uvpro/audio/codec.rs`; uncomment `pub mod codec;` in `audio/mod.rs`.

- [ ] **Step 1: Write the failing test.**

```rust
#[test]
fn null_codec_is_identity() {
    let c = NullSbcCodec;
    assert_eq!(c.encode(&[1, 2, 3]), vec![1, 2, 3]);
    assert_eq!(c.decode(&[4, 5, 6]), vec![4, 5, 6]);
}
#[test]
fn recording_codec_captures_encode_inputs() {
    let c = RecordingSbcCodec::default();
    let _ = c.encode(&[9, 9]);
    assert_eq!(c.encoded_inputs(), vec![vec![9u8, 9]]);
}
```

- [ ] **Step 2: Run, verify fail.**

- [ ] **Step 3: Implement.**

```rust
//! SBC codec seam (tuxlink-bcsy). The real pure-Rust encoder/decoder is a sibling
//! sub-project; the transport depends only on this trait so it is unit-testable
//! with fakes (mirrors how `native_driver.rs` tests against a recording fake).

use std::sync::Mutex;

/// Encode 32 kHz mono s16le PCM ⇄ SBC payload bytes (the opaque `AudioData` body).
/// Frame-boundary semantics (how many PCM samples map to one `encode` call) are the
/// codec's concern; the transport hands whole PCM chunks and ships whatever bytes
/// come back. Both directions are infallible at this layer (a malformed SBC frame
/// on decode yields empty PCM, not an error, so one bad RX frame can't kill the loop).
pub trait SbcCodec: Send {
    fn encode(&self, pcm: &[u8]) -> Vec<u8>;
    fn decode(&self, sbc: &[u8]) -> Vec<u8>;
}

/// Identity passthrough — lets transport tests assert framing/pacing without a codec.
pub struct NullSbcCodec;
impl SbcCodec for NullSbcCodec {
    fn encode(&self, pcm: &[u8]) -> Vec<u8> { pcm.to_vec() }
    fn decode(&self, sbc: &[u8]) -> Vec<u8> { sbc.to_vec() }
}

/// Records encode inputs for assertions.
#[derive(Default)]
pub struct RecordingSbcCodec { encoded: Mutex<Vec<Vec<u8>>> }
impl RecordingSbcCodec {
    pub fn encoded_inputs(&self) -> Vec<Vec<u8>> { self.encoded.lock().unwrap().clone() }
}
impl SbcCodec for RecordingSbcCodec {
    fn encode(&self, pcm: &[u8]) -> Vec<u8> {
        self.encoded.lock().unwrap().push(pcm.to_vec());
        pcm.to_vec()
    }
    fn decode(&self, sbc: &[u8]) -> Vec<u8> { sbc.to_vec() }
}
```

- [ ] **Step 4: Run, verify pass.**
- [ ] **Step 5: Commit** — `feat(uvpro): SbcCodec trait seam + test fakes for audio transport (tuxlink-bcsy)`.

---

## Task 3: Audio-channel SDP resolution (`rfcomm.rs`)

The audio channel is a DIFFERENT RFCOMM service from SPP. The existing `parse_spp_channel` fixture already documents the UV-Pro advertising audio-gateway services: "Headset Audio Gateway" (`0x1112`) and "Handsfree Audio Gateway" (`0x111f`). Resolve the audio channel by targeting those classes. **The exact service the vendor app uses for SSTV audio is operator-confirmed via HCI snoop** — so return a RANKED candidate list and let the transport try them in order, logging which connected.

**Files:** Modify `src-tauri/src/winlink/ax25/rfcomm.rs` (single shared file → one task).

- [ ] **Step 1: Write the failing tests** (reuse the existing `UVPRO_RECORDS` fixture already in the file — it has Headset AG on ch2, Handsfree AG on ch3, SPP on ch1).

```rust
#[test]
fn parse_audio_channels_ranks_audio_gateways() {
    // From UVPRO_RECORDS: Headset AG (0x1112) ch2, Handsfree AG (0x111f) ch3.
    // Both are audio-channel candidates; SPP (ch1) is NOT.
    let chans = parse_audio_channels(UVPRO_RECORDS);
    assert!(chans.contains(&2));
    assert!(chans.contains(&3));
    assert!(!chans.contains(&1)); // SPP is not an audio candidate
}
#[test]
fn parse_audio_channels_empty_when_no_audio_service() {
    let records = "Service Name: SPP Dev\n  \"Serial Port\" (0x1101)\n    Channel: 1\n";
    assert!(parse_audio_channels(records).is_empty());
}
```

- [ ] **Step 2: Run, verify fail.**

- [ ] **Step 3: Implement** `parse_audio_channels` + `resolve_audio_channels`, mirroring the existing `parse_spp_channel`/`resolve_spp_channel` block structure (walk `Service Name:`-delimited blocks; flag blocks whose Service Class line contains `(0x1112)` or `(0x111f)`; collect their `Channel:`). Return `Vec<u8>` in advertisement order (caller tries in order). `resolve_audio_channels(mac)` shells `sdptool records <mac>` exactly like `resolve_spp_channel` and returns the parsed candidates (empty on query failure — caller surfaces a clear error, since there is no safe channel-1 fallback for audio).

```rust
/// Audio-gateway RFCOMM service classes the UV-Pro advertises (per the SDP record
/// fixture): Headset (0x1112) and Handsfree (0x111f) Audio Gateway. The vendor app's
/// SSTV audio rides one of these RFCOMM channels (NOT the SPP serial port). Which one
/// is operator-confirmed via HCI snoop; until then the transport tries candidates in
/// advertised order.
pub fn parse_audio_channels(records: &str) -> Vec<u8> {
    // ... block-walk identical in shape to parse_spp_channel, matching "(0x1112)"
    // or "(0x111f)" in the Service Class ID List, collecting each block's Channel.
}

/// Resolve audio-channel candidates from the radio's live SDP record. Empty if the
/// query fails or the radio advertises no audio gateway (caller errors out — unlike
/// SPP there is no sane channel-1 default for audio).
pub fn resolve_audio_channels(mac: &str) -> Vec<u8> {
    std::process::Command::new("sdptool")
        .args(["records", mac]).output().ok()
        .filter(|o| o.status.success())
        .map(|o| parse_audio_channels(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}
```

- [ ] **Step 4: Run, verify pass.**
- [ ] **Step 5: Commit** — `feat(rfcomm): resolve UV-Pro audio-gateway RFCOMM channel via SDP (tuxlink-bcsy)`.

> **Pitfall (do NOT skip):** Do not hardcode channel 2 or 3 — the SDP channel rotates per registration exactly like SPP (see `resolve_spp_channel`'s doc). Resolve fresh at connect time.

---

## Task 4: `AudioTransport` — TX/RX assembly + RADIO-1 abort (`transport.rs`)

The transport owns the audio `ByteLink`, an `SbcCodec`, and an `AudioDeframer`. TX: PCM chunk → `codec.encode` → `AudioMessage::Data` → `to_bytes` → `link.write`. Stop: send `AudioMessage::End`, then drop. Abort (RADIO-1): send `End` best-effort, then drop the link immediately (halts TX). RX: `link.read` → `deframer.push` → for each `Data`, `codec.decode` → emit PCM; `End` → signal end-of-image.

**Files:** Create `src-tauri/src/winlink/ax25/uvpro/audio/transport.rs`; uncomment `pub mod transport;`.

- [ ] **Step 1: Write the failing tests** (use an in-memory fake `ByteLink` — a `Vec<u8>` capture for TX and a scripted reader for RX, mirroring the `Driver` tests' in-memory fake; and `NullSbcCodec`/`RecordingSbcCodec`).

```rust
#[test]
fn send_pcm_encodes_frames_and_writes_audiodata() {
    let sink = SharedSink::default();              // test ByteLink capturing writes
    let codec = Arc::new(RecordingSbcCodec::default());
    let mut tx = AudioTransport::new(Box::new(sink.link()), codec.clone(), KeyingMode::Implicit);
    tx.send_pcm(&[0x11, 0x22]).unwrap();
    // One AudioData frame containing the codec output ([0x11,0x22] via Null/Recording).
    assert_eq!(sink.written(), AudioMessage::Data(vec![0x11, 0x22]).to_bytes());
    assert_eq!(codec.encoded_inputs(), vec![vec![0x11u8, 0x22]]);
}
#[test]
fn finish_sends_audio_end() {
    let sink = SharedSink::default();
    let mut tx = AudioTransport::new(Box::new(sink.link()), Arc::new(NullSbcCodec), KeyingMode::Implicit);
    tx.finish().unwrap();
    assert_eq!(sink.written(), AudioMessage::End.to_bytes());
}
#[test]
fn abort_sends_end_then_drops_link() {
    // RADIO-1 working abort: End is emitted (best-effort de-key) and the link is
    // released so no further AudioData can be written.
    let sink = SharedSink::default();
    let mut tx = AudioTransport::new(Box::new(sink.link()), Arc::new(NullSbcCodec), KeyingMode::Implicit);
    tx.abort();
    assert_eq!(sink.written(), AudioMessage::End.to_bytes());
    assert!(tx.send_pcm(&[0x00]).is_err()); // link gone → cannot transmit after abort
}
#[test]
fn rx_pump_decodes_audiodata_to_pcm_until_end() {
    let mut script = AudioMessage::Data(vec![0xDE, 0xAD]).to_bytes();
    script.extend(AudioMessage::End.to_bytes());
    let link = ScriptedReader::new(script);        // test ByteLink replaying bytes
    let mut rx = AudioTransport::new(Box::new(link), Arc::new(NullSbcCodec), KeyingMode::Implicit);
    let mut pcm = Vec::new();
    let ended = rx.pump_rx(&mut |chunk| pcm.extend_from_slice(chunk));
    assert_eq!(pcm, vec![0xDE, 0xAD]);
    assert!(ended); // AudioEnd observed
}
```

- [ ] **Step 2: Run, verify fail.**

- [ ] **Step 3: Implement.** Key points the implementation MUST honor:
  - `KeyingMode { Implicit, Explicit }`. Implicit (default) does NO `c1` keying — opening the channel + first `AudioData` keys TX (benlink-confirmed). Explicit would send `c1.TX_AUDIO` over the GAIA channel before the first frame and `c1.TX_AUDIO_STOP` on finish — but the transport does NOT own the GAIA link, so Explicit takes an injected callback `Box<dyn Fn(keying::AudioKey) -> Result<(), String>>` wired by the session (Task 7). In THIS task, Explicit-mode keying is exercised only via a recording callback fake; default tests use Implicit.
  - `send_pcm` returns `Err` if the link was dropped (post-abort/finish). After `finish()` or `abort()`, the link is `None`.
  - `abort()` is infallible (best-effort `End`, swallow write errors, then drop). It must NOT panic. This is the RADIO-1 abort path.
  - No internal airtime cap / timer (no tuxlink-added safeguard — see header).
  - `pump_rx(&mut on_pcm)` does ONE bounded `read` (the `RfcommSocket` read timeout governs), feeds the deframer, decodes each `Data` to PCM via the callback, returns `true` when an `End` is seen. (The session's RX loop calls `pump_rx` repeatedly; keeping one read per call makes it abort-observable, mirroring `native_driver.rs`'s 50 ms poll loop.)

- [ ] **Step 4: Run, verify pass.**
- [ ] **Step 5: Commit** — `feat(uvpro): AudioTransport TX/RX assembly with RADIO-1 abort (tuxlink-bcsy)`.

---

## Task 5: `c1` keying opcodes over GAIA (`keying.rs`)

Built but DEFAULT-OFF (Implicit keying). Provides the GAIA command bytes for `c1.TX_AUDIO`/`TX_AUDIO_STOP`/`RX_AUDIO`/`RX_AUDIO_STOP` so the session can flip to Explicit keying IF the operator HCI snoop shows the vendor app keys via GAIA.

**Files:** Create `src-tauri/src/winlink/ax25/uvpro/audio/keying.rs`; uncomment `pub mod keying;`.

> **GROUNDING GAP — resolve at build time, do NOT guess:** The `c1` enum values are known (`UNKNOWN=0, TX_AUDIO=1, TX_AUDIO_STOP=2, RX_AUDIO=3, RX_AUDIO_STOP=4, SET_SIGN_DATA=5`, from `v4/c1.java`), but the GAIA **command_group + command_id** that carries them (the `W0()` wrapper in `v4/g2.java:420`) is NOT yet extracted. Before implementing, grep the decompile: `dev/scratch/benshi-re/apk/jadx-out/sources/v4/g2.java` around the `W0(` calls and the class's command header constants, and cross-check `message.rs`'s `header()` + `command_group` (BASIC=2) convention. If the group/id cannot be determined from the decompile, mark this task BLOCKED on the HCI snoop and ship Tasks 1–4,6,7 with Implicit-only keying (the benlink-proven path) — Explicit keying is not on the critical path for first on-air.

- [ ] **Step 1:** Write a golden-vector test for each opcode's GAIA-wrapped bytes ONCE the group/id is extracted (format: `header(group, id, body=[opcode])` then `gaia_wrap`). If BLOCKED, write a `#[ignore]`'d placeholder test documenting the gap and skip to Task 6.
- [ ] **Step 2–4:** TDD as usual once unblocked.
- [ ] **Step 5: Commit** — `feat(uvpro): c1 audio keying opcodes (default-off, snoop-gated) (tuxlink-bcsy)`.

---

## Task 6: Wire `AudioTransport` into `UvproSession` (`session.rs`)

**Sequence LAST** — modifies the shared `session.rs`. Depends on Tasks 1–4 (and 5 if unblocked).

**Files:** Modify `src-tauri/src/winlink/ax25/uvpro/session.rs`.

- [ ] **Step 1: Write the failing test** at the `UvproSession` level using a fake audio link factory (do NOT require real Bluetooth). Assert: `open_audio` resolves a channel and constructs a transport; `abort_audio` drops it; the GAIA control link and the audio link are independent (aborting audio does NOT drop the control session).
- [ ] **Step 2: Run, verify fail.**
- [ ] **Step 3: Implement** `open_audio(&self) -> Result<(), UvproError>`:
  - Resolve via `resolve_audio_channels(mac)`; error `UvproError::Protocol("no audio-gateway RFCOMM service advertised")` if empty.
  - Connect a SECOND `RfcommSocket` (try candidates in order; first success wins; log the channel chosen — this is the snoop-confirm evidence). Read/write timeouts mirror the control link (`READ_POLL` / `WRITE_TIMEOUT`).
  - Construct `AudioTransport::new(audio_link, codec, KeyingMode::Implicit)`. The codec is the real `SbcCodec` once the sibling sub-project lands; until then, gate `open_audio` behind a clear `UvproError::Protocol("SBC codec not yet available")` OR accept an injected codec so this task can land + be tested ahead of the codec. **Prefer injection** (constructor takes `Arc<dyn SbcCodec>`), so the transport+session wiring is testable and mergeable before the codec exists.
  - Store the transport behind the session mutex; `abort_audio` calls `transport.abort()` and clears it.
  - **Concurrency note (must address):** the audio socket is a separate fd from the GAIA control socket — they do NOT share the `Driver`'s single-reader serialization. The `UvproLinkLock` guards ONE Bluetooth host connection; opening a 2nd RFCOMM channel to the SAME radio over the SAME ACL link is allowed (multiplexed), but confirm the lock model: the audio channel should be permitted while the control session holds the lock (same radio, same operator intent), NOT treated as a competing host. Document the decision inline; the Codex adrev will attack this.
- [ ] **Step 4: Run, verify pass.**
- [ ] **Step 5: Commit** — `feat(uvpro): open audio channel alongside GAIA control session (tuxlink-bcsy)`.

---

## Task 7: Module exports + integration smoke (compile-level)

- [ ] Ensure `audio/mod.rs` exposes `framing`, `codec`, `transport`, `keying` (if unblocked), and re-exports `AudioTransport`, `AudioMessage`, `KeyingMode`, `SbcCodec`.
- [ ] Run the full crate test subset for the uvpro module: `cargo test --manifest-path src-tauri/Cargo.toml uvpro::` → green (or push + CI).
- [ ] Run `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` for the touched files (CI runs `--all-targets`; local scoped runs miss later-target lints — re-run till exit 0). See memory `scoped_vitest_misses_contract_tests` analog for Rust.
- [ ] Commit any export/lint fixups.

---

## After every logical group of tasks

After Tasks 1–2 (pure units), after Tasks 3–5 (channel + keying), and after Tasks 6–7 (integration): review the batch from multiple perspectives. Minimum three review rounds; if substantive issues remain at round three, keep going. Update the private journal, then continue.

## Definition of done (this plan = component 1 only)

This plan delivers the **transport foundation**, NOT the shippable feature. The SSTV feature is "done" only after: (a) the SBC codec sub-project lands and is injected, (b) the SSTV codec (component 2) lands, (c) the inline image UI (component 3) lands, and (d) the **`wire-walk` gate** passes against operator-supplied flows (e.g. "field op on stock app sends an image → it appears inline in tuxlink APRS chat"; "operator attaches an image → field op's stock app renders it"). Per the wire-walk gate in CLAUDE.md, do NOT claim the feature shipped before that trace. The operator HCI snoop (audio channel #/UUID + keying mode confirmation) is the gate before the first on-air run.

## Build/verify note (this Pi)

Per the operator memory `no_cold_cargo_on_contended_pi`: cold `cargo build`/`test` on this contended Pi often does not finish. If a local `cargo test` stalls, do NOT burn sessions on it — push the branch (draft PR ok) and let GitHub CI (amd64+arm64) compile and run the tests. The framing/codec/SDP tests are pure and fast once compiled; the bottleneck is the cold build, not the tests.

## Sibling sub-projects (separate bd issues + plans — NOT in this plan)

1. **SBC codec** (pure-Rust encoder port + decoder, golden-vector tested against benlink) — implements `SbcCodec`. Risk: no off-the-shelf pure-Rust SBC encoder exists (crates.io has `mini_sbc` = decoder-only, `libsbc` = C-FFI). Decision pending operator confirm at review: port encoder (pure-Rust, matches repo ethos) vs `libsbc` FFI fallback.
2. **SSTV codec** (component 2) — PCM↔image, HTCommander C# port (Robot36 + a PD mode encode; STFT decode). Reference: `dev/scratch/benshi-re/HTCommander/src/SSTV/`.
3. **Inline image UI** (component 3) — composer attach + inbound auto-decode thumbnail in `AprsChatPanel`.
