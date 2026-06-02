//! AX.25 frame structures: address, path, control field, and full frame.
//! KISS invariant: no FCS here — the TNC/modem owns FCS, flags, and bit-stuffing.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    pub call: String, // base callsign, ≤6 chars, uppercased
    pub ssid: u8,     // 0–15
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    BadAddressLength,
    Truncated,
    UnknownControl(u8),
    /// `Frame::encode` was called with `info` bytes on a frame whose control
    /// field does not carry info (e.g. SABM, UA, RR). The control byte is valid;
    /// this is a caller usage-contract violation, not an unknown control type.
    InfoOnNonInfoFrame,
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::BadAddressLength => {
                write!(f, "AX.25 address field must be exactly 7 bytes (or a path must have 2–4 addresses)")
            }
            FrameError::Truncated => {
                write!(f, "AX.25 frame is truncated — not enough bytes to decode")
            }
            FrameError::UnknownControl(b) => {
                write!(f, "unknown AX.25 control byte: 0x{b:02X}")
            }
            FrameError::InfoOnNonInfoFrame => {
                write!(f, "info bytes supplied for a non-info frame (I/UI only carry info)")
            }
        }
    }
}

impl std::error::Error for FrameError {}

impl Address {
    /// Encode to the 7-byte AX.25 address field. `cr` sets bit7 (command/response
    /// or has-been-repeated); `last` sets bit0 (final address in the path).
    pub fn encode(&self, cr: bool, last: bool) -> [u8; 7] {
        let mut out = [0u8; 7];
        let call = self.call.as_bytes();
        for i in 0..6 {
            let c = if i < call.len() { call[i] } else { b' ' };
            out[i] = c.wrapping_shl(1);
        }
        out[6] = (if cr { 0x80 } else { 0 }) | 0x60 | ((self.ssid & 0x0F) << 1) | (if last { 1 } else { 0 });
        out
    }

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

#[cfg(test)]
mod address_decode_tests {
    use super::*;
    #[test]
    fn round_trips_encode_decode() {
        let a = Address { call: "N7CPZ".into(), ssid: 7 };
        let bytes = a.encode(false, true);
        let (decoded, cr, last) = Address::decode(&bytes).unwrap();
        assert_eq!(decoded, a);
        assert!(!cr);
        assert!(last);
    }
    #[test]
    fn trims_trailing_spaces_from_call() {
        let bytes = Address { call: "W1AW".into(), ssid: 0 }.encode(false, false);
        let (d, _, last) = Address::decode(&bytes).unwrap();
        assert_eq!(d.call, "W1AW"); // no trailing spaces
        assert!(!last);
    }
    #[test]
    fn rejects_wrong_length() {
        assert!(Address::decode(&[0u8; 6]).is_err());
    }
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

// ── Control field (mod-8) ─────────────────────────────────────────────────────

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

impl Control {
    pub fn encode(&self) -> u8 {
        let pf = |b: bool| if b { 0x10 } else { 0 };
        match *self {
            Control::Sabm { pf: p } => 0x2F | pf(p),
            Control::Disc { pf: p } => 0x43 | pf(p),
            Control::Ua { pf: p } => 0x63 | pf(p),
            Control::Dm { pf: p } => 0x0F | pf(p),
            Control::Rr { nr, pf: p } => ((nr & 0x07) << 5) | pf(p) | 0x01,
            Control::Rnr { nr, pf: p } => ((nr & 0x07) << 5) | pf(p) | 0x05,
            Control::Rej { nr, pf: p } => ((nr & 0x07) << 5) | pf(p) | 0x09,
            Control::I { ns, nr, pf: p } => ((nr & 0x07) << 5) | pf(p) | ((ns & 0x07) << 1),
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

    /// The C/R bit this control type encodes with by default, per AX.25 v2.2 §2.4.1.2
    /// (tuxlink-b0i). `Frame::encode_as_command` / `Frame::encode_as_response`
    /// override this when a future caller needs to flip the polarity (e.g., a T1
    /// enquiry RR poll); the default `Frame::encode` uses the value here.
    ///
    /// * **`SABM` / `DISC` / `I`** — always commands; v2.2 fixes dest C-bit=1, src
    ///   C-bit=0.
    /// * **`UA` / `DM`** — always responses; v2.2 fixes dest C-bit=0, src C-bit=1.
    /// * **`RR` / `RNR` / `REJ`** — context-dependent. In tuxlink's connected-mode
    ///   driver these are sent exclusively in response to inbound I-frames
    ///   (`accept_inbound_i`'s RR ack and REJ for out-of-order) — never as
    ///   T1-timeout-triggered enquiry polls — so the default is `false` (response).
    ///   When a T1-enquiry path is added, that send site must call
    ///   `Frame::encode_as_command()` to override.
    pub fn cr_command_default(&self) -> bool {
        match *self {
            Control::Sabm { .. } | Control::Disc { .. } | Control::I { .. } => true,
            Control::Ua { .. } | Control::Dm { .. } => false,
            Control::Rr { .. } | Control::Rnr { .. } | Control::Rej { .. } => false,
        }
    }
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

// ── Address Path ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path {
    pub dest: Address,
    pub src: Address,
    pub digis: Vec<Address>, // 0..=2
}

impl Path {
    /// Encode the address path with C/R bits set per AX.25 v2.2 §2.4.1.2 (tuxlink-b0i).
    ///
    /// `cr_command=true` means COMMAND framing: dest C-bit=1, src C-bit=0
    /// (SABM/DISC/I; an enquiry-poll RR/RNR/REJ if one were emitted).
    /// `cr_command=false` means RESPONSE framing: dest C-bit=0, src C-bit=1
    /// (UA/DM; the RR/REJ acks `accept_inbound_i` emits).
    ///
    /// A strict v2.2 decoder (RMS gateway, BPQ, Direwolf) rejects or mis-handles a
    /// frame whose dest+src C-bits don't match its semantic role; the previous
    /// hardcoded `dest=1, src=0` (always command) shipped responses with command
    /// bits — visible only against an independent decoder, never against tuxlink's
    /// own loopback. Callers don't pick this bit directly: `Frame::encode` derives
    /// it from `Control::cr_command_default()` so each control type encodes per
    /// spec automatically.
    pub fn encode(&self, cr_command: bool) -> Result<Vec<u8>, FrameError> {
        if self.digis.len() > 2 {
            return Err(FrameError::BadAddressLength);
        }
        let mut out = Vec::with_capacity(7 * (2 + self.digis.len()));
        // dest + src C-bits are complements per v2.2 — command has dest=1, src=0;
        // response has dest=0, src=1. v2.0 had both 0; a v2.2 decoder still accepts
        // a v2.0-bit frame but distinguishes command/response by these bits.
        out.extend_from_slice(&self.dest.encode(cr_command, false));
        let src_last = self.digis.is_empty();
        out.extend_from_slice(&self.src.encode(!cr_command, src_last));
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
        let bytes = p.encode(true).unwrap();
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
        let bytes = p.encode(true).unwrap();
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
        assert!(p.encode(true).is_err());
    }

    // tuxlink-b0i / AX.25 v2.2 §2.4.1.2 conformance.
    //
    // Command framing: dest C-bit=1, src C-bit=0. The C-bit is bit 7 of the SSID
    // octet (offset 6 of each 7-byte address). For a direct path, dest is at
    // bytes 0..7 and src at 7..14, so the C-bits are bytes[6]&0x80 and bytes[13]&0x80.
    #[test]
    fn path_encode_command_sets_dest_c1_src_c0() {
        let p = Path {
            dest: Address { call: "W7AUX".into(), ssid: 10 },
            src: Address { call: "N7CPZ".into(), ssid: 7 },
            digis: vec![],
        };
        let bytes = p.encode(/*cr_command=*/ true).unwrap();
        assert_eq!(bytes[6] & 0x80, 0x80, "command: dest C-bit must be 1");
        assert_eq!(bytes[13] & 0x80, 0x00, "command: src C-bit must be 0");
    }
    // Response framing: dest C-bit=0, src C-bit=1. The bug closed by tuxlink-b0i:
    // before the fix Path::encode hardcoded dest=1, src=0 for every frame, so a
    // UA/DM (always-response per v2.2) shipped with command C-bits — a strict
    // v2.2 decoder (RMS gateway, BPQ, Direwolf) may reject or mis-handle that.
    #[test]
    fn path_encode_response_sets_dest_c0_src_c1() {
        let p = Path {
            dest: Address { call: "W7AUX".into(), ssid: 10 },
            src: Address { call: "N7CPZ".into(), ssid: 7 },
            digis: vec![],
        };
        let bytes = p.encode(/*cr_command=*/ false).unwrap();
        assert_eq!(bytes[6] & 0x80, 0x00, "response: dest C-bit must be 0");
        assert_eq!(bytes[13] & 0x80, 0x80, "response: src C-bit must be 1");
    }
    // The digipeater H bits are always 0 on TX regardless of command/response —
    // the C-bit logic applies to dest and src only. (Once a digi repeats the
    // frame, that digi sets its OWN H bit to 1; we never repeat, so always 0.)
    #[test]
    fn path_encode_digipeater_h_bit_is_zero_regardless_of_command_response() {
        let p = Path {
            dest: Address { call: "W7AUX".into(), ssid: 10 },
            src: Address { call: "N7CPZ".into(), ssid: 7 },
            digis: vec![Address { call: "W7RPT".into(), ssid: 1 }],
        };
        for cr_command in [true, false] {
            let bytes = p.encode(cr_command).unwrap();
            assert_eq!(
                bytes[20] & 0x80,
                0x00,
                "digi H-bit must be 0 on TX (cr_command={cr_command})"
            );
        }
    }
}

// ── Full Frame ────────────────────────────────────────────────────────────────

/// An AX.25 connected-mode frame (address path + control + optional info).
///
/// **P1 PID assumption — Winlink B2F only (0xF0 / no layer 3):**
/// Every Winlink B2F I-frame uses PID 0xF0 ("no layer 3"), so `decode` discards
/// the PID byte from the wire (it is not retained in this struct) and `encode`
/// always emits 0xF0. Frames that carry a non-0xF0 PID will decode without error
/// but the PID is silently dropped; re-encoding will emit 0xF0. If non-Winlink
/// frames that must round-trip PID values are needed in a future phase, add a
/// `pid: u8` field and update `decode`/`encode` accordingly — deferred to P2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub path: Path,
    pub control: Control,
    pub info: Vec<u8>, // empty unless control.has_info()
}

/// PID for "no layer 3 protocol" — used for Winlink B2F payload over I-frames.
pub const PID_NO_L3: u8 = 0xF0;

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

    /// Encode the frame to its on-wire bytes with C/R bits derived automatically
    /// from `self.control` (tuxlink-b0i). The default route for every send site;
    /// callers needing to force a non-default C/R polarity (e.g., a future T1
    /// enquiry that sends RR as a command-poll instead of an ack-response) call
    /// [`Frame::encode_as_command`] or [`Frame::encode_as_response`] instead.
    pub fn encode(&self) -> Result<Vec<u8>, FrameError> {
        self.encode_with_cr(self.control.cr_command_default())
    }

    /// Encode the frame with COMMAND C/R bits (dest=1, src=0). Use for the future
    /// T1-enquiry RR-poll path that tuxlink does not yet implement; current
    /// production sites all use [`Frame::encode`] and get the correct polarity
    /// from the control type's default.
    pub fn encode_as_command(&self) -> Result<Vec<u8>, FrameError> {
        self.encode_with_cr(true)
    }

    /// Encode the frame with RESPONSE C/R bits (dest=0, src=1). For
    /// SABM/DISC/I-frames this would mis-frame a command as a response; provided
    /// for completeness/symmetry with [`Frame::encode_as_command`]. Current
    /// production sites all use [`Frame::encode`].
    pub fn encode_as_response(&self) -> Result<Vec<u8>, FrameError> {
        self.encode_with_cr(false)
    }

    fn encode_with_cr(&self, cr_command: bool) -> Result<Vec<u8>, FrameError> {
        if !self.info.is_empty() && !self.control.has_info() {
            return Err(FrameError::InfoOnNonInfoFrame);
        }
        let mut out = self.path.encode(cr_command)?;
        out.push(self.control.encode());
        if self.control.has_info() {
            out.push(PID_NO_L3);
            out.extend_from_slice(&self.info);
        }
        Ok(out)
    }
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

    // tuxlink-b0i / AX.25 v2.2 §2.4.1.2.
    //
    // For a direct (no-digi) path, the dest C-bit is bytes[6]&0x80 and the src
    // C-bit is bytes[13]&0x80. Frame::encode picks the polarity from
    // Control::cr_command_default() — these tests pin the per-control-type
    // outcome that closes the bug where every frame shipped with command bits.

    /// Command frames (SABM/DISC/I): dest C=1, src C=0 per v2.2 §2.4.1.2.
    /// SABM was correct under the old code (cr=command was the right default);
    /// this is the regression-pin so a future refactor doesn't flip the default.
    #[test]
    fn sabm_encodes_with_command_cr_bits() {
        let f = Frame { path: sample_path(), control: Control::Sabm { pf: true }, info: vec![] };
        let bytes = f.encode().unwrap();
        assert_eq!(bytes[6] & 0x80, 0x80, "SABM: dest C-bit must be 1 (command)");
        assert_eq!(bytes[13] & 0x80, 0x00, "SABM: src C-bit must be 0 (command)");
    }
    #[test]
    fn disc_encodes_with_command_cr_bits() {
        let f = Frame { path: sample_path(), control: Control::Disc { pf: true }, info: vec![] };
        let bytes = f.encode().unwrap();
        assert_eq!(bytes[6] & 0x80, 0x80, "DISC: dest C-bit must be 1 (command)");
        assert_eq!(bytes[13] & 0x80, 0x00, "DISC: src C-bit must be 0 (command)");
    }
    #[test]
    fn i_frame_encodes_with_command_cr_bits() {
        let f = Frame {
            path: sample_path(),
            control: Control::I { ns: 0, nr: 0, pf: false },
            info: b"HELLO".to_vec(),
        };
        let bytes = f.encode().unwrap();
        assert_eq!(bytes[6] & 0x80, 0x80, "I: dest C-bit must be 1 (command)");
        assert_eq!(bytes[13] & 0x80, 0x00, "I: src C-bit must be 0 (command)");
    }

    /// Response frames (UA/DM): dest C=0, src C=1 per v2.2 §2.4.1.2. This is the
    /// regression the bug describes — the LISTEN/answer path sent UA with command
    /// C-bits, breaking against a strict v2.2 decoder.
    #[test]
    fn ua_encodes_with_response_cr_bits() {
        let f = Frame { path: sample_path(), control: Control::Ua { pf: true }, info: vec![] };
        let bytes = f.encode().unwrap();
        assert_eq!(bytes[6] & 0x80, 0x00, "UA: dest C-bit must be 0 (response)");
        assert_eq!(bytes[13] & 0x80, 0x80, "UA: src C-bit must be 1 (response)");
    }
    #[test]
    fn dm_encodes_with_response_cr_bits() {
        let f = Frame { path: sample_path(), control: Control::Dm { pf: true }, info: vec![] };
        let bytes = f.encode().unwrap();
        assert_eq!(bytes[6] & 0x80, 0x00, "DM: dest C-bit must be 0 (response)");
        assert_eq!(bytes[13] & 0x80, 0x80, "DM: src C-bit must be 1 (response)");
    }

    /// Context-dependent S-frames (RR/RNR/REJ): in tuxlink's connected-mode driver
    /// these are sent only as ACKs from `accept_inbound_i` (RR for in-order,
    /// REJ for out-of-order), so the default is response (dest=0, src=1) — the
    /// other half of the bug from sustained-exchange acks.
    #[test]
    fn rr_encodes_with_response_cr_bits_by_default() {
        let f = Frame { path: sample_path(), control: Control::Rr { nr: 1, pf: false }, info: vec![] };
        let bytes = f.encode().unwrap();
        assert_eq!(bytes[6] & 0x80, 0x00, "RR-ack: dest C-bit must be 0 (response)");
        assert_eq!(bytes[13] & 0x80, 0x80, "RR-ack: src C-bit must be 1 (response)");
    }
    #[test]
    fn rej_encodes_with_response_cr_bits_by_default() {
        let f = Frame { path: sample_path(), control: Control::Rej { nr: 0, pf: false }, info: vec![] };
        let bytes = f.encode().unwrap();
        assert_eq!(bytes[6] & 0x80, 0x00, "REJ-ack: dest C-bit must be 0 (response)");
        assert_eq!(bytes[13] & 0x80, 0x80, "REJ-ack: src C-bit must be 1 (response)");
    }
    /// The override path — `encode_as_command` — is the future T1-enquiry hook so
    /// a poll-RR ships with command C-bits. tuxlink doesn't emit one today, but
    /// the encoder must honor the override.
    #[test]
    fn rr_encode_as_command_overrides_to_command_cr_bits() {
        let f = Frame { path: sample_path(), control: Control::Rr { nr: 1, pf: true }, info: vec![] };
        let bytes = f.encode_as_command().unwrap();
        assert_eq!(bytes[6] & 0x80, 0x80, "RR-poll: dest C-bit must be 1 (command)");
        assert_eq!(bytes[13] & 0x80, 0x00, "RR-poll: src C-bit must be 0 (command)");
    }
}

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
