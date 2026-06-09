//! `tiles` — LAN map-tile subsystem.
//!
//! Spec: docs/superpowers/specs/tuxlink-dyop-lan-tiles-design.md
//!
//! Phase 1 ships the SSRF security boundary primitives:
//! - Shared domain types (this module)
//! - URL-shape validation + resolved-IP allow/deny (`host`)
//!
//! Fetch-time resolve-then-vet wiring is Phase 3's job.

pub mod cache;
pub mod commands;
pub mod coord;
pub mod crs;
pub mod fetch;
pub mod host;
pub mod serve;

use serde::{Deserialize, Serialize};

/// Describes a tile source configured by the operator.
/// `NO` auth field by design — credentials on tile URLs are rejected by
/// `host::validate_source_url` (embedded credentials are an SSRF escalation path).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TileSource {
    /// Base URL of the tile server (e.g. `http://192.168.1.5:8080/tiles/`).
    pub url: String,
    /// Coordinate reference system this source uses.
    pub crs: Crs,
    /// Tile-numbering scheme.
    pub scheme: TileScheme,
    pub min_zoom: u32,
    pub max_zoom: u32,
    /// Local cache budget in MiB.
    pub cache_budget_mb: u64,
    /// Optional attribution string rendered on the map.
    pub attribution: Option<String>,
    /// Short operator-visible label ("shack", "field kit", …).
    pub label: String,
}

/// Coordinate Reference System for a tile source.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Crs {
    /// EPSG:4326 — standard GPS / Winlink grid-square mapping.
    Geodetic,
}

/// Tile URL numbering scheme.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum TileScheme {
    /// `{z}/{x}/{y}` — origin top-left (OSM convention).
    Xyz,
    /// `{z}/{x}/{y}` — origin bottom-left (TMS convention).
    Tms,
}

/// Runtime status of a tile source as reported to the frontend.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TileSourceStatus {
    pub kind: StatusKind,
    /// Current zoom level being displayed.
    pub zoom: u32,
    /// Operator label for the source, if known.
    pub label: Option<String>,
    /// ISO-8601 timestamp of the last successful cache write, if any.
    pub cached_at: Option<String>,
}

/// Availability kind for a `TileSourceStatus`.
///
/// Serializes with `kebab-case` to match the TypeScript union expected by
/// the frontend (`"lan-live"`, `"lan-cached"`, etc.).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum StatusKind {
    /// Bundled offline base map (always available).
    Bundled,
    /// LAN tile server reachable and serving live tiles.
    LanLive,
    /// LAN tile server unreachable; serving from local cache.
    LanCached,
    /// Cache partially covers the viewport.
    Partial,
    /// LAN tile server unreachable and no cache available.
    Unreachable,
    /// Server responded but tile format is incompatible.
    Incompatible,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_source_and_status_serde_round_trip() {
        let s = TileSource {
            url: "http://192.168.1.5:8080/".into(),
            crs: Crs::Geodetic,
            scheme: TileScheme::Xyz,
            min_zoom: 0,
            max_zoom: 16,
            cache_budget_mb: 384,
            attribution: None,
            label: "shack".into(),
        };
        let j = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<TileSource>(&j).unwrap().label, "shack");
        let st = TileSourceStatus {
            kind: StatusKind::LanLive,
            zoom: 13,
            label: Some("shack".into()),
            cached_at: None,
        };
        let _ = serde_json::to_string(&st).unwrap();
    }
}
