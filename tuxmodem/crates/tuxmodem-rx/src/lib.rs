//! tuxmodem-rx — capture + demod + BER composition.
//!
//! Phase 4 of the tuxmodem hardware bring-up (tuxlink-xvrb / tuxlink-9ggl).
//! Receive complement to [tuxlink-i3bz] (tuxmodem-tx). Composes the
//! capture side of [`tuxmodem_phy::audio_device::AudioInput`] with the
//! demod entry [`WidebandLowDensityFloor::receive`] into a single
//! binary that observes a real radio's audio output and reports
//! decoded bytes.
//!
//! ## Three workflows
//!
//! - `--decode-wav <PATH>` — read a WAV file (48 kHz f32 mono), take
//!   the first OFDM-symbol-worth of samples, demodulate, print bytes.
//!   Agent-runnable end-to-end: encode→WAV→decode roundtrip is the
//!   library's primary integration test.
//! - `--record-wav <PATH> --device <NAME> --duration <SECS>` — capture
//!   N seconds of audio to a WAV file. No demod; useful for off-air
//!   captures the operator post-processes (trim to the symbol of
//!   interest, then `--decode-wav`).
//! - `--list-devices` — enumerate CPAL input devices.
//!
//! ## Safety
//!
//! Pure capture. No PTT, no audio output, no transmission. RADIO-1
//! does NOT gate this binary the way it gates `tuxmodem-tx`; agents
//! may run the decode path freely. Capture-side (`--record-wav` /
//! `--listen`) is also safe to run as long as the radio is in receive
//! mode at the time of the capture (the operator owns that decision).
//!
//! ## Symbol-only assumption
//!
//! Without frame sync (PHY Phase 12+), the demod entry expects EXACTLY
//! one OFDM symbol's worth of samples (FFT body + cyclic prefix). For
//! the Wide mode at 48 kHz this is currently 2560 samples (≈53 ms).
//! `--decode-wav` slices the first symbol-size samples; longer files
//! are NOT scanned. Padding/trimming to the symbol-of-interest is
//! manual until Phase 12.
//!
//! [tuxlink-i3bz]: https://github.com/cameronzucker/tuxlink/issues?q=tuxlink-i3bz
//! [`WidebandLowDensityFloor::receive`]: tuxmodem_phy::robustness_floor::wideband_lowdensity::WidebandLowDensityFloor::receive

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use thiserror::Error;
use tuxmodem_phy::audio_device::{AudioInput, RecordOutcome};
use tuxmodem_phy::audio_io::{AudioBuffer, SAMPLE_RATE_HZ};
use tuxmodem_phy::error::PhyError;
use tuxmodem_phy::ofdm_main::ofdm_params::{OfdmModeName, OfdmParams};
use tuxmodem_phy::robustness_floor::wideband_lowdensity::WidebandLowDensityFloor;

// ─── Modes ──────────────────────────────────────────────────────────

/// Which PHY mode to demodulate under. Phase 4 ships with one mode —
/// the robustness floor's wide-band low-density OFDM — mirroring
/// `tuxmodem-tx`'s `wide-floor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// `floor-wblo` (wide-band low-density OFDM, BPSK / sub-carrier).
    WideFloor,
}

impl Mode {
    /// Parse a mode-name string. Accepts both `wide-floor` (the
    /// tuxmodem-tx-facing name) and `floor-wblo` (the PHY's
    /// `ModeDescriptor` short-name).
    pub fn parse(name: &str) -> Result<Self, RxError> {
        match name {
            "wide-floor" | "floor-wblo" => Ok(Self::WideFloor),
            other => Err(RxError::UnknownMode {
                name: other.to_string(),
            }),
        }
    }

    /// Stable kebab-case identifier.
    pub fn short_name(&self) -> &'static str {
        match self {
            Self::WideFloor => "wide-floor",
        }
    }

    /// Sample count for one OFDM symbol under this mode (FFT body +
    /// cyclic prefix). The demod entry takes exactly this many samples.
    pub fn symbol_size_samples(&self) -> usize {
        match self {
            Self::WideFloor => {
                let p = OfdmParams::for_mode(OfdmModeName::Wide);
                p.fft_size() + p.cp_len()
            }
        }
    }
}

/// Frame format selection. Mirrors `tuxmodem-tx`'s `FrameMode` —
/// `Raw` decodes a bare OFDM symbol from the FIRST symbol_size samples
/// of the input; `Sync` scans for the Zadoff-Chu preamble and decodes
/// the symbol that follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FrameMode {
    /// Bare OFDM symbol. The v0.0.1 wire format — operator must trim
    /// the WAV manually to the symbol of interest.
    #[default]
    Raw,
    /// Preamble + single OFDM symbol. Payload limited to one symbol's
    /// capacity (~9 bytes for the Wide mode). Pairs with
    /// `tuxmodem-tx --frame-mode sync`.
    Sync,
    /// Preamble + multi-symbol body carrying a 2-byte length-prefix
    /// header. Supports arbitrary-length payloads up to u16::MAX
    /// bytes. Pairs with `tuxmodem-tx --frame-mode multi-sync`.
    MultiSync,
}

impl FrameMode {
    /// Parse a `--frame-mode` argument value.
    pub fn parse(name: &str) -> Result<Self, RxError> {
        match name {
            "raw" => Ok(Self::Raw),
            "sync" => Ok(Self::Sync),
            "multi-sync" => Ok(Self::MultiSync),
            other => Err(RxError::UnknownFrameMode {
                name: other.to_string(),
            }),
        }
    }

    /// Stable kebab-case identifier (matches what `FrameMode::parse`
    /// accepts).
    pub fn short_name(&self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Sync => "sync",
            Self::MultiSync => "multi-sync",
        }
    }
}

// ─── Expected-payload resolution (mirrors tuxmodem-tx::resolve_payload) ──

/// Resolve an `--expected` argument value to its byte sequence. Same
/// two forms as `tuxmodem-tx`'s `--payload`: bare text (UTF-8 bytes)
/// or `@<path>` to read from a file.
pub fn resolve_expected(arg: &str) -> Result<Vec<u8>, RxError> {
    if let Some(path_str) = arg.strip_prefix('@') {
        let path = Path::new(path_str);
        std::fs::read(path).map_err(|e| RxError::ExpectedFileRead {
            path: path.display().to_string(),
            io_error: e.to_string(),
        })
    } else {
        Ok(arg.as_bytes().to_vec())
    }
}

// ─── Decoding ───────────────────────────────────────────────────────

/// Demodulate one symbol's worth of samples into a byte payload.
///
/// For [`FrameMode::Raw`]: takes the FIRST `mode.symbol_size_samples()`
/// of the input; errors with [`RxError::InsufficientSamples`] if the
/// input is shorter than that. v0.0.1 behavior — requires manual
/// trimming of arbitrary-length captures.
///
/// For [`FrameMode::Sync`]: scans for the Zadoff-Chu preamble via
/// [`WidebandLowDensityFloor::receive_with_sync`] and decodes the
/// symbol that follows; errors with [`RxError::Phy`] (wrapping
/// `PhyError::FrameDetect`) if the preamble can't be found or the
/// symbol is truncated.
pub fn decode_one_symbol(
    mode: Mode,
    samples: &[f32],
    frame_mode: FrameMode,
) -> Result<Vec<u8>, RxError> {
    match (mode, frame_mode) {
        (Mode::WideFloor, FrameMode::Raw) => {
            let needed = mode.symbol_size_samples();
            if samples.len() < needed {
                return Err(RxError::InsufficientSamples {
                    got: samples.len(),
                    needed,
                });
            }
            let slice = &samples[..needed];
            WidebandLowDensityFloor::new()
                .receive(slice)
                .map_err(RxError::Phy)
        }
        (Mode::WideFloor, FrameMode::Sync) => {
            let (_start, bytes) = WidebandLowDensityFloor::new()
                .receive_with_sync(samples)
                .map_err(RxError::Phy)?;
            Ok(bytes)
        }
        (Mode::WideFloor, FrameMode::MultiSync) => {
            let (_start, bytes) = WidebandLowDensityFloor::new()
                .receive_multi_with_sync(samples)
                .map_err(RxError::Phy)?;
            Ok(bytes)
        }
    }
}

/// Like [`decode_one_symbol`] but also returns the sample index where
/// the preamble was detected (for `FrameMode::Sync` only). Returns
/// `None` for the start index in `FrameMode::Raw`.
pub fn decode_one_symbol_with_offset(
    mode: Mode,
    samples: &[f32],
    frame_mode: FrameMode,
) -> Result<(Option<usize>, Vec<u8>), RxError> {
    match (mode, frame_mode) {
        (Mode::WideFloor, FrameMode::Raw) => {
            let bytes = decode_one_symbol(mode, samples, frame_mode)?;
            Ok((None, bytes))
        }
        (Mode::WideFloor, FrameMode::Sync) => {
            let (start, bytes) = WidebandLowDensityFloor::new()
                .receive_with_sync(samples)
                .map_err(RxError::Phy)?;
            Ok((Some(start), bytes))
        }
        (Mode::WideFloor, FrameMode::MultiSync) => {
            let (start, bytes) = WidebandLowDensityFloor::new()
                .receive_multi_with_sync(samples)
                .map_err(RxError::Phy)?;
            Ok((Some(start), bytes))
        }
    }
}

/// Read a WAV file into an [`AudioBuffer`]. Errors if the WAV isn't
/// 48 kHz f32 (the PHY's pinned format).
pub fn read_wav(path: &Path) -> Result<AudioBuffer, RxError> {
    AudioBuffer::read_wav(path).map_err(RxError::Phy)
}

// ─── BER ────────────────────────────────────────────────────────────

/// Bit-error-rate report comparing a decoded payload to an expected
/// reference. `len_mismatch = true` when the two byte sequences differ
/// in length (the BER is then computed against the shorter slice;
/// downstream callers may treat length mismatch as a hard failure).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BerReport {
    /// Bits that differed between expected + decoded (counted across
    /// `min(len)` bytes).
    pub bit_errors: u32,
    /// Total bits considered (= 8 × min(expected.len, decoded.len)).
    pub bits_compared: u32,
    /// Length of the expected slice in bytes.
    pub expected_len: usize,
    /// Length of the decoded slice in bytes.
    pub decoded_len: usize,
}

impl BerReport {
    /// `true` if the two payloads are equal byte-for-byte.
    pub fn is_clean(&self) -> bool {
        self.bit_errors == 0 && self.expected_len == self.decoded_len
    }
    /// `true` if the lengths differ.
    pub fn len_mismatch(&self) -> bool {
        self.expected_len != self.decoded_len
    }
    /// BER as a fraction in [0.0, 1.0]. NaN when `bits_compared == 0`.
    pub fn ber(&self) -> f32 {
        if self.bits_compared == 0 {
            f32::NAN
        } else {
            self.bit_errors as f32 / self.bits_compared as f32
        }
    }
}

/// Compute BER between an expected and a decoded payload.
pub fn compute_ber(expected: &[u8], decoded: &[u8]) -> BerReport {
    let n = expected.len().min(decoded.len());
    let mut bit_errors: u32 = 0;
    for i in 0..n {
        bit_errors += (expected[i] ^ decoded[i]).count_ones();
    }
    BerReport {
        bit_errors,
        bits_compared: (n as u32) * 8,
        expected_len: expected.len(),
        decoded_len: decoded.len(),
    }
}

// ─── WAV recording ──────────────────────────────────────────────────

/// Record `duration` of audio from `input`, polling `abort`, and write
/// it to `path` as a 48 kHz f32 mono WAV. Returns the actual capture
/// outcome — `Aborted` if the operator interrupted before duration
/// elapsed.
pub fn record_to_wav(
    input: &mut AudioInput,
    duration: Duration,
    abort: &AtomicBool,
    path: &Path,
) -> Result<(RecordOutcome, AudioBuffer), RxError> {
    let target_samples = (duration.as_secs_f32() * SAMPLE_RATE_HZ as f32) as usize;
    let (outcome, buffer) = input
        .record_blocking_with_abort(target_samples, abort)
        .map_err(RxError::Phy)?;
    buffer.write_wav(path).map_err(RxError::Phy)?;
    Ok((outcome, buffer))
}

// ─── Error type ─────────────────────────────────────────────────────

/// Top-level error type.
#[derive(Debug, Error)]
pub enum RxError {
    /// `--mode <name>` referenced a name not in the catalogue.
    #[error("unknown mode: {name} (try `wide-floor`)")]
    UnknownMode {
        /// The unrecognized name.
        name: String,
    },
    /// `--frame-mode <name>` referenced a name not in the catalogue.
    #[error("unknown frame mode: {name} (try `raw` or `sync`)")]
    UnknownFrameMode {
        /// The unrecognized name.
        name: String,
    },
    /// `--expected @<path>` couldn't read the file.
    #[error("expected file {path:?} could not be read: {io_error}")]
    ExpectedFileRead {
        /// The path the operator passed.
        path: String,
        /// The underlying I/O error.
        io_error: String,
    },
    /// Decode input was shorter than one OFDM symbol's worth.
    #[error("insufficient samples: got {got}, need {needed} for one symbol")]
    InsufficientSamples {
        /// The actual sample count provided.
        got: usize,
        /// The required sample count for one symbol of the chosen mode.
        needed: usize,
    },
    /// Underlying PHY / WAV / audio-device error.
    #[error("PHY error: {0}")]
    Phy(PhyError),
}

// ─── CLI argument parsing ───────────────────────────────────────────

/// Parsed CLI arguments. Exactly one of `list_devices`, `decode_wav`,
/// or `record_wav` is required.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    /// `--list-devices` flag.
    pub list_devices: bool,
    /// `--decode-wav <PATH>` requested.
    pub decode_wav: Option<PathBuf>,
    /// `--record-wav <PATH>` requested.
    pub record_wav: Option<PathBuf>,
    /// Mode requested via `--mode`. Defaults to `wide-floor` when
    /// decoding.
    pub mode: Option<String>,
    /// `--device <NAME>` (required for `--record-wav`).
    pub device: Option<String>,
    /// `--duration <SECS>` (required for `--record-wav`).
    pub duration_secs: Option<u32>,
    /// Optional `--expected <ARG>` for BER reporting on `--decode-wav`.
    pub expected: Option<String>,
    /// Frame format. `Raw` (default) decodes the first symbol-sized
    /// window; `Sync` finds the preamble in arbitrary-length captures.
    pub frame_mode: FrameMode,
    /// `--help` flag.
    pub help: bool,
}

impl Args {
    /// Parse argv-style args.
    pub fn parse(argv: &[String]) -> Result<Self, String> {
        let mut a = Args {
            list_devices: false,
            decode_wav: None,
            record_wav: None,
            mode: None,
            device: None,
            duration_secs: None,
            expected: None,
            frame_mode: FrameMode::Raw,
            help: false,
        };
        let mut iter = argv.iter().peekable();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--list-devices" => a.list_devices = true,
                "--decode-wav" => {
                    let v = iter
                        .next()
                        .ok_or_else(|| "--decode-wav requires a path".to_string())?;
                    a.decode_wav = Some(PathBuf::from(v));
                }
                "--record-wav" => {
                    let v = iter
                        .next()
                        .ok_or_else(|| "--record-wav requires a path".to_string())?;
                    a.record_wav = Some(PathBuf::from(v));
                }
                "--mode" => {
                    a.mode = Some(
                        iter.next()
                            .ok_or_else(|| "--mode requires a value".to_string())?
                            .clone(),
                    );
                }
                "--frame-mode" => {
                    let v = iter
                        .next()
                        .ok_or_else(|| "--frame-mode requires a value (raw|sync)".to_string())?;
                    a.frame_mode = FrameMode::parse(v).map_err(|e| e.to_string())?;
                }
                "--device" | "-d" => {
                    a.device = Some(
                        iter.next()
                            .ok_or_else(|| "--device requires a value".to_string())?
                            .clone(),
                    );
                }
                "--duration" => {
                    let v = iter
                        .next()
                        .ok_or_else(|| "--duration requires a value in seconds".to_string())?;
                    let n: u32 = v.parse().map_err(|_| {
                        format!("--duration must be an integer count of seconds: {v}")
                    })?;
                    if n == 0 {
                        return Err("--duration must be > 0".to_string());
                    }
                    a.duration_secs = Some(n);
                }
                "--expected" => {
                    a.expected = Some(
                        iter.next()
                            .ok_or_else(|| "--expected requires a value".to_string())?
                            .clone(),
                    );
                }
                "--help" | "-h" => a.help = true,
                other => return Err(format!("unknown argument: {other}")),
            }
        }
        Ok(a)
    }

    /// Validate mode-specific requirements. Exactly one of
    /// `--list-devices`, `--decode-wav`, `--record-wav` must be set;
    /// `--record-wav` additionally needs `--device` + `--duration`.
    pub fn validate(&self) -> Result<(), String> {
        let mode_count = (self.list_devices as u8)
            + (self.decode_wav.is_some() as u8)
            + (self.record_wav.is_some() as u8);
        if mode_count == 0 {
            return Err(
                "missing operation: pick one of --list-devices, --decode-wav <PATH>, \
                 or --record-wav <PATH>"
                    .to_string(),
            );
        }
        if mode_count > 1 {
            return Err(
                "operations are mutually exclusive: pick exactly one of --list-devices, \
                 --decode-wav, --record-wav"
                    .to_string(),
            );
        }
        if self.record_wav.is_some() {
            if self.device.is_none() {
                return Err("missing --device <NAME> (required for --record-wav)".to_string());
            }
            if self.duration_secs.is_none() {
                return Err(
                    "missing --duration <SECS> (required for --record-wav)".to_string()
                );
            }
        }
        Ok(())
    }
}

// ─── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    // ─── Mode ───────────────────────────────────────────────────────

    #[test]
    fn mode_parse_accepts_wide_floor() {
        assert_eq!(Mode::parse("wide-floor").unwrap(), Mode::WideFloor);
    }

    #[test]
    fn mode_parse_accepts_floor_wblo_alias() {
        assert_eq!(Mode::parse("floor-wblo").unwrap(), Mode::WideFloor);
    }

    #[test]
    fn mode_parse_rejects_unknown() {
        let err = Mode::parse("ofdm-mid").unwrap_err();
        assert!(matches!(err, RxError::UnknownMode { .. }));
    }

    #[test]
    fn mode_symbol_size_is_positive() {
        assert!(Mode::WideFloor.symbol_size_samples() > 0);
    }

    // ─── resolve_expected ───────────────────────────────────────────

    #[test]
    fn resolve_expected_plain_text_is_utf8_bytes() {
        assert_eq!(resolve_expected("hi").unwrap(), b"hi".to_vec());
    }

    #[test]
    fn resolve_expected_at_prefix_reads_file() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("tuxmodem-rx-test-exp-{}", std::process::id()));
        std::fs::write(&path, b"world").unwrap();
        let arg = format!("@{}", path.display());
        let got = resolve_expected(&arg).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(got, b"world".to_vec());
    }

    #[test]
    fn resolve_expected_missing_file_errors() {
        let err =
            resolve_expected("@/nonexistent/path/that/should/not/exist").unwrap_err();
        assert!(matches!(err, RxError::ExpectedFileRead { .. }));
    }

    // ─── decode_one_symbol + roundtrip ──────────────────────────────

    #[test]
    fn roundtrip_wide_floor_encode_then_decode_recovers_payload() {
        // The headline acceptance test: tuxmodem-tx's encoder + our
        // decoder are inverse operations on a clean channel. No
        // hardware, no WAV file, no audio device — purely lib-level.
        let payload = b"HELLO";
        let samples = WidebandLowDensityFloor::new().transmit(payload).unwrap();
        let decoded = decode_one_symbol(Mode::WideFloor, &samples, FrameMode::Raw).unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn roundtrip_wide_floor_handles_max_payload() {
        // 9 bytes is roughly the wide-floor single-symbol capacity
        // (74 data sub-carriers / 8 bits = 9 bytes; receive() trims
        // trailing zeros).
        let payload = b"ABCDEFGHI";
        let samples = WidebandLowDensityFloor::new().transmit(payload).unwrap();
        let decoded = decode_one_symbol(Mode::WideFloor, &samples, FrameMode::Raw).unwrap();
        // The decoder trims trailing zeros so payloads ending in 0x00
        // would round-trip lossy; our ASCII payload has no NULs so
        // it round-trips clean.
        assert_eq!(decoded, payload);
    }

    #[test]
    fn decode_insufficient_samples_errors() {
        let too_short = vec![0.0_f32; 10];
        let err = decode_one_symbol(Mode::WideFloor, &too_short, FrameMode::Raw).unwrap_err();
        assert!(matches!(err, RxError::InsufficientSamples { .. }));
    }

    #[test]
    fn decode_uses_only_first_symbol_size_samples() {
        // Encode a known payload; pad the buffer with garbage AFTER
        // the symbol. The decoder should ignore the trailing garbage.
        let payload = b"hi";
        let mut samples = WidebandLowDensityFloor::new().transmit(payload).unwrap();
        samples.extend(std::iter::repeat(0.5_f32).take(10_000));
        let decoded = decode_one_symbol(Mode::WideFloor, &samples, FrameMode::Raw).unwrap();
        assert_eq!(decoded, payload);
    }

    // ─── WAV roundtrip ──────────────────────────────────────────────

    #[test]
    fn wav_roundtrip_via_audiobuffer() {
        // Encode → write_wav → read_wav → decode. Operator's
        // workflow boils down to this minus the manual SDR/file
        // step in between.
        let payload = b"WAV";
        let samples = WidebandLowDensityFloor::new().transmit(payload).unwrap();
        let buf = AudioBuffer::from_samples(samples);
        let dir = std::env::temp_dir();
        let path = dir.join(format!("tuxmodem-rx-test-wav-{}.wav", std::process::id()));
        buf.write_wav(&path).unwrap();
        let read_back = read_wav(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let decoded = decode_one_symbol(Mode::WideFloor, read_back.samples(), FrameMode::Raw).unwrap();
        assert_eq!(decoded, payload);
    }

    // ─── BER ────────────────────────────────────────────────────────

    #[test]
    fn ber_clean_when_payloads_match() {
        let r = compute_ber(b"abc", b"abc");
        assert!(r.is_clean());
        assert_eq!(r.bit_errors, 0);
        assert_eq!(r.bits_compared, 24);
        assert_eq!(r.ber(), 0.0);
    }

    #[test]
    fn ber_counts_single_bit_diff() {
        // 'a' = 0x61, 'A' = 0x41 → XOR = 0x20 → popcount 1
        let r = compute_ber(b"a", b"A");
        assert_eq!(r.bit_errors, 1);
        assert_eq!(r.bits_compared, 8);
        assert!(!r.is_clean());
        assert!(!r.len_mismatch());
        assert!((r.ber() - 0.125).abs() < 1e-6);
    }

    #[test]
    fn ber_handles_length_mismatch() {
        let r = compute_ber(b"abc", b"ab");
        assert!(r.len_mismatch());
        assert_eq!(r.bits_compared, 16); // min(3,2) × 8
        assert!(!r.is_clean());
    }

    #[test]
    fn ber_empty_inputs_yield_nan_ber() {
        let r = compute_ber(&[], &[]);
        assert!(r.ber().is_nan());
        // Vacuously matching: zero bit errors and zero length diff.
        assert!(r.is_clean());
    }

    #[test]
    fn ber_all_bits_differ_is_one() {
        let r = compute_ber(&[0x00], &[0xff]);
        assert_eq!(r.bit_errors, 8);
        assert!((r.ber() - 1.0).abs() < 1e-6);
    }

    // ─── Args ───────────────────────────────────────────────────────

    #[test]
    fn args_parse_list_devices() {
        let a = Args::parse(&s(&["--list-devices"])).unwrap();
        assert!(a.list_devices);
        a.validate().unwrap();
    }

    #[test]
    fn args_parse_decode_wav_minimal() {
        let a = Args::parse(&s(&["--decode-wav", "/tmp/in.wav"])).unwrap();
        assert_eq!(a.decode_wav.as_deref(), Some(Path::new("/tmp/in.wav")));
        a.validate().unwrap();
    }

    #[test]
    fn args_parse_record_wav_requires_device_and_duration() {
        let a = Args::parse(&s(&["--record-wav", "/tmp/out.wav"])).unwrap();
        let err = a.validate().unwrap_err();
        assert!(err.contains("--device") || err.contains("--duration"));
    }

    #[test]
    fn args_parse_record_wav_with_all_required_validates() {
        let a = Args::parse(&s(&[
            "--record-wav", "/tmp/out.wav", "--device", "USB Audio", "--duration", "3",
        ]))
        .unwrap();
        a.validate().unwrap();
    }

    #[test]
    fn args_parse_help_flag() {
        assert!(Args::parse(&s(&["--help"])).unwrap().help);
    }

    #[test]
    fn args_parse_decode_wav_with_expected() {
        let a = Args::parse(&s(&[
            "--decode-wav", "/tmp/in.wav", "--expected", "HELLO",
        ]))
        .unwrap();
        assert_eq!(a.expected.as_deref(), Some("HELLO"));
    }

    #[test]
    fn args_parse_rejects_mutually_exclusive_modes() {
        let a = Args::parse(&s(&[
            "--list-devices", "--decode-wav", "/tmp/in.wav",
        ]))
        .unwrap();
        let err = a.validate().unwrap_err();
        assert!(err.contains("mutually exclusive"));
    }

    #[test]
    fn args_parse_rejects_no_operation() {
        let a = Args::parse(&s(&[])).unwrap();
        let err = a.validate().unwrap_err();
        assert!(err.contains("missing operation"));
    }

    #[test]
    fn args_parse_rejects_unknown_flag() {
        let err = Args::parse(&s(&["--gibberish"])).unwrap_err();
        assert!(err.contains("unknown argument"));
    }

    #[test]
    fn args_parse_rejects_zero_duration() {
        let err = Args::parse(&s(&["--duration", "0"])).unwrap_err();
        assert!(err.contains("--duration"));
    }

    #[test]
    fn args_parse_rejects_non_numeric_duration() {
        let err = Args::parse(&s(&["--duration", "forever"])).unwrap_err();
        assert!(err.contains("--duration"));
    }

    // ─── FrameMode (Phase 12 slice 2, tuxlink-fxmc) ─────────────────

    #[test]
    fn frame_mode_default_is_raw() {
        assert_eq!(FrameMode::default(), FrameMode::Raw);
    }

    #[test]
    fn frame_mode_parse_accepts_raw_and_sync() {
        assert_eq!(FrameMode::parse("raw").unwrap(), FrameMode::Raw);
        assert_eq!(FrameMode::parse("sync").unwrap(), FrameMode::Sync);
    }

    #[test]
    fn frame_mode_parse_rejects_unknown() {
        let err = FrameMode::parse("upside-down").unwrap_err();
        assert!(matches!(err, RxError::UnknownFrameMode { .. }));
    }

    #[test]
    fn frame_mode_short_name_round_trips() {
        for m in [FrameMode::Raw, FrameMode::Sync] {
            assert_eq!(FrameMode::parse(m.short_name()).unwrap(), m);
        }
    }

    #[test]
    fn decode_sync_roundtrip_via_transmit_with_preamble() {
        // The headline sync-side acceptance test: tuxmodem-phy's
        // transmit_with_preamble produces preamble + symbol; we decode
        // it via sync mode and recover the payload.
        let payload = b"HELLO";
        let samples = WidebandLowDensityFloor::new()
            .transmit_with_preamble(payload)
            .unwrap();
        let decoded =
            decode_one_symbol(Mode::WideFloor, &samples, FrameMode::Sync).unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn decode_sync_handles_leading_silence() {
        // The operational reason --frame-mode sync exists: arbitrary-
        // length captures (e.g. an off-air WAV with silence before the
        // signal) decode without manual trimming.
        let payload = b"OFFSET";
        let core = WidebandLowDensityFloor::new()
            .transmit_with_preamble(payload)
            .unwrap();
        let mut samples = vec![0.0_f32; 1000];
        samples.extend_from_slice(&core);
        let decoded =
            decode_one_symbol(Mode::WideFloor, &samples, FrameMode::Sync).unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn decode_sync_with_offset_reports_start_sample() {
        // decode_one_symbol_with_offset returns Some(start) in sync mode.
        let payload = b"HELLO";
        let core = WidebandLowDensityFloor::new()
            .transmit_with_preamble(payload)
            .unwrap();
        let mut samples = vec![0.0_f32; 500];
        samples.extend_from_slice(&core);
        let (start, decoded) =
            decode_one_symbol_with_offset(Mode::WideFloor, &samples, FrameMode::Sync).unwrap();
        assert_eq!(decoded, payload);
        let start = start.expect("sync mode should return Some(start)");
        // Detector finds the preamble at sample ~500 (±2).
        let offset_err = (start as i64 - 500).unsigned_abs() as usize;
        assert!(
            offset_err <= 2,
            "detected start {start} should be within ±2 of expected 500"
        );
    }

    #[test]
    fn decode_sync_returns_phy_error_on_silence() {
        let silence = vec![0.0_f32; 10_000];
        let err =
            decode_one_symbol(Mode::WideFloor, &silence, FrameMode::Sync).unwrap_err();
        assert!(matches!(err, RxError::Phy(PhyError::FrameDetect(_))));
    }

    #[test]
    fn decode_raw_still_returns_insufficient_samples_on_short_input() {
        // Regression: the raw-mode error path is unchanged from PR #367.
        let too_short = vec![0.0_f32; 10];
        let err =
            decode_one_symbol(Mode::WideFloor, &too_short, FrameMode::Raw).unwrap_err();
        assert!(matches!(err, RxError::InsufficientSamples { .. }));
    }

    #[test]
    fn decode_one_symbol_with_offset_raw_returns_none_start() {
        // In raw mode the start sample concept doesn't apply; we
        // return None.
        let payload = b"hi";
        let samples = WidebandLowDensityFloor::new().transmit(payload).unwrap();
        let (start, decoded) =
            decode_one_symbol_with_offset(Mode::WideFloor, &samples, FrameMode::Raw).unwrap();
        assert_eq!(start, None);
        assert_eq!(decoded, payload);
    }

    #[test]
    fn args_parse_frame_mode_sync() {
        let a = Args::parse(&s(&[
            "--decode-wav", "/tmp/in.wav", "--frame-mode", "sync",
        ]))
        .unwrap();
        assert_eq!(a.frame_mode, FrameMode::Sync);
    }

    #[test]
    fn args_parse_frame_mode_default_is_raw_when_omitted() {
        let a = Args::parse(&s(&["--decode-wav", "/tmp/in.wav"])).unwrap();
        assert_eq!(a.frame_mode, FrameMode::Raw);
    }

    #[test]
    fn args_parse_frame_mode_unknown_value_errors() {
        let err = Args::parse(&s(&["--frame-mode", "interplanetary"])).unwrap_err();
        assert!(err.contains("frame mode"));
    }

    #[test]
    fn args_parse_frame_mode_without_value_errors() {
        let err = Args::parse(&s(&["--frame-mode"])).unwrap_err();
        assert!(err.contains("--frame-mode"));
    }

    #[test]
    fn end_to_end_sync_roundtrip_via_wav_file() {
        // End-to-end via WAV file: encode + preamble → write_wav →
        // read_wav → decode sync → recover. Closes the CLI workflow
        // story: tx --write-wav --frame-mode sync + rx --decode-wav
        // --frame-mode sync produces a CLEAN MATCH without manual
        // trimming.
        let payload = b"WAVSYNC";
        let buffer = AudioBuffer::from_samples(
            WidebandLowDensityFloor::new()
                .transmit_with_preamble(payload)
                .unwrap(),
        );
        let dir = std::env::temp_dir();
        let path = dir.join(format!(
            "tuxmodem-rx-sync-roundtrip-{}.wav",
            std::process::id()
        ));
        buffer.write_wav(&path).unwrap();
        let read_back = read_wav(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let decoded =
            decode_one_symbol(Mode::WideFloor, read_back.samples(), FrameMode::Sync).unwrap();
        assert_eq!(decoded, payload);
    }

    // ─── MultiSync (Phase 10 slice 3, tuxlink-ot37) ─────────────────

    #[test]
    fn frame_mode_parse_accepts_multi_sync() {
        assert_eq!(FrameMode::parse("multi-sync").unwrap(), FrameMode::MultiSync);
    }

    #[test]
    fn frame_mode_short_name_multi_sync_round_trips() {
        assert_eq!(
            FrameMode::parse(FrameMode::MultiSync.short_name()).unwrap(),
            FrameMode::MultiSync
        );
    }

    #[test]
    fn decode_multi_sync_roundtrip_via_transmit_multi_with_preamble() {
        let payload = b"HELLO_MULTI";
        let samples = WidebandLowDensityFloor::new()
            .transmit_multi_with_preamble(payload)
            .unwrap();
        let decoded =
            decode_one_symbol(Mode::WideFloor, &samples, FrameMode::MultiSync).unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn decode_multi_sync_handles_100_byte_payload() {
        let payload: Vec<u8> = (0..100).map(|i| (i * 13 % 251) as u8).collect();
        let samples = WidebandLowDensityFloor::new()
            .transmit_multi_with_preamble(&payload)
            .unwrap();
        let decoded =
            decode_one_symbol(Mode::WideFloor, &samples, FrameMode::MultiSync).unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn decode_multi_sync_handles_1000_byte_payload() {
        let payload: Vec<u8> = (0..1000).map(|i| (i % 251) as u8).collect();
        let samples = WidebandLowDensityFloor::new()
            .transmit_multi_with_preamble(&payload)
            .unwrap();
        let decoded =
            decode_one_symbol(Mode::WideFloor, &samples, FrameMode::MultiSync).unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn decode_multi_sync_handles_leading_silence() {
        let payload: Vec<u8> = (0..30).map(|i| (i * 7 % 251) as u8).collect();
        let core = WidebandLowDensityFloor::new()
            .transmit_multi_with_preamble(&payload)
            .unwrap();
        let mut samples = vec![0.0_f32; 1500];
        samples.extend_from_slice(&core);
        let decoded =
            decode_one_symbol(Mode::WideFloor, &samples, FrameMode::MultiSync).unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn decode_multi_sync_with_offset_reports_preamble_start() {
        let payload = b"OFFSET";
        let core = WidebandLowDensityFloor::new()
            .transmit_multi_with_preamble(payload)
            .unwrap();
        let mut samples = vec![0.0_f32; 800];
        samples.extend_from_slice(&core);
        let (start, decoded) = decode_one_symbol_with_offset(
            Mode::WideFloor,
            &samples,
            FrameMode::MultiSync,
        )
        .unwrap();
        let start = start.expect("multi-sync mode should return Some(start)");
        let offset_err = (start as i64 - 800).unsigned_abs() as usize;
        assert!(
            offset_err <= 2,
            "detected start {start} should be within ±2 of expected 800"
        );
        assert_eq!(decoded, payload);
    }

    #[test]
    fn decode_multi_sync_returns_phy_error_on_silence() {
        let silence = vec![0.0_f32; 10_000];
        let err = decode_one_symbol(Mode::WideFloor, &silence, FrameMode::MultiSync)
            .unwrap_err();
        assert!(matches!(err, RxError::Phy(PhyError::FrameDetect(_))));
    }

    #[test]
    fn args_parse_frame_mode_multi_sync() {
        let a = Args::parse(&s(&[
            "--decode-wav", "/tmp/in.wav", "--frame-mode", "multi-sync",
        ]))
        .unwrap();
        assert_eq!(a.frame_mode, FrameMode::MultiSync);
    }

    #[test]
    fn end_to_end_multi_sync_roundtrip_via_wav_file() {
        // Mirrors end_to_end_sync_roundtrip_via_wav_file but with a
        // 100-byte payload that Sync (single-symbol) couldn't carry.
        let payload: Vec<u8> = (0..100).map(|i| ((i * 17 + 3) % 251) as u8).collect();
        let buffer = AudioBuffer::from_samples(
            WidebandLowDensityFloor::new()
                .transmit_multi_with_preamble(&payload)
                .unwrap(),
        );
        let dir = std::env::temp_dir();
        let path = dir.join(format!(
            "tuxmodem-rx-multi-sync-roundtrip-{}.wav",
            std::process::id()
        ));
        buffer.write_wav(&path).unwrap();
        let read_back = read_wav(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let decoded = decode_one_symbol(
            Mode::WideFloor,
            read_back.samples(),
            FrameMode::MultiSync,
        )
        .unwrap();
        assert_eq!(decoded, payload);
    }
}
