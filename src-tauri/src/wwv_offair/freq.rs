/// Coarse WWV frequency choice by UTC hour. 10 MHz is the safe all-rounder; the
/// simple split below prefers 5 MHz overnight (better LF/MF propagation) and
/// 15 MHz midday. Operator override lands in WwvOffairConfig later.
pub fn freq_for_utc_hour(utc_hour: u8) -> u64 {
    match utc_hour {
        0..=11 => 5_000_000,
        _ => 15_000_000,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_night_frequency_selection() {
        assert_eq!(freq_for_utc_hour(3), 5_000_000);   // night
        assert_eq!(freq_for_utc_hour(18), 15_000_000); // day
        assert_eq!(freq_for_utc_hour(11), 5_000_000);
        assert_eq!(freq_for_utc_hour(12), 15_000_000);
    }
}
