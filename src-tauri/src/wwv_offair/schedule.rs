/// Nearest-upcoming WWV/WWVH voice-announcement window. WWV (Fort Collins,
/// CO) announces space-weather at :18 past the hour; WWVH (Kekaha, HI)
/// announces at :45. Pure modulo-hour millisecond arithmetic — no calendar
/// library needed since the schedule only depends on minute-of-hour.
const MIN: u64 = 60_000;
const HOUR: u64 = 3_600_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Station {
    Wwv,
    Wwvh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WwvWindow {
    pub station: Station,
    pub at_unix_ms: u64,
}

/// Returns the nearest upcoming WWV (:18) or WWVH (:45) announcement window
/// after `now_unix_ms`. Rolls into the next hour's WWV window once past :45.
pub fn next_window(now_unix_ms: u64) -> WwvWindow {
    let into_hour = now_unix_ms % HOUR;
    let hour_start = now_unix_ms - into_hour;
    let wwv = 18 * MIN;
    let wwvh = 45 * MIN;
    if into_hour < wwv {
        WwvWindow { station: Station::Wwv, at_unix_ms: hour_start + wwv }
    } else if into_hour < wwvh {
        WwvWindow { station: Station::Wwvh, at_unix_ms: hour_start + wwvh }
    } else {
        WwvWindow { station: Station::Wwv, at_unix_ms: hour_start + HOUR + wwv }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2026-07-11T12:00:00Z = 1_783_512_000_000 ms. Next window is WWV :18 (12:18).
    #[test]
    fn picks_wwv_18_when_before_18() {
        let now = 1_783_512_000_000; // 12:00:00Z
        let w = next_window(now);
        assert_eq!(w.station, Station::Wwv);
        assert_eq!(w.at_unix_ms, now + 18 * 60_000);
    }
    #[test]
    fn picks_wwvh_45_when_between_18_and_45() {
        let now = 1_783_512_000_000 + 20 * 60_000; // 12:20:00Z
        let w = next_window(now);
        assert_eq!(w.station, Station::Wwvh);
    }
    #[test]
    fn rolls_to_next_hour_wwv_after_45() {
        let now = 1_783_512_000_000 + 50 * 60_000; // 12:50:00Z
        let w = next_window(now);
        assert_eq!(w.station, Station::Wwv);
        assert_eq!(w.at_unix_ms, 1_783_512_000_000 + 78 * 60_000); // 13:18
    }
}
