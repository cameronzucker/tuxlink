use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeMode { General, WwvBiased }

impl DecodeMode {
    /// The `set_initial_prompt` text used to bias decoding.
    pub fn initial_prompt(&self) -> Option<&'static str> {
        match self {
            DecodeMode::General => None,
            DecodeMode::WwvBiased => Some(
                "NOAA space weather bulletin. Solar flux, estimated planetary \
                 A-index, planetary K-index at UTC, geomagnetic storms, minor, \
                 moderate, strong.",
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SttConfidence { pub avg_logprob: f32, pub no_speech_prob: f32 }

#[derive(Debug, Clone, PartialEq)]
pub struct SttResult { pub text: String, pub confidence: SttConfidence }

#[derive(Debug, thiserror::Error)]
pub enum SttError {
    #[error("model load failed: {0}")] ModelLoad(String),
    #[error("audio read failed: {0}")] Audio(String),
    #[error("transcription failed: {0}")] Transcribe(String),
}

/// Loads a WAV file, downmixing to mono and rescaling samples to `f32` in
/// `[-1.0, 1.0]`. Errors if the file's sample rate is not exactly 16 kHz —
/// this crate does not resample.
pub fn load_wav_16k_mono_f32(path: &Path) -> Result<Vec<f32>, SttError> {
    let reader = hound::WavReader::open(path).map_err(|e| SttError::Audio(e.to_string()))?;
    let spec = reader.spec();
    if spec.sample_rate != 16000 {
        return Err(SttError::Audio(format!("expected 16kHz, got {}", spec.sample_rate)));
    }
    let ch = spec.channels.max(1) as usize;
    let ints: Vec<i16> = reader.into_samples::<i16>()
        .collect::<Result<_, _>>().map_err(|e| SttError::Audio(e.to_string()))?;
    // Downmix to mono by averaging channels; scale i16 -> f32 [-1,1].
    let mut out = Vec::with_capacity(ints.len() / ch);
    for frame in ints.chunks(ch) {
        let sum: i32 = frame.iter().map(|&s| s as i32).sum();
        out.push((sum as f32 / ch as f32) / 32768.0);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wwv_mode_has_biasing_prompt() {
        assert!(DecodeMode::WwvBiased.initial_prompt().unwrap().contains("Solar flux"));
        assert_eq!(DecodeMode::General.initial_prompt(), None);
    }

    #[test]
    fn loads_16k_mono_wav() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.wav");
        let spec = hound::WavSpec { channels: 1, sample_rate: 16000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        for _ in 0..1600 { w.write_sample(0i16).unwrap(); }
        w.finalize().unwrap();
        let samples = load_wav_16k_mono_f32(&path).unwrap();
        assert_eq!(samples.len(), 1600);
    }

    #[test]
    fn rejects_wrong_sample_rate() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t8k.wav");
        let spec = hound::WavSpec { channels: 1, sample_rate: 8000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        w.write_sample(0i16).unwrap();
        w.finalize().unwrap();
        assert!(load_wav_16k_mono_f32(&path).is_err());
    }
}
