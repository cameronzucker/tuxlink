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

/// A validated tile coordinate.
///
/// Construction via [`TileCoord::new`] or [`TileCoord::from_parts`] enforces:
/// - `z ≤ max_zoom` (checked **first** to prevent `1u32 << z` overflow for huge
///   adversarial zoom values before the bound is computed)
/// - `x < 2^z` and `y < 2^z`
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
    /// The zoom cap is checked **before** the `1u32 << z` shift to ensure that an
    /// adversarial `z` (e.g. 40) cannot cause a panic via overflow.
    pub fn new(z: u32, x: u32, y: u32, max_zoom: u32) -> Result<TileCoord, String> {
        if z > max_zoom {
            return Err(format!(
                "zoom {z} exceeds max_zoom {max_zoom}"
            ));
        }
        // `z` is now `≤ max_zoom`, but a caller could pass an absurd `max_zoom`
        // (≥ 32). `checked_shl` returns `None` for a shift `≥ 32` instead of
        // panicking, so the bound computation is panic-safe regardless of what
        // `max_zoom` the caller supplies (defense-in-depth — the config-time
        // cap is NOT relied on for this primitive's panic-safety).
        let bound = 1u32
            .checked_shl(z)
            .ok_or_else(|| format!("zoom {z} too large to compute a tile bound"))?;
        if x >= bound {
            return Err(format!("x {x} out of range for zoom {z} (bound {bound})"));
        }
        if y >= bound {
            return Err(format!("y {y} out of range for zoom {z} (bound {bound})"));
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
    fn rejects_x_y_out_of_2_pow_z() {
        assert!(TileCoord::new(1, 2, 0, 16).is_err()); // x must be < 2^1 = 2
        assert!(TileCoord::new(0, 0, 1, 16).is_err()); // y must be < 2^0 = 1
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
