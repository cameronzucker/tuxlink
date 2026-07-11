//! Testability seams (spec §Testability traits). All four production impls
//! are thin; everything above them is driven by fakes in unit tests.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::ft8::records::AudioDeviceChoice;
use crate::modem_status::{ModemSession, ModemState};
use crate::winlink::ax25::devices::{
    alsa_hw_name, enumerate_capture_devices, read_sys_snapshot, resolve_managed_device,
    ResolvedManagedDevice, StableAudioId,
};
use tuxlink_capture::slot::GapReport;
use tuxlink_jt9::discover::Jt9Binary;
use tuxlink_jt9::runner::Jt9Runner;
use tuxlink_jt9::types::{SlotOutcome, SLOT_DECODE_TIMEOUT_SECS};

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

/// The impure-probe seam: every filesystem/process/CAT touchpoint the
/// service needs, bundled so tests fake ONE object. Errors are `String`s
/// (result_large_err discipline). Production is [`ProdPlatform`]; the test
/// double is `testutil::FakePlatform`.
pub trait Ft8Platform: Send + Sync {
    fn discover_jt9(&self) -> Result<Jt9Binary, String>;
    /// Re-resolve the persisted identity against a FRESH snapshot — the card
    /// index can change on re-enumeration; never reuse a cached name
    /// (spec §Device loss).
    fn resolve_device(&self, id: &StableAudioId) -> Option<ResolvedManagedDevice>;
    fn enumerate_capture(&self) -> Vec<AudioDeviceChoice>;
    fn probe_busy(&self, plughw: &str, card_index: u32) -> Result<(), String>;
    fn open_source(&self, alsa_hw: &str) -> Result<Box<dyn SampleSource>, SourceError>;
    /// ADR-0015 release confirm against `/dev/snd/pcmC<card>D0c`.
    fn confirm_released(&self, card_index: u32) -> bool;
    fn write_slot_wav(&self, path: &Path, samples: &[i16]) -> std::io::Result<()>;
    fn make_engine(&self, bin: &Jt9Binary, wisdom_dir: &Path) -> Arc<dyn DecodeEngine>;
    fn rig_configured(&self) -> bool;
    /// One spawn-read-drop `ManagedRig` session (serial NEVER held while
    /// capturing). Caller serializes via the service's rig lock.
    fn rig_read_dial(&self) -> Result<u64, String>;
    /// One spawn-tune-drop session. Same serialization contract.
    fn rig_tune(&self, dial_hz: u64) -> Result<(), String>;
    /// Positive resume eligibility over ModemState: `Stopped | Error |
    /// SocketLost` (spec §Resume — `Idle` means ardopcf holds the card).
    fn modem_resume_eligible(&self) -> bool;
    fn wisdom_dir(&self) -> PathBuf;
    fn slot_dir_root(&self) -> PathBuf;
    fn utc_now_ms(&self) -> u64;
    fn mono_now_us(&self) -> u64;
    /// Pipe-type entries in /proc/self/fd (readlink → "pipe:[...]"), or None
    /// when /proc is unreadable. tuxlink-b026z.8 watermark.
    fn count_pipe_fds(&self) -> Option<usize>;
}

/// Production platform. Paths are injected at construction (lib.rs setup
/// resolves them from Tauri's path API) so this struct stays Tauri-free and
/// the setup wiring stays trivial.
pub struct ProdPlatform {
    pub wisdom_dir: PathBuf,
    pub slot_root: PathBuf,
    pub modem: Arc<ModemSession>,
}

// Monotonic stamps: `process_mono_us` (defined above in this file, T10) is
// THE process epoch — ProdPlatform and AlsaSource share it, because the
// assembler DIFFERENCES monotonic values across both producers.

impl Ft8Platform for ProdPlatform {
    fn discover_jt9(&self) -> Result<Jt9Binary, String> {
        tuxlink_jt9::discover::discover_jt9(None).map_err(|e| format!("{e:?}"))
    }
    fn resolve_device(&self, id: &StableAudioId) -> Option<ResolvedManagedDevice> {
        resolve_managed_device(id, &read_sys_snapshot())
    }
    fn enumerate_capture(&self) -> Vec<AudioDeviceChoice> {
        enumerate_capture_devices(&read_sys_snapshot())
            .into_iter()
            .map(|d| AudioDeviceChoice {
                human_name: d.human_name,
                alsa_hw: alsa_hw_name(d.card_index),
                stable_id: d.stable_id,
            })
            .collect()
    }
    fn probe_busy(&self, plughw: &str, card_index: u32) -> Result<(), String> {
        crate::winlink::ax25::direwolf_probe::probe_device_busy(plughw, card_index)
    }
    fn open_source(&self, alsa_hw: &str) -> Result<Box<dyn SampleSource>, SourceError> {
        crate::ft8::alsa_source::AlsaSource::open(alsa_hw).map(|s| Box::new(s) as Box<dyn SampleSource>)
    }
    fn confirm_released(&self, card_index: u32) -> bool {
        crate::winlink::modem::process::ManagedModem::confirm_audio_device_released(
            std::path::Path::new(&format!("/dev/snd/pcmC{card_index}D0c")),
            Duration::from_secs(2),
        )
    }
    fn write_slot_wav(&self, path: &std::path::Path, samples: &[i16]) -> std::io::Result<()> {
        tuxlink_capture::wavwrite::write_slot_wav(path, samples)
    }
    fn make_engine(&self, bin: &Jt9Binary, wisdom_dir: &std::path::Path) -> Arc<dyn DecodeEngine> {
        Arc::new(Jt9Engine::new(Jt9Runner::new(
            bin.clone(),
            wisdom_dir.to_path_buf(),
            Duration::from_secs(SLOT_DECODE_TIMEOUT_SECS),
        )))
    }
    fn rig_configured(&self) -> bool {
        crate::config::read_config().map(|c| c.rig.is_configured()).unwrap_or(false)
    }
    fn rig_read_dial(&self) -> Result<u64, String> {
        let cfg = crate::config::read_config().map_err(|e| e.to_string())?;
        let rc = crate::modem_commands::rig_config_from(&cfg.rig)
            .ok_or_else(|| "rig not configured".to_string())?;
        let mut rig = tux_rig::ManagedRig::spawn(rc).map_err(|e| e.to_string())?;
        let status = rig.status().map_err(|e| e.to_string())?;
        Ok(status.freq_hz)
        // rig drops here → rigctld killed → serial released.
    }
    fn rig_tune(&self, dial_hz: u64) -> Result<(), String> {
        let cfg = crate::config::read_config().map_err(|e| e.to_string())?;
        let rc = crate::modem_commands::rig_config_from(&cfg.rig)
            .ok_or_else(|| "rig not configured".to_string())?;
        let mode = crate::modem_commands::rig_data_mode(&cfg.rig);
        let mut rig = tux_rig::ManagedRig::spawn(rc).map_err(|e| e.to_string())?;
        rig.tune(dial_hz, mode).map_err(|e| e.to_string())
        // rig drops here → serial released.
    }
    fn modem_resume_eligible(&self) -> bool {
        matches!(
            self.modem.status_snapshot().state,
            ModemState::Stopped | ModemState::Error | ModemState::SocketLost
        )
    }
    fn wisdom_dir(&self) -> PathBuf {
        self.wisdom_dir.clone()
    }
    fn slot_dir_root(&self) -> PathBuf {
        self.slot_root.clone()
    }
    fn utc_now_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0)
    }
    fn mono_now_us(&self) -> u64 {
        process_mono_us()
    }
    fn count_pipe_fds(&self) -> Option<usize> {
        let entries = std::fs::read_dir("/proc/self/fd").ok()?;
        Some(
            entries
                .flatten()
                .filter(|e| {
                    std::fs::read_link(e.path())
                        .map(|t| t.to_string_lossy().starts_with("pipe:["))
                        .unwrap_or(false)
                })
                .count(),
        )
    }
}
