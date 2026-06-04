//! tuxmodem-tx — payload → PHY → PTT + audio composition.
//!
//! Phase 3 of the tuxmodem hardware bring-up (tuxlink-i3bz / tuxlink-9ggl).
//! Composes three already-shipped layers:
//!
//! - [`tuxmodem_phy::robustness_floor::wideband_lowdensity::WidebandLowDensityFloor`] — the PHY encoder
//! - [`tuxmodem_phy::audio_device::AudioOutput`] — real-time soundcard output
//! - [`tux_rig_rts::RtsPtt`] — Digirig + G90 PTT primitive (serial-RTS)
//!
//! into a single binary that takes a payload and transmits it through a
//! real radio. The library exposes the testable pieces — argument
//! parsing, payload resolution, airtime budgeting, transmission
//! orchestration over abstract `Ptt` + `AbortablePlay` traits. The
//! `bin/tuxmodem-tx.rs` wires the orchestration to the Linux-specific
//! [`LinuxTty`] + CPAL [`AudioOutput`] for production.
//!
//! ## Safety primitives (bd issue tuxlink-i3bz)
//!
//! - **PTT lead-in.** [`DEFAULT_LEAD_IN`] (~100 ms) elapses between PTT
//!   assert and the first sample reaching the soundcard. Without this
//!   the radio's TX chain isn't fully keyed when the waveform starts
//!   and the preamble gets chopped.
//! - **Bounded total airtime.** [`AirtimeBudget::total`] sums all four
//!   components (lead-in + buffer + tail-drain + setup slack);
//!   [`check_budget`] rejects configurations exceeding the caller's
//!   max BEFORE PTT is asserted. [`DEFAULT_MAX_AIRTIME`] = 30 s and
//!   [`HARD_CAP_AIRTIME`] = 60 s.
//! - **`--dry-run`.** Encodes the payload + reports duration WITHOUT
//!   opening any device. Validates the encode pipeline without RF risk.
//! - **SIGINT/SIGTERM early-release.** Caller drives the abort flag;
//!   [`run_transmission`] checks it during the play loop and, when
//!   raised, drops the audio stream + releases PTT immediately.
//! - **Agent does NOT run the binary against the real device** (RADIO-1).
//!   The operator is the licensee.
//!
//! [`LinuxTty`]: tux_rig_rts::LinuxTty
//! [`AudioOutput`]: tuxmodem_phy::audio_device::AudioOutput

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use thiserror::Error;
use tux_rig_rts::{Ptt, PttState};
use tuxmodem_phy::audio_device::PlayOutcome;
use tuxmodem_phy::audio_io::AudioBuffer;
use tuxmodem_phy::error::PhyError;
use tuxmodem_phy::robustness_floor::wideband_lowdensity::WidebandLowDensityFloor;

// ─── Constants ──────────────────────────────────────────────────────

/// Default PTT lead-in: time between asserting PTT and starting audio.
/// 100 ms is the operator-empirical threshold below which HF radios
/// chop the preamble of the OFDM waveform.
pub const DEFAULT_LEAD_IN: Duration = Duration::from_millis(100);

/// Default tail-drain budget reserved for the soundcard's internal
/// ring after the last sample is queued. Matches the constant inside
/// [`tuxmodem_phy::audio_device::AudioOutput::play_blocking_with_abort`].
pub const DEFAULT_TAIL_DRAIN: Duration = Duration::from_millis(100);

/// Default setup-slack budget reserved for CPAL stream construction +
/// the PHY encoder. Empirically a few hundred ms; the budget gate
/// uses this so callers don't have to.
pub const DEFAULT_SETUP_SLACK: Duration = Duration::from_millis(200);

/// Default maximum total airtime allowed for one transmission. The
/// budget gate rejects configurations exceeding this BEFORE PTT
/// assert.
pub const DEFAULT_MAX_AIRTIME: Duration = Duration::from_secs(30);

/// Hard ceiling. No configuration may exceed this regardless of the
/// caller's max; protects against an over-zealous `--max-airtime`
/// override. The bd issue (tuxlink-i3bz) pinned 60 s.
pub const HARD_CAP_AIRTIME: Duration = Duration::from_secs(60);

// ─── Modes ──────────────────────────────────────────────────────────

/// Which PHY mode to encode under. Phase 3 ships with one mode — the
/// robustness floor's wide-band low-density OFDM — per the bd issue's
/// "wide-floor is the FIRST mode to test" pin. Richer modes get
/// added here as the PHY ladder lights up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// `floor-wblo` (wide-band low-density OFDM, BPSK / sub-carrier).
    /// The robustness-floor default — designed for "operator hears
    /// noise but the signal carries."
    WideFloor,
}

impl Mode {
    /// Parse a mode-name string. The short-name vocabulary follows
    /// the PHY's [`tuxmodem_phy::modes::ModeDescriptor::short_name`]
    /// convention — kebab-case identifiers.
    pub fn parse(name: &str) -> Result<Self, TxError> {
        match name {
            "wide-floor" | "floor-wblo" => Ok(Self::WideFloor),
            other => Err(TxError::UnknownMode {
                name: other.to_string(),
            }),
        }
    }

    /// Stable kebab-case identifier (matches what `Mode::parse`
    /// accepts).
    pub fn short_name(&self) -> &'static str {
        match self {
            Self::WideFloor => "wide-floor",
        }
    }
}

/// Frame format selection. `Raw` emits only the OFDM symbol — the
/// v0.0.1 wire format, used for back-to-back loopback where alignment
/// is implicit. `Sync` prepends the [`PREAMBLE_LEN_SAMPLES`]-sample
/// Zadoff-Chu preamble so a receiver can find the symbol in an
/// arbitrary-length capture (PHY Phase 12 slice 1 / tuxlink-iyl9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FrameMode {
    /// Bare OFDM symbol, no preamble. The v0.0.1 default — preserved
    /// here so existing tooling stays bit-compatible.
    #[default]
    Raw,
    /// Zadoff-Chu preamble (192 samples / 4 ms @ 48 kHz) prepended to
    /// a single OFDM symbol. Payload limited to one symbol's capacity
    /// (~9 bytes for the Wide mode). Pairs with `tuxmodem-rx --frame-mode sync`.
    Sync,
    /// Zadoff-Chu preamble + N OFDM symbols carrying a 2-byte length-
    /// prefix header. Supports arbitrary-length payloads up to
    /// u16::MAX bytes. Pairs with `tuxmodem-rx --frame-mode multi-sync`.
    MultiSync,
}

impl FrameMode {
    /// Parse a `--frame-mode` argument value.
    pub fn parse(name: &str) -> Result<Self, TxError> {
        match name {
            "raw" => Ok(Self::Raw),
            "sync" => Ok(Self::Sync),
            "multi-sync" => Ok(Self::MultiSync),
            other => Err(TxError::UnknownFrameMode {
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

/// Sample count of the preamble that `FrameMode::Sync` prepends. Mirrors
/// [`tuxmodem_phy::robustness_floor::wideband_lowdensity::PREAMBLE_LEN_SAMPLES`].
pub const PREAMBLE_LEN_SAMPLES: usize = 192;

// ─── Payload resolution ─────────────────────────────────────────────

/// Resolve a `--payload` argument value to its byte sequence. Two
/// forms:
///
/// - `text` (any non-`@`-prefixed string) → UTF-8 bytes of the string.
/// - `@<path>` → read the file at `<path>` as bytes. Lets the operator
///   transmit binary blobs without shell-escaping.
pub fn resolve_payload(arg: &str) -> Result<Vec<u8>, TxError> {
    if let Some(path_str) = arg.strip_prefix('@') {
        let path = Path::new(path_str);
        std::fs::read(path).map_err(|e| TxError::PayloadFileRead {
            path: path.display().to_string(),
            io_error: e.to_string(),
        })
    } else {
        Ok(arg.as_bytes().to_vec())
    }
}

// ─── Encoding ───────────────────────────────────────────────────────

/// Encode a payload into an [`AudioBuffer`] under the chosen mode +
/// frame format.
///
/// For [`Mode::WideFloor`]:
/// - [`FrameMode::Raw`] delegates to [`WidebandLowDensityFloor::transmit`]
///   (bare OFDM symbol; v0.0.1 wire format).
/// - [`FrameMode::Sync`] delegates to
///   [`WidebandLowDensityFloor::transmit_with_preamble`] (preamble +
///   OFDM symbol; receiver-friendly format).
///
/// Returns [`TxError::Phy`] when the payload exceeds the mode's
/// per-symbol capacity (currently ~9 bytes at BPSK over the 74 data
/// sub-carriers of the Wide-mode OFDM grid; multi-symbol framing
/// arrives in PHY Phase 10).
pub fn encode_payload(
    mode: Mode,
    payload: &[u8],
    frame_mode: FrameMode,
) -> Result<AudioBuffer, TxError> {
    let floor = WidebandLowDensityFloor::new();
    let samples = match (mode, frame_mode) {
        (Mode::WideFloor, FrameMode::Raw) => {
            floor.transmit(payload).map_err(TxError::Phy)?
        }
        (Mode::WideFloor, FrameMode::Sync) => floor
            .transmit_with_preamble(payload)
            .map_err(TxError::Phy)?,
        (Mode::WideFloor, FrameMode::MultiSync) => floor
            .transmit_multi_with_preamble(payload)
            .map_err(TxError::Phy)?,
    };
    Ok(AudioBuffer::from_samples(samples))
}

// ─── Airtime budgeting ──────────────────────────────────────────────

/// Components of a transmission's worst-case wall-clock airtime.
/// The total — what [`check_budget`] gates against — is the sum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AirtimeBudget {
    /// PTT lead-in: time between assert + first sample reaching the
    /// soundcard.
    pub lead_in: Duration,
    /// The audio buffer's playback duration at 48 kHz.
    pub buffer_duration: Duration,
    /// Tail-drain budget reserved for the soundcard's internal ring.
    pub tail_drain: Duration,
    /// Setup-slack budget reserved for stream construction + encode.
    pub setup_slack: Duration,
}

impl AirtimeBudget {
    /// Construct a budget from an [`AudioBuffer`] using the default
    /// lead-in / tail-drain / slack constants. Use when the operator
    /// hasn't overridden any of them.
    pub fn from_buffer_defaults(buffer: &AudioBuffer) -> Self {
        Self {
            lead_in: DEFAULT_LEAD_IN,
            buffer_duration: Duration::from_secs_f32(buffer.duration_seconds()),
            tail_drain: DEFAULT_TAIL_DRAIN,
            setup_slack: DEFAULT_SETUP_SLACK,
        }
    }

    /// Sum of all four components — the worst-case wall-clock time
    /// the radio will spend keyed.
    pub fn total(&self) -> Duration {
        self.lead_in + self.buffer_duration + self.tail_drain + self.setup_slack
    }
}

/// Verify that the budget fits under both the caller-supplied max and
/// the [`HARD_CAP_AIRTIME`] ceiling. Returns the effective cap (the
/// lower of the two) on success; rejects with [`TxError::AirtimeExceeded`]
/// when `budget.total()` exceeds it.
pub fn check_budget(budget: &AirtimeBudget, max: Duration) -> Result<Duration, TxError> {
    let effective = max.min(HARD_CAP_AIRTIME);
    let total = budget.total();
    if total > effective {
        Err(TxError::AirtimeExceeded {
            actual: total,
            max: effective,
        })
    } else {
        Ok(effective)
    }
}

// ─── Player abstraction (test seam) ─────────────────────────────────

/// Abortable playback abstraction over
/// [`tuxmodem_phy::audio_device::AudioOutput`]. Lets
/// [`run_transmission`] be exercised in unit tests without CPAL —
/// production wires up to the real `AudioOutput`; tests wire up to a
/// recording mock that asserts on call ordering.
pub trait AbortablePlay {
    /// Play `buffer`, polling `abort` for early termination.
    fn play_blocking_with_abort(
        &mut self,
        buffer: &AudioBuffer,
        abort: &AtomicBool,
    ) -> Result<PlayOutcome, PhyError>;
}

impl AbortablePlay for tuxmodem_phy::audio_device::AudioOutput {
    fn play_blocking_with_abort(
        &mut self,
        buffer: &AudioBuffer,
        abort: &AtomicBool,
    ) -> Result<PlayOutcome, PhyError> {
        Self::play_blocking_with_abort(self, buffer, abort)
    }
}

// ─── Orchestration ──────────────────────────────────────────────────

/// Outcome of a [`run_transmission`] call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxOutcome {
    /// Buffer played in full; PTT released cleanly.
    Completed,
    /// Caller's abort flag was observed during playback; the audio
    /// stream was dropped early and PTT released.
    AbortedEarly,
}

/// Execute the assert → lead-in → play → release sequence.
///
/// Pre-conditions: caller has already verified the [`AirtimeBudget`]
/// with [`check_budget`] (this function does NOT re-check — it trusts
/// the budget gate ran first).
///
/// Sequence:
///
/// 1. `ptt.assert()` — radio's TX chain begins to key up.
/// 2. `sleep(lead_in)` — by the time the lead-in elapses, the radio
///    is fully keyed and ready to accept audio.
/// 3. `player.play_blocking_with_abort(buffer, abort)` — the waveform
///    streams to the soundcard. Polls `abort` every ~20 ms.
/// 4. `ptt.release()` — radio's TX chain unkeys.
///
/// On abort: step 3 returns [`PlayOutcome::Aborted`]; we still run
/// step 4. On any error from steps 1, 3, or 4: the [`Ptt`] impl's
/// `Drop` is the backstop (every shipped backend's Drop releases
/// when the state is `Asserted`).
pub fn run_transmission<P, A>(
    ptt: &mut P,
    player: &mut A,
    buffer: &AudioBuffer,
    lead_in: Duration,
    abort: &AtomicBool,
) -> Result<TxOutcome, TxError>
where
    P: Ptt,
    P::Error: std::fmt::Display + Send + Sync + 'static,
    A: AbortablePlay,
{
    ptt.assert()
        .map_err(|e| TxError::PttAssert(e.to_string()))?;
    debug_assert_eq!(ptt.state(), PttState::Asserted);

    // Lead-in. A tight sleep is fine — the only thing we miss while
    // sleeping is signal delivery latency, and the next op (play) is
    // itself a long blocking call that polls the abort flag.
    let lead_in_start = Instant::now();
    while lead_in_start.elapsed() < lead_in {
        if abort.load(std::sync::atomic::Ordering::Acquire) {
            // Operator aborted before audio even started — go straight
            // to release.
            let release_result = ptt.release();
            release_result.map_err(|e| TxError::PttRelease(e.to_string()))?;
            return Ok(TxOutcome::AbortedEarly);
        }
        std::thread::sleep(Duration::from_millis(20).min(lead_in - lead_in_start.elapsed()));
    }

    let play_result = player.play_blocking_with_abort(buffer, abort);

    // Always release PTT, even when play_blocking returned an error.
    // If the release itself fails, the PTT's Drop impl is the last
    // backstop.
    let release_result = ptt.release();

    let play_outcome = play_result.map_err(TxError::Phy)?;
    release_result.map_err(|e| TxError::PttRelease(e.to_string()))?;

    Ok(match play_outcome {
        PlayOutcome::Completed => TxOutcome::Completed,
        PlayOutcome::Aborted => TxOutcome::AbortedEarly,
    })
}

// ─── Error type ─────────────────────────────────────────────────────

/// Top-level error type. Variants carry enough context for the bin's
/// human-readable error reporting.
#[derive(Debug, Error)]
pub enum TxError {
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
    /// `--payload @<path>` couldn't read the file.
    #[error("payload file {path:?} could not be read: {io_error}")]
    PayloadFileRead {
        /// The path the operator passed.
        path: String,
        /// The underlying I/O error.
        io_error: String,
    },
    /// The estimated total airtime exceeds the budget gate.
    #[error(
        "estimated airtime {} ms exceeds budget {} ms — \
        either pick a smaller payload or raise --max-airtime (hard cap: {} s)",
        .actual.as_millis(),
        .max.as_millis(),
        HARD_CAP_AIRTIME.as_secs(),
    )]
    AirtimeExceeded {
        /// What we would have transmitted.
        actual: Duration,
        /// What we'd allowed.
        max: Duration,
    },
    /// The PHY encoder rejected the payload (most commonly:
    /// payload too large for the chosen mode's per-symbol capacity).
    #[error("PHY error: {0}")]
    Phy(PhyError),
    /// `Ptt::assert` returned an error.
    #[error("PTT assert failed: {0}")]
    PttAssert(String),
    /// `Ptt::release` returned an error.
    #[error("PTT release failed: {0}")]
    PttRelease(String),
}

// ─── CLI argument parsing ───────────────────────────────────────────

/// Parsed CLI arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    /// Mode requested via `--mode`. `None` is only valid for `--help`.
    pub mode: Option<String>,
    /// Payload arg (text or `@<path>`) requested via `--payload`.
    pub payload: Option<String>,
    /// Audio device name requested via `--device`. Required unless
    /// `dry_run` is set.
    pub device: Option<String>,
    /// PTT tty path requested via `--ptt-device`. Required unless
    /// `dry_run` is set.
    pub ptt_device: Option<String>,
    /// Frame format. `Raw` (default) emits a bare OFDM symbol; `Sync`
    /// prepends the Zadoff-Chu preamble for receiver-friendly framing.
    pub frame_mode: FrameMode,
    /// Encode + report ONLY; don't open any device.
    pub dry_run: bool,
    /// Encode + write the waveform to a 48 kHz f32 mono WAV at this
    /// path. No audio device opens, no PTT asserts. Mutually exclusive
    /// with full-TX mode (`--device` + `--ptt-device`); the dry-run
    /// flag is a strict subset (also no device, also no PTT, but
    /// additionally no file is written).
    pub write_wav: Option<PathBuf>,
    /// Override [`DEFAULT_MAX_AIRTIME`] for this run. Hard-capped at
    /// [`HARD_CAP_AIRTIME`].
    pub max_airtime: Option<Duration>,
    /// User asked for `--help`.
    pub help: bool,
}

impl Args {
    /// Parse argv-style args (the binary's `env::args().skip(1)`).
    pub fn parse(argv: &[String]) -> Result<Self, String> {
        let mut args = Args {
            mode: None,
            payload: None,
            device: None,
            ptt_device: None,
            frame_mode: FrameMode::Raw,
            dry_run: false,
            write_wav: None,
            max_airtime: None,
            help: false,
        };
        let mut iter = argv.iter().peekable();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--mode" => {
                    args.mode = Some(
                        iter.next()
                            .ok_or_else(|| "--mode requires a value".to_string())?
                            .clone(),
                    );
                }
                "--frame-mode" => {
                    let v = iter
                        .next()
                        .ok_or_else(|| "--frame-mode requires a value (raw|sync)".to_string())?;
                    args.frame_mode = FrameMode::parse(v).map_err(|e| e.to_string())?;
                }
                "--payload" => {
                    args.payload = Some(
                        iter.next()
                            .ok_or_else(|| "--payload requires a value".to_string())?
                            .clone(),
                    );
                }
                "--device" | "-d" => {
                    args.device = Some(
                        iter.next()
                            .ok_or_else(|| "--device requires a value".to_string())?
                            .clone(),
                    );
                }
                "--ptt-device" | "-p" => {
                    args.ptt_device = Some(
                        iter.next()
                            .ok_or_else(|| "--ptt-device requires a value".to_string())?
                            .clone(),
                    );
                }
                "--dry-run" => args.dry_run = true,
                "--write-wav" => {
                    let v = iter
                        .next()
                        .ok_or_else(|| "--write-wav requires a path".to_string())?;
                    args.write_wav = Some(PathBuf::from(v));
                }
                "--max-airtime" => {
                    let v = iter
                        .next()
                        .ok_or_else(|| "--max-airtime requires a value in seconds".to_string())?;
                    let secs = v.parse::<u64>().map_err(|_| {
                        format!("--max-airtime must be an integer count of seconds: {v}")
                    })?;
                    args.max_airtime = Some(Duration::from_secs(secs));
                }
                "--help" | "-h" => args.help = true,
                other => return Err(format!("unknown argument: {other}")),
            }
        }
        Ok(args)
    }

    /// Validate the parsed args against the chosen output mode.
    ///
    /// Three output modes exist:
    /// - **dry-run** (`--dry-run`): encode + report; no device, no
    ///   PTT, no file written.
    /// - **write-wav** (`--write-wav <PATH>`): encode + write waveform
    ///   to a 48 kHz f32 mono WAV file; no device, no PTT.
    /// - **full-tx** (default): encode + assert PTT + play to
    ///   `--device`; requires `--device` + `--ptt-device`.
    ///
    /// `--dry-run` and `--write-wav` are mutually exclusive (they
    /// disagree on whether to write the file). Setting both errors.
    pub fn validate(&self) -> Result<(), String> {
        if self.mode.is_none() {
            return Err("missing --mode <name> (try `--mode wide-floor`)".to_string());
        }
        if self.payload.is_none() {
            return Err("missing --payload <text|@file>".to_string());
        }
        if self.dry_run && self.write_wav.is_some() {
            return Err(
                "--dry-run and --write-wav are mutually exclusive — pick one"
                    .to_string(),
            );
        }
        if !self.dry_run && self.write_wav.is_none() {
            // Full-TX mode: device + ptt-device required.
            if self.device.is_none() {
                return Err(
                    "missing --device <name> (required for full TX; use \
                     --dry-run or --write-wav for hardware-free runs)"
                        .to_string(),
                );
            }
            if self.ptt_device.is_none() {
                return Err(
                    "missing --ptt-device <path> (required for full TX; \
                     e.g. /dev/digirig)"
                        .to_string(),
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
    use std::sync::atomic::Ordering;
    use std::sync::Mutex;
    use tux_rig_rts::{MockTtyWriter, RtsPtt, TtyOp};

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
        // The PHY's own short_name is `floor-wblo`; the bd issue talks
        // about `wide-floor`. Accept both.
        assert_eq!(Mode::parse("floor-wblo").unwrap(), Mode::WideFloor);
    }

    #[test]
    fn mode_parse_rejects_unknown() {
        let err = Mode::parse("ofdm-mid").unwrap_err();
        assert!(matches!(err, TxError::UnknownMode { .. }));
    }

    #[test]
    fn mode_short_name_round_trips() {
        let m = Mode::WideFloor;
        assert_eq!(Mode::parse(m.short_name()).unwrap(), m);
    }

    // ─── resolve_payload ────────────────────────────────────────────

    #[test]
    fn resolve_payload_plain_text_is_utf8_bytes() {
        assert_eq!(resolve_payload("hi").unwrap(), b"hi".to_vec());
    }

    #[test]
    fn resolve_payload_at_prefix_reads_file() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("tuxmodem-tx-test-payload-{}", std::process::id()));
        std::fs::write(&path, b"hello").unwrap();
        let arg = format!("@{}", path.display());
        let got = resolve_payload(&arg).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(got, b"hello".to_vec());
    }

    #[test]
    fn resolve_payload_missing_file_errors() {
        let err = resolve_payload("@/nonexistent/path/that/should/not/exist").unwrap_err();
        assert!(matches!(err, TxError::PayloadFileRead { .. }));
    }

    #[test]
    fn resolve_payload_treats_empty_at_as_empty_path_error() {
        // "@" alone — empty path — should error on read, not silently
        // succeed.
        let err = resolve_payload("@").unwrap_err();
        assert!(matches!(err, TxError::PayloadFileRead { .. }));
    }

    // ─── encode_payload ─────────────────────────────────────────────

    #[test]
    fn encode_payload_wide_floor_returns_nonzero_buffer() {
        let buf = encode_payload(Mode::WideFloor, b"hi", FrameMode::Raw).unwrap();
        assert!(buf.samples().len() > 0, "encoder should emit some samples");
    }

    #[test]
    fn encode_payload_oversized_returns_phy_error() {
        // The wide-floor mode's single-symbol capacity is ~9 bytes
        // (per wideband_lowdensity.rs docstring). 64 bytes is well
        // over.
        let err = encode_payload(Mode::WideFloor, &[0u8; 64], FrameMode::Raw).unwrap_err();
        assert!(matches!(err, TxError::Phy(PhyError::PayloadTooLarge { .. })));
    }

    // ─── AirtimeBudget ──────────────────────────────────────────────

    #[test]
    fn airtime_total_sums_four_components() {
        let b = AirtimeBudget {
            lead_in: Duration::from_millis(100),
            buffer_duration: Duration::from_millis(200),
            tail_drain: Duration::from_millis(50),
            setup_slack: Duration::from_millis(150),
        };
        assert_eq!(b.total(), Duration::from_millis(500));
    }

    #[test]
    fn airtime_from_buffer_defaults_uses_pinned_constants() {
        let buf = AudioBuffer::from_samples(vec![0.0; 480]); // 10 ms at 48 kHz
        let b = AirtimeBudget::from_buffer_defaults(&buf);
        assert_eq!(b.lead_in, DEFAULT_LEAD_IN);
        assert_eq!(b.tail_drain, DEFAULT_TAIL_DRAIN);
        assert_eq!(b.setup_slack, DEFAULT_SETUP_SLACK);
        // 480 samples / 48000 = 10 ms
        assert!(
            (b.buffer_duration.as_millis() as i64 - 10).abs() <= 1,
            "buffer duration {:?} should be ~10 ms",
            b.buffer_duration,
        );
    }

    #[test]
    fn check_budget_within_max_returns_effective() {
        let b = AirtimeBudget {
            lead_in: Duration::from_millis(100),
            buffer_duration: Duration::from_millis(200),
            tail_drain: Duration::from_millis(50),
            setup_slack: Duration::from_millis(150),
        };
        let max = Duration::from_secs(10);
        let eff = check_budget(&b, max).unwrap();
        assert_eq!(eff, max);
    }

    #[test]
    fn check_budget_above_max_rejects() {
        let b = AirtimeBudget {
            lead_in: Duration::from_secs(10),
            buffer_duration: Duration::from_secs(10),
            tail_drain: Duration::from_secs(10),
            setup_slack: Duration::from_secs(10),
        };
        let max = Duration::from_secs(30);
        let err = check_budget(&b, max).unwrap_err();
        assert!(matches!(err, TxError::AirtimeExceeded { .. }));
    }

    #[test]
    fn check_budget_above_hard_cap_uses_lower_cap() {
        // Operator asks for max = 120 s; hard cap is 60 s. Effective
        // is 60 s. A budget of 65 s should reject against the hard cap,
        // not the operator's max.
        let b = AirtimeBudget {
            lead_in: Duration::ZERO,
            buffer_duration: Duration::from_secs(65),
            tail_drain: Duration::ZERO,
            setup_slack: Duration::ZERO,
        };
        let max = Duration::from_secs(120);
        let err = check_budget(&b, max).unwrap_err();
        let TxError::AirtimeExceeded { max: effective, .. } = err else {
            panic!("expected AirtimeExceeded");
        };
        assert_eq!(effective, HARD_CAP_AIRTIME);
    }

    #[test]
    fn check_budget_exact_max_passes() {
        let b = AirtimeBudget {
            lead_in: Duration::from_secs(1),
            buffer_duration: Duration::from_secs(1),
            tail_drain: Duration::ZERO,
            setup_slack: Duration::ZERO,
        };
        check_budget(&b, Duration::from_secs(2)).unwrap();
    }

    // ─── Args ───────────────────────────────────────────────────────

    #[test]
    fn args_parse_dry_run_minimal() {
        let a = Args::parse(&s(&["--dry-run", "--mode", "wide-floor", "--payload", "hi"])).unwrap();
        assert!(a.dry_run);
        assert_eq!(a.mode.as_deref(), Some("wide-floor"));
        assert_eq!(a.payload.as_deref(), Some("hi"));
        a.validate().unwrap();
    }

    #[test]
    fn args_parse_full_run_requires_device_and_ptt_device() {
        // Without --dry-run, both --device and --ptt-device are
        // required.
        let a = Args::parse(&s(&["--mode", "wide-floor", "--payload", "hi"])).unwrap();
        let err = a.validate().unwrap_err();
        assert!(err.contains("--device") || err.contains("--ptt-device"));
    }

    #[test]
    fn args_parse_full_run_with_all_required_validates() {
        let a = Args::parse(&s(&[
            "--mode", "wide-floor", "--payload", "hi", "--device", "USB Audio", "--ptt-device",
            "/dev/digirig",
        ]))
        .unwrap();
        a.validate().unwrap();
    }

    #[test]
    fn args_parse_help_flag() {
        let a = Args::parse(&s(&["--help"])).unwrap();
        assert!(a.help);
    }

    #[test]
    fn args_parse_max_airtime_accepted() {
        let a = Args::parse(&s(&[
            "--dry-run",
            "--mode",
            "wide-floor",
            "--payload",
            "hi",
            "--max-airtime",
            "45",
        ]))
        .unwrap();
        assert_eq!(a.max_airtime, Some(Duration::from_secs(45)));
    }

    #[test]
    fn args_parse_short_flags_d_and_p() {
        let a = Args::parse(&s(&[
            "--mode", "wide-floor", "--payload", "hi", "-d", "USB Audio", "-p", "/dev/digirig",
        ]))
        .unwrap();
        assert_eq!(a.device.as_deref(), Some("USB Audio"));
        assert_eq!(a.ptt_device.as_deref(), Some("/dev/digirig"));
    }

    #[test]
    fn args_parse_rejects_unknown_flag() {
        let err = Args::parse(&s(&["--gibberish"])).unwrap_err();
        assert!(err.contains("unknown argument"));
    }

    #[test]
    fn args_parse_rejects_flag_without_value() {
        let err = Args::parse(&s(&["--payload"])).unwrap_err();
        assert!(err.contains("--payload"));
    }

    #[test]
    fn args_parse_rejects_max_airtime_non_numeric() {
        let err = Args::parse(&s(&["--max-airtime", "ten"])).unwrap_err();
        assert!(err.contains("--max-airtime"));
    }

    #[test]
    fn args_validate_missing_mode_is_an_error_even_for_dry_run() {
        let a = Args::parse(&s(&["--dry-run", "--payload", "hi"])).unwrap();
        let err = a.validate().unwrap_err();
        assert!(err.contains("--mode"));
    }

    // ─── --write-wav follow-up (tuxlink-4dv9) ───────────────────────

    #[test]
    fn args_parse_write_wav_with_path() {
        let a = Args::parse(&s(&[
            "--write-wav", "/tmp/foo.wav", "--mode", "wide-floor", "--payload", "TEST",
        ]))
        .unwrap();
        assert_eq!(
            a.write_wav.as_deref(),
            Some(std::path::Path::new("/tmp/foo.wav"))
        );
        a.validate().unwrap();
    }

    #[test]
    fn args_parse_write_wav_without_value_errors() {
        let err = Args::parse(&s(&["--write-wav"])).unwrap_err();
        assert!(err.contains("--write-wav"));
    }

    #[test]
    fn args_parse_write_wav_doesnt_require_device_or_ptt() {
        // The whole point: --write-wav is a hardware-free mode like
        // --dry-run, so neither --device nor --ptt-device should be
        // required when --write-wav is set.
        let a = Args::parse(&s(&[
            "--write-wav", "/tmp/x.wav", "--mode", "wide-floor", "--payload", "hi",
        ]))
        .unwrap();
        a.validate().unwrap();
        assert!(a.device.is_none());
        assert!(a.ptt_device.is_none());
    }

    #[test]
    fn args_parse_dry_run_and_write_wav_are_mutually_exclusive() {
        let a = Args::parse(&s(&[
            "--dry-run", "--write-wav", "/tmp/x.wav", "--mode", "wide-floor",
            "--payload", "hi",
        ]))
        .unwrap();
        let err = a.validate().unwrap_err();
        assert!(err.contains("mutually exclusive"));
    }

    #[test]
    fn args_parse_full_tx_default_when_neither_dry_run_nor_write_wav() {
        // Sanity: without dry-run AND without write-wav, validate
        // still requires device + ptt-device (the full-TX path).
        let a = Args::parse(&s(&["--mode", "wide-floor", "--payload", "hi"])).unwrap();
        let err = a.validate().unwrap_err();
        assert!(err.contains("--device") || err.contains("--ptt-device"));
    }

    #[test]
    fn write_wav_roundtrip_via_audiobuffer_write_then_read_then_decode() {
        // The headline acceptance test for --write-wav: encode →
        // write_wav → re-read_wav → demod via the floor's receive →
        // assert payload recovered. Proves the CLI's --write-wav
        // workflow without spawning the binary.
        use tuxmodem_phy::audio_io::AudioBuffer;
        use tuxmodem_phy::robustness_floor::wideband_lowdensity::WidebandLowDensityFloor;

        let payload = b"WAVTEST";
        let mode = Mode::WideFloor;
        let buffer = encode_payload(mode, payload, FrameMode::Raw).unwrap();
        let dir = std::env::temp_dir();
        let path = dir.join(format!(
            "tuxmodem-tx-write-wav-test-{}.wav",
            std::process::id()
        ));
        buffer.write_wav(&path).unwrap();
        let read_back = AudioBuffer::read_wav(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let decoded = WidebandLowDensityFloor::new()
            .receive(read_back.samples())
            .unwrap();
        assert_eq!(decoded, payload);
    }

    // ─── run_transmission orchestration (mock PTT + mock player) ────

    /// Recording mock for [`AbortablePlay`]. Captures every call's
    /// buffer length + the value of the abort flag at entry, so tests
    /// can assert on call ordering and on whether the abort flag was
    /// observed.
    struct RecordingPlayer {
        calls: Mutex<Vec<RecordedPlay>>,
        outcome: PlayOutcome,
        /// If `Some(idx)`, raise the abort flag on the `idx`-th call
        /// just before returning. Simulates SIGINT delivery during
        /// playback.
        raise_abort_at_call: Option<usize>,
        /// Set to `Some(err)` to make play_blocking_with_abort return
        /// `Err`. Used to drive the "release-on-play-error" test path.
        return_err: Option<String>,
    }

    #[derive(Debug, Clone)]
    struct RecordedPlay {
        buffer_samples: usize,
        abort_was_set_at_entry: bool,
    }

    impl RecordingPlayer {
        fn new(outcome: PlayOutcome) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                outcome,
                raise_abort_at_call: None,
                return_err: None,
            }
        }
        fn calls(&self) -> Vec<RecordedPlay> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl AbortablePlay for RecordingPlayer {
        fn play_blocking_with_abort(
            &mut self,
            buffer: &AudioBuffer,
            abort: &AtomicBool,
        ) -> Result<PlayOutcome, PhyError> {
            let mut calls = self.calls.lock().unwrap();
            let call_idx = calls.len();
            calls.push(RecordedPlay {
                buffer_samples: buffer.samples().len(),
                abort_was_set_at_entry: abort.load(Ordering::Acquire),
            });
            drop(calls);
            if let Some(err) = &self.return_err {
                return Err(PhyError::AudioIo(err.clone()));
            }
            if let Some(target_idx) = self.raise_abort_at_call {
                if target_idx == call_idx {
                    abort.store(true, Ordering::Release);
                    return Ok(PlayOutcome::Aborted);
                }
            }
            Ok(self.outcome)
        }
    }

    #[test]
    fn run_transmission_happy_path_asserts_plays_releases_in_order() {
        let mut ptt = RtsPtt::new(MockTtyWriter::new()).unwrap();
        let mut player = RecordingPlayer::new(PlayOutcome::Completed);
        let buf = AudioBuffer::from_samples(vec![0.0; 480]); // 10 ms
        let abort = AtomicBool::new(false);

        let outcome = run_transmission(
            &mut ptt,
            &mut player,
            &buf,
            Duration::from_millis(20),
            &abort,
        )
        .unwrap();

        assert_eq!(outcome, TxOutcome::Completed);
        // The MockTtyWriter records exactly: open-clear, then assert, then release.
        assert_eq!(
            ptt.writer().ops,
            vec![TtyOp::OpenClearBoth, TtyOp::AssertRts, TtyOp::ReleaseRts],
        );
        // The player saw the buffer with the expected sample count.
        let calls = player.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].buffer_samples, 480);
        assert!(!calls[0].abort_was_set_at_entry);
        assert_eq!(ptt.state(), PttState::Released);
    }

    #[test]
    fn run_transmission_lead_in_elapses_before_play_call() {
        let mut ptt = RtsPtt::new(MockTtyWriter::new()).unwrap();
        let mut player = RecordingPlayer::new(PlayOutcome::Completed);
        let buf = AudioBuffer::from_samples(vec![0.0; 48]); // 1 ms
        let abort = AtomicBool::new(false);

        let started = Instant::now();
        run_transmission(
            &mut ptt,
            &mut player,
            &buf,
            Duration::from_millis(80),
            &abort,
        )
        .unwrap();
        let elapsed = started.elapsed();
        // The sleep is at least the lead-in (80 ms). Allow generous
        // slop for CI noise — we just want to make sure the lead-in
        // actually ran, not zero ms.
        assert!(
            elapsed >= Duration::from_millis(70),
            "elapsed {elapsed:?} should be >= lead-in (80 ms)"
        );
    }

    #[test]
    fn run_transmission_player_aborted_returns_aborted_early() {
        let mut ptt = RtsPtt::new(MockTtyWriter::new()).unwrap();
        let mut player = RecordingPlayer {
            calls: Mutex::new(Vec::new()),
            outcome: PlayOutcome::Completed,
            raise_abort_at_call: Some(0),
            return_err: None,
        };
        let buf = AudioBuffer::from_samples(vec![0.0; 480]);
        let abort = AtomicBool::new(false);

        let outcome = run_transmission(
            &mut ptt,
            &mut player,
            &buf,
            Duration::from_millis(10),
            &abort,
        )
        .unwrap();

        assert_eq!(outcome, TxOutcome::AbortedEarly);
        // Release still ran.
        assert_eq!(
            ptt.writer().ops,
            vec![TtyOp::OpenClearBoth, TtyOp::AssertRts, TtyOp::ReleaseRts],
        );
        assert_eq!(ptt.state(), PttState::Released);
    }

    #[test]
    fn run_transmission_abort_during_lead_in_skips_play_and_releases() {
        // Operator pressed Ctrl-C right after assert, before play
        // started. Lead-in loop should observe the flag and proceed
        // directly to release.
        let mut ptt = RtsPtt::new(MockTtyWriter::new()).unwrap();
        let mut player = RecordingPlayer::new(PlayOutcome::Completed);
        let buf = AudioBuffer::from_samples(vec![0.0; 480]);
        let abort = AtomicBool::new(true); // already set

        let outcome = run_transmission(
            &mut ptt,
            &mut player,
            &buf,
            Duration::from_millis(100),
            &abort,
        )
        .unwrap();

        assert_eq!(outcome, TxOutcome::AbortedEarly);
        // Player was NOT called.
        assert_eq!(player.calls().len(), 0);
        // Release still ran.
        assert_eq!(
            ptt.writer().ops,
            vec![TtyOp::OpenClearBoth, TtyOp::AssertRts, TtyOp::ReleaseRts],
        );
    }

    #[test]
    fn run_transmission_release_still_runs_when_play_errors() {
        // Audio device returned an error mid-play. We still release
        // PTT, then surface the error.
        let mut ptt = RtsPtt::new(MockTtyWriter::new()).unwrap();
        let mut player = RecordingPlayer {
            calls: Mutex::new(Vec::new()),
            outcome: PlayOutcome::Completed,
            raise_abort_at_call: None,
            return_err: Some("stream stopped".into()),
        };
        let buf = AudioBuffer::from_samples(vec![0.0; 480]);
        let abort = AtomicBool::new(false);

        let err = run_transmission(
            &mut ptt,
            &mut player,
            &buf,
            Duration::from_millis(10),
            &abort,
        )
        .unwrap_err();

        assert!(matches!(err, TxError::Phy(_)));
        // Release ran despite the error.
        assert_eq!(
            ptt.writer().ops,
            vec![TtyOp::OpenClearBoth, TtyOp::AssertRts, TtyOp::ReleaseRts],
        );
        assert_eq!(ptt.state(), PttState::Released);
    }

    #[test]
    fn run_transmission_completed_outcome_when_player_returns_completed() {
        let mut ptt = RtsPtt::new(MockTtyWriter::new()).unwrap();
        let mut player = RecordingPlayer::new(PlayOutcome::Completed);
        let buf = AudioBuffer::from_samples(vec![0.0; 480]);
        let abort = AtomicBool::new(false);
        let outcome = run_transmission(
            &mut ptt,
            &mut player,
            &buf,
            Duration::from_millis(10),
            &abort,
        )
        .unwrap();
        assert_eq!(outcome, TxOutcome::Completed);
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
        let err = FrameMode::parse("garbage").unwrap_err();
        assert!(matches!(err, TxError::UnknownFrameMode { .. }));
    }

    #[test]
    fn frame_mode_short_name_round_trips() {
        for m in [FrameMode::Raw, FrameMode::Sync] {
            assert_eq!(FrameMode::parse(m.short_name()).unwrap(), m);
        }
    }

    #[test]
    fn encode_payload_sync_is_longer_than_raw_by_preamble_len() {
        // The headline correctness invariant: sync output = raw output
        // + PREAMBLE_LEN_SAMPLES extra samples (the preamble) at the front.
        let raw = encode_payload(Mode::WideFloor, b"hi", FrameMode::Raw).unwrap();
        let sync = encode_payload(Mode::WideFloor, b"hi", FrameMode::Sync).unwrap();
        assert_eq!(
            sync.samples().len(),
            raw.samples().len() + PREAMBLE_LEN_SAMPLES,
            "sync should equal raw + {PREAMBLE_LEN_SAMPLES} preamble samples"
        );
    }

    #[test]
    fn encode_payload_sync_preserves_raw_tail() {
        // The OFDM symbol portion of the sync output should be
        // bit-identical to the corresponding raw output.
        let raw = encode_payload(Mode::WideFloor, b"hi", FrameMode::Raw).unwrap();
        let sync = encode_payload(Mode::WideFloor, b"hi", FrameMode::Sync).unwrap();
        let sync_tail = &sync.samples()[PREAMBLE_LEN_SAMPLES..];
        assert_eq!(
            sync_tail.len(),
            raw.samples().len(),
            "tail length should match"
        );
        for (i, (&s, &r)) in sync_tail.iter().zip(raw.samples().iter()).enumerate() {
            assert!(
                (s - r).abs() < 1e-6,
                "sync tail sample {i} differs from raw: sync={s}, raw={r}"
            );
        }
    }

    #[test]
    fn args_parse_frame_mode_sync() {
        let a = Args::parse(&s(&[
            "--frame-mode", "sync", "--mode", "wide-floor", "--dry-run", "--payload", "hi",
        ]))
        .unwrap();
        assert_eq!(a.frame_mode, FrameMode::Sync);
    }

    #[test]
    fn args_parse_frame_mode_default_is_raw_when_omitted() {
        let a = Args::parse(&s(&[
            "--mode", "wide-floor", "--dry-run", "--payload", "hi",
        ]))
        .unwrap();
        assert_eq!(a.frame_mode, FrameMode::Raw);
    }

    #[test]
    fn args_parse_frame_mode_unknown_value_errors() {
        let err = Args::parse(&s(&["--frame-mode", "twelve"])).unwrap_err();
        assert!(err.contains("frame mode"));
    }

    #[test]
    fn args_parse_frame_mode_without_value_errors() {
        let err = Args::parse(&s(&["--frame-mode"])).unwrap_err();
        assert!(err.contains("--frame-mode"));
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
    fn encode_payload_multi_sync_routes_to_transmit_multi_with_preamble() {
        // The bit-equivalence check: encode_payload(MultiSync, X) must
        // produce the exact samples that
        // WidebandLowDensityFloor::transmit_multi_with_preamble(X) does.
        use tuxmodem_phy::robustness_floor::wideband_lowdensity::WidebandLowDensityFloor;
        let payload = b"HELLO_MULTI_SYNC";
        let got = encode_payload(Mode::WideFloor, payload, FrameMode::MultiSync).unwrap();
        let want = WidebandLowDensityFloor::new()
            .transmit_multi_with_preamble(payload)
            .unwrap();
        assert_eq!(got.samples().len(), want.len());
        for (i, (&a, &b)) in got.samples().iter().zip(want.iter()).enumerate() {
            assert!((a - b).abs() < 1e-6, "sample {i} differs: {a} vs {b}");
        }
    }

    #[test]
    fn encode_payload_multi_sync_accepts_large_payload() {
        // 100-byte payload would fail in Sync (single-symbol cap ~9
        // bytes); MultiSync handles it via length-prefix framing.
        let payload: Vec<u8> = (0..100).map(|i| (i % 251) as u8).collect();
        let buf = encode_payload(Mode::WideFloor, &payload, FrameMode::MultiSync).unwrap();
        // Should be preamble (192) + 12 symbols × symbol_size.
        assert!(buf.samples().len() > 192 + 11 * 2560);
    }

    #[test]
    fn args_parse_frame_mode_multi_sync() {
        let a = Args::parse(&s(&[
            "--frame-mode", "multi-sync", "--mode", "wide-floor", "--dry-run",
            "--payload", "hi",
        ]))
        .unwrap();
        assert_eq!(a.frame_mode, FrameMode::MultiSync);
    }
}
