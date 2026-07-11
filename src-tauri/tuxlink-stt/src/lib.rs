use std::path::Path;

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

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

/// A loaded `whisper.cpp` model, ready to transcribe 16 kHz mono `f32` PCM.
pub struct WhisperStt {
    ctx: WhisperContext,
}

impl WhisperStt {
    /// Loads a GGML/GGUF whisper model from disk.
    pub fn load(model_path: &Path) -> Result<Self, SttError> {
        let ctx = WhisperContext::new_with_params(
            model_path
                .to_str()
                .ok_or_else(|| SttError::ModelLoad("non-utf8 path".into()))?,
            WhisperContextParameters::default(),
        )
        .map_err(|e| SttError::ModelLoad(e.to_string()))?;
        Ok(Self { ctx })
    }

    /// Transcribes a 16 kHz mono WAV file, optionally biasing decoding via
    /// `mode`'s initial prompt (see [`DecodeMode::initial_prompt`]).
    pub fn transcribe(&self, wav: &Path, mode: DecodeMode) -> Result<SttResult, SttError> {
        let audio = load_wav_16k_mono_f32(wav)?;
        // Guard: a near-silent capture (dead device / squelched radio) must
        // not be fed to Whisper, where the WwvBiased prompt can hallucinate a
        // plausible-looking bulletin. Short-circuit to an unconfident result
        // so `is_confident` rejects it downstream.
        if is_silent(&audio) {
            return Ok(SttResult {
                text: String::new(),
                confidence: SttConfidence { avg_logprob: -10.0, no_speech_prob: 1.0 },
            });
        }
        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| SttError::Transcribe(e.to_string()))?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("en"));
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        if let Some(p) = mode.initial_prompt() {
            params.set_initial_prompt(p);
        }
        state
            .full(params, &audio)
            .map_err(|e| SttError::Transcribe(e.to_string()))?;

        let n = state
            .full_n_segments()
            .map_err(|e| SttError::Transcribe(e.to_string()))?;
        let mut text = String::new();
        // Aggregate a worst-case (lowest mean-token-logprob) across segments.
        // NOTE: whisper-rs 0.14.4 exposes NO per-segment no-speech probability
        // getter (verified against the crate source), so confidence rests on
        // token log-probs alone; no_speech_prob is reported as 0.0 (neutral).
        let mut min_logprob = 0.0f32;
        for i in 0..n {
            text.push_str(
                &state
                    .full_get_segment_text(i)
                    .map_err(|e| SttError::Transcribe(e.to_string()))?,
            );
            text.push(' ');

            let toks = state.full_n_tokens(i).unwrap_or(0);
            if toks > 0 {
                let mut sum = 0.0f32;
                let mut cnt = 0.0f32;
                for j in 0..toks {
                    if let Ok(p) = state.full_get_token_prob(i, j) {
                        // token prob in (0,1]; guard ln(0).
                        sum += p.max(1e-9).ln();
                        cnt += 1.0;
                    }
                }
                if cnt > 0.0 {
                    min_logprob = min_logprob.min(sum / cnt);
                }
            }
        }

        Ok(SttResult {
            text: text.trim().to_string(),
            confidence: SttConfidence {
                avg_logprob: min_logprob,
                no_speech_prob: 0.0,
            },
        })
    }
}

/// Ported from Geographica's tuned Whisper thresholds: reject hallucinated
/// transcripts from noise instead of emitting confident nonsense.
pub fn is_confident(c: &SttConfidence) -> bool {
    c.no_speech_prob < 0.8 && c.avg_logprob > -0.8
}

/// RMS floor below which `samples` counts as digital silence (dead capture
/// device, squelched/unkeyed radio). Well below any real received-audio
/// level, so it only trips on effectively-zero input.
const SILENCE_FLOOR: f32 = 1e-4;

/// Pure RMS silence check, extracted from `transcribe` so it can be
/// unit-tested without a loaded Whisper model.
pub(crate) fn is_silent(samples: &[f32]) -> bool {
    let rms = if samples.is_empty() {
        0.0
    } else {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    };
    rms < SILENCE_FLOOR
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

    #[test]
    #[ignore = "requires ggml base.en model + fixture WAV; run where present"]
    fn transcribes_fixture() {
        let model = std::path::PathBuf::from(std::env::var("TUXLINK_STT_MODEL").unwrap());
        let stt = WhisperStt::load(&model).unwrap();
        let wav = std::path::PathBuf::from("tests/fixtures/wwv_clean_16k.wav");
        let r = stt.transcribe(&wav, DecodeMode::WwvBiased).unwrap();
        assert!(r.text.to_lowercase().contains("solar flux"));
    }

    #[test]
    fn rejects_low_confidence() {
        assert!(!is_confident(&SttConfidence { avg_logprob: -1.2, no_speech_prob: 0.2 }));
        assert!(!is_confident(&SttConfidence { avg_logprob: -0.3, no_speech_prob: 0.9 }));
        assert!(is_confident(&SttConfidence { avg_logprob: -0.3, no_speech_prob: 0.2 }));
    }

    #[test]
    fn is_silent_detects_digital_silence_not_real_audio() {
        assert!(is_silent(&[0.0; 100]));
        assert!(is_silent(&[]));
        assert!(!is_silent(&[0.5, -0.5, 0.5]));
    }
}
