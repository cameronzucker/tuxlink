//! VOACAP input-deck builder.
//!
//! Produces the ASCII deck that `voacapl` consumes for one circuit prediction.
//! The deck format is determined by `voacap.for` (Fortran) — field widths are
//! Fortran edit descriptors (F5.2, F9.2, etc.) and must match exactly.
//!
//! Plan: docs/superpowers/plans/2026-06-10-u1-voacapl-prediction.md

use super::{PredictionInputs, PropagationError};
use crate::position::grid_to_lat_lon;

/// The active HF dials (kHz) in deck order: finite, >0, within the HF window
/// (1800–30000 kHz). Errors on >11 (VOACAP's slot limit) — never silent-truncate.
/// F1: the command layer calls this to know which exact input dials map to each
/// output column, so results carry the exact kHz, not VOACAP's lossy display.
pub fn active_hf_frequencies_khz(input: &[f64]) -> Result<Vec<f64>, PropagationError> {
    let kept: Vec<f64> = input
        .iter()
        .copied()
        .filter(|f| f.is_finite() && *f > 0.0 && (1800.0..=30000.0).contains(f))
        .collect();
    if kept.is_empty() {
        return Err(PropagationError::NoFrequencies);
    }
    if kept.len() > 11 {
        return Err(PropagationError::TooManyFrequencies(kept.len()));
    }
    Ok(kept)
}

/// Build a VOACAP input deck for the given prediction inputs.
///
/// Returns the complete deck as a `String` (ASCII, `\n`-terminated). The caller
/// writes it to a temp file and passes the path to `voacapl`.
pub fn build_deck(inputs: &PredictionInputs) -> Result<String, PropagationError> {
    let freqs_khz = active_hf_frequencies_khz(&inputs.frequencies_khz)?;

    let (tx_lat, tx_lon) = grid_to_lat_lon(&inputs.tx_grid)
        .ok_or_else(|| PropagationError::InvalidGrid(inputs.tx_grid.clone()))?;
    let (rx_lat, rx_lon) = grid_to_lat_lon(&inputs.rx_grid)
        .ok_or_else(|| PropagationError::InvalidGrid(inputs.rx_grid.clone()))?;

    // Split signed coordinates into hemisphere letter + magnitude.
    let (tla, tlah) = (tx_lat.abs(), if tx_lat >= 0.0 { 'N' } else { 'S' });
    let (tlo, tloh) = (tx_lon.abs(), if tx_lon >= 0.0 { 'E' } else { 'W' });
    let (rla, rlah) = (rx_lat.abs(), if rx_lat >= 0.0 { 'N' } else { 'S' });
    let (rlo, rloh) = (rx_lon.abs(), if rx_lon >= 0.0 { 'E' } else { 'W' });

    // FREQUENCY card: 11 F5.2 MHz slots, unused = 0.00.
    let mut mhz_padded: Vec<f64> = freqs_khz.iter().map(|f| f / 1000.0).collect();
    mhz_padded.resize(11, 0.0);
    let freq_slots: String = mhz_padded.iter().map(|f| format!("{:5.2}", f)).collect();

    // SUNSPOT: F5.0 with trailing dot.
    let sunspot = format!("SUNSPOT   {:>5}", format!("{:.0}.", inputs.ssn));

    // SYSTEM card: REQ.SNR comes from inputs.req_snr_db (F7).
    // v1 default 73.0 (VOACAP standard, matching the captured fixture).
    // The data-mode-calibrated value (VARA/ARDOP) is a documented empirical
    // tunable — do NOT invent a VARA SNR number here.
    let system = format!(
        "SYSTEM       1. 145. 0.10  90. {:4.1} 3.00 0.10",
        inputs.req_snr_db
    );

    // ANTENNA tx: power in kW (F10.4).
    let ant_tx = format!(
        "ANTENNA       1    1    2   30     0.000[default/const17.voa  ]  0.0{:10.4}",
        inputs.tx_power_w / 1000.0
    );

    let lines: Vec<String> = vec![
        "COMMENT    Any VOACAP default cards may be placed in the file: VOACAP.DEF".to_string(),
        "LINEMAX      55       number of lines-per-page".to_string(),
        "COEFFS    CCIR".to_string(),
        format!("TIME      {:5}{:5}{:5}{:5}", 1, 24, 1, 1),
        // F8: year comes from inputs.year (NOT hardcoded).
        format!("MONTH     {:5}{:5.2}", inputs.year, inputs.month as f64),
        sunspot,
        format!(
            "LABEL     {:<20}{:<20}",
            inputs.tx_grid, inputs.rx_grid
        ),
        // CIRCUIT: TX-lat F5.2, TX-lon/RX-lat/RX-lon each F9.2; hemisphere carries sign.
        format!(
            "CIRCUIT   {:5.2}{}{:9.2}{}{:9.2}{}{:9.2}{}  S {:5}",
            tla, tlah, tlo, tloh, rla, rlah, rlo, rloh, 0
        ),
        system,
        "FPROB      1.00 1.00 1.00 0.00".to_string(),
        ant_tx,
        "ANTENNA       2    2    2   30     0.000[default/swwhip.voa   ]  0.0    0.0000"
            .to_string(),
        format!("FREQUENCY {}", freq_slots),
        "METHOD       30    0".to_string(),
        "EXECUTE".to_string(),
        "QUIT".to_string(),
    ];

    Ok(lines.join("\n") + "\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::propagation::PredictionInputs;

    fn dm43_dm34() -> PredictionInputs {
        PredictionInputs {
            tx_grid: "DM43".to_string(),
            rx_grid: "DM34".to_string(),
            frequencies_khz: vec![3590.0, 7103.0, 7108.0, 10147.0, 14103.0, 14115.0],
            year: 2026,
            month: 6,
            ssn: 100.0,
            tx_power_w: 100.0,
            req_snr_db: 73.0,
        }
    }

    #[test]
    fn circuit_card_matches_fortran_format_widths() {
        let deck = build_deck(&dm43_dm34()).unwrap();
        let circuit_line = deck
            .lines()
            .find(|l| l.starts_with("CIRCUIT"))
            .expect("CIRCUIT line missing");
        assert_eq!(
            circuit_line,
            "CIRCUIT   33.50N   111.00W    34.50N   113.00W  S     0"
        );
        assert_eq!(circuit_line.len(), 55);
    }

    #[test]
    fn frequencies_convert_khz_to_mhz_and_pad_to_11() {
        let deck = build_deck(&dm43_dm34()).unwrap();
        let freq_line = deck
            .lines()
            .find(|l| l.starts_with("FREQUENCY"))
            .expect("FREQUENCY line missing");
        assert_eq!(
            freq_line,
            "FREQUENCY  3.59 7.10 7.1110.1514.1014.12 0.00 0.00 0.00 0.00 0.00"
        );
    }

    #[test]
    fn matches_captured_golden_deck() {
        let golden = include_str!("../../tests/fixtures/voacap/dm43-dm34-input-deck.dat");
        let built = build_deck(&dm43_dm34()).unwrap();

        let strip_label = |s: &str| -> String {
            s.lines()
                .filter(|l| !l.starts_with("LABEL"))
                .collect::<Vec<_>>()
                .join("\n")
                + "\n"
        };

        let golden_stripped = strip_label(golden);
        let built_stripped = strip_label(&built);

        if golden_stripped != built_stripped {
            // Print a line-by-line diff for diagnostics before failing.
            let g_lines: Vec<&str> = golden_stripped.lines().collect();
            let b_lines: Vec<&str> = built_stripped.lines().collect();
            for (i, (g, b)) in g_lines.iter().zip(b_lines.iter()).enumerate() {
                if g != b {
                    eprintln!("line {i} MISMATCH");
                    eprintln!("  golden: {g:?} (len {})", g.len());
                    eprintln!("  built:  {b:?} (len {})", b.len());
                }
            }
            if g_lines.len() != b_lines.len() {
                eprintln!(
                    "line count mismatch: golden={} built={}",
                    g_lines.len(),
                    b_lines.len()
                );
            }
        }

        assert_eq!(golden_stripped, built_stripped);
    }

    #[test]
    fn invalid_grid_is_error() {
        let mut inputs = dm43_dm34();
        inputs.rx_grid = "ZZ".to_string();
        assert!(matches!(build_deck(&inputs), Err(PropagationError::InvalidGrid(_))));
    }

    #[test]
    fn no_finite_frequencies_is_error() {
        let result = active_hf_frequencies_khz(&[0.0, -1.0, f64::NAN]);
        assert!(matches!(result, Err(PropagationError::NoFrequencies)));
    }

    #[test]
    fn non_hf_frequencies_are_dropped() {
        // 146000 kHz = 2m; should be dropped, only HF freqs survive.
        let freqs = vec![7103.0, 146000.0, 14103.0];
        let kept = active_hf_frequencies_khz(&freqs).unwrap();
        assert_eq!(kept, vec![7103.0, 14103.0]);

        // Also verify the deck doesn't include 146.00 MHz in the FREQUENCY card.
        let mut inputs = dm43_dm34();
        inputs.frequencies_khz = freqs.clone();
        let deck = build_deck(&inputs).unwrap();
        assert!(!deck.contains("146.00"), "2m freq must not appear in deck");
    }

    #[test]
    fn too_many_frequencies_is_error() {
        // 12 distinct in-window HF dials.
        let freqs: Vec<f64> = (1..=12).map(|i| 3500.0 + i as f64 * 100.0).collect();
        assert_eq!(freqs.len(), 12);
        assert!(matches!(
            active_hf_frequencies_khz(&freqs),
            Err(PropagationError::TooManyFrequencies(12))
        ));
    }

    #[test]
    fn year_flows_into_month_card() {
        let mut inputs = dm43_dm34();
        inputs.year = 2030;
        let deck = build_deck(&inputs).unwrap();
        let month_line = deck
            .lines()
            .find(|l| l.starts_with("MONTH"))
            .expect("MONTH line missing");
        assert!(month_line.contains("2030"), "year 2030 must appear in MONTH card");
    }

    #[test]
    fn req_snr_flows_into_system_card() {
        let mut inputs = dm43_dm34();
        inputs.req_snr_db = 20.0;
        let deck = build_deck(&inputs).unwrap();
        let system_line = deck
            .lines()
            .find(|l| l.starts_with("SYSTEM"))
            .expect("SYSTEM line missing");
        assert!(system_line.contains("20.0"), "req_snr_db=20.0 must appear in SYSTEM card");
    }
}
