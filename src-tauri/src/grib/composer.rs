//! Saildocs GRIB-request composer. Builds the body string per
//! https://saildocs.com/gribinfo and routes through `compose_message`.
//!
//! Defaults (from canonical Saildocs docs):
//! - Grid spacing: 2°×2°
//! - Forecast hours (VTs): 24,48,72
//! - Parameters: PRESS,WIND
//! - Model: gfs (only model exposed in v0.x; Saildocs has others)
//!
//! The composer is deterministic — given a `GribRequest`, the body bytes
//! are always identical (useful for golden testing + reproducible request
//! audit trails).

use crate::winlink::compose::compose_message;
use crate::winlink::message::Message;
use serde::{Deserialize, Serialize};

pub const SAILDOCS_RECIPIENT: &str = "query@saildocs.com";

/// One-shot (`send`) vs recurring (`sub`) request mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GribMode {
    /// One-time request — Saildocs replies once.
    Send,
    /// Recurring subscription — Saildocs delivers on a schedule.
    Sub,
}

/// Compass direction for a latitude or longitude bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum GribDirection {
    N,
    S,
    E,
    W,
}

/// Bounded latitude in whole degrees + N/S. 0 ≤ degrees ≤ 90.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Latitude {
    pub degrees: u8,
    pub dir: GribDirection, // must be N or S
}

/// Bounded longitude in whole degrees + E/W. 0 ≤ degrees ≤ 180.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Longitude {
    pub degrees: u16,
    pub dir: GribDirection, // must be E or W
}

/// A single forecast time. Either a literal hour count (`24`) or a range
/// with step (`6..96` → 6, 12, …, 96 at default step or via leading range).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ForecastTime {
    /// `24`
    Hour(u32),
    /// `6,12..96` → emitted as `6,12..96` (range notation; Saildocs expands)
    Range { start: u32, end: u32 },
}

/// Weather parameter axes. Subset selected becomes the `Params` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum GribParameter {
    Prmsl,
    Wind,
    Hgt,
    Seatmp,
    Airtmp,
    Waves,
}

impl GribParameter {
    fn as_str(self) -> &'static str {
        match self {
            GribParameter::Prmsl => "PRMSL",
            GribParameter::Wind => "WIND",
            GribParameter::Hgt => "HGT",
            GribParameter::Seatmp => "SEATMP",
            GribParameter::Airtmp => "AIRTMP",
            GribParameter::Waves => "WAVES",
        }
    }
}

/// A fully-specified GRIB request. The model is hardcoded to `gfs` in
/// v0.x (Saildocs offers more, deferred to a future iteration).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GribRequest {
    pub mode: GribMode,
    /// First latitude bound (e.g. 40N).
    pub lat0: Latitude,
    /// Second latitude bound (e.g. 60N).
    pub lat1: Latitude,
    /// First longitude bound (e.g. 140W).
    pub lon0: Longitude,
    /// Second longitude bound (e.g. 120W).
    pub lon1: Longitude,
    /// Grid spacing in degrees, (dlat, dlon). Saildocs default 2,2.
    pub grid: (u8, u8),
    /// Forecast hours. Saildocs default [24,48,72]. Empty → emit Saildocs
    /// default by omitting the field entirely (rely on server default).
    pub times: Vec<ForecastTime>,
    /// Selected parameter axes. Empty → emit Saildocs default
    /// ([PRESS, WIND]) by omitting the field.
    pub params: Vec<GribParameter>,
    /// For `Sub` mode: subscription length in days (Saildocs default
    /// behavior if not set). Ignored when mode == Send.
    pub sub_days: Option<u32>,
    /// For `Sub` mode: daily delivery time in `HH:MM` UTC. Optional.
    pub sub_time: Option<String>,
    /// Subject line — operator-editable (default "GRIB request" suggested
    /// by the UI). Saildocs ignores the subject; it's only meaningful in
    /// the operator's outbox/sent listing.
    pub subject: String,
}

#[derive(Debug, thiserror::Error)]
pub enum GribComposeError {
    #[error("latitude direction must be N or S (got {0:?})")]
    LatitudeDirection(GribDirection),
    #[error("longitude direction must be E or W (got {0:?})")]
    LongitudeDirection(GribDirection),
    #[error("latitude {0} out of range [0, 90]")]
    LatitudeOutOfRange(u8),
    #[error("longitude {0} out of range [0, 180]")]
    LongitudeOutOfRange(u16),
    #[error("grid spacing must be > 0 degrees (got {0}, {1})")]
    GridSpacingZero(u8, u8),
    #[error("at least one of lat0/lat1 must differ from the other")]
    DegenerateLatRange,
    #[error("at least one of lon0/lon1 must differ from the other")]
    DegenerateLonRange,
    #[error("subject is empty (operator must supply or use the default)")]
    EmptySubject,
    #[error("sub_time {0:?} is not in HH:MM format")]
    BadSubTime(String),
}

fn fmt_lat(l: Latitude) -> Result<String, GribComposeError> {
    if !matches!(l.dir, GribDirection::N | GribDirection::S) {
        return Err(GribComposeError::LatitudeDirection(l.dir));
    }
    if l.degrees > 90 {
        return Err(GribComposeError::LatitudeOutOfRange(l.degrees));
    }
    Ok(format!("{}{}", l.degrees, dir_letter(l.dir)))
}

fn fmt_lon(l: Longitude) -> Result<String, GribComposeError> {
    if !matches!(l.dir, GribDirection::E | GribDirection::W) {
        return Err(GribComposeError::LongitudeDirection(l.dir));
    }
    if l.degrees > 180 {
        return Err(GribComposeError::LongitudeOutOfRange(l.degrees));
    }
    Ok(format!("{}{}", l.degrees, dir_letter(l.dir)))
}

fn dir_letter(d: GribDirection) -> &'static str {
    match d {
        GribDirection::N => "N",
        GribDirection::S => "S",
        GribDirection::E => "E",
        GribDirection::W => "W",
    }
}

fn fmt_time(t: &ForecastTime) -> String {
    match t {
        ForecastTime::Hour(h) => h.to_string(),
        ForecastTime::Range { start, end } => format!("{}..{}", start, end),
    }
}

/// Build the Saildocs request body string. Pure / deterministic. Does NOT
/// add a trailing newline (Saildocs accepts either; the compose layer adds
/// its own line endings).
pub fn build_grib_body(req: &GribRequest) -> Result<String, GribComposeError> {
    if req.lat0 == req.lat1 {
        return Err(GribComposeError::DegenerateLatRange);
    }
    if req.lon0 == req.lon1 {
        return Err(GribComposeError::DegenerateLonRange);
    }
    if req.grid.0 == 0 || req.grid.1 == 0 {
        return Err(GribComposeError::GridSpacingZero(req.grid.0, req.grid.1));
    }
    if let Some(ref t) = req.sub_time {
        if !is_valid_hhmm(t) {
            return Err(GribComposeError::BadSubTime(t.clone()));
        }
    }

    let lat0 = fmt_lat(req.lat0)?;
    let lat1 = fmt_lat(req.lat1)?;
    let lon0 = fmt_lon(req.lon0)?;
    let lon1 = fmt_lon(req.lon1)?;

    let verb = match req.mode {
        GribMode::Send => "send",
        GribMode::Sub => "sub",
    };

    // Region is mandatory: `lat0,lat1,lon0,lon1`.
    let region = format!("{lat0},{lat1},{lon0},{lon1}");

    // Pipe-delimited optional fields. Saildocs accepts either omitting
    // trailing fields or supplying them; we emit only what's set, in
    // order: |grid|times|params. (Omitted = server default.)
    let grid = format!("{},{}", req.grid.0, req.grid.1);
    let times = if req.times.is_empty() {
        String::new()
    } else {
        req.times.iter().map(fmt_time).collect::<Vec<_>>().join(",")
    };
    let params = if req.params.is_empty() {
        String::new()
    } else {
        req.params.iter().map(|p| p.as_str()).collect::<Vec<_>>().join(",")
    };

    // Build minimally — drop empty trailing segments to match canonical
    // Saildocs example "send gfs:40N,60N,140W,120W" (no |grid|... when
    // everything is default).
    let mut body = format!("{verb} gfs:{region}");
    let has_grid_override = req.grid != (2, 2);
    let has_times = !times.is_empty();
    let has_params = !params.is_empty();
    if has_grid_override || has_times || has_params {
        body.push('|');
        if has_grid_override {
            body.push_str(&grid);
        } else {
            body.push_str("2,2");
        }
        if has_times || has_params {
            body.push('|');
            body.push_str(&times);
            if has_params {
                body.push('|');
                body.push_str(&params);
            }
        }
    }

    // Subscription-mode trailing fields: ` days=N time=HH:MM`. Saildocs
    // documents these as space-separated suffixes on the `sub` form.
    if matches!(req.mode, GribMode::Sub) {
        if let Some(days) = req.sub_days {
            body.push_str(&format!(" days={days}"));
        }
        if let Some(ref t) = req.sub_time {
            body.push_str(&format!(" time={t}"));
        }
    }

    Ok(body)
}

/// Compose a Saildocs GRIB-request `Message` ready to drop into the outbox.
pub fn compose_grib_message(
    mycall: &str,
    req: &GribRequest,
    unix_secs: u64,
) -> Result<Message, GribComposeError> {
    if req.subject.trim().is_empty() {
        return Err(GribComposeError::EmptySubject);
    }
    let body = build_grib_body(req)?;
    Ok(compose_message(
        mycall,
        &[SAILDOCS_RECIPIENT],
        &[],
        &req.subject,
        &body,
        unix_secs,
    ))
}

fn is_valid_hhmm(s: &str) -> bool {
    // Accept HH:MM with 1-2 digit hour, 2 digit minute. 0-23 / 0-59.
    let Some((h, m)) = s.split_once(':') else {
        return false;
    };
    let Ok(hh) = h.parse::<u32>() else { return false };
    let Ok(mm) = m.parse::<u32>() else { return false };
    if h.is_empty() || h.len() > 2 || m.len() != 2 {
        return false;
    }
    hh <= 23 && mm <= 59
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(d: u8) -> Latitude {
        Latitude { degrees: d, dir: GribDirection::N }
    }
    fn s(d: u8) -> Latitude {
        Latitude { degrees: d, dir: GribDirection::S }
    }
    fn e(d: u16) -> Longitude {
        Longitude { degrees: d, dir: GribDirection::E }
    }
    fn w(d: u16) -> Longitude {
        Longitude { degrees: d, dir: GribDirection::W }
    }

    fn defaults() -> GribRequest {
        GribRequest {
            mode: GribMode::Send,
            lat0: n(40),
            lat1: n(60),
            lon0: w(140),
            lon1: w(120),
            grid: (2, 2),
            times: vec![],
            params: vec![],
            sub_days: None,
            sub_time: None,
            subject: "GRIB request".to_string(),
        }
    }

    /// Canonical Saildocs example: "send gfs:40N,60N,140W,120W" — all
    /// defaults, no pipe fields.
    #[test]
    fn canonical_saildocs_minimal_send() {
        let body = build_grib_body(&defaults()).unwrap();
        assert_eq!(body, "send gfs:40N,60N,140W,120W");
    }

    /// Saildocs expanded form per their docs: with explicit grid + times
    /// + params, all pipe-separated.
    #[test]
    fn canonical_saildocs_expanded_send() {
        let mut req = defaults();
        // Force explicit grid (even at default value) by supplying times/params
        req.times = vec![ForecastTime::Hour(24), ForecastTime::Hour(48), ForecastTime::Hour(72)];
        req.params = vec![GribParameter::Prmsl, GribParameter::Wind];
        let body = build_grib_body(&req).unwrap();
        // Default grid "2,2" emits because times/params are present.
        assert_eq!(body, "send gfs:40N,60N,140W,120W|2,2|24,48,72|PRMSL,WIND");
    }

    #[test]
    fn explicit_grid_override_emits() {
        let mut req = defaults();
        req.grid = (1, 1);
        let body = build_grib_body(&req).unwrap();
        assert_eq!(body, "send gfs:40N,60N,140W,120W|1,1");
    }

    #[test]
    fn range_forecast_time_uses_dotdot_syntax() {
        let mut req = defaults();
        req.times = vec![ForecastTime::Hour(6), ForecastTime::Range { start: 12, end: 96 }];
        let body = build_grib_body(&req).unwrap();
        assert_eq!(body, "send gfs:40N,60N,140W,120W|2,2|6,12..96");
    }

    #[test]
    fn sub_mode_with_days_and_time() {
        let mut req = defaults();
        req.mode = GribMode::Sub;
        req.sub_days = Some(30);
        req.sub_time = Some("18:00".to_string());
        let body = build_grib_body(&req).unwrap();
        assert_eq!(body, "sub gfs:40N,60N,140W,120W days=30 time=18:00");
    }

    #[test]
    fn sub_without_optional_fields_is_just_the_verb_swap() {
        let mut req = defaults();
        req.mode = GribMode::Sub;
        let body = build_grib_body(&req).unwrap();
        assert_eq!(body, "sub gfs:40N,60N,140W,120W");
    }

    #[test]
    fn southern_and_eastern_hemispheres_format_correctly() {
        let req = GribRequest {
            lat0: s(40),
            lat1: s(20),
            lon0: e(150),
            lon1: e(170),
            ..defaults()
        };
        assert_eq!(build_grib_body(&req).unwrap(), "send gfs:40S,20S,150E,170E");
    }

    #[test]
    fn latitude_with_wrong_direction_fails() {
        let req = GribRequest {
            lat0: Latitude { degrees: 40, dir: GribDirection::E },
            ..defaults()
        };
        assert!(matches!(build_grib_body(&req), Err(GribComposeError::LatitudeDirection(_))));
    }

    #[test]
    fn longitude_with_wrong_direction_fails() {
        let req = GribRequest {
            lon0: Longitude { degrees: 140, dir: GribDirection::N },
            ..defaults()
        };
        assert!(matches!(build_grib_body(&req), Err(GribComposeError::LongitudeDirection(_))));
    }

    #[test]
    fn out_of_range_latitude_fails() {
        let req = GribRequest { lat0: n(91), ..defaults() };
        assert!(matches!(build_grib_body(&req), Err(GribComposeError::LatitudeOutOfRange(91))));
    }

    #[test]
    fn out_of_range_longitude_fails() {
        let req = GribRequest { lon0: w(181), ..defaults() };
        assert!(matches!(build_grib_body(&req), Err(GribComposeError::LongitudeOutOfRange(181))));
    }

    #[test]
    fn degenerate_lat_range_fails() {
        let req = GribRequest { lat0: n(40), lat1: n(40), ..defaults() };
        assert!(matches!(build_grib_body(&req), Err(GribComposeError::DegenerateLatRange)));
    }

    #[test]
    fn degenerate_lon_range_fails() {
        let req = GribRequest { lon0: w(140), lon1: w(140), ..defaults() };
        assert!(matches!(build_grib_body(&req), Err(GribComposeError::DegenerateLonRange)));
    }

    #[test]
    fn zero_grid_spacing_fails() {
        let req = GribRequest { grid: (0, 2), ..defaults() };
        assert!(matches!(build_grib_body(&req), Err(GribComposeError::GridSpacingZero(0, 2))));
    }

    #[test]
    fn bad_sub_time_fails() {
        let mut req = defaults();
        req.mode = GribMode::Sub;
        req.sub_time = Some("25:00".to_string()); // invalid hour
        assert!(matches!(build_grib_body(&req), Err(GribComposeError::BadSubTime(_))));
        req.sub_time = Some("noon".to_string()); // not HH:MM
        assert!(matches!(build_grib_body(&req), Err(GribComposeError::BadSubTime(_))));
        req.sub_time = Some("18:0".to_string()); // minute too short
        assert!(matches!(build_grib_body(&req), Err(GribComposeError::BadSubTime(_))));
    }

    #[test]
    fn valid_hhmm_examples() {
        for ok in ["0:00", "00:00", "9:30", "18:00", "23:59"] {
            assert!(is_valid_hhmm(ok), "expected {ok:?} to validate");
        }
        for bad in ["", "noon", "24:00", "12:60", "1:2", "12-30", "1:230"] {
            assert!(!is_valid_hhmm(bad), "expected {bad:?} to fail");
        }
    }

    #[test]
    fn compose_sets_canonical_headers() {
        let req = defaults();
        let msg = compose_grib_message("N7CPZ", &req, 1_716_200_000).unwrap();
        // To: query@saildocs.com is an external (non-winlink.org) SMTP
        // address, so compose layer prefixes "SMTP:".
        let tos = msg.header_all("To");
        assert_eq!(tos, vec!["SMTP:query@saildocs.com"]);
        assert_eq!(msg.header("Subject").unwrap(), "GRIB request");
        let body = std::str::from_utf8(msg.body()).unwrap();
        assert!(body.contains("send gfs:40N,60N,140W,120W"));
    }

    #[test]
    fn compose_with_empty_subject_fails_loudly() {
        let mut req = defaults();
        req.subject = "   ".to_string();
        assert!(matches!(
            compose_grib_message("N7CPZ", &req, 1_716_200_000),
            Err(GribComposeError::EmptySubject)
        ));
    }
}
