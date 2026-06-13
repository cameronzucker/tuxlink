//! SBC codec seam (tuxlink-bcsy).
//!
//! The audio wire payload is SBC (Bluetooth sub-band codec), not raw PCM. A real
//! pure-Rust encoder/decoder is a sibling sub-project (crates.io has no mature
//! pure-Rust SBC *encoder* — `mini_sbc` is decode-only, `libsbc` is a C-FFI
//! binding); the transport depends ONLY on this trait so it is fully unit-testable
//! with fakes, mirroring how `aprs/native_driver.rs` tests against a recording
//! `AprsFrameTx` fake. The transport never sees real SBC bytes in tests.

use std::sync::Mutex;

/// Encode 32 kHz mono s16le PCM ⇄ SBC payload bytes (the opaque body of an
/// [`super::framing::AudioMessage::Data`] frame).
///
/// Frame-boundary semantics — how many PCM samples map to one `encode` call — are
/// the codec's concern (SBC has a fixed samples-per-frame given the subband/block
/// params); the transport hands whole PCM chunks and ships whatever bytes come
/// back. Both directions are infallible at this layer: a malformed SBC frame on
/// `decode` yields empty PCM rather than an error, so one corrupt RX frame cannot
/// kill the receive loop.
pub trait SbcCodec: Send + Sync {
    fn encode(&self, pcm: &[u8]) -> Vec<u8>;
    fn decode(&self, sbc: &[u8]) -> Vec<u8>;
}

/// Identity passthrough — lets transport tests assert framing/pacing without a real
/// codec. (Not used in production; the real codec replaces it via injection.)
pub struct NullSbcCodec;

impl SbcCodec for NullSbcCodec {
    fn encode(&self, pcm: &[u8]) -> Vec<u8> {
        pcm.to_vec()
    }
    fn decode(&self, sbc: &[u8]) -> Vec<u8> {
        sbc.to_vec()
    }
}

/// Records `encode` inputs so transport tests can assert what PCM was handed to the
/// codec (and in what chunking). Behaves as identity otherwise.
#[derive(Default)]
pub struct RecordingSbcCodec {
    encoded: Mutex<Vec<Vec<u8>>>,
}

impl RecordingSbcCodec {
    pub fn encoded_inputs(&self) -> Vec<Vec<u8>> {
        self.encoded.lock().unwrap().clone()
    }
}

impl SbcCodec for RecordingSbcCodec {
    fn encode(&self, pcm: &[u8]) -> Vec<u8> {
        self.encoded.lock().unwrap().push(pcm.to_vec());
        pcm.to_vec()
    }
    fn decode(&self, sbc: &[u8]) -> Vec<u8> {
        sbc.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let _ = c.encode(&[7]);
        assert_eq!(c.encoded_inputs(), vec![vec![9u8, 9], vec![7u8]]);
    }
}
