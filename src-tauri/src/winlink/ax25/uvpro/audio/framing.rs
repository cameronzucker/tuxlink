//! UV-Pro audio-channel framing (SSTV transport, tuxlink-bcsy).
//!
//! The audio RFCOMM channel carries HDLC-style framed messages, distinct from the
//! GAIA control channel's `ff 01` framing (see `gaia.rs`). Frame layout:
//! `0x7e` ++ escape(type_byte ++ payload) ++ `0x7e`, where `0x7d`/`0x7e` in the
//! escaped region are stuffed as `0x7d` then `byte ^ 0x20`. Verified byte-for-byte
//! against benlink's `protocol/audio.py` (sanctioned RE source; see the bd
//! tuxlink-bcsy notes for the full transport spec).
//!
//! RADIO-1 / ADR 0018: framing is non-transmitting pure byte math; the transmit
//! gate lives at the transport/session layer. This module never touches hardware.

const DELIM: u8 = 0x7e;
const ESC: u8 = 0x7d;
const ESC_XOR: u8 = 0x20;

/// A decoded audio-channel message. The SBC payload of [`AudioMessage::Data`] is
/// opaque here — the codec (a sibling sub-project) produces/consumes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioMessage {
    /// `0x00` — one chunk of opaque (SBC) audio payload.
    Data(Vec<u8>),
    /// `0x01` — transmit finished / de-key. Transmitted with an 8-byte zero pad to
    /// byte-match the vendor app; on RX the pad is ignored (only the type byte
    /// determines the message).
    End,
    /// `0x02` — acknowledgement. The radio does not ack in the normal send loop;
    /// kept for completeness and RX tolerance.
    Ack,
    /// Any other type byte, preserved for diagnostics.
    Unknown(u8, Vec<u8>),
}

/// Append `payload` to `out`, byte-stuffing the delimiter/escape sentinels.
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
    /// Encode to on-wire bytes: `0x7e` ++ escape(type ++ payload) ++ `0x7e`.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut inner = Vec::new();
        match self {
            AudioMessage::Data(p) => {
                inner.push(0x00);
                inner.extend_from_slice(p);
            }
            // benlink transmits End/Ack with an 8-byte zero pad; we match it so our
            // frames are byte-identical to the vendor app's.
            AudioMessage::End => {
                inner.push(0x01);
                inner.extend_from_slice(&[0u8; 8]);
            }
            AudioMessage::Ack => {
                inner.push(0x02);
                inner.extend_from_slice(&[0u8; 8]);
            }
            AudioMessage::Unknown(t, d) => {
                inner.push(*t);
                inner.extend_from_slice(d);
            }
        }
        let mut out = Vec::with_capacity(inner.len() + 2);
        out.push(DELIM);
        escape_into(&mut out, &inner);
        out.push(DELIM);
        out
    }

    /// Decode an already-unescaped frame body (type byte + payload).
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

/// Streaming deframer: feed it arbitrary RFCOMM read chunks; it yields every
/// complete message now available and retains any partial tail across calls.
/// Bounds the in-flight frame against a wedged/garbage peer that never sends a
/// closing delimiter.
#[derive(Default)]
pub struct AudioDeframer {
    /// True once an opening delimiter has been seen and we are accumulating a body.
    in_frame: bool,
    /// The escaped body bytes between the opening and closing delimiters.
    frame: Vec<u8>,
}

impl AudioDeframer {
    /// Far larger than any real audio frame (SBC frames are tens-to-hundreds of
    /// bytes); bounds a wedged or hostile peer that never terminates a frame.
    pub const MAX_BUFFER: usize = 8192;

    pub fn new() -> Self {
        Self::default()
    }

    /// In-flight (un-yielded) byte count — the partial frame body, if any.
    pub fn buffered_len(&self) -> usize {
        self.frame.len()
    }

    /// Append `data` and return every complete message now decodable.
    pub fn push(&mut self, data: &[u8]) -> Vec<AudioMessage> {
        let mut out = Vec::new();
        for &b in data {
            if b == DELIM {
                if self.in_frame {
                    // Closing delimiter: unescape + decode the accumulated body.
                    // An empty body (`7e 7e`) or dangling escape decodes to None.
                    if let Some(msg) = decode_body(&self.frame) {
                        out.push(msg);
                    }
                    self.frame.clear();
                    self.in_frame = false;
                } else {
                    // Opening delimiter: begin a fresh frame; any bytes seen before
                    // it were inter-frame garbage and are already discarded.
                    self.in_frame = true;
                    self.frame.clear();
                }
            } else if self.in_frame {
                self.frame.push(b);
                if self.frame.len() > Self::MAX_BUFFER {
                    // Unterminated runaway: abandon this frame and resync on the
                    // next delimiter rather than growing without bound.
                    self.frame.clear();
                    self.in_frame = false;
                }
            }
            // else: byte outside any frame (pre-first-delimiter garbage) — discard.
        }
        out
    }
}

/// Unescape `0x7d`-stuffed bytes then decode the type byte. A dangling escape (a
/// trailing `0x7d` with no following byte) or an empty body yields `None` so a
/// single corrupt frame is dropped rather than crashing the loop.
fn decode_body(escaped: &[u8]) -> Option<AudioMessage> {
    let mut inner = Vec::with_capacity(escaped.len());
    let mut i = 0;
    while i < escaped.len() {
        if escaped[i] == ESC {
            i += 1;
            if i >= escaped.len() {
                return None; // dangling escape → drop frame
            }
            inner.push(escaped[i] ^ ESC_XOR);
        } else {
            inner.push(escaped[i]);
        }
        i += 1;
    }
    AudioMessage::from_unescaped(&inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse space-separated hex into bytes (test helper).
    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace()
            .map(|h| u8::from_str_radix(h, 16).unwrap())
            .collect()
    }

    #[test]
    fn audio_data_to_bytes_matches_golden() {
        assert_eq!(AudioMessage::Data(vec![0xAB, 0xCD]).to_bytes(), hex("7e 00 ab cd 7e"));
    }

    #[test]
    fn audio_data_escapes_delimiter_and_escape_bytes() {
        assert_eq!(AudioMessage::Data(vec![0x7e]).to_bytes(), hex("7e 00 7d 5e 7e"));
        assert_eq!(AudioMessage::Data(vec![0x7d]).to_bytes(), hex("7e 00 7d 5d 7e"));
        assert_eq!(
            AudioMessage::Data(vec![0x7d, 0x7e]).to_bytes(),
            hex("7e 00 7d 5d 7d 5e 7e")
        );
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
        assert_eq!(
            d.push(&hex("7e 01 00 00 00 00 00 00 00 00 7e")),
            vec![AudioMessage::End]
        ); // padded
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
    fn deframer_drops_empty_and_dangling_escape_frames() {
        let mut d = AudioDeframer::new();
        assert!(d.push(&hex("7e 7e")).is_empty()); // empty body → None
        let mut d2 = AudioDeframer::new();
        assert!(d2.push(&hex("7e 7d")).is_empty()); // dangling escape, no close yet
        assert!(d2.push(&hex("7e")).is_empty()); // closes with dangling escape → None
    }

    #[test]
    fn deframer_bounds_buffer_against_unterminated_garbage() {
        let mut d = AudioDeframer::new();
        // A long run with no closing delimiter (and no delimiter byte) must not grow
        // the in-flight frame without bound.
        let mut junk = vec![0x7eu8]; // open a frame
        junk.extend(std::iter::repeat(0x00u8).take(AudioDeframer::MAX_BUFFER + 100));
        let _ = d.push(&junk);
        assert!(d.buffered_len() <= AudioDeframer::MAX_BUFFER);
    }

    #[test]
    fn unknown_type_is_preserved() {
        let mut d = AudioDeframer::new();
        // type 0x05 (SET_SIGN_DATA in the c1 enum) with a payload byte.
        assert_eq!(d.push(&hex("7e 05 99 7e")), vec![AudioMessage::Unknown(0x05, vec![0x99])]);
    }
}
