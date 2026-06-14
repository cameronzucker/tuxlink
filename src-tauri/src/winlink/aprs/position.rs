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
    /// APRS position-ambiguity level (0–4): how many least-significant minute
    /// digits the sender masked. `0` = full precision; higher = a coarser fix.
    /// RF-honesty: the wire reports a *region*, not a point, when this is > 0, so
    /// the parser surfaces the level rather than silently collapsing the masked
    /// digits to a false-exact coordinate. (Uncompressed: count of masked minute
    /// digits; Mic-E: the destination-encoded ambiguity; compressed: always 0 —
    /// Base-91 reports carry no ambiguity field.)
    pub ambiguity: u8,
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
    // APRS position ambiguity = masked (space) minute digits, taken from the
    // latitude field per the spec (lon mirrors lat). The four maskable minute
    // bytes of `DDMM.mmH` are at indices 2, 3, 5, 6 (skipping `.`@4 and hemi@7).
    let ambiguity = [b[2], b[3], b[5], b[6]].iter().filter(|&&c| c == b' ').count() as u8;
    Some(AprsPosition { lat, lon, symbol_table, symbol_code, comment, ambiguity })
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
    // Base-91 compressed reports carry no position-ambiguity field (always full
    // precision to ~0.3 m); ambiguity is 0.
    Some(AprsPosition { lat, lon, symbol_table, symbol_code, comment, ambiguity: 0 })
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

/// Parse a Mic-E packet. Ported from aprslib `mice.py`.
///
/// Mic-E is the odd one: latitude (+ 3 message-type bits + position ambiguity)
/// is packed into the AX.25 **destination address**, while longitude / speed /
/// course / symbol live in the info field. So this takes BOTH the destination
/// callsign (`dest`, SSID stripped, exactly 6 chars) and the info field (`info`,
/// with the Mic-E DTI at `info[0]`: `` ` `` current, `'` old, or 0x1c/0x1d for
/// the TM-D700 variants). Note the symbol order is reversed vs normal reports:
/// symbol *code* precedes symbol *table*.
///
/// Surfaces lat/lon + symbol + comment only; speed/course/altitude/telemetry are
/// decodable but not part of [`AprsPosition`].
pub fn parse_mice(dest: &str, info: &[u8]) -> Option<AprsPosition> {
    if !matches!(*info.first()?, 0x60 | 0x27 | 0x1c | 0x1d) {
        return None;
    }
    let body = &info[1..];
    if body.len() < 8 {
        return None;
    }
    let d = dest.as_bytes();
    if d.len() != 6 {
        return None;
    }
    // dstcall must match ^[0-9A-Z]{3}[0-9L-Z]{3}$
    for (i, &c) in d.iter().enumerate() {
        let ok = if i < 3 {
            c.is_ascii_digit() || c.is_ascii_uppercase()
        } else {
            c.is_ascii_digit() || (b'L'..=b'Z').contains(&c)
        };
        if !ok {
            return None;
        }
    }

    // Translate each dest char to a latitude digit (or ambiguity space).
    let mut tmp = [0u8; 6];
    for (i, &c) in d.iter().enumerate() {
        tmp[i] = match c {
            b'K' | b'L' | b'Z' => b' ', // ambiguity spaces
            _ if c > 76 => c - 32,      // P-Y -> '0'..'9'
            _ if c > 57 => c - 17,      // A-J -> '0'..'9'
            _ => c,                     // '0'..'9'
        };
    }

    // Position ambiguity = trailing spaces; anything else must be a digit.
    let mut posamb = 0usize;
    let mut seen_space = false;
    for &t in &tmp {
        if t == b' ' {
            seen_space = true;
            posamb += 1;
        } else {
            if seen_space || !t.is_ascii_digit() {
                return None;
            }
        }
    }
    // Move the coordinate to the center of the ambiguity box.
    if posamb >= 4 {
        tmp[2] = b'3';
    } else if posamb > 0 {
        tmp[6 - posamb] = b'5';
    }

    let dd = two_digit(tmp[0], tmp[1])?;
    let mm = two_digit(tmp[2], tmp[3])? as f64 + two_digit(tmp[4], tmp[5])? as f64 / 100.0;
    let mut lat = dd as f64 + mm / 60.0;
    if d[3] <= 0x4c {
        lat = -lat; // dest[3] <= 'L' => South
    }

    // Longitude: degrees from info[0] + the 100°/180-189/190-199 corrections.
    let mut lon_deg = body[0] as i32 - 28;
    if d[4] >= 0x50 {
        lon_deg += 100; // dest[4] >= 'P' => +100 offset
    }
    if (180..=189).contains(&lon_deg) {
        lon_deg -= 80;
    }
    if (190..=199).contains(&lon_deg) {
        lon_deg -= 190;
    }
    let mut lngmin = body[1] as f64 - 28.0;
    if lngmin >= 60.0 {
        lngmin -= 60.0;
    }
    lngmin += (body[2] as f64 - 28.0) / 100.0;
    match posamb {
        0 => {}
        1 => lngmin = ((lngmin * 10.0).floor() + 0.5) / 10.0,
        2 => lngmin = lngmin.floor() + 0.5,
        3 => lngmin = ((lngmin / 10.0).floor() + 0.5) * 10.0,
        4 => lngmin = 30.0,
        _ => return None, // ambiguity > 4 unsupported for longitude
    }
    let mut lon = lon_deg as f64 + lngmin / 60.0;
    if d[5] >= 0x50 {
        lon = -lon; // dest[5] >= 'P' => West
    }

    // Symbol order is REVERSED in Mic-E: code then table.
    let symbol_code = body[6] as char;
    let symbol_table = body[7] as char;
    let comment = if body.len() > 8 {
        String::from_utf8_lossy(&body[8..]).trim().to_string()
    } else {
        String::new()
    };

    sane(lat, lon)?;
    Some(AprsPosition { lat, lon, symbol_table, symbol_code, comment, ambiguity: posamb as u8 })
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
        assert_eq!(p.ambiguity, 0); // full-precision fix
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
        assert_eq!(p.ambiguity, 1); // one masked minute digit (hundredths)
    }

    #[test]
    fn position_ambiguity_level_reported() {
        // Level 2: both hundredths-of-minute digits masked (DDMM.  ).
        let p2 = parse_position(b"!4903.  N/07201.  W-").unwrap();
        assert_eq!(p2.ambiguity, 2);
        approx(p2.lat, 49.05); // 03.00'
        // Level 4: all four minute digits masked (DD  .  ).
        let p4 = parse_position(b"!49  .  N/072  .  W-").unwrap();
        assert_eq!(p4.ambiguity, 4);
        approx(p4.lat, 49.0); // 00.00'
    }

    #[test]
    fn compressed_base91() {
        // aprslib reference: "/5L!!<*e7>" => 49.5, -72.75.
        let p = parse_position(b"!/5L!!<*e7>  T").unwrap();
        approx(p.lat, 49.5);
        approx(p.lon, -72.75);
        assert_eq!(p.symbol_table, '/');
        assert_eq!(p.symbol_code, '>');
        assert_eq!(p.ambiguity, 0); // compressed reports carry no ambiguity
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

    // -- Mic-E --------------------------------------------------------------
    // Hand-derived vectors (aprslib mice.py formulas applied by hand):
    // dest "332UVT": digits 33/25.64, dest[3]='U'(>'L')=>N, dest[4]='V'(>='P')=>+100,
    //   dest[5]='T'(>='P')=>W. info "`(`n\x1c\x1c\x1c>/": lon byte '('=>12(+100=112),
    //   '`'=>8 min, 'n'=>.82 => 112°08.82'W; symbol code '>' table '/'.
    const MICE_INFO: &[u8] = b"\x60\x28\x60\x6e\x1c\x1c\x1c\x3e\x2f";

    #[test]
    fn mice_north_west_with_offset() {
        let p = parse_mice("332UVT", MICE_INFO).unwrap();
        approx(p.lat, 33.427333); // 33 + 25.64/60
        approx(p.lon, -112.147); // -(112 + 8.82/60)
        assert_eq!(p.symbol_code, '>');
        assert_eq!(p.symbol_table, '/');
        assert_eq!(p.ambiguity, 0); // all-digit dest => no ambiguity
    }

    #[test]
    fn mice_ambiguity_from_masked_dest() {
        // dest with trailing ambiguity chars (K/L/Z => masked) => posamb > 0.
        // "332UVL": positions 33/2x.xx with the last lat digit masked (L) => amb 1.
        let p = parse_mice("332UVL", MICE_INFO).unwrap();
        assert_eq!(p.ambiguity, 1);
    }

    #[test]
    fn mice_south_east_no_offset() {
        // All-digit dest => same magnitudes but dest[3]='5'(<='L')=>S,
        // dest[4]='6'(<'P')=> no offset, dest[5]='4'(<'P')=>E.
        let p = parse_mice("332564", MICE_INFO).unwrap();
        approx(p.lat, -33.427333);
        approx(p.lon, 12.147); // +(12 + 8.82/60)
    }

    #[test]
    fn mice_rejects_bad_input() {
        assert!(parse_mice("332UVT", b"!short").is_none()); // wrong DTI
        assert!(parse_mice("332UVT", b"\x60\x28\x60").is_none()); // body < 8
        assert!(parse_mice("332UV", MICE_INFO).is_none()); // dest not 6 chars
        assert!(parse_mice("33!UVT", MICE_INFO).is_none()); // invalid dest char
    }
}
