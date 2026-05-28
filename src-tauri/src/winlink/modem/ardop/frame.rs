//! Data-socket inbound frame codec for ARDOP TCP mode.
//!
//! Wire format (per wl2k-go `transport/ardop/frame.go`):
//!
//! ```text
//! [u16 BE length][3-byte type tag][payload of (length - 3) bytes]
//! ```
//!
//! `length` counts the bytes that follow the 2-byte length field itself
//! (i.e., `3-byte tag + payload`). The `+2` in wl2k-go's read:
//! `length := binary.BigEndian.Uint16(peeked) + 2` accounts for the 2
//! length bytes themselves when allocating the full wire buffer.
//!
//! Outbound on the data socket is raw bytes — the TNC frames them for TX.
//! We never encode an inbound-style frame for sending.

/// Classification of an inbound data frame based on its 3-byte type tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataKind {
    /// ARQ (connected-mode) data.
    Arq,
    /// FEC (broadcast/multicast) data.
    Fec,
    /// Error frame.
    Err,
    /// ID frame.
    Idf,
    /// Unrecognized type tag.
    Other,
}

/// A decoded inbound ARDOP data frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataFrame {
    pub kind: DataKind,
    pub payload: Vec<u8>,
}

/// Incremental inbound data-frame decoder.
///
/// Feed it arbitrary byte chunks with `push`; pull complete frames with
/// `next_frame`. Partial frames are held in an internal buffer until enough
/// bytes arrive to complete them.
#[derive(Debug, Default)]
pub struct DataDecoder {
    buf: Vec<u8>,
}

impl DataDecoder {
    /// Append `bytes` to the internal buffer.
    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Attempt to pull the next complete frame from the buffer.
    ///
    /// Returns `None` if there are not yet enough bytes for a complete frame.
    /// Consumed bytes are drained from the buffer so the next call sees only
    /// the remaining data.
    pub fn next_frame(&mut self) -> Option<DataFrame> {
        // Need at least 2 (length) + 3 (type tag) = 5 bytes to read anything.
        if self.buf.len() < 5 {
            return None;
        }
        let length = u16::from_be_bytes([self.buf[0], self.buf[1]]) as usize;
        // Defensive: a well-formed frame always has at least the 3-byte tag.
        if length < 3 {
            return None;
        }
        // Total wire bytes = 2 (length field) + length (tag + payload).
        let total = 2 + length;
        if self.buf.len() < total {
            return None; // incomplete frame — wait for more bytes
        }
        let tag = &self.buf[2..5];
        let kind = match tag {
            b"ARQ" => DataKind::Arq,
            b"FEC" => DataKind::Fec,
            b"ERR" => DataKind::Err,
            b"IDF" => DataKind::Idf,
            _ => DataKind::Other,
        };
        let payload = self.buf[5..total].to_vec();
        self.buf.drain(..total);
        Some(DataFrame { kind, payload })
    }
}

#[cfg(test)]
mod frame_tests {
    use super::*;

    fn arq_wire(payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        let length = (3 + payload.len()) as u16;
        v.extend_from_slice(&length.to_be_bytes());
        v.extend_from_slice(b"ARQ");
        v.extend_from_slice(payload);
        v
    }

    #[test]
    fn decode_arq_frame_strips_length_and_type_tag() {
        // Wire: [u16 BE length=8][ARQ][HELLO]
        // length covers the 3-byte type tag + 5-byte payload = 8.
        let wire = arq_wire(b"HELLO");
        let mut dec = DataDecoder::default();
        dec.push(&wire);
        let f = dec.next_frame().expect("a complete ARQ frame");
        assert_eq!(f.kind, DataKind::Arq);
        assert_eq!(f.payload, b"HELLO");
        assert!(dec.next_frame().is_none(), "no more frames");
    }

    #[test]
    fn decode_holds_partial_until_complete() {
        // length=8 means 10 wire bytes total (2 + 8). Feed only 5 bytes; expect None.
        let wire = arq_wire(b"HELLO"); // 10 bytes total
        assert_eq!(wire.len(), 10);
        let mut dec = DataDecoder::default();
        dec.push(&wire[..5]);
        assert!(dec.next_frame().is_none(), "5 of 10 wire bytes -> incomplete");
        dec.push(&wire[5..]);
        let f = dec.next_frame().expect("complete now");
        assert_eq!(f.payload, b"HELLO");
    }

    #[test]
    fn decode_distinguishes_arq_fec_err_idf() {
        for (tag, expected) in [
            (b"ARQ" as &[u8], DataKind::Arq),
            (b"FEC", DataKind::Fec),
            (b"ERR", DataKind::Err),
            (b"IDF", DataKind::Idf),
        ] {
            let mut wire = Vec::new();
            wire.extend_from_slice(&3u16.to_be_bytes()); // length=3, empty payload
            wire.extend_from_slice(tag);
            let mut dec = DataDecoder::default();
            dec.push(&wire);
            let f = dec.next_frame().expect("complete");
            assert_eq!(f.kind, expected, "tag {:?}", std::str::from_utf8(tag));
            assert!(f.payload.is_empty());
        }
    }

    #[test]
    fn decode_yields_multiple_frames_from_one_push() {
        let mut wire = Vec::new();
        for payload in [b"AA" as &[u8], b"BBB", b"CCCC"] {
            wire.extend_from_slice(&arq_wire(payload));
        }
        let mut dec = DataDecoder::default();
        dec.push(&wire);
        let mut payloads = Vec::new();
        while let Some(f) = dec.next_frame() {
            payloads.push(f.payload);
        }
        assert_eq!(
            payloads,
            vec![b"AA".to_vec(), b"BBB".to_vec(), b"CCCC".to_vec()]
        );
    }

    #[test]
    fn decode_other_kind_for_unknown_tag() {
        let mut wire = Vec::new();
        wire.extend_from_slice(&3u16.to_be_bytes());
        wire.extend_from_slice(b"XYZ");
        let mut dec = DataDecoder::default();
        dec.push(&wire);
        let f = dec.next_frame().expect("complete");
        assert_eq!(f.kind, DataKind::Other);
    }

    #[test]
    fn decode_byte_by_byte_eventually_yields_frame() {
        let wire = arq_wire(b"HI");
        let mut dec = DataDecoder::default();
        let mut got = None;
        for &byte in &wire {
            dec.push(&[byte]);
            if let Some(f) = dec.next_frame() {
                got = Some(f);
                break;
            }
        }
        let f = got.expect("frame after byte-by-byte feed");
        assert_eq!(f.kind, DataKind::Arq);
        assert_eq!(f.payload, b"HI");
    }
}
