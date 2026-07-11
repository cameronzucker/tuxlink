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

/// Omits `-D <device>` entirely when `device` is empty (blank/whitespace) —
/// `WwvOffairConfig.capture_device` defaults to `""`, and `arecord -D ""`
/// fails ALSA lookup ("Unknown PCM"). arecord falls back to its default
/// device when `-D` is absent.
pub(crate) fn arecord_args(device: &str, secs: u64, out: &Path) -> Vec<String> {
    let mut args = Vec::new();
    if !device.trim().is_empty() {
        args.push("-D".into());
        args.push(device.into());
    }
    args.extend([
        "-f".into(),
        "S16_LE".into(),
        "-c".into(),
        "1".into(),
        "-r".into(),
        "16000".into(),
        "-d".into(),
        secs.to_string(),
        out.to_string_lossy().into_owned(),
    ]);
    args
}

pub struct ArecordCapture {
    pub device: String,
    pub out_dir: PathBuf,
}

impl CaptureSource for ArecordCapture {
    fn capture(&self, _freq_hz: u64, dwell: Duration) -> Result<PathBuf, CaptureError> {
        // Unique per capture (pid + nanosecond timestamp) so overlapping
        // captures (retry after a failed cycle, or a manual + scheduled
        // refresh racing) never collide on a single fixed filename.
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let out = self.out_dir.join(format!(
            "wwv-{}-{}-{}.wav",
            std::process::id(),
            stamp,
            dwell.as_secs()
        ));
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

    #[test]
    fn arecord_argv_omits_dash_d_for_empty_device() {
        let args = arecord_args("", 70, std::path::Path::new("/tmp/x.wav"));
        assert_eq!(
            args,
            vec!["-f", "S16_LE", "-c", "1", "-r", "16000", "-d", "70", "/tmp/x.wav"]
        );
        // Whitespace-only device is treated as empty too.
        let args_ws = arecord_args("   ", 70, std::path::Path::new("/tmp/x.wav"));
        assert_eq!(args_ws, args);
    }
}
