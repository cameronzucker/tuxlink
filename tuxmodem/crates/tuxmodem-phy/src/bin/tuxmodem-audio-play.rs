//! tuxmodem-audio-play — CLI for the PHY's real-time audio output.
//!
//! Operator's bench tool for validating that the audio chain from
//! tuxmodem-phy to the soundcard (Digirig USB audio + onward to the
//! radio's audio-input pin if cabled) actually works. Two modes:
//!
//! ```text
//! tuxmodem-audio-play --list
//!     Enumerate output devices CPAL can see, with their default
//!     channel count + sample-rate range + 48kHz-f32-support flag.
//!
//! tuxmodem-audio-play --device <name> --sine <hz>:<seconds>
//!     Generate a pure sine tone at <hz> Hz for <seconds> seconds
//!     and play it out the named device. <hz> must be ≤ 20000 (Nyquist
//!     headroom against the 48 kHz sample rate) and <seconds> ≤ 60
//!     (bench-test cap; longer plays should use a dedicated test
//!     binary with their own deadline + abort).
//! ```
//!
//! Builds only when the `audio-device` feature is on:
//!
//! ```sh
//! cargo run --manifest-path tuxmodem/crates/tuxmodem-phy/Cargo.toml \
//!   --features audio-device --bin tuxmodem-audio-play -- --list
//! ```
//!
//! ## Safety
//!
//! This CLI does NOT assert PTT. The sine tone leaves the
//! soundcard's audio-out pin and goes wherever the operator's cable
//! routes it. If that cable goes into a radio with VOX or with
//! a separate manually-asserted PTT, the radio WILL transmit the
//! tone. RADIO-1 places that responsibility on the operator.

use std::process::ExitCode;
use std::time::Duration;

use tuxmodem_phy::audio_device::{list_output_devices, AudioOutput};
use tuxmodem_phy::audio_io::{AudioBuffer, SAMPLE_RATE_HZ};
use tuxmodem_phy::PhyError;

const MAX_TONE_HZ: u32 = 20_000;
const MAX_DURATION_SECS: u32 = 60;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let parsed = match parse_args(&args) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!();
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    if parsed.help {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    match run(parsed) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[derive(Debug)]
struct Parsed {
    mode: Mode,
    device: Option<String>,
    help: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Mode {
    List,
    Sine { freq_hz: u32, duration_secs: u32 },
    None,
}

fn parse_args(args: &[String]) -> Result<Parsed, String> {
    let mut parsed = Parsed {
        mode: Mode::None,
        device: None,
        help: false,
    };
    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--list" => parsed.mode = Mode::List,
            "--sine" => {
                let v = iter.next().ok_or_else(|| {
                    "--sine requires a HZ:SECS value (e.g. --sine 1000:3)".to_string()
                })?;
                let (hz, secs) = parse_sine_arg(v)?;
                parsed.mode = Mode::Sine {
                    freq_hz: hz,
                    duration_secs: secs,
                };
            }
            "--device" | "-d" => {
                parsed.device = Some(
                    iter.next()
                        .ok_or_else(|| "--device requires a name".to_string())?
                        .clone(),
                );
            }
            "--help" | "-h" => parsed.help = true,
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(parsed)
}

fn parse_sine_arg(s: &str) -> Result<(u32, u32), String> {
    let (hz_str, secs_str) = s
        .split_once(':')
        .ok_or_else(|| format!("--sine expects HZ:SECS, got: {s}"))?;
    let hz: u32 = hz_str
        .parse()
        .map_err(|_| format!("--sine HZ must be a positive integer, got: {hz_str}"))?;
    let secs: u32 = secs_str
        .parse()
        .map_err(|_| format!("--sine SECS must be a positive integer, got: {secs_str}"))?;
    if hz == 0 || hz > MAX_TONE_HZ {
        return Err(format!(
            "--sine HZ must be in 1..={MAX_TONE_HZ} (Nyquist cap), got: {hz}"
        ));
    }
    if secs == 0 || secs > MAX_DURATION_SECS {
        return Err(format!(
            "--sine SECS must be in 1..={MAX_DURATION_SECS} (bench cap), got: {secs}"
        ));
    }
    Ok((hz, secs))
}

fn run(parsed: Parsed) -> Result<(), AppError> {
    match parsed.mode {
        Mode::None => Err(AppError::MissingMode),
        Mode::List => run_list(),
        Mode::Sine {
            freq_hz,
            duration_secs,
        } => {
            let device = parsed.device.ok_or(AppError::MissingDevice)?;
            run_sine(&device, freq_hz, duration_secs)
        }
    }
}

fn run_list() -> Result<(), AppError> {
    let devices = list_output_devices().map_err(AppError::Phy)?;
    if devices.is_empty() {
        println!("(no output devices)");
        return Ok(());
    }
    println!(
        "{:<48} {:>4}  {:>15}  48kHz-f32?",
        "device", "ch", "rate range (Hz)"
    );
    println!("{}", "-".repeat(80));
    for d in devices {
        println!(
            "{:<48} {:>4}  {:>7} - {:>5}  {}",
            truncate(&d.name, 48),
            d.default_channels,
            d.min_sample_rate_hz,
            d.max_sample_rate_hz,
            if d.supports_48k_f32 { "yes" } else { "no" },
        );
    }
    Ok(())
}

fn run_sine(device_name: &str, hz: u32, secs: u32) -> Result<(), AppError> {
    let mut output = AudioOutput::open(Some(device_name)).map_err(AppError::Phy)?;
    let channels = output.channels();
    let resolved = output
        .device_name()
        .unwrap_or_else(|_| device_name.to_string());
    println!(
        "playing {hz} Hz sine for {secs}s to '{resolved}' ({channels} channel(s)) ..."
    );
    let buffer = generate_sine(hz, secs);
    let started = std::time::Instant::now();
    output.play_blocking(&buffer).map_err(AppError::Phy)?;
    let elapsed = started.elapsed();
    println!(
        "played {samples} samples ({secs}s of audio) in {elapsed:.2?}",
        samples = buffer.samples().len(),
    );
    let _ = elapsed;
    let _ = Duration::ZERO;
    Ok(())
}

/// Generate a pure sine tone at `hz` for `secs` seconds at the PHY's
/// pinned [`SAMPLE_RATE_HZ`]. Amplitude 0.3 — gives ~10 dB headroom
/// against clipping when the operator hasn't dialed the soundcard's
/// output gain yet (the alternative — 0.9 — is unkind to anyone
/// wearing headphones during the first run).
fn generate_sine(hz: u32, secs: u32) -> AudioBuffer {
    let n = (secs as usize) * (SAMPLE_RATE_HZ as usize);
    let mut samples = Vec::with_capacity(n);
    let two_pi_f_over_fs = 2.0 * std::f32::consts::PI * (hz as f32) / (SAMPLE_RATE_HZ as f32);
    let amplitude = 0.3_f32;
    for i in 0..n {
        samples.push(amplitude * (two_pi_f_over_fs * (i as f32)).sin());
    }
    AudioBuffer::from_samples(samples)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max - 1).collect();
        out.push('…');
        out
    }
}

#[derive(Debug)]
enum AppError {
    MissingMode,
    MissingDevice,
    Phy(PhyError),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingMode => write!(f, "missing --list or --sine; one is required"),
            Self::MissingDevice => write!(f, "missing --device <name> (required for --sine)"),
            Self::Phy(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for AppError {}

const USAGE: &str = "\
tuxmodem-audio-play — bench tool for the tuxmodem PHY's real-time audio output

USAGE:
    tuxmodem-audio-play --list
    tuxmodem-audio-play --device <NAME> --sine <HZ>:<SECONDS>

OPTIONS:
        --list                  enumerate CPAL output devices and their support state
        --sine <HZ>:<SECONDS>   generate + play a pure sine tone (e.g. --sine 1000:3)
    -d, --device <NAME>         output device name (required for --sine; from --list)
    -h, --help                  this usage

CONSTRAINTS:
    HZ:           1..=20000   (Nyquist cap against the 48 kHz sample rate)
    SECONDS:      1..=60      (bench-test cap)

EXAMPLES:
    # 1. Discover the Digirig's CPAL device name:
    tuxmodem-audio-play --list

    # 2. Play a 1 kHz tone for 3 seconds out the Digirig:
    tuxmodem-audio-play --device 'USB Audio Device' --sine 1000:3

SAFETY:
    This CLI does NOT assert PTT. The tone leaves the soundcard's
    audio-out pin and goes wherever the operator's cable routes it.
    If that cable goes into a radio with VOX or with a separately-
    asserted PTT, the radio WILL transmit the tone. RADIO-1 puts
    that responsibility on the operator.

    For radios that key on RTS (Digirig + G90), assert PTT with
    tux-rig-rts in a separate terminal BEFORE running this — and
    release it after, ideally inside the SIGINT/SIGTERM-handled
    `assert --duration N` form rather than a manual hold.
";

// ─── tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_list_mode() {
        let p = parse_args(&s(&["--list"])).unwrap();
        assert_eq!(p.mode, Mode::List);
    }

    #[test]
    fn parse_sine_with_device() {
        let p = parse_args(&s(&[
            "--device",
            "Digirig",
            "--sine",
            "1000:3",
        ]))
        .unwrap();
        assert_eq!(p.mode, Mode::Sine { freq_hz: 1000, duration_secs: 3 });
        assert_eq!(p.device.as_deref(), Some("Digirig"));
    }

    #[test]
    fn parse_sine_with_d_short_flag() {
        let p = parse_args(&s(&["-d", "X", "--sine", "440:1"])).unwrap();
        assert_eq!(p.mode, Mode::Sine { freq_hz: 440, duration_secs: 1 });
    }

    #[test]
    fn parse_help_flag() {
        let p = parse_args(&s(&["--help"])).unwrap();
        assert!(p.help);
    }

    #[test]
    fn parse_rejects_sine_without_colon() {
        let err = parse_args(&s(&["--sine", "1000"])).unwrap_err();
        assert!(err.contains("HZ:SECS"));
    }

    #[test]
    fn parse_rejects_zero_hz() {
        let err = parse_args(&s(&["--sine", "0:3"])).unwrap_err();
        assert!(err.contains("HZ"));
    }

    #[test]
    fn parse_rejects_zero_secs() {
        let err = parse_args(&s(&["--sine", "1000:0"])).unwrap_err();
        assert!(err.contains("SECS"));
    }

    #[test]
    fn parse_rejects_hz_above_nyquist_cap() {
        let err = parse_args(&s(&["--sine", "30000:1"])).unwrap_err();
        assert!(err.contains("HZ"));
        assert!(err.contains("20000"));
    }

    #[test]
    fn parse_rejects_secs_above_bench_cap() {
        let err = parse_args(&s(&["--sine", "1000:120"])).unwrap_err();
        assert!(err.contains("SECS"));
        assert!(err.contains("60"));
    }

    #[test]
    fn parse_rejects_unknown_arg() {
        let err = parse_args(&s(&["--list", "--gibberish"])).unwrap_err();
        assert!(err.contains("unknown argument"));
    }

    #[test]
    fn parse_rejects_non_numeric_hz() {
        let err = parse_args(&s(&["--sine", "loud:3"])).unwrap_err();
        assert!(err.contains("HZ"));
    }

    #[test]
    fn generate_sine_emits_expected_sample_count() {
        let b = generate_sine(1000, 1);
        assert_eq!(b.samples().len(), SAMPLE_RATE_HZ as usize);
    }

    #[test]
    fn generate_sine_first_sample_is_zero_or_near_zero() {
        // Pure sine starts at 0 (sin(0) = 0); first sample should be
        // effectively zero up to numerical precision.
        let b = generate_sine(1000, 1);
        assert!(b.samples()[0].abs() < 1e-5);
    }

    #[test]
    fn generate_sine_respects_amplitude_bound() {
        // Amplitude should stay ≤ 0.3 to leave clipping headroom.
        let b = generate_sine(1000, 1);
        let max = b.samples().iter().fold(0.0_f32, |a, &b| a.max(b.abs()));
        assert!(max <= 0.3 + 1e-6, "max sample {max} exceeds amplitude bound");
    }

    #[test]
    fn truncate_below_max_returns_input() {
        assert_eq!(truncate("short", 10), "short");
    }

    #[test]
    fn truncate_above_max_uses_ellipsis() {
        let s = truncate("0123456789", 5);
        assert_eq!(s.chars().count(), 5);
        assert!(s.ends_with('…'));
    }
}
