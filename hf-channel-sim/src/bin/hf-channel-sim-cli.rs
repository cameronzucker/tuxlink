// SPDX-License-Identifier: AGPL-3.0-only

//! hf-channel-sim CLI: read I/Q samples from stdin, produce JSON
//! characterization report on stdout.
//!
//! Designed for AI-agent harnesses: deterministic, structured-output,
//! pipe-friendly. No interactive prompts; all parameters via CLI flags.

use clap::Parser;
use hf_channel_sim::{
    run_characterization, CharacterizationInputs, ChannelCondition,
};
use num_complex::Complex;
use std::io::{self, Read, Write};

#[derive(Parser, Debug)]
#[command(
    name = "hf-channel-sim-cli",
    about = "Watterson HF channel simulator — pipe-friendly characterization runner",
    version
)]
struct Args {
    /// ITU-R F.520 / F.1487 channel condition
    #[arg(long, value_enum)]
    condition: ConditionArg,

    /// Sample rate in Hz
    #[arg(long, default_value_t = 8000.0)]
    sample_rate: f64,

    /// Channel RNG seed
    #[arg(long, default_value_t = 1)]
    channel_seed: u64,

    /// Noise RNG seed
    #[arg(long, default_value_t = 2)]
    noise_seed: u64,

    /// Target SNR in dB (signal-to-noise after channel)
    #[arg(long, default_value_t = 10.0)]
    target_snr_db: f64,

    /// FFT size for sub-carrier SNR analysis (must be power of two)
    #[arg(long, default_value_t = 1024)]
    fft_size: usize,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy)]
enum ConditionArg {
    Good,
    Moderate,
    Poor,
    Flutter,
}

impl From<ConditionArg> for ChannelCondition {
    fn from(c: ConditionArg) -> Self {
        match c {
            ConditionArg::Good => ChannelCondition::Good,
            ConditionArg::Moderate => ChannelCondition::Moderate,
            ConditionArg::Poor => ChannelCondition::Poor,
            ConditionArg::Flutter => ChannelCondition::Flutter,
        }
    }
}

fn main() -> io::Result<()> {
    let args = Args::parse();
    if !args.fft_size.is_power_of_two() {
        eprintln!("error: --fft-size must be a power of two");
        std::process::exit(2);
    }

    // Read all stdin as interleaved f32 LE I/Q pairs.
    let mut raw = Vec::new();
    io::stdin().read_to_end(&mut raw)?;
    if raw.len() % 8 != 0 {
        eprintln!("error: stdin length must be a multiple of 8 bytes (f32 I/Q pairs)");
        std::process::exit(2);
    }

    let mut signal = Vec::with_capacity(raw.len() / 8);
    let mut idx = 0;
    while idx + 8 <= raw.len() {
        let re = f32::from_le_bytes([raw[idx], raw[idx + 1], raw[idx + 2], raw[idx + 3]]);
        let im = f32::from_le_bytes([raw[idx + 4], raw[idx + 5], raw[idx + 6], raw[idx + 7]]);
        signal.push(Complex { re, im });
        idx += 8;
    }

    if signal.is_empty() {
        eprintln!("error: stdin produced 0 samples");
        std::process::exit(2);
    }

    let inputs = CharacterizationInputs {
        condition: args.condition.into(),
        sample_rate_hz: args.sample_rate,
        signal_length_samples: signal.len(),
        channel_seed: args.channel_seed,
        noise_seed: args.noise_seed,
        target_snr_db: args.target_snr_db,
        fft_size: args.fft_size,
    };

    let report = run_characterization(&signal, inputs);
    let json = serde_json::to_string_pretty(&report).expect("serde infallible");
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    handle.write_all(json.as_bytes())?;
    handle.write_all(b"\n")?;
    Ok(())
}
