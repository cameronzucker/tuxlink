//! Offline HF-propagation prediction (voacapl sidecar). Pure offline compute:
//! no network, no transmit, no writes outside a per-call scratch dir.
//! Plan: docs/superpowers/plans/2026-06-10-u1-voacapl-prediction.md

pub mod antenna;
pub mod commands;
pub mod deck;
pub mod engine;
pub mod parse;
pub mod patterns;
pub mod prefs;
pub mod solar;
pub mod ssn;
pub mod type14;

use serde::{Deserialize, Serialize};

/// Inputs for one operator→station HF circuit prediction.
///
/// No Serialize/Deserialize: inputs are constructed Rust-side from the command
/// args, never round-tripped through JSON.
#[derive(Debug, Clone, PartialEq)]
pub struct PredictionInputs {
    /// Operator Maidenhead grid (reference point; from the status bar).
    pub tx_grid: String,
    /// Station Maidenhead grid (from `Gateway.grid`).
    pub rx_grid: String,
    /// Frequencies in kHz (from `Gateway.frequencies_khz`); converted to MHz for VOACAP.
    /// F1: these EXACT values are carried through to results by index — never
    /// re-derived from VOACAP's lossy display.
    pub frequencies_khz: Vec<f64>,
    /// UTC year (F8: used for the SSN lookup and the MONTH card; do NOT hardcode).
    pub year: i32,
    /// UTC month 1-12.
    pub month: u8,
    /// Smoothed sunspot number (from the SSN cache).
    pub ssn: f64,
    /// TX power in watts (v1 default 100 W; operator-configurable).
    pub tx_power_w: f64,
    /// F7: required SNR (dB) for the SYSTEM card. v1 default 73.0 (VOACAP standard,
    /// matching the captured fixture); the data-mode-calibrated value (VARA/ARDOP)
    /// is a documented empirical tunable, NOT a fabricated number. This is what REL
    /// is computed against.
    pub req_snr_db: f64,
    /// TX antenna pattern file under `itshfbc/antennas/default/`, e.g.
    /// `"const17.voa"` or `"ccir.000"`. Maps from the operator's selected antenna
    /// preset. The bracketed `[default/<file>]` field on the ANTENNA card is a
    /// fixed 21-char Fortran width; `build_deck` pads it.
    pub tx_antenna_voa: String,
    /// RX (far/gateway-end) antenna pattern file under
    /// `itshfbc/antennas/default/`. Maps from the gateway's parsed "Antenna being
    /// used" code (B/D/V), with an isotropic (`ccir.000`) fallback when the gateway
    /// reports none. NEVER force `swwhip.voa` for an unknown gateway — the whip's
    /// zenith null is what made short NVIS paths predict ~0% reliability.
    pub rx_antenna_voa: String,
    /// Generated VOACAP pattern content for the TX antenna (operator preset +
    /// height + ground, built by `antenna::operator_voa_content`). When `Some`,
    /// the engine writes it to the scratch `antennas/default/<tx_antenna_voa>`
    /// before the run, so a height-aware IONCAP pattern (type 22/23/24) is used
    /// instead of a stock file. When `None`, `tx_antenna_voa` names a stock file
    /// already present there (e.g. `ccir.000` for the `Unknown` preset).
    pub tx_antenna_voa_content: Option<String>,
    /// Man-made noise level for the SYSTEM card, as the positive dBW@3MHz
    /// magnitude (voacapl renders it `-<value> dBW`). From the operator's noise
    /// environment; default 145 (residential).
    pub noise_dbw: f64,
}

/// Per-frequency reliability over the 24 UTC hours.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelReliability {
    /// F1: the EXACT input dial in kHz (e.g. 7103.0), carried through by column
    /// index — the value U3 maps back to the operator's channel.
    pub frequency_khz: f64,
    /// The rounded MHz VOACAP actually computed this column at (informational;
    /// 7103 kHz and 7108 kHz both compute at ~7.10/7.11 MHz). Lets the UI show
    /// "computed at 7.10 MHz" without losing the real dial.
    pub voacap_mhz: f64,
    /// 24 reliability values (0.0-1.0), index = UTC hour 0..23. REL is vs `req_snr_db`.
    pub rel_by_hour: Vec<f64>,
    /// 24 SNR values (dB), index = UTC hour 0..23 (F7: lets U3 rank by SNR margin).
    pub snr_by_hour: Vec<f64>,
    /// 24 MUFday values (0.0-1.0), index = UTC hour 0..23 (F7: MUF-gating context).
    pub mufday_by_hour: Vec<f64>,
}

/// Full prediction result for one path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathPrediction {
    /// Great-circle bearing TX→RX in degrees (from VOACAP's AZIMUTHS line; for antenna aiming).
    pub bearing_deg: f64,
    /// Path distance in km (from VOACAP).
    pub distance_km: f64,
    /// F12: SSN provenance so U3 can render "solar data N old".
    pub ssn: f64,
    pub year: i32,
    pub month: u8,
    pub channels: Vec<ChannelReliability>,
}

#[derive(Debug, thiserror::Error)]
pub enum PropagationError {
    #[error("invalid grid {0:?}")]
    InvalidGrid(String),
    #[error("no usable HF frequencies in input")]
    NoFrequencies,
    #[error("too many HF frequencies: {0} (max 11)")]
    TooManyFrequencies(usize),
    #[error("voacapl binary not found: {0}")]
    BinaryNotFound(String),
    #[error("voacapl run failed: {0}")]
    RunFailed(String),
    #[error("could not parse voacapx.out: {0}")]
    ParseFailed(String),
    #[error("ssn cache error: {0}")]
    Ssn(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_prediction_serializes_camel_case() {
        let p = PathPrediction {
            bearing_deg: 301.65,
            distance_km: 215.2,
            ssn: 100.0,
            year: 2026,
            month: 6,
            channels: vec![ChannelReliability {
                frequency_khz: 7103.0,
                voacap_mhz: 7.10,
                rel_by_hour: vec![0.21; 24],
                snr_by_hour: vec![65.0; 24],
                mufday_by_hour: vec![0.69; 24],
            }],
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"bearingDeg\":301.65"));
        assert!(json.contains("\"distanceKm\":215.2"));
        assert!(json.contains("\"relByHour\""));
        assert!(json.contains("\"mufdayByHour\""));
        assert!(json.contains("\"voacapMhz\":7.1"));
        // F1: the exact dial survives, not a rounded 7100.
        assert!(json.contains("\"frequencyKhz\":7103.0"));
        assert!(json.contains("\"ssn\":100.0"));
    }
}
