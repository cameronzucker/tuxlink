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

use std::path::{Path, PathBuf};
use std::sync::RwLock;

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

/// Tauri-managed serving state for the `tile` URI scheme.
///
/// Holds the single active [`TileSource`] (set by the Phase-8 configure command)
/// and the on-disk cache root. The `tile`-scheme handler ([`serve::serve_tile`])
/// reads the active source under a read lock and delegates to the SSRF-guarded
/// fetch pipeline; only integer `{z}/{x}/{y}` derived from the webview's tile
/// request ever reach this state, never a caller-supplied URL.
///
/// Interior mutability via [`RwLock`] is required because the gatekeeper is
/// shared Tauri state accessed behind `&self`. The `RwLock<Option<…>>` + owned
/// `PathBuf` make this `Send + Sync + 'static`, satisfying `.manage()`.
pub struct TileGatekeeper {
    /// The currently configured source, if any. `None` = no source configured
    /// (serving returns [`serve::ServeError::NoSource`], never a panic).
    active: RwLock<Option<TileSource>>,
    /// Root directory of the on-disk tile cache. Resolved once at construction
    /// (typically `<app_data_dir>/tile-cache`); never mutated.
    cache_root: PathBuf,
    // Phase 9 (circuit breaker): a breaker-state field lands here — e.g.
    // `breaker: RwLock<BreakerState>` tracking consecutive upstream failures
    // so the gatekeeper can fast-fail to cache-only while a source is down.
    // Intentionally NOT implemented now; documented placeholder only.
}

impl TileGatekeeper {
    /// Construct a gatekeeper rooted at `cache_root` with NO active source.
    ///
    /// Does NO network or filesystem I/O — it only stores the path and an empty
    /// source slot. The cache directory is created lazily by the cache layer on
    /// first write.
    pub fn new(cache_root: impl Into<PathBuf>) -> Self {
        TileGatekeeper {
            active: RwLock::new(None),
            cache_root: cache_root.into(),
        }
    }

    /// Set (or clear, with `None`) the active source.
    pub fn set_source(&self, source: Option<TileSource>) {
        let mut guard = self
            .active
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = source;
    }

    /// Return a clone of the active source, or `None` if no source is configured.
    pub fn active_source(&self) -> Option<TileSource> {
        self.active
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// The on-disk cache root for this gatekeeper.
    pub fn cache_root(&self) -> &Path {
        &self.cache_root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_source() -> TileSource {
        TileSource {
            url: "http://192.168.1.5:8080/tiles/".into(),
            crs: Crs::Geodetic,
            scheme: TileScheme::Xyz,
            min_zoom: 0,
            max_zoom: 16,
            cache_budget_mb: 384,
            attribution: None,
            label: "shack".into(),
        }
    }

    #[test]
    fn gatekeeper_new_has_no_active_source() {
        let gk = TileGatekeeper::new("/tmp/does-not-need-to-exist/tile-cache");
        // No source configured on construction; no I/O performed.
        assert!(gk.active_source().is_none());
        assert_eq!(gk.cache_root(), Path::new("/tmp/does-not-need-to-exist/tile-cache"));
    }

    #[test]
    fn gatekeeper_set_and_clear_source() {
        let gk = TileGatekeeper::new("/tmp/tile-cache");
        let src = sample_source();
        gk.set_source(Some(src.clone()));
        assert_eq!(gk.active_source().as_ref(), Some(&src));
        gk.set_source(None);
        assert!(gk.active_source().is_none());
    }

    #[test]
    fn gatekeeper_exposes_cache_root() {
        let gk = TileGatekeeper::new("/var/cache/tuxlink/tiles");
        assert_eq!(gk.cache_root(), Path::new("/var/cache/tuxlink/tiles"));
    }

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
