//! SBC codec for the UV-Pro audio transport (tuxlink-vgvn).
//!
//! The `AudioData` wire payload is SBC (Bluetooth sub-band codec), not raw PCM.
//! DECODE uses `mini_sbc` (pure-Rust, MIT/Apache); ENCODE is a from-scratch
//! pure-Rust port (no pure-Rust SBC encoder exists on crates.io) and is the
//! remaining work — see `docs/superpowers/plans/2026-06-13-sbc-codec.md`.
//!
//! Params are the UV-Pro's fixed format (= benlink, confirmed by the golden frame
//! header `9c 71 10`): 32 kHz, MONO, 8 subbands, 16 blocks, bitpool 16, Loudness
//! allocation. The decoder is hardcoded to mono/8-subband accordingly.
//!
//! NOTE on the synthesis-filterbank delay: SBC decode introduces a fixed ~137-sample
//! group delay (mono/8-subband). Downstream SSTV decode (tuxlink-st5n, an STFT) is
//! robust to a fixed offset; tests align by that delay (see the golden-vector test).

use mini_sbc::filter_state::FilterState;
use mini_sbc::frame_decoder::FrameDecoder;
use mini_sbc::header::SBCHeader;

/// Decode a stream of UV-Pro SBC frames (mono, 8-subband) to 16-bit PCM samples.
/// A truncated trailing frame is dropped (returns what fully decoded) so a partial
/// RX chunk cannot panic the receive loop.
pub fn decode_sbc(sbc: &[u8]) -> Vec<i16> {
    let mut data: &[u8] = sbc;
    let mut filter = FilterState::<1, 8>::new();
    let mut pcm: Vec<i16> = Vec::new();
    while !data.is_empty() {
        let header = match SBCHeader::decode(&mut data) {
            Ok(h) => h,
            Err(_) => break, // not a frame start / truncated — stop
        };
        let frame = match FrameDecoder::new(&header, &mut filter, &mut data) {
            Ok(f) => f,
            Err(_) => break,
        };
        for block in frame {
            for ch in block {
                for sample in ch {
                    pcm.push(sample);
                }
            }
        }
    }
    pcm
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Golden-vector decode: the ffmpeg-encoded 1 kHz sine (UV-Pro params) decodes
    /// back to ~the source PCM. SBC's synthesis filterbank adds a fixed ~137-sample
    /// startup delay, so align ref[i] with out[i + DELAY] (a naive aligned compare
    /// reads as ~98% error — a false negative). Fixtures regenerated via
    /// `dev/tools/gen-sbc-golden-vectors.sh`. Standalone proto measured MAE 6.7.
    #[test]
    fn decodes_golden_sine_with_low_mae_after_filterbank_delay() {
        let sbc = include_bytes!("testdata/sine1k_32k_mono.sbc");
        let pcm_ref: Vec<i16> = include_bytes!("testdata/sine1k_32k_mono.pcm")
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();

        let out = decode_sbc(sbc);
        assert_eq!(out.len(), pcm_ref.len(), "sample count mismatch");

        const DELAY: usize = 137;
        let n = pcm_ref.len() - DELAY;
        let mae: f64 = (0..n)
            .map(|i| (pcm_ref[i] as f64 - out[i + DELAY] as f64).abs())
            .sum::<f64>()
            / n as f64;
        assert!(mae < 50.0, "decode MAE {mae:.1} too high (expected ~6.7)");
    }

    #[test]
    fn decode_of_garbage_does_not_panic_and_returns_empty_or_partial() {
        // A non-sync-word stream yields no frames rather than panicking.
        let _ = decode_sbc(&[0x00, 0x01, 0x02, 0x03]);
        let _ = decode_sbc(&[]);
    }
}
