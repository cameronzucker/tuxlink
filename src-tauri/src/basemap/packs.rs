//! Region-pack coverage math + installed-pack model (tuxlink-ndi4, phase 4).
//!
//! Design: docs/design/2026-06-13-ndi4-d1-region-pack-distribution.md
//!
//! This module is the *pure* half of pack management: turning an operator grid
//! centroid + a tier into a validated bounding box, deriving a filesystem-safe
//! pack id, and the on-disk installed-pack manifest shape. The filesystem +
//! sidecar half (download, validate, atomic install, orphan sweep) lives in
//! [`super::download`]; keeping the math here makes the security-critical bbox /
//! id logic unit-testable without touching disk or spawning a process.
//!
//! SECURITY: a [`Bbox`] is only ever constructed through [`tier_bbox`] /
//! [`continent_bbox`], which clamp to the valid lon/lat domain and reject any
//! degenerate (zero/negative-width) box — so the `W,S,E,N` string that reaches
//! the go-pmtiles sidecar argv is always four finite, ordered, in-range numbers.
//! A pack id is only ever used in a path or `tile://pmtiles/<id>` URL after
//! [`is_safe_pack_id`] confirms it is `[a-z0-9-]+`, blocking traversal.

use serde::{Deserialize, Serialize};

/// Web-mercator latitude clamp — tiles do not exist beyond ±85.0511°; the project
/// uses ±85 throughout (matches the bundle build bbox).
const LAT_LIMIT: f64 = 85.0;
const LON_LIMIT: f64 = 180.0;

/// A validated `[west, south, east, north]` bounding box, all finite, in range,
/// with `west < east` and `south < north`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bbox {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
}

/// Why a bbox could not be formed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BboxError {
    /// An input was NaN/Infinity.
    NotFinite,
    /// After clamping, the box had zero or negative width/height (e.g. a centroid
    /// exactly on a pole, or a half-width of 0).
    Degenerate,
}

impl std::fmt::Display for BboxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BboxError::NotFinite => write!(f, "bbox input was not finite"),
            BboxError::Degenerate => write!(f, "bbox collapsed to zero/negative area after clamping"),
        }
    }
}

impl std::error::Error for BboxError {}

impl Bbox {
    /// The `--bbox=W,S,E,N` argument value for go-pmtiles. Fixed 6-decimal
    /// formatting — no locale, no scientific notation, no `inf`/`NaN` (a `Bbox`
    /// can only hold finite, ordered values).
    pub fn to_arg(&self) -> String {
        format!(
            "{:.6},{:.6},{:.6},{:.6}",
            self.west, self.south, self.east, self.north
        )
    }
}

/// Build a tier box centered on `(lon0, lat0)` with the tier's `[half_lon, half_lat]`.
///
/// Longitude and latitude are clamped to the valid domain (`±180` / `±85`). A box
/// that would cross the antimeridian is **truncated** at ±180 rather than wrapped
/// — acceptable for v1 (an operator within a half-width of the dateline gets a
/// slightly asymmetric box; mid-Pacific stations are rare). The result is rejected
/// as [`BboxError::Degenerate`] only in the pathological case where clamping
/// collapses the box (a centroid on a pole / at exactly ±180 with the clamp eating
/// the whole span), which cannot happen for a real grid centroid.
pub fn tier_bbox(lon0: f64, lat0: f64, half_lon: f64, half_lat: f64) -> Result<Bbox, BboxError> {
    if ![lon0, lat0, half_lon, half_lat].iter().all(|v| v.is_finite()) {
        return Err(BboxError::NotFinite);
    }
    let west = (lon0 - half_lon).clamp(-LON_LIMIT, LON_LIMIT);
    let east = (lon0 + half_lon).clamp(-LON_LIMIT, LON_LIMIT);
    let south = (lat0 - half_lat).clamp(-LAT_LIMIT, LAT_LIMIT);
    let north = (lat0 + half_lat).clamp(-LAT_LIMIT, LAT_LIMIT);
    if west >= east || south >= north {
        return Err(BboxError::Degenerate);
    }
    Ok(Bbox { west, south, east, north })
}

/// Build a continent box from a manifest `[w, s, e, n]` array, re-validating
/// (the manifest is also validated at parse time, but this is the single
/// construction point so the invariant holds regardless of caller).
pub fn continent_bbox(bbox: [f64; 4]) -> Result<Bbox, BboxError> {
    let [west, south, east, north] = bbox;
    if !bbox.iter().all(|v| v.is_finite()) {
        return Err(BboxError::NotFinite);
    }
    if west >= east
        || south >= north
        || !(-LON_LIMIT..=LON_LIMIT).contains(&west)
        || !(-LON_LIMIT..=LON_LIMIT).contains(&east)
        || !(-LAT_LIMIT..=LAT_LIMIT).contains(&south)
        || !(-LAT_LIMIT..=LAT_LIMIT).contains(&north)
    {
        return Err(BboxError::Degenerate);
    }
    Ok(Bbox { west, south, east, north })
}

/// True iff `id` is safe to use as a filename stem and a `tile://pmtiles/<id>`
/// path segment: non-empty, `[a-z0-9-]+`, length-bounded, no leading/trailing/
/// doubled dash. Blocks `..`, `/`, `%`, and any path-traversal or scheme trickery.
pub fn is_safe_pack_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && !id.starts_with('-')
        && !id.ends_with('-')
        && !id.contains("--")
        && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Compass-tagged integer-degree token for a coordinate (e.g. `n33`, `w112`).
/// Avoids the `.`/`-` characters a signed decimal would put in a pack id.
fn coord_token(value: f64, positive: char, negative: char) -> String {
    let mag = value.abs().round() as u32;
    let dir = if value < 0.0 { negative } else { positive };
    format!("{dir}{mag}")
}

/// Deterministic, filesystem-safe id for a tier pack centered on `(lon0, lat0)`.
/// Two requests for the same tier at the same rounded location yield the same id
/// (idempotent re-download), e.g. `tier-wide-n33-w112`. The caller still asserts
/// [`is_safe_pack_id`] before any filesystem use (defence in depth — `tier_id`
/// originates in the manifest).
pub fn tier_pack_id(tier_id: &str, lon0: f64, lat0: f64) -> String {
    let lat_tok = coord_token(lat0, 'n', 's');
    let lon_tok = coord_token(lon0, 'e', 'w');
    format!("tier-{tier_id}-{lat_tok}-{lon_tok}")
}

/// Deterministic id for a named continent pack, e.g. `continent-na`.
pub fn continent_pack_id(continent_id: &str) -> String {
    format!("continent-{continent_id}")
}

/// One installed, registered region pack. Persisted in the app-data packs
/// manifest (`packs/manifest.json`); written only AFTER the archive is validated
/// and atomically renamed into place (see [`super::download`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstalledPack {
    pub id: String,
    pub label: String,
    /// `[west, south, east, north]`.
    pub bbox: [f64; 4],
    pub minzoom: u8,
    pub maxzoom: u8,
    /// Protomaps schema version recorded at install (metadata `version`).
    pub schema_version: String,
    pub bytes: u64,
    /// The planet build the pack was extracted from.
    pub source_build: String,
    /// RFC3339 UTC install timestamp.
    pub installed_at: String,
}

/// The app-data packs manifest: every installed pack. Total disk used by packs is
/// the sum of `bytes`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PacksManifest {
    #[serde(default)]
    pub packs: Vec<InstalledPack>,
}

impl PacksManifest {
    /// Total bytes across all installed packs (for the manager's disk-used display).
    pub fn total_bytes(&self) -> u64 {
        self.packs.iter().map(|p| p.bytes).sum()
    }

    /// Insert or replace the pack with `entry.id` (a re-download of the same area
    /// replaces, never duplicates).
    pub fn upsert(&mut self, entry: InstalledPack) {
        if let Some(existing) = self.packs.iter_mut().find(|p| p.id == entry.id) {
            *existing = entry;
        } else {
            self.packs.push(entry);
        }
    }

    /// Remove the pack with `id`; returns it if present.
    pub fn remove(&mut self, id: &str) -> Option<InstalledPack> {
        let idx = self.packs.iter().position(|p| p.id == id)?;
        Some(self.packs.remove(idx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── bbox math ──────────────────────────────────────────────────────────────

    #[test]
    fn tier_bbox_centers_and_orders() {
        // Phoenix-ish DM43 centroid, Wide tier.
        let b = tier_bbox(-112.0, 33.5, 7.5, 6.0).unwrap();
        assert_eq!(b.west, -119.5);
        assert_eq!(b.east, -104.5);
        assert_eq!(b.south, 27.5);
        assert_eq!(b.north, 39.5);
        assert!(b.west < b.east && b.south < b.north);
    }

    #[test]
    fn tier_bbox_clamps_latitude_near_pole() {
        // High-latitude operator: north clamps to +85, box stays ordered.
        let b = tier_bbox(20.0, 82.0, 7.5, 6.0).unwrap();
        assert_eq!(b.north, 85.0);
        assert!(b.south < b.north);
    }

    #[test]
    fn tier_bbox_clamps_longitude_near_dateline() {
        // Operator near +180: east truncates to 180, west stays in range, ordered.
        let b = tier_bbox(178.0, 0.0, 7.5, 6.0).unwrap();
        assert_eq!(b.east, 180.0);
        assert!(b.west < b.east);
        assert!((-180.0..=180.0).contains(&b.west));
    }

    #[test]
    fn tier_bbox_rejects_nonfinite() {
        assert_eq!(tier_bbox(f64::NAN, 0.0, 7.5, 6.0), Err(BboxError::NotFinite));
        assert_eq!(tier_bbox(0.0, f64::INFINITY, 7.5, 6.0), Err(BboxError::NotFinite));
    }

    #[test]
    fn tier_bbox_rejects_zero_halfwidth() {
        assert_eq!(tier_bbox(-112.0, 33.5, 0.0, 6.0), Err(BboxError::Degenerate));
        assert_eq!(tier_bbox(-112.0, 33.5, 7.5, 0.0), Err(BboxError::Degenerate));
    }

    #[test]
    fn tier_bbox_at_pole_is_degenerate_not_a_bad_arg() {
        // Pathological centroid exactly at the lat limit: south and north both
        // clamp to 85 → degenerate, rejected (never a zero-height sidecar arg).
        assert_eq!(tier_bbox(0.0, 85.0, 7.5, 6.0), Err(BboxError::Degenerate));
    }

    #[test]
    fn continent_bbox_accepts_manifest_box() {
        let b = continent_bbox([-170.0, 5.0, -50.0, 84.0]).unwrap();
        assert_eq!(b.west, -170.0);
        assert_eq!(b.north, 84.0);
    }

    #[test]
    fn continent_bbox_rejects_inverted_or_out_of_range() {
        assert_eq!(continent_bbox([-50.0, 5.0, -170.0, 84.0]), Err(BboxError::Degenerate));
        assert_eq!(continent_bbox([-170.0, 5.0, -50.0, 999.0]), Err(BboxError::Degenerate));
    }

    #[test]
    fn bbox_to_arg_is_fixed_decimal() {
        let b = tier_bbox(-112.5, 32.25, 15.0, 12.0).unwrap();
        assert_eq!(b.to_arg(), "-127.500000,20.250000,-97.500000,44.250000");
    }

    // ── pack id safety ───────────────────────────────────────────────────────────

    #[test]
    fn tier_pack_id_is_deterministic_and_safe() {
        let id = tier_pack_id("wide", -112.0, 33.5);
        assert_eq!(id, "tier-wide-n34-w112"); // 33.5 rounds to 34
        assert!(is_safe_pack_id(&id));
        // Same rounded location + tier → same id (idempotent).
        assert_eq!(tier_pack_id("wide", -112.3, 33.5), id);
    }

    #[test]
    fn continent_pack_id_is_safe() {
        let id = continent_pack_id("na");
        assert_eq!(id, "continent-na");
        assert!(is_safe_pack_id(&id));
    }

    #[test]
    fn is_safe_pack_id_blocks_traversal_and_tricks() {
        assert!(!is_safe_pack_id(""));
        assert!(!is_safe_pack_id(".."));
        assert!(!is_safe_pack_id("a/b"));
        assert!(!is_safe_pack_id("../etc/passwd"));
        assert!(!is_safe_pack_id("a.pmtiles"));
        assert!(!is_safe_pack_id("UPPER"));
        assert!(!is_safe_pack_id("trailing-"));
        assert!(!is_safe_pack_id("-leading"));
        assert!(!is_safe_pack_id("double--dash"));
        assert!(!is_safe_pack_id(&"x".repeat(65)));
        assert!(is_safe_pack_id("tier-wide-n34-w112"));
        assert!(is_safe_pack_id("continent-na"));
    }

    // ── installed-pack manifest ───────────────────────────────────────────────────

    fn pack(id: &str, bytes: u64) -> InstalledPack {
        InstalledPack {
            id: id.to_string(),
            label: id.to_string(),
            bbox: [-119.5, 27.5, -104.5, 39.5],
            minzoom: 0,
            maxzoom: 14,
            schema_version: "3.7.1".to_string(),
            bytes,
            source_build: "20260608".to_string(),
            installed_at: "2026-06-13T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn manifest_upsert_replaces_same_id() {
        let mut m = PacksManifest::default();
        m.upsert(pack("tier-wide-n34-w112", 1000));
        m.upsert(pack("tier-wide-n34-w112", 2000)); // re-download, same area
        assert_eq!(m.packs.len(), 1);
        assert_eq!(m.packs[0].bytes, 2000);
    }

    #[test]
    fn manifest_total_bytes_and_remove() {
        let mut m = PacksManifest::default();
        m.upsert(pack("a", 1000));
        m.upsert(pack("b", 2500));
        assert_eq!(m.total_bytes(), 3500);
        let removed = m.remove("a").unwrap();
        assert_eq!(removed.id, "a");
        assert_eq!(m.total_bytes(), 2500);
        assert!(m.remove("missing").is_none());
    }

    #[test]
    fn manifest_round_trips_json() {
        let mut m = PacksManifest::default();
        m.upsert(pack("continent-na", 30_000_000_000));
        let json = serde_json::to_string(&m).unwrap();
        let back: PacksManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }
}
