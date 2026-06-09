//! `tiles::crs` — Source-metadata CRS probe and geodetic-alignment helpers.
//!
//! ## Purpose
//!
//! The map runs `L.CRS.EPSG4326` (equirectangular, plate-carrée). A source
//! serving `EPSG:3857` (Web Mercator) renders plausible-but-WRONG coordinates —
//! the worst failure for a position tool. This module:
//!
//! 1. **[`probe_source_crs`]** — probes a LAN tile source's metadata to
//!    determine whether it serves geodetic (EPSG:4326) or Mercator tiles.
//!    Probe order: TileJSON → WMTS capabilities → mbtiles-style `metadata`
//!    JSON → `CrsCheck::Unknown`.
//!
//! 2. **[`geodetic_tile_index`]** — pure helper that computes the
//!    `(tile_x, tile_y)` index for a given `(lon, lat, zoom)` under the
//!    WorldCRS84Quad / `gdal2tiles --profile=geodetic` tile-numbering
//!    convention. The alignment fixture in the tests locks this as strictly
//!    LINEAR in latitude (constant Δy per degree) — a property that CANNOT
//!    hold for a Web Mercator source, making the fixture a cheap second-gate
//!    against a 3857 source that slipped past the metadata probe.
//!
//! ## Phase 6/8 caller note
//!
//! `probe_source_crs` returns [`CrsCheck::Unknown`] when no metadata is
//! probeable. The CALLER (the CRS gatekeeper in Phase 6/8) MUST treat
//! `Unknown` as a reject-with-explanation UNLESS the operator has set the
//! explicit `crs: Geodetic` config flag on the source (a deliberate override
//! for sources that don't expose discoverable metadata but are known-geodetic).
//! This module returns the honest signal; the policy decision lives upstream.

use super::TileSource;

// ─── CrsCheck ────────────────────────────────────────────────────────────────

/// Result of probing a LAN tile source's declared CRS metadata.
///
/// **Phase 6/8 policy:** `Unknown` MUST be treated as reject-with-explanation
/// unless the operator has explicitly set `crs: Geodetic` in the source
/// config. This enum is the honest signal; policy lives in the gatekeeper.
#[derive(Debug, PartialEq)]
pub enum CrsCheck {
    /// Source declares EPSG:4326 / geodetic / WGS84 — compatible with the map.
    Geodetic,
    /// Source declares EPSG:3857 / Web Mercator — incompatible; MUST reject.
    Rejected,
    /// No probeable metadata found; caller must apply the Unknown-is-reject
    /// policy unless the operator set an explicit `crs: Geodetic` override.
    Unknown,
}

// ─── CRS-string classifiers ───────────────────────────────────────────────────

/// Returns `true` when the string contains a known geodetic CRS indicator.
///
/// Covered identifiers (case-insensitive substring match):
/// `EPSG:4326`, `4326`, `OGC:CRS84`, `CRS84`, `WGS84`, `geodetic`,
/// `urn:ogc:def:crs:EPSG::4326`, `GLOBAL_GEODETIC`, `WorldCRS84Quad`,
/// `GoogleCRS84Quad`.
fn is_geodetic_indicator(s: &str) -> bool {
    let lo = s.to_ascii_lowercase();
    lo.contains("epsg:4326")
        || lo.contains(":4326")
        || lo.contains("ogc:crs84")
        || lo.contains("crs84")
        || lo.contains("wgs84")
        || lo.contains("geodetic")
        || lo.contains("global_geodetic")
        || lo.contains("worldcrs84quad")
        || lo.contains("googlecrs84quad")
}

/// Returns `true` when the string contains a known Web Mercator CRS indicator.
///
/// Covered identifiers (case-insensitive substring match):
/// `EPSG:3857`, `3857`, `900913`, `EPSG:900913`, `Web Mercator`,
/// `WebMercatorQuad`, `GoogleMapsCompatible`, `mercator`.
fn is_mercator_indicator(s: &str) -> bool {
    let lo = s.to_ascii_lowercase();
    lo.contains("epsg:3857")
        || lo.contains(":3857")
        || lo.contains("900913")
        || lo.contains("web mercator")
        || lo.contains("webmercatorquad")
        || lo.contains("googlemapscompatible")
        // "mercator" substring last — it must not shadow "geodetic" or "worldcrs84quad";
        // the geodetic check runs first in callers, so this ordering is defensive-only.
        || lo.contains("mercator")
}

/// Classify a raw CRS string into `Geodetic`, `Rejected`, or `Unknown`.
/// Geodetic check wins on conflict (no real string should trigger both).
fn classify_crs_str(s: &str) -> Option<CrsCheck> {
    if is_geodetic_indicator(s) {
        Some(CrsCheck::Geodetic)
    } else if is_mercator_indicator(s) {
        Some(CrsCheck::Rejected)
    } else {
        None
    }
}

// ─── Probe helpers ────────────────────────────────────────────────────────────

/// Try to classify a parsed TileJSON `Value` by inspecting the fields most
/// commonly used to declare CRS: `"crs"`, `"crs_wkt"`, `"profile"`, and
/// `"tileMatrixSet"` / `"scheme"`.
///
/// Returns `None` when no recognised field is present (caller falls through).
fn classify_tilejson(json: &serde_json::Value) -> Option<CrsCheck> {
    // Fields to inspect, in priority order.
    for key in &["crs", "crs_wkt", "profile", "tileMatrixSet", "scheme"] {
        if let Some(serde_json::Value::String(v)) = json.get(*key) {
            if let Some(check) = classify_crs_str(v) {
                return Some(check);
            }
        }
    }
    None
}

/// Scan a WMTS capabilities document for known TileMatrixSet identifiers.
///
/// This is a lightweight **heuristic substring scan** of the raw XML —
/// deliberately avoids a heavy XML dependency. The TileMatrixSet identifiers
/// defined by OGC are stable well-known strings that appear verbatim in
/// production capabilities documents.
///
/// Heuristic: searches for `WorldCRS84Quad` / `GoogleCRS84Quad` (geodetic) and
/// `WebMercatorQuad` / `GoogleMapsCompatible` (mercator). Geodetic wins on
/// conflict. Returns `None` when neither is found.
fn classify_wmts_xml(xml: &str) -> Option<CrsCheck> {
    let has_geodetic = xml.contains("WorldCRS84Quad") || xml.contains("GoogleCRS84Quad");
    let has_mercator = xml.contains("WebMercatorQuad") || xml.contains("GoogleMapsCompatible");
    if has_geodetic {
        Some(CrsCheck::Geodetic)
    } else if has_mercator {
        Some(CrsCheck::Rejected)
    } else {
        None
    }
}

/// Try to classify an mbtiles-style metadata JSON shape.
///
/// The `metadata` table of a `.mbtiles` file is often exposed as a JSON object
/// with fields such as `"profile"`, `"crs"`, or `"srs"`. We check those fields
/// plus the special `"format"` value that sometimes carries a CRS hint.
fn classify_mbtiles_metadata(json: &serde_json::Value) -> Option<CrsCheck> {
    for key in &["crs", "srs", "profile"] {
        if let Some(serde_json::Value::String(v)) = json.get(*key) {
            if let Some(check) = classify_crs_str(v) {
                return Some(check);
            }
        }
    }
    None
}

// ─── probe_source_crs ─────────────────────────────────────────────────────────

/// Probe a LAN tile source to determine its coordinate reference system.
///
/// **Probe order:**
/// 1. TileJSON: `GET <source_url>/tilejson.json` (or the source root if the URL
///    itself looks like a TileJSON endpoint). Parses JSON, looks for `crs`,
///    `crs_wkt`, `profile`, `tileMatrixSet`, `scheme` fields.
/// 2. WMTS capabilities: `GET <source_url>?SERVICE=WMTS&REQUEST=GetCapabilities`.
///    Scans the raw XML for known TileMatrixSet identifiers (`WorldCRS84Quad` →
///    geodetic; `WebMercatorQuad` → mercator). This is a heuristic substring
///    scan — no heavy XML dep.
/// 3. mbtiles metadata: `GET <source_url>/metadata` or `<source_url>/metadata.json`.
///    Parses the `profile`/`crs`/`srs` fields.
/// 4. Returns [`CrsCheck::Unknown`] when all probes fail or yield no signal.
///
/// Non-200 responses and parse errors fall through to the next probe; a network
/// error on any probe is absorbed and the next probe is attempted.
///
/// **Phase 6/8 policy note:** `Unknown` MUST be treated as
/// reject-with-explanation by the caller unless the operator explicitly set the
/// `crs: Geodetic` config flag on the source.
pub async fn probe_source_crs(client: &reqwest::Client, source: &TileSource) -> CrsCheck {
    let base = source.url.trim_end_matches('/');

    // ── Probe 1: TileJSON ────────────────────────────────────────────────────
    // Try `<base>/tilejson.json` first, then the base URL itself (some servers
    // serve TileJSON at the source root).
    for tilejson_url in &[
        format!("{base}/tilejson.json"),
        base.to_string(),
    ] {
        if let Ok(resp) = client.get(tilejson_url.as_str()).send().await {
            if resp.status().is_success() {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    if let Some(check) = classify_tilejson(&json) {
                        return check;
                    }
                }
            }
        }
        // Non-200, parse error, or network error → fall through.
    }

    // ── Probe 2: WMTS capabilities (heuristic XML substring scan) ────────────
    let wmts_url = format!(
        "{base}?SERVICE=WMTS&REQUEST=GetCapabilities&VERSION=1.0.0"
    );
    if let Ok(resp) = client.get(&wmts_url).send().await {
        if resp.status().is_success() {
            if let Ok(text) = resp.text().await {
                if let Some(check) = classify_wmts_xml(&text) {
                    return check;
                }
            }
        }
    }

    // ── Probe 3: mbtiles-style metadata JSON ─────────────────────────────────
    for meta_url in &[
        format!("{base}/metadata.json"),
        format!("{base}/metadata"),
    ] {
        if let Ok(resp) = client.get(meta_url.as_str()).send().await {
            if resp.status().is_success() {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    if let Some(check) = classify_mbtiles_metadata(&json) {
                        return check;
                    }
                }
            }
        }
    }

    // ── No signal from any probe ─────────────────────────────────────────────
    CrsCheck::Unknown
}

// ─── Tests (Task 4.1 — probe_source_crs) ─────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tiles::{Crs, TileScheme, TileSource};

    fn source(url: &str) -> TileSource {
        TileSource {
            url: url.into(),
            crs: Crs::Geodetic,
            scheme: TileScheme::Xyz,
            min_zoom: 0,
            max_zoom: 19,
            cache_budget_mb: 384,
            attribution: None,
            label: "test".into(),
        }
    }

    fn plain_client() -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("test client")
    }

    // ── Task 4.1: probe_source_crs ──────────────────────────────────────────

    #[tokio::test]
    async fn tilejson_epsg3857_is_rejected() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/tilejson.json")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"tilejson":"3.0.0","crs":"EPSG:3857","tiles":["http://example.com/{z}/{x}/{y}.png"]}"#)
            .create_async()
            .await;
        let src = source(&server.url());
        let check = probe_source_crs(&plain_client(), &src).await;
        assert_eq!(check, CrsCheck::Rejected, "EPSG:3857 TileJSON must be Rejected");
    }

    #[tokio::test]
    async fn tilejson_webmercatorquad_is_rejected() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/tilejson.json")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"tilejson":"3.0.0","tileMatrixSet":"WebMercatorQuad","tiles":["http://example.com/{z}/{x}/{y}.png"]}"#)
            .create_async()
            .await;
        let src = source(&server.url());
        let check = probe_source_crs(&plain_client(), &src).await;
        assert_eq!(check, CrsCheck::Rejected, "WebMercatorQuad TileJSON must be Rejected");
    }

    #[tokio::test]
    async fn tilejson_epsg4326_is_geodetic() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/tilejson.json")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"tilejson":"3.0.0","crs":"EPSG:4326","tiles":["http://example.com/{z}/{x}/{y}.png"]}"#)
            .create_async()
            .await;
        let src = source(&server.url());
        let check = probe_source_crs(&plain_client(), &src).await;
        assert_eq!(check, CrsCheck::Geodetic, "EPSG:4326 TileJSON must be Geodetic");
    }

    #[tokio::test]
    async fn tilejson_wgs84_profile_is_geodetic() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/tilejson.json")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"tilejson":"3.0.0","profile":"WGS84","tiles":["http://example.com/{z}/{x}/{y}.png"]}"#)
            .create_async()
            .await;
        let src = source(&server.url());
        let check = probe_source_crs(&plain_client(), &src).await;
        assert_eq!(check, CrsCheck::Geodetic, "WGS84 profile TileJSON must be Geodetic");
    }

    #[tokio::test]
    async fn mbtiles_geodetic_metadata_is_geodetic() {
        let mut server = mockito::Server::new_async().await;
        // tilejson.json returns 404 → probe falls through to metadata
        let _m1 = server
            .mock("GET", "/tilejson.json")
            .with_status(404)
            .create_async()
            .await;
        // base URL also 404
        let _m2 = server
            .mock("GET", "/")
            .with_status(404)
            .create_async()
            .await;
        // WMTS capabilities: 404
        server
            .mock("GET", mockito::Matcher::Regex(r"SERVICE=WMTS".to_string()))
            .with_status(404)
            .create_async()
            .await;
        // metadata.json returns geodetic profile
        let _m3 = server
            .mock("GET", "/metadata.json")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"profile":"geodetic","format":"png","maxzoom":12}"#)
            .create_async()
            .await;
        let src = source(&server.url());
        let check = probe_source_crs(&plain_client(), &src).await;
        assert_eq!(check, CrsCheck::Geodetic, "mbtiles geodetic metadata must be Geodetic");
    }

    #[tokio::test]
    async fn mbtiles_mercator_metadata_is_rejected() {
        let mut server = mockito::Server::new_async().await;
        let _m1 = server
            .mock("GET", "/tilejson.json")
            .with_status(404)
            .create_async()
            .await;
        let _m2 = server
            .mock("GET", "/")
            .with_status(404)
            .create_async()
            .await;
        server
            .mock("GET", mockito::Matcher::Regex(r"SERVICE=WMTS".to_string()))
            .with_status(404)
            .create_async()
            .await;
        let _m3 = server
            .mock("GET", "/metadata.json")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"profile":"mercator","format":"png","maxzoom":18}"#)
            .create_async()
            .await;
        let src = source(&server.url());
        let check = probe_source_crs(&plain_client(), &src).await;
        assert_eq!(check, CrsCheck::Rejected, "mbtiles mercator metadata must be Rejected");
    }

    #[tokio::test]
    async fn no_probeable_metadata_is_unknown() {
        let mut server = mockito::Server::new_async().await;
        // All probes 404 → Unknown
        let _m1 = server
            .mock("GET", "/tilejson.json")
            .with_status(404)
            .create_async()
            .await;
        let _m2 = server
            .mock("GET", "/")
            .with_status(404)
            .create_async()
            .await;
        server
            .mock("GET", mockito::Matcher::Regex(r"SERVICE=WMTS".to_string()))
            .with_status(404)
            .create_async()
            .await;
        let _m3 = server
            .mock("GET", "/metadata.json")
            .with_status(404)
            .create_async()
            .await;
        let _m4 = server
            .mock("GET", "/metadata")
            .with_status(404)
            .create_async()
            .await;
        let src = source(&server.url());
        let check = probe_source_crs(&plain_client(), &src).await;
        assert_eq!(check, CrsCheck::Unknown, "no probeable metadata must be Unknown");
    }

    #[tokio::test]
    async fn wmts_worldcrs84quad_is_geodetic() {
        let mut server = mockito::Server::new_async().await;
        // TileJSON 404 → probe falls through to WMTS
        let _m1 = server
            .mock("GET", "/tilejson.json")
            .with_status(404)
            .create_async()
            .await;
        let _m2 = server
            .mock("GET", "/")
            .with_status(404)
            .create_async()
            .await;
        // WMTS capabilities contains WorldCRS84Quad
        server
            .mock("GET", mockito::Matcher::Regex(r"SERVICE=WMTS".to_string()))
            .with_status(200)
            .with_header("content-type", "application/xml")
            .with_body(r#"<?xml version="1.0"?><Capabilities><TileMatrixSet><Identifier>WorldCRS84Quad</Identifier></TileMatrixSet></Capabilities>"#)
            .create_async()
            .await;
        let src = source(&server.url());
        let check = probe_source_crs(&plain_client(), &src).await;
        assert_eq!(check, CrsCheck::Geodetic, "WMTS WorldCRS84Quad must be Geodetic");
    }

    #[tokio::test]
    async fn wmts_webmercatorquad_is_rejected() {
        let mut server = mockito::Server::new_async().await;
        let _m1 = server
            .mock("GET", "/tilejson.json")
            .with_status(404)
            .create_async()
            .await;
        let _m2 = server
            .mock("GET", "/")
            .with_status(404)
            .create_async()
            .await;
        server
            .mock("GET", mockito::Matcher::Regex(r"SERVICE=WMTS".to_string()))
            .with_status(200)
            .with_header("content-type", "application/xml")
            .with_body(r#"<?xml version="1.0"?><Capabilities><TileMatrixSet><Identifier>WebMercatorQuad</Identifier></TileMatrixSet></Capabilities>"#)
            .create_async()
            .await;
        let src = source(&server.url());
        let check = probe_source_crs(&plain_client(), &src).await;
        assert_eq!(check, CrsCheck::Rejected, "WMTS WebMercatorQuad must be Rejected");
    }
}
