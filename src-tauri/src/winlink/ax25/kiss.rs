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
