//! Real-time audio output via CPAL.
//!
//! Bridges the PHY's mono 48 kHz f32 [`crate::audio_io::AudioBuffer`]
//! into a live soundcard stream. Feature-gated behind `audio-device`
//! so the workspace's non-hardware crates and CI builds don't pull
//! in CPAL's ALSA/CoreAudio/WASAPI deps unless this surface is
//! actually wanted.
//!
//! ## Sample format
//!
//! The PHY emits **mono 48 kHz f32** per the spec pinned in
//! [`crate::audio_io::SAMPLE_RATE_HZ`]. CPAL device support varies
//! by host; we negotiate against the device's reported configs:
//!
//! - 48 kHz sample rate is required; mismatches surface as
//!   [`PhyError::AudioIo`] (no resampling — the PHY's mode tables
//!   are tied to this rate).
//! - Channel count is whatever the device offers, prefer mono when
//!   available. For stereo-only devices, each mono sample is
//!   duplicated to both channels in [`AudioOutput::play_blocking`].
//! - Sample format is `f32` (matching the PHY); 16-bit-PCM-only
//!   devices will error rather than auto-convert (we don't want to
//!   silently clip the PHY's full-scale output).
//!
//! ## Blocking semantics
//!
//! [`AudioOutput::play_blocking`] starts a fresh CPAL stream, pumps
//! the buffer into the device's callback, waits until the callback
//! has consumed all samples, then drains the device's internal
//! ring before returning. After return, the stream is closed and
//! the next `play_blocking` call builds a fresh one. This costs a
//! few hundred ms of setup per call but keeps each call
//! self-contained — no global stream state for the caller to
//! worry about.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::audio_io::{AudioBuffer, SAMPLE_RATE_HZ};
use crate::error::PhyError;

/// Outcome of an abortable playback call. `Completed` = the buffer
/// played in full and the device's tail-drain finished. `Aborted` =
/// the caller's abort flag was observed and the CPAL stream was
/// dropped before the buffer was fully consumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayOutcome {
    /// Buffer played in full; tail-drain elapsed.
    Completed,
    /// Caller's abort flag observed; stream dropped early. The remainder
    /// of the buffer did NOT reach the device.
    Aborted,
}

/// Outcome of an abortable capture call. `Completed` = the target
/// sample count was reached. `Aborted` = the caller's abort flag was
/// observed before the target count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordOutcome {
    /// Target sample count reached cleanly.
    Completed,
    /// Caller's abort flag observed; the returned buffer carries
    /// whatever samples had been captured up to that point (which may
    /// be the empty buffer if the abort fired before any samples
    /// arrived).
    Aborted,
}

/// Information about an available output device.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    /// CPAL device name — the value to pass to [`AudioOutput::open`]
    /// for an exact match.
    pub name: String,
    /// Number of channels the device's default config offers.
    pub default_channels: u16,
    /// Minimum sample rate the device supports across all configs.
    pub min_sample_rate_hz: u32,
    /// Maximum sample rate the device supports across all configs.
    pub max_sample_rate_hz: u32,
    /// True if the device supports the PHY's pinned 48 kHz rate in
    /// at least one f32 output config.
    pub supports_48k_f32: bool,
}

/// Enumerate output devices that CPAL's default host can see.
///
/// Returns even devices that won't work for the PHY (wrong sample
/// rate, wrong sample format, etc.) — the operator picks based on
/// the [`DeviceInfo::supports_48k_f32`] flag.
pub fn list_output_devices() -> Result<Vec<DeviceInfo>, PhyError> {
    let host = cpal::default_host();
    let devices = host.output_devices().map_err(audio_err("output_devices"))?;
    let mut out = Vec::new();
    for device in devices {
        let name = device
            .name()
            .map_err(audio_err("device name"))?;
        let default = device
            .default_output_config()
            .map_err(audio_err("default_output_config"))?;
        let mut min_rate = u32::MAX;
        let mut max_rate = 0u32;
        let mut supports = false;
        if let Ok(configs) = device.supported_output_configs() {
            for cfg in configs {
                let lo = cfg.min_sample_rate().0;
                let hi = cfg.max_sample_rate().0;
                min_rate = min_rate.min(lo);
                max_rate = max_rate.max(hi);
                if lo <= SAMPLE_RATE_HZ
                    && hi >= SAMPLE_RATE_HZ
                    && cfg.sample_format() == cpal::SampleFormat::F32
                {
                    supports = true;
                }
            }
        }
        if min_rate == u32::MAX {
            // No supported_output_configs() entries — fall back to the
            // default config's rate.
            min_rate = default.sample_rate().0;
            max_rate = default.sample_rate().0;
        }
        out.push(DeviceInfo {
            name,
            default_channels: default.channels(),
            min_sample_rate_hz: min_rate,
            max_sample_rate_hz: max_rate,
            supports_48k_f32: supports,
        });
    }
    Ok(out)
}

/// Live output to a soundcard. Constructed via [`Self::open`]; each
/// [`Self::play_blocking`] call builds and tears down a CPAL stream
/// of its own (see module docs for rationale).
pub struct AudioOutput {
    device: cpal::Device,
    config: cpal::SupportedStreamConfig,
}

impl AudioOutput {
    /// Open the named device, or the host's default output when
    /// `device_name` is `None`. Errors if the named device isn't
    /// found OR if it can't be configured for 48 kHz f32.
    pub fn open(device_name: Option<&str>) -> Result<Self, PhyError> {
        let host = cpal::default_host();
        let device = match device_name {
            None => host
                .default_output_device()
                .ok_or_else(|| PhyError::AudioIo("no default output device".into()))?,
            Some(name) => {
                let devices = host.output_devices().map_err(audio_err("output_devices"))?;
                let mut found: Option<cpal::Device> = None;
                for d in devices {
                    let dn = d.name().map_err(audio_err("device name"))?;
                    if dn == name {
                        found = Some(d);
                        break;
                    }
                }
                found.ok_or_else(|| {
                    PhyError::AudioIo(format!("output device not found: {name}"))
                })?
            }
        };

        // Find a config that includes 48 kHz f32. Prefer mono when
        // available — fewer samples to push per audio frame.
        let configs = device
            .supported_output_configs()
            .map_err(audio_err("supported_output_configs"))?;
        let target_rate = cpal::SampleRate(SAMPLE_RATE_HZ);
        let mut mono: Option<cpal::SupportedStreamConfigRange> = None;
        let mut other: Option<cpal::SupportedStreamConfigRange> = None;
        for cfg in configs {
            if cfg.sample_format() != cpal::SampleFormat::F32 {
                continue;
            }
            if cfg.min_sample_rate() > target_rate || cfg.max_sample_rate() < target_rate {
                continue;
            }
            if cfg.channels() == 1 {
                mono = Some(cfg);
                break;
            }
            if other.is_none() {
                other = Some(cfg);
            }
        }
        let chosen = mono
            .or(other)
            .ok_or_else(|| {
                PhyError::AudioIo(format!(
                    "device does not support {SAMPLE_RATE_HZ} Hz f32 output"
                ))
            })?
            .with_sample_rate(target_rate);

        Ok(Self {
            device,
            config: chosen,
        })
    }

    /// The CPAL device name this output is bound to.
    pub fn device_name(&self) -> Result<String, PhyError> {
        self.device.name().map_err(audio_err("device name"))
    }

    /// Channel count this output is configured for (1 = mono,
    /// 2 = stereo; higher counts get the mono sample replicated to
    /// every channel).
    pub fn channels(&self) -> u16 {
        self.config.channels()
    }

    /// Play the buffer to the device, blocking until the device's
    /// ring drains. After return, the stream is closed.
    ///
    /// Mono buffers are expanded to the device's channel count by
    /// duplicating each sample to every channel. The PHY's pinned
    /// 48 kHz f32 mono format is the only supported input.
    ///
    /// Equivalent to [`Self::play_blocking_with_abort`] with a never-set
    /// abort flag — provided for callers that don't need abort semantics.
    pub fn play_blocking(&mut self, buffer: &AudioBuffer) -> Result<(), PhyError> {
        let abort = AtomicBool::new(false);
        match self.play_blocking_with_abort(buffer, &abort)? {
            PlayOutcome::Completed => Ok(()),
            PlayOutcome::Aborted => {
                // Unreachable in practice — the flag is never set —
                // but the type system doesn't know that.
                Err(PhyError::AudioIo("playback aborted without request".into()))
            }
        }
    }

    /// Play the buffer to the device with caller-driven abort.
    ///
    /// Polls `abort` every ~20 ms. When the flag is observed `true`,
    /// drops the CPAL stream immediately (silencing the audio thread)
    /// and returns [`PlayOutcome::Aborted`]. The remainder of the buffer
    /// does NOT reach the device.
    ///
    /// Used by `sonde-tx` to wire SIGINT/SIGTERM → release-PTT-fast
    /// without waiting for the buffer to finish playing.
    pub fn play_blocking_with_abort(
        &mut self,
        buffer: &AudioBuffer,
        abort: &AtomicBool,
    ) -> Result<PlayOutcome, PhyError> {
        let channels = usize::from(self.config.channels());
        // Interleave mono → device-channels. For stereo, each sample
        // duplicates to L + R; for 5.1, it duplicates to all 6.
        let mut frames: Vec<f32> = Vec::with_capacity(buffer.samples().len() * channels);
        for s in buffer.samples() {
            for _ in 0..channels {
                frames.push(*s);
            }
        }

        let total = frames.len();
        let (done_tx, done_rx) = mpsc::channel::<()>();
        let (err_tx, err_rx) = mpsc::channel::<String>();
        // CPAL needs to own the frames + the cursor. Move both into
        // the callback closure; clone the channels needed for signalling.
        let mut cursor = 0usize;
        let done_tx_cb = done_tx.clone();
        let stream = self
            .device
            .build_output_stream(
                &self.config.config(),
                move |out: &mut [f32], _info| {
                    let remaining = total.saturating_sub(cursor);
                    let to_copy = out.len().min(remaining);
                    if to_copy > 0 {
                        out[..to_copy]
                            .copy_from_slice(&frames[cursor..cursor + to_copy]);
                        cursor += to_copy;
                    }
                    // Zero-fill any remainder of this callback's
                    // frame so the device doesn't replay stale data
                    // after we're done.
                    for s in out[to_copy..].iter_mut() {
                        *s = 0.0;
                    }
                    // Once we've reached the end and JUST consumed
                    // the last samples, signal done. send() may fail
                    // if the receiver dropped early — that's fine.
                    if cursor >= total && to_copy > 0 {
                        let _ = done_tx_cb.send(());
                    }
                },
                move |err| {
                    let _ = err_tx.send(err.to_string());
                },
                None,
            )
            .map_err(audio_err("build_output_stream"))?;
        // Drop the original done_tx so an early callback failure can
        // still close the channel and unblock the recv below.
        drop(done_tx);

        stream.play().map_err(audio_err("stream.play"))?;

        // Poll the abort flag on a tight cadence while waiting for the
        // callback's "done" signal. Abort latency is bounded by the
        // poll interval — keep it tight enough to feel responsive on a
        // ctrl-C without burning CPU.
        let total_budget = Duration::from_secs_f32(buffer.duration_seconds() + 2.0);
        let deadline = Instant::now() + total_budget;
        let poll = Duration::from_millis(20);
        loop {
            if abort.load(Ordering::Acquire) {
                // Drop the stream NOW. This stops the CPAL audio
                // thread on its next callback — typically within a
                // few ms — so the radio sees the audio fall silent
                // shortly before the caller releases PTT.
                drop(stream);
                return Ok(PlayOutcome::Aborted);
            }
            match done_rx.recv_timeout(poll) {
                Ok(()) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if Instant::now() >= deadline {
                        return Err(PhyError::AudioIo(format!(
                            "playback timeout after {:.2}s",
                            total_budget.as_secs_f32()
                        )));
                    }
                    // Otherwise: keep polling.
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if let Ok(msg) = err_rx.try_recv() {
                        return Err(PhyError::AudioIo(format!("stream error: {msg}")));
                    }
                    return Err(PhyError::AudioIo("playback ended unexpectedly".into()));
                }
            }
        }

        // Brief tail-drain for the device's internal ring buffer to
        // play out before we drop the stream. Without this the last
        // ~10-50 ms of audio gets truncated on some ALSA configs.
        std::thread::sleep(Duration::from_millis(100));
        // Stream drops here, closing the CPAL handle.
        drop(stream);
        Ok(PlayOutcome::Completed)
    }
}

/// Information about an available input device.
#[derive(Debug, Clone)]
pub struct InputDeviceInfo {
    /// CPAL device name — the value to pass to [`AudioInput::open`]
    /// for an exact match.
    pub name: String,
    /// Number of channels the device's default config offers.
    pub default_channels: u16,
    /// Minimum sample rate the device supports across all configs.
    pub min_sample_rate_hz: u32,
    /// Maximum sample rate the device supports across all configs.
    pub max_sample_rate_hz: u32,
    /// True if the device supports the PHY's pinned 48 kHz rate in
    /// at least one f32 input config.
    pub supports_48k_f32: bool,
}

/// Enumerate input devices that CPAL's default host can see.
///
/// Mirrors [`list_output_devices`] for the capture side. Returns even
/// devices that won't work for the PHY (wrong rate, wrong format) —
/// the operator picks based on [`InputDeviceInfo::supports_48k_f32`].
pub fn list_input_devices() -> Result<Vec<InputDeviceInfo>, PhyError> {
    let host = cpal::default_host();
    let devices = host.input_devices().map_err(audio_err("input_devices"))?;
    let mut out = Vec::new();
    for device in devices {
        let name = device.name().map_err(audio_err("device name"))?;
        let default = device
            .default_input_config()
            .map_err(audio_err("default_input_config"))?;
        let mut min_rate = u32::MAX;
        let mut max_rate = 0u32;
        let mut supports = false;
        if let Ok(configs) = device.supported_input_configs() {
            for cfg in configs {
                let lo = cfg.min_sample_rate().0;
                let hi = cfg.max_sample_rate().0;
                min_rate = min_rate.min(lo);
                max_rate = max_rate.max(hi);
                if lo <= SAMPLE_RATE_HZ
                    && hi >= SAMPLE_RATE_HZ
                    && cfg.sample_format() == cpal::SampleFormat::F32
                {
                    supports = true;
                }
            }
        }
        if min_rate == u32::MAX {
            min_rate = default.sample_rate().0;
            max_rate = default.sample_rate().0;
        }
        out.push(InputDeviceInfo {
            name,
            default_channels: default.channels(),
            min_sample_rate_hz: min_rate,
            max_sample_rate_hz: max_rate,
            supports_48k_f32: supports,
        });
    }
    Ok(out)
}

/// Live capture from a soundcard. Constructed via [`Self::open`]; each
/// [`Self::record_blocking_with_abort`] call builds and tears down a
/// CPAL stream of its own (matching [`AudioOutput`]'s pattern — see
/// the module-level "Blocking semantics" doc).
pub struct AudioInput {
    device: cpal::Device,
    config: cpal::SupportedStreamConfig,
}

impl AudioInput {
    /// Open the named device, or the host's default input when
    /// `device_name` is `None`. Errors if the named device isn't
    /// found OR if it can't be configured for 48 kHz f32.
    pub fn open(device_name: Option<&str>) -> Result<Self, PhyError> {
        let host = cpal::default_host();
        let device = match device_name {
            None => host
                .default_input_device()
                .ok_or_else(|| PhyError::AudioIo("no default input device".into()))?,
            Some(name) => {
                let devices = host.input_devices().map_err(audio_err("input_devices"))?;
                let mut found: Option<cpal::Device> = None;
                for d in devices {
                    let dn = d.name().map_err(audio_err("device name"))?;
                    if dn == name {
                        found = Some(d);
                        break;
                    }
                }
                found.ok_or_else(|| {
                    PhyError::AudioIo(format!("input device not found: {name}"))
                })?
            }
        };

        // Find a config that includes 48 kHz f32. Prefer mono when
        // available; otherwise we'll de-interleave channel 0 in the
        // capture callback.
        let configs = device
            .supported_input_configs()
            .map_err(audio_err("supported_input_configs"))?;
        let target_rate = cpal::SampleRate(SAMPLE_RATE_HZ);
        let mut mono: Option<cpal::SupportedStreamConfigRange> = None;
        let mut other: Option<cpal::SupportedStreamConfigRange> = None;
        for cfg in configs {
            if cfg.sample_format() != cpal::SampleFormat::F32 {
                continue;
            }
            if cfg.min_sample_rate() > target_rate || cfg.max_sample_rate() < target_rate {
                continue;
            }
            if cfg.channels() == 1 {
                mono = Some(cfg);
                break;
            }
            if other.is_none() {
                other = Some(cfg);
            }
        }
        let chosen = mono
            .or(other)
            .ok_or_else(|| {
                PhyError::AudioIo(format!(
                    "device does not support {SAMPLE_RATE_HZ} Hz f32 input"
                ))
            })?
            .with_sample_rate(target_rate);

        Ok(Self {
            device,
            config: chosen,
        })
    }

    /// The CPAL device name this input is bound to.
    pub fn device_name(&self) -> Result<String, PhyError> {
        self.device.name().map_err(audio_err("device name"))
    }

    /// Channel count this input is configured for (1 = mono; > 1
    /// gets de-interleaved to channel 0 in the capture callback).
    pub fn channels(&self) -> u16 {
        self.config.channels()
    }

    /// Capture `target_samples` mono samples (at 48 kHz), polling the
    /// caller's `abort` flag every ~20 ms. Returns the captured buffer
    /// + the [`RecordOutcome`].
    ///
    /// For multi-channel input devices, only channel 0 is kept —
    /// keeps the API mono-only on the PHY side, matching what
    /// `WidebandLowDensityFloor::receive` expects.
    pub fn record_blocking_with_abort(
        &mut self,
        target_samples: usize,
        abort: &AtomicBool,
    ) -> Result<(RecordOutcome, AudioBuffer), PhyError> {
        let channels = usize::from(self.config.channels());
        if channels == 0 {
            return Err(PhyError::AudioIo(
                "input device reports 0 channels".into(),
            ));
        }
        // Shared accumulator the capture callback writes into.
        // Mutex (not std::sync::RwLock or atomic) because CPAL's
        // callback runs on a different thread and we need exclusive
        // append access.
        let acc: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::with_capacity(target_samples)));
        let acc_cb = Arc::clone(&acc);
        let (err_tx, err_rx) = mpsc::channel::<String>();

        let stream = self
            .device
            .build_input_stream(
                &self.config.config(),
                move |samples: &[f32], _info| {
                    // De-interleave: take channel 0 of each frame.
                    let mut guard = acc_cb.lock().unwrap();
                    if guard.len() >= target_samples {
                        return;
                    }
                    for frame in samples.chunks_exact(channels) {
                        if guard.len() >= target_samples {
                            break;
                        }
                        guard.push(frame[0]);
                    }
                },
                move |err| {
                    let _ = err_tx.send(err.to_string());
                },
                None,
            )
            .map_err(audio_err("build_input_stream"))?;

        stream.play().map_err(audio_err("stream.play"))?;

        // Generous wall-clock budget: target duration + 2 s slack.
        let target_duration =
            Duration::from_secs_f32(target_samples as f32 / SAMPLE_RATE_HZ as f32);
        let total_budget = target_duration + Duration::from_secs(2);
        let deadline = Instant::now() + total_budget;
        let poll = Duration::from_millis(20);
        let outcome = loop {
            if abort.load(Ordering::Acquire) {
                break RecordOutcome::Aborted;
            }
            if let Ok(msg) = err_rx.try_recv() {
                drop(stream);
                return Err(PhyError::AudioIo(format!("stream error: {msg}")));
            }
            {
                let guard = acc.lock().unwrap();
                if guard.len() >= target_samples {
                    break RecordOutcome::Completed;
                }
            }
            if Instant::now() >= deadline {
                drop(stream);
                return Err(PhyError::AudioIo(format!(
                    "capture timeout after {:.2}s (got {}/{} samples)",
                    total_budget.as_secs_f32(),
                    acc.lock().unwrap().len(),
                    target_samples,
                )));
            }
            std::thread::sleep(poll);
        };

        drop(stream);
        let mut samples = Arc::try_unwrap(acc)
            .map_err(|_| PhyError::AudioIo("capture acc still shared after stream drop".into()))?
            .into_inner()
            .map_err(|e| PhyError::AudioIo(format!("capture acc poisoned: {e}")))?;
        // For a Completed outcome the buffer should be exactly target
        // long; trim any over-shoot the callback might have written in
        // a final chunk. Aborted outcomes return whatever was captured.
        if matches!(outcome, RecordOutcome::Completed) {
            samples.truncate(target_samples);
        }
        Ok((outcome, AudioBuffer::from_samples(samples)))
    }
}

/// Wrap a CPAL error type into [`PhyError::AudioIo`] with a context tag.
fn audio_err<E: std::fmt::Display>(context: &'static str) -> impl FnOnce(E) -> PhyError {
    move |e| PhyError::AudioIo(format!("{context}: {e}"))
}

// ─── tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Most behavior tests live in the binary's MockOutput unit tests
    // (where we can stub the device side without CPAL). What we CAN
    // unit-test here is the channel-expansion arithmetic — the same
    // expansion the production play_blocking does, factored for
    // independent verification.
    fn expand_mono_to_channels(samples: &[f32], channels: usize) -> Vec<f32> {
        let mut out = Vec::with_capacity(samples.len() * channels);
        for s in samples {
            for _ in 0..channels {
                out.push(*s);
            }
        }
        out
    }

    #[test]
    fn channel_expansion_mono_to_mono_is_identity() {
        let mono = vec![0.1, 0.2, 0.3];
        assert_eq!(expand_mono_to_channels(&mono, 1), vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn channel_expansion_mono_to_stereo_duplicates() {
        let mono = vec![0.5, -0.5];
        assert_eq!(
            expand_mono_to_channels(&mono, 2),
            vec![0.5, 0.5, -0.5, -0.5]
        );
    }

    #[test]
    fn channel_expansion_mono_to_quad_duplicates_to_all_four() {
        let mono = vec![1.0];
        assert_eq!(expand_mono_to_channels(&mono, 4), vec![1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn channel_expansion_empty_input_yields_empty_output() {
        let mono: Vec<f32> = vec![];
        assert_eq!(expand_mono_to_channels(&mono, 2), Vec::<f32>::new());
    }

    #[test]
    fn audio_err_includes_context_tag() {
        // Sanity check on the error wrapper — failures from cpal
        // should arrive with a context tag so the operator can tell
        // which call site fired.
        let wrapped = audio_err("build_output_stream")("oh no");
        match wrapped {
            PhyError::AudioIo(msg) => {
                assert!(msg.contains("build_output_stream"));
                assert!(msg.contains("oh no"));
            }
            other => panic!("expected AudioIo, got {other:?}"),
        }
    }
}
