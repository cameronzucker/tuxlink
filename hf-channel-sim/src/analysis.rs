// SPDX-License-Identifier: AGPL-3.0-only

//! Per-sub-carrier SNR estimation.
//!
//! Forcing function §3.6 (overview §5.B): bit-adaptive OFDM (subsystem #3)
//! needs per-sub-carrier channel-quality observations for bit-loading
//! decisions. This analyzer takes a known clean reference and the post-
//! channel observed signal, FFTs both in windowed blocks, and produces
//! per-bin SNR estimates over time.

use num_complex::Complex;
use rustfft::FftPlanner;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Per-sub-carrier SNR characterization produced by [`estimate_subcarrier_snr`].
///
/// Holds both time-averaged per-bin mean SNR and per-window snapshots in dB,
/// plus the FFT parameters needed to map bins to frequencies. JSON-
/// serializable for AI-agent consumption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubcarrierSnrEstimate {
    /// FFT size used (number of frequency bins per window).
    pub fft_size: usize,
    /// Simulation sample rate in Hz; combine with `fft_size` to derive bin frequencies.
    pub sample_rate_hz: f64,
    /// Number of FFT-sized windows the estimator processed.
    pub window_count: usize,
    /// Time-averaged per-bin SNR in dB. Length == fft_size.
    pub mean_snr_db: Vec<f32>,
    /// Per-window per-bin SNR snapshots in dB.
    /// Outer length == window_count; inner == fft_size.
    pub snapshots: Vec<Vec<f32>>,
}

/// Estimate per-sub-carrier SNR over time from parallel clean / observed
/// signal streams.
///
/// `clean.len()` and `observed.len()` must be equal and a multiple of
/// `fft_size`. Excess samples beyond the last full window are ignored.
pub fn estimate_subcarrier_snr(
    clean: &[Complex<f32>],
    observed: &[Complex<f32>],
    fft_size: usize,
    sample_rate_hz: f64,
) -> SubcarrierSnrEstimate {
    assert_eq!(clean.len(), observed.len(), "len mismatch");
    assert!(fft_size.is_power_of_two(), "fft_size must be power of two");

    let mut planner = FftPlanner::<f32>::new();
    let fft: Arc<dyn rustfft::Fft<f32>> = planner.plan_fft_forward(fft_size);

    let window_count = clean.len() / fft_size;
    let mut snapshots: Vec<Vec<f32>> = Vec::with_capacity(window_count);
    // Accumulate in linear domain for proper averaging.
    let mut sig_pow_accum = vec![0.0_f64; fft_size];
    let mut noise_pow_accum = vec![0.0_f64; fft_size];

    for w in 0..window_count {
        let s = w * fft_size;
        let e = s + fft_size;

        let mut s_buf: Vec<Complex<f32>> = clean[s..e].to_vec();
        let mut y_buf: Vec<Complex<f32>> = observed[s..e].to_vec();

        fft.process(&mut s_buf);
        fft.process(&mut y_buf);

        let mut snap = vec![0.0_f32; fft_size];
        for bin in 0..fft_size {
            let sig_pow = s_buf[bin].norm_sqr();
            let noise = y_buf[bin] - s_buf[bin];
            let noise_pow = noise.norm_sqr();
            sig_pow_accum[bin] += sig_pow as f64;
            noise_pow_accum[bin] += noise_pow as f64;

            // Per-window snapshot.
            let snr_db = if noise_pow > 0.0 {
                10.0 * (sig_pow / noise_pow).log10()
            } else {
                f32::INFINITY
            };
            snap[bin] = snr_db;
        }
        snapshots.push(snap);
    }

    let mut mean_snr_db = vec![0.0_f32; fft_size];
    for bin in 0..fft_size {
        let s = sig_pow_accum[bin];
        let n = noise_pow_accum[bin];
        mean_snr_db[bin] = if n > 0.0 {
            (10.0 * (s / n).log10()) as f32
        } else {
            f32::INFINITY
        };
    }

    SubcarrierSnrEstimate {
        fft_size,
        sample_rate_hz,
        window_count,
        mean_snr_db,
        snapshots,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::noise::AwgnGenerator;

    fn tone(n: usize, freq_hz: f32, sample_rate_hz: f32) -> Vec<Complex<f32>> {
        (0..n)
            .map(|i| {
                let t = i as f32 / sample_rate_hz;
                Complex {
                    re: (2.0 * std::f32::consts::PI * freq_hz * t).cos(),
                    im: (2.0 * std::f32::consts::PI * freq_hz * t).sin(),
                }
            })
            .collect()
    }

    #[test]
    fn shape_matches_inputs() {
        let n = 4096;
        let clean = tone(n, 1000.0, 8000.0);
        let observed = clean.clone();
        let est = estimate_subcarrier_snr(&clean, &observed, 1024, 8000.0);
        assert_eq!(est.fft_size, 1024);
        assert_eq!(est.window_count, 4);
        assert_eq!(est.mean_snr_db.len(), 1024);
        assert_eq!(est.snapshots.len(), 4);
        assert_eq!(est.snapshots[0].len(), 1024);
    }

    #[test]
    fn noise_free_is_infinite_snr() {
        let clean = tone(2048, 500.0, 8000.0);
        let observed = clean.clone();
        let est = estimate_subcarrier_snr(&clean, &observed, 1024, 8000.0);
        // At least one bin should report infinity (noise == 0 everywhere
        // when observed == clean).
        assert!(est.mean_snr_db.iter().any(|x| x.is_infinite()));
    }

    #[test]
    fn awgn_yields_expected_per_bin_snr() {
        // Drive a white-noise SIGNAL (uniform across bins) at 0 dB AWGN.
        // Expected per-bin SNR ~ 0 dB averaged.
        use crate::rng::{complex_gaussian_block, rng_from_seed};
        let mut sig_rng = rng_from_seed(101);
        let clean: Vec<Complex<f32>> = complex_gaussian_block(&mut sig_rng, 8192)
            .into_iter()
            .map(|(re, im)| Complex { re, im })
            .collect();
        let mut observed = clean.clone();
        let mut awgn = AwgnGenerator::new(202);
        awgn.add_noise(&mut observed, 0.0);

        let est = estimate_subcarrier_snr(&clean, &observed, 1024, 8000.0);
        // Drop the DC bin which can be biased; check a typical mid-band bin.
        let mid_bin = 100;
        let snr_mid = est.mean_snr_db[mid_bin];
        assert!(
            (snr_mid - 0.0).abs() < 2.0,
            "expected ~0 dB SNR at mid bin, got {snr_mid}",
        );
    }

    #[test]
    fn serde_roundtrip() {
        let clean = tone(2048, 500.0, 8000.0);
        let observed = clean.clone();
        let est = estimate_subcarrier_snr(&clean, &observed, 1024, 8000.0);
        // Replace infinities with finite values so JSON survives.
        let safe = SubcarrierSnrEstimate {
            mean_snr_db: est
                .mean_snr_db
                .iter()
                .map(|x| if x.is_finite() { *x } else { 999.0 })
                .collect(),
            snapshots: est
                .snapshots
                .iter()
                .map(|s| {
                    s.iter()
                        .map(|x| if x.is_finite() { *x } else { 999.0 })
                        .collect()
                })
                .collect(),
            ..est
        };
        let json = serde_json::to_string(&safe).unwrap();
        let back: SubcarrierSnrEstimate = serde_json::from_str(&json).unwrap();
        assert_eq!(back.fft_size, 1024);
        assert_eq!(back.window_count, 2);
    }
}
