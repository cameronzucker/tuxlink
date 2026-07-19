/// Coarse WWV frequency choice by UTC hour. WWV/WWVH transmit on 5 / 10 / 15
/// MHz (among others); this picks one to tune. The heuristic follows the
/// D-layer: 5 MHz overnight (low absorption, the low bands reach), 15 MHz at
/// midday (D-layer absorption kills the low bands), and 10 MHz — the
/// documented all-rounder — across the dawn/dusk transition hours where
/// neither extreme is reliable.
///
/// tuxlink-76y11: the previous split returned ONLY 5 or 15 MHz and never 10,
/// despite its own comment naming 10 MHz the safe all-rounder — so a
/// transition-hour attempt tuned a band that was propagating poorly and came
/// back NoCopy. This is still a coarse UTC proxy for local solar time (it does
/// not know the receiver's longitude); a per-attempt frequency fallback across
/// windows (5 -> 10 -> 15 on repeated NoCopy) and an operator override remain
/// the next refinements on the same issue.
pub fn freq_for_utc_hour(utc_hour: u8) -> u64 {
    match utc_hour {
        0..=9 => 5_000_000,
        10..=13 | 22..=23 => 10_000_000,
        _ => 15_000_000,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequency_selection_covers_all_three_bands() {
        // Deep night: low band.
        assert_eq!(freq_for_utc_hour(3), 5_000_000);
        assert_eq!(freq_for_utc_hour(9), 5_000_000);
        // Dawn / dusk transition: the 10 MHz all-rounder — the band the old
        // split never returned (tuxlink-76y11).
        assert_eq!(freq_for_utc_hour(10), 10_000_000);
        assert_eq!(freq_for_utc_hour(13), 10_000_000);
        assert_eq!(freq_for_utc_hour(22), 10_000_000);
        // Daylight: high band.
        assert_eq!(freq_for_utc_hour(14), 15_000_000);
        assert_eq!(freq_for_utc_hour(19), 15_000_000);
        assert_eq!(freq_for_utc_hour(21), 15_000_000);
    }

    #[test]
    fn every_hour_maps_to_a_real_wwv_band() {
        for h in 0u8..24 {
            let f = freq_for_utc_hour(h);
            assert!(
                matches!(f, 5_000_000 | 10_000_000 | 15_000_000),
                "hour {h} -> {f} is not a WWV band we tune"
            );
        }
    }
}
