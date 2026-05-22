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
}
