//! gpsd TPV client: parse gpsd JSON fix reports into position `Fix`es.
//! (The watch task + reconnect loop arrive in Task 10; this task is the parser.)

use crate::position::{lat_lon_to_grid, Fix};

/// Parse ONE gpsd JSON line into a `Fix`. Accepts only a TPV report with a usable
/// fix: `class == "TPV"` AND `mode >= 2` (2 = 2D, 3 = 3D; 0/1 = no fix) AND both
/// `lat`/`lon` present. Returns `None` for anything else (non-TPV, no-fix, malformed
/// JSON, missing fields). Uses `serde_json::Value` (already a dependency) to avoid a
/// rigid struct.
pub(crate) fn parse_tpv(line: &str) -> Option<Fix> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    if v.get("class")?.as_str()? != "TPV" { return None; }
    if v.get("mode")?.as_i64()? < 2 { return None; }   // 0/1 = no fix
    let lat = v.get("lat")?.as_f64()?;
    let lon = v.get("lon")?.as_f64()?;
    Some(Fix { grid: lat_lon_to_grid(lat, lon), received: std::time::Instant::now() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_3d_tpv_into_a_grid() {
        let line = r#"{"class":"TPV","mode":3,"lat":48.143,"lon":11.608}"#;
        let fix = parse_tpv(line).unwrap();
        assert_eq!(fix.grid, "JN58td");
    }
    #[test]
    fn rejects_no_fix_and_non_tpv() {
        assert!(parse_tpv(r#"{"class":"TPV","mode":1}"#).is_none());   // no fix (mode 1)
        assert!(parse_tpv(r#"{"class":"SKY"}"#).is_none());            // not a fix report
        assert!(parse_tpv("not json").is_none());                      // not JSON
    }
}
