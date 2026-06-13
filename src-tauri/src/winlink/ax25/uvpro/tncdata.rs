//! Benshi TNC data fragmentation (tuxlink-7my9): the `HT_SEND_DATA` / `DATA_RXD`
//! payload that carries APRS (AX.25) frames over the native GAIA connection —
//! the unified-model data path that rides the SAME Bluetooth link as control.
//!
//! One AX.25 frame is split into ordered fragments of ≤ [`MAX_FRAGMENT_DATA`]
//! data bytes (the firmware's `HT_SEND_DATA` body limit); `fragment_id`
//! increments from 0 and the last fragment is `is_final`. Inbound `DATA_RXD`
//! events are reassembled back into whole AX.25 frames by [`Reassembler`].
//!
//! Wire layout of one fragment body (MSB-first; matches the firmware bitfield,
//! verified against the decompiled official app + benlink):
//!
//! ```text
//! bit 7    : is_final_fragment
//! bit 6    : with_channel_id
//! bits 5-0 : fragment_id (0..=63)
//! then     : data bytes (the AX.25 frame slice)
//! then     : channel_id (1 byte) — ONLY if with_channel_id
//! ```
//!
//! All three units are pure (no I/O) and pinned to golden vectors, the nx95
//! testing posture. RADIO-1: this module never transmits; it only shapes bytes.

// TODO(tuxlink-7my9): the TX path (fragment_ax25 + encode_body) has no live
// caller until Task 7/8 wires UvproSession::send_aprs_frame to the native APRS
// driver; the RX path (Reassembler) is already live via the session event loop.
#![allow(dead_code)]

use super::bits::{BitReader, BitWriter};

/// Max AX.25 data bytes per `HT_SEND_DATA` fragment (firmware body limit).
pub const MAX_FRAGMENT_DATA: usize = 53;

/// One Benshi TNC data fragment: a slice of an AX.25 frame plus the reassembly
/// header. `channel_id` is `Some` iff the wire flag was set (tuxlink emits
/// `None` — the radio routes on its active channel — but inbound fragments may
/// carry one).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TncDataFragment {
    pub is_final: bool,
    pub fragment_id: u8, // 0..=63
    pub channel_id: Option<u8>,
    pub data: Vec<u8>,
}

impl TncDataFragment {
    /// Encode the fragment body (the bytes after the GAIA command header in an
    /// `HT_SEND_DATA` request, and after `event_type` in a `DATA_RXD` event).
    /// MSB-first, matching the firmware bitfield.
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

    /// Decode a fragment body. Returns `None` for an empty body (no header byte)
    /// or a `with_channel_id` body missing its trailing channel-id byte. The
    /// trailing channel-id byte (when flagged) is split off the data tail.
    pub fn decode_body(body: &[u8]) -> Option<Self> {
        if body.is_empty() {
            return None;
        }
        let mut r = BitReader::new(body);
        let is_final = r.read_bool();
        let with_channel_id = r.read_bool();
        let fragment_id = r.read_uint(6) as u8;
        // The remaining whole bytes after the 1-byte header.
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

/// Split a raw AX.25 frame into ordered fragments for `HT_SEND_DATA`.
/// `fragment_id` increments from 0; the last fragment is `is_final`. tuxlink
/// emits with no channel id (the radio uses its active channel). An empty frame
/// yields one empty final fragment (degenerate but well-formed).
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

/// Stateful reassembler for inbound `DATA_RXD` fragments. Pure (no I/O); fed by
/// the session's event loop, emits a completed AX.25 frame on the final
/// fragment. Resilient to the real RF failure modes: `fragment_id == 0` always
/// (re)starts a frame (a dropped final), and a non-contiguous id drops the
/// partial rather than splicing two frames together.
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

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Task 1: single-fragment codec (golden vectors) --------------------

    // not-final, frag 0, no channel id, data [0xDE,0xAD] -> byte0 = 0x00.
    #[test]
    fn encode_no_channel_id_golden() {
        let f = TncDataFragment { is_final: false, fragment_id: 0, channel_id: None, data: vec![0xDE, 0xAD] };
        assert_eq!(f.encode_body(), vec![0x00, 0xDE, 0xAD]);
    }

    // final, frag 5, channel id 7, data [0x01] -> byte0 = 0b11_000101 = 0xC5,
    // then data 0x01, then channel_id 0x07.
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

    #[test]
    fn decode_rejects_channel_flag_without_byte() {
        // byte0 = with_channel_id set, frag 0, no trailing byte after header.
        assert_eq!(TncDataFragment::decode_body(&[0x40]), None);
    }

    // ---- Task 2: fragmenter -------------------------------------------------

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
        let joined: Vec<u8> = frags.iter().flat_map(|f| f.data.clone()).collect();
        assert_eq!(joined, frame);
    }

    #[test]
    fn fragment_exact_multiple_last_is_final() {
        let frags = fragment_ax25(&[0xCC; 53 * 2]);
        assert_eq!(frags.len(), 2);
        assert!(frags[1].is_final);
    }

    #[test]
    fn fragment_empty_is_single_empty_final() {
        let frags = fragment_ax25(&[]);
        assert_eq!(frags.len(), 1);
        assert!(frags[0].is_final);
        assert!(frags[0].data.is_empty());
    }

    // ---- Task 3: reassembler (golden + RF-loss cases) ----------------------

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
        ra.push(&TncDataFragment { is_final: false, fragment_id: 0, channel_id: None, data: vec![9, 9] });
        assert_eq!(ra.push(&TncDataFragment { is_final: false, fragment_id: 0, channel_id: None, data: vec![1] }), None);
        assert_eq!(ra.push(&TncDataFragment { is_final: true, fragment_id: 1, channel_id: None, data: vec![2] }), Some(vec![1, 2]));
    }

    #[test]
    fn reassemble_out_of_sequence_resets_and_drops() {
        let mut ra = Reassembler::new();
        ra.push(&TncDataFragment { is_final: false, fragment_id: 0, channel_id: None, data: vec![1] });
        assert_eq!(ra.push(&TncDataFragment { is_final: true, fragment_id: 5, channel_id: None, data: vec![2] }), None);
    }

    // ---- end-to-end: fragmenter -> reassembler is transparent ---------------

    #[test]
    fn fragment_then_reassemble_roundtrips_a_long_frame() {
        let frame: Vec<u8> = (0..200u32).map(|i| (i % 251) as u8).collect();
        let mut ra = Reassembler::new();
        let mut out = None;
        for f in fragment_ax25(&frame) {
            // exercise the wire codec too: encode then decode each fragment.
            let decoded = TncDataFragment::decode_body(&f.encode_body()).unwrap();
            assert_eq!(decoded, f);
            out = ra.push(&decoded);
        }
        assert_eq!(out, Some(frame));
    }
}
