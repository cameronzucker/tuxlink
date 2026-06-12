//! Preamble generation + correlation-based detection.
//!
//! Design primitive (per foundation doc §2.5 Meyr/Moeneclaey/Fechtel):
//! a Schmidl-Cox-style preamble — a known sequence whose
//! self-correlation under any reasonable channel impairment has a
//! sharp time-domain peak. We synthesise a chirp-modulated CAZAC
//! (constant-amplitude zero-autocorrelation) waveform — a Zadoff-Chu
//! sequence projected to the audio band.
//!
//! Exact length, root index, and audio-band placement are pinned in
//! Phase 11 after characterization sweeps; this initial pin gives a
//! 192-sample Zadoff-Chu sequence (4 ms @ 48 kHz). Re-tune in Phase 11.

use num_complex::Complex;

const PREAMBLE_LEN: usize = 192; // 4 ms @ 48 kHz
const PREAMBLE_ROOT: usize = 25; // co-prime with PREAMBLE_LEN

/// Generator for the canonical Zadoff-Chu preamble waveform.
pub struct PreambleGenerator;

impl PreambleGenerator {
    /// Construct a new preamble generator.
    pub fn new() -> Self {
        Self
    }
    /// Real-valued preamble samples ready to push into the audio buffer.
    /// We take the real part of a Zadoff-Chu sequence — keeping it real
    /// pre-Hilbert is fine since at sync time we re-create it the same
    /// way for correlation.
    pub fn generate(&self) -> Vec<f32> {
        zadoff_chu(PREAMBLE_LEN, PREAMBLE_ROOT)
            .iter()
            .map(|c| c.re)
            .collect()
    }
}

impl Default for PreambleGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Correlation-based detector for the Zadoff-Chu preamble in a real-valued
/// audio stream.
pub struct PreambleDetector {
    template: Vec<f32>,
}

impl PreambleDetector {
    /// Construct a detector pre-loaded with the canonical preamble template.
    pub fn new() -> Self {
        Self {
            template: PreambleGenerator::new().generate(),
        }
    }
}

impl Default for PreambleDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of a preamble scan.
#[derive(Debug, Clone)]
pub struct Detection {
    /// Sample index where the preamble starts in the scanned signal.
    pub start_sample: usize,
    /// Estimated post-correlation SNR in dB.
    pub snr_estimate_db: f32,
}

impl PreambleDetector {
    /// Scan `signal` for the preamble template. Returns `None` if no peak
    /// above the detection threshold is found.
    pub fn scan(&self, signal: &[f32]) -> Option<Detection> {
        if signal.len() < self.template.len() {
            return None;
        }
        let n = self.template.len();
        let template_energy: f32 = self.template.iter().map(|s| s * s).sum();
        let template_norm = template_energy.sqrt().max(1e-9);

        let mut best_corr = 0.0_f32;
        let mut best_idx = 0usize;
        for i in 0..(signal.len() - n) {
            let mut corr = 0.0_f32;
            let mut sig_energy = 0.0_f32;
            for j in 0..n {
                corr += signal[i + j] * self.template[j];
                sig_energy += signal[i + j] * signal[i + j];
            }
            let sig_norm = sig_energy.sqrt().max(1e-9);
            let normalised = corr / (sig_norm * template_norm);
            if normalised.abs() > best_corr {
                best_corr = normalised.abs();
                best_idx = i;
            }
        }

        // Detection threshold: |normalised correlation| > 0.5 is a
        // reasonable Phase-4 baseline. Phase 11 sweeps tighten this.
        if best_corr < 0.5 {
            return None;
        }
        // Approximate SNR from correlation strength.
        // For a perfect match in AWGN: |rho|^2 ≈ SNR / (1 + SNR).
        let rho_sq = (best_corr * best_corr).clamp(1e-6, 1.0 - 1e-6);
        let snr_lin = rho_sq / (1.0 - rho_sq);
        let snr_db = 10.0 * snr_lin.log10();
        Some(Detection {
            start_sample: best_idx,
            snr_estimate_db: snr_db,
        })
    }
}

fn zadoff_chu(n: usize, q: usize) -> Vec<Complex<f32>> {
    let pi = std::f32::consts::PI;
    (0..n)
        .map(|k| {
            let arg = -pi * (q as f32) * (k as f32) * ((k + 1) as f32) / (n as f32);
            Complex::new(arg.cos(), arg.sin())
        })
        .collect()
}
