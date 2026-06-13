//! Pure-Rust SSTV PCM<->image codec (bd tuxlink-st5n), ported 1:1 from
//! HTCommander's port of <https://github.com/xdsopl/robot36>.
//!
//! Pipeline: image (packed ARGB) -> [`encoder::Encoder`] -> 32 kHz mono `f32`
//! PCM -> radio. RX mirrors it: PCM -> [`decoder::Decoder`] -> image.
//!
//! NOTE: the bd issue described decode as "STFT decode", but the actual
//! xdsopl/HTCommander decode path is a *time-domain FM discriminator* with
//! sync-pulse detection. The `ShortTimeFourierTransform`/`FastFourierTransform`
//! C# classes are referenced only by each other and never by the decode path,
//! so they are not ported.

pub mod color;
pub mod decoder;
pub mod demodulator;
pub mod dsp;
pub mod encoder;
pub mod modes;

pub use decoder::Decoder;
pub use encoder::{Encoder, PdMode};

pub const DEFAULT_SAMPLE_RATE: u32 = 32_000;

/// Convert `f32` PCM (-1..1) to 16-bit signed little-endian bytes.
pub fn f32_to_pcm16(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// Convert 16-bit signed little-endian PCM bytes to `f32` (-1..1).
pub fn pcm16_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
        .collect()
}

/// Decode a full image from a complete sample stream by feeding it through the
/// decoder in small chunks (mirrors the streaming/RFCOMM delivery). Returns
/// `(pixels, width, height, mode_name)` once a complete image is decoded.
pub fn decode_full(
    samples: &[f32],
    sample_rate: u32,
    chunk: usize,
) -> Option<(Vec<u32>, usize, usize, String)> {
    let mut dec = Decoder::new(sample_rate);
    let mut buf = vec![0.0f32; chunk];
    // Pad with trailing silence: the decoder flushes the final scan line via
    // its timeout branch, which needs ~1.25 scan lines of samples *after* the
    // last sync pulse. A real radio path supplies this as post-TX silence.
    let pad = sample_rate as usize / 2; // 0.5 s
    let mut feed: Vec<f32> = samples.to_vec();
    feed.extend(std::iter::repeat_n(0.0, pad));
    for block in feed.chunks_mut(chunk) {
        let n = block.len();
        buf[..n].copy_from_slice(block);
        dec.process(&mut buf[..n]);
        if dec.is_complete() {
            let name = dec.current_mode_name().to_string();
            let (px, w, h) = dec.image();
            return Some((px, w, h, name));
        }
    }
    None
}

#[cfg(test)]
mod roundtrip_tests;

#[cfg(test)]
mod sbc_vendored;
