//! Offline magnetic declination via the World Magnetic Model 2025 (spec
//! §Declination; §NewCommands; Task A5 of the Station Intelligence L3 plan).
//!
//! # Why from-coefficients (not a crate)
//!
//! The plan's Step-1 gate asked whether a maintained pure-Rust WMM crate
//! (`world-magnetic-model`) declares `rust-version <= 1.75` and builds on both
//! CI arches. It does NOT: `world-magnetic-model` v0.4.0 publishes a **null**
//! `rust_version` (no declared MSRV), so it fails the crate-route criterion,
//! its build cannot be verified against this project's MSRV-1.75 floor, and its
//! coefficient epoch is not pinnable from crate metadata. The controller ran
//! this gate and selected the **from-coefficients** route: bundle NOAA's
//! public-domain WMM2025 Gauss coefficients and implement the spherical-harmonic
//! synthesis with `std` math only — **no new Cargo dependency**.
//!
//! # Source of truth
//!
//! Coefficients (`WMM2025.COF`) and the validation vectors used in the tests are
//! the official US-Government (public-domain) files from NOAA NCEI:
//! <https://www.ncei.noaa.gov/products/world-magnetic-model/wmm-coefficients>
//! (`WMM2025COF.zip` → `WMM2025.COF` + `WMM2025_TestValues.txt`,
//! <https://www.ncei.noaa.gov/sites/default/files/2024-12/WMM2025COF.zip>).
//!
//! # Algorithm
//!
//! Faithful transcription of NOAA's reference `geomag70.c` spherical-harmonic
//! synthesis (public domain), which is validated against the same NOAA test
//! vectors the unit tests below assert:
//!
//! 1. **Geodetic → geocentric.** WGS-84 ellipsoid (`a = 6378.137 km`,
//!    `f = 1/298.257223563`). Produces geocentric radius `r`, `sin`/`cos` of the
//!    geocentric latitude, and the geocentric-vs-geodetic latitude offset used
//!    to rotate the field back at the end.
//! 2. **Schmidt semi-normalized associated Legendre functions** `P(n,m)` and
//!    their colatitude derivatives, via the standard recurrence with the
//!    Schmidt quasi-normalization factors folded into the Gauss coefficients.
//! 3. **Time adjustment.** `g(n,m,t) = g(n,m,t0) + (t - 2025.0)·ġ(n,m)` (and `h`
//!    likewise) using the secular-variation rates from the `.COF` file.
//! 4. **Field synthesis** to degree/order 12 → geocentric `(Bt, Bp, Br)`,
//!    rotated to geodetic `X` (north), `Y` (east), `Z` (down).
//! 5. **Declination** `D = atan2(Y, X)`, in degrees.
//!
//! The reference geomagnetic radius is `6371.2 km`; the epoch is `2025.0`; the
//! model is valid through `2030.0`.

use serde::Serialize;

use crate::ft8::commands::Ft8CmdError;

/// Bundled NOAA public-domain WMM2025 Gauss coefficients (main field + secular
/// variation), parsed at call time. Small (12 degrees → 90 lines); reparsing per
/// call is negligible and avoids a `lazy_static`/`OnceLock` dependency surface.
const WMM2025_COF: &str = include_str!("WMM2025.COF");

/// Maximum spherical-harmonic degree/order of WMM.
const MAX_ORDER: usize = 12;
/// WMM model epoch (start of the 5-year validity window).
const EPOCH: f64 = 2025.0;
/// Geomagnetic reference radius (km), per the WMM definition.
const GEOMAG_RE: f64 = 6371.2;
/// WGS-84 semi-major axis (km).
const WGS84_A: f64 = 6378.137;
/// WGS-84 flattening.
const WGS84_F: f64 = 1.0 / 298.257_223_563;

/// Declination result for the setup surface's aim hero (Task C6 consumer).
///
/// Serializes with **camelCase** wire keys `declDeg` / `modelEpoch` /
/// `validUntil`; a snake_case leak would make C6's aim hero read `undefined`, so
/// the key names are asserted in [`tests::decl_dto_serializes_camelcase`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeclDto {
    /// Magnetic declination in degrees, east-positive (true→magnetic offset).
    pub decl_deg: f64,
    /// The model epoch label, e.g. `"WMM2025"`.
    pub model_epoch: String,
    /// ISO date the model epoch expires (5-year WMM window end).
    pub valid_until: String,
}

/// Parsed WMM coefficient set: main-field Gauss coefficients `g`/`h` at the
/// epoch and their secular-variation rates `gd`/`hd`. Indexed `[n][m]` with
/// `1 <= n <= 12`, `0 <= m <= n`.
struct WmmCoefficients {
    g: [[f64; MAX_ORDER + 1]; MAX_ORDER + 1],
    h: [[f64; MAX_ORDER + 1]; MAX_ORDER + 1],
    gd: [[f64; MAX_ORDER + 1]; MAX_ORDER + 1],
    hd: [[f64; MAX_ORDER + 1]; MAX_ORDER + 1],
}

impl WmmCoefficients {
    /// Parse the bundled `.COF` text. The first line is the epoch header; each
    /// subsequent data line is `n m g h gdot hdot`; a terminating line of `9`s
    /// closes the block. Returns `None` on any malformed numeric field so the
    /// command can surface `internal-error` rather than panic.
    fn parse(cof: &str) -> Option<Self> {
        let mut c = WmmCoefficients {
            g: [[0.0; MAX_ORDER + 1]; MAX_ORDER + 1],
            h: [[0.0; MAX_ORDER + 1]; MAX_ORDER + 1],
            gd: [[0.0; MAX_ORDER + 1]; MAX_ORDER + 1],
            hd: [[0.0; MAX_ORDER + 1]; MAX_ORDER + 1],
        };
        let mut lines = cof.lines();
        // First non-empty line is the epoch header ("2025.0 WMM-2025 ...").
        let _header = lines.by_ref().find(|l| !l.trim().is_empty())?;
        let mut seen = 0usize;
        for line in lines {
            let t = line.trim();
            if t.is_empty() {
                continue;
            }
            // Terminator: a run of 9s (two such lines end the file).
            if t.starts_with("9999") {
                break;
            }
            let mut it = t.split_whitespace();
            let n: usize = it.next()?.parse().ok()?;
            let m: usize = it.next()?.parse().ok()?;
            let g: f64 = it.next()?.parse().ok()?;
            let h: f64 = it.next()?.parse().ok()?;
            let gd: f64 = it.next()?.parse().ok()?;
            let hd: f64 = it.next()?.parse().ok()?;
            if n == 0 || n > MAX_ORDER || m > n {
                return None;
            }
            c.g[n][m] = g;
            c.h[n][m] = h;
            c.gd[n][m] = gd;
            c.hd[n][m] = hd;
            seen += 1;
        }
        // Degree-12 triangle has 12 + 11 + ... + 1 = ... plus the m=0 row per n.
        // Total (n,m) pairs for 1..=12 = sum_{n=1}^{12}(n+1) = 90.
        if seen != 90 {
            return None;
        }
        Some(c)
    }
}

/// Precomputed Schmidt quasi-normalization factors `S(n,m)` and the Legendre
/// recurrence coefficients `k(n,m)`. These depend only on `n`/`m`, never on the
/// evaluation point, so they are built once per call and reused.
struct Normalization {
    schmidt: [[f64; MAX_ORDER + 1]; MAX_ORDER + 1],
    k: [[f64; MAX_ORDER + 1]; MAX_ORDER + 1],
}

impl Normalization {
    /// Build the Schmidt factors and recurrence coefficients, mirroring the
    /// `geomag70.c` initialization exactly (indices transposed to `[n][m]`).
    fn build() -> Self {
        let mut schmidt = [[0.0f64; MAX_ORDER + 1]; MAX_ORDER + 1];
        let mut k = [[0.0f64; MAX_ORDER + 1]; MAX_ORDER + 1];
        schmidt[0][0] = 1.0;
        for n in 1..=MAX_ORDER {
            schmidt[n][0] = schmidt[n - 1][0] * (2 * n - 1) as f64 / n as f64;
            let mut j = 2.0f64;
            for m in 0..=n {
                // Recurrence coefficient k(n,m) = ((n-1)^2 - m^2)/((2n-1)(2n-3)).
                if n > 1 {
                    let num = ((n - 1) * (n - 1)) as f64 - (m * m) as f64;
                    let den = ((2 * n - 1) * (2 * n - 3)) as f64;
                    k[n][m] = num / den;
                }
                if m > 0 {
                    let flnmj = ((n - m + 1) as f64 * j) / (n + m) as f64;
                    schmidt[n][m] = schmidt[n][m - 1] * flnmj.sqrt();
                    j = 1.0;
                }
            }
        }
        // k(1,1) is unused by the recurrence (the n==m branch never reads k), but
        // geomag70 zeroes it defensively; match that.
        k[1][1] = 0.0;
        Normalization { schmidt, k }
    }
}

/// Compute the geodetic field components `(X_north, Y_east, Z_down)` in nT at the
/// given geodetic latitude/longitude (degrees), altitude above the WGS-84
/// ellipsoid (km), and decimal year.
///
/// Faithful port of NOAA `geomag70.c`'s spherical-harmonic synthesis; the
/// Legendre `dp` derivatives are with respect to geocentric colatitude.
fn field_components(lat_deg: f64, lon_deg: f64, alt_km: f64, year: f64) -> Option<(f64, f64, f64)> {
    let coef = WmmCoefficients::parse(WMM2025_COF)?;
    let norm = Normalization::build();

    let dt = year - EPOCH;

    // Schmidt-normalize the time-adjusted Gauss coefficients up front.
    let mut g = [[0.0f64; MAX_ORDER + 1]; MAX_ORDER + 1];
    let mut h = [[0.0f64; MAX_ORDER + 1]; MAX_ORDER + 1];
    for n in 1..=MAX_ORDER {
        for m in 0..=n {
            let s = norm.schmidt[n][m];
            g[n][m] = (coef.g[n][m] + dt * coef.gd[n][m]) * s;
            h[n][m] = (coef.h[n][m] + dt * coef.hd[n][m]) * s;
        }
    }

    // ---- geodetic -> geocentric (spherical) --------------------------------
    let lat = lat_deg.to_radians();
    let lon = lon_deg.to_radians();
    let a2 = WGS84_A * WGS84_A;
    let b = WGS84_A * (1.0 - WGS84_F);
    let b2 = b * b;
    let c2 = a2 - b2;
    let a4 = a2 * a2;
    let c4 = a4 - b2 * b2;

    let srlat = lat.sin();
    let crlat = lat.cos();
    let srlat2 = srlat * srlat;
    let crlat2 = crlat * crlat;

    let q = (a2 - c2 * srlat2).sqrt();
    let q1 = alt_km * q;
    let q2 = ((q1 + a2) / (q1 + b2)).powi(2);
    // ct = cos(colatitude) = sin(geocentric latitude); st = sin(colatitude).
    let ct = srlat / (q2 * crlat2 + srlat2).sqrt();
    let st = (1.0 - ct * ct).sqrt();
    let r2 = alt_km * alt_km + 2.0 * q1 + (a4 - c4 * srlat2) / (q * q);
    let r = r2.sqrt();
    let d = (a2 * crlat2 + b2 * srlat2).sqrt();
    // (ca, sa) rotate the field from geocentric back to geodetic latitude.
    let ca = (alt_km + d) / r;
    let sa = c2 * crlat * srlat / (r * d);

    // ---- sin/cos of m*longitude via recurrence -----------------------------
    let mut sp = [0.0f64; MAX_ORDER + 1];
    let mut cp = [0.0f64; MAX_ORDER + 1];
    sp[0] = 0.0;
    cp[0] = 1.0;
    sp[1] = lon.sin();
    cp[1] = lon.cos();
    for m in 2..=MAX_ORDER {
        sp[m] = sp[1] * cp[m - 1] + cp[1] * sp[m - 1];
        cp[m] = cp[1] * cp[m - 1] - sp[1] * sp[m - 1];
    }

    // ---- Legendre recurrence + spherical-harmonic accumulation -------------
    let mut p = [[0.0f64; MAX_ORDER + 1]; MAX_ORDER + 1];
    let mut dp = [[0.0f64; MAX_ORDER + 1]; MAX_ORDER + 1];
    p[0][0] = 1.0;
    dp[0][0] = 0.0;

    let aor = GEOMAG_RE / r;
    let mut ar = aor * aor; // becomes (re/r)^(n+2) after the *= aor below
    let mut bt = 0.0f64; // -theta (colatitude) component
    let mut bp = 0.0f64; // phi (east) component, divided by st at the end
    let mut br = 0.0f64; // radial component

    for n in 1..=MAX_ORDER {
        ar *= aor;
        for m in 0..=n {
            // ---- associated Legendre P(n,m) and its colatitude derivative ----
            if n == m {
                p[n][m] = st * p[n - 1][m - 1];
                dp[n][m] = st * dp[n - 1][m - 1] + ct * p[n - 1][m - 1];
            } else if n == 1 && m == 0 {
                p[n][m] = ct * p[n - 1][m];
                dp[n][m] = ct * dp[n - 1][m] - st * p[n - 1][m];
            } else {
                // n > 1, n != m
                let (pn2, dpn2) = if m > n - 2 {
                    (0.0, 0.0)
                } else {
                    (p[n - 2][m], dp[n - 2][m])
                };
                p[n][m] = ct * p[n - 1][m] - norm.k[n][m] * pn2;
                dp[n][m] = ct * dp[n - 1][m] - st * p[n - 1][m] - norm.k[n][m] * dpn2;
            }

            // ---- accumulate the spherical-harmonic expansion terms ----------
            let (temp1, temp2) = if m == 0 {
                (g[n][m] * cp[m], g[n][m] * sp[m])
            } else {
                (
                    g[n][m] * cp[m] + h[n][m] * sp[m],
                    g[n][m] * sp[m] - h[n][m] * cp[m],
                )
            };
            let par = ar * p[n][m];
            bt -= ar * temp1 * dp[n][m];
            bp += m as f64 * temp2 * par;
            br += (n + 1) as f64 * temp1 * par;
        }
    }

    // Valid Maidenhead centers never reach the exact geographic pole (|lat| < 90),
    // so `st` is strictly positive here; guard against a division blow-up only for
    // defensiveness. At the pole declination is undefined; report 0 east-field.
    if st.abs() < 1e-12 {
        bp = 0.0;
    } else {
        bp /= st;
    }

    // ---- rotate spherical -> geodetic components ---------------------------
    let x = -bt * ca - br * sa; // north
    let y = bp; // east
    let z = bt * sa - br * ca; // down
    Some((x, y, z))
}

/// Magnetic declination in degrees (east-positive) at sea level (altitude 0) for
/// the given geodetic latitude/longitude and decimal year. `None` only if the
/// bundled coefficients fail to parse (a build/packaging fault, not user input).
pub fn declination_at(lat_deg: f64, lon_deg: f64, year: f64) -> Option<f64> {
    declination_at_alt(lat_deg, lon_deg, 0.0, year)
}

/// Altitude-aware declination, in degrees east-positive. The full synthesis
/// supports any altitude above the ellipsoid; [`declination_at`] pins `alt_km =
/// 0` since the setup surface is a sea-level model and NOAA's published
/// declination oracle rows used in the tests are height=0.
fn declination_at_alt(lat_deg: f64, lon_deg: f64, alt_km: f64, year: f64) -> Option<f64> {
    let (x, y, _z) = field_components(lat_deg, lon_deg, alt_km, year)?;
    Some(y.atan2(x).to_degrees())
}

/// Decimal year from a `SystemTime`-derived Unix timestamp (seconds since the
/// 1970 epoch), using a dependency-free civil-calendar conversion. E.g. midday
/// 2027-07-02 → ≈ 2027.5.
fn decimal_year_from_unix(secs: u64) -> f64 {
    let secs = secs as i64;
    let days = secs.div_euclid(86_400);
    let year = civil_year_from_days(days);
    let year_start = days_from_civil(year, 1, 1) * 86_400;
    let next_start = days_from_civil(year + 1, 1, 1) * 86_400;
    year as f64 + (secs - year_start) as f64 / (next_start - year_start) as f64
}

/// Howard Hinnant's `civil_from_days`, reduced to just the year field. `days` is
/// days since 1970-01-01 (may be negative).
fn civil_year_from_days(days: i64) -> i64 {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    if m <= 2 {
        y + 1
    } else {
        y
    }
}

/// Howard Hinnant's `days_from_civil`: days since 1970-01-01 for a proleptic
/// Gregorian `(year, month, day)`.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + (d - 1); // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

/// Current decimal year from the system clock; `internal-error` on a clock set
/// before the Unix epoch.
fn now_decimal_year() -> Result<f64, Ft8CmdError> {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| Ft8CmdError::new("internal-error", format!("system clock error: {e}")))?
        .as_secs();
    Ok(decimal_year_from_unix(secs))
}

/// Testable body of the `magnetic_declination` command: resolve the grid to a
/// lat/lon, evaluate WMM2025 at the current decimal year, and package the
/// declination with its model epoch/validity metadata.
pub(crate) fn magnetic_declination_inner(grid: &str, year: f64) -> Result<DeclDto, Ft8CmdError> {
    let (lat, lon) = crate::position::grid_to_lat_lon(grid)
        .ok_or_else(|| Ft8CmdError::new("invalid-grid", format!("not a Maidenhead grid: {grid}")))?;
    let decl = declination_at(lat, lon, year).ok_or_else(|| {
        Ft8CmdError::new("internal-error", "WMM2025 coefficients failed to parse")
    })?;
    Ok(DeclDto {
        decl_deg: decl,
        model_epoch: "WMM2025".to_string(),
        valid_until: "2030-01-01".to_string(),
    })
}

/// Offline magnetic declination for a Maidenhead grid (spec §NewCommands, Task
/// A5). Pure computation over the bundled WMM2025 coefficients — no radio, no
/// state, no I/O. Consumed by the setup surface's aim hero (Task C6).
#[tauri::command]
pub fn magnetic_declination(grid: String) -> Result<DeclDto, Ft8CmdError> {
    let year = now_decimal_year()?;
    magnetic_declination_inner(&grid, year)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GENUINE NOAA-published WMM2025 declination test values — the real,
    /// independent oracle. These are the `D (Deg)` column of NOAA NCEI's
    /// official "Test Values for WMM2025" table, height=0 (sea-level) rows:
    /// <https://www.ncei.noaa.gov/sites/default/files/2025-02/WMM2025testvalues.pdf>
    ///
    /// `(decimal_year, lat_deg, lon_deg, published_D_deg)`. NOT computed by this
    /// crate — asserting against them catches a systematic error the
    /// implementation cannot influence. They span both hemispheres, high
    /// latitude (±80° stresses the near-pole Legendre terms), and two epochs
    /// (2025.0 and 2027.5 exercise the secular-variation term). Longitude 240
    /// is +240°E; the synthesis uses `sin`/`cos` of `m·λ`, so 240°E and −120°E
    /// are identical — the published 240 value is passed verbatim.
    const NOAA_PUBLISHED: &[(f64, f64, f64, f64)] = &[
        (2025.0, 80.0, 0.0, 1.28),    // Arctic, epoch start
        (2025.0, 0.0, 120.0, -0.16),  // equator
        (2025.0, -80.0, 240.0, 68.78), // Antarctic, large declination
        (2027.5, 80.0, 0.0, 2.59),    // Arctic, mid-epoch (secular variation)
        (2027.5, -80.0, 240.0, 68.49), // Antarctic, mid-epoch
    ];

    #[test]
    fn declination_matches_noaa_published_values() {
        for &(year, lat, lon, expected) in NOAA_PUBLISHED {
            // The public sea-level entry point is what production calls; assert
            // it directly against NOAA's independently-published D.
            let d = declination_at(lat, lon, year).expect("coefficients parse");
            assert!(
                (d - expected).abs() < 0.1,
                "WMM2025 ({lat},{lon}) yr={year}: got {d}, NOAA-published {expected}"
            );
        }
    }

    #[test]
    fn grid_input_matches_lat_lon_path() {
        // A valid grid resolves via the shared maidenhead converter, and the
        // command's declination equals the direct lat/lon computation for that
        // same center point.
        let grid = "DM79"; // Denver-ish; center ≈ (39.5, -105.0)
        let (lat, lon) = crate::position::grid_to_lat_lon(grid).expect("valid grid");
        let dto = magnetic_declination_inner(grid, 2025.0).expect("declination");
        let direct = declination_at(lat, lon, 2025.0).expect("coefficients parse");
        assert!((dto.decl_deg - direct).abs() < 1e-9, "grid path diverged");
        assert_eq!(dto.model_epoch, "WMM2025");
        assert_eq!(dto.valid_until, "2030-01-01");
    }

    #[test]
    fn bad_grid_is_invalid_grid() {
        let err = magnetic_declination_inner("ZZ99", 2025.0).expect_err("must reject");
        assert_eq!(err.kind, "invalid-grid");
    }

    #[test]
    fn decl_dto_serializes_camelcase() {
        let v = serde_json::to_value(DeclDto {
            decl_deg: 9.7,
            model_epoch: "WMM2025".into(),
            valid_until: "2030-01-01".into(),
        })
        .unwrap();
        // A snake_case leak (`decl_deg`) would fail these — C6's aim hero reads
        // the camelCase keys.
        assert!(v["declDeg"].is_number());
        assert!(v["modelEpoch"].is_string());
        assert!(v["validUntil"].is_string());
        assert!(v.get("decl_deg").is_none(), "snake_case key leaked");
    }

    #[test]
    fn coefficients_parse_to_full_degree_12_triangle() {
        let c = WmmCoefficients::parse(WMM2025_COF).expect("bundled COF parses");
        // Spot-check a couple of known entries from the header of WMM2025.COF.
        assert!((c.g[1][0] - (-29351.8)).abs() < 1e-6);
        assert!((c.h[1][1] - 4545.4).abs() < 1e-6);
        assert!((c.gd[1][0] - 12.0).abs() < 1e-6);
        assert!((c.g[12][12] - (-0.7)).abs() < 1e-6);
    }

    #[test]
    fn decimal_year_conversion_is_sane() {
        // 2025-01-01T00:00:00Z = 1735689600 unix seconds.
        let y = decimal_year_from_unix(1_735_689_600);
        assert!((y - 2025.0).abs() < 1e-6, "got {y}");
        // ~mid-2025 (2025-07-02T12:00:00Z = 1751457600) ≈ 2025.5.
        let mid = decimal_year_from_unix(1_751_457_600);
        assert!((mid - 2025.5).abs() < 0.01, "got {mid}");
    }
}
