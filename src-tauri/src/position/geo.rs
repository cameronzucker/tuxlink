//! Great-circle distance + bearing over geographic points and Maidenhead grids.
//! Mirrors the shipping catalog UI distance (`src/catalog/distance.ts`, R=6371, clamped
//! haversine) so the agent surface and the human catalog report the same kilometers.

use super::maidenhead::grid_to_lat_lon;

const EARTH_RADIUS_KM: f64 = 6371.0;
const KM_TO_MI: f64 = 0.621371;

/// Great-circle distance in km between two `(lat, lon)` points (degrees), haversine.
/// The root argument is clamped to `<= 1.0` (mirror `src/catalog/distance.ts:23`) so
/// near-antipodal float error cannot push `asin` out of its domain and yield `NaN`.
pub fn haversine_km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (lat1, lon1) = a;
    let (lat2, lon2) = b;
    let d_lat = (lat2 - lat1).to_radians();
    let d_lon = (lon2 - lon1).to_radians();
    let h = (d_lat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (d_lon / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_KM * h.sqrt().min(1.0).asin()
}

/// Initial great-circle bearing from `a` to `b` in degrees `[0, 360)` (0=N, 90=E, clockwise).
pub fn bearing_deg(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (lat1, lon1) = a;
    let (lat2, lon2) = b;
    let r1 = lat1.to_radians();
    let r2 = lat2.to_radians();
    let d_lon = (lon2 - lon1).to_radians();
    let y = d_lon.sin() * r2.cos();
    let x = r1.cos() * r2.sin() - r1.sin() * r2.cos() * d_lon.cos();
    (y.atan2(x).to_degrees() + 360.0) % 360.0
}

/// Distance (km) + optional bearing (deg) between two Maidenhead grids, each taken at its
/// square center. `None` if either grid is absent or malformed. Bearing is `None` when the
/// distance is exactly 0 (co-located / identical square) — `atan2(0,0)=0` would otherwise
/// read as a spurious due-North.
pub fn distance_bearing_between_grids(
    a: Option<&str>,
    b: Option<&str>,
) -> Option<(f64, Option<f64>)> {
    let ga = grid_to_lat_lon(a?)?;
    let gb = grid_to_lat_lon(b?)?;
    let km = haversine_km(ga, gb);
    let bearing = if km == 0.0 { None } else { Some(bearing_deg(ga, gb)) };
    Some((km, bearing))
}

/// Kilometers to statute miles (matches `src/catalog/distance.ts:33` `kmToMi`).
pub fn km_to_mi(km: f64) -> f64 {
    km * KM_TO_MI
}

#[cfg(test)]
mod tests {
    use super::*;

    // Hand-derived + cross-checked against propagation_live.rs:87 (VOACAP 215.2km / 301.65°).
    const DM43: (f64, f64) = (33.5, -111.0);
    const DM34: (f64, f64) = (34.5, -113.0);

    #[test]
    fn haversine_matches_shipping_fixture() {
        let km = haversine_km(DM43, DM34);
        assert!((km - 215.28).abs() < 0.5, "DM43->DM34 haversine {km} != ~215.28");
    }

    #[test]
    fn haversine_identical_points_is_zero_not_nan() {
        let km = haversine_km(DM43, DM43);
        assert_eq!(km, 0.0);
    }

    #[test]
    fn haversine_antipodal_no_nan() {
        // near-antipodal: clamp must prevent asin domain overflow
        let km = haversine_km((0.0, 0.0), (0.0, 179.9999999));
        assert!(km.is_finite(), "antipodal produced non-finite {km}");
    }

    #[test]
    fn bearing_cardinals() {
        // due north: same lon, higher lat -> ~0
        assert!(bearing_deg((0.0, 0.0), (1.0, 0.0)).abs() < 1e-6);
        // due east: same lat, higher lon -> ~90
        assert!((bearing_deg((0.0, 0.0), (0.0, 1.0)) - 90.0).abs() < 1e-6);
        // due south -> 180
        assert!((bearing_deg((0.0, 0.0), (-1.0, 0.0)) - 180.0).abs() < 1e-6);
    }

    #[test]
    fn bearing_fixture() {
        assert!((bearing_deg(DM43, DM34) - 301.5).abs() < 1.0);
    }

    #[test]
    fn grids_distance_and_bearing() {
        let (km, brg) = distance_bearing_between_grids(Some("DM43"), Some("DM34")).unwrap();
        assert!((km - 215.28).abs() < 0.5);
        assert!((brg.unwrap() - 301.5).abs() < 1.0);
    }

    #[test]
    fn grids_zero_distance_bearing_is_none() {
        let (km, brg) = distance_bearing_between_grids(Some("DM43"), Some("DM43")).unwrap();
        assert_eq!(km, 0.0);
        assert_eq!(brg, None); // co-located: no spurious due-North
    }

    #[test]
    fn grids_absent_or_malformed_is_none() {
        assert_eq!(distance_bearing_between_grids(None, Some("DM43")), None);
        assert_eq!(distance_bearing_between_grids(Some("DM43"), None), None);
        assert_eq!(distance_bearing_between_grids(Some("ZZ99"), Some("DM43")), None);
    }

    #[test]
    fn km_to_mi_conversion() {
        assert!((km_to_mi(100.0) - 62.1371).abs() < 1e-4);
    }
}
