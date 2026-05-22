//! Maidenhead locator conversion (no external crate — the algorithm is small and
//! we need exact control over precision + clamping). Field (A-R) / square (0-9) /
//! subsquare (a-x). Longitude uses 20°/2°/5′ steps; latitude 10°/1°/2.5′.

/// Convert WGS-84 lat/lon (degrees) to a 6-char Maidenhead locator.
/// Inputs are clamped to valid ranges so this never panics.
pub fn lat_lon_to_grid(lat: f64, lon: f64) -> String {
    let lon = (lon.clamp(-180.0, 179.999) + 180.0) / 20.0;
    let lat = (lat.clamp(-90.0, 89.999) + 90.0) / 10.0;

    let lon_field = lon.floor();
    let lat_field = lat.floor();
    let lon_sq = ((lon - lon_field) * 10.0).floor();
    let lat_sq = ((lat - lat_field) * 10.0).floor();
    let lon_sub = ((lon - lon_field - lon_sq / 10.0) * 240.0).floor();
    let lat_sub = ((lat - lat_field - lat_sq / 10.0) * 240.0).floor();

    let a = |n: f64, base: u8| (base + n as u8) as char;
    format!(
        "{}{}{}{}{}{}",
        a(lon_field, b'A'),
        a(lat_field, b'A'),
        a(lon_sq, b'0'),
        a(lat_sq, b'0'),
        a(lon_sub, b'a'),
        a(lat_sub, b'a'),
    )
}

/// Convert a 4- or 6-char Maidenhead locator to the lat/lon at the CENTER of the
/// square. Returns `None` for malformed input (wrong length, out-of-range chars).
pub fn grid_to_lat_lon(grid: &str) -> Option<(f64, f64)> {
    let g = grid.as_bytes();
    if g.len() != 4 && g.len() != 6 { return None; }
    let up = |b: u8| b.to_ascii_uppercase();
    let lo = |b: u8| b.to_ascii_lowercase();
    let field = |b: u8| (b'A'..=b'R').contains(&up(b)).then(|| (up(b) - b'A') as f64);
    let digit = |b: u8| b.is_ascii_digit().then(|| (b - b'0') as f64);

    let mut lon = field(g[0])? * 20.0 - 180.0;
    let mut lat = field(g[1])? * 10.0 - 90.0;
    lon += digit(g[2])? * 2.0;
    lat += digit(g[3])? * 1.0;
    if g.len() == 6 {
        let sub = |b: u8| (b'a'..=b'x').contains(&lo(b)).then(|| (lo(b) - b'a') as f64);
        lon += sub(g[4])? * 5.0 / 60.0;
        lat += sub(g[5])? * 2.5 / 60.0;
        lon += 2.5 / 60.0; lat += 1.25 / 60.0;       // center of subsquare
    } else {
        lon += 1.0; lat += 0.5;                       // center of square
    }
    Some((lat, lon))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_references_round_to_six_char() {
        assert_eq!(lat_lon_to_grid(48.143, 11.608), "JN58td"); // Munich
        assert_eq!(lat_lon_to_grid(-34.91, -56.21), "GF15vc"); // Montevideo
        assert_eq!(lat_lon_to_grid(0.0, 0.0), "JJ00aa");       // origin corner
    }

    #[test]
    fn clamps_out_of_range_inputs() {
        let g = lat_lon_to_grid(95.0, 200.0);
        assert_eq!(g.len(), 6);
    }

    #[test]
    fn grid_to_lat_lon_round_trips_to_same_grid() {
        let (lat, lon) = grid_to_lat_lon("JN58td").unwrap();
        assert_eq!(lat_lon_to_grid(lat, lon), "JN58td");
    }

    #[test]
    fn grid_to_lat_lon_rejects_malformed() {
        assert!(grid_to_lat_lon("ZZ99").is_none());   // field letters only go A-R
        assert!(grid_to_lat_lon("J").is_none());       // too short
    }
}
