//! Testability seams (spec §Testability traits). All four production impls
//! are thin; everything above them is driven by fakes in unit tests.

use std::path::Path;

use tuxlink_capture::slot::GapReport;
use tuxlink_jt9::runner::Jt9Runner;
use tuxlink_jt9::types::SlotOutcome;

/// One capture read's result. **Time is data at this seam**: the monotonic
/// timestamp arrives as a value so the slot assembler stays pure and tests
/// drive synthetic clocks.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadBatch {
    /// Valid frames written into the caller's buffer (channel-0, 48 kHz).
    pub frames: usize,
    /// Monotonic timestamp (µs) at which this batch was read.
    pub mono_ts_us: u64,
    /// A gap the source detected BEFORE these frames (xrun recovery /
    /// suspend). Size is never trusted from ALSA — the assembler computes it
    /// from the monotonic expected-frame counter.
    pub gap: Option<GapReport>,
}

/// Capture-source failure classes (spec §ALSA read loop errno mapping).
/// Diagnostics are `String`s (clippy result_large_err discipline).
#[derive(Debug, Clone, PartialEq)]
pub enum SourceError {
    /// Device held by another process (EBUSY at open).
    Busy,
    /// Device gone (ENODEV/EBADFD-class, or open ENOENT).
    Absent,
    /// Parameter negotiation failed on the hw device (rate/format/channels);
    /// carries the ALSA diagnostic for `blocked(unsupported-sample-rate)`.
    UnsupportedFormat(String),
    /// -ESTRPIPE: stream suspended (system sleep). The source recovers the
    /// PCM internally; the capture loop abandons the slot (clock anomaly).
    Suspended,
    /// 10 consecutive wait-timeouts on a silent, non-erroring stream — the
    /// C-Media wedge class. Treated as device loss.
    Wedged,
    /// Any other errno, stringified.
    Io(String),
}

/// The one audio seam. Production: [`crate::ft8::alsa_source::AlsaSource`].
pub trait SampleSource: Send {
    /// Blocking-bounded read: waits at most ~200 ms before returning either
    /// frames, an empty batch, or an error. Never parks unboundedly — the
    /// capture loop checks its abort flag between calls.
    fn read(&mut self, buf: &mut [i16]) -> Result<ReadBatch, SourceError>;
}

/// The decode seam. Production wraps [`Jt9Runner`] 1:1.
pub trait DecodeEngine: Send + Sync {
    /// One-time FFTW wisdom warm (spec §WAV writeout: once per runner
    /// construction, during `starting`, BEFORE any PCM is held). Errors are
    /// stringified `SlotFailure`s; the start sequence matches the
    /// spawn/not-found class by substring (Task 11).
    fn prewarm(&self) -> Result<(), String>;
    fn decode_slot(&self, wav: &Path, slot_tmp: &Path, slot_utc_ms: u64) -> SlotOutcome;
}

/// Production [`DecodeEngine`]: delegates to the L1 runner.
pub struct Jt9Engine {
    runner: Jt9Runner,
}

impl Jt9Engine {
    pub fn new(runner: Jt9Runner) -> Self {
        Self { runner }
    }
}

impl DecodeEngine for Jt9Engine {
    fn prewarm(&self) -> Result<(), String> {
        self.runner.prewarm().map_err(|f| format!("{f:?}"))
    }
    fn decode_slot(&self, wav: &Path, slot_tmp: &Path, slot_utc_ms: u64) -> SlotOutcome {
        self.runner.decode_slot(wav, slot_tmp, slot_utc_ms)
    }
}

/// THE process-lifetime monotonic epoch (µs). ONE epoch for the whole
/// process, by contract: **assembler mono values MUST come from one epoch**
/// — the slot assembler DIFFERENCES monotonic stamps across producers
/// (`AlsaSource` read batches and `Ft8Platform::mono_now_us` during gap
/// handling), so a second epoch would read as a giant clock anomaly on the
/// first mixed push. Every production monotonic stamp in `src/ft8/` calls
/// this; no other `OnceLock<Instant>` epoch may exist in the module.
pub(crate) fn process_mono_us() -> u64 {
    static EPOCH: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let epoch = EPOCH.get_or_init(std::time::Instant::now);
    u64::try_from(epoch.elapsed().as_micros()).unwrap_or(u64::MAX)
}
