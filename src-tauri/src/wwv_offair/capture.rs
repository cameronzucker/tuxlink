//! Audio capture seam for the off-air WWV decode pipeline.
//!
//! `CaptureSource` abstracts "get me N seconds of 16kHz mono S16_LE audio
//! from somewhere." `ArecordCapture` is the primary implementation: it
//! shells out to `arecord` against the primary-radio audio device. A future
//! `SdrSource` (SDR front-end, tuning its own receiver via `freq_hz`) can
//! implement the same trait without touching callers.

use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("arecord failed: {0}")]
    Arecord(String),
    #[error("capture device busy: {0}")]
    DeviceBusy(String),
    #[error("io: {0}")]
    Io(String),
}

pub trait CaptureSource {
    fn capture(&self, freq_hz: u64, dwell: Duration) -> Result<PathBuf, CaptureError>;
}

pub(crate) fn arecord_args(device: &str, secs: u64, out: &Path) -> Vec<String> {
    vec![
        "-D".into(),
        device.into(),
        "-f".into(),
        "S16_LE".into(),
        "-c".into(),
        "1".into(),
        "-r".into(),
        "16000".into(),
        "-d".into(),
        secs.to_string(),
        out.to_string_lossy().into_owned(),
    ]
}

pub struct ArecordCapture {
    pub device: String,
    pub out_dir: PathBuf,
}

impl CaptureSource for ArecordCapture {
    fn capture(&self, _freq_hz: u64, dwell: Duration) -> Result<PathBuf, CaptureError> {
        let out = self.out_dir.join(format!("wwv-{}.wav", dwell.as_secs()));
        let status = std::process::Command::new("arecord")
            .args(arecord_args(&self.device, dwell.as_secs().max(1), &out))
            .status()
            .map_err(|e| CaptureError::Io(e.to_string()))?;
        if !status.success() {
            return Err(CaptureError::Arecord(format!("exit {status}")));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arecord_argv_is_16k_mono_s16() {
        let args = arecord_args("plughw:1,0", 70, std::path::Path::new("/tmp/x.wav"));
        assert_eq!(
            args,
            vec![
                "-D",
                "plughw:1,0",
                "-f",
                "S16_LE",
                "-c",
                "1",
                "-r",
                "16000",
                "-d",
                "70",
                "/tmp/x.wav"
            ]
        );
    }
}
