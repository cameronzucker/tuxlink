//! FT8 band → dial-frequency table (spec §Band table).
//!
//! Pinned FT8 dial frequencies (Hz), USB. The table is the single source for
//! Phase C's band chips, CAT start-labeling (nearest entry within ±3 kHz),
//! sweep QSY targets, and `ft8_set_band` validation.

/// Band label → dial Hz, low band to high. Order is part of the contract:
/// sweep round-robin walks the CONFIGURED band list, but display surfaces
/// sort by this table's order.
pub const BANDS: [(&str, u64); 9] = [
    ("160m", 1_840_000),
    ("80m", 3_573_000),
    ("40m", 7_074_000),
    ("30m", 10_136_000),
    ("20m", 14_074_000),
    ("17m", 18_100_000),
    ("15m", 21_074_000),
    ("12m", 24_915_000),
    ("10m", 28_074_000),
];

/// Exact-label lookup. Labels are case-sensitive lowercase ("20m", not
/// "20M") — the config layer owns normalization; this table does not guess.
pub fn dial_hz(band: &str) -> Option<u64> {
    BANDS.iter().find(|(b, _)| *b == band).map(|&(_, hz)| hz)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pinned_band_resolves_to_its_exact_dial() {
        let want: [(&str, u64); 9] = [
            ("160m", 1_840_000),
            ("80m", 3_573_000),
            ("40m", 7_074_000),
            ("30m", 10_136_000),
            ("20m", 14_074_000),
            ("17m", 18_100_000),
            ("15m", 21_074_000),
            ("12m", 24_915_000),
            ("10m", 28_074_000),
        ];
        for (band, hz) in want {
            assert_eq!(dial_hz(band), Some(hz), "band {band}");
        }
    }

    #[test]
    fn unknown_bands_are_none_never_panic() {
        for b in ["60m", "6m", "2m", "20M", " 20m", "20m ", "", "ft8", "14074"] {
            assert_eq!(dial_hz(b), None, "band {b:?}");
        }
    }

    #[test]
    fn table_shape_is_pinned() {
        assert_eq!(BANDS.len(), 9);
        // Strictly ascending in frequency — catches transposed entries.
        for w in BANDS.windows(2) {
            assert!(w[0].1 < w[1].1, "{} !< {}", w[0].0, w[1].0);
        }
    }
}
