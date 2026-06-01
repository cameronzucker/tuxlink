//! Audio sample plumbing. Real-time device I/O is feature-gated
//! (`audio-device`); buffer-level I/O is always available and is what
//! all tests use.
//!
//! Sample-rate decision (PHY spec §3.Q6): pinned at **48 kHz f32 mono**.
//! Rationale: matches CM108B-class USB audio device default; gives
//! ample oversampling vs. the 2300 Hz audio bandwidth target; FFT
//! sizing for the OFDM sub-carrier grid remains a per-mode parameter
//! independent of the audio sample rate.

use crate::error::PhyError;
use std::path::Path;

/// Pinned audio sample rate. Per spec §3.Q6 settlement.
pub const SAMPLE_RATE_HZ: u32 = 48_000;

/// Single-channel f32 audio buffer.
#[derive(Debug, Clone)]
pub struct AudioBuffer {
    samples: Vec<f32>,
}

impl AudioBuffer {
    /// Construct from a `Vec<f32>` of samples at [`SAMPLE_RATE_HZ`].
    pub fn from_samples(samples: Vec<f32>) -> Self {
        Self { samples }
    }
    /// Borrowed view into the underlying f32 sample slice.
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }
    /// Consume the buffer and return the owned sample vector.
    pub fn into_samples(self) -> Vec<f32> {
        self.samples
    }
    /// Wall-clock duration of the buffer in seconds, at [`SAMPLE_RATE_HZ`].
    pub fn duration_seconds(&self) -> f32 {
        self.samples.len() as f32 / SAMPLE_RATE_HZ as f32
    }

    /// Write the buffer to a WAV file at `path`. Single-channel, f32, 48 kHz.
    pub fn write_wav(&self, path: &Path) -> Result<(), PhyError> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: SAMPLE_RATE_HZ,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(path, spec)
            .map_err(|e| PhyError::AudioIo(format!("wav create: {e}")))?;
        for s in &self.samples {
            w.write_sample(*s)
                .map_err(|e| PhyError::AudioIo(format!("wav write: {e}")))?;
        }
        w.finalize()
            .map_err(|e| PhyError::AudioIo(format!("wav finalize: {e}")))
    }

    /// Read a WAV file from `path`. Errors if the sample rate doesn't
    /// match [`SAMPLE_RATE_HZ`] or the file is not f32 PCM.
    pub fn read_wav(path: &Path) -> Result<Self, PhyError> {
        let mut r = hound::WavReader::open(path)
            .map_err(|e| PhyError::AudioIo(format!("wav open: {e}")))?;
        let spec = r.spec();
        if spec.sample_rate != SAMPLE_RATE_HZ {
            return Err(PhyError::AudioIo(format!(
                "wav sample_rate {} != expected {}",
                spec.sample_rate, SAMPLE_RATE_HZ
            )));
        }
        let samples: Result<Vec<f32>, _> = r.samples::<f32>().collect();
        let samples = samples.map_err(|e| PhyError::AudioIo(format!("wav read: {e}")))?;
        Ok(Self { samples })
    }
}
