// SPDX-License-Identifier: AGPL-3.0-only

//! Two-tap Watterson HF ionospheric channel.
//!
//! Per Watterson, Juroshek, Bensema (1970) and ITU-R F.520 / F.1487.
//!
//! The model: `y[n] = (f₁[n]·s[n] + f₂[n]·s[n−D]) / √2`
//!   where `f₁`, `f₂` are independent spectrum-shaped complex-Gaussian
//!   fading processes (Task 4), and `D` is the delay in samples
//!   corresponding to `Δτ` seconds at the simulation sample rate.

use crate::fading::generate_fading_block;
use crate::params::{ChannelCondition, WattersonParams};
use crate::rng::{rng_from_seed, split_mix64};
use num_complex::Complex;
use rand_xoshiro::Xoshiro256PlusPlus;
use rustfft::FftPlanner;
use std::collections::VecDeque;

const FADING_BLOCK_LEN: usize = 4096;

/// Watterson-class HF ionospheric channel.
///
/// Construct with `new(seed, params, sample_rate_hz)` or
/// `from_condition(seed, ChannelCondition::Moderate, sample_rate_hz)`.
/// Call `process_block(&samples)` to apply the channel; the output is
/// the same length as the input.
///
/// **Determinism:** same seed + same params + same input sequence
/// produces bit-identical output, across runs and machines. There is
/// no internal use of OS RNG, system time, or other non-deterministic
/// source.
pub struct WattersonChannel {
    params: WattersonParams,
    sample_rate_hz: f64,
    seed: u64,
    tap1_rng: Xoshiro256PlusPlus,
    tap2_rng: Xoshiro256PlusPlus,
    fft_planner: FftPlanner<f32>,
    /// Pre-generated fading envelopes; refilled in chunks of FADING_BLOCK_LEN.
    tap1_buf: VecDeque<Complex<f32>>,
    tap2_buf: VecDeque<Complex<f32>>,
    /// Delay-line for tap 2 (the delayed path). Length = `delay_samples`.
    delay_line: VecDeque<Complex<f32>>,
    delay_samples: usize,
}

impl WattersonChannel {
    /// Construct from explicit numeric parameters.
    pub fn new(seed: u64, params: WattersonParams, sample_rate_hz: f64) -> Self {
        // Derive two independent sub-stream seeds from the user seed.
        let mut mixer = seed;
        let s1 = split_mix64(&mut mixer);
        let s2 = split_mix64(&mut mixer);

        let delay_samples = (params.delay_spread_s * sample_rate_hz).round() as usize;
        let mut delay_line = VecDeque::with_capacity(delay_samples.max(1));
        for _ in 0..delay_samples {
            delay_line.push_back(Complex { re: 0.0, im: 0.0 });
        }

        Self {
            params,
            sample_rate_hz,
            seed,
            tap1_rng: rng_from_seed(s1),
            tap2_rng: rng_from_seed(s2),
            fft_planner: FftPlanner::<f32>::new(),
            tap1_buf: VecDeque::new(),
            tap2_buf: VecDeque::new(),
            delay_line,
            delay_samples,
        }
    }

    /// Construct from a standardized ITU-R F.520 / F.1487 condition.
    pub fn from_condition(seed: u64, condition: ChannelCondition, sample_rate_hz: f64) -> Self {
        Self::new(seed, condition.params(), sample_rate_hz)
    }

    /// Rewind to initial state (as if freshly constructed with the same
    /// seed + params + sample rate).
    pub fn reset(&mut self) {
        *self = Self::new(self.seed, self.params, self.sample_rate_hz);
    }

    /// Apply the channel to a block of input samples. Returns a Vec the
    /// same length as `input`. Noise-free — caller applies AWGN externally.
    pub fn process_block(&mut self, input: &[Complex<f32>]) -> Vec<Complex<f32>> {
        let mut out = Vec::with_capacity(input.len());
        let inv_sqrt2 = 1.0_f32 / 2.0_f32.sqrt();

        for &s in input {
            // Replenish fading buffers if either is empty.
            if self.tap1_buf.is_empty() {
                let block = generate_fading_block(
                    &mut self.tap1_rng,
                    FADING_BLOCK_LEN,
                    self.sample_rate_hz,
                    self.params.doppler_spread_hz,
                    &mut self.fft_planner,
                );
                self.tap1_buf.extend(block);
            }
            if self.tap2_buf.is_empty() {
                let block = generate_fading_block(
                    &mut self.tap2_rng,
                    FADING_BLOCK_LEN,
                    self.sample_rate_hz,
                    self.params.doppler_spread_hz,
                    &mut self.fft_planner,
                );
                self.tap2_buf.extend(block);
            }
            let f1 = self.tap1_buf.pop_front().unwrap();
            let f2 = self.tap2_buf.pop_front().unwrap();

            // Pull the delayed sample from tap 2's delay line.
            let s_delayed = if self.delay_samples == 0 {
                s
            } else {
                let d = self.delay_line.pop_front().unwrap();
                self.delay_line.push_back(s);
                d
            };

            let y = (f1 * s + f2 * s_delayed) * inv_sqrt2;
            out.push(y);
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn impulse(n: usize) -> Vec<Complex<f32>> {
        let mut v = vec![Complex { re: 0.0, im: 0.0 }; n];
        v[0] = Complex { re: 1.0, im: 0.0 };
        v
    }

    #[test]
    fn same_seed_bit_identical_output() {
        let input: Vec<Complex<f32>> = (0..2048)
            .map(|i| Complex {
                re: (i as f32 * 0.1).sin(),
                im: 0.0,
            })
            .collect();

        let mut ch1 =
            WattersonChannel::from_condition(0xA5A5_A5A5, ChannelCondition::Moderate, 8000.0);
        let mut ch2 =
            WattersonChannel::from_condition(0xA5A5_A5A5, ChannelCondition::Moderate, 8000.0);

        let out1 = ch1.process_block(&input);
        let out2 = ch2.process_block(&input);

        assert_eq!(out1, out2, "same seed → bit-identical output");
    }

    #[test]
    fn different_seeds_diverge() {
        let input = vec![Complex { re: 1.0, im: 0.0 }; 1024];
        let mut a = WattersonChannel::from_condition(1, ChannelCondition::Moderate, 8000.0);
        let mut b = WattersonChannel::from_condition(2, ChannelCondition::Moderate, 8000.0);
        let oa = a.process_block(&input);
        let ob = b.process_block(&input);
        assert_ne!(oa, ob);
    }

    #[test]
    fn output_length_matches_input_length() {
        let input = vec![Complex { re: 0.5, im: 0.0 }; 777];
        let mut ch = WattersonChannel::from_condition(0, ChannelCondition::Good, 8000.0);
        let out = ch.process_block(&input);
        assert_eq!(out.len(), 777);
    }

    #[test]
    fn streaming_equals_one_shot() {
        // Calling process_block once with N samples should produce the same
        // result as calling it twice with N/2 samples each (state preserved).
        let input: Vec<Complex<f32>> = (0..512)
            .map(|i| Complex {
                re: (i as f32 * 0.01).cos(),
                im: (i as f32 * 0.01).sin(),
            })
            .collect();

        let mut one_shot = WattersonChannel::from_condition(99, ChannelCondition::Moderate, 8000.0);
        let out_full = one_shot.process_block(&input);

        let mut streaming =
            WattersonChannel::from_condition(99, ChannelCondition::Moderate, 8000.0);
        let mut out_streamed = streaming.process_block(&input[..256]);
        out_streamed.extend(streaming.process_block(&input[256..]));

        assert_eq!(out_full, out_streamed, "streaming must equal one-shot");
    }

    #[test]
    fn reset_returns_to_initial_state() {
        let input = vec![Complex { re: 1.0, im: 0.0 }; 100];
        let mut ch = WattersonChannel::from_condition(123, ChannelCondition::Poor, 8000.0);

        let out1 = ch.process_block(&input);
        ch.reset();
        let out2 = ch.process_block(&input);

        assert_eq!(out1, out2, "reset must restore initial state");
    }

    #[test]
    fn delay_line_introduces_expected_lag() {
        // With Poor (Δτ=2 ms) at 8 kHz, delay = 16 samples.
        // With doppler_spread = 0 (constant-magnitude fading), the impulse
        // response should show two peaks separated by 16 samples.
        let params = WattersonParams {
            delay_spread_s: 2.0e-3,
            doppler_spread_hz: 0.0, // STATIC fading for impulse-response clarity
        };
        let mut ch = WattersonChannel::new(0, params, 8000.0);
        let imp = impulse(64);
        let resp = ch.process_block(&imp);

        // Peak 1 at sample 0; peak 2 at sample 16. Magnitudes should be
        // non-zero at both locations and ~zero between them.
        assert!(resp[0].norm() > 0.1, "peak 1 missing: {}", resp[0].norm());
        assert!(resp[16].norm() > 0.1, "peak 2 missing: {}", resp[16].norm());
        // Index `i` is wanted in the failure message; enumerate would obscure that.
        #[allow(clippy::needless_range_loop)]
        for i in 1..16 {
            assert!(
                resp[i].norm() < 0.01,
                "unexpected energy between taps at index {i}: {}",
                resp[i].norm(),
            );
        }
    }
}
