use std::path::Path;
use std::time::Duration;

use chrono::{Datelike, TimeZone, Timelike, Utc};
use serde::Serialize;

use crate::propagation::solar::{self, SolarIndices};
use crate::propagation::solar_update::{self, SolarSnapshot, UpdateOutcome};
use crate::ui_commands::UiError;
use crate::wwv_offair::capture::CaptureSource;
use crate::wwv_offair::WwvError;
use tuxlink_stt::{is_confident, DecodeMode, SttConfidence, WhisperStt};

/// (year, month, utc_hour) from a unix-ms instant. Mirrors propagation's
/// `utc_year_month`; falls back to the epoch on an out-of-range value.
pub(crate) fn utc_year_month_hour(now_ms: u64) -> (i32, u8, u8) {
    let dt = Utc
        .timestamp_millis_opt(now_ms as i64)
        .single()
        .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().expect("epoch valid"));
    (dt.year(), dt.month() as u8, dt.hour() as u8)
}

pub(crate) enum DecodeOutcome {
    Ingested(UpdateOutcome),
    NoCopy,
}

/// PURE post-transcription logic (no Whisper): normalize → parse_wwv → ingest.
/// Rejects low-confidence or unparseable transcripts as NoCopy (never writes a
/// hallucinated value).
pub(crate) fn ingest_transcript(
    text: &str,
    confidence: &SttConfidence,
    year: i32,
    month: u8,
    now_ms: u64,
    config_dir: &Path,
) -> Result<DecodeOutcome, WwvError> {
    if !is_confident(confidence) {
        return Ok(DecodeOutcome::NoCopy);
    }
    let normalized = crate::wwv_offair::normalize::normalize_spoken_numbers(text);
    let indices = match solar::parse_wwv(&normalized) {
        Some(i) => i,
        None => return Ok(DecodeOutcome::NoCopy),
    };
    let out = solar_update::apply_rf_solar_indices(indices, "rf-wwv-voice", year, month, now_ms, config_dir)
        .map_err(|e| WwvError::Capture(e.to_string()))?;
    Ok(DecodeOutcome::Ingested(out))
}

/// Transcribe the captured WAV, then ingest. Not unit-tested (needs a model);
/// the logic under it is `ingest_transcript` (tested).
pub(crate) fn decode_and_ingest(
    stt: &WhisperStt,
    wav: &Path,
    year: i32,
    month: u8,
    now_ms: u64,
    config_dir: &Path,
) -> Result<DecodeOutcome, WwvError> {
    let r = stt
        .transcribe(wav, DecodeMode::WwvBiased)
        .map_err(|e| WwvError::Capture(e.to_string()))?;
    ingest_transcript(&r.text, &r.confidence, year, month, now_ms, config_dir)
}

#[derive(Debug, Serialize)]
pub struct WwvRefreshOutcome {
    pub updated: bool,
    pub indices: Option<SolarIndices>,
    pub source: String,
    pub no_copy: bool,
    /// The captured clip's path, set only on `no_copy` (no confident
    /// transcript) so the frontend can offer playback + manual entry. The
    /// WAV is deleted on a successful ingest, so this is `None` there.
    pub wav_path: Option<String>,
}

/// Capture the next WWV bulletin off-air and ingest it into the propagation
/// forecast. Blocking work runs on `spawn_blocking`. RX-only; never transmits.
///
/// CAT rig control is optional: when configured, the rig is tuned to WWV and
/// restored afterward; when absent, the operator is expected to have tuned
/// WWV manually (see `wwv_offair_cat_configured`) and capture proceeds
/// directly against the configured audio device.
#[tauri::command]
pub async fn wwv_offair_refresh(now_ms: u64) -> Result<WwvRefreshOutcome, UiError> {
    let cfg = crate::config::read_config().map_err(|e| UiError::Internal { detail: e.to_string() })?;
    let rig_cfg_opt = crate::modem_commands::rig_config_from(&cfg.rig);
    let close_serial = cfg.rig.close_serial_sequencing;
    let device = cfg
        .wwv_offair
        .as_ref()
        .map(|w| w.capture_device.clone())
        .unwrap_or_default();
    let config_dir = crate::config::config_path()
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| UiError::Internal { detail: "no config dir".into() })?;
    let model = crate::wwv_offair::model::resolve_model_path(&cfg)
        .map_err(|reason| UiError::Unavailable { reason })?;
    let (year, month, hour) = utc_year_month_hour(now_ms);
    let freq_hz = crate::wwv_offair::freq::freq_for_utc_hour(hour);

    let outcome = tokio::task::spawn_blocking(move || -> Result<WwvRefreshOutcome, String> {
        // Capture FIRST, then load the Whisper model: model load can take
        // long enough to eat the pre-roll pad and start `arecord` after the
        // bulletin has already begun.
        let cap = crate::wwv_offair::capture::ArecordCapture { device, out_dir: std::env::temp_dir() };
        let wav = match rig_cfg_opt {
            Some(rig_cfg) => crate::wwv_offair::capture_cycle(rig_cfg, close_serial, freq_hz, Duration::from_secs(70), &cap)
                .map_err(|e| e.to_string())?,
            None => cap.capture(freq_hz, Duration::from_secs(70)).map_err(|e| e.to_string())?, // manual-tune fallback
        };
        let stt = WhisperStt::load(&model).map_err(|e| e.to_string())?;
        match decode_and_ingest(&stt, &wav, year, month, now_ms, &config_dir).map_err(|e| e.to_string())? {
            DecodeOutcome::Ingested(o) => {
                // Confident ingest: the clip has served its purpose, delete it.
                let _ = std::fs::remove_file(&wav);
                Ok(WwvRefreshOutcome {
                    updated: o.forecast_updated,
                    indices: o.indices,
                    source: o.source,
                    no_copy: false,
                    wav_path: None,
                })
            }
            DecodeOutcome::NoCopy => {
                // No confident transcript: keep the clip so the operator can
                // play it back and/or manually enter the indices.
                Ok(WwvRefreshOutcome {
                    updated: false,
                    indices: None,
                    source: "rf-wwv-voice".into(),
                    no_copy: true,
                    wav_path: Some(wav.to_string_lossy().into_owned()),
                })
            }
        }
    })
    .await
    .map_err(|e| UiError::Internal { detail: e.to_string() })?
    .map_err(|detail| UiError::Internal { detail })?;
    Ok(outcome)
}

/// Read the persisted off-air solar snapshot (for the conditions readout).
#[tauri::command]
pub async fn wwv_offair_snapshot_read() -> Result<Option<SolarSnapshot>, UiError> {
    let config_dir = crate::config::config_path()
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| UiError::Internal { detail: "no config dir".into() })?;
    Ok(SolarSnapshot::load(&config_dir))
}

/// A path is a valid kept capture clip iff it lives directly under the temp
/// dir and matches the `wwv-*.wav` naming `ArecordCapture` produces. Guards
/// `wwv_offair_read_clip` against reading arbitrary files off the operator's
/// disk via a spoofed path from the frontend.
pub(crate) fn is_valid_clip_path(p: &Path) -> bool {
    let fname = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
    let in_temp = p.starts_with(std::env::temp_dir());
    in_temp && fname.starts_with("wwv-") && fname.ends_with(".wav")
}

/// Read a kept no-copy capture WAV for playback. Path-validated to the temp
/// capture files only (no arbitrary file read). Returns raw WAV bytes; the
/// frontend wraps them in a Blob.
#[tauri::command]
pub async fn wwv_offair_read_clip(path: String) -> Result<Vec<u8>, UiError> {
    let p = std::path::PathBuf::from(&path);
    if !is_valid_clip_path(&p) {
        return Err(UiError::Rejected("not a wwv capture clip".into()));
    }
    std::fs::read(&p).map_err(|e| UiError::Internal { detail: e.to_string() })
}

/// PURE bounds-check + ingest for operator-entered SFI/A/K. Extracted from
/// `wwv_offair_manual_ingest` so it's unit-testable without a tokio runtime.
pub(crate) fn manual_ingest_indices(
    sfi: f64,
    a_index: Option<f64>,
    k_index: Option<f64>,
    year: i32,
    month: u8,
    now_ms: u64,
    config_dir: &Path,
) -> Result<UpdateOutcome, String> {
    if !(50.0..=500.0).contains(&sfi) {
        return Err(format!("solar flux {sfi} outside the plausible 50–500 range"));
    }
    let indices = SolarIndices { sfi, a_index, k_index };
    solar_update::apply_rf_solar_indices(indices, "rf-wwv-manual", year, month, now_ms, config_dir)
        .map_err(|e| e.to_string())
}

/// Operator-entered SFI/A/K (they heard the WWV bulletin but STT couldn't copy).
/// Same ingestion + provenance as an off-air decode, tagged "rf-wwv-manual".
#[tauri::command]
pub async fn wwv_offair_manual_ingest(
    sfi: f64,
    a_index: Option<f64>,
    k_index: Option<f64>,
    now_ms: u64,
) -> Result<WwvRefreshOutcome, UiError> {
    let config_dir = crate::config::config_path()
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| UiError::Internal { detail: "no config dir".into() })?;
    let (year, month, _h) = utc_year_month_hour(now_ms);
    let out = manual_ingest_indices(sfi, a_index, k_index, year, month, now_ms, &config_dir)
        .map_err(UiError::Rejected)?;
    Ok(WwvRefreshOutcome {
        updated: out.forecast_updated,
        indices: out.indices,
        source: out.source,
        no_copy: false,
        wav_path: None,
    })
}

/// Whether CAT rig control is configured. When false, the frontend prompts the
/// operator to tune WWV manually before the capture window.
#[tauri::command]
pub async fn wwv_offair_cat_configured() -> Result<bool, UiError> {
    let cfg = crate::config::read_config().map_err(|e| UiError::Internal { detail: e.to_string() })?;
    Ok(crate::modem_commands::rig_config_from(&cfg.rig).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_decomposition() {
        // 1_783_512_000_000 ms = 2026-07-08T12:00:00Z.
        assert_eq!(utc_year_month_hour(1_783_512_000_000), (2026, 7, 12));
    }

    const WWV: &str = "Solar flux 150 and estimated planetary A-index 8. \
        The estimated planetary K-index at 1200 UTC on 11 July was 2.";

    #[test]
    fn confident_parseable_transcript_ingests() {
        let dir = tempfile::tempdir().unwrap();
        let c = SttConfidence { avg_logprob: -0.2, no_speech_prob: 0.0 };
        let out = ingest_transcript(WWV, &c, 2026, 7, 1_000, dir.path()).unwrap();
        assert!(matches!(out, DecodeOutcome::Ingested(_)));
        // snapshot persisted with the voice provenance
        let snap = SolarSnapshot::load(dir.path()).unwrap();
        assert_eq!(snap.source, "rf-wwv-voice");
    }

    #[test]
    fn low_confidence_is_nocopy() {
        let dir = tempfile::tempdir().unwrap();
        let c = SttConfidence { avg_logprob: -1.5, no_speech_prob: 0.0 };
        assert!(matches!(ingest_transcript(WWV, &c, 2026, 7, 1_000, dir.path()).unwrap(), DecodeOutcome::NoCopy));
    }

    #[test]
    fn unparseable_transcript_is_nocopy() {
        let dir = tempfile::tempdir().unwrap();
        let c = SttConfidence { avg_logprob: -0.2, no_speech_prob: 0.0 };
        assert!(matches!(ingest_transcript("hello there no numbers", &c, 2026, 7, 1_000, dir.path()).unwrap(), DecodeOutcome::NoCopy));
    }

    #[test]
    fn manual_ingest_valid_sfi_ingests_with_manual_provenance() {
        let dir = tempfile::tempdir().unwrap();
        let out = manual_ingest_indices(150.0, Some(8.0), Some(2.0), 2026, 7, 1_000, dir.path()).unwrap();
        assert!(out.forecast_updated);
        assert_eq!(out.source, "rf-wwv-manual");
        // snapshot persisted with the manual provenance
        let snap = SolarSnapshot::load(dir.path()).unwrap();
        assert_eq!(snap.source, "rf-wwv-manual");
    }

    #[test]
    fn manual_ingest_rejects_implausible_sfi() {
        let dir = tempfile::tempdir().unwrap();
        assert!(manual_ingest_indices(10.0, None, None, 2026, 7, 1_000, dir.path()).is_err());
        assert!(manual_ingest_indices(600.0, None, None, 2026, 7, 1_000, dir.path()).is_err());
        // no snapshot written on rejection
        assert!(SolarSnapshot::load(dir.path()).is_none());
    }

    #[test]
    fn clip_path_validation() {
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("wwv-123-456789-70.wav");
        std::fs::write(&good, b"RIFF....WAVEfmt ").unwrap();
        assert!(is_valid_clip_path(&good), "temp wwv-*.wav path should validate");

        assert!(!is_valid_clip_path(Path::new("/etc/passwd")), "outside temp dir should reject");

        let non_wwv = dir.path().join("notes.txt");
        std::fs::write(&non_wwv, b"hi").unwrap();
        assert!(!is_valid_clip_path(&non_wwv), "non wwv-*.wav filename should reject");
    }
}
