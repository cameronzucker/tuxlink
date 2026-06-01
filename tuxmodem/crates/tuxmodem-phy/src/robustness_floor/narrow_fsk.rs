//! Narrow-FSK situational floor mode. Conceptual primitive borrowed
//! from FT8/JS8 weak-signal design (8-FSK; foundation doc §6.1) —
//! primitive only, not specific protocol parameters per
//! `feedback_clean_sheet_concepts_only`. Reserved for crowded-band
//! slots where wide-band isn't available.
//!
//! Noncoherent energy-detector receiver: for each symbol period, FFT
//! the segment (zero-padded to the next power of two) and pick the
//! bin with maximum magnitude across the 8 candidate tone bins.

use crate::audio_io::SAMPLE_RATE_HZ;
use crate::error::PhyError;
use num_complex::Complex;
use rustfft::FftPlanner;

const M: usize = 8;                      // 8-FSK ⇒ 3 bits/symbol
const TONE_SPACING_HZ: f32 = 50.0;       // spacing between tones
const SYMBOL_DURATION_SEC: f32 = 0.16;   // FT8-class baud as design primitive
const CENTER_FREQ_HZ: f32 = 1500.0;      // middle of audio band

/// 8-FSK noncoherent floor mode for crowded-band slots. Three bits
/// per symbol, 50 Hz tone spacing, 0.16 s symbol duration, centered
/// at 1500 Hz.
pub struct NarrowFskFloor;

impl NarrowFskFloor {
    /// Construct the floor. The mode has no tunable state today —
    /// all parameters are pinned per the foundation-doc §6.1 design
    /// primitives.
    pub fn new() -> Self {
        Self
    }

    /// Occupied bandwidth in Hz, including a one-tone-spacing guard
    /// band on each side of the active tone cluster.
    pub fn occupied_bandwidth_hz(&self) -> f32 {
        (M as f32 - 1.0) * TONE_SPACING_HZ + 2.0 * TONE_SPACING_HZ
    }

    fn samples_per_symbol(&self) -> usize {
        (SAMPLE_RATE_HZ as f32 * SYMBOL_DURATION_SEC) as usize
    }

    fn tone_freq_hz(&self, idx: usize) -> f32 {
        let low = CENTER_FREQ_HZ - (M as f32 / 2.0 - 0.5) * TONE_SPACING_HZ;
        low + idx as f32 * TONE_SPACING_HZ
    }

    /// Modulate a byte payload to a stream of FSK tones at 48 kHz f32
    /// audio sample rate.
    pub fn transmit(&self, payload: &[u8]) -> Result<Vec<f32>, PhyError> {
        let mut bits: Vec<u8> = Vec::with_capacity(payload.len() * 8);
        for byte in payload {
            for i in (0..8).rev() {
                bits.push((byte >> i) & 1);
            }
        }
        while bits.len() % 3 != 0 {
            bits.push(0);
        }
        let n_symbols = bits.len() / 3;
        let sps = self.samples_per_symbol();
        let mut samples = Vec::with_capacity(n_symbols * sps);
        for sym_idx in 0..n_symbols {
            let tone_idx = ((bits[sym_idx * 3] as usize) << 2)
                | ((bits[sym_idx * 3 + 1] as usize) << 1)
                | (bits[sym_idx * 3 + 2] as usize);
            let f = self.tone_freq_hz(tone_idx);
            for n in 0..sps {
                let t = n as f32 / SAMPLE_RATE_HZ as f32;
                samples.push((2.0 * std::f32::consts::PI * f * t).sin());
            }
        }
        Ok(samples)
    }

    /// Demodulate the audio stream back to a byte payload. Trailing
    /// zero bytes from the bit-padding are trimmed; multi-symbol
    /// framing arrives in Phase 10.
    pub fn receive(&self, samples: &[f32]) -> Result<Vec<u8>, PhyError> {
        let sps = self.samples_per_symbol();
        let n_symbols = samples.len() / sps;
        let mut planner = FftPlanner::<f32>::new();
        let fft_size = sps.next_power_of_two();
        let fft = planner.plan_fft_forward(fft_size);

        let mut bits = Vec::with_capacity(n_symbols * 3);
        for sym_idx in 0..n_symbols {
            let mut buf: Vec<Complex<f32>> = samples
                [sym_idx * sps..sym_idx * sps + sps]
                .iter()
                .map(|s| Complex::new(*s, 0.0))
                .collect();
            buf.resize(fft_size, Complex::new(0.0, 0.0));
            fft.process(&mut buf);

            let mut best_tone = 0usize;
            let mut best_mag = 0.0_f32;
            for tone_idx in 0..M {
                let f = self.tone_freq_hz(tone_idx);
                let bin = (f * fft_size as f32 / SAMPLE_RATE_HZ as f32).round() as usize;
                let m = buf[bin].norm();
                if m > best_mag {
                    best_mag = m;
                    best_tone = tone_idx;
                }
            }
            bits.push(((best_tone >> 2) & 1) as u8);
            bits.push(((best_tone >> 1) & 1) as u8);
            bits.push((best_tone & 1) as u8);
        }
        let trim = bits.len() - (bits.len() % 8);
        let bits = &bits[..trim];
        let mut bytes = Vec::with_capacity(bits.len() / 8);
        for chunk in bits.chunks(8) {
            let mut b = 0u8;
            for (i, &bit) in chunk.iter().enumerate() {
                b |= bit << (7 - i);
            }
            bytes.push(b);
        }
        let last_nonzero = bytes
            .iter()
            .rposition(|&b| b != 0)
            .map(|i| i + 1)
            .unwrap_or(0);
        bytes.truncate(last_nonzero);
        Ok(bytes)
    }
}

impl Default for NarrowFskFloor {
    fn default() -> Self {
        Self::new()
    }
}
