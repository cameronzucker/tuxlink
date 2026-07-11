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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wwv_mode_has_biasing_prompt() {
        assert!(DecodeMode::WwvBiased.initial_prompt().unwrap().contains("Solar flux"));
        assert_eq!(DecodeMode::General.initial_prompt(), None);
    }
}
