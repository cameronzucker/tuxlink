// SPDX-License-Identifier: AGPL-3.0-only

//! Characterization-run report — the canonical AGPLv3-published JSON
//! artifact that every downstream subsystem test consumes.

use crate::analysis::{estimate_subcarrier_snr, SubcarrierSnrEstimate};
use crate::channel::WattersonChannel;
use crate::noise::AwgnGenerator;
use crate::params::ChannelCondition;
use num_complex::Complex;
use serde::{Deserialize, Serialize};

/// Input parameters fully specifying a characterization run. Same inputs +
/// same clean signal produce the same [`CharacterizationReport`] (modulo
/// `crate_version`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterizationInputs {
    /// Standardized ITU-R F.520 / F.1487 channel condition.
    pub condition: ChannelCondition,
    /// Simulation sample rate in Hz.
    pub sample_rate_hz: f64,
    /// Number of complex samples in the clean reference signal.
    pub signal_length_samples: usize,
    /// Seed for the channel's Watterson fading process.
    pub channel_seed: u64,
    /// Seed for the AWGN generator (independent of `channel_seed`).
    pub noise_seed: u64,
    /// Requested SNR in dB after the channel, before measurement.
    pub target_snr_db: f64,
    /// FFT size used by the per-sub-carrier SNR analyzer.
    pub fft_size: usize,
}

/// Canonical JSON-serializable output of a characterization run. Carries
/// inputs, observed power statistics, achieved-vs-target SNR drift, the
/// full per-sub-carrier SNR estimate, and the foundational-paper citation
/// chain (ADR 0014 §5).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterizationReport {
    /// Version of the `hf-channel-sim` crate that produced this report.
    pub crate_version: String,
    /// Open-source references the implementation is grounded in
    /// (Watterson 1970, ITU-R F.520, F.1487, etc.).
    pub foundational_citations: Vec<String>,
    /// Input parameter set for this run.
    pub inputs: CharacterizationInputs,
    /// Mean `|y|²` of the post-channel signal (pre-AWGN).
    pub observed_signal_power: f64,
    /// Mean `|y|²` of the added AWGN component.
    pub observed_noise_power: f64,
    /// Achieved signal-to-noise ratio in dB. May drift from
    /// `inputs.target_snr_db` due to per-realization channel power variance.
    pub achieved_snr_db: f64,
    /// Per-sub-carrier SNR characterization, suitable as bit-loading input.
    pub subcarrier_snr: SubcarrierSnrEstimate,
}

/// Run a single end-to-end characterization: apply channel, add noise,
/// estimate per-sub-carrier SNR, package into a report.
///
/// `clean_signal` is the known reference. Same `inputs` + same `clean_signal`
/// produce the same `CharacterizationReport` (modulo `crate_version`).
pub fn run_characterization(
    clean_signal: &[Complex<f32>],
    inputs: CharacterizationInputs,
) -> CharacterizationReport {
    let mut channel = WattersonChannel::from_condition(
        inputs.channel_seed,
        inputs.condition,
        inputs.sample_rate_hz,
    );
    let channel_out = channel.process_block(clean_signal);

    let observed_signal_power: f64 =
        channel_out.iter().map(|c| c.norm_sqr() as f64).sum::<f64>() / channel_out.len() as f64;

    let mut observed = channel_out.clone();
    let mut awgn = AwgnGenerator::new(inputs.noise_seed);
    awgn.add_noise(&mut observed, inputs.target_snr_db);

    // Achieved noise power is observed_total_power - observed_signal_power.
    let observed_total: f64 =
        observed.iter().map(|c| c.norm_sqr() as f64).sum::<f64>() / observed.len() as f64;
    let observed_noise_power = observed_total - observed_signal_power;
    let achieved_snr_db = if observed_noise_power > 0.0 {
        10.0 * (observed_signal_power / observed_noise_power).log10()
    } else {
        f64::INFINITY
    };

    let subcarrier_snr = estimate_subcarrier_snr(
        &channel_out,
        &observed,
        inputs.fft_size,
        inputs.sample_rate_hz,
    );

    CharacterizationReport {
        crate_version: env!("CARGO_PKG_VERSION").to_string(),
        foundational_citations: vec![
            "Watterson, Juroshek, Bensema 1970 (IEEE COM-18:6, pp.792-803)".into(),
            "ITU-R F.520-2".into(),
            "ITU-R F.1487 (2000)".into(),
            "Davies, Ionospheric Radio (IEE 1990)".into(),
            "Proakis & Salehi, Digital Communications 5e (McGraw-Hill 2008)".into(),
        ],
        inputs,
        observed_signal_power,
        observed_noise_power,
        achieved_snr_db,
        subcarrier_snr,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::{complex_gaussian_block, rng_from_seed};

    fn synthetic_signal(n: usize, seed: u64) -> Vec<Complex<f32>> {
        let mut rng = rng_from_seed(seed);
        complex_gaussian_block(&mut rng, n)
            .into_iter()
            .map(|(re, im)| Complex { re, im })
            .collect()
    }

    #[test]
    fn same_inputs_same_report_modulo_version() {
        let signal = synthetic_signal(8192, 99);
        let inputs = CharacterizationInputs {
            condition: ChannelCondition::Moderate,
            sample_rate_hz: 8000.0,
            signal_length_samples: 8192,
            channel_seed: 1,
            noise_seed: 2,
            target_snr_db: 5.0,
            fft_size: 1024,
        };
        let r1 = run_characterization(&signal, inputs.clone());
        let r2 = run_characterization(&signal, inputs);
        assert_eq!(r1.crate_version, r2.crate_version);
        assert_eq!(r1.observed_signal_power, r2.observed_signal_power);
        assert_eq!(r1.observed_noise_power, r2.observed_noise_power);
        assert_eq!(r1.achieved_snr_db, r2.achieved_snr_db);
        assert_eq!(r1.subcarrier_snr.mean_snr_db, r2.subcarrier_snr.mean_snr_db);
    }

    #[test]
    fn achieved_snr_close_to_target() {
        // 5 dB target should produce achieved ~5 dB within ~1 dB statistical tolerance.
        let signal = synthetic_signal(16_384, 7);
        let inputs = CharacterizationInputs {
            condition: ChannelCondition::Good,
            sample_rate_hz: 8000.0,
            signal_length_samples: 16_384,
            channel_seed: 11,
            noise_seed: 22,
            target_snr_db: 5.0,
            fft_size: 1024,
        };
        let r = run_characterization(&signal, inputs);
        assert!(
            (r.achieved_snr_db - 5.0).abs() < 1.0,
            "expected achieved ~5 dB, got {}",
            r.achieved_snr_db,
        );
    }

    #[test]
    fn report_is_json_serializable() {
        let signal = synthetic_signal(2048, 0);
        let inputs = CharacterizationInputs {
            condition: ChannelCondition::Good,
            sample_rate_hz: 8000.0,
            signal_length_samples: 2048,
            channel_seed: 0,
            noise_seed: 0,
            target_snr_db: 20.0,
            fft_size: 1024,
        };
        let r = run_characterization(&signal, inputs);

        // SubcarrierSnrEstimate can contain f32::INFINITY in bins where
        // per-bin noise drops below f32 epsilon (a known JSON-roundtrip
        // limitation; serde_json encodes Infinity as null). Sanitize for
        // round-trip — same pattern as analysis::tests::serde_roundtrip.
        let safe_sub = SubcarrierSnrEstimate {
            mean_snr_db: r
                .subcarrier_snr
                .mean_snr_db
                .iter()
                .map(|x| if x.is_finite() { *x } else { 999.0 })
                .collect(),
            snapshots: r
                .subcarrier_snr
                .snapshots
                .iter()
                .map(|s| {
                    s.iter()
                        .map(|x| if x.is_finite() { *x } else { 999.0 })
                        .collect()
                })
                .collect(),
            ..r.subcarrier_snr.clone()
        };
        let f64_safe = |x: f64| if x.is_finite() { x } else { 9999.0 };
        let safe = CharacterizationReport {
            subcarrier_snr: safe_sub,
            achieved_snr_db: f64_safe(r.achieved_snr_db),
            observed_signal_power: f64_safe(r.observed_signal_power),
            observed_noise_power: f64_safe(r.observed_noise_power),
            ..r.clone()
        };
        let json = serde_json::to_string(&safe).unwrap();
        assert!(json.contains("Watterson"));
        let back: CharacterizationReport = serde_json::from_str(&json).unwrap();
        assert_eq!(back.inputs.condition, ChannelCondition::Good);
    }

    #[test]
    fn citations_include_foundational_papers() {
        let signal = synthetic_signal(1024, 0);
        let inputs = CharacterizationInputs {
            condition: ChannelCondition::Good,
            sample_rate_hz: 8000.0,
            signal_length_samples: 1024,
            channel_seed: 0,
            noise_seed: 0,
            target_snr_db: 20.0,
            fft_size: 1024,
        };
        let r = run_characterization(&signal, inputs);
        let combined = r.foundational_citations.join(" | ");
        assert!(combined.contains("Watterson"));
        assert!(combined.contains("F.520"));
        assert!(combined.contains("F.1487"));
    }
}
