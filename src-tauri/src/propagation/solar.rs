//! Solar-index parsing for the "Update propagation data" feature (tuxlink-ot71).
//!
//! Pure + offline: no network, no disk. Callers fetch the bytes (over the
//! internet from NOAA SWPC, or over radio via a Winlink catalog reply) and hand
//! the raw text/JSON here. Everything is defensively tolerant — external solar
//! bulletins are human-formatted and occasionally malformed, so a parse failure
//! degrades to `None`/`Err` and the caller falls back, never panics.
//!
//! Two source shapes, per the operator's model-correct decision (2026-06-16):
//!  - `parse_swpc_predicted_ssn`: the SWPC `predicted-solar-cycle.json` product —
//!    monthly **smoothed** SSN, the exact VOACAP input. The internet primary.
//!  - `parse_wwv`: the WWV/SGAS prose bulletin (SFI/A/K). Feeds the conditions
//!    bar, and over radio is the only source available, so the RF fallback
//!    derives SSN from its SFI via `derive_ssn_from_sfi`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::ssn::SsnForecast;
use super::PropagationError;

/// Live solar indices for the conditions bar (context only — NOT the SSN model
/// when an internet smoothed-SSN forecast is available).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SolarIndices {
    /// 10.7 cm solar radio flux (SFI).
    pub sfi: f64,
    /// Planetary A index, when the bulletin states it.
    pub a_index: Option<f64>,
    /// Planetary K index, when the bulletin states it.
    pub k_index: Option<f64>,
}

/// One row of SWPC `predicted-solar-cycle.json`. We read only the two fields the
/// forecast needs; the product carries many more (`high_ssn`, f10.7 bands, …)
/// which `serde` ignores.
#[derive(Debug, Deserialize)]
struct SwpcPredictedRow {
    #[serde(rename = "time-tag")]
    time_tag: String,
    predicted_ssn: f64,
}

/// Parse the SWPC `predicted-solar-cycle.json` array into an [`SsnForecast`].
///
/// `predicted_ssn` is the **smoothed** sunspot number keyed by `YYYY-MM`, which
/// is exactly the VOACAP input and exactly the `SsnForecast.monthly` shape — so
/// this is a direct projection. Rows with a malformed key or a non-finite /
/// negative SSN are skipped defensively rather than failing the whole update.
pub fn parse_swpc_predicted_ssn(json: &str) -> Result<SsnForecast, PropagationError> {
    let rows: Vec<SwpcPredictedRow> = serde_json::from_str(json)
        .map_err(|e| PropagationError::Ssn(format!("swpc predicted-ssn parse: {e}")))?;
    let mut monthly: BTreeMap<String, f64> = BTreeMap::new();
    for r in rows {
        if is_year_month(&r.time_tag) && r.predicted_ssn.is_finite() && r.predicted_ssn >= 0.0 {
            monthly.insert(r.time_tag, r.predicted_ssn);
        }
    }
    if monthly.is_empty() {
        return Err(PropagationError::Ssn(
            "swpc predicted-ssn had no usable monthly rows".to_string(),
        ));
    }
    Ok(SsnForecast { monthly })
}

/// Parse a WWV-style geophysical bulletin into live [`SolarIndices`].
///
/// Verified live format (NOAA SWPC `wwv.txt`, 2026-06-16):
/// ```text
/// Solar flux 117 and estimated planetary A-index 6.
/// The estimated planetary K-index at 1200 UTC on 16 June was 1.33.
/// ```
/// SFI is required (its absence means this isn't a solar bulletin → `None`);
/// A and K are best-effort. K is read after the word "was" so the time-of-day
/// number ("at 1200 UTC") on the K line is not mistaken for the index.
pub fn parse_wwv(text: &str) -> Option<SolarIndices> {
    let sfi = number_after(text, "solar flux")?;
    // Sanity bound: real SFI sits ~64 (floor) to ~300+; reject garbage.
    if !(50.0..=500.0).contains(&sfi) {
        return None;
    }
    let a_index = number_after(text, "a-index").or_else(|| number_after(text, "a index"));
    // K index: locate the K-index clause, then read the number after "was"
    // (skips the "1200 UTC" timestamp); fall back to the first number on the
    // clause if the bulletin omits "was".
    let k_index = find_ci(text, "k-index")
        .or_else(|| find_ci(text, "k index"))
        .and_then(|pos| {
            let clause = &text[pos..];
            number_after(clause, "was").or_else(|| number_after(clause, "index"))
        });
    Some(SolarIndices { sfi, a_index, k_index })
}

/// Derive a sunspot number from the 10.7 cm solar flux.
///
/// Uses the standard published F10.7 ↔ SSN relation (Covington/NOAA):
/// `F10.7 = 63.7 + 0.728·R + 0.00089·R²`, inverted for R via the quadratic
/// formula. This is established public ionospheric science, not a Winlink- or
/// VARA-proprietary mapping. Over radio only daily SFI is available, so the RF
/// fallback uses this coarse derivation; the internet path uses SWPC's smoothed
/// SSN directly and never calls this.
///
/// Clamped at 0 (a quiet-sun SFI below ~64 yields a tiny/negative root).
pub fn derive_ssn_from_sfi(sfi: f64) -> f64 {
    // 0.00089 R² + 0.728 R + (63.7 - F) = 0
    const A: f64 = 0.00089;
    const B: f64 = 0.728;
    let c = 63.7 - sfi;
    let disc = B * B - 4.0 * A * c;
    if disc <= 0.0 {
        return 0.0;
    }
    let r = (-B + disc.sqrt()) / (2.0 * A);
    if r.is_finite() && r > 0.0 {
        r
    } else {
        0.0
    }
}

// ---- helpers ---------------------------------------------------------------

/// True for a well-formed `YYYY-MM` key (4 digits, '-', month 01..=12).
fn is_year_month(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 7 || b[4] != b'-' {
        return false;
    }
    if !b[0..4].iter().all(|c| c.is_ascii_digit()) {
        return false;
    }
    if !b[5..7].iter().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let month = (b[5] - b'0') * 10 + (b[6] - b'0');
    (1..=12).contains(&month)
}

/// Case-insensitive byte-offset search (inputs are ASCII NOAA bulletins).
fn find_ci(haystack: &str, needle: &str) -> Option<usize> {
    haystack.to_ascii_lowercase().find(&needle.to_ascii_lowercase())
}

/// First numeric token (integer or decimal) appearing after `needle`
/// (case-insensitive). Skips intervening non-numeric text.
fn number_after(text: &str, needle: &str) -> Option<f64> {
    let pos = find_ci(text, needle)?;
    let after = &text[pos + needle.len()..];
    let mut num = String::new();
    let mut seen_digit = false;
    for ch in after.chars() {
        if ch.is_ascii_digit() {
            num.push(ch);
            seen_digit = true;
        } else if ch == '.' && seen_digit {
            num.push(ch);
        } else if seen_digit {
            break;
        }
    }
    if seen_digit {
        // A trailing '.' (sentence period) must not break the parse.
        num.trim_end_matches('.').parse().ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- SWPC predicted-ssn JSON (real format, 2026-06-16) -----------------

    const SWPC_SAMPLE: &str = r#"[
        {"time-tag":"2025-12","predicted_ssn":108.1,"high_ssn":118.9,"predicted_f10.7":140.7},
        {"time-tag":"2026-01","predicted_ssn":107.8,"high_ssn":118.8,"predicted_f10.7":141.6},
        {"time-tag":"2026-02","predicted_ssn":106.4,"high_ssn":117.6,"predicted_f10.7":141.8}
    ]"#;

    #[test]
    fn swpc_predicted_ssn_maps_time_tag_to_monthly() {
        let f = parse_swpc_predicted_ssn(SWPC_SAMPLE).expect("should parse");
        assert_eq!(f.ssn_for(2026, 1), 107.8);
        assert_eq!(f.ssn_for(2025, 12), 108.1);
        assert_eq!(f.monthly.len(), 3);
    }

    #[test]
    fn swpc_skips_malformed_rows_but_keeps_good_ones() {
        let json = r#"[
            {"time-tag":"2026-01","predicted_ssn":107.8},
            {"time-tag":"bad-key","predicted_ssn":50.0},
            {"time-tag":"2026-13","predicted_ssn":50.0},
            {"time-tag":"2026-02","predicted_ssn":-5.0}
        ]"#;
        let f = parse_swpc_predicted_ssn(json).expect("should parse");
        assert_eq!(f.monthly.len(), 1, "only the one well-formed, non-negative row");
        assert_eq!(f.ssn_for(2026, 1), 107.8);
    }

    #[test]
    fn swpc_all_rows_unusable_is_error() {
        let json = r#"[{"time-tag":"bad","predicted_ssn":1.0}]"#;
        assert!(matches!(
            parse_swpc_predicted_ssn(json),
            Err(PropagationError::Ssn(_))
        ));
    }

    #[test]
    fn swpc_malformed_json_is_error() {
        assert!(matches!(
            parse_swpc_predicted_ssn("{not json"),
            Err(PropagationError::Ssn(_))
        ));
    }

    // ---- WWV bulletin (real format, 2026-06-16) ----------------------------

    const WWV_SAMPLE: &str = "\
:Product: Geophysical Alert Message wwv.txt
:Issued: 2026 Jun 16 1205 UTC
Solar-terrestrial indices for 15 June follow.
Solar flux 117 and estimated planetary A-index 6.
The estimated planetary K-index at 1200 UTC on 16 June was 1.33.
";

    #[test]
    fn wwv_extracts_sfi_a_and_k() {
        let s = parse_wwv(WWV_SAMPLE).expect("should parse");
        assert_eq!(s.sfi, 117.0);
        assert_eq!(s.a_index, Some(6.0));
        // K must be 1.33 (the value after "was"), NOT 1200 (the timestamp).
        assert_eq!(s.k_index, Some(1.33));
    }

    #[test]
    fn wwv_without_solar_flux_is_none() {
        assert!(parse_wwv("No space weather storms were observed.").is_none());
    }

    #[test]
    fn wwv_sfi_out_of_range_is_rejected() {
        // A stray "Solar flux 9" (below the ~64 quiet-sun floor) is garbage.
        assert!(parse_wwv("Solar flux 9 today.").is_none());
    }

    #[test]
    fn wwv_missing_a_and_k_still_yields_sfi() {
        let s = parse_wwv("Solar flux 142 reported.").expect("sfi alone is enough");
        assert_eq!(s.sfi, 142.0);
        assert_eq!(s.a_index, None);
        assert_eq!(s.k_index, None);
    }

    // ---- SSN from SFI (published Covington/NOAA relation) -------------------

    #[test]
    fn derive_ssn_is_monotonic_and_grounded() {
        // SFI 117 → SSN ~67-68 (the SGAS showed SESC sunspot ~67-86 for SFI
        // 114-118 on 2026-05, so this lands in the right neighborhood).
        let r = derive_ssn_from_sfi(117.0);
        assert!((60.0..75.0).contains(&r), "SFI 117 → SSN ~67, got {r}");
        // Monotonic increasing in SFI.
        assert!(derive_ssn_from_sfi(200.0) > derive_ssn_from_sfi(117.0));
        assert!(derive_ssn_from_sfi(117.0) > derive_ssn_from_sfi(80.0));
    }

    #[test]
    fn derive_ssn_quiet_sun_floors_at_zero() {
        // SFI at/below the ~64 quiet-sun floor → 0, never negative.
        assert_eq!(derive_ssn_from_sfi(60.0), 0.0);
        assert!(derive_ssn_from_sfi(64.0) >= 0.0);
    }

    #[test]
    fn is_year_month_validates() {
        assert!(is_year_month("2026-01"));
        assert!(is_year_month("2026-12"));
        assert!(!is_year_month("2026-13"));
        assert!(!is_year_month("2026-00"));
        assert!(!is_year_month("2026-1"));
        assert!(!is_year_month("abcd-01"));
        assert!(!is_year_month("2026/01"));
    }
}
