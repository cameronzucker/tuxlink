// SPDX-License-Identifier: AGPL-3.0-only

//! Deterministic seeded RNG primitives.
//!
//! Per forcing function §3.5: every random draw in the simulator is
//! reproducible given the seed + parameters + input. No OsRng, no
//! time-based seeding, no parallel-stream non-determinism.

use rand::SeedableRng;
use rand_distr::{Distribution, Normal};
use rand_xoshiro::Xoshiro256PlusPlus;

/// SplitMix64 — used to derive sub-stream seeds from a single user seed.
///
/// Reference: Vigna, S. "Further scramblings of Marsaglia's xorshift
/// generators." 2014. Public-domain reference implementation widely
/// available; this is a 5-line re-implementation from the algorithm
/// description.
pub fn split_mix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Construct a Xoshiro256++ RNG from a u64 seed.
pub fn rng_from_seed(seed: u64) -> Xoshiro256PlusPlus {
    Xoshiro256PlusPlus::seed_from_u64(seed)
}

/// Draw N complex-Gaussian samples (zero mean, unit total variance
/// distributed as 1/√2 per real and imag part).
///
/// Returns interleaved (re, im) pairs as f32 to keep the hot path on
/// f32 arithmetic; cast at the call site if higher precision is needed.
pub fn complex_gaussian_block(rng: &mut Xoshiro256PlusPlus, n: usize) -> Vec<(f32, f32)> {
    let normal = Normal::new(0.0_f32, std::f32::consts::FRAC_1_SQRT_2).unwrap();
    (0..n)
        .map(|_| (normal.sample(rng), normal.sample(rng)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = rng_from_seed(42);
        let mut b = rng_from_seed(42);
        let sa = complex_gaussian_block(&mut a, 1024);
        let sb = complex_gaussian_block(&mut b, 1024);
        assert_eq!(sa, sb, "identical seeds must produce identical sequences");
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = rng_from_seed(1);
        let mut b = rng_from_seed(2);
        let sa = complex_gaussian_block(&mut a, 64);
        let sb = complex_gaussian_block(&mut b, 64);
        assert_ne!(sa, sb);
    }

    #[test]
    fn split_mix64_advances() {
        let mut s = 0u64;
        let a = split_mix64(&mut s);
        let b = split_mix64(&mut s);
        assert_ne!(a, b);
        // Verify SplitMix64 standard first-output for seed=0:
        //   first output for seed=0 is 0xE220_A839_7B1D_CDAF (canonical).
        let mut s0 = 0u64;
        assert_eq!(split_mix64(&mut s0), 0xE220_A839_7B1D_CDAF);
    }

    #[test]
    fn gaussian_block_has_expected_variance() {
        // With Normal(0, 1/√2) per component, total complex variance is 1.
        // Empirical variance over 100k samples should be within ~3% of 1.0.
        let mut rng = rng_from_seed(0xC0FFEE);
        let samples = complex_gaussian_block(&mut rng, 100_000);
        let sum_sq: f64 = samples
            .iter()
            .map(|(re, im)| (*re as f64).powi(2) + (*im as f64).powi(2))
            .sum();
        let variance = sum_sq / samples.len() as f64;
        assert!(
            (variance - 1.0).abs() < 0.03,
            "expected complex variance ~1.0, got {variance}",
        );
    }
}
