use std::path::Path;
use std::time::Duration;

use chrono::{Datelike, TimeZone, Timelike, Utc};
use serde::Serialize;

use crate::propagation::solar::{self, SolarIndices};
use crate::propagation::solar_update::{self, SolarSnapshot, UpdateOutcome};
use crate::ui_commands::UiError;
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
}

/// Capture the next WWV bulletin off-air and ingest it into the propagation
/// forecast. Blocking work runs on `spawn_blocking`. RX-only; never transmits.
#[tauri::command]
pub async fn wwv_offair_refresh(now_ms: u64) -> Result<WwvRefreshOutcome, UiError> {
    let cfg = crate::config::read_config().map_err(|e| UiError::Internal { detail: e.to_string() })?;
    let rig_cfg = crate::modem_commands::rig_config_from(&cfg.rig).ok_or_else(|| {
        UiError::NotConfigured("Configure CAT rig control, or tune WWV manually.".into())
    })?;
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
        let wav = crate::wwv_offair::capture_cycle(rig_cfg, close_serial, freq_hz, Duration::from_secs(70), &cap)
            .map_err(|e| e.to_string())?;
        let stt = WhisperStt::load(&model).map_err(|e| e.to_string())?;
        match decode_and_ingest(&stt, &wav, year, month, now_ms, &config_dir).map_err(|e| e.to_string())? {
            DecodeOutcome::Ingested(o) => Ok(WwvRefreshOutcome {
                updated: o.forecast_updated,
                indices: o.indices,
                source: o.source,
                no_copy: false,
            }),
            DecodeOutcome::NoCopy => Ok(WwvRefreshOutcome {
                updated: false,
                indices: None,
                source: "rf-wwv-voice".into(),
                no_copy: true,
            }),
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
}
