//! APRS weather (WX) report parser (RX).
//!
//! Decodes the APRS weather data format (APRS101 §12), pinned to the documented
//! field letters, widths, units, and conversions — protocol FACTS, implemented
//! from scratch as idiomatic Rust (not a port of any library).
//!
//! A weather report carries a fixed leading **wind group** `ddd/sss` (direction
//! in degrees `/` sustained speed in mph) followed by single-letter field groups,
//! each a letter plus fixed-width digits:
//!
//! | letter | field | width | unit / conversion |
//! |---|---|---|---|
//! | `g` | wind gust | 3 | mph |
//! | `t` | temperature | 3, or `t-NN` (negative, 2) | °F |
//! | `r` | rain last hour | 3 | 1/100 in → in (÷100) |
//! | `p` | rain last 24h | 3 | 1/100 in → in (÷100) |
//! | `P` | rain since local midnight | 3 | 1/100 in → in (÷100) |
//! | `h` | humidity | 2 | %, `h00` = 100% |
//! | `b` | barometric pressure | 5 | 1/10 hPa → hPa (÷10) |
//! | `l`/`L` | luminosity | 3 | W/m²; `l` adds 1000 (values ≥1000) |
//! | `s` | snowfall last 24h | 3 | inches |
//! | `#` | raw rain counter | 3 | count (not surfaced as a channel) |
//!
//! **The `s` overload trap:** `s` is the SPEED letter inside the leading wind
//! group (`ddd/sNN`-style encoders) AND the SNOW letter later. We consume the
//! fixed `ddd/sss` wind prefix FIRST, so any *subsequent* `sNNN` is unambiguously
//! snowfall — the wind speed can never be stolen by snow, nor snow by wind.
//!
//! RF-honesty: a field absent from the wire is `None`, never a fabricated `0`.
//! `h00` (100% humidity) and a literally-zero reading (`t000` = 0 °F) are both
//! genuine present values and are preserved as `Some(_)`.
//!
//! Engine wiring (tuxlink-wu2x): two carriers feed this module —
//!   1. A **positionless** weather report (DTI `_`, then an 8-char `MDHM`
//!      timestamp, then the WX data) → [`parse_positionless_weather`].
//!   2. A **position** report whose symbol code is the weather `_` → the WX data
//!      lives in the position COMMENT, parsed by [`parse_weather_data`].
//! Both emit the `aprs-weather:new` DTO. The source-reactive panel is the
//! tuxlink-2phz fast-follow.

/// A decoded APRS weather report. Serializes camelCase as `aprs-weather:new`.
///
/// Every measurement is `Option` — only fields actually present on the wire are
/// `Some`. Units are ham-conventional (mph / °F / inches / hPa / W·m⁻²); a metric
/// toggle is a panel concern, deferred.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeatherReport {
    /// Reporting station (callsign-SSID). Filled by the engine seam, not the
    /// pure field parser (which has no callsign in scope).
    pub station: String,
    /// Wind direction the weather is coming FROM, degrees true. `None` when the
    /// sender masked it (`.../`, blank) — unknown, not 0°.
    pub wind_direction_deg: Option<u16>,
    /// Sustained wind speed, mph.
    pub wind_speed_mph: Option<u16>,
    /// Wind gust (`g`), mph.
    pub wind_gust_mph: Option<u16>,
    /// Temperature (`t`), °F. May be negative (`t-NN`).
    pub temperature_f: Option<i16>,
    /// Relative humidity (`h`), %. `h00` decodes to 100.
    pub humidity_pct: Option<u8>,
    /// Barometric pressure (`b`), hPa (wire is 1/10 hPa).
    pub pressure_hpa: Option<f64>,
    /// Rain in the last hour (`r`), inches (wire is 1/100 in).
    pub rain_1h_in: Option<f64>,
    /// Rain in the last 24 hours (`p`), inches.
    pub rain_24h_in: Option<f64>,
    /// Rain since local midnight (`P`), inches.
    pub rain_since_midnight_in: Option<f64>,
    /// Luminosity (`l`/`L`), W/m². `l` values carry +1000 (so `l` is ≥1000 W/m²).
    pub luminosity_wm2: Option<u16>,
    /// Snowfall in the last 24 hours (`s`), inches.
    pub snow_in: Option<f64>,
    /// Free-text comment trailing the parsable WX run (station/equipment type,
    /// e.g. a Davis/`dvs`/`wRSW` software identifier), if any.
    pub comment: String,
}

impl WeatherReport {
    /// An empty report (all measurements absent), ready for the parser to fill.
    fn empty() -> Self {
        WeatherReport {
            station: String::new(),
            wind_direction_deg: None,
            wind_speed_mph: None,
            wind_gust_mph: None,
            temperature_f: None,
            humidity_pct: None,
            pressure_hpa: None,
            rain_1h_in: None,
            rain_24h_in: None,
            rain_since_midnight_in: None,
            luminosity_wm2: None,
            snow_in: None,
            comment: String::new(),
        }
    }

    /// True when no measurement field was decoded — used to reject a body that
    /// merely *looked* like it might be weather (e.g. a `_`-symbol position whose
    /// comment carries no WX letters).
    fn has_no_measurements(&self) -> bool {
        self.wind_direction_deg.is_none()
            && self.wind_speed_mph.is_none()
            && self.wind_gust_mph.is_none()
            && self.temperature_f.is_none()
            && self.humidity_pct.is_none()
            && self.pressure_hpa.is_none()
            && self.rain_1h_in.is_none()
            && self.rain_24h_in.is_none()
            && self.rain_since_midnight_in.is_none()
            && self.luminosity_wm2.is_none()
            && self.snow_in.is_none()
    }
}

/// True when a position report's symbol designates a weather station, i.e. the
/// symbol code is `_`. The table char (`/` primary or `\` alternate, or an
/// overlay digit/letter) does not change the weather designation, so it is
/// accepted but unused.
pub fn is_weather_symbol(_table: char, code: char) -> bool {
    code == '_'
}

/// Parse the WX data run of a weather report body. `body` is the text positioned
/// at the leading wind group (`ddd/sss…`) — i.e. AFTER the DTI + timestamp for a
/// positionless report, or AFTER lat/lon/symbol for a position-embedded report.
///
/// Returns `None` if no weather measurement field is present (so a non-weather
/// comment is not mistaken for a report). On success the [`WeatherReport`] has
/// `station` left blank for the engine to fill, and any trailing non-field text
/// captured in `comment`.
pub fn parse_weather_data(body: &str) -> Option<WeatherReport> {
    let mut wx = WeatherReport::empty();
    let bytes = body.as_bytes();
    let mut i = 0;

    // --- Leading wind group. ----------------------------------------------------
    // APRS encodes the wind two ways depending on the carrier:
    //   * Position-embedded WX uses `ddd/sss` (direction '/' sustained speed) as
    //     the fixed lead-in.
    //   * Positionless WX uses prefixed `c<ddd>` (course/direction) then `s<sss>`
    //     (speed) — the SAME `c`/`s` letters a position report uses for course &
    //     speed. Here the LEADING `s` (immediately following the `c` group) is the
    //     wind SPEED; consuming it now is what frees a later `sNNN` to be snow.
    // Either form may mask the direction (`...`, blank) → None.
    if i + 3 <= bytes.len() && bytes[i + 3] == b'/' {
        // `ddd/sss` form.
        wx.wind_direction_deg = parse_u16_field(&bytes[i..i + 3]);
        i += 4; // skip the 3 direction digits + the '/'
        if i + 3 <= bytes.len() {
            wx.wind_speed_mph = parse_u16_field(&bytes[i..i + 3]);
            i += 3;
        }
    } else if i < bytes.len() && bytes[i] == b'c' && i + 4 <= bytes.len() {
        // `c<ddd>` course/direction form (positionless). Then an immediately
        // following `s<sss>` is the wind SPEED (not snow — that comes later).
        wx.wind_direction_deg = parse_u16_field(&bytes[i + 1..i + 4]);
        i += 4;
        if i + 4 <= bytes.len() && bytes[i] == b's' {
            wx.wind_speed_mph = parse_u16_field(&bytes[i + 1..i + 4]);
            i += 4;
        }
    }

    // --- Field-letter groups. ---------------------------------------------------
    // Each iteration consumes exactly one letter + its fixed-width digits. When a
    // letter's digits don't fit / don't parse, that letter is treated as the start
    // of the trailing comment and parsing stops (APRS101: trailing text is a
    // comment, not a field).
    while i < bytes.len() {
        let letter = bytes[i] as char;
        // Width of the digit field that follows this letter.
        let width = match letter {
            'g' | 't' | 'r' | 'p' | 'P' | 'l' | 'L' | 's' | '#' => 3,
            'h' => 2,
            'b' => 5,
            _ => break, // not a WX field letter → start of comment
        };
        let start = i + 1;
        let end = start + width;

        // Special case: negative temperature `t-NN` (a '-' then 2 digits).
        if letter == 't' && start < bytes.len() && bytes[start] == b'-' {
            let num_start = start + 1;
            let num_end = num_start + 2;
            if num_end <= bytes.len() {
                if let Some(v) = parse_u16_field(&bytes[num_start..num_end]) {
                    wx.temperature_f = Some(-(v as i16));
                    i = num_end;
                    continue;
                }
            }
            break; // malformed `t-` → comment
        }

        if end > bytes.len() {
            break; // not enough digits for this field → it's comment text
        }
        let digits = &bytes[start..end];
        let val = parse_u16_field(digits);

        match letter {
            'g' => wx.wind_gust_mph = val,
            't' => wx.temperature_f = val.map(|v| v as i16),
            'h' => wx.humidity_pct = val.map(|v| if v == 0 { 100 } else { v as u8 }),
            'b' => wx.pressure_hpa = val.map(|v| v as f64 / 10.0),
            'r' => wx.rain_1h_in = val.map(|v| v as f64 / 100.0),
            'p' => wx.rain_24h_in = val.map(|v| v as f64 / 100.0),
            'P' => wx.rain_since_midnight_in = val.map(|v| v as f64 / 100.0),
            'l' => wx.luminosity_wm2 = val.map(|v| v + 1000), // 'l' carries +1000
            'L' => wx.luminosity_wm2 = val,
            's' => wx.snow_in = val.map(|v| v as f64 / 10.0),
            '#' => { /* raw rain counter — consumed but not surfaced as a channel */ }
            _ => unreachable!("width matched above implies a known letter"),
        }
        i = end;
    }

    // Anything left is the trailing comment (station/software identifier, etc.).
    if i < bytes.len() {
        wx.comment = body[i..].trim().to_string();
    }

    if wx.has_no_measurements() {
        return None;
    }
    Some(wx)
}

/// Parse a **positionless** weather report info field. The wire form is DTI `_`
/// then an 8-char `MDHM` timestamp (month-day-hour-minute, e.g. `10090556`) then
/// the WX data run. Strips the DTI + timestamp and delegates to
/// [`parse_weather_data`]. Returns `None` if it is not a positionless WX report
/// (wrong DTI, too short, or no measurement fields).
pub fn parse_positionless_weather(info: &[u8]) -> Option<WeatherReport> {
    let s = std::str::from_utf8(info).ok()?;
    let rest = s.strip_prefix('_')?;
    // 8-char MDHM timestamp; we don't surface it (the panel client-stamps), but
    // it must be skipped before the WX run begins.
    if rest.len() < 8 {
        return None;
    }
    let (_ts, wx_run) = rest.split_at(8);
    parse_weather_data(wx_run)
}

/// Parse a 3-or-2 digit ASCII field into a `u16`. Returns `None` if any byte is a
/// non-digit (masked/blank field, e.g. `...` direction or `  ` spaces) — that
/// absence becomes a `None` measurement (RF-honesty), never a fabricated 0.
fn parse_u16_field(digits: &[u8]) -> Option<u16> {
    if digits.is_empty() || !digits.iter().all(|b| b.is_ascii_digit()) {
        return None;
    }
    std::str::from_utf8(digits).ok()?.parse::<u16>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_full_davis_style_body() {
        // 220/004 g005 t068 r000 p000 P000 h53 b10138
        let wx = parse_weather_data("220/004g005t068r000p000P000h53b10138").unwrap();
        assert_eq!(wx.wind_direction_deg, Some(220));
        assert_eq!(wx.wind_speed_mph, Some(4));
        assert_eq!(wx.wind_gust_mph, Some(5));
        assert_eq!(wx.temperature_f, Some(68));
        assert_eq!(wx.rain_1h_in, Some(0.0));
        assert_eq!(wx.rain_24h_in, Some(0.0));
        assert_eq!(wx.rain_since_midnight_in, Some(0.0));
        assert_eq!(wx.humidity_pct, Some(53));
        assert_eq!(wx.pressure_hpa, Some(1013.8));
        assert_eq!(wx.comment, "");
        // Absent fields stay None (RF-honesty).
        assert_eq!(wx.snow_in, None);
        assert_eq!(wx.luminosity_wm2, None);
    }

    #[test]
    fn negative_temperature_decodes_as_signed() {
        // t-05 → -5 °F. Wind group present so the 't' starts the field run.
        let wx = parse_weather_data("000/000t-05h99").unwrap();
        assert_eq!(wx.temperature_f, Some(-5));
        assert_eq!(wx.humidity_pct, Some(99));
    }

    #[test]
    fn humidity_h00_is_one_hundred_percent() {
        let wx = parse_weather_data("180/010h00").unwrap();
        assert_eq!(wx.humidity_pct, Some(100), "h00 means 100%, not 0%");
    }

    #[test]
    fn humidity_explicit_value_is_preserved() {
        let wx = parse_weather_data("180/010h01").unwrap();
        assert_eq!(wx.humidity_pct, Some(1));
    }

    #[test]
    fn snow_after_wind_is_not_stolen_by_the_speed_letter() {
        // The trap: the wind group already consumed `/004` as SPEED, so the later
        // `s050` MUST parse as 5.0 in of snow — not be mis-read as another speed.
        let wx = parse_weather_data("050/004g010t030s050").unwrap();
        assert_eq!(wx.wind_speed_mph, Some(4), "wind speed comes from the wind group");
        assert_eq!(wx.wind_gust_mph, Some(10));
        assert_eq!(wx.temperature_f, Some(30));
        assert_eq!(wx.snow_in, Some(5.0), "s after the wind group is snowfall");
    }

    #[test]
    fn partial_field_set_leaves_absent_fields_none() {
        // Only wind + temperature present; everything else absent → None.
        let wx = parse_weather_data("270/008t045").unwrap();
        assert_eq!(wx.wind_direction_deg, Some(270));
        assert_eq!(wx.wind_speed_mph, Some(8));
        assert_eq!(wx.temperature_f, Some(45));
        assert_eq!(wx.wind_gust_mph, None);
        assert_eq!(wx.humidity_pct, None);
        assert_eq!(wx.pressure_hpa, None);
        assert_eq!(wx.rain_1h_in, None);
        assert_eq!(wx.snow_in, None);
    }

    #[test]
    fn unknown_wind_direction_is_none_not_zero() {
        // Masked direction `.../` → None; speed still parses.
        let wx = parse_weather_data(".../005g008t072").unwrap();
        assert_eq!(wx.wind_direction_deg, None, "masked direction is unknown, not 0°");
        assert_eq!(wx.wind_speed_mph, Some(5));
        assert_eq!(wx.wind_gust_mph, Some(8));
        assert_eq!(wx.temperature_f, Some(72));
    }

    #[test]
    fn luminosity_lowercase_carries_plus_1000() {
        // l carries +1000 (so `l050` = 1050 W/m²); uppercase L is the literal value.
        let lo = parse_weather_data("000/000t050l050").unwrap();
        assert_eq!(lo.luminosity_wm2, Some(1050));
        let hi = parse_weather_data("000/000t050L850").unwrap();
        assert_eq!(hi.luminosity_wm2, Some(850));
    }

    #[test]
    fn trailing_text_after_the_field_run_is_a_comment() {
        // After the parsable WX run, the equipment/software identifier is a comment.
        let wx = parse_weather_data("000/000t068h50dU2k").unwrap();
        assert_eq!(wx.temperature_f, Some(68));
        assert_eq!(wx.humidity_pct, Some(50));
        assert_eq!(wx.comment, "dU2k", "non-field trailing text is the comment");
    }

    #[test]
    fn positionless_report_strips_dti_and_timestamp() {
        // `_` + MDHM(10090556) + WX run. (Oct 09, 05:56.)
        let wx =
            parse_positionless_weather(b"_10090556c220s004g005t068r000p000P000h53b10138").unwrap();
        // Positionless wind group is `c<dir>` then `s<spd>` (the c/s prefixed form).
        assert_eq!(wx.wind_direction_deg, Some(220));
        assert_eq!(wx.wind_speed_mph, Some(4), "leading s is wind speed, not snow");
        assert_eq!(wx.wind_gust_mph, Some(5));
        assert_eq!(wx.temperature_f, Some(68));
        assert_eq!(wx.rain_1h_in, Some(0.0));
        assert_eq!(wx.humidity_pct, Some(53));
        assert_eq!(wx.pressure_hpa, Some(1013.8));
        assert_eq!(wx.snow_in, None, "no later s field → snow stays absent");
    }

    #[test]
    fn positionless_c_s_form_disambiguates_a_later_snow_field() {
        // The hard case: c<dir>, leading s<spd> = WIND SPEED, then a SECOND s = SNOW.
        // `_` ts(8) c180 s006 g012 t028 s100 → dir 180, spd 6, gust 12, 28°F, 10.0in snow.
        let wx = parse_positionless_weather(b"_01011200c180s006g012t028s100").unwrap();
        assert_eq!(wx.wind_direction_deg, Some(180));
        assert_eq!(wx.wind_speed_mph, Some(6), "first s (after c) is wind speed");
        assert_eq!(wx.wind_gust_mph, Some(12));
        assert_eq!(wx.temperature_f, Some(28));
        assert_eq!(wx.snow_in, Some(10.0), "the SECOND s is snowfall");
    }

    #[test]
    fn position_embedded_weather_in_comment() {
        // A `_`-symbol position report carries its WX run in the COMMENT (after
        // lat/lon/symbol). The engine extracts the comment and calls this; the
        // comment text begins at the `ddd/sss` wind group.
        let wx = parse_weather_data("180/010g015t055r000p000P000h68b09900wDVS").unwrap();
        assert_eq!(wx.wind_direction_deg, Some(180));
        assert_eq!(wx.wind_speed_mph, Some(10));
        assert_eq!(wx.wind_gust_mph, Some(15));
        assert_eq!(wx.temperature_f, Some(55));
        assert_eq!(wx.humidity_pct, Some(68));
        assert_eq!(wx.pressure_hpa, Some(990.0));
        assert_eq!(wx.comment, "wDVS");
    }

    #[test]
    fn positionless_too_short_is_none() {
        // DTI present but fewer than 8 timestamp chars → not a valid report.
        assert!(parse_positionless_weather(b"_1009").is_none());
    }

    #[test]
    fn non_weather_body_returns_none() {
        // No wind group, no WX letters → not a weather report.
        assert!(parse_weather_data("just a comment").is_none());
        // A `_`-symbol position whose comment is plain text must not be coerced.
        assert!(parse_weather_data("hello from the field").is_none());
    }

    #[test]
    fn is_weather_symbol_matches_underscore_code() {
        assert!(is_weather_symbol('/', '_'));
        assert!(is_weather_symbol('\\', '_'));
        assert!(!is_weather_symbol('/', '>'));
        assert!(!is_weather_symbol('/', '-'));
    }

    #[test]
    fn zero_temperature_is_a_real_value_not_absence() {
        // t000 = 0 °F is a genuine reading; it must be Some(0), distinct from a
        // missing temperature field.
        let wx = parse_weather_data("000/000t000").unwrap();
        assert_eq!(wx.temperature_f, Some(0));
    }

    #[test]
    fn rain_hundredths_convert_to_inches() {
        // r123 = 1.23 in last hour; p045 = 0.45 in last 24h; P200 = 2.00 in.
        let wx = parse_weather_data("000/000r123p045P200").unwrap();
        assert_eq!(wx.rain_1h_in, Some(1.23));
        assert_eq!(wx.rain_24h_in, Some(0.45));
        assert_eq!(wx.rain_since_midnight_in, Some(2.0));
    }
}
