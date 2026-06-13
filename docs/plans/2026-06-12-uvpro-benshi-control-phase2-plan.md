# Native UV-Pro Benshi Control Backend — Implementation Plan (Phase 2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Worktree-commit rule (tuxlink-specific):** dispatched implementer subagents
> CANNOT commit in this worktree — the main-checkout-race hook denies their commit
> because their Bash cwd resets to repo root each call. Implementers must
> code + run gates + STOP uncommitted; the PARENT controller commits each task
> from a standalone `cd <worktree>` (see `feedback_subagents_cannot_commit_in_worktrees`).
> NO cold cargo locally (`no_cold_cargo_on_contended_pi`): do NOT `cargo build/test`
> locally — gate on GitHub CI via the draft PR. Each "run the test" step means
> "the test exists + is correct"; CI is the green oracle.

**Goal:** A Rust backend that controls the BTECH UV-Pro over its native Benshi
protocol (RFCOMM + GAIA), exposing a documented Tauri command/event API
(`uvpro_connect/disconnect/get_status/get_channels/set_channel/set_frequency/set_mode`
+ a `uvpro:status` event) that a parallel frontend session wires to.

**Architecture:** A new capability-profile module `src-tauri/src/winlink/ax25/uvpro/`
that REUSES the existing `RfcommSocket` (winlink/ax25/rfcomm.rs) and layers a
big-endian bit codec → GAIA framing → Benshi `Message` codec → a `UvproSession`
driver (connect/hydrate/serialized-request/event-loop/disconnect) → Tauri
commands. It does NOT touch the `aprs/` KISS module (PR #642) or the AX.25 state
machine. Single-Bluetooth-host arbitration via a process-global owner-lock.

**Tech Stack:** Rust (Tauri v2 backend), `libc` RFCOMM socket (existing),
`serde` DTOs, `std::thread` read-loop + `std::sync::mpsc`/`Mutex` correlation.
No new crate dependencies (hand-rolled bit codec).

**Ground truth:** `docs/design/2026-06-12-uvpro-benshi-control-phase2-design.md`
(spec) + `docs/design/uvpro-benshi-golden-vectors.md` (the exact byte fixtures —
EVERY codec task asserts against these). Protocol facts:
`dev/scratch/benshi-GROUNDING-FINDINGS.md` (local).

---

## File structure

| File | Responsibility |
|---|---|
| `src-tauri/src/winlink/ax25/uvpro/mod.rs` | module root; re-exports; `UvproError` enum |
| `…/uvpro/bits.rs` | `BitWriter`/`BitReader` — big-endian pack/unpack of u-N across byte boundaries |
| `…/uvpro/gaia.rs` | `gaia_wrap(msg) -> bytes`; `GaiaDeframer` (streaming, resync, RX-checksum, buffer cap) |
| `…/uvpro/message.rs` | `Message` header codec + the command/reply/event body enums; `encode`/`decode` |
| `…/uvpro/rf_ch.rs` | `RfCh` struct codec (25-byte channel) + freq/mod/bandwidth mapping |
| `…/uvpro/model.rs` | serde DTOs: `UvproStatus`, `UvproChannel`, `UvproDeviceInfo`, enums (camelCase + per-enum rename) |
| `…/uvpro/settings.rs` | opaque-22-byte `Settings` holder + `patch_channel_a/b` nibble patch |
| `…/uvpro/session.rs` | `UvproSession` driver: connect/hydrate/read-loop/serialized-request/disconnect/owner-lock |
| `…/uvpro/commands.rs` | `#[tauri::command]` fns + `uvpro:status` broadcaster |
| `src-tauri/src/winlink/ax25/mod.rs` | add `pub mod uvpro;` |
| `src-tauri/src/lib.rs` | register commands in `generate_handler!` + `.manage(UvproSession)` |

---

## Task 1: Big-endian bit codec (`bits.rs`)

**Files:** Create `src-tauri/src/winlink/ax25/uvpro/bits.rs` + `mod.rs` (stub `pub mod bits;`).

The Benshi codec packs fields at bit granularity (u16 group, 1-bit is_reply,
u15 command, u30 freq, u4 rssi…), MSB-first, fields concatenated across byte
boundaries. `BitWriter` appends N-bit big-endian values and pads the final byte
with zeros; `BitReader` reads N-bit values in the same order.

- [ ] **Step 1: failing tests** (`#[cfg(test)]` in bits.rs)
```rust
#[test]
fn writes_then_reads_back_across_byte_boundary() {
    let mut w = BitWriter::new();
    w.write_uint(0x0002, 16); // command_group
    w.write_bool(false);      // is_reply
    w.write_uint(0x14, 15);   // command
    assert_eq!(w.into_bytes(), vec![0x00, 0x02, 0x00, 0x14]); // matches GET_HT_STATUS golden header

    let mut r = BitReader::new(&[0x00, 0x02, 0x80, 0x14]); // is_reply=1 variant
    assert_eq!(r.read_uint(16), 0x0002);
    assert!(r.read_bool());
    assert_eq!(r.read_uint(15), 0x14);
}

#[test]
fn packs_u30_freq_with_mod_prefix() {
    // tx_mod(2)=00 (FM) then tx_freq u30 = 146_520_000 → 08 bb b7 c0
    let mut w = BitWriter::new();
    w.write_uint(0, 2);
    w.write_uint(146_520_000, 30);
    assert_eq!(w.into_bytes(), vec![0x08, 0xbb, 0xb7, 0xc0]);
}
```
- [ ] **Step 2:** confirm tests are written and reference the golden bytes from `uvpro-benshi-golden-vectors.md`.
- [ ] **Step 3: implement**
```rust
pub struct BitWriter { bits: Vec<bool> }
impl BitWriter {
    pub fn new() -> Self { Self { bits: Vec::new() } }
    pub fn write_uint(&mut self, v: u64, n: u32) {
        for i in (0..n).rev() { self.bits.push((v >> i) & 1 == 1); }
    }
    pub fn write_bool(&mut self, b: bool) { self.bits.push(b); }
    pub fn into_bytes(self) -> Vec<u8> {
        let mut out = vec![0u8; (self.bits.len() + 7) / 8];
        for (i, b) in self.bits.iter().enumerate() {
            if *b { out[i / 8] |= 0x80 >> (i % 8); }
        }
        out
    }
    pub fn bit_len(&self) -> usize { self.bits.len() }
}
pub struct BitReader<'a> { bytes: &'a [u8], pos: usize }
impl<'a> BitReader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self { Self { bytes, pos: 0 } }
    pub fn read_uint(&mut self, n: u32) -> u64 {
        let mut v = 0u64;
        for _ in 0..n {
            let byte = self.bytes[self.pos / 8];
            let bit = (byte >> (7 - (self.pos % 8))) & 1;
            v = (v << 1) | bit as u64;
            self.pos += 1;
        }
        v
    }
    pub fn read_bool(&mut self) -> bool { self.read_uint(1) == 1 }
    pub fn remaining_bits(&self) -> usize { self.bytes.len() * 8 - self.pos }
}
```
> Use a `bool` Vec for clarity (these messages are ≤ 30 bytes; perf is irrelevant).
> Do NOT optimize into a packed-cursor — KISS, and the golden tests are the proof.
- [ ] **Step 4:** tests exist + match golden bytes (CI runs them).
- [ ] **Step 5 (parent commits):** `feat(uvpro): big-endian bit codec for the Benshi wire format`

## Task 2: GAIA framing + streaming deframer (`gaia.rs`)

**Files:** Create `…/uvpro/gaia.rs`; add `pub mod gaia;` to mod.rs.

GAIA (RFCOMM transport): `ff 01 <flags:u8> <n:u8> <data[n+4]> [csum:u8 if flags&1]`.
We send `flags=0` (no checksum). The deframer buffers bytes and yields complete
`data` payloads, handling: multiple frames per chunk, a frame split across reads,
RX frames WITH checksum, desync (resync to next `ff 01`), and a buffer cap.

- [ ] **Step 1: failing tests**
```rust
#[test]
fn wraps_a_message_with_correct_n() {
    // message = 4-byte GET_HT_STATUS header
    assert_eq!(gaia_wrap(&[0x00,0x02,0x00,0x14]), vec![0xff,0x01,0x00,0x00,0x00,0x02,0x00,0x14]);
}
#[test]
fn deframes_two_frames_from_one_buffer() {
    // golden two-frame concat (status reply 5-byte data + GET_HT_STATUS 4-byte data)
    let buf = hex("ff 01 00 05 00 02 80 14 00 b4 3c c0 00 ff 01 00 00 00 02 00 14");
    let mut d = GaiaDeframer::new();
    let frames = d.push(&buf);
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0], hex("00 02 80 14 00 b4 3c c0 00")); // n=5 → 5+4=9 bytes data
    assert_eq!(frames[1], hex("00 02 00 14"));                // n=0 → 4 bytes data
    assert_eq!(d.buffered_len(), 0);
}
#[test]
fn reassembles_a_frame_split_across_two_pushes() {
    let mut d = GaiaDeframer::new();
    assert!(d.push(&hex("ff 01 00 00 00 02")).is_empty()); // partial
    let frames = d.push(&hex("00 14"));
    assert_eq!(frames, vec![hex("00 02 00 14")]);
}
#[test]
fn resyncs_past_leading_garbage() {
    let mut d = GaiaDeframer::new();
    let frames = d.push(&hex("de ad ff 01 00 00 00 02 00 14"));
    assert_eq!(frames, vec![hex("00 02 00 14")]);
}
#[test]
fn consumes_trailing_checksum_when_flagged() {
    // flags=1 → 1 trailing csum byte after data; deframer must not desync
    let mut d = GaiaDeframer::new();
    let frames = d.push(&hex("ff 01 01 00 00 02 00 14 99")); // 99 = csum
    assert_eq!(frames, vec![hex("00 02 00 14")]);
}
```
(`hex(&str)` = test helper parsing space-separated hex into Vec<u8>; define it in the test module.)
- [ ] **Step 2:** verify tests fail (functions undefined).
- [ ] **Step 3: implement**
```rust
const START: u8 = 0xff;
const VERSION: u8 = 0x01;
const FLAG_CHECKSUM: u8 = 0x01;
const MAX_BUFFER: usize = 4096; // bound a desync'd/garbage stream

pub fn gaia_wrap(msg: &[u8]) -> Vec<u8> {
    let n = (msg.len() - 4) as u8; // data = command(4) + payload; n excludes the 4 command bytes
    let mut out = vec![START, VERSION, 0x00, n];
    out.extend_from_slice(msg);
    out
}

#[derive(Default)]
pub struct GaiaDeframer { buf: Vec<u8> }
impl GaiaDeframer {
    pub fn new() -> Self { Self::default() }
    pub fn buffered_len(&self) -> usize { self.buf.len() }
    pub fn push(&mut self, data: &[u8]) -> Vec<Vec<u8>> {
        self.buf.extend_from_slice(data);
        if self.buf.len() > MAX_BUFFER { // desync guard: drop to last possible start
            if let Some(i) = find_start(&self.buf) { self.buf.drain(..i); }
            else { self.buf.clear(); }
        }
        let mut out = Vec::new();
        loop {
            // resync: ensure buffer head is a frame start
            match find_start(&self.buf) {
                Some(0) => {}
                Some(i) => { self.buf.drain(..i); }
                None => { self.buf.clear(); break; }
            }
            if self.buf.len() < 4 { break; } // need start,version,flags,n
            let flags = self.buf[2];
            let n = self.buf[3] as usize;
            let csum = if flags & FLAG_CHECKSUM != 0 { 1 } else { 0 };
            let total = 4 + (n + 4) + csum; // hdr + data(n+4) + optional csum
            if self.buf.len() < total { break; }
            let data = self.buf[4..4 + n + 4].to_vec();
            self.buf.drain(..total);
            out.push(data);
        }
        out
    }
}
// find the next `ff 01` start sentinel
fn find_start(b: &[u8]) -> Option<usize> {
    b.windows(2).position(|w| w[0] == START && w[1] == VERSION)
}
```
- [ ] **Step 4:** tests exist + assert against golden two-frame vector.
- [ ] **Step 5 (parent commits):** `feat(uvpro): GAIA frame wrap + resilient streaming deframer`

## Task 3: Message header + command bodies encode (`message.rs`)

**Files:** Create `…/uvpro/message.rs`; `pub mod message;`.

Encode the request `Message`s the session sends. Header = `group:u16 + is_reply:1 + command:u15`. Command IDs: `GET_DEV_INFO=4, READ_STATUS=5, REGISTER_NOTIFICATION=6, READ_RF_CH=13, WRITE_RF_CH=14, GET_HT_STATUS=20`. Group BASIC=2.

- [ ] **Step 1: failing tests** (assert exact golden request bytes)
```rust
#[test]
fn encodes_request_headers_and_bodies() {
    assert_eq!(encode_get_ht_status(), hex("00 02 00 14"));
    assert_eq!(encode_read_rf_ch(0), hex("00 02 00 0d 00"));
    assert_eq!(encode_read_battery_pct(), hex("00 02 00 05 00 04"));
    assert_eq!(encode_register_notification(EventType::HtStatusChanged), hex("00 02 00 06 01"));
    assert_eq!(encode_get_dev_info(), hex("00 02 00 04 03"));
}
```
- [ ] **Step 2:** verify fail.
- [ ] **Step 3: implement** using `BitWriter`. A private `fn header(cmd: u16, is_reply: bool) -> BitWriter` writes group(16)=2, is_reply(1), command(15)=cmd. Then each encoder appends its body bytes:
  - `get_ht_status`: body empty.
  - `read_rf_ch(id)`: body `write_uint(id, 8)`.
  - `read_battery_pct`: body `write_uint(4, 16)` (PowerStatusType u16, BATTERY_LEVEL_AS_PERCENTAGE=4).
  - `register_notification(ev)`: body `write_uint(ev as u64, 8)`.
  - `get_dev_info`: body byte `0x03` (matches golden — GetDevInfoBody is a single 0x03; pin it literally).
  `EventType` enum: `HtStatusChanged=1, DataRxd=2, HtChChanged=5, HtSettingsChanged=6`.
- [ ] **Step 4:** golden assertions pass on CI.
- [ ] **Step 5 (parent commits):** `feat(uvpro): Benshi Message header + request encoders`

## Task 4: RfCh codec (`rf_ch.rs`)

**Files:** Create `…/uvpro/rf_ch.rs`; `pub mod rf_ch;`.

`RfCh` is 200 bits = 25 bytes in field order from GROUNDING-FINDINGS: channel_id(8),
tx_mod(2), tx_freq(u30), rx_mod(2), rx_freq(u30), tx_sub_audio(16), rx_sub_audio(16),
scan(1), tx_at_max_power(1), talk_around(1), bandwidth(1), pre_de_emph_bypass(1),
sign(1), tx_at_med_power(1), tx_disable(1), fixed_freq(1), fixed_bandwidth(1),
fixed_tx_power(1), mute(1), _pad(4), name_str(10 bytes). Decode + encode (round-trip).

- [ ] **Step 1: failing tests** (golden WRITE_RF_CH body for 146.520 FM WIDE "CALL")
```rust
const RFCH_146520_FM_WIDE_CALL: &str =
    "00 08 bb b7 c0 08 bb b7 c0 00 00 00 00 50 00 43 41 4c 4c 00 00 00 00 00 00";
#[test]
fn rfch_roundtrips_against_golden() {
    let bytes = hex(RFCH_146520_FM_WIDE_CALL);
    let ch = RfCh::decode(&bytes);
    assert_eq!(ch.channel_id, 0);
    assert_eq!(ch.tx_mod, Modulation::Fm);
    assert!((ch.tx_freq_hz as i64 - 146_520_000).abs() == 0);
    assert!((ch.rx_freq_hz as i64 - 146_520_000).abs() == 0);
    assert_eq!(ch.bandwidth, Bandwidth::Wide);
    assert_eq!(ch.name(), "CALL");
    assert_eq!(ch.encode(), bytes); // identity
}
```
- [ ] **Step 2:** verify fail.
- [ ] **Step 3: implement** `RfCh { channel_id:u8, tx_mod, tx_freq_hz:u32, rx_mod, rx_freq_hz:u32, tx_sub_audio:u16, rx_sub_audio:u16, flags…, name:[u8;10] }` with `decode(&[u8])` via `BitReader` and `encode()` via `BitWriter` in the exact field order. `Modulation { Fm=0, Am=1, Dmr=2 }`, `Bandwidth { Narrow=0, Wide=1 }`. `name()` trims trailing zeros / non-printables. Freq stored as Hz (u32). **Encode MUST reproduce every field incl. flags/sub-audio/pad** (identity test is the guard). DMR variant (longer) — decode only the 25-byte RfCh; if the reply length is the DMR length, decode the extra DMR fields too OR store remaining bytes opaque (note in code).
- [ ] **Step 4:** identity round-trip passes on CI.
- [ ] **Step 5 (parent commits):** `feat(uvpro): RfCh channel codec (freq/mod/bandwidth round-trip)`

## Task 5: Reply + event decode (`message.rs` extension)

**Files:** Modify `…/uvpro/message.rs`.

Decode inbound frames into a `Frame` enum the session routes. Route by is_reply
(MSB of byte 2) + command. Tolerate/skip unknown ids (return `Frame::Unknown`).

- [ ] **Step 1: failing tests** (golden decode vectors)
```rust
#[test]
fn decodes_status_reply_with_rssi() {
    match decode_frame(&hex("00 02 80 14 00 b4 3c c0 00")) {
        Frame::StatusReply { status } => {
            assert!(!status.is_in_tx); assert!(status.is_in_rx);
            assert_eq!(status.curr_channel_id, 3);
            assert_eq!(status.rssi, Some(80));
        }
        f => panic!("wrong frame: {f:?}"),
    }
}
#[test]
fn decodes_ch_changed_event() {
    match decode_frame(&hex("00 02 00 09 05 05 1a 95 6b 80 1a 95 6b 80 00 00 00 00 40 00 55 48 46 00 00 00 00 00 00 00")) {
        Frame::Event(Event::ChannelChanged { channel }) => {
            assert_eq!(channel.channel_id, 5);
            assert_eq!(channel.name(), "UHF");
        }
        f => panic!("wrong frame: {f:?}"),
    }
}
#[test]
fn decodes_battery_reply() {
    match decode_frame(&hex("00 02 80 05 00 00 04 49")) {
        Frame::BatteryReply { kind, value } => { assert_eq!(value, 73); }
        f => panic!("wrong: {f:?}"),
    }
}
#[test]
fn decodes_write_rf_ch_reply_ok() {
    assert!(matches!(decode_frame(&hex("00 02 80 0e 00 00")), Frame::WriteRfChReply { reply_status: 0, .. }));
}
#[test]
fn unknown_command_is_not_an_error() {
    assert!(matches!(decode_frame(&hex("00 02 00 7f")), Frame::Unknown { .. }));
}
```
- [ ] **Step 2:** verify fail.
- [ ] **Step 3: implement** `decode_frame(&[u8]) -> Frame`. Read header (group, is_reply, command). Match (is_reply, command):
  - (true, 20 GET_HT_STATUS): reply_status u8; then Status — discriminate by remaining bytes: 2 bytes = base Status, 4 bytes = StatusExt (has rssi). Build `UvproStatusFields`. rssi = `round(raw4 * 100.0/15.0)` as u8.
  - (false, 9 EVENT_NOTIFICATION): event_type u8; 5=HT_CH_CHANGED → RfCh decode → `Event::ChannelChanged`; 1=HT_STATUS_CHANGED → Status decode → `Event::StatusChanged`; else `Event::OtherIgnored` (incl. DATA_RXD=2 — belongs to data path).
  - (true, 5 READ_STATUS): reply_status u8; power_status_type u16; value (u8 for level/pct, u16 for voltage) → `Frame::BatteryReply`.
  - (true, 13 READ_RF_CH): reply_status u8; if SUCCESS, RfCh decode → `Frame::ChannelReply`.
  - (true, 14 WRITE_RF_CH): reply_status u8 + channel_id u8 → `Frame::WriteRfChReply`.
  - (true, 4 GET_DEV_INFO): reply_status u8 + dev_info bytes → `Frame::DevInfoReply` (parse model/fw/channel_count per dev_info.py — read dev_info.py before implementing this arm).
  - (true, 11 WRITE_SETTINGS) / (true,10 READ_SETTINGS): `Frame::SettingsReply`/`Frame::WriteSettingsReply`.
  - else → `Frame::Unknown { command, is_reply }`.
  `Frame` derives `Debug`.
- [ ] **Step 4:** golden decode tests pass on CI.
- [ ] **Step 5 (parent commits):** `feat(uvpro): decode Benshi replies + push events`

## Task 6: serde DTOs + RfCh↔DTO mapping (`model.rs`)

**Files:** Create `…/uvpro/model.rs`; `pub mod model;`.

The wire-facing DTOs from the spec, camelCase, with per-enum `rename_all`
(documented Codex catch — enum variants don't inherit struct rename).

- [ ] **Step 1: failing tests**
```rust
#[test]
fn status_serializes_camelcase_with_state_enum() {
    let s = UvproStatus { state: ConnState::Connected, rx_mhz: Some(146.52), mode: Some(Modulation::Fm), ..Default::default() };
    let j = serde_json::to_string(&s).unwrap();
    assert!(j.contains("\"state\":\"connected\""));
    assert!(j.contains("\"rxMhz\":146.52"));
    assert!(j.contains("\"mode\":\"fm\""));
}
#[test]
fn channel_from_rfch_maps_freq_to_mhz() {
    let ch = UvproChannel::from_rfch(&RfCh::decode(&hex(RFCH_146520_FM_WIDE_CALL)));
    assert_eq!(ch.rx_mhz, 146.52);
    assert_eq!(ch.mode, Modulation::Fm);
    assert_eq!(ch.bandwidth, Bandwidth::Wide);
    assert_eq!(ch.name, "CALL");
}
```
- [ ] **Step 2:** verify fail.
- [ ] **Step 3: implement** `#[derive(Serialize, Deserialize, Clone, Debug, Default)] #[serde(rename_all="camelCase")]` for `UvproStatus`, `UvproChannel`, `UvproDeviceInfo`; `#[serde(rename_all="lowercase")]` (or `camelCase`) on `enum ConnState { Disconnected, Connecting, Connected }`, `Modulation`, `Bandwidth`. `from_rfch`: `rx_mhz = rfch.rx_freq_hz as f64 / 1e6`, etc. (round display to avoid float noise where shown).
- [ ] **Step 4:** serde shape tests pass on CI.
- [ ] **Step 5 (parent commits):** `feat(uvpro): frontend-facing camelCase DTOs + RfCh mapping`

## Task 7: Settings opaque + channel patch (`settings.rs`)

**Files:** Create `…/uvpro/settings.rs`; `pub mod settings;`.

`set_channel` writes the FULL 22-byte Settings back with only `channel_a`/`channel_b`
changed. Keep Settings opaque (don't decode 50 fields); patch the nibbles at
pinned bit offsets. Offsets pinned by a diff-of-two-encodings golden vector
generated at implementation time (see below).

- [ ] **Step 0 (pin offsets):** generate two Settings encodings differing only in
  channel_a (and only channel_b) using the benlink venv:
```bash
/tmp/benv/bin/python - <<'PY'
import sys; sys.path.insert(0,'dev/scratch/benshi-re/benlink/src'); import benlink.protocol as p
base=dict(channel_a_lower=0,channel_b_lower=0,scan=False,aghfp_call_mode=0,double_channel=0,squelch_level=0,tail_elim=False,auto_relay_en=False,auto_power_on=False,keep_aghfp_link=False,mic_gain=0,tx_hold_time=0,tx_time_limit=0,local_speaker=0,bt_mic_gain=0,adaptive_response=False,dis_tone=False,power_saving_mode=False,auto_power_off=0,auto_share_loc_ch=0,hm_speaker=0,positioning_system=0,time_offset=0,use_freq_range_2=False,ptt_lock=False,leading_sync_bit_en=False,pairing_at_power_on=False,screen_timeout=0,kiss_upload_tx_msg=False,kiss_en=False,imperial_unit=False,channel_a_upper=0,channel_b_upper=0,wx_mode=0,noaa_ch=0,vfol_tx_power_x=0,vfo2_tx_power_x=0,dis_digital_mute=False,signaling_ecc_en=False,ch_data_lock=False,auto_share_loc_ch_upper=0,kiss_tx_delay=0,kiss_tx_tail=0,vox_en=False,vox_level=0,dis_bt_mic=False,vox_delay=0,ns_en=False,alarm_volume=0,use_custom_location=False,gpwpl_upload_en=False,vfo1_mod_freq_x=0,custom_location_lat=0,custom_location_lon=0)
def b(**kw): d=dict(base); d.update(kw); return bytes(p.Settings(**d).to_bytes())
print("a=0:", b().hex()); print("a_lower=15:", b(channel_a_lower=15).hex()); print("a_upper=15:", b(channel_a_upper=15).hex()); print("b_lower=15:", b(channel_b_lower=15).hex()); print("b_upper=15:", b(channel_b_upper=15).hex())
PY
```
  Record which byte/nibble each field occupies → write the patch offsets as named consts.
- [ ] **Step 1: failing test**
```rust
#[test]
fn patch_channel_a_changes_only_channel_a_nibbles() {
    let raw = hex("12 13 94 0a 51 60 04 02 28 00 00 00 04 00 00 00 00 00 00 00 00 00"); // a=(0,1) b=(0,2)
    let out = patch_channel(&raw, Vfo::A, 1); // set channel_a = 1
    // byte0 high nibble already 1; identical here. Use a different target to prove the diff:
    let out2 = patch_channel(&raw, Vfo::A, 200); // upper=12 lower=8 → 0xC.. nibble + upper
    assert_eq!(out2.len(), 22);
    // assert exactly the channel_a nibbles changed vs raw (positions from Step 0)
}
```
- [ ] **Step 2:** verify fail.
- [ ] **Step 3: implement** `pub fn patch_channel(raw22: &[u8], vfo: Vfo, channel_id: u8) -> Vec<u8>` — copy raw, split channel_id into lower=id&0x0F, upper=id>>4, write the four nibbles at the pinned offsets (use bit-level set via a small helper or byte/nibble masks). Guard `raw22.len()==22`.
- [ ] **Step 4:** patch test passes; add an identity test (`patch` then read back the channel id == input).
- [ ] **Step 5 (parent commits):** `feat(uvpro): Settings channel-select nibble patch (set_channel)`

## Task 8: UvproSession driver (`session.rs`)

**Files:** Create `…/uvpro/session.rs` + `mod.rs` `UvproError`; `pub mod session;`.

The stateful driver. Mirrors `ModemSession` shape. Owns the `RfcommSocket`,
runs a read-loop thread that deframes + dispatches, serializes commands (one
outstanding, with timeout), caches state, emits status. NO auto-reconnect.

- [ ] **Step 1: failing tests** against an in-memory fake `ByteLink` peer (no radio).
  Build a `FakePeer` (a `Read+Write` pair backed by `VecDeque`/channels) seeded
  to answer `GET_DEV_INFO`→devinfo(channel_count=2), `READ_RF_CH(0/1)`→two channels,
  `GET_HT_STATUS`→StatusExt, `REGISTER_NOTIFICATION`→(no reply, fire-and-forget),
  and to push a `HT_CH_CHANGED` event on demand.
```rust
#[test]
fn hydrate_populates_state_and_channels() {
    let (sess, peer) = UvproSession::with_link_for_test(fake_uvpro_peer());
    sess.hydrate().unwrap();
    let st = sess.status_snapshot();
    assert_eq!(st.state, ConnState::Connected);
    assert_eq!(sess.channels().len(), 2);
    assert!(st.rssi.is_some());
}
#[test]
fn set_frequency_emits_write_rf_ch_with_new_freq() {
    let (sess, peer) = UvproSession::with_link_for_test(fake_uvpro_peer());
    sess.hydrate().unwrap();
    sess.set_frequency(0, 146.520, None).unwrap();
    let sent = peer.last_written_frame();
    assert_eq!(&sent[..4], &hex("00 02 00 0e")[..]); // WRITE_RF_CH header
}
#[test]
fn channel_changed_event_updates_cached_state() {
    let (sess, peer) = UvproSession::with_link_for_test(fake_uvpro_peer());
    sess.hydrate().unwrap();
    peer.push_event(hex("00 02 00 09 05 05 1a 95 6b 80 1a 95 6b 80 00 00 00 00 40 00 55 48 46 00 00 00 00 00 00 00"));
    sess.pump_for_test(); // drain one read
    assert_eq!(sess.status_snapshot().current_channel_id, Some(5));
}
#[test]
fn command_times_out_when_no_reply() {
    let (sess, _peer) = UvproSession::with_link_for_test(silent_peer());
    sess.hydrate_with_timeout(Duration::from_millis(50)).unwrap_err(); // Timeout
}
```
- [ ] **Step 2:** verify fail.
- [ ] **Step 3: implement**
  - `UvproSession { inner: Mutex<Inner> }`, `Inner { link: Option<Box<dyn ByteLink>>, deframer: GaiaDeframer, state: UvproStatus, channels: Vec<UvproChannel>, settings_raw: Option<Vec<u8>>, abort: Arc<AtomicBool> }`.
  - `connect(mac)` → reuse `rfcomm::resolve_spp_channel(mac)` + `RfcommSocket::connect(mac, ch, read_timeout, write_timeout)`; store as link; `hydrate()`.
  - `hydrate()` runs the sequence (Task-3 encoders → `send_and_wait` for each reply): GET_DEV_INFO, READ_RF_CH per channel, READ_SETTINGS (store raw), GET_HT_STATUS, REGISTER_NOTIFICATION(HtStatusChanged) (fire-and-forget). Populate state.
  - `send_and_wait(req_bytes, expect_cmd, timeout)`: hold a command-mutex (one outstanding); `link.write(gaia_wrap(&req))`; loop `link.read` → `deframer.push` → for each frame `decode_frame`; if it's the expected reply, return it; if it's an event, apply to cached state (+ mark for emit); honor timeout + abort. (Production read happens on the read-loop thread; for tests expose `pump_for_test`/`send_and_wait` synchronously.)
  - In production: a background thread loops `link.read`→deframer→`decode_frame`; replies go to a `mpsc`/`Notify` the command path waits on; events update state + call the emit callback. `disconnect()` sets abort, drops the socket, sets state Disconnected, releases owner-lock (Task 9), joins thread. NO reconnect.
  - `set_frequency(ch,rx,tx)`: clone cached `RfCh` for ch, set rx/tx freq Hz = round(mhz*1e6), `WRITE_RF_CH`, await reply, update cache. `set_mode` similar (mod+bandwidth). `set_channel(id,vfo)`: `settings::patch_channel(settings_raw, vfo, id)` → WRITE_SETTINGS → await reply.
  - `UvproError`: `LinkBusy{holder:String}, NotConnected, Timeout, Protocol(String), RadioRejected(String), Io(String), BadMac`. Map non-SUCCESS reply_status → `RadioRejected`.
- [ ] **Step 4:** session tests pass on CI.
- [ ] **Step 5 (parent commits):** `feat(uvpro): UvproSession driver — connect/hydrate/serialized-request/events`

## Task 9: Single-Bluetooth-host owner-lock (`session.rs` / `mod.rs`)

**Files:** Modify `…/uvpro/mod.rs` + `session.rs`.

A process-global owner of the UV-Pro BT link. Native control acquires it on
connect; releases on every disconnect path (drop guard).

- [ ] **Step 1: failing test**
```rust
#[test]
fn second_acquire_is_link_busy() {
    let lock = UvproLinkLock::default();
    let _g = lock.acquire("uvpro-native").expect("first");
    assert!(matches!(lock.acquire("uvpro-native"), Err(UvproError::LinkBusy { .. })));
}
#[test]
fn drop_releases_the_lock() {
    let lock = UvproLinkLock::default();
    { let _g = lock.acquire("x").unwrap(); }
    assert!(lock.acquire("y").is_ok());
}
```
- [ ] **Step 2:** verify fail.
- [ ] **Step 3: implement** `UvproLinkLock { holder: Mutex<Option<String>> }`; `acquire(who) -> Result<LinkGuard, UvproError>` sets holder if None else `LinkBusy{holder}`; `LinkGuard` holds an `Arc<UvproLinkLock>` and clears holder on `Drop`. `UvproSession::connect` acquires before opening the socket; stores the guard in `Inner`; `disconnect`/error/`Drop` release it.
  > **Phase-2 limitation (documented):** the KISS/packet path does NOT yet consult
  > this lock, so a conflict from THAT direction surfaces as a raw socket error,
  > not `LinkBusy`. File the KISS-side follow-up (see plan tail).
- [ ] **Step 4:** lock tests pass on CI.
- [ ] **Step 5 (parent commits):** `feat(uvpro): single-Bluetooth-host owner-lock + LinkBusy`

## Task 10: Tauri commands + status broadcaster + registration (`commands.rs`, `lib.rs`)

**Files:** Create `…/uvpro/commands.rs`; modify `src-tauri/src/lib.rs` + `winlink/ax25/mod.rs`.

- [ ] **Step 1: failing tests** (inner helpers, no Tauri `State`)
```rust
#[test]
fn get_status_inner_reports_disconnected_before_connect() {
    let sess = Arc::new(UvproSession::new());
    assert_eq!(uvpro_get_status_inner(&sess).state, ConnState::Disconnected);
}
#[test]
fn connect_inner_link_busy_surfaces() {
    let sess = Arc::new(UvproSession::new());
    let _g = sess.link_lock().acquire("other").unwrap();
    assert!(matches!(uvpro_connect_inner(&sess, Some("38:D2:00:01:55:5C".into())), Err(UvproError::LinkBusy{..})));
}
```
- [ ] **Step 2:** verify fail.
- [ ] **Step 3: implement** thin `#[tauri::command]` wrappers delegating to `*_inner(&Arc<UvproSession>, …)` helpers (so they're unit-testable without `State`), per the `modem_commands.rs` pattern:
  `uvpro_connect(mac: Option<String>)`, `uvpro_disconnect()`, `uvpro_get_status()`, `uvpro_get_channels()`, `uvpro_set_channel(channel_id, vfo)`, `uvpro_set_frequency(channel_id, rx_mhz, tx_mhz)`, `uvpro_set_mode(channel_id, mode, bandwidth)`. mac defaults to the configured packet `Bluetooth.mac` (`config::read_config()…`). Each returns `Result<UvproStatus|.., String>` (map `UvproError` to a string carrying `kind`). Status broadcaster: on connect, install an emit callback `|s| app.emit("uvpro:status", s)` (mirror `modem_status::STATUS_EVENT`); define `pub const STATUS_EVENT: &str = "uvpro:status";`. Battery poll: a bounded (≥30s) timer in the read-loop that issues `READ_STATUS` through the serialized path. Register in `lib.rs`: `.manage(Arc::new(UvproSession::new()))` + add the 7 commands to `generate_handler!`. Add `pub mod uvpro;` to `winlink/ax25/mod.rs`.
- [ ] **Step 4:** inner tests pass; CI clippy `--all-targets -D warnings` clean.
- [ ] **Step 5 (parent commits):** `feat(uvpro): Tauri commands + uvpro:status broadcaster + registration`

## Task 11: API contract doc for the frontend session

**Files:** Create `docs/design/uvpro-control-api.md`.

- [ ] **Step 1:** write the doc: command names, args, return DTOs (camelCase JSON
  shapes from the spec), the `uvpro:status` event payload + when it fires, the
  `UvproError` kinds, the connect→status→set flow, and the single-BT-host
  arbitration contract (must not use the UV-Pro while a KISS/packet session holds
  it; `LinkBusy` semantics; the Phase-2 limitation). No Rust required to consume it.
- [ ] **Step 2 (parent commits):** `docs(uvpro): frontend control API contract`

## Task 12: Integration review + draft PR + CI green

- [ ] After Tasks 1–11: review the batch from multiple perspectives — a minimum
  of THREE review rounds; if substantive issues remain in round 3, keep going.
  Check: every codec arm has a golden test; the deframer handles all five hazard
  cases; `send_and_wait` can't deadlock (lock dropped before blocking read on the
  read-loop thread); owner-lock released on all disconnect paths; no `unwrap()` on
  socket I/O; clippy `--all-targets` clean (`scoped_vitest_misses_contract_tests`
  analog — clippy hides later-target lints, re-run till exit 0).
- [ ] Push the branch; open a **draft** PR `--base main`; let GitHub CI compile +
  run `verify` (clippy + tests, both arches) + `build-linux`. Iterate on CI until
  green. Do NOT mark ready — operator on-air smoke + the deferred Codex round gate
  that.

---

## Outstanding follow-ups (file as bd issues during execution)
- KISS-side arbitration: teach the Winlink packet / APRS-over-KISS path to consult
  `UvproLinkLock` so a conflict from that direction is a clean `LinkBusy`.
- Native-data integration: APRS over `HT_SEND_DATA` on the native link (parent
  epic premium-tier; depends on Phase 1a landing).
- DEFERRED Codex cross-provider adrev (quota resets Jun 13 1:49 PM) — run on the
  code diff.

## Self-review (against the spec)
- **Spec coverage:** transport+GAIA (T2), Message codec (T3/T5), RfCh (T4),
  status/rssi/battery (T5/T6), set_channel via Settings (T7), session+hydrate+events
  +no-reconnect (T8), arbitration/LinkBusy (T9), 7 commands + status event (T10),
  API doc (T11), RADIO-1 non-transmitting (no TX command exists; abort=drop socket,
  covered in T8 disconnect) — all mapped.
- **Placeholders:** none — every codec task pins golden bytes; T7 Step 0 generates
  the only runtime-derived constants (offsets) with an exact script.
- **Type consistency:** `RfCh`, `Modulation`, `Bandwidth`, `UvproStatus`,
  `ConnState`, `Frame`, `Event`, `UvproError`, `UvproSession`, `UvproLinkLock`,
  `STATUS_EVENT` used consistently across tasks.
