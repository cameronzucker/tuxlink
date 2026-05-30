//! KISS framing: wraps AX.25 frames for delivery to a TNC over serial/TCP.
//! KISS spec: FEND (0xC0) delimiters; FEND/FESC escaping inside the body.

pub const FEND: u8 = 0xC0;
pub const FESC: u8 = 0xDB;
pub const TFEND: u8 = 0xDC;
pub const TFESC: u8 = 0xDD;

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

/// Incremental KISS decoder. Feed it arbitrary byte chunks; it returns the
/// de-escaped bodies of any *data* frames (command/port nibble 0x00) completed
/// by that chunk. Non-data commands (param frames) and empty frames are dropped.
pub struct KissDecoder {
    buf: Vec<u8>,
    in_frame: bool,
    escaped: bool,
}

impl Default for KissDecoder {
    fn default() -> Self {
        Self::new()
    }
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
                        // The first buffered byte is the KISS command byte: low nibble =
                        // command (0 = data), high nibble = TNC port. A data frame from
                        // ANY port has low-nibble 0 (tuxlink-2y4) — accept 0x00, 0x10,
                        // 0x20, … not only the literal 0x00.
                        if self.buf[0] & 0x0f == 0 {
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

#[cfg(test)]
mod kiss_decode_tests {
    use super::*;

    // tuxlink-2y4: KISS data frames carry command 0x00 in the LOW nibble; the HIGH
    // nibble is the TNC port. A multi-port TNC may emit a data frame on port 1+
    // (command byte 0x10, 0x20, …). The decoder must accept any (byte & 0x0f) == 0,
    // not only the literal 0x00, or it silently drops a valid inbound frame (RX bug).
    #[test]
    fn decoder_accepts_a_data_frame_from_a_nonzero_kiss_port() {
        let mut dec = KissDecoder::new();
        // FEND, cmd 0x10 (port 1, type 0 = data), payload AB CD, FEND
        let frames = dec.push(&[FEND, 0x10, 0xAB, 0xCD, FEND]);
        assert_eq!(frames, vec![vec![0xAB, 0xCD]], "a port-1 data frame must decode");
    }

    // tuxlink-2y4 guard: a non-data KISS command (low nibble != 0, e.g. a SET-param
    // command 0x01) is still NOT a data frame and must be dropped.
    #[test]
    fn decoder_still_drops_a_non_data_command_frame() {
        let mut dec = KissDecoder::new();
        let frames = dec.push(&[FEND, 0x01, 0xAB, FEND]); // low nibble 1 => not data
        assert!(frames.is_empty(), "a non-data KISS command must not decode as a data frame");
    }

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
