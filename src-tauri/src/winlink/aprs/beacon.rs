//! APRS beacon payload builder (tuxlink-hj4k).
//!
//! Bridges the operator's stored Maidenhead grid + privacy precision to an
//! on-air APRS position report. RF-honesty + privacy-consistency: the beacon
//! advertises EXACTLY the precision the operator chose to share. The stored grid
//! is first reduced by [`broadcast_grid`] (4-char by default), converted to the
//! square CENTRE by [`grid_to_lat_lon`], and encoded by [`encode_position`] with
//! a position-ambiguity level matching the grid resolution — so a peer tuxlink
//! client decodes the same coarse fix it would for any ambiguous beacon and
//! plots an uncertainty region, never a false-exact pin (the TX-side mirror of
//! the heard-positions map honesty, tuxlink-f717 / PR #705).

use crate::config::{broadcast_grid, PositionPrecision};
use crate::position::grid_to_lat_lon;

use super::position::encode_position;

/// Default APRS symbol for our beacon: primary table `/`, code `-` (house/QTH).
/// A sensible station default; the UI may expose a chooser later.
pub const DEFAULT_SYMBOL_TABLE: char = '/';
pub const DEFAULT_SYMBOL_CODE: char = '-';

/// Position-ambiguity level (0–4) that matches a broadcast-precision setting, so
/// the encoded beacon is no more precise than the grid the operator shares.
///
///   - `FourCharGrid` (~1° latitude band) → level 4 (±30′): masks all minute
///     digits, so the decoded ±30′ region covers the 4-char grid's latitude band.
///   - `SixCharGrid` (2.5′ latitude) → level 2 (±0.5′): the nearest level that
///     respects the finer precision the operator opted into without claiming the
///     exact subsquare centre.
pub fn precision_to_ambiguity(precision: PositionPrecision) -> u8 {
    match precision {
        PositionPrecision::FourCharGrid => 4,
        PositionPrecision::SixCharGrid => 2,
    }
}

/// Build the APRS info field for a beacon of our own position from the stored
/// `grid`, reduced to `precision`. Returns `None` when the stored grid is
/// missing/malformed (nothing to honestly beacon — the caller must not transmit).
pub fn beacon_info(
    grid: &str,
    precision: PositionPrecision,
    symbol_table: char,
    symbol_code: char,
    comment: &str,
) -> Option<Vec<u8>> {
    let reduced = broadcast_grid(grid, precision);
    let (lat, lon) = grid_to_lat_lon(&reduced)?;
    let ambiguity = precision_to_ambiguity(precision);
    Some(encode_position(lat, lon, symbol_table, symbol_code, comment, ambiguity))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::winlink::aprs::position::parse_position;

    #[test]
    fn four_char_grid_beacons_a_coarse_ambiguous_position() {
        // Stored full grid, default FourCharGrid precision → a level-4 ambiguous
        // beacon at the 4-char grid's centre.
        let info = beacon_info("FN42lq", PositionPrecision::FourCharGrid, '/', '-', "tuxlink").unwrap();
        let p = parse_position(&info).unwrap();
        assert_eq!(p.ambiguity, 4, "a 4-char-grid beacon must be coarse on air");
        assert_eq!(p.comment, "tuxlink");
        // Decoded fix sits inside the FN42 square (lat 42–43°N, lon 72–70°W).
        assert!((42.0..=43.0).contains(&p.lat), "lat {} not in FN42 band", p.lat);
        assert!((-72.0..=-70.0).contains(&p.lon), "lon {} not in FN42 band", p.lon);
    }

    #[test]
    fn six_char_grid_beacons_a_finer_position() {
        let info = beacon_info("FN42lq", PositionPrecision::SixCharGrid, '/', '-', "").unwrap();
        let p = parse_position(&info).unwrap();
        assert_eq!(p.ambiguity, 2, "a 6-char-grid beacon is finer but still not exact");
    }

    #[test]
    fn missing_or_malformed_grid_yields_no_beacon() {
        assert!(beacon_info("", PositionPrecision::FourCharGrid, '/', '-', "").is_none());
        assert!(beacon_info("ZZ", PositionPrecision::FourCharGrid, '/', '-', "").is_none());
    }

    #[test]
    fn precision_mapping() {
        assert_eq!(precision_to_ambiguity(PositionPrecision::FourCharGrid), 4);
        assert_eq!(precision_to_ambiguity(PositionPrecision::SixCharGrid), 2);
    }
}
