//! APRS position-report parser (RX).
//!
//! Parses uncompressed and Base-91-compressed APRS position reports into
//! lat/lon + symbol. Pinned to aprslib (`parse_aprs` position handling) and
//! direwolf `decode_aprs.c`:
//!   - Uncompressed: `DDMM.mmH` lat (8) + symbol-table (1) + `DDDMM.mmH` lon (9)
//!     + symbol-code (1), with position-ambiguity spaces treated as `0`.
//!   - Compressed: symbol-table (1) + YYYY (4, lat) + XXXX (4, lon) + code (1)
//!     + cs (2) + comp-type (1); base-91 decode per the APRS spec.
//!
//! Handles the four position DTIs: `!` `=` (no timestamp) and `@` `/`
//! (7-char timestamp, skipped). Mic-E (latitude packed into the AX.25
//! destination callsign) is a separate fast-follow, NOT handled here.

/// A parsed APRS position report. Only fields actually present on the wire are
/// reported (RF-honesty: no estimated/derived locations).
#[derive(Debug, Clone, PartialEq)]
pub struct AprsPosition {
    pub lat: f64,
    pub lon: f64,
    pub symbol_table: char,
    pub symbol_code: char,
    pub comment: String,
}

/// Parse an APRS position info field. Returns `None` if it is not a well-formed
/// uncompressed/compressed position report (wrong DTI, too short, malformed
/// coordinates, out-of-range lat/lon).
pub fn parse_position(info: &[u8]) -> Option<AprsPosition> {
    let dti = *info.first()?;
    if !matches!(dti, b'!' | b'=' | b'/' | b'@') {
        return None;
    }
    let mut rest = &info[1..];
    // `@` and `/` carry a 7-char timestamp (DDHHMMz / HHMMSSh / DDHHMMz-local).
    if matches!(dti, b'/' | b'@') {
        if rest.len() < 7 {
            return None;
        }
        rest = &rest[7..];
    }
    // Compressed vs uncompressed: an uncompressed report opens with a latitude
    // digit (or an ambiguity space); a compressed report opens with the
    // symbol-table id. (aprslib heuristic. A compressed report with a numeric
    // overlay table id is the one ambiguous case — rare; deferred.)
    match rest.first()? {
        b'0'..=b'9' | b' ' => parse_uncompressed(rest),
        _ => parse_compressed(rest),
    }
}

fn parse_uncompressed(b: &[u8]) -> Option<AprsPosition> {
    // DDMM.mmH(8) + sym-table(1) + DDDMM.mmH(9) + sym-code(1) = 19 minimum.
    if b.len() < 19 {
        return None;
    }
    let lat = parse_lat(&b[0..8])?;
    let symbol_table = b[8] as char;
    let lon = parse_lon(&b[9..18])?;
    let symbol_code = b[18] as char;
    let comment = String::from_utf8_lossy(&b[19..]).trim_end().to_string();
    sane(lat, lon)?;
    Some(AprsPosition { lat, lon, symbol_table, symbol_code, comment })
}

/// `DDMM.mmH` — 8 bytes. Ambiguity spaces are treated as `0` (aprslib).
fn parse_lat(f: &[u8]) -> Option<f64> {
    if f.len() != 8 || f[4] != b'.' {
        return None;
    }
    let hemi = f[7];
    if hemi != b'N' && hemi != b'S' {
        return None;
    }
    let deg = two_digit(f[0], f[1])?;
    let min = two_digit(f[2], f[3])? as f64 + two_digit(f[5], f[6])? as f64 / 100.0;
    let mut v = deg as f64 + min / 60.0;
    if hemi == b'S' {
        v = -v;
    }
    Some(v)
}

/// `DDDMM.mmH` — 9 bytes. Ambiguity spaces treated as `0`.
fn parse_lon(f: &[u8]) -> Option<f64> {
    if f.len() != 9 || f[5] != b'.' {
        return None;
    }
    let hemi = f[8];
    if hemi != b'E' && hemi != b'W' {
        return None;
    }
    let deg = three_digit(f[0], f[1], f[2])?;
    let min = two_digit(f[3], f[4])? as f64 + two_digit(f[6], f[7])? as f64 / 100.0;
    let mut v = deg as f64 + min / 60.0;
    if hemi == b'W' {
        v = -v;
    }
    Some(v)
}

fn digit(c: u8) -> Option<u32> {
    match c {
        b'0'..=b'9' => Some((c - b'0') as u32),
        b' ' => Some(0), // position ambiguity
        _ => None,
    }
}

fn two_digit(a: u8, b: u8) -> Option<u32> {
    Some(digit(a)? * 10 + digit(b)?)
}

fn three_digit(a: u8, b: u8, c: u8) -> Option<u32> {
    Some(digit(a)? * 100 + digit(b)? * 10 + digit(c)?)
}

fn parse_compressed(b: &[u8]) -> Option<AprsPosition> {
    // sym-table(1) + lat YYYY(4) + lon XXXX(4) + sym-code(1) + cs(2) + ctype(1) = 13.
    if b.len() < 13 {
        return None;
    }
    let symbol_table = b[0] as char;
    let lat = 90.0 - base91(&b[1..5])? as f64 / 380926.0;
    let lon = -180.0 + base91(&b[5..9])? as f64 / 190463.0;
    let symbol_code = b[9] as char;
    // b[10..13] = compressed course/speed (or altitude/range) + comp-type byte;
    // not surfaced in this atomic slice.
    let comment = String::from_utf8_lossy(&b[13..]).trim_end().to_string();
    sane(lat, lon)?;
    Some(AprsPosition { lat, lon, symbol_table, symbol_code, comment })
}

/// Base-91 decode of a printable APRS field (each byte in `!`..=`{`).
fn base91(b: &[u8]) -> Option<i64> {
    let mut v = 0i64;
    for &c in b {
        if !(0x21..=0x7b).contains(&c) {
            return None;
        }
        v = v * 91 + (c as i64 - 33);
    }
    Some(v)
}

fn sane(lat: f64, lon: f64) -> Option<()> {
    if (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon) {
        Some(())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-3, "{a} != {b}");
    }

    #[test]
    fn uncompressed_no_timestamp() {
        let p = parse_position(b"!4903.50N/07201.75W-Hello").unwrap();
        approx(p.lat, 49.058333);
        approx(p.lon, -72.029167);
        assert_eq!(p.symbol_table, '/');
        assert_eq!(p.symbol_code, '-');
        assert_eq!(p.comment, "Hello");
    }

    #[test]
    fn uncompressed_with_messaging_dti() {
        // '=' is position-no-timestamp WITH messaging — same coordinates.
        let p = parse_position(b"=4903.50N/07201.75W-").unwrap();
        approx(p.lat, 49.058333);
        approx(p.lon, -72.029167);
    }

    #[test]
    fn uncompressed_with_timestamp_at() {
        // '@' carries a 7-char timestamp that must be skipped.
        let p = parse_position(b"@092345z4903.50N/07201.75W>test").unwrap();
        approx(p.lat, 49.058333);
        approx(p.lon, -72.029167);
        assert_eq!(p.symbol_code, '>');
        assert_eq!(p.comment, "test");
    }

    #[test]
    fn uncompressed_with_timestamp_slash() {
        let p = parse_position(b"/092345z4903.50N/07201.75W-").unwrap();
        approx(p.lat, 49.058333);
        approx(p.lon, -72.029167);
    }

    #[test]
    fn southern_eastern_hemispheres() {
        let p = parse_position(b"!4903.50S/07201.75E-").unwrap();
        approx(p.lat, -49.058333);
        approx(p.lon, 72.029167);
    }

    #[test]
    fn position_ambiguity_spaces_treated_as_zero() {
        // 1-digit ambiguity in the hundredths place (space => 0).
        let p = parse_position(b"!4903.5 N/07201.7 W-").unwrap();
        approx(p.lat, 49.058333); // 03.50'
        approx(p.lon, -72.028333); // 01.70'
    }

    #[test]
    fn compressed_base91() {
        // aprslib reference: "/5L!!<*e7>" => 49.5, -72.75.
        let p = parse_position(b"!/5L!!<*e7>  T").unwrap();
        approx(p.lat, 49.5);
        approx(p.lon, -72.75);
        assert_eq!(p.symbol_table, '/');
        assert_eq!(p.symbol_code, '>');
    }

    #[test]
    fn rejects_non_position_dti() {
        assert!(parse_position(b":WXBOT    :hi").is_none()); // message DTI
        assert!(parse_position(b"").is_none());
        assert!(parse_position(b">status text").is_none()); // status DTI
    }

    #[test]
    fn rejects_malformed_uncompressed() {
        assert!(parse_position(b"!4903.50X/07201.75W-").is_none()); // bad hemisphere
        assert!(parse_position(b"!4903.50N/07201.75W").is_none()); // missing symbol code (18 < 19)
    }

    #[test]
    fn rejects_out_of_range() {
        // 99 degrees latitude is impossible.
        assert!(parse_position(b"!9903.50N/07201.75W-").is_none());
    }
}
