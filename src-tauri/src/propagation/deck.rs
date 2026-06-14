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

/// The maximum filename length that fits the ANTENNA card's pattern slot.
/// The bracketed field is `[default/<name>]` where `default/` is 8 chars and the
/// whole bracket content is a fixed 21-char Fortran field — leaving 13 for `<name>`.
const ANTENNA_NAME_WIDTH: usize = 13;

/// Render the `[default/<file>]` field for an ANTENNA card, padding the filename
/// to the fixed 13-char slot. Returns an error (never silently truncates/shifts)
/// if the name overflows the slot — a card-width overflow would corrupt every
/// field to its right, exactly the failure class the `req_snr_db` guard prevents.
fn antenna_field(file: &str) -> Result<String, PropagationError> {
    if file.len() > ANTENNA_NAME_WIDTH {
        return Err(PropagationError::RunFailed(format!(
            "antenna file name {file:?} exceeds the {ANTENNA_NAME_WIDTH}-char ANTENNA card slot"
        )));
    }
    Ok(format!("[default/{file:<width$}]", width = ANTENNA_NAME_WIDTH))
}

/// Build a VOACAP input deck for the given prediction inputs.
///
/// Returns the complete deck as a `String` (ASCII, `\n`-terminated). The caller
/// writes it to a temp file and passes the path to `voacapl`.
pub fn build_deck(inputs: &PredictionInputs) -> Result<String, PropagationError> {
    let freqs_khz = active_hf_frequencies_khz(&inputs.frequencies_khz)?;

    // Guard req_snr_db to the SYSTEM card's 4-char Fortran field width ({:4.1}).
    // Values ≥ 100.0 format as "100.0" (5 chars) and silently overflow, shifting
    // the rest of the SYSTEM card. Negative or non-finite values are also invalid.
    if !inputs.req_snr_db.is_finite() || !(0.0..100.0).contains(&inputs.req_snr_db) {
        return Err(PropagationError::RunFailed(format!(
            "req_snr_db {} out of range (0..100 dB) — SYSTEM card uses a 4-char field",
            inputs.req_snr_db
        )));
    }

    // Guard the man-made-noise magnitude to a 3-digit value so the SYSTEM card's
    // fixed-width noise field ({:3.0}.) cannot overflow and shift the rest of the
    // card. Operator selections (140–164) are always in range; this guards a bad
    // programmatic value.
    if !inputs.noise_dbw.is_finite() || !(0.0..1000.0).contains(&inputs.noise_dbw) {
        return Err(PropagationError::RunFailed(format!(
            "noise_dbw {} out of range (0..1000) — SYSTEM card noise field width",
            inputs.noise_dbw
        )));
    }

    let (tx_lat, tx_lon) = grid_to_lat_lon(&inputs.tx_grid)
        .ok_or_else(|| PropagationError::InvalidGrid(inputs.tx_grid.clone()))?;
    let (rx_lat, rx_lon) = grid_to_lat_lon(&inputs.rx_grid)
        .ok_or_else(|| PropagationError::InvalidGrid(inputs.rx_grid.clone()))?;

    // Split signed coordinates into hemisphere letter + magnitude.
    // t=tx, r=rx; la=lat, lo=lon magnitude; *h = hemisphere letter (N/S/E/W).
    let (tla, tlah) = (tx_lat.abs(), if tx_lat >= 0.0 { 'N' } else { 'S' });
    let (tlo, tloh) = (tx_lon.abs(), if tx_lon >= 0.0 { 'E' } else { 'W' });
    let (rla, rlah) = (rx_lat.abs(), if rx_lat >= 0.0 { 'N' } else { 'S' });
    let (rlo, rloh) = (rx_lon.abs(), if rx_lon >= 0.0 { 'E' } else { 'W' });

    // FREQUENCY card: 11 F5.2 MHz slots, unused = 0.00.
    let mut mhz_padded: Vec<f64> = freqs_khz.iter().map(|f| f / 1000.0).collect();
    mhz_padded.resize(11, 0.0);
    let freq_slots: String = mhz_padded.iter().map(|f| format!("{:5.2}", f)).collect();

    // SUNSPOT: Fortran F5.0 field; the trailing '.' is required by VOACAP.
    // {:.0} on f64 yields fixed-decimal (e.g. 100), and the literal '.' appends
    // the Fortran dot.
    let sunspot = format!("SUNSPOT   {:>5}", format!("{:.0}.", inputs.ssn));

    // SYSTEM card: REQ.SNR comes from inputs.req_snr_db (F7).
    // v1 default 73.0 (VOACAP standard, matching the captured fixture).
    // The data-mode-calibrated value (VARA/ARDOP) is a documented empirical
    // tunable — do NOT invent a VARA SNR number here.
    // {:4.1} is a 4-char Fortran field ("73.0"); values ≥ 100.0 would overflow
    // to 5 chars and shift the rest of the card — guarded above in build_deck.
    let system = format!(
        "SYSTEM       1. {:3.0}. 0.10  90. {:4.1} 3.00 0.10",
        inputs.noise_dbw, inputs.req_snr_db
    );

    // ANTENNA cards: the bracketed `[default/<file>]` is a fixed 21-char Fortran
    // field — `default/` (8 chars) + the pattern filename padded to 13. The TX/RX
    // pattern files come from `inputs` (operator preset / parsed gateway antenna),
    // NOT hardcoded: the prior fixed RX `swwhip.voa` (a vertical with a zenith null)
    // is what made short NVIS paths predict ~0% reliability. `antenna_field` rejects
    // a filename too long for the 13-char slot rather than silently shifting the card.
    let ant_tx = format!(
        "ANTENNA       1    1    2   30     0.000{}  0.0{:10.4}",
        antenna_field(&inputs.tx_antenna_voa)?,
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
        // v1: short-path only ('S'). Long-path ('L') prediction for DX/antipodal paths
        // is deferred (adrev F15) — correct for the regional/NVIS emcomm use case, but a
        // DX station beyond ~half-circumference may read as unreachable. A future task
        // adds path-direction selection.
        format!(
            "CIRCUIT   {:5.2}{}{:9.2}{}{:9.2}{}{:9.2}{}  S {:5}",
            tla, tlah, tlo, tloh, rla, rlah, rlo, rloh, 0
        ),
        system,
        "FPROB      1.00 1.00 1.00 0.00".to_string(),
        ant_tx,
        format!(
            "ANTENNA       2    2    2   30     0.000{}  0.0    0.0000",
            antenna_field(&inputs.rx_antenna_voa)?
        ),
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
            // Legacy stock files: the captured golden deck was generated with these,
            // so the golden test pins them. Production defaults (operator preset /
            // parsed gateway antenna) are exercised by the antenna-specific tests.
            tx_antenna_voa: "const17.voa".to_string(),
            rx_antenna_voa: "swwhip.voa".to_string(),
            tx_antenna_voa_content: None,
            // 145 (residential) keeps the golden deck's "145." noise field.
            noise_dbw: 145.0,
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

    /// The RX (gateway) antenna is no longer hardcoded to the whip: it comes from
    /// `inputs.rx_antenna_voa`. An isotropic gateway (`ccir.000`) must render its own
    /// ANTENNA 2 card — this is the core of the NVIS-pessimism fix (the whip's zenith
    /// null is gone for non-vertical / unknown gateways).
    #[test]
    fn rx_antenna_comes_from_inputs_not_hardcoded_whip() {
        let mut inputs = dm43_dm34();
        inputs.rx_antenna_voa = "ccir.000".to_string();
        let deck = build_deck(&inputs).unwrap();
        let ant2 = deck
            .lines()
            .find(|l| l.starts_with("ANTENNA       2"))
            .expect("ANTENNA 2 (RX) line missing");
        assert_eq!(
            ant2,
            "ANTENNA       2    2    2   30     0.000[default/ccir.000     ]  0.0    0.0000"
        );
        assert!(!deck.contains("swwhip"), "whip must not appear once RX is isotropic");
    }

    /// The TX antenna likewise comes from `inputs.tx_antenna_voa` (operator preset).
    #[test]
    fn tx_antenna_comes_from_inputs() {
        let mut inputs = dm43_dm34();
        inputs.tx_antenna_voa = "ccir.000".to_string();
        let deck = build_deck(&inputs).unwrap();
        let ant1 = deck
            .lines()
            .find(|l| l.starts_with("ANTENNA       1"))
            .expect("ANTENNA 1 (TX) line missing");
        // Power tail (kW, F10.4) is preserved; only the pattern file changed.
        assert_eq!(
            ant1,
            "ANTENNA       1    1    2   30     0.000[default/ccir.000     ]  0.0    0.1000"
        );
    }

    /// An antenna filename longer than the 13-char card slot is rejected, never
    /// silently shifted (same card-integrity contract as the req_snr guard).
    #[test]
    fn overlong_antenna_name_is_error() {
        let mut inputs = dm43_dm34();
        inputs.tx_antenna_voa = "this_name_is_way_too_long.voa".to_string();
        assert!(
            matches!(build_deck(&inputs), Err(PropagationError::RunFailed(_))),
            "expected RunFailed for an antenna name overflowing the card slot"
        );
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

    #[test]
    fn noise_dbw_flows_into_system_card_at_fixed_width() {
        let mut inputs = dm43_dm34();
        inputs.noise_dbw = 150.0; // rural
        let deck = build_deck(&inputs).unwrap();
        let system_line = deck
            .lines()
            .find(|l| l.starts_with("SYSTEM"))
            .expect("SYSTEM line missing");
        // The noise field renders as "150." in the same fixed column as "145.".
        assert!(system_line.contains(" 150. "), "noise 150 must appear as '150.':\n{system_line}");
        // The card length is unchanged from the golden 145 case (no field shift).
        let golden = build_deck(&dm43_dm34()).unwrap();
        let golden_system = golden.lines().find(|l| l.starts_with("SYSTEM")).unwrap();
        assert_eq!(system_line.len(), golden_system.len(), "SYSTEM card width must be stable");
    }

    #[test]
    fn noise_dbw_out_of_range_is_error() {
        let mut inputs = dm43_dm34();
        inputs.noise_dbw = 1000.0; // 4 digits would overflow the field
        assert!(matches!(build_deck(&inputs), Err(PropagationError::RunFailed(_))));
        inputs.noise_dbw = f64::NAN;
        assert!(matches!(build_deck(&inputs), Err(PropagationError::RunFailed(_))));
    }

    /// Fix 3: req_snr_db ≥ 100.0 (or non-finite) must be rejected before building
    /// the deck — the SYSTEM card uses a 4-char Fortran field ({:4.1}) that would
    /// silently overflow for values ≥ 100.0, shifting the rest of the card.
    #[test]
    fn req_snr_out_of_range_is_error() {
        let mut inputs = dm43_dm34();
        inputs.req_snr_db = 150.0;
        assert!(
            matches!(build_deck(&inputs), Err(PropagationError::RunFailed(_))),
            "expected RunFailed for req_snr_db=150.0"
        );

        // Also check boundary: 100.0 itself overflows the 4-char field.
        inputs.req_snr_db = 100.0;
        assert!(
            matches!(build_deck(&inputs), Err(PropagationError::RunFailed(_))),
            "expected RunFailed for req_snr_db=100.0"
        );

        // Negative and non-finite values are also rejected.
        inputs.req_snr_db = -1.0;
        assert!(
            matches!(build_deck(&inputs), Err(PropagationError::RunFailed(_))),
            "expected RunFailed for req_snr_db=-1.0"
        );

        inputs.req_snr_db = f64::NAN;
        assert!(
            matches!(build_deck(&inputs), Err(PropagationError::RunFailed(_))),
            "expected RunFailed for req_snr_db=NaN"
        );

        // 99.9 is the maximum valid value (fits in 4 chars as "99.9").
        inputs.req_snr_db = 99.9;
        assert!(
            build_deck(&inputs).is_ok(),
            "expected Ok for req_snr_db=99.9"
        );
    }
}
