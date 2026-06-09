//! Tile coordinate parsing and validation.
//!
//! ## §8.4 traversal-safety rationale
//!
//! `{z}/{x}/{y}` values are webview-supplied and become BOTH upstream URL path
//! segments AND cache filesystem paths — they are the filesystem twin of the SSRF
//! entry. All three components are parsed as bounded `u32` integers and paths are
//! constructed from those validated integers ONLY (never string interpolation of
//! raw input), so a malicious coordinate cannot traverse the cache directory or
//! inject path separators.
//!
//! ## Geodetic tile-numbering convention (EPSG:4326 / WorldCRS84Quad)
//!
//! The feature serves ONLY geodetic (EPSG:4326 / WorldCRS84Quad) tiles. That
//! pyramid is `2^(z+1)` COLUMNS (x) wide and `2^z` ROWS (y) tall at every zoom
//! `z` — the world is 2 tiles wide × 1 tile tall at z=0 (lon ∈ [-180,180] → 2
//! columns; lat ∈ [-90,90] → 1 row). Leaflet under `L.CRS.EPSG4326` therefore
//! requests `x=1` at z=0 (the eastern hemisphere). Bounding x by `2^z` (the
//! square Web-Mercator convention) would reject the entire eastern half of every
//! zoom level. See `crate::tiles::crs::geodetic_tile_index`, which documents and
//! computes the same `2^(z+1)×2^z` convention.

/// A validated tile coordinate.
///
/// Construction via [`TileCoord::new`] or [`TileCoord::from_parts`] enforces:
/// - `z ≤ max_zoom` (checked **first** to prevent the bound-shift overflowing for
///   huge adversarial zoom values before the bound is computed)
/// - `x < 2^(z+1)` (geodetic columns: the EPSG:4326 world is twice as wide as it
///   is tall — `2^(z+1)` columns × `2^z` rows; see the module-level docs)
/// - `y < 2^z` (geodetic rows)
///
/// After construction all arithmetic on `z`/`x`/`y` is provably safe.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TileCoord {
    pub z: u32,
    pub x: u32,
    pub y: u32,
}

impl TileCoord {
    /// Validate and construct a `TileCoord`.
    ///
    /// The zoom cap is checked **before** the bound shifts to ensure that an
    /// adversarial `z` (e.g. 40) cannot cause a panic via overflow.
    ///
    /// Geodetic (EPSG:4326) bounds: `x < 2^(z+1)` columns, `y < 2^z` rows. The
    /// x bound uses `z + 1` because the EPSG:4326 world is twice as wide as it is
    /// tall (see the module-level docs and `crs::geodetic_tile_index`).
    pub fn new(z: u32, x: u32, y: u32, max_zoom: u32) -> Result<TileCoord, String> {
        if z > max_zoom {
            return Err(format!(
                "zoom {z} exceeds max_zoom {max_zoom}"
            ));
        }
        // `z` is now `≤ max_zoom`, but a caller could pass an absurd `max_zoom`
        // (≥ 32, or ≥ 31 for the x bound's `z + 1` shift). `checked_shl` returns
        // `None` for a shift `≥ 32` instead of panicking, and `checked_add(1)`
        // guards the `z = u32::MAX` edge, so both bound computations are
        // panic-safe regardless of what `max_zoom` the caller supplies
        // (defense-in-depth — the config-time cap is NOT relied on for this
        // primitive's panic-safety).
        //
        // x bound = 2^(z+1) columns (geodetic: world is 2 tiles wide at z=0).
        let x_bound = z
            .checked_add(1)
            .and_then(|zp1| 1u32.checked_shl(zp1))
            .ok_or_else(|| format!("zoom {z} too large to compute a tile column bound"))?;
        // y bound = 2^z rows.
        let y_bound = 1u32
            .checked_shl(z)
            .ok_or_else(|| format!("zoom {z} too large to compute a tile row bound"))?;
        if x >= x_bound {
            return Err(format!("x {x} out of range for zoom {z} (column bound {x_bound})"));
        }
        if y >= y_bound {
            return Err(format!("y {y} out of range for zoom {z} (row bound {y_bound})"));
        }
        Ok(TileCoord { z, x, y })
    }

    /// Parse raw string components (as received from the webview URL) and
    /// validate bounds.  Rejects any input that does not parse as a `u32`
    /// (including negative strings, non-numeric strings, and empty strings).
    pub fn from_parts(z: &str, x: &str, y: &str, max_zoom: u32) -> Result<TileCoord, String> {
        let z: u32 = z.parse().map_err(|_| format!("invalid z: {z:?}"))?;
        let x: u32 = x.parse().map_err(|_| format!("invalid x: {x:?}"))?;
        let y: u32 = y.parse().map_err(|_| format!("invalid y: {y:?}"))?;
        TileCoord::new(z, x, y, max_zoom)
    }

    /// Return the y value for the upstream URL.
    ///
    /// For TMS sources the y-axis is flipped relative to XYZ; this converts a
    /// validated XYZ `y` into the TMS equivalent.  The result is always in
    /// `[0, 2^z)` because `z`, `x`, and `y` were already validated.
    pub fn upstream_y(&self, tms: bool) -> u32 {
        if tms {
            (1u32 << self.z) - 1 - self.y
        } else {
            self.y
        }
    }

    /// Return a cache-relative path for this tile.
    ///
    /// The path is built entirely from validated integer components — no raw
    /// string interpolation of webview input — so directory traversal is
    /// structurally impossible (§8.4).
    ///
    /// The y component of the filename matches `upstream_y(tms)` so the cache
    /// key is identical to the coordinate used when fetching from the upstream
    /// server.
    pub fn rel_path(&self, tms: bool) -> std::path::PathBuf {
        let mut p = std::path::PathBuf::new();
        p.push(self.z.to_string());
        p.push(self.x.to_string());
        p.push(format!("{}.tile", self.upstream_y(tms)));
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Task 2.1 tests (paste-verbatim per plan) ---

    #[test]
    fn accepts_in_range() {
        let c = TileCoord::new(3, 5, 2, /*max_zoom*/ 16).unwrap();
        assert_eq!((c.z, c.x, c.y), (3, 5, 2));
    }

    #[test]
    fn geodetic_x_bound_is_2_pow_z_plus_1_y_bound_is_2_pow_z() {
        // EPSG:4326 / WorldCRS84Quad: 2^(z+1) columns (x) × 2^z rows (y).
        // x bound = 2^(z+1): at z=1, x=2 is IN range (2 < 2^2 = 4) — this is the
        // eastern-hemisphere column the old square `2^z` bound wrongly rejected.
        assert!(TileCoord::new(1, 2, 0, 16).is_ok(), "x=2 < 2^(1+1)=4 must be Ok");
        // x boundary at z=1: last valid column is 2^2 - 1 = 3; 4 is out of range.
        assert!(TileCoord::new(1, 3, 0, 16).is_ok()); // x = 2^(z+1)-1 Ok
        assert!(TileCoord::new(1, 4, 0, 16).is_err()); // x = 2^(z+1) Err
        // y bound = 2^z (unchanged): at z=0, y must be < 2^0 = 1.
        assert!(TileCoord::new(0, 0, 1, 16).is_err()); // y = 2^0 Err
        // x=1 at z=0 (eastern hemisphere) is now accepted (Finding 3 regression):
        // x = 2^(0+1)-1 = 1 < 2 Ok; the old bound rejected it → BadPath → half map.
        assert!(TileCoord::new(0, 1, 0, 16).is_ok(), "x=1@z0 (eastern hemisphere) must be Ok");
        assert!(TileCoord::new(0, 2, 0, 16).is_err()); // x = 2^(0+1) Err
    }

    #[test]
    fn geodetic_boundaries_at_higher_zoom() {
        // z=6: 2^7 = 128 columns, 2^6 = 64 rows.
        assert!(TileCoord::new(6, 127, 0, 16).is_ok()); // x = 2^(z+1)-1 Ok
        assert!(TileCoord::new(6, 128, 0, 16).is_err()); // x = 2^(z+1) Err
        assert!(TileCoord::new(6, 0, 63, 16).is_ok()); // y = 2^z-1 Ok
        assert!(TileCoord::new(6, 0, 64, 16).is_err()); // y = 2^z Err
    }

    #[test]
    fn rejects_zoom_above_cap() {
        assert!(TileCoord::new(17, 0, 0, 16).is_err());
    }

    #[test]
    fn shift_is_panic_safe_for_absurd_max_zoom() {
        // Even if a caller passes an absurd max_zoom (≥ 32) so the zoom-cap check
        // passes, the `1u32 << z` bound computation must NOT panic — `checked_shl`
        // turns it into an error instead. Guards against shift-overflow panics.
        assert!(TileCoord::new(32, 0, 0, 40).is_err());
        assert!(TileCoord::new(64, 0, 0, 64).is_err());
        assert!(TileCoord::from_parts("32", "0", "0", 40).is_err());
    }

    #[test]
    fn from_str_rejects_non_integer_and_huge_zoom() {
        assert!(TileCoord::from_parts("..", "0", "0", 16).is_err());
        assert!(TileCoord::from_parts("3", "-1", "0", 16).is_err());
        assert!(TileCoord::from_parts("3", "x", "0", 16).is_err());
        // adversarial z far above cap — must reject BEFORE any 2^z is computed
        // (else `2u32.pow(40)` panics on overflow). Webview-supplied.
        assert!(TileCoord::from_parts("40", "0", "0", 16).is_err());
    }

    // --- Task 2.2 tests (paste-verbatim per plan) ---

    #[test]
    #[allow(clippy::identity_op)] // `- 0` is kept to mirror the spec formula exactly
    fn tms_flip_is_consistent_and_in_range() {
        let c = TileCoord::new(2, 1, 0, 16).unwrap();
        assert_eq!(c.upstream_y(/*tms*/ true), (1 << 2) - 1 - 0); // 3
        assert_eq!(c.upstream_y(false), 0);
    }

    #[test]
    fn rel_path_is_integers_only() {
        let c = TileCoord::new(3, 5, 2, 16).unwrap();
        assert_eq!(c.rel_path(false), std::path::PathBuf::from("3/5/2.tile"));
    }
}
