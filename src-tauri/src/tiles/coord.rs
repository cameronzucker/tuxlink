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
        // z ≤ max_zoom ≤ 30 (practical limit); 1u32 << z is now safe.
        let bound = 1u32 << z;
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
    fn from_str_rejects_non_integer_and_huge_zoom() {
        assert!(TileCoord::from_parts("..", "0", "0", 16).is_err());
        assert!(TileCoord::from_parts("3", "-1", "0", 16).is_err());
        assert!(TileCoord::from_parts("3", "x", "0", 16).is_err());
        // adversarial z far above cap — must reject BEFORE any 2^z is computed
        // (else `2u32.pow(40)` panics on overflow). Webview-supplied.
        assert!(TileCoord::from_parts("40", "0", "0", 16).is_err());
    }
}
