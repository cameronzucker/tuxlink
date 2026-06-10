//! Parser for voacapl's METHOD-30 `voacapx.out` output.
//!
//! Carries EXACT input-dial frequencies by column index (F1) — never re-derives
//! them from VOACAP's lossy display (e.g. 3590 kHz → "3.6", 7103 & 7108 kHz both
//! → "7.1"). Parses REL, SNR, and MUFday per-hour per-channel (F7). Tokenises to
//! the fixed label boundary (col 67) instead of a hardcoded DATA_END (F4). Guards
//! all three 24-element vectors after parsing (F16). Documents azimuth/distance
//! magic indices (F19).

use super::{ChannelReliability, PathPrediction, PropagationError};

/// Label column boundary (0-based, exclusive): all right-edge row labels begin at
/// this column.  Determined by inspecting the fixture — every label string in the
/// output starts at col 67.
const LABEL_COL: usize = 67;

/// Parse voacapl's METHOD-30 voacapx.out into a PathPrediction.
///
/// `active_freqs_khz` is the exact input dials in deck order (from
/// `deck::active_hf_frequencies_khz`) — these EXACT kHz values are carried into
/// each [`ChannelReliability::frequency_khz`] by column index (F1).
/// `ssn`/`year`/`month` are provenance passthrough (F12).
/// REL, SNR, and MUFday are read positionally; `-` tokens in trailing unused
/// slots (beyond `freq_count`) are never reached and are harmless.
pub fn parse_voacapx_out(
    text: &str,
    active_freqs_khz: &[f64],
    ssn: f64,
    year: i32,
    month: u8,
) -> Result<PathPrediction, PropagationError> {
    let freq_count = active_freqs_khz.len();

    // ── Summary line (bearing + distance) ────────────────────────────────────
    // Locate the header line that contains both "AZIMUTHS" and "KM", then take
    // the very next non-empty line which carries the numeric summary.
    let (bearing_deg, distance_km) = parse_bearing_distance(text)?;

    // ── FREQ header: capture voacap_mhz display values (informational) ───────
    // The FREQ header row format is:
    //   <hour_tok> <MUF_tok> <freq_slot_0> ... <freq_slot_10>  FREQ
    // The first two numeric tokens are hour and MUF; the remainder are the 11
    // rounded-MHz display slots.  We capture them once from the first FREQ row.
    let voacap_mhz_display = parse_voacap_mhz_display(text, freq_count)?;

    // ── Per-hour data rows ────────────────────────────────────────────────────
    // 24 hourly blocks, each containing rows labelled REL, SNR, MUFday (among
    // others we ignore).  Collect 24 values per channel for each parameter.
    let mut rel_rows: Vec<Vec<f64>> = Vec::new(); // [hour][channel]
    let mut snr_rows: Vec<Vec<f64>> = Vec::new();
    let mut mufday_rows: Vec<Vec<f64>> = Vec::new();

    for line in text.lines() {
        let label = line_label(line);
        match label {
            Some("REL") => {
                let vals = parse_data_row(line, freq_count)?;
                rel_rows.push(vals);
            }
            Some("SNR") => {
                let vals = parse_data_row(line, freq_count)?;
                snr_rows.push(vals);
            }
            Some("MUFday") => {
                let vals = parse_data_row(line, freq_count)?;
                mufday_rows.push(vals);
            }
            _ => {}
        }
    }

    // ── F16: guard all three 24-length vectors ────────────────────────────────
    if rel_rows.len() != 24 {
        return Err(PropagationError::ParseFailed(format!(
            "expected 24 REL rows, got {}",
            rel_rows.len()
        )));
    }
    if snr_rows.len() != 24 {
        return Err(PropagationError::ParseFailed(format!(
            "expected 24 SNR rows, got {}",
            snr_rows.len()
        )));
    }
    if mufday_rows.len() != 24 {
        return Err(PropagationError::ParseFailed(format!(
            "expected 24 MUFday rows, got {}",
            mufday_rows.len()
        )));
    }

    // ── Assemble per-channel results ──────────────────────────────────────────
    let channels: Vec<ChannelReliability> = (0..freq_count)
        .map(|i| ChannelReliability {
            // F1: carry the exact input dial — never parse from output
            frequency_khz: active_freqs_khz[i],
            // Informational rounded MHz as VOACAP computed this column
            voacap_mhz: voacap_mhz_display[i],
            rel_by_hour: rel_rows.iter().map(|r| r[i]).collect(),
            snr_by_hour: snr_rows.iter().map(|r| r[i]).collect(),
            mufday_by_hour: mufday_rows.iter().map(|r| r[i]).collect(),
        })
        .collect();

    Ok(PathPrediction {
        bearing_deg,
        distance_km,
        ssn,
        year,
        month,
        channels,
    })
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Extract the right-edge label from a line.
///
/// Returns the trimmed content of `line[LABEL_COL..]`, which is the full label
/// string (e.g. `"SNR"`, `"SNR LW"`, `"MUFday"`, `"FREQ"`).  Using the full
/// trimmed region (not just the first token) ensures that `"SNR"` does NOT match
/// `"SNR LW"` or `"SNR UP"`, which all begin with the same first token.
///
/// Returns `None` if the line is shorter than `LABEL_COL` or the label region is
/// blank.
fn line_label(line: &str) -> Option<&str> {
    if line.len() <= LABEL_COL {
        return None;
    }
    let label_region = &line[LABEL_COL..];
    let trimmed = label_region.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Parse the numeric data tokens from a non-FREQ data row.
///
/// Slices to [`LABEL_COL`] first so the label text never bleeds into the token
/// stream (F4 fix).
///
/// **Column-alignment assumption:** METHOD-30 emits a numeric value (0.00
/// minimum) for every active channel; `-` appears only in unused deck slots
/// beyond `freq_count`.  A `-` (or any non-numeric token) within the first
/// `freq_count` whitespace-split tokens indicates broken column alignment and
/// is treated as a parse error rather than silently compacted away.
///
/// The implementation takes the FIRST `freq_count` tokens positionally and
/// requires each to parse as `f64`.  Trailing `-`-padded slots beyond
/// `freq_count` are never reached by the `take`, so they remain harmless.
fn parse_data_row(line: &str, freq_count: usize) -> Result<Vec<f64>, PropagationError> {
    let data_region = if line.len() > LABEL_COL {
        &line[..LABEL_COL]
    } else {
        line
    };

    let tokens: Vec<&str> = data_region.split_whitespace().collect();

    if tokens.len() < freq_count {
        return Err(PropagationError::ParseFailed(format!(
            "data row has {} whitespace tokens, need {} (line: {:?})",
            tokens.len(),
            freq_count,
            &line[..line.len().min(80)]
        )));
    }

    // Take exactly the first freq_count tokens positionally; any non-numeric
    // within that window is a column-alignment error — fail closed.
    let mut vals = Vec::with_capacity(freq_count);
    for tok in tokens.iter().take(freq_count) {
        match tok.parse::<f64>() {
            Ok(v) => vals.push(v),
            Err(_) => {
                return Err(PropagationError::ParseFailed(format!(
                    "non-numeric token {:?} in first {} columns of data row (line: {:?})",
                    tok,
                    freq_count,
                    &line[..line.len().min(80)]
                )));
            }
        }
    }

    Ok(vals)
}

/// Parse voacap_mhz display values from the first FREQ header row.
///
/// The FREQ row format: `<hour> <MUF> <slot0> ... <slot10>  FREQ`
/// Skip the first 2 tokens (hour, MUF) to reach the 11 freq display slots.
/// Validates that at least `freq_count` display slots are present (F4).
fn parse_voacap_mhz_display(
    text: &str,
    freq_count: usize,
) -> Result<Vec<f64>, PropagationError> {
    for line in text.lines() {
        if line_label(line) != Some("FREQ") {
            continue;
        }
        let data_region = if line.len() > LABEL_COL {
            &line[..LABEL_COL]
        } else {
            line
        };
        let tokens: Vec<&str> = data_region.split_whitespace().collect();
        // tokens[0] = hour, tokens[1] = MUF, tokens[2..] = freq display slots
        if tokens.len() < 2 + freq_count {
            return Err(PropagationError::ParseFailed(format!(
                "FREQ header has {} tokens (need at least {} for {} channels + 2 prefix)",
                tokens.len(),
                2 + freq_count,
                freq_count,
            )));
        }
        let display: Vec<f64> = tokens[2..2 + freq_count]
            .iter()
            .filter_map(|t| t.parse::<f64>().ok())
            .collect();
        if display.len() < freq_count {
            return Err(PropagationError::ParseFailed(format!(
                "FREQ header: could only parse {} display-MHz values, need {}",
                display.len(),
                freq_count,
            )));
        }
        return Ok(display);
    }
    Err(PropagationError::ParseFailed(
        "no FREQ header row found in voacapx.out".to_string(),
    ))
}

/// Extract bearing (TX→RX azimuth) and distance (km) from the circuit summary.
///
/// Finds the header line containing both "AZIMUTHS" and "KM", then reads the
/// numeric tokens from the immediately following line.
///
/// **Layout assumption:** the summary data line immediately follows the
/// AZIMUTHS/KM header with no intervening blank line — holds for the METHOD-30
/// page layout as observed in the fixture.
///
/// Summary line format (from the fixture):
/// ```text
///   33.50 N  111.00 W - 34.50 N  113.00 W    301.65  120.54     116.2    215.2
/// ```
/// The hemisphere letters (N, W, N, W) and the `-` separator are
/// space-separated in real VOACAP output and therefore filter out as
/// non-numeric tokens, leaving 8 numbers: [33.50, 111.00, 34.50, 113.00,
/// 301.65, 120.54, 116.2, 215.2].
///
/// F19 layout — magic indices (documented):
///   nums[len-4] = TX→RX azimuth (301.65 in example)
///   nums[len-3] = RX→TX azimuth (120.54, not used)
///   nums[len-2] = distance in nautical miles (116.2, not used)
///   nums[len-1] = distance in km (215.2)
fn parse_bearing_distance(text: &str) -> Result<(f64, f64), PropagationError> {
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        if line.contains("AZIMUTHS") && line.contains("KM") {
            // Next line is the summary
            if let Some(summary) = lines.next() {
                let nums: Vec<f64> = summary
                    .split_whitespace()
                    .filter_map(|tok| tok.parse::<f64>().ok())
                    .collect();
                if nums.len() < 4 {
                    return Err(PropagationError::ParseFailed(format!(
                        "AZIMUTHS/KM summary line has too few numeric tokens ({}) in {:?}",
                        nums.len(),
                        summary
                    )));
                }
                // F19: bearing = nums[len-4], km = nums[len-1]
                let bearing = nums[nums.len() - 4];
                let km = nums[nums.len() - 1];
                return Ok((bearing, km));
            }
            return Err(PropagationError::ParseFailed(
                "AZIMUTHS/KM header found but no following summary line".to_string(),
            ));
        }
    }
    Err(PropagationError::ParseFailed(
        "no AZIMUTHS/KM header line found in voacapx.out".to_string(),
    ))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str =
        include_str!("../../tests/fixtures/voacap/dm43-dm34-voacapx.out");

    fn active() -> Vec<f64> {
        vec![3590.0, 7103.0, 7108.0, 10147.0, 14103.0, 14115.0]
    }

    fn parsed() -> PathPrediction {
        parse_voacapx_out(FIXTURE, &active(), 100.0, 2026, 6).expect("fixture must parse")
    }

    #[test]
    fn parses_bearing_and_distance() {
        let p = parsed();
        assert!(
            (p.bearing_deg - 301.65).abs() < 0.01,
            "bearing_deg expected ~301.65, got {}",
            p.bearing_deg
        );
        assert!(
            (p.distance_km - 215.2).abs() < 0.1,
            "distance_km expected ~215.2, got {}",
            p.distance_km
        );
    }

    #[test]
    fn parses_24_hour_rel_snr_mufday_per_frequency() {
        let p = parsed();
        assert_eq!(p.channels.len(), 6, "expected 6 channels");
        for ch in &p.channels {
            assert_eq!(
                ch.rel_by_hour.len(),
                24,
                "freq {}: rel_by_hour len {}",
                ch.frequency_khz,
                ch.rel_by_hour.len()
            );
            assert_eq!(
                ch.snr_by_hour.len(),
                24,
                "freq {}: snr_by_hour len {}",
                ch.frequency_khz,
                ch.snr_by_hour.len()
            );
            assert_eq!(
                ch.mufday_by_hour.len(),
                24,
                "freq {}: mufday_by_hour len {}",
                ch.frequency_khz,
                ch.mufday_by_hour.len()
            );
        }
    }

    /// F1 keystone: 7103 and 7108 must NOT collapse to the same value even though
    /// VOACAP's display rounds both to "7.1".  The exact input dials are carried by
    /// column index.
    #[test]
    fn carries_exact_input_frequency_by_index() {
        let p = parsed();
        assert_eq!(
            p.channels[0].frequency_khz, 3590.0,
            "channel[0] frequency_khz"
        );
        assert_eq!(
            p.channels[1].frequency_khz, 7103.0,
            "channel[1] frequency_khz (must not collapse to 7100 or 7108)"
        );
        assert_eq!(
            p.channels[2].frequency_khz, 7108.0,
            "channel[2] frequency_khz (must not collapse to 7100 or 7103)"
        );
        // Verify the lossy display is preserved separately (informational)
        assert!(
            (p.channels[1].voacap_mhz - 7.1).abs() < 0.05,
            "channel[1].voacap_mhz should be ~7.1 (rounded display), got {}",
            p.channels[1].voacap_mhz
        );
    }

    /// Values from fixture hour-1 block (index 0).
    #[test]
    fn rel_values_match_captured_data() {
        let p = parsed();
        // channel[1] = 7103 kHz; hour-1 REL = 0.21
        assert!(
            (p.channels[1].rel_by_hour[0] - 0.21).abs() < 1e-4,
            "ch[1] hour-1 REL expected 0.21, got {}",
            p.channels[1].rel_by_hour[0]
        );
        // channel[4] = 14103 kHz; hour-1 REL = 0.03
        assert!(
            (p.channels[4].rel_by_hour[0] - 0.03).abs() < 1e-4,
            "ch[4] hour-1 REL expected 0.03, got {}",
            p.channels[4].rel_by_hour[0]
        );
    }

    /// Values from fixture hour-1 block (index 0).
    #[test]
    fn snr_and_mufday_match_captured_data() {
        let p = parsed();
        // channel[1] = 7103 kHz; hour-1 SNR = 65
        assert!(
            (p.channels[1].snr_by_hour[0] - 65.0).abs() < 0.5,
            "ch[1] hour-1 SNR expected 65, got {}",
            p.channels[1].snr_by_hour[0]
        );
        // channel[1] = 7103 kHz; hour-1 MUFday = 1.00
        assert!(
            (p.channels[1].mufday_by_hour[0] - 1.00).abs() < 1e-4,
            "ch[1] hour-1 MUFday expected 1.00, got {}",
            p.channels[1].mufday_by_hour[0]
        );
    }

    #[test]
    fn provenance_passthrough() {
        let p =
            parse_voacapx_out(FIXTURE, &active(), 100.0, 2026, 6).expect("fixture must parse");
        assert_eq!(p.ssn, 100.0);
        assert_eq!(p.year, 2026);
        assert_eq!(p.month, 6);
    }

    /// Fix 1: a `-` in the 2nd of 3 active columns must produce ParseFailed, not
    /// silently shift columns.  freq_count=3 via a 3-element active_freqs slice.
    #[test]
    fn dash_in_active_column_is_parse_error() {
        // Minimal output: AZIMUTHS/KM summary + 1 FREQ row + 24 REL/SNR/MUFday rows.
        // REL row 1 has `-` in the 2nd of 3 active columns — should trigger ParseFailed.
        let summary_block = "\
  DM43 ref            N0DAJ DM34            AZIMUTHS          N. MI.      KM\n\
  33.50 N  111.00 W - 34.50 N  113.00 W    301.65  120.54     116.2    215.2\n\
   1.0  7.8  3.6  7.1  0.0  0.0  0.0  0.0  0.0  0.0  0.0  0.0  0.0 FREQ\n";

        // 24 rows each for REL, SNR, MUFday; row 3 of REL has `-` in column 2.
        let mut text = summary_block.to_string();
        for i in 0..24usize {
            if i == 2 {
                // `-` in the 2nd active column — must fail closed
                text.push_str("       0.10  -   0.30   -    -    -    -    -    -    -    -  REL   \n");
            } else {
                text.push_str("       0.10 0.20 0.30   -    -    -    -    -    -    -    -  REL   \n");
            }
            text.push_str("        59   65   66   -    -    -    -    -    -    -    -  SNR   \n");
            text.push_str("       0.50 1.00 0.69   -    -    -    -    -    -    -    -  MUFday\n");
        }

        let active_3 = vec![3590.0, 7103.0, 7108.0];
        let result = parse_voacapx_out(&text, &active_3, 100.0, 2026, 6);
        assert!(
            matches!(result, Err(PropagationError::ParseFailed(_))),
            "expected ParseFailed when '-' appears in an active column, got: {:?}",
            result
        );
    }

    #[test]
    fn missing_summary_line_is_parse_error() {
        // Text with no AZIMUTHS/KM header → ParseFailed
        let text = "just some text\nno summary here\n";
        let result = parse_voacapx_out(text, &active(), 100.0, 2026, 6);
        assert!(
            matches!(result, Err(PropagationError::ParseFailed(_))),
            "expected ParseFailed, got: {:?}",
            result
        );
    }

    #[test]
    fn wrong_hour_count_is_parse_error() {
        // Truncated fixture: keep only 1 hour of REL/SNR/MUFday → F16 guard fires.
        // Build minimal text with AZIMUTHS/KM header + 1 hour of data.
        let text = "\
  DM43 ref            N0DAJ DM34            AZIMUTHS          N. MI.      KM\n\
  33.50 N  111.00 W - 34.50 N  113.00 W    301.65  120.54     116.2    215.2\n\
   1.0  7.8  3.6  7.1  7.1 10.1 14.1 14.1  0.0  0.0  0.0  0.0  0.0 FREQ\n\
       0.04 0.21 0.18 0.18 0.03 0.00 0.00   -    -    -    -    -  REL   \n\
         59   65   66   66   49    9    9   -    -    -    -    -  SNR   \n\
       0.50 1.00 0.69 0.69 0.02 0.00 0.00   -    -    -    -    -  MUFday\n\
";
        let result = parse_voacapx_out(text, &active(), 100.0, 2026, 6);
        assert!(
            matches!(result, Err(PropagationError::ParseFailed(_))),
            "expected ParseFailed for truncated data, got: {:?}",
            result
        );
    }
}
