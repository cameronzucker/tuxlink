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

/// Reverse lookup (spec §NewCommands `ft8_cat_probe` row, tuxlink-b026z.4
/// Task A4): the band whose canonical FT8 dial is within a small tolerance
/// of `hz`. Tolerance is ±3 kHz — the same window `service.rs`'s
/// start-labeling `nearest_band` helper uses to absorb minor VFO/rig-CAT
/// rounding, so a rig read that lands a few hundred Hz off the pinned dial
/// (e.g. 14_074_050) still resolves to its band instead of `None`. Returns
/// `None` when `hz` falls outside every band's window (out-of-band dial, or
/// a rig CAT bug reporting garbage).
pub fn band_for_dial(hz: u64) -> Option<&'static str> {
    BANDS
        .iter()
        .find(|(_, dial)| hz.abs_diff(*dial) <= 3_000)
        .map(|&(b, _)| b)
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

    #[test]
    fn band_for_dial_every_pinned_dial_round_trips() {
        for (band, hz) in BANDS {
            assert_eq!(band_for_dial(hz), Some(band), "band {band}");
        }
    }

    #[test]
    fn band_for_dial_exact_20m() {
        assert_eq!(band_for_dial(14_074_000), Some("20m"));
    }

    #[test]
    fn band_for_dial_tolerates_small_offset() {
        assert_eq!(band_for_dial(14_074_500), Some("20m"), "+500 Hz within ±3 kHz");
        assert_eq!(band_for_dial(14_071_200), Some("20m"), "-2800 Hz within ±3 kHz");
    }

    #[test]
    fn band_for_dial_outside_tolerance_is_none() {
        assert_eq!(band_for_dial(14_078_000), None, "+4 kHz exceeds ±3 kHz");
        assert_eq!(band_for_dial(0), None);
        assert_eq!(band_for_dial(u64::MAX), None);
    }

    #[test]
    fn band_for_dial_never_panics_on_boundary_adjacent_bands() {
        // 40m (7_074_000) and 30m (10_136_000) are far enough apart that a
        // value between them stays None — no accidental "nearest of two"
        // ambiguity within the ±3 kHz window.
        assert_eq!(band_for_dial(8_500_000), None);
    }
}
