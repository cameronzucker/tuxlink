// SPDX-License-Identifier: AGPL-3.0-only

//! Spectrum-shaped complex-Gaussian fading process.
//!
//! Per Watterson 1970 and Proakis §13: a tap's fading envelope is a
//! complex-Gaussian process with a Gaussian-shape Doppler power-spectral
//! density centered at 0 Hz with bi-sided spread `2σ`. We generate this
//! via the "shaping filter" approach — draw white complex-Gaussian,
//! multiply by a Gaussian frequency mask in the FFT domain, inverse-FFT
//! to time domain.

use crate::rng::complex_gaussian_block;
use num_complex::Complex;
use rand_xoshiro::Xoshiro256PlusPlus;
use rustfft::FftPlanner;
use std::sync::Arc;

/// Generate one block of spectrum-shaped fading samples.
///
/// # Parameters
/// - `rng`: seeded RNG; advances `block_len` complex-Gaussian draws.
/// - `block_len`: number of samples (must be a power of two for FFT efficiency).
/// - `sample_rate_hz`: simulation sample rate (e.g., 8000.0 for audio-band).
/// - `doppler_spread_hz`: F.520 `2σ` Doppler spread parameter.
///
/// # Returns
/// A `Vec<Complex<f32>>` of length `block_len`, normalized to unit
/// average power so multiplication preserves expected input power.
pub fn generate_fading_block(
    rng: &mut Xoshiro256PlusPlus,
    block_len: usize,
    sample_rate_hz: f64,
    doppler_spread_hz: f64,
    fft_planner: &mut FftPlanner<f32>,
) -> Vec<Complex<f32>> {
    assert!(block_len.is_power_of_two(), "block_len must be power of two");

    // 1. White complex-Gaussian draw.
    let pairs = complex_gaussian_block(rng, block_len);
    let mut buf: Vec<Complex<f32>> = pairs
        .into_iter()
        .map(|(re, im)| Complex { re, im })
        .collect();

    // 2. Forward FFT.
    let fft: Arc<dyn rustfft::Fft<f32>> = fft_planner.plan_fft_forward(block_len);
    fft.process(&mut buf);

    // 3. Build Gaussian frequency mask. The mask is centered at DC (bin 0)
    //    and wraps around symmetrically per FFT bin convention. Each bin i
    //    corresponds to frequency f_i = i * (sample_rate / block_len) for
    //    i ≤ block_len/2, and to negative frequencies for i > block_len/2.
    //
    //    Watterson Gaussian PSD: S(f) = exp(-f^2 / (2 * σ^2)), where the
    //    F.520 `2σ` parameter is the FULL bi-sided spread → σ = (2σ)/2.
    let sigma = doppler_spread_hz / 2.0;
    let two_sigma_sq = 2.0 * sigma * sigma;
    let bin_hz = sample_rate_hz / block_len as f64;
    for i in 0..block_len {
        let raw_f = i as f64 * bin_hz;
        let f = if i <= block_len / 2 {
            raw_f
        } else {
            raw_f - sample_rate_hz
        };
        let mag = if two_sigma_sq > 0.0 {
            (-(f * f) / two_sigma_sq).exp().sqrt() as f32
        } else {
            // doppler_spread_hz == 0 ⇒ DC only (static channel).
            if i == 0 { 1.0 } else { 0.0 }
        };
        buf[i] = buf[i] * mag;
    }

    // 4. Inverse FFT.
    let ifft: Arc<dyn rustfft::Fft<f32>> = fft_planner.plan_fft_inverse(block_len);
    ifft.process(&mut buf);

    // 5. rustfft is unnormalized — divide by block_len so the round-trip
    //    preserves unit scale.
    let inv_n = 1.0 / block_len as f32;
    for s in &mut buf {
        *s = *s * inv_n;
    }

    // 6. Renormalize to unit average power. After shaping, the average
    //    power |z|^2 differs from 1 by a factor depending on the mask's
    //    L2 norm; rescale so the post-renormalization mean |z|^2 ≈ 1.
    let mean_power: f32 = buf.iter().map(|c| c.norm_sqr()).sum::<f32>() / block_len as f32;
    if mean_power > 0.0 {
        let scale = 1.0 / mean_power.sqrt();
        for s in &mut buf {
            *s = *s * scale;
        }
    }

    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::rng_from_seed;

    #[test]
    fn block_is_unit_power() {
        let mut rng = rng_from_seed(0xBEEF);
        let mut planner = FftPlanner::<f32>::new();
        let block = generate_fading_block(&mut rng, 1024, 8000.0, 1.0, &mut planner);
        let power: f32 = block.iter().map(|c| c.norm_sqr()).sum::<f32>() / 1024.0;
        assert!((power - 1.0).abs() < 1e-4, "expected unit power, got {power}");
    }

    #[test]
    fn same_seed_same_block() {
        let mut planner = FftPlanner::<f32>::new();
        let mut r1 = rng_from_seed(7);
        let mut r2 = rng_from_seed(7);
        let b1 = generate_fading_block(&mut r1, 1024, 8000.0, 0.5, &mut planner);
        let b2 = generate_fading_block(&mut r2, 1024, 8000.0, 0.5, &mut planner);
        assert_eq!(b1, b2, "deterministic same-seed reproduction");
    }

    #[test]
    fn zero_doppler_is_static() {
        // doppler=0 should produce a constant-magnitude block (DC tap only).
        let mut rng = rng_from_seed(0);
        let mut planner = FftPlanner::<f32>::new();
        let block = generate_fading_block(&mut rng, 1024, 8000.0, 0.0, &mut planner);
        // All samples should have the same magnitude (within fp tolerance).
        let mag0 = block[0].norm();
        for s in &block {
            assert!(
                (s.norm() - mag0).abs() < 1e-4,
                "zero-doppler must be constant magnitude",
            );
        }
    }

    #[test]
    fn higher_doppler_decorrelates_faster() {
        // Compare autocorrelation lag-1 normalized magnitude:
        // low Doppler → high correlation; high Doppler → lower correlation.
        let mut planner = FftPlanner::<f32>::new();

        let mut lag1_corr = |doppler: f64, seed: u64| -> f32 {
            let mut rng = rng_from_seed(seed);
            let b = generate_fading_block(&mut rng, 4096, 8000.0, doppler, &mut planner);
            let mut num = Complex { re: 0.0_f32, im: 0.0 };
            let mut den = 0.0_f32;
            for i in 0..b.len() - 1 {
                num = num + b[i + 1] * b[i].conj();
                den += b[i].norm_sqr();
            }
            num.norm() / den
        };

        let low = lag1_corr(0.1, 1234);
        let high = lag1_corr(10.0, 1234);
        assert!(
            low > high,
            "expected low-Doppler corr > high-Doppler corr, got low={low} high={high}",
        );
    }
}
