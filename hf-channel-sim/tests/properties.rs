// SPDX-License-Identifier: AGPL-3.0-only

use hf_channel_sim::{
    run_characterization, CharacterizationInputs, ChannelCondition,
    WattersonChannel,
};
use num_complex::Complex;
use proptest::prelude::*;

fn synth_signal(n: usize, seed: u64) -> Vec<Complex<f32>> {
    use rand::{Rng, SeedableRng};
    use rand_xoshiro::Xoshiro256PlusPlus;
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(seed);
    (0..n)
        .map(|_| Complex {
            re: rng.gen_range(-1.0_f32..1.0),
            im: rng.gen_range(-1.0_f32..1.0),
        })
        .collect()
}

fn condition_strategy() -> impl Strategy<Value = ChannelCondition> {
    prop_oneof![
        Just(ChannelCondition::Good),
        Just(ChannelCondition::Moderate),
        Just(ChannelCondition::Poor),
        Just(ChannelCondition::Flutter),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 32,
        ..ProptestConfig::default()
    })]

    #[test]
    fn determinism(
        seed in any::<u64>(),
        condition in condition_strategy(),
        sig_seed in any::<u64>(),
        n in 256usize..4096,
    ) {
        let signal = synth_signal(n, sig_seed);
        let mut ch1 = WattersonChannel::from_condition(seed, condition, 8000.0);
        let mut ch2 = WattersonChannel::from_condition(seed, condition, 8000.0);
        let o1 = ch1.process_block(&signal);
        let o2 = ch2.process_block(&signal);
        prop_assert_eq!(o1, o2);
    }

    #[test]
    fn output_length_eq_input_length(
        seed in any::<u64>(),
        condition in condition_strategy(),
        n in 1usize..4096,
    ) {
        let signal = synth_signal(n, 42);
        let mut ch = WattersonChannel::from_condition(seed, condition, 8000.0);
        let out = ch.process_block(&signal);
        prop_assert_eq!(out.len(), signal.len());
    }

    #[test]
    fn energy_approximately_preserved(
        seed in any::<u64>(),
        condition in condition_strategy(),
    ) {
        // Use a long signal for low-variance energy measurement.
        let signal = synth_signal(16384, 7);
        let p_in: f64 = signal.iter().map(|c| c.norm_sqr() as f64).sum::<f64>() / signal.len() as f64;
        let mut ch = WattersonChannel::from_condition(seed, condition, 8000.0);
        let out = ch.process_block(&signal);
        let p_out: f64 = out.iter().map(|c| c.norm_sqr() as f64).sum::<f64>() / out.len() as f64;
        // Expect within 30% — Watterson is randomly faded so per-realization
        // variance is real. Looser bound than per-block fading-sample tests.
        prop_assert!(
            (p_out / p_in).log10().abs() < 0.15,
            "p_in={p_in} p_out={p_out} ratio_log10={}",
            (p_out / p_in).log10(),
        );
    }
}

#[test]
fn achieved_snr_tracks_target_over_wide_range() {
    // Sweep target SNR from -10 to +30 dB; achieved should be within 2 dB.
    for target in [-10.0_f64, -5.0, 0.0, 5.0, 10.0, 20.0, 30.0] {
        let signal = synth_signal(16384, 13);
        let inputs = CharacterizationInputs {
            condition: ChannelCondition::Moderate,
            sample_rate_hz: 8000.0,
            signal_length_samples: 16384,
            channel_seed: 100,
            noise_seed: 200,
            target_snr_db: target,
            fft_size: 1024,
        };
        let r = run_characterization(&signal, inputs);
        assert!(
            (r.achieved_snr_db - target).abs() < 2.0,
            "target {target} dB → achieved {} dB (out of tolerance)",
            r.achieved_snr_db,
        );
    }
}
