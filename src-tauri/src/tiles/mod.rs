//! `tiles` — LAN map-tile subsystem.
//!
//! Spec: docs/superpowers/specs/tuxlink-dyop-lan-tiles-design.md
//!
//! Phase 1 ships the SSRF security boundary primitives:
//! - Shared domain types (this module)
//! - URL-shape validation + resolved-IP allow/deny (`host`)
//!
//! Fetch-time resolve-then-vet wiring is Phase 3's job.

pub mod breaker;
pub mod cache;
pub mod commands;
pub mod coord;
pub mod crs;
pub mod fetch;
pub mod host;
pub mod serve;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};
use std::time::Instant;

use breaker::{BreakerHealth, CircuitBreaker, Outcome};

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
    /// Phase-9 source-level circuit breaker (§8.5). Tracks consecutive host
    /// failures for the active source so [`serve::serve_tile`] can fast-fail to
    /// "serve bundled" while the source is down, instead of issuing a per-tile
    /// timeout storm. A [`Mutex`] (not `RwLock`) because every consultation may
    /// mutate the phase (`should_attempt` transitions `Degraded → Probing`), and
    /// the critical section is a few enum-field comparisons — uncontended.
    ///
    /// Construction performs NO network I/O: the breaker starts `Live` and
    /// engages only as outcomes are recorded during serving (§8.5 "no
    /// synchronous network on startup/mount").
    breaker: Mutex<CircuitBreaker>,
    /// Whether the active source has returned a coverage-gap 404 above its
    /// raster-native zoom (§8.5 `partial`). Set on an [`Outcome::Coverage`],
    /// cleared on a fresh success/host-failure transition or a source change.
    /// Surfaced as [`StatusKind::Partial`] when the source is otherwise live.
    partial_coverage: Mutex<bool>,
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
            breaker: Mutex::new(CircuitBreaker::new()),
            partial_coverage: Mutex::new(false),
        }
    }

    /// Set (or clear, with `None`) the active source.
    ///
    /// Changing the source resets the breaker and the partial-coverage flag: the
    /// new source's health is unrelated to the old one's failure history, so a
    /// stale `Degraded` must not carry over.
    pub fn set_source(&self, source: Option<TileSource>) {
        let mut guard = self
            .active
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = source;
        *self.lock_breaker() = CircuitBreaker::new();
        *self.lock_partial() = false;
    }

    /// Lock the breaker, recovering from poisoning (a panic mid-transition must
    /// not wedge every later tile request).
    fn lock_breaker(&self) -> std::sync::MutexGuard<'_, CircuitBreaker> {
        self.breaker
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Lock the partial-coverage flag, recovering from poisoning.
    fn lock_partial(&self) -> std::sync::MutexGuard<'_, bool> {
        self.partial_coverage
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Should a per-tile fetch be attempted right now, given the breaker state?
    ///
    /// `serve_tile` consults this BEFORE any network I/O: a `false` means the
    /// source is degraded and cooling, so the caller short-circuits to "serve
    /// bundled" instead of issuing a fetch that would just time out. `now` is
    /// the injected clock instant (production: `Instant::now()`).
    pub(crate) fn breaker_should_attempt(&self, now: Instant) -> bool {
        self.lock_breaker().should_attempt(now)
    }

    /// Feed a fetch outcome back to the breaker and update the partial flag.
    ///
    /// A [`Outcome::Coverage`] sets the partial flag (a coverage-gap 404); any
    /// other outcome clears it (the source either served a real tile or failed
    /// at the host level, neither of which is a partial-coverage view).
    pub(crate) fn breaker_record(&self, outcome: Outcome, now: Instant) {
        self.lock_breaker().record(outcome, now);
        *self.lock_partial() = matches!(outcome, Outcome::Coverage);
    }

    /// The breaker's current health AT `now` (for the status surface).
    pub(crate) fn breaker_health(&self, now: Instant) -> BreakerHealth {
        self.lock_breaker().health(now)
    }

    /// Whether the active source last reported a coverage-gap 404 (§8.5
    /// `partial`).
    pub(crate) fn is_partial_coverage(&self) -> bool {
        *self.lock_partial()
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
