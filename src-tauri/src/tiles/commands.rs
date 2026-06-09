//! `tiles::commands` — the Tauri command surface for the LAN map-tile feature
//! (tuxlink-dyop Phase 8.1).
//!
//! Four commands the frontend (Task 7.1) calls verbatim:
//!
//! | Command                | Args                  | Returns            |
//! |------------------------|-----------------------|--------------------|
//! | `configure_tile_source`| `{ source }`          | `TileSourceStatus` |
//! | `test_tile_source`     | `{ source }`          | `TileSourceStatus` |
//! | `clear_tile_cache`     | `{}`                  | `()`               |
//! | `tile_source_status`   | `{}`                  | `TileSourceStatus` |
//!
//! ## Activation policy (§8.1 / §4.1 / §8.3)
//!
//! [`validate`] is the plain-async core (cache root + allow_loopback) that
//! both `configure` and `test` delegate to. In order:
//!
//! 1. **URL shape** ([`validate_source_url`]) — a malformed URL, non-http(s)
//!    scheme, embedded creds, or missing host yields [`StatusKind::Incompatible`]
//!    and does NOT activate. This is the cheap first gate before any network I/O.
//! 2. **CRS probe** ([`probe_source_crs`]):
//!    - `Rejected` (Mercator) → [`StatusKind::Incompatible`] — never activate; a
//!      Mercator source renders plausible-but-WRONG coordinates (§4.1 ship-blocker).
//!    - `Unknown` → [`StatusKind::Incompatible`] UNLESS the operator set the
//!      explicit `source.crs == Crs::Geodetic` override. `Crs` has only the
//!      `Geodetic` variant, so the field's presence IS the explicit operator
//!      assertion "this server is geodetic but exposes no probeable metadata"
//!      (§4.1 caller note in `crs.rs`). With the override present, `Unknown`
//!      proceeds; without it, `Unknown` rejects. (`Crs::Geodetic` is currently
//!      the only variant, so the override always holds — by design: the field
//!      exists to be widened later, and the policy is written to honor it now.)
//!    - `Geodetic` → proceed.
//! 3. **Reachability probe** — fetch ONE tile (`{z=0, x=0, y=0}` at the source's
//!    `max_zoom` bound) via [`fetch_tile_single_flight`] with `allow_loopback`
//!    threaded through (production passes `false`; tests pass `true` to reach a
//!    mockito loopback server):
//!    - `Ok` → reachable + serves a real image → [`StatusKind::LanLive`].
//!    - `HostDenied` / `Network` (covers DNS + timeout) → [`StatusKind::Unreachable`].
//!    - `NotFound` (the probe tile is simply absent on an otherwise-reachable
//!      server) → [`StatusKind::LanLive`]: the host answered and the CRS already
//!      validated; a missing `0/0/0` tile is not an incompatibility, only a
//!      coverage gap the cache/serve layer handles per-tile. We do NOT reject a
//!      working geodetic server because its world-tile is absent.
//!    - `NotAnImage` / `TooLarge` / `Redirect` / `Status` / `BadUrl` →
//!      [`StatusKind::Incompatible`]: the server responded but the response is
//!      not a usable tile (wrong content, oversized, a redirect pivot, an error
//!      status, or an un-buildable URL) — a compatibility problem, not a
//!      reachability one.
//!
//! `configure_tile_source` activates + persists ONLY on `LanLive`. `test_tile_source`
//! never activates or persists (dry-run). `tile_source_status` does NO network I/O
//! (§8.5 "no synchronous network on startup/mount"): it reflects the gatekeeper's
//! `active_source()` — `Bundled` when `None`, `LanLive` when `Some`.

use std::sync::Arc;

use super::cache;
use super::coord::TileCoord;
use super::crs::{probe_source_crs, CrsCheck};
use super::fetch::{fetch_tile_single_flight, FetchError};
use super::host::validate_source_url;
use super::{Crs, StatusKind, TileGatekeeper, TileSource, TileSourceStatus};
use crate::config::{read_config, write_config_atomic, ConfigWriteError};
use crate::ui_commands::UiError;

/// Build a [`TileSourceStatus`] of the given `kind` for `source`. `zoom` is the
/// validated zoom (the source `max_zoom` on `LanLive`, else `0`); `label` echoes
/// the operator's source label so the UI can name what it just (failed to)
/// configure. `cached_at` is always `None` here — the validation path performs a
/// LIVE probe, not a cache read; the serving layer is what surfaces cache state.
fn status(kind: StatusKind, source: &TileSource, zoom: u32) -> TileSourceStatus {
    TileSourceStatus {
        kind,
        zoom,
        label: Some(source.label.clone()),
        cached_at: None,
    }
}

/// Validate a candidate [`TileSource`] and return the [`TileSourceStatus`] it
/// earns, WITHOUT mutating any state. The plain-async core both `configure` and
/// `test` call (the `#[tauri::command]` wrappers are thin `State` extractors).
///
/// `cache_root` + `allow_loopback` are threaded into BOTH the CRS metadata probe
/// (which builds its own vetted/pinned/no-proxy client internally — Findings 1+2)
/// and the single reachability fetch. See the module-level docs for the full
/// branch table.
async fn validate(
    cache_root: &std::path::Path,
    source: &TileSource,
    allow_loopback: bool,
) -> TileSourceStatus {
    // 1. URL shape — cheapest gate, no network.
    if validate_source_url(&source.url).is_err() {
        return status(StatusKind::Incompatible, source, 0);
    }

    // 2. CRS probe + the Unknown-with-override policy (§4.1).
    //
    // SSRF (Findings 1+2): `probe_source_crs` now builds the SAME vetted+pinned+
    // no-proxy client the reachability fetch uses, vetting the host BEFORE any
    // probe GET — `allow_loopback` is threaded through so a public-IP source is
    // denied before the probe can connect (it returns Unknown → rejected below).
    match probe_source_crs(source, allow_loopback).await {
        CrsCheck::Rejected => return status(StatusKind::Incompatible, source, 0),
        CrsCheck::Unknown => {
            // `Crs::Geodetic` is the explicit operator override for a
            // known-geodetic server with no probeable metadata. The enum has
            // only this variant, so the match below always proceeds — written
            // as a match so widening `Crs` later forces a deliberate decision.
            match source.crs {
                Crs::Geodetic => { /* operator asserts geodetic — proceed */ }
            }
        }
        CrsCheck::Geodetic => { /* proceed */ }
    }

    // 3. Reachability probe — fetch ONE world tile (0/0/0 at the source bound).
    // `TileCoord::new(0,0,0, max_zoom)` is always valid (0 < 2^0 = 1) for any
    // max_zoom ≥ 0; the `.unwrap_or` keeps the fn total even on a degenerate
    // (pathological) coord build.
    let probe = match TileCoord::new(0, 0, 0, source.max_zoom) {
        Ok(c) => c,
        Err(_) => return status(StatusKind::Incompatible, source, 0),
    };
    match fetch_tile_single_flight(cache_root, source, &probe, allow_loopback).await {
        Ok(_) => status(StatusKind::LanLive, source, source.max_zoom),
        Err(FetchError::HostDenied(_)) | Err(FetchError::Network(_)) => {
            status(StatusKind::Unreachable, source, 0)
        }
        // The host answered but has no 0/0/0 tile — still a reachable, geodetic,
        // image-serving server (the CRS gate already passed). A missing world
        // tile is a coverage gap, not an incompatibility.
        Err(FetchError::NotFound) => status(StatusKind::LanLive, source, source.max_zoom),
        // Responded with something that is not a usable tile → incompatible.
        Err(FetchError::NotAnImage)
        | Err(FetchError::TooLarge)
        | Err(FetchError::Redirect)
        | Err(FetchError::Status(_))
        | Err(FetchError::BadUrl(_)) => status(StatusKind::Incompatible, source, 0),
    }
}

/// Core of [`configure_tile_source`]: validate, then on `LanLive` activate the
/// source on the gatekeeper AND persist it to the config (so it survives a
/// restart). On any non-`LanLive` outcome: do NOT activate, do NOT persist;
/// return the status so the UI can explain. Persistence failure is surfaced as
/// `Err` (the source DID validate; the operator should know the on-disk write
/// failed even though the in-memory gatekeeper is now live).
async fn configure_core(
    gatekeeper: &TileGatekeeper,
    source: &TileSource,
    allow_loopback: bool,
) -> Result<TileSourceStatus, UiError> {
    let st = validate(gatekeeper.cache_root(), source, allow_loopback).await;
    if st.kind == StatusKind::LanLive {
        gatekeeper.set_source(Some(source.clone()));
        persist_source(Some(source.clone()))?;
    }
    Ok(st)
}

/// Persist (or clear, with `None`) the active tile source into the config via
/// [`write_config_atomic`]. Reads the current config, sets `map_tile_source`,
/// and rewrites the whole `Config` (the writer's contract — it rewrites the
/// whole file). A missing/unreadable config (first-run before any config has
/// been written) is surfaced as an error: persisting a tile source presupposes
/// a configured app, and silently dropping the write would leave the on-disk
/// state diverged from the live gatekeeper.
fn persist_source(source: Option<TileSource>) -> Result<(), UiError> {
    let mut config = read_config().map_err(|e| UiError::Internal {
        detail: format!("cannot read config to persist tile source: {e}"),
    })?;
    config.map_tile_source = source;
    write_config_atomic(&config).map_err(|e: ConfigWriteError| UiError::Internal {
        detail: format!("cannot persist tile source to config: {e}"),
    })
}

/// Core of [`clear_tile_cache`]: empty the active source's cache subtree (if a
/// source is active). No active source → no-op `Ok(())` (nothing to clear). A
/// filesystem error during the clear is surfaced as `Err`.
fn clear_cache_core(gatekeeper: &TileGatekeeper) -> Result<(), UiError> {
    if let Some(source) = gatekeeper.active_source() {
        cache::clear(gatekeeper.cache_root(), &source).map_err(|e| UiError::Internal {
            detail: format!("cannot clear tile cache: {e}"),
        })?;
    }
    Ok(())
}

/// Core of [`tile_source_status`]: report the CURRENT status with NO network I/O
/// (§8.5). `Bundled` when no source is active; otherwise the active source's
/// status reflecting the §8.5 circuit-breaker phase:
///
/// - breaker `Degraded` (tripped + cooling) → [`StatusKind::Unreachable`]: the
///   source failed K consecutive host fetches and the breaker is suppressing
///   per-tile fetches, so the map is serving bundled.
/// - breaker `Live` with a recorded coverage-gap 404 → [`StatusKind::Partial`]:
///   the source is reachable but missing tiles above its raster-native zoom.
/// - breaker `Live`, no coverage gap → [`StatusKind::LanLive`].
///
/// This is a lightweight reflection of in-memory breaker state, NOT a re-probe —
/// a re-probe on every mount would violate the no-synchronous-network-on-startup
/// rule and stall the map (§8.5). The clock is read once here (`Instant::now()`)
/// so a cooldown that has elapsed reports `LanLive` (the source is re-probe-able
/// on the next tile request).
fn status_core(gatekeeper: &TileGatekeeper) -> TileSourceStatus {
    match gatekeeper.active_source() {
        None => TileSourceStatus {
            kind: StatusKind::Bundled,
            zoom: 0,
            label: None,
            cached_at: None,
        },
        Some(source) => {
            use crate::tiles::breaker::BreakerHealth;
            let kind = match gatekeeper.breaker_health(std::time::Instant::now()) {
                BreakerHealth::Degraded => StatusKind::Unreachable,
                BreakerHealth::Live if gatekeeper.is_partial_coverage() => StatusKind::Partial,
                BreakerHealth::Live => StatusKind::LanLive,
            };
            // Degraded/Partial: zoom 0 (no validated live ceiling to advertise);
            // LanLive: the source's validated max.
            let zoom = if kind == StatusKind::LanLive {
                source.max_zoom
            } else {
                0
            };
            status(kind, &source, zoom)
        }
    }
}

// ─── Tauri command wrappers (thin State extractors) ────────────────────────

/// Validate, activate, and persist a LAN tile source (operator "Use this source").
/// Returns the resulting [`TileSourceStatus`]; only `LanLive` activates+persists.
#[tauri::command]
pub async fn configure_tile_source(
    source: TileSource,
    gatekeeper: tauri::State<'_, Arc<TileGatekeeper>>,
) -> Result<TileSourceStatus, UiError> {
    // Production: never permit loopback (only tests opt in via the core fn). The
    // CRS probe + reachability fetch each build their own vetted/pinned/no-proxy
    // client internally (Findings 1+2); no shared client is threaded in.
    configure_core(gatekeeper.inner(), &source, false).await
}

/// Dry-run validation of a LAN tile source (operator "Test source"). Returns the
/// [`TileSourceStatus`] WITHOUT activating or persisting anything.
#[tauri::command]
pub async fn test_tile_source(
    source: TileSource,
    gatekeeper: tauri::State<'_, Arc<TileGatekeeper>>,
) -> Result<TileSourceStatus, UiError> {
    Ok(validate(gatekeeper.cache_root(), &source, false).await)
}

/// Empty the active source's on-disk tile cache. No-op when no source is active.
#[tauri::command]
pub async fn clear_tile_cache(
    gatekeeper: tauri::State<'_, Arc<TileGatekeeper>>,
) -> Result<(), UiError> {
    clear_cache_core(gatekeeper.inner())
}

/// Report the current tile-source status with NO network I/O (§8.5). `Bundled`
/// when no source is active; `LanLive` for the active source.
#[tauri::command]
pub async fn tile_source_status(
    gatekeeper: tauri::State<'_, Arc<TileGatekeeper>>,
) -> Result<TileSourceStatus, UiError> {
    Ok(status_core(gatekeeper.inner()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tiles::{TileScheme, TileSource};

    fn source(url: &str) -> TileSource {
        TileSource {
            url: url.into(),
            crs: Crs::Geodetic,
            scheme: TileScheme::Xyz,
            min_zoom: 0,
            max_zoom: 16,
            cache_budget_mb: 384,
            attribution: None,
            label: "shack".into(),
        }
    }

    fn png_bytes() -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
        v.extend_from_slice(&[0u8; 32]);
        v
    }

    /// A mockito server that serves a geodetic TileJSON at `/tilejson.json` and a
    /// PNG at the 0/0/0 probe tile (`/0/0/0.png`).
    async fn geodetic_png_server() -> mockito::ServerGuard {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/tilejson.json")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"tilejson":"3.0.0","crs":"EPSG:4326"}"#)
            .create_async()
            .await;
        server
            .mock("GET", "/0/0/0.png")
            .with_status(200)
            .with_header("content-type", "image/png")
            .with_body(png_bytes())
            .create_async()
            .await;
        server
    }

    // ── validate: LanLive happy path ───────────────────────────────────────

    #[tokio::test]
    async fn validate_geodetic_reachable_is_lan_live() {
        let server = geodetic_png_server().await;
        let cache = tempfile::tempdir().unwrap();
        let src = source(&server.url());
        // allow_loopback=true: mockito binds loopback.
        let st = validate(cache.path(), &src, true).await;
        assert_eq!(st.kind, StatusKind::LanLive, "got {st:?}");
        assert_eq!(st.zoom, 16);
        assert_eq!(st.label.as_deref(), Some("shack"));
    }

    // ── validate: Mercator → Incompatible, and configure does NOT activate ──

    #[tokio::test]
    async fn validate_mercator_is_incompatible() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/tilejson.json")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"tilejson":"3.0.0","crs":"EPSG:3857"}"#)
            .create_async()
            .await;
        let cache = tempfile::tempdir().unwrap();
        let src = source(&server.url());
        let st = validate(cache.path(), &src, true).await;
        assert_eq!(st.kind, StatusKind::Incompatible, "got {st:?}");
    }

    #[tokio::test]
    async fn configure_does_not_activate_on_incompatible() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/tilejson.json")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"tilejson":"3.0.0","crs":"EPSG:3857"}"#)
            .create_async()
            .await;
        let cache = tempfile::tempdir().unwrap();
        let gk = TileGatekeeper::new(cache.path());
        let src = source(&server.url());
        let st = configure_core(&gk, &src, true).await.unwrap();
        assert_eq!(st.kind, StatusKind::Incompatible, "got {st:?}");
        assert!(
            gk.active_source().is_none(),
            "incompatible source must NOT be activated on the gatekeeper"
        );
    }

    // ── validate: Unknown CRS + explicit Geodetic override → proceed ────────

    #[tokio::test]
    async fn validate_unknown_crs_with_geodetic_override_proceeds() {
        // No probeable metadata (all probes 404) → Unknown. The source carries
        // the explicit `crs: Geodetic` override, so validation proceeds to the
        // reachability probe and the 0/0/0 PNG yields LanLive.
        let mut server = mockito::Server::new_async().await;
        server.mock("GET", "/tilejson.json").with_status(404).create_async().await;
        server.mock("GET", "/").with_status(404).create_async().await;
        server
            .mock("GET", mockito::Matcher::Regex(r"SERVICE=WMTS".to_string()))
            .with_status(404)
            .create_async()
            .await;
        server.mock("GET", "/metadata.json").with_status(404).create_async().await;
        server.mock("GET", "/metadata").with_status(404).create_async().await;
        server
            .mock("GET", "/0/0/0.png")
            .with_status(200)
            .with_header("content-type", "image/png")
            .with_body(png_bytes())
            .create_async()
            .await;
        let cache = tempfile::tempdir().unwrap();
        let src = source(&server.url()); // crs == Geodetic (the override)
        let st = validate(cache.path(), &src, true).await;
        assert_eq!(
            st.kind,
            StatusKind::LanLive,
            "Unknown CRS + explicit Geodetic override must proceed: got {st:?}"
        );
    }

    // ── validate: unreachable / denied host → Unreachable ───────────────────

    #[tokio::test]
    async fn validate_loopback_denied_without_optin_is_unreachable() {
        // A loopback server with allow_loopback=false: the CRS probe's client is
        // the plain test client (which CAN reach loopback for the metadata GET),
        // but the reachability fetch routes through the SSRF gate with
        // allow_loopback=false → HostDenied → Unreachable.
        let server = geodetic_png_server().await;
        let cache = tempfile::tempdir().unwrap();
        let src = source(&server.url());
        let st = validate(cache.path(), &src, false).await;
        assert_eq!(
            st.kind,
            StatusKind::Unreachable,
            "loopback denied (no opt-in) must be Unreachable: got {st:?}"
        );
    }

    #[tokio::test]
    async fn validate_bad_url_is_incompatible() {
        let cache = tempfile::tempdir().unwrap();
        let mut src = source("not-a-url");
        src.url = "ftp://192.168.1.5/".into(); // non-http scheme
        let st = validate(cache.path(), &src, true).await;
        assert_eq!(st.kind, StatusKind::Incompatible, "got {st:?}");
    }

    // ── clear_tile_cache core ───────────────────────────────────────────────

    #[tokio::test]
    async fn clear_cache_empties_active_source_subtree() {
        let server = geodetic_png_server().await;
        let cache = tempfile::tempdir().unwrap();
        let gk = TileGatekeeper::new(cache.path());
        let src = source(&server.url());
        // Configure → activates + caches the probe tile.
        let st = configure_core(&gk, &src, true).await.unwrap();
        assert_eq!(st.kind, StatusKind::LanLive);
        let probe = TileCoord::new(0, 0, 0, src.max_zoom).unwrap();
        assert!(
            cache::get(cache.path(), &src, &probe).is_some(),
            "probe tile should be cached after configure"
        );
        clear_cache_core(&gk).unwrap();
        assert!(
            cache::get(cache.path(), &src, &probe).is_none(),
            "clear must empty the active source's cache subtree"
        );
    }

    #[tokio::test]
    async fn clear_cache_no_active_source_is_noop() {
        let cache = tempfile::tempdir().unwrap();
        let gk = TileGatekeeper::new(cache.path());
        // No source active → Ok, no panic, nothing to clear.
        clear_cache_core(&gk).unwrap();
    }

    // ── tile_source_status core: no network, Bundled vs LanLive ─────────────

    #[test]
    fn status_is_bundled_when_no_source() {
        let cache = tempfile::tempdir().unwrap();
        let gk = TileGatekeeper::new(cache.path());
        let st = status_core(&gk);
        assert_eq!(st.kind, StatusKind::Bundled);
        assert_eq!(st.label, None);
    }

    #[test]
    fn status_is_lan_live_when_source_active() {
        let cache = tempfile::tempdir().unwrap();
        let gk = TileGatekeeper::new(cache.path());
        let src = source("http://192.168.1.5:8080/tiles/");
        gk.set_source(Some(src.clone()));
        let st = status_core(&gk);
        assert_eq!(st.kind, StatusKind::LanLive);
        assert_eq!(st.zoom, 16);
        assert_eq!(st.label.as_deref(), Some("shack"));
    }

    // ── status_core reflects the §8.5 circuit-breaker phase (no network) ─────

    #[test]
    fn status_is_unreachable_when_breaker_degraded() {
        use crate::tiles::breaker::{Outcome, FAILURE_THRESHOLD};
        let cache = tempfile::tempdir().unwrap();
        let gk = TileGatekeeper::new(cache.path());
        gk.set_source(Some(source("http://192.168.1.5:8080/tiles/")));
        let now = std::time::Instant::now();
        // K consecutive host failures trip the breaker → status reflects it.
        for _ in 0..FAILURE_THRESHOLD {
            gk.breaker_record(Outcome::HostFailure, now);
        }
        let st = status_core(&gk);
        assert_eq!(
            st.kind,
            StatusKind::Unreachable,
            "degraded breaker → Unreachable: got {st:?}"
        );
        assert_eq!(st.zoom, 0, "degraded advertises no live zoom ceiling");
    }

    #[test]
    fn status_is_partial_after_a_coverage_404() {
        use crate::tiles::breaker::Outcome;
        let cache = tempfile::tempdir().unwrap();
        let gk = TileGatekeeper::new(cache.path());
        gk.set_source(Some(source("http://192.168.1.5:8080/tiles/")));
        let now = std::time::Instant::now();
        // A coverage-gap 404: source live, tile missing → Partial.
        gk.breaker_record(Outcome::Coverage, now);
        let st = status_core(&gk);
        assert_eq!(st.kind, StatusKind::Partial, "coverage 404 → Partial: got {st:?}");
    }

    #[test]
    fn status_returns_to_lan_live_after_a_success_clears_partial() {
        use crate::tiles::breaker::Outcome;
        let cache = tempfile::tempdir().unwrap();
        let gk = TileGatekeeper::new(cache.path());
        gk.set_source(Some(source("http://192.168.1.5:8080/tiles/")));
        let now = std::time::Instant::now();
        gk.breaker_record(Outcome::Coverage, now);
        assert_eq!(status_core(&gk).kind, StatusKind::Partial);
        // A subsequent real tile clears the partial flag → LanLive again.
        gk.breaker_record(Outcome::Success, now);
        assert_eq!(status_core(&gk).kind, StatusKind::LanLive);
    }

    // ── configure persistence (file I/O path; serial env isolation) ─────────
    //
    // `configure_core` persists via `write_config_atomic`, which resolves the
    // process-global `config_path()` from `TUXLINK_CONFIG_DIR`. std::env::set_var
    // is not thread-safe under parallel tests, so these run serially with env
    // save/restore (mirrors modem_commands::tests). The config.rs round-trip test
    // covers the pure-serde shape; THESE prove the activate+persist wiring.

    fn seed_config(dir: &std::path::Path) {
        let seed = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }}
            }}"#,
            ver = crate::config::CONFIG_SCHEMA_VERSION,
        );
        std::fs::write(dir.join("config.json"), seed).expect("seed config.json");
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn configure_activates_and_persists_on_lan_live() {
        let server = geodetic_png_server().await;
        let cache = tempfile::tempdir().unwrap();
        let cfg = tempfile::tempdir().unwrap();
        let prior = std::env::var("TUXLINK_CONFIG_DIR").ok();
        // SAFETY: single-threaded (serial) test; no concurrent env access.
        unsafe { std::env::set_var("TUXLINK_CONFIG_DIR", cfg.path()); }
        seed_config(cfg.path());

        let gk = TileGatekeeper::new(cache.path());
        let src = source(&server.url());
        let st = configure_core(&gk, &src, true).await.unwrap();
        assert_eq!(st.kind, StatusKind::LanLive, "got {st:?}");
        // (a) gatekeeper activated.
        assert_eq!(gk.active_source().as_ref(), Some(&src));
        // (b) persisted: reload the config from disk and confirm the source.
        let reloaded = crate::config::read_config().expect("config reloads");
        assert_eq!(
            reloaded.map_tile_source.as_ref().map(|s| s.url.clone()),
            Some(src.url.clone()),
            "configure must persist the activated source to config"
        );

        // SAFETY: symmetric restore; single-threaded test.
        unsafe {
            match prior {
                Some(v) => std::env::set_var("TUXLINK_CONFIG_DIR", v),
                None => std::env::remove_var("TUXLINK_CONFIG_DIR"),
            }
        }
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn configure_does_not_persist_on_incompatible() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/tilejson.json")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"tilejson":"3.0.0","crs":"EPSG:3857"}"#)
            .create_async()
            .await;
        let cache = tempfile::tempdir().unwrap();
        let cfg = tempfile::tempdir().unwrap();
        let prior = std::env::var("TUXLINK_CONFIG_DIR").ok();
        // SAFETY: single-threaded (serial) test.
        unsafe { std::env::set_var("TUXLINK_CONFIG_DIR", cfg.path()); }
        seed_config(cfg.path());

        let gk = TileGatekeeper::new(cache.path());
        let src = source(&server.url());
        let st = configure_core(&gk, &src, true).await.unwrap();
        assert_eq!(st.kind, StatusKind::Incompatible, "got {st:?}");
        assert!(gk.active_source().is_none(), "incompatible must not activate");
        let reloaded = crate::config::read_config().expect("config reloads");
        assert!(
            reloaded.map_tile_source.is_none(),
            "incompatible source must NOT be persisted"
        );

        // SAFETY: symmetric restore; single-threaded test.
        unsafe {
            match prior {
                Some(v) => std::env::set_var("TUXLINK_CONFIG_DIR", v),
                None => std::env::remove_var("TUXLINK_CONFIG_DIR"),
            }
        }
    }
}
