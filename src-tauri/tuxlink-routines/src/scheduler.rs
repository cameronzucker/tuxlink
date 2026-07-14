//! Schedule math (spec §8): pure functions over unix seconds. The engine
//! facade owns the tick loop; everything here is testable at any timestamp.

use std::time::Duration;

use crate::types::Trigger;

/// Alignment target shared by `Control::Delay`'s `next:hour` / `next:day`
/// (executor.rs) and `Trigger::Schedule`'s `align` field (this module).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Hour,
    Day,
}

/// Duration until the next hour/day boundary from `now_unix` (UTC, epoch
/// seconds).
pub fn duration_to_next_align(now_unix: i64, align: Align) -> Duration {
    let modulus: i64 = match align {
        Align::Hour => 3600,
        Align::Day => 86_400,
    };
    Duration::from_secs((modulus - now_unix.rem_euclid(modulus)) as u64)
}

/// Parse "30m" / "2h" / "45s" to seconds.
pub fn every_seconds(every: &str) -> Option<i64> {
    let (num, unit) = every.split_at(every.len().checked_sub(1)?);
    let n: i64 = num.parse().ok()?;
    match unit {
        "s" => Some(n),
        "m" => Some(n * 60),
        "h" => Some(n * 3600),
        _ => None,
    }
}

/// Parse "HH:MM-HH:MM" into (start_minutes, end_minutes) since midnight.
/// `None` on any malformed input: missing separator, non-numeric fields, or
/// an out-of-range hour (0-23) or minute (0-59). Callers MUST fail closed on
/// `None` — this string gates when the operator's radio is allowed to
/// transmit, and a config typo must not silently open the gate.
fn parse_window(window: &str) -> Option<(i64, i64)> {
    fn minutes(part: &str) -> Option<i64> {
        let (h, m) = part.split_once(':')?;
        let h: i64 = h.parse().ok()?;
        let m: i64 = m.parse().ok()?;
        if !(0..24).contains(&h) || !(0..60).contains(&m) {
            return None;
        }
        Some(h * 60 + m)
    }
    let (start_s, end_s) = window.split_once('-')?;
    Some((minutes(start_s)?, minutes(end_s)?))
}

/// Whether `start_min..end_min` (minutes since midnight, end-exclusive)
/// contains `now_unix`'s LOCAL minute-of-day. `start > end` is an overnight
/// window that wraps midnight.
///
/// `utc_offset_seconds` is `local - utc`, the same convention
/// `chrono::FixedOffset::local_minus_utc` uses (production threads
/// `chrono::Local::now().offset().local_minus_utc()` in from the app layer —
/// this leaf crate stays chrono-free and just takes the plain `i32`). Adding
/// it to `now_unix` before taking the minute-of-day is what makes the
/// "local" in this fn's name true instead of aspirational: a bare
/// `now_unix.rem_euclid(86_400)` is UTC, and a quiet-hours window authored
/// in the operator's own clock (Arizona, UTC-7 year-round) would gate 7
/// hours off from what the operator configured.
fn window_contains(start_min: i64, end_min: i64, now_unix: i64, utc_offset_seconds: i32) -> bool {
    let local = now_unix + utc_offset_seconds as i64;
    let now_min = (local.rem_euclid(86_400)) / 60;
    if start_min <= end_min {
        now_min >= start_min && now_min < end_min
    } else {
        now_min >= start_min || now_min < end_min
    }
}

/// "HH:MM-HH:MM"; overnight windows (start > end) wrap midnight. Fails
/// closed (`false`) on a malformed window string rather than treating it as
/// wide-open — see `parse_window`. `utc_offset_seconds` (`local - utc`) is
/// applied before gating — see [`window_contains`].
pub fn within_window(window: &str, now_unix: i64, utc_offset_seconds: i32) -> bool {
    match parse_window(window) {
        Some((start, end)) => window_contains(start, end, now_unix, utc_offset_seconds),
        None => false,
    }
}

/// The next unix timestamp this trigger fires at, strictly after `now_unix`.
/// `None` for manual triggers, for an unparseable `every`, or for a present
/// but unparseable `window` (fail closed: a config typo stalls the routine
/// visibly on the dashboard rather than silently disabling the operator's
/// quiet-hours TX gate).
///
/// Aligned (`align: hour`/`day`) schedules fire on the epoch-anchored
/// interval grid; for intervals dividing the alignment period (e.g. every
/// 15/20/30m against `hour`) this coincides with top-of-hour/day
/// boundaries, since the Unix epoch itself falls on an hour/day boundary.
/// For intervals that do NOT divide the alignment period (e.g. every 45m
/// against `hour`), epoch anchoring keeps the cadence constant; anchoring to
/// the hour/day *containing* `now` instead would drift the cadence on every
/// call (each call re-bases from a new `now`, producing an unstable
/// schedule).
///
/// The alignment grid itself is UTC-anchored (unaffected by
/// `utc_offset_seconds`) — only a configured `window` is interpreted in
/// LOCAL time (`local = now_unix + utc_offset_seconds`, `chrono`'s
/// `local - utc` convention). That is deliberate: the grid's whole point is
/// a stable, offset-independent cadence (see above), while a `window` is an
/// operator-authored quiet-hours string ("22:00-06:00") that means the
/// operator's OWN clock, not UTC.
pub fn next_fire(trigger: &Trigger, now_unix: i64, utc_offset_seconds: i32) -> Option<i64> {
    let Trigger::Schedule {
        every,
        align,
        window,
        ..
    } = trigger
    else {
        return None;
    };
    let interval = every_seconds(every)?;
    if interval <= 0 {
        return None;
    }
    // A configured window must parse before we compute anything from it;
    // unparseable means "no next fire" rather than "unrestricted".
    let parsed_window = match window {
        Some(w) => Some(parse_window(w)?),
        None => None,
    };
    let mut candidate = match align.as_deref() {
        Some("hour") | Some("day") => {
            // Smallest multiple of `interval` (from the Unix epoch) that is
            // strictly greater than `now_unix`.
            (now_unix.div_euclid(interval) + 1) * interval
        }
        _ => now_unix + interval,
    };
    if let Some((start, end)) = parsed_window {
        // Advance to the window's next opening instant when the grid
        // candidate falls outside it (locked semantics: jump to the
        // window's start, not the next on-grid instant inside it).
        let mut guard = 0;
        while !window_contains(start, end, candidate, utc_offset_seconds) {
            // The window's "today" is the LOCAL calendar day the candidate
            // falls on — shift to local before finding the day boundary,
            // then shift the computed opening instant back to UTC (the unit
            // every other timestamp in this module, and every caller, is
            // in).
            let local = candidate + utc_offset_seconds as i64;
            let day_base_local = local - local.rem_euclid(86_400);
            let today_open_local = day_base_local + start * 60;
            let today_open = today_open_local - utc_offset_seconds as i64;
            candidate = if today_open > candidate {
                today_open
            } else {
                today_open + 86_400
            };
            guard += 1;
            if guard > 3 {
                return None; // window can never admit this schedule
            }
        }
    }
    Some(candidate)
}

/// Whole fires elapsed between `last_seen_unix` and `now_unix` (spec §8
/// missed-fire record). Manual triggers never miss.
pub fn missed_fires(trigger: &Trigger, last_seen_unix: i64, now_unix: i64) -> u64 {
    let Trigger::Schedule { every, .. } = trigger else {
        return 0;
    };
    let Some(interval) = every_seconds(every) else {
        return 0;
    };
    if interval <= 0 || now_unix <= last_seen_unix {
        return 0;
    }
    ((now_unix - last_seen_unix) / interval) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{IfMissed, Trigger};

    fn sched(every: &str, align: Option<&str>, window: Option<&str>) -> Trigger {
        Trigger::Schedule {
            every: every.into(),
            align: align.map(String::from),
            window: window.map(String::from),
            if_missed: IfMissed::Skip,
        }
    }

    // NOW's calendar date is informational only; the tests below derive all
    // expectations arithmetically from NOW itself, per Task 9 binding
    // resolution #2 — the wall-clock reading isn't load-bearing.
    const NOW: i64 = 1_784_124_420;

    #[test]
    fn unaligned_interval_fires_interval_after_now() {
        assert_eq!(
            next_fire(&sched("30m", None, None), NOW, 0),
            Some(NOW + 1800)
        );
    }

    #[test]
    fn hour_aligned_interval_fires_at_the_next_boundary() {
        // 14:07 with every=30m align=hour → next boundary on the 30m grid
        // anchored at the top of the hour: 14:30.
        let at_1430 = NOW - (NOW % 3600) + 1800;
        assert_eq!(
            next_fire(&sched("30m", Some("hour"), None), NOW, 0),
            Some(at_1430)
        );
    }

    #[test]
    fn manual_trigger_never_fires() {
        assert_eq!(next_fire(&Trigger::Manual, NOW, 0), None);
    }

    #[test]
    fn window_gates_fires() {
        assert!(within_window("06:00-22:00", NOW, 0)); // 14:07 is inside
        assert!(!within_window("22:00-06:00", NOW, 0)); // overnight window, 14:07 outside
                                                          // 03:00 UTC same day:
        let three_am = NOW - (NOW % 86_400) + 3 * 3600;
        assert!(within_window("22:00-06:00", three_am, 0)); // overnight window wraps
        assert!(!within_window("06:00-22:00", three_am, 0));
    }

    /// Task 6c (spec §8's quiet-hours window is authored in the OPERATOR'S
    /// clock, not UTC): the offset parameter must actually change the
    /// answer, not just compile. 06:00 UTC reads as 23:00 LOCAL at UTC-7
    /// (Arizona, no DST) — inside the overnight "22:00-06:00" window — while
    /// the SAME unix instant at offset 0 reads as exactly 06:00, the
    /// window's own (exclusive) end boundary, so it is outside.
    #[test]
    fn utc_offset_shifts_which_local_minute_the_window_gates() {
        let day_base = NOW - (NOW % 86_400);
        let six_am_utc = day_base + 6 * 3600;
        let arizona_offset = -7 * 3600;

        assert!(
            within_window("22:00-06:00", six_am_utc, arizona_offset),
            "06:00 UTC is 23:00 local at UTC-7 — inside the overnight window"
        );
        assert!(
            !within_window("22:00-06:00", six_am_utc, 0),
            "the SAME unix instant reads as exactly 06:00 at offset 0 — outside \
             the window (end-exclusive) — the offset parameter must be load-bearing"
        );
    }

    #[test]
    fn next_fire_outside_window_advances_into_it() {
        let three_am = NOW - (NOW % 86_400) + 3 * 3600;
        let fire = next_fire(
            &sched("30m", Some("hour"), Some("06:00-22:00")),
            three_am,
            0,
        )
        .unwrap();
        // First on-grid instant inside the window: 06:00.
        assert_eq!(fire, three_am - 3 * 3600 + 6 * 3600);
    }

    /// Same shape as `next_fire_outside_window_advances_into_it`, but under a
    /// non-zero offset: the window-advance math must land on LOCAL window
    /// open, not UTC window open. UTC 09:00 is LOCAL 02:00 at UTC-7 —
    /// outside "06:00-22:00" local — so the next fire must land at LOCAL
    /// 06:00 (UTC 13:00), not UTC 06:00 (what the old UTC-only math would
    /// have produced, and not the trivial "already inside" answer offset 0
    /// gives for the same anchor).
    #[test]
    fn next_fire_advances_to_local_window_open_under_a_nonzero_offset() {
        let day_base = NOW - (NOW % 86_400);
        let nine_am_utc = day_base + 9 * 3600;
        let arizona_offset = -7 * 3600;

        let fire = next_fire(
            &sched("30m", Some("hour"), Some("06:00-22:00")),
            nine_am_utc,
            arizona_offset,
        )
        .unwrap();
        assert_eq!(
            fire,
            day_base + 13 * 3600,
            "expected LOCAL 06:00 (UTC 13:00 at offset -7h), got unix {fire}"
        );

        // At offset 0 the SAME UTC anchor (09:00) is already inside
        // "06:00-22:00", so next_fire takes the ordinary aligned-grid path
        // instead of advancing to a window open — proving the offset
        // parameter, not incidental arithmetic, produced the result above.
        let fire_utc0 = next_fire(
            &sched("30m", Some("hour"), Some("06:00-22:00")),
            nine_am_utc,
            0,
        )
        .unwrap();
        assert_ne!(
            fire_utc0, fire,
            "offset -7h and offset 0 must take different paths from the same anchor"
        );
    }

    #[test]
    fn aligned_grid_is_stable_for_non_divisor_intervals() {
        // every=45m does not evenly divide align=hour (3600s). Anchoring the
        // grid to the epoch (rather than to the hour *containing* `now`)
        // keeps the cadence constant across repeated calls instead of
        // drifting (previously traced: 14:50 → 15:30 → 15:45 → 16:30).
        let trigger = sched("45m", Some("hour"), None);
        let fire1 = next_fire(&trigger, NOW, 0).unwrap();
        let fire2 = next_fire(&trigger, fire1, 0).unwrap();
        let fire3 = next_fire(&trigger, fire2, 0).unwrap();
        assert_eq!(fire2 - fire1, 2700);
        assert_eq!(fire3 - fire2, 2700);
    }

    #[test]
    fn aligned_divisor_intervals_still_hit_hour_boundaries() {
        // every=30m divides align=hour evenly, so the epoch-anchored grid
        // coincides with top-of-hour boundaries (the epoch itself is
        // hour-aligned): 14:07 → 14:30.
        let at_1430 = NOW - (NOW % 3600) + 1800;
        assert_eq!(
            next_fire(&sched("30m", Some("hour"), None), NOW, 0),
            Some(at_1430)
        );
    }

    #[test]
    fn malformed_window_is_fail_closed() {
        // An unparseable window must never fail open — that would silently
        // disable the operator's quiet-hours TX gate on a config typo.
        assert!(!within_window("garbage", NOW, 0));
        assert_eq!(
            next_fire(&sched("30m", None, Some("25:99-xx")), NOW, 0),
            None
        );
    }

    #[test]
    fn missed_fires_counts_elapsed_slots() {
        // App closed for 2h05m with a 30m unaligned schedule → 4 whole
        // intervals elapsed.
        let last = NOW - (2 * 3600 + 5 * 60);
        assert_eq!(missed_fires(&sched("30m", None, None), last, NOW), 4);
        assert_eq!(missed_fires(&Trigger::Manual, last, NOW), 0);
    }

    #[test]
    fn align_helpers() {
        assert_eq!(
            duration_to_next_align(NOW, Align::Hour).as_secs(),
            3600 - (NOW % 3600) as u64
        );
        assert_eq!(
            duration_to_next_align(NOW, Align::Day).as_secs(),
            86_400 - (NOW % 86_400) as u64
        );
    }
}
