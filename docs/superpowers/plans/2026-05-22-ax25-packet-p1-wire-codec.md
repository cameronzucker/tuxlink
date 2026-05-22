# AX.25 Packet — P1: Wire Codec Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the pure, deterministic AX.25-over-KISS wire codec — address/path/control-field encode+decode and KISS framing — as a self-contained `winlink/ax25/` module, fully unit-tested with no Winlink, no network, and no RF.

**Architecture:** A new `src-tauri/src/winlink/ax25/` module under the existing native-client crate. P1 is *only* the byte-level codec: `frame.rs` (AX.25 addresses, address path with ≤2 digipeaters, control fields for the connected-mode frame set, full-frame assembly/parse) and `kiss.rs` (KISS FEND/FESC framing + TNC param command frames). The connected-mode **state machine**, **transports**, **Winlink integration**, and **UI** are separate plans (P2–P4). **KISS-mode invariant: the host does NOT compute the HDLC FCS, flags, or bit-stuffing — the TNC/modem does.** So frames here carry only `[address-path][control][PID?][info?]`.

**Tech Stack:** Rust (the existing `src-tauri` crate). No new dependencies in P1 (pure `std`). Builds/tests on `origin/main` in `worktrees/bd-tuxlink-7fr-ax25-packet` (already contains the native client).

**Authority for bit layouts:** AX.25 v2.2 §3–4 + the decompiled `TNCKissInterface.dll` (`Frame.FrameTypes`, `StationAddress`) at `dev/scratch/winlink-re/decompiled/tnckiss/` (local). Each task that encodes wire bits names the value to cross-check; the tests use those values as fixtures.

**Run tests with:** `cargo test --manifest-path src-tauri/Cargo.toml ax25::` (absolute manifest path per the worktree path-pinning convention).

---

### Task 1: Scaffold the `ax25` module

**Files:**
- Create: `src-tauri/src/winlink/ax25/mod.rs`
- Modify: `src-tauri/src/winlink/mod.rs` (add `pub mod ax25;`)

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/winlink/ax25/mod.rs`:
```rust
//! AX.25 connected-mode packet codec + (later) link layer.
//! P1 = wire codec only: addresses, paths, control fields, KISS framing.
//! KISS invariant: the TNC owns FCS/flags/bit-stuffing; the host frames carry
//! only [address-path][control][PID?][info?].

pub mod frame;
pub mod kiss;

#[cfg(test)]
mod module_smoke {
    #[test]
    fn module_is_wired() {
        assert_eq!(2 + 2, 4);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::`
Expected: FAIL to compile — `frame`/`kiss` modules don't exist yet, and `winlink/mod.rs` doesn't declare `ax25`.

- [ ] **Step 3: Write minimal implementation**

Add to `src-tauri/src/winlink/mod.rs` (alongside the existing `pub mod` lines):
```rust
pub mod ax25;
```
Create empty `src-tauri/src/winlink/ax25/frame.rs` and `src-tauri/src/winlink/ax25/kiss.rs` (one line each: `//! placeholder` ).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::`
Expected: PASS (`module_is_wired`).

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/winlink/mod.rs src-tauri/src/winlink/ax25/
git commit -m "feat(ax25): scaffold winlink/ax25 wire-codec module (tuxlink-7fr)

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: AX.25 address — encode

**Files:**
- Modify: `src-tauri/src/winlink/ax25/frame.rs`

AX.25 address octet layout (7 bytes): bytes 0–5 are the callsign ASCII (uppercase, space-padded to 6) each **left-shifted 1 bit**; byte 6 is the SSID octet `C RR SSSS E` = bit7 `cr` (command/response, or H "has-been-repeated" for digis), bits6–5 reserved = `11`, bits4–1 = SSID `(ssid & 0x0F) << 1`, bit0 `last` (extension; 1 = final address in the path). Cross-check: `TNCKissInterface.StationAddress`.

- [ ] **Step 1: Write the failing test**
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    pub call: String, // base callsign, ≤6 chars, uppercased
    pub ssid: u8,     // 0–15
}

#[cfg(test)]
mod address_encode_tests {
    use super::*;
    #[test]
    fn encodes_call_and_ssid_with_flags() {
        // "N7CPZ" padded to "N7CPZ ", each byte <<1; SSID=0, reserved=11,
        // cr=false, last=true -> SSID octet 0x60|0x01 = 0x61.
        let a = Address { call: "N7CPZ".into(), ssid: 0 };
        let bytes = a.encode(/*cr=*/ false, /*last=*/ true);
        assert_eq!(
            bytes,
            [0x9C, 0x6E, 0x86, 0xA0, 0xB4, 0x40, 0x61]
        );
    }
    #[test]
    fn encodes_ssid_and_cr_bit() {
        // SSID=7, cr=true, last=false -> 0x80|0x60|(7<<1)|0 = 0xEE.
        let a = Address { call: "W7AUX".into(), ssid: 7 };
        let b = a.encode(true, false);
        assert_eq!(b[6], 0xEE);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::frame::address_encode`
Expected: FAIL — `Address::encode` not defined.

- [ ] **Step 3: Write minimal implementation**
```rust
impl Address {
    /// Encode to the 7-byte AX.25 address field. `cr` sets bit7 (command/response
    /// or has-been-repeated); `last` sets bit0 (final address in the path).
    pub fn encode(&self, cr: bool, last: bool) -> [u8; 7] {
        let mut out = [0u8; 7];
        let call = self.call.as_bytes();
        for i in 0..6 {
            let c = if i < call.len() { call[i] } else { b' ' };
            out[i] = c << 1;
        }
        out[6] = (if cr { 0x80 } else { 0 }) | 0x60 | ((self.ssid & 0x0F) << 1) | (if last { 1 } else { 0 });
        out
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::frame::address_encode`
Expected: PASS (both tests).

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/winlink/ax25/frame.rs
git commit -m "feat(ax25): encode AX.25 address field (call+SSID, cr/last bits) (tuxlink-7fr)

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: AX.25 address — decode

**Files:**
- Modify: `src-tauri/src/winlink/ax25/frame.rs`

- [ ] **Step 1: Write the failing test**
```rust
#[cfg(test)]
mod address_decode_tests {
    use super::*;
    #[test]
    fn round_trips_encode_decode() {
        let a = Address { call: "N7CPZ".into(), ssid: 7 };
        let bytes = a.encode(false, true);
        let (decoded, cr, last) = Address::decode(&bytes).unwrap();
        assert_eq!(decoded, a);
        assert_eq!(cr, false);
        assert_eq!(last, true);
    }
    #[test]
    fn trims_trailing_spaces_from_call() {
        let bytes = Address { call: "W1AW".into(), ssid: 0 }.encode(false, false);
        let (d, _, last) = Address::decode(&bytes).unwrap();
        assert_eq!(d.call, "W1AW"); // no trailing spaces
        assert_eq!(last, false);
    }
    #[test]
    fn rejects_wrong_length() {
        assert!(Address::decode(&[0u8; 6]).is_err());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::frame::address_decode`
Expected: FAIL — `Address::decode` not defined.

- [ ] **Step 3: Write minimal implementation**
```rust
#[derive(Debug, PartialEq, Eq)]
pub enum FrameError {
    BadAddressLength,
    Truncated,
    UnknownControl(u8),
}

impl Address {
    /// Decode a 7-byte address field. Returns (address, cr_bit, last_bit).
    pub fn decode(bytes: &[u8]) -> Result<(Address, bool, bool), FrameError> {
        if bytes.len() != 7 {
            return Err(FrameError::BadAddressLength);
        }
        let mut call = String::with_capacity(6);
        for &b in &bytes[0..6] {
            let c = (b >> 1) as char;
            call.push(c);
        }
        let call = call.trim_end().to_string();
        let ssid_octet = bytes[6];
        let ssid = (ssid_octet >> 1) & 0x0F;
        let cr = ssid_octet & 0x80 != 0;
        let last = ssid_octet & 0x01 != 0;
        Ok((Address { call, ssid }, cr, last))
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::frame::address_decode`
Expected: PASS (all three).

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/winlink/ax25/frame.rs
git commit -m "feat(ax25): decode AX.25 address field with round-trip (tuxlink-7fr)

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: Control field (mod-8) — encode + decode

**Files:**
- Modify: `src-tauri/src/winlink/ax25/frame.rs`

Mod-8 control byte values (cross-check AX.25 v2.2 §4.3 + `TNCKissInterface.Frame.FrameTypes`): U-frames — SABM `0x2F`, DISC `0x43`, UA `0x63`, DM `0x0F` (P/F = bit4 `0x10`). S-frames — RR `0x01`, RNR `0x05`, REJ `0x09` with `nr<<5` and P/F bit4. I-frame — bit0=0, `ns<<1`, P/F bit4, `nr<<5`.

- [ ] **Step 1: Write the failing test**
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Control {
    Sabm { pf: bool },
    Disc { pf: bool },
    Ua { pf: bool },
    Dm { pf: bool },
    Rr { nr: u8, pf: bool },
    Rnr { nr: u8, pf: bool },
    Rej { nr: u8, pf: bool },
    I { ns: u8, nr: u8, pf: bool },
}

#[cfg(test)]
mod control_tests {
    use super::*;
    #[test]
    fn encodes_u_and_s_and_i_frames() {
        assert_eq!(Control::Sabm { pf: true }.encode(), 0x3F);   // 0x2F|0x10
        assert_eq!(Control::Ua { pf: true }.encode(), 0x73);     // 0x63|0x10
        assert_eq!(Control::Disc { pf: false }.encode(), 0x43);
        assert_eq!(Control::Dm { pf: true }.encode(), 0x1F);     // 0x0F|0x10
        assert_eq!(Control::Rr { nr: 3, pf: false }.encode(), 0x61); // (3<<5)|0x01
        assert_eq!(Control::Rej { nr: 2, pf: true }.encode(), 0x59); // (2<<5)|0x10|0x09
        assert_eq!(Control::I { ns: 2, nr: 3, pf: false }.encode(), 0x64); // (3<<5)|(2<<1)
    }
    #[test]
    fn round_trips_each_variant() {
        for c in [
            Control::Sabm { pf: true }, Control::Disc { pf: false },
            Control::Ua { pf: true }, Control::Dm { pf: false },
            Control::Rr { nr: 5, pf: true }, Control::Rnr { nr: 0, pf: false },
            Control::Rej { nr: 7, pf: false }, Control::I { ns: 1, nr: 6, pf: true },
        ] {
            assert_eq!(Control::decode(c.encode()).unwrap(), c);
        }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::frame::control`
Expected: FAIL — `Control::encode`/`decode` not defined.

- [ ] **Step 3: Write minimal implementation**
```rust
impl Control {
    pub fn encode(&self) -> u8 {
        let pf = |b: bool| if b { 0x10 } else { 0 };
        match *self {
            Control::Sabm { pf: p } => 0x2F | pf(p),
            Control::Disc { pf: p } => 0x43 | pf(p),
            Control::Ua { pf: p } => 0x63 | pf(p),
            Control::Dm { pf: p } => 0x0F | pf(p),
            Control::Rr { nr, pf: p } => (nr << 5) | pf(p) | 0x01,
            Control::Rnr { nr, pf: p } => (nr << 5) | pf(p) | 0x05,
            Control::Rej { nr, pf: p } => (nr << 5) | pf(p) | 0x09,
            Control::I { ns, nr, pf: p } => (nr << 5) | pf(p) | ((ns & 0x07) << 1),
        }
    }
    pub fn decode(b: u8) -> Result<Control, FrameError> {
        let pf = b & 0x10 != 0;
        let nr = (b >> 5) & 0x07;
        if b & 0x01 == 0 {
            // I-frame
            return Ok(Control::I { ns: (b >> 1) & 0x07, nr, pf });
        }
        if b & 0x03 == 0x01 {
            // S-frame: bits 2-3 select type
            return match b & 0x0C {
                0x00 => Ok(Control::Rr { nr, pf }),
                0x04 => Ok(Control::Rnr { nr, pf }),
                0x08 => Ok(Control::Rej { nr, pf }),
                _ => Err(FrameError::UnknownControl(b)),
            };
        }
        // U-frame: mask off the P/F bit, match the type bits
        match b & !0x10 {
            0x2F => Ok(Control::Sabm { pf }),
            0x43 => Ok(Control::Disc { pf }),
            0x63 => Ok(Control::Ua { pf }),
            0x0F => Ok(Control::Dm { pf }),
            _ => Err(FrameError::UnknownControl(b)),
        }
    }
    /// True for I and UI frames (which carry a PID + info). P1 has no UI yet.
    pub fn has_info(&self) -> bool {
        matches!(self, Control::I { .. })
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::frame::control`
Expected: PASS (both).

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/winlink/ax25/frame.rs
git commit -m "feat(ax25): mod-8 control-field encode/decode (U/S/I frames) (tuxlink-7fr)

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: Address path (dest, src, ≤2 digipeaters) — encode + decode

**Files:**
- Modify: `src-tauri/src/winlink/ax25/frame.rs`

Path order on the wire: **dest, src, then digipeaters in order**; only the *final* address has the extension (last) bit set. Digipeaters use bit7 as the "has-been-repeated" H bit (0 = not yet repeated when we transmit). Reject >2 digis (v0.1 cap). Cross-check `TNCKissInterface` path handling.

- [ ] **Step 1: Write the failing test**
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path {
    pub dest: Address,
    pub src: Address,
    pub digis: Vec<Address>, // 0..=2
}

#[cfg(test)]
mod path_tests {
    use super::*;
    #[test]
    fn encodes_direct_path_sets_last_bit_on_src() {
        let p = Path {
            dest: Address { call: "W7AUX".into(), ssid: 10 },
            src: Address { call: "N7CPZ".into(), ssid: 7 },
            digis: vec![],
        };
        let bytes = p.encode().unwrap();
        assert_eq!(bytes.len(), 14); // 2 addresses * 7
        assert_eq!(bytes[6] & 0x01, 0x00, "dest is not last");
        assert_eq!(bytes[13] & 0x01, 0x01, "src is last (direct)");
    }
    #[test]
    fn encodes_one_digi_last_bit_moves_to_digi() {
        let p = Path {
            dest: Address { call: "W7AUX".into(), ssid: 10 },
            src: Address { call: "N7CPZ".into(), ssid: 7 },
            digis: vec![Address { call: "W7RPT".into(), ssid: 1 }],
        };
        let bytes = p.encode().unwrap();
        assert_eq!(bytes.len(), 21);
        assert_eq!(bytes[13] & 0x01, 0x00, "src not last when a digi follows");
        assert_eq!(bytes[20] & 0x01, 0x01, "digi is last");
        // round-trip
        let (decoded, used) = Path::decode(&bytes).unwrap();
        assert_eq!(decoded, p);
        assert_eq!(used, 21);
    }
    #[test]
    fn rejects_more_than_two_digis() {
        let p = Path {
            dest: Address { call: "A".into(), ssid: 0 },
            src: Address { call: "B".into(), ssid: 0 },
            digis: vec![
                Address { call: "C".into(), ssid: 0 },
                Address { call: "D".into(), ssid: 0 },
                Address { call: "E".into(), ssid: 0 },
            ],
        };
        assert!(p.encode().is_err());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::frame::path`
Expected: FAIL — `Path::encode`/`decode` not defined.

- [ ] **Step 3: Write minimal implementation**
```rust
impl Path {
    pub fn encode(&self) -> Result<Vec<u8>, FrameError> {
        if self.digis.len() > 2 {
            return Err(FrameError::BadAddressLength);
        }
        let mut out = Vec::with_capacity(7 * (2 + self.digis.len()));
        // dest (cr=true for a command frame; refined in P2), src, digis.
        out.extend_from_slice(&self.dest.encode(true, false));
        let src_last = self.digis.is_empty();
        out.extend_from_slice(&self.src.encode(false, src_last));
        for (i, d) in self.digis.iter().enumerate() {
            let last = i == self.digis.len() - 1;
            out.extend_from_slice(&d.encode(false, last)); // H bit (cr) = 0 on TX
        }
        Ok(out)
    }
    /// Decode the address path, returning the path and the number of bytes consumed.
    pub fn decode(bytes: &[u8]) -> Result<(Path, usize), FrameError> {
        let mut addrs = Vec::new();
        let mut off = 0;
        loop {
            if bytes.len() < off + 7 {
                return Err(FrameError::Truncated);
            }
            let (a, _cr, last) = Address::decode(&bytes[off..off + 7])?;
            addrs.push(a);
            off += 7;
            if last {
                break;
            }
            if addrs.len() >= 4 {
                // dest + src + 2 digis max
                return Err(FrameError::BadAddressLength);
            }
        }
        if addrs.len() < 2 {
            return Err(FrameError::BadAddressLength);
        }
        let dest = addrs.remove(0);
        let src = addrs.remove(0);
        Ok((Path { dest, src, digis: addrs }, off))
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::frame::path`
Expected: PASS (all three).

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/winlink/ax25/frame.rs
git commit -m "feat(ax25): address path (dest/src/<=2 digis) encode/decode (tuxlink-7fr)

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: Full AX.25 frame — encode (no FCS; KISS/TNC adds it)

**Files:**
- Modify: `src-tauri/src/winlink/ax25/frame.rs`

A frame is `[path][control][PID 0xF0 + info]?`. PID `0xF0` (no layer 3) is present only for I-frames (P1's only info-bearing type). **No FCS** — the TNC computes it in KISS mode.

- [ ] **Step 1: Write the failing test**
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub path: Path,
    pub control: Control,
    pub info: Vec<u8>, // empty unless control.has_info()
}

#[cfg(test)]
mod frame_encode_tests {
    use super::*;
    fn sample_path() -> Path {
        Path {
            dest: Address { call: "W7AUX".into(), ssid: 10 },
            src: Address { call: "N7CPZ".into(), ssid: 7 },
            digis: vec![],
        }
    }
    #[test]
    fn sabm_has_no_pid_or_info() {
        let f = Frame { path: sample_path(), control: Control::Sabm { pf: true }, info: vec![] };
        let bytes = f.encode().unwrap();
        // 14 (path) + 1 (control) = 15, no PID, no info, no FCS.
        assert_eq!(bytes.len(), 15);
        assert_eq!(bytes[14], 0x3F); // SABM+P
    }
    #[test]
    fn i_frame_carries_pid_then_info() {
        let f = Frame {
            path: sample_path(),
            control: Control::I { ns: 0, nr: 0, pf: false },
            info: b"HELLO".to_vec(),
        };
        let bytes = f.encode().unwrap();
        assert_eq!(bytes.len(), 14 + 1 + 1 + 5);
        assert_eq!(bytes[15], 0xF0); // PID no-layer-3
        assert_eq!(&bytes[16..], b"HELLO");
    }
    #[test]
    fn rejects_info_on_non_info_frame() {
        let f = Frame { path: sample_path(), control: Control::Ua { pf: true }, info: b"x".to_vec() };
        assert!(f.encode().is_err());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::frame::frame_encode`
Expected: FAIL — `Frame::encode` not defined.

- [ ] **Step 3: Write minimal implementation**
```rust
/// PID for "no layer 3 protocol" — used for Winlink B2F payload over I-frames.
pub const PID_NO_L3: u8 = 0xF0;

impl Frame {
    pub fn encode(&self) -> Result<Vec<u8>, FrameError> {
        if !self.info.is_empty() && !self.control.has_info() {
            return Err(FrameError::UnknownControl(self.control.encode()));
        }
        let mut out = self.path.encode()?;
        out.push(self.control.encode());
        if self.control.has_info() {
            out.push(PID_NO_L3);
            out.extend_from_slice(&self.info);
        }
        Ok(out)
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::frame::frame_encode`
Expected: PASS (all three).

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/winlink/ax25/frame.rs
git commit -m "feat(ax25): full-frame encode (path+control+PID/info, no FCS) (tuxlink-7fr)

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 7: Full AX.25 frame — decode

**Files:**
- Modify: `src-tauri/src/winlink/ax25/frame.rs`

- [ ] **Step 1: Write the failing test**
```rust
#[cfg(test)]
mod frame_decode_tests {
    use super::*;
    #[test]
    fn round_trips_sabm_and_i_frame() {
        let path = Path {
            dest: Address { call: "W7AUX".into(), ssid: 10 },
            src: Address { call: "N7CPZ".into(), ssid: 7 },
            digis: vec![Address { call: "W7RPT".into(), ssid: 1 }],
        };
        for f in [
            Frame { path: path.clone(), control: Control::Sabm { pf: true }, info: vec![] },
            Frame { path: path.clone(), control: Control::I { ns: 1, nr: 2, pf: false }, info: b"B2F DATA".to_vec() },
        ] {
            let bytes = f.encode().unwrap();
            assert_eq!(Frame::decode(&bytes).unwrap(), f);
        }
    }
    #[test]
    fn rejects_truncated() {
        assert!(Frame::decode(&[0x9C, 0x6E]).is_err());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::frame::frame_decode`
Expected: FAIL — `Frame::decode` not defined.

- [ ] **Step 3: Write minimal implementation**
```rust
impl Frame {
    pub fn decode(bytes: &[u8]) -> Result<Frame, FrameError> {
        let (path, off) = Path::decode(bytes)?;
        if bytes.len() < off + 1 {
            return Err(FrameError::Truncated);
        }
        let control = Control::decode(bytes[off])?;
        let mut info = Vec::new();
        if control.has_info() {
            // skip the PID byte, take the rest as info
            if bytes.len() < off + 2 {
                return Err(FrameError::Truncated);
            }
            info = bytes[off + 2..].to_vec();
        }
        Ok(Frame { path, control, info })
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::frame::frame_decode`
Expected: PASS (both).

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/winlink/ax25/frame.rs
git commit -m "feat(ax25): full-frame decode with round-trip (tuxlink-7fr)

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 8: KISS framing — encode a data frame

**Files:**
- Modify: `src-tauri/src/winlink/ax25/kiss.rs`

KISS: frames delimited by `FEND` (0xC0). First byte after FEND = command/port nibble (data on port 0 = `0x00`). Escaping inside the body: `FEND`→`FESC TFEND` (0xDB 0xDC), `FESC`→`FESC TFESC` (0xDB 0xDD). Cross-check the KISS spec + Dire Wolf.

- [ ] **Step 1: Write the failing test**
```rust
pub const FEND: u8 = 0xC0;
pub const FESC: u8 = 0xDB;
pub const TFEND: u8 = 0xDC;
pub const TFESC: u8 = 0xDD;

#[cfg(test)]
mod kiss_encode_tests {
    use super::*;
    #[test]
    fn wraps_data_frame_with_fend_and_port_zero() {
        let out = kiss_data_frame(&[0x01, 0x02, 0x03]);
        assert_eq!(out, vec![FEND, 0x00, 0x01, 0x02, 0x03, FEND]);
    }
    #[test]
    fn escapes_fend_and_fesc_in_body() {
        let out = kiss_data_frame(&[FEND, FESC, 0xAA]);
        assert_eq!(out, vec![FEND, 0x00, FESC, TFEND, FESC, TFESC, 0xAA, FEND]);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::kiss::kiss_encode`
Expected: FAIL — `kiss_data_frame` not defined.

- [ ] **Step 3: Write minimal implementation**
```rust
/// Wrap an AX.25 frame body in a KISS data frame (port 0, command 0).
pub fn kiss_data_frame(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 4);
    out.push(FEND);
    out.push(0x00); // data frame, port 0
    for &b in body {
        match b {
            FEND => out.extend_from_slice(&[FESC, TFEND]),
            FESC => out.extend_from_slice(&[FESC, TFESC]),
            _ => out.push(b),
        }
    }
    out.push(FEND);
    out
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::kiss::kiss_encode`
Expected: PASS (both).

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/winlink/ax25/kiss.rs
git commit -m "feat(ax25): KISS data-frame encode with FEND/FESC escaping (tuxlink-7fr)

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 9: KISS framing — streaming decoder

**Files:**
- Modify: `src-tauri/src/winlink/ax25/kiss.rs`

A `KissDecoder` accumulates bytes and yields complete de-escaped data-frame bodies (stripping the port/command byte). It tolerates leading FENDs and split reads (TCP/serial deliver arbitrary chunks).

- [ ] **Step 1: Write the failing test**
```rust
#[cfg(test)]
mod kiss_decode_tests {
    use super::*;
    #[test]
    fn decodes_a_full_frame_across_two_chunks() {
        let framed = kiss_data_frame(&[FEND, FESC, 0xAA, 0xBB]);
        let (a, b) = framed.split_at(3);
        let mut d = KissDecoder::new();
        assert!(d.push(a).is_empty());
        let frames = d.push(b);
        assert_eq!(frames, vec![vec![FEND, FESC, 0xAA, 0xBB]]);
    }
    #[test]
    fn ignores_empty_frames_and_non_data_commands() {
        let mut d = KissDecoder::new();
        // FEND FEND (empty) then a param frame (cmd 0x01) then a data frame.
        let mut bytes = vec![FEND, FEND, 0x01, 0x10, FEND];
        bytes.extend(kiss_data_frame(&[0x42]));
        let frames = d.push(&bytes);
        assert_eq!(frames, vec![vec![0x42]]); // only the port-0 data frame
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::kiss::kiss_decode`
Expected: FAIL — `KissDecoder` not defined.

- [ ] **Step 3: Write minimal implementation**
```rust
/// Incremental KISS decoder. Feed it arbitrary byte chunks; it returns the
/// de-escaped bodies of any *data* frames (command/port nibble 0x00) completed
/// by that chunk. Non-data commands (param frames) and empty frames are dropped.
pub struct KissDecoder {
    buf: Vec<u8>,
    in_frame: bool,
    escaped: bool,
}

impl KissDecoder {
    pub fn new() -> Self {
        KissDecoder { buf: Vec::new(), in_frame: false, escaped: false }
    }
    pub fn push(&mut self, chunk: &[u8]) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        for &b in chunk {
            match b {
                FEND => {
                    if self.in_frame && !self.buf.is_empty() {
                        // first buffered byte is the command/port nibble
                        if self.buf[0] == 0x00 {
                            out.push(self.buf[1..].to_vec());
                        }
                    }
                    self.buf.clear();
                    self.in_frame = true;
                    self.escaped = false;
                }
                FESC if self.in_frame => self.escaped = true,
                _ if self.in_frame => {
                    let v = if self.escaped {
                        self.escaped = false;
                        match b { TFEND => FEND, TFESC => FESC, other => other }
                    } else {
                        b
                    };
                    self.buf.push(v);
                }
                _ => {} // bytes outside a frame (before the first FEND) are ignored
            }
        }
        out
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::kiss::kiss_decode`
Expected: PASS (both).

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/winlink/ax25/kiss.rs
git commit -m "feat(ax25): incremental KISS decoder (de-escape, split reads) (tuxlink-7fr)

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 10: KISS TNC parameter command frames

**Files:**
- Modify: `src-tauri/src/winlink/ax25/kiss.rs`

KISS param commands (port 0): TXDELAY=1, P(persistence)=2, SlotTime=3, TXtail=4, FullDuplex=5 — each a single value byte. Values come from the AX.25 timing config (P3); P1 just builds the frames.

- [ ] **Step 1: Write the failing test**
```rust
#[cfg(test)]
mod kiss_param_tests {
    use super::*;
    #[test]
    fn builds_txdelay_and_persistence_frames() {
        assert_eq!(kiss_param(KissParam::TxDelay, 30), vec![FEND, 0x01, 30, FEND]);
        assert_eq!(kiss_param(KissParam::Persistence, 63), vec![FEND, 0x02, 63, FEND]);
        assert_eq!(kiss_param(KissParam::SlotTime, 10), vec![FEND, 0x03, 10, FEND]);
        assert_eq!(kiss_param(KissParam::FullDuplex, 0), vec![FEND, 0x05, 0, FEND]);
    }
    #[test]
    fn value_byte_is_escaped_if_it_collides_with_fend() {
        // value 0xC0 (FEND) must be escaped in the body
        assert_eq!(kiss_param(KissParam::TxDelay, 0xC0), vec![FEND, 0x01, FESC, TFEND, FEND]);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::kiss::kiss_param`
Expected: FAIL — `kiss_param`/`KissParam` not defined.

- [ ] **Step 3: Write minimal implementation**
```rust
#[derive(Debug, Clone, Copy)]
pub enum KissParam {
    TxDelay = 0x01,
    Persistence = 0x02,
    SlotTime = 0x03,
    TxTail = 0x04,
    FullDuplex = 0x05,
}

/// Build a KISS parameter-set command frame (port 0).
pub fn kiss_param(param: KissParam, value: u8) -> Vec<u8> {
    let mut out = vec![FEND, param as u8];
    match value {
        FEND => out.extend_from_slice(&[FESC, TFEND]),
        FESC => out.extend_from_slice(&[FESC, TFESC]),
        v => out.push(v),
    }
    out.push(FEND);
    out
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ax25::kiss::kiss_param`
Expected: PASS (both).

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/winlink/ax25/kiss.rs
git commit -m "feat(ax25): KISS TNC parameter command frames (tuxlink-7fr)

Agent: sorrel-moss-hemlock
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Self-review (completed by author)

- **Spec coverage (P1 slice of §4.2–4.3):** address encode/decode ✓, control fields (mod-8 U/S/I) ✓, ≤2-digipeater path ✓ (the load-bearing digi support — Tasks 5/6/7 carry it), full-frame encode/decode ✓, KISS framing encode + streaming decode ✓, KISS param frames ✓. FCS deliberately **excluded** — the TNC owns it in KISS mode (spec §4.2); recorded as a P2 verification point. SABME/mod-128, UI/FRMR frames, and the connected-mode *state machine* are out of P1 scope (P2).
- **Placeholder scan:** none — every step has runnable code + an exact `cargo test` command + expected result.
- **Type consistency:** `Address`, `Control`, `Path`, `Frame`, `FrameError`, `KissDecoder`, `KissParam`, `kiss_data_frame`, `kiss_param`, `PID_NO_L3` are defined once and reused consistently across tasks; `Control::has_info()` (Task 4) gates PID/info in Tasks 6–7.
- **Verify-during-execution (carried from spec §9):** the exact control-byte values + address bit layout must be cross-checked against `dev/scratch/winlink-re/decompiled/tnckiss/` (`Frame.FrameTypes`, `StationAddress`) and AX.25 v2.2; the tests above encode the expected values as fixtures, so a mismatch fails loudly. C/R-bit *semantics* (command vs response framing) are stubbed here (`Path::encode` sets dest cr=true) and finalized in P2 where they matter.

## Follow-on plans (not in P1)
- **P2 — datalink state machine + KissLink transports:** the connected-mode engine (SABM/UA, I/RR/REJ, T1, MAXFRAME, PACLEN) presenting `Ax25Stream: Read+Write`, plus TCP / USB-serial / Bluetooth-serial byte-pipes (adds the `serialport` dep). Correctness-critical → cross-provider Codex round.
- **P3 — Winlink-over-packet integration:** `ExchangeRole {Dial, Answer}` in `session.rs`, `TransportConfig::Packet`, the idle-listen lifecycle, config `[packet]` + global sticky SSID. Coordinate merges with `tuxlink-686` (shared `config.rs`, `winlink_backend.rs`, `ui_commands.rs`, `lib.rs`).
- **P4 — UI:** Connections-section Packet entry, reading-pane connection panel, SSID control, ribbon/status transport+listen, session-log packet lines. Coordinate ribbon edits with `tuxlink-686`.
