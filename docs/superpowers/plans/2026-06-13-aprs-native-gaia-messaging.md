# APRS Messaging over Native Benshi GAIA — Implementation Plan (tuxlink-7my9)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the control-only nx95 UV-Pro backend so the **one** native GAIA Bluetooth connection also carries APRS chat — sending APRS frames as `HT_SEND_DATA` fragments and receiving them via `DATA_RXD` events — realizing the unified model where control + chat share a single connection (no KISS, no mode-switch on the UV-Pro).

**Architecture:** Frame-level integration. The shipped APRS engine's codec + TxQueue + ACK/timeout/dedup (`src-tauri/src/winlink/aprs/`) are reused **unchanged**; they already deal in raw AX.25 frame bytes. The net-new, correctness-critical units are pure functions: a `TncDataFragment` codec, an AX.25→fragments **fragmenter**, and a fragments→AX.25 **reassembler**. `UvproSession` becomes the owner of the native APRS data path: `send_aprs_frame()` fragments + sends; the inbound event loop's currently-ignored `DATA_RXD` case feeds the reassembler, which pushes completed AX.25 frames onto an `mpsc` channel a native driver drains into the engine. KISS stays the path for generic TNCs; native is the UV-Pro path.

**Tech Stack:** Rust (`src-tauri`), the existing `uvpro::bits` `BitWriter`/`BitReader`, `std::sync::mpsc`, golden-byte-vector tests (the nx95 pattern). Protocol is vendor-verified (decompile of `com.benshikj.ht.btech.ham`, see `dev/scratch/benshi-re/DECOMPILE-FINDINGS.md`) and cross-checked against benlink.

**Worktree:** `worktrees/bd-tuxlink-2f2n-aprs-tactical-chat` (branch `bd-tuxlink-2f2n/aprs-tactical-chat`, PR #642). All paths below are relative to it.

**RADIO-1 / ADR-0018:** write + test with mocks/golden vectors; the agent never transmits. Correctness bar: working abort (drop the link), no command-storm. On-air validation is operator-only and blocked by tuxlink-9ky.

---

## Protocol reference (vendor-verified)

**GAIA Message header** (`message.rs` `header()`): `write_uint(GROUP_BASIC=2, 16)` · `write_bool(is_reply)` · `write_uint(command, 15)`, MSB-first.

**`HT_SEND_DATA` command** = opcode **31** (basic group). Request body = one `TncDataFragment`. Reply body = 1-byte `reply_status` (0 = SUCCESS).

**`DATA_RXD`** = `EventType` **2**, arrives inside a `CMD_EVENT_NOTIFICATION` (9) frame: `body[0] = event_type(2)`, `body[1..] =` the `TncDataFragment`.

**`TncDataFragment` wire layout** (MSB-first, from benlink `common.py` + decompile):
- bit 7: `is_final_fragment` (bool)
- bit 6: `with_channel_id` (bool)
- bits 5–0: `fragment_id` (u6, 0–63)
- then: `data` bytes (the AX.25 frame slice)
- then, **only if `with_channel_id`**: a trailing `channel_id` byte (u8)

**Fragmentation:** one AX.25 frame is split across fragments of ≤ **53** data bytes each (the firmware's `HT_SEND_DATA` max body), `fragment_id` incrementing from 0, `is_final_fragment=true` on the last. tuxlink emits with `with_channel_id=false` (the radio routes on its active channel); the reassembler must still parse the trailing `channel_id` when an inbound fragment sets the flag.

---

## File structure

| File | Responsibility | New/Modify |
|---|---|---|
| `src-tauri/src/winlink/ax25/uvpro/tncdata.rs` | Pure `TncDataFragment` codec + fragmenter + reassembler | **Create** |
| `src-tauri/src/winlink/ax25/uvpro/message.rs` | Add `CMD_HT_SEND_DATA`, `encode_ht_send_data`, decode `SendDataReply` + `DataReceived` event | Modify |
| `src-tauri/src/winlink/ax25/uvpro/mod.rs` | Register `tncdata` module | Modify |
| `src-tauri/src/winlink/ax25/uvpro/session.rs` | `send_aprs_frame()`, `DATA_RXD` reassembly → inbound `mpsc`, subscribe `DataRxd` in hydrate | Modify |
| `src-tauri/src/winlink/aprs/engine.rs` | `handle_inbound_frame()` (non-KISS sibling), expose due **raw** AX.25 frames | Modify |
| `src-tauri/src/winlink/aprs/native_driver.rs` | Drive the engine from the session's APRS channel (the native analogue of the KISS `run()` loop) | **Create** |
| `src-tauri/src/ui_commands.rs` | Capability-gated transport select: UV-Pro → native APRS path | Modify |

The risky, novel code is `tncdata.rs` + the `message.rs` opcode (Tasks 1–4): fully specified + golden-vector TDD + a Codex adversarial round (Task 9). Tasks 5–8 are wiring against cited seams.

---

## Task 1: `TncDataFragment` struct + single-fragment codec

**Files:**
- Create: `src-tauri/src/winlink/ax25/uvpro/tncdata.rs`
- Modify: `src-tauri/src/winlink/ax25/uvpro/mod.rs`

- [ ] **Step 1: Register the module**

In `mod.rs`, add alongside the existing `mod` lines (e.g. after `mod message;`):
```rust
mod tncdata;
```

- [ ] **Step 2: Write the failing test** (`tncdata.rs`, append a `#[cfg(test)]` module)

```rust
use super::bits::{BitReader, BitWriter};

/// One Benshi TNC data fragment: a slice of an AX.25 frame plus the
/// reassembly header. `channel_id` is `Some` iff the wire flag was set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TncDataFragment {
    pub is_final: bool,
    pub fragment_id: u8, // 0..=63
    pub channel_id: Option<u8>,
    pub data: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Golden vector: not-final, frag 0, no channel id, data = [0xDE,0xAD].
    // byte0 = 0b00_000000 = 0x00, then data.
    #[test]
    fn encode_no_channel_id_golden() {
        let f = TncDataFragment { is_final: false, fragment_id: 0, channel_id: None, data: vec![0xDE, 0xAD] };
        assert_eq!(f.encode_body(), vec![0x00, 0xDE, 0xAD]);
    }

    // Golden vector: final, frag 5, channel id 7, data = [0x01].
    // byte0 = 0b11_000101 = 0xC5, then data 0x01, then channel_id 0x07.
    #[test]
    fn encode_with_channel_id_golden() {
        let f = TncDataFragment { is_final: true, fragment_id: 5, channel_id: Some(7), data: vec![0x01] };
        assert_eq!(f.encode_body(), vec![0xC5, 0x01, 0x07]);
    }

    #[test]
    fn decode_roundtrips_both_golden_vectors() {
        for f in [
            TncDataFragment { is_final: false, fragment_id: 0, channel_id: None, data: vec![0xDE, 0xAD] },
            TncDataFragment { is_final: true, fragment_id: 5, channel_id: Some(7), data: vec![0x01] },
        ] {
            assert_eq!(TncDataFragment::decode_body(&f.encode_body()), Some(f));
        }
    }

    #[test]
    fn decode_rejects_empty() {
        assert_eq!(TncDataFragment::decode_body(&[]), None);
    }
}
```

- [ ] **Step 3: Run it, verify it fails**

Run: `cargo test -p tuxlink --manifest-path src-tauri/Cargo.toml uvpro::tncdata -- --nocapture`
Expected: FAIL — `encode_body`/`decode_body` not found.

- [ ] **Step 4: Implement the codec** (above the test module in `tncdata.rs`)

```rust
impl TncDataFragment {
    /// Encode the fragment body (the bytes that follow the GAIA command header
    /// in an `HT_SEND_DATA` request, and that follow `event_type` in a
    /// `DATA_RXD` event). MSB-first, matching the firmware bitfield.
    pub fn encode_body(&self) -> Vec<u8> {
        let mut w = BitWriter::new();
        w.write_bool(self.is_final);
        w.write_bool(self.channel_id.is_some());
        w.write_uint((self.fragment_id & 0x3f) as u64, 6);
        w.write_bytes(&self.data);
        if let Some(cid) = self.channel_id {
            w.write_uint(cid as u64, 8);
        }
        w.into_bytes()
    }

    /// Decode a fragment body. Returns `None` for an empty body (no header byte).
    /// The trailing channel-id byte (when flagged) is split off the data tail.
    pub fn decode_body(body: &[u8]) -> Option<Self> {
        if body.is_empty() {
            return None;
        }
        let mut r = BitReader::new(body);
        let is_final = r.read_bool();
        let with_channel_id = r.read_bool();
        let fragment_id = r.read_uint(6) as u8;
        // Remaining whole bytes after the 1-byte header.
        let rest = &body[1..];
        let (data, channel_id) = if with_channel_id {
            if rest.is_empty() {
                return None; // flag set but no channel-id byte
            }
            let (d, cid) = rest.split_at(rest.len() - 1);
            (d.to_vec(), Some(cid[0]))
        } else {
            (rest.to_vec(), None)
        };
        Some(TncDataFragment { is_final, fragment_id, channel_id, data })
    }
}
```

- [ ] **Step 5: Run tests, verify pass**

Run: `cargo test -p tuxlink --manifest-path src-tauri/Cargo.toml uvpro::tncdata`
Expected: PASS (4 tests).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/winlink/ax25/uvpro/tncdata.rs src-tauri/src/winlink/ax25/uvpro/mod.rs
git commit -m "feat(uvpro): TncDataFragment wire codec (golden vectors)

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Fragmenter — AX.25 frame → ordered fragments

**Files:** Modify `src-tauri/src/winlink/ax25/uvpro/tncdata.rs`

- [ ] **Step 1: Write the failing test** (add to the test module)

```rust
const MAX_FRAGMENT_DATA: usize = 53;

#[test]
fn fragment_short_frame_is_single_final() {
    let frame = vec![0xAA; 10];
    let frags = fragment_ax25(&frame);
    assert_eq!(frags.len(), 1);
    assert!(frags[0].is_final);
    assert_eq!(frags[0].fragment_id, 0);
    assert_eq!(frags[0].channel_id, None);
    assert_eq!(frags[0].data, frame);
}

#[test]
fn fragment_long_frame_splits_at_53_with_incrementing_ids() {
    let frame = vec![0xBB; 53 * 2 + 7]; // 113 bytes -> 3 fragments
    let frags = fragment_ax25(&frame);
    assert_eq!(frags.len(), 3);
    assert_eq!(frags.iter().map(|f| f.data.len()).collect::<Vec<_>>(), vec![53, 53, 7]);
    assert_eq!(frags.iter().map(|f| f.fragment_id).collect::<Vec<_>>(), vec![0, 1, 2]);
    assert_eq!(frags.iter().map(|f| f.is_final).collect::<Vec<_>>(), vec![false, false, true]);
    // round-trip: concatenated data == original
    let joined: Vec<u8> = frags.iter().flat_map(|f| f.data.clone()).collect();
    assert_eq!(joined, frame);
}

#[test]
fn fragment_exact_multiple_last_is_final() {
    let frame = vec![0xCC; 53 * 2]; // exactly 2 fragments
    let frags = fragment_ax25(&frame);
    assert_eq!(frags.len(), 2);
    assert!(frags[1].is_final);
}
```

- [ ] **Step 2: Run, verify FAIL** (`fragment_ax25` undefined).
Run: `cargo test -p tuxlink --manifest-path src-tauri/Cargo.toml uvpro::tncdata`

- [ ] **Step 3: Implement** (above the test module)

```rust
/// Max AX.25 data bytes per `HT_SEND_DATA` fragment (firmware limit).
pub const MAX_FRAGMENT_DATA: usize = 53;

/// Split a raw AX.25 frame into ordered fragments for `HT_SEND_DATA`.
/// `fragment_id` increments from 0; the last fragment is `is_final`. tuxlink
/// emits with no channel id (the radio uses its active channel). An empty frame
/// yields one empty final fragment (degenerate, but well-formed).
pub fn fragment_ax25(frame: &[u8]) -> Vec<TncDataFragment> {
    if frame.is_empty() {
        return vec![TncDataFragment { is_final: true, fragment_id: 0, channel_id: None, data: Vec::new() }];
    }
    let chunks: Vec<&[u8]> = frame.chunks(MAX_FRAGMENT_DATA).collect();
    let last = chunks.len() - 1;
    chunks
        .into_iter()
        .enumerate()
        .map(|(i, chunk)| TncDataFragment {
            is_final: i == last,
            fragment_id: (i as u8) & 0x3f,
            channel_id: None,
            data: chunk.to_vec(),
        })
        .collect()
}
```

- [ ] **Step 4: Run, verify PASS.** **Step 5: Commit**
```bash
git add src-tauri/src/winlink/ax25/uvpro/tncdata.rs
git commit -m "feat(uvpro): AX.25 -> TncDataFragment fragmenter (golden vectors)

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Reassembler — fragments → completed AX.25 frame

**Files:** Modify `src-tauri/src/winlink/ax25/uvpro/tncdata.rs`

The reassembler is stateful but pure (no I/O). It buffers `data` across fragments and emits the joined AX.25 frame when `is_final` arrives. Defensive against the real RF failure modes: a fresh `fragment_id==0` restarts the buffer (a dropped final), and a lone `is_final` fragment emits immediately.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn reassemble_single_final_emits_immediately() {
    let mut ra = Reassembler::new();
    let out = ra.push(&TncDataFragment { is_final: true, fragment_id: 0, channel_id: None, data: vec![1, 2, 3] });
    assert_eq!(out, Some(vec![1, 2, 3]));
}

#[test]
fn reassemble_multi_joins_in_order() {
    let mut ra = Reassembler::new();
    assert_eq!(ra.push(&TncDataFragment { is_final: false, fragment_id: 0, channel_id: None, data: vec![1, 2] }), None);
    assert_eq!(ra.push(&TncDataFragment { is_final: false, fragment_id: 1, channel_id: None, data: vec![3, 4] }), None);
    assert_eq!(ra.push(&TncDataFragment { is_final: true, fragment_id: 2, channel_id: None, data: vec![5] }), Some(vec![1, 2, 3, 4, 5]));
}

#[test]
fn reassemble_restart_on_new_zero_discards_partial() {
    let mut ra = Reassembler::new();
    ra.push(&TncDataFragment { is_final: false, fragment_id: 0, channel_id: None, data: vec![9, 9] }); // partial, then dropped final
    // a fresh frame begins
    assert_eq!(ra.push(&TncDataFragment { is_final: false, fragment_id: 0, channel_id: None, data: vec![1] }), None);
    assert_eq!(ra.push(&TncDataFragment { is_final: true, fragment_id: 1, channel_id: None, data: vec![2] }), Some(vec![1, 2]));
}

#[test]
fn reassemble_out_of_sequence_resets_and_drops() {
    let mut ra = Reassembler::new();
    ra.push(&TncDataFragment { is_final: false, fragment_id: 0, channel_id: None, data: vec![1] });
    // unexpected id 5 (gap) -> drop the partial, no emit
    assert_eq!(ra.push(&TncDataFragment { is_final: true, fragment_id: 5, channel_id: None, data: vec![2] }), None);
}
```

- [ ] **Step 2: Run, verify FAIL.**

- [ ] **Step 3: Implement**

```rust
/// Stateful reassembler for inbound `DATA_RXD` fragments. Pure (no I/O); fed by
/// the session's event loop, emits a completed AX.25 frame on the final fragment.
#[derive(Debug, Default)]
pub struct Reassembler {
    buf: Vec<u8>,
    next_id: u8,
    active: bool,
}

impl Reassembler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one fragment. Returns `Some(frame)` when a frame completes.
    /// Resilience rules (RF is lossy): `fragment_id==0` always (re)starts a
    /// frame; a non-contiguous id drops the partial and returns `None`.
    pub fn push(&mut self, f: &TncDataFragment) -> Option<Vec<u8>> {
        if f.fragment_id == 0 {
            self.buf.clear();
            self.buf.extend_from_slice(&f.data);
            self.active = true;
            self.next_id = 1;
        } else if self.active && f.fragment_id == self.next_id {
            self.buf.extend_from_slice(&f.data);
            self.next_id = self.next_id.wrapping_add(1);
        } else {
            // gap / stray continuation — discard partial, wait for a fresh id 0
            self.buf.clear();
            self.active = false;
            return None;
        }
        if f.is_final {
            self.active = false;
            return Some(std::mem::take(&mut self.buf));
        }
        None
    }
}
```

- [ ] **Step 4: Run, verify PASS. Step 5: Commit** (`feat(uvpro): TncDataFragment reassembler (golden vectors + RF-loss cases)`, trailers as above).

---

## Task 4: `message.rs` — `HT_SEND_DATA` encode + `DATA_RXD`/reply decode

**Files:** Modify `src-tauri/src/winlink/ax25/uvpro/message.rs`

Read first: `message.rs:14–25` (the `CMD_*` consts), `:39–45` (`header()`), `:121–128` (`Event`/`Frame` enums), `:206–279` (`decode_frame`). Match local style.

- [ ] **Step 1: Write failing tests** (in `message.rs`'s test module)

```rust
#[test]
fn encode_ht_send_data_golden() {
    // header: group 2 (16b) + is_reply=false (1b) + cmd 31 (15b) = bytes
    //   0x00 0x02  then  0b0_000000000011111 packed after the bool...
    // Assert against the BYTES the existing header() produces for cmd=31, then
    // the fragment body (frag 0, not-final, no channel id, data [0x41]).
    let frag = super::tncdata::TncDataFragment { is_final: false, fragment_id: 0, channel_id: None, data: vec![0x41] };
    let got = encode_ht_send_data(&frag);
    let mut expected = header(31, false).into_bytes();
    expected.extend_from_slice(&frag.encode_body());
    assert_eq!(got, expected);
}

#[test]
fn decode_ht_send_data_reply_success() {
    let mut w = header(31, true);
    w.write_uint(0, 8); // reply_status = SUCCESS
    assert!(matches!(decode_frame(&w.into_bytes()), Frame::SendDataReply { reply_status: 0 }));
}

#[test]
fn decode_data_rxd_event_yields_fragment() {
    // CMD_EVENT_NOTIFICATION(9), event_type=2 (DataRxd), then fragment body.
    let frag = super::tncdata::TncDataFragment { is_final: true, fragment_id: 0, channel_id: None, data: vec![0x99] };
    let mut w = header(9, false);
    w.write_uint(2, 8); // EventType::DataRxd
    w.write_bytes(&frag.encode_body());
    match decode_frame(&w.into_bytes()) {
        Frame::Event(Event::DataReceived { fragment }) => assert_eq!(fragment, frag),
        other => panic!("expected DataReceived, got {other:?}"),
    }
}
```

- [ ] **Step 2: Run, verify FAIL** (`encode_ht_send_data`, `Frame::SendDataReply`, `Event::DataReceived` undefined).

- [ ] **Step 3: Implement**

Add the opcode const near the others (after `CMD_GET_HT_STATUS`):
```rust
const CMD_HT_SEND_DATA: u64 = 31;
```
Add the encoder (in the request-encoders section):
```rust
/// Encode an `HT_SEND_DATA` request carrying one TNC fragment.
pub fn encode_ht_send_data(frag: &super::tncdata::TncDataFragment) -> Vec<u8> {
    let mut w = header(CMD_HT_SEND_DATA, false);
    w.write_bytes(&frag.encode_body());
    w.into_bytes()
}
```
Extend the `Frame` enum with `SendDataReply { reply_status: u8 }` and the `Event` enum with `DataReceived { fragment: super::tncdata::TncDataFragment }`.
In `decode_frame`, add a reply arm:
```rust
(true, CMD_HT_SEND_DATA) => Frame::SendDataReply {
    reply_status: body.first().copied().unwrap_or(0xff),
},
```
In the `CMD_EVENT_NOTIFICATION` `match event_type { … }`, replace the `DataRxd` fall-through with:
```rust
x if x == EventType::DataRxd as u8 => match super::tncdata::TncDataFragment::decode_body(&body[1..]) {
    Some(fragment) => Frame::Event(Event::DataReceived { fragment }),
    None => Frame::Event(Event::OtherIgnored { event_type }),
},
```

- [ ] **Step 4: Run, verify PASS. Step 5: Commit** (`feat(uvpro): HT_SEND_DATA encode + DATA_RXD/reply decode`, trailers).

---

## Task 5: `UvproSession` — native APRS send + inbound channel

**Files:** Modify `src-tauri/src/winlink/ax25/uvpro/session.rs`

Read first: `session.rs:85–151` (`Driver`, `send_and_wait`, `send_no_reply`), `:180–200` (`apply_event`), `hydrate*` (notification subscription via `encode_register_notification`).

Contract to add:
- `Driver` gains a `reassembler: Reassembler` field and an inbound sender `aprs_tx: Option<std::sync::mpsc::Sender<Vec<u8>>>`.
- `apply_event` gains `Event::DataReceived { fragment }` → `if let Some(frame) = self.reassembler.push(&fragment) { if let Some(tx) = &self.aprs_tx { let _ = tx.send(frame); } }`.
- `Driver` (and the `UvproSession` wrapper) expose `send_aprs_frame(&mut self, ax25: &[u8]) -> Result<(), UvproError>` that does, per fragment: `let req = encode_ht_send_data(&frag); match self.send_and_wait(&req, COMMAND_TIMEOUT)? { Frame::SendDataReply { reply_status: 0 } => continue, Frame::SendDataReply { reply_status } => return Err(UvproError::RadioRejected(reply_status)), _ => continue }` — serialized through the same socket the control commands use.
- `hydrate` subscribes to `DataRxd`: add `self.send_no_reply(&encode_register_notification(EventType::DataRxd))?;` next to the existing notification subscriptions.
- A constructor/setter wires the `aprs_tx` channel at connect time; `UvproSession` exposes `take_aprs_receiver() -> Option<Receiver<Vec<u8>>>` for the native driver.

- [ ] **Step 1: Failing test** — a mock `ByteLink` that, when written an `HT_SEND_DATA` request, replies with a `SendDataReply{0}`; assert `send_aprs_frame(&[0xAA;120])` writes **3** `HT_SEND_DATA` requests (fragmented) and returns `Ok`. Reuse the nx95 mock-link test harness in `session.rs`'s test module (read it first; mirror its shape).
- [ ] **Step 2:** Run → FAIL. **Step 3:** Implement the contract above. **Step 4:** Run → PASS.
- [ ] **Step 5: Failing test (RX):** feed the driver (via the mock link's read side) an `EVENT_NOTIFICATION`/`DATA_RXD` carrying a 2-fragment frame; assert the joined AX.25 frame arrives on the `aprs` receiver. **Implement** (the `apply_event` arm). Run → PASS.
- [ ] **Step 6: Commit** (`feat(uvpro): native APRS send_aprs_frame + DATA_RXD reassembly channel`, trailers).

---

## Task 6: APRS engine — frame-level inbound + raw outbound

**Files:** Modify `src-tauri/src/winlink/aprs/engine.rs`

Read first: `engine.rs:118–210` (`handle_inbound_bytes`), `:214–254` (`enqueue_send`, `tick`), `:408–458` (`run`).

The engine already builds the AX.25 frame in `enqueue_send` before KISS-wrapping, and decodes AX.25 in `handle_inbound_bytes` after KISS-deframing. Add the non-KISS siblings so native skips KISS entirely:

- [ ] **Step 1: Failing test** — `handle_inbound_frame(&mut self, ax25: &[u8], now_ms)` routes a raw AX.25 UI frame through `Frame::decode → extract_inbound → parse_info` (same as `handle_inbound_bytes` does *after* its `KissDecoder`), returning any auto-ACK as **raw AX.25** frames (not KISS-wrapped). Assert an inbound addressed message emits the chat event and yields one raw-AX.25 ACK frame.
- [ ] **Step 2:** Run → FAIL. **Step 3:** Implement `handle_inbound_frame` by factoring the post-KISS body of `handle_inbound_bytes` into a shared `fn ingest_ax25(&mut self, body, now_ms) -> Vec<Vec<u8>>` that returns **raw** AX.25 frames; `handle_inbound_bytes` KISS-wraps that return for its callers, `handle_inbound_frame` returns it raw. **Step 4:** Run → PASS.
- [ ] **Step 5: Failing test** — `tick_frames(&mut self, now_ms) -> Vec<Vec<u8>>` returns due TX frames as **raw AX.25** (the same frames `tick()` returns, minus the KISS wrap). Assert a queued send yields the raw AX.25 frame at its due time and the existing `tick()` still yields the KISS-wrapped form. **Implement** by splitting the AX.25-frame production from the KISS-wrap in the TxQueue path (store/produce raw frames; `tick()` wraps, `tick_frames()` doesn't). **Step 6:** Run → PASS. **Commit** (`feat(aprs): frame-level inbound/outbound seam for non-KISS transports`, trailers).

---

## Task 7: Native driver — bridge session ⇄ engine

**Files:** Create `src-tauri/src/winlink/aprs/native_driver.rs`; register in `aprs/mod.rs`.

The KISS analogue is `engine.rs:408–458 run()`. The native driver is the same loop without a socket: drain the session's APRS receiver → `engine.handle_inbound_frame()` → push returned ACK frames + `engine.tick_frames()` due frames via `session.send_aprs_frame()`; abort = drop the session link.

- [ ] **Step 1: Failing test** — with a mock session pair (an in-memory APRS `Sender`/`Receiver` + a recording `send_aprs_frame`), drive an inbound frame and assert (a) the chat event fired and (b) the auto-ACK was sent via `send_aprs_frame`. **Step 2:** FAIL. **Step 3:** Implement the loop + a `stop`/abort flag (drop the link). **Step 4:** PASS. **Commit** (trailers).

---

## Task 8: Capability-gated transport selection (Tauri wiring)

**Files:** Modify `src-tauri/src/ui_commands.rs`; read `engine.rs:321–351 AprsState::start()` (the hardcoded `KissLinkConfig::Bluetooth` path) and the `aprs_listen_start` command.

- [ ] **Step 1:** When the configured packet transport is a UV-Pro/Benshi profile, `aprs_listen_start` brings up the **native** path (open/reuse the `UvproSession`, start the `native_driver`) instead of the KISS `run()` loop; a generic Classic-SPP TNC keeps the KISS path. The frontend's `controlStrip` (tuxlink-ve3j) reuses the *same* `UvproSession` — one connection, control + chat. Add the capability flag to the config/transport resolution; gate the branch on it.
- [ ] **Step 2:** Test the selection function purely (config → transport kind) with a UV-Pro config → `Native`, a Mobilinkd config → `Kiss`. **Implement. Test. Commit** (trailers).

> **Note (no silent cap):** this task connects native APRS to the listener; the always-live control surface itself is **tuxlink-ve3j** (depends on this). On-air round-trip is operator-only (tuxlink-9ky).

---

## Task 9: Codex adversarial round (correctness gate)

nx95 shipped without its cross-provider adrev (`tuxlink-bv0b`); do **not** repeat that for the messaging half. Per CLAUDE.md's Codex recipe:

- [ ] **Step 1:** Run a directed Codex review of the fragment codec + fragmenter + reassembler + `HT_SEND_DATA`/`DATA_RXD` framing against the diff:
```bash
cat > /tmp/codex-prompt.txt <<'EOF'
Adversarial review of the native Benshi GAIA APRS messaging diff against origin/main
in this worktree. Run `git diff origin/main..HEAD`. Audit src-tauri/src/winlink/ax25/uvpro/tncdata.rs
and message.rs (HT_SEND_DATA/DATA_RXD): fragment boundary math (53-byte chunking, off-by-one,
empty/oversize frames), fragment_id wraparound past 63, reassembler RF-loss handling (dropped final,
duplicate id 0, interleaved frames, out-of-order), the with_channel_id trailing-byte split, and any
panic path (slice indexing on truncated bodies). Output findings as markdown.
EOF
cat /tmp/codex-prompt.txt | npx --yes @openai/codex review - 2>&1 | tee dev/adversarial/2026-06-13-native-gaia-aprs-codex.md
```
- [ ] **Step 2:** Verify it's a real review (`wc -l dev/adversarial/2026-06-13-native-gaia-aprs-codex.md` ≫ 5; if a 5-line argparse stub, re-run per CLAUDE.md). If quota-limited ("usage limit … try again at HH:MM"), that is a **capacity-defer**, not a skip — defer to the next adrev window; do not substitute Claude.
- [ ] **Step 3:** Disposition each finding (fix / wontfix-with-reason) as a TDD cycle; summarize dispositions in the PR body + the handoff (raw transcript stays gitignored-local).

---

## Task 10: End-to-end integration test (mock link)

- [ ] **Step 1:** A `session.rs`/integration test that round-trips a real APRS message: build an APRS UI frame via the engine's encoder, `fragment_ax25` it, encode each `HT_SEND_DATA`, feed the bytes back through `decode_frame` → `Event::DataReceived` → `Reassembler` → assert the reassembled bytes equal the original AX.25 frame, and that `Frame::decode`/`extract_inbound`/`parse_info` recover the original callsign + message text. Proves the fragment layer is transparent to the APRS codec. **Implement/assert. Commit** (trailers).

---

## Verification (before pushing / marking ready)

- [ ] `cargo clippy --all-targets --manifest-path src-tauri/Cargo.toml -- -D warnings` (re-run until exit 0; it hides later-target lints behind the first failure — per `scoped-vitest-misses-contract-tests`).
- [ ] `cargo test -p tuxlink --manifest-path src-tauri/Cargo.toml uvpro:: aprs::` green.
- [ ] Task 9 Codex round done (or explicitly capacity-deferred with a follow-up note).
- [ ] PR #642 stays **draft**; the remaining gate is the operator on-air smoke (tuxlink-9ky). Do **not** mark ready.

---

## Self-review notes

- **Spec coverage:** the spec's "Phase 2 unified native model" = native carries messaging (Tasks 1–8) reusing the Phase-1 codec/ACK logic (engine seams, Task 6); capability-gating (Task 8); correctness gate (Task 9). Open-Q#1/#3 are already RESOLVED in the design doc.
- **Reused unchanged (YAGNI):** `tx.rs` (retransmit/timeout), `message.rs`/`framebuild.rs`/`frame.rs` APRS codec, delivery-state emit — Tasks 6–7 feed them, don't reimplement.
- **Type consistency:** `TncDataFragment` fields (`is_final`/`fragment_id`/`channel_id`/`data`), `encode_body`/`decode_body`, `fragment_ax25`, `Reassembler::push`, `encode_ht_send_data`, `Frame::SendDataReply`, `Event::DataReceived`, `handle_inbound_frame`, `tick_frames`, `send_aprs_frame` are used consistently across tasks.
- **Replace `<SESSION-MONIKER>`** in every commit trailer with the executing session's moniker (the commit-msg hook rejects the literal placeholder).
