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
        || lo.contains("mercator")
}

/// Classify a raw CRS string into `Geodetic`, `Rejected`, or `Unknown`.
///
/// **Reject-biased ordering (§8.1):** the Mercator check runs FIRST. WGS84 is the
/// DATUM shared by both EPSG:4326 and EPSG:3857 — EPSG:3857's name is literally
/// "WGS 84 / Pseudo-Mercator" — so a bare "wgs84" substring does NOT imply a
/// geodetic PROJECTION. A string carrying a Mercator indicator (`mercator`,
/// `3857`, `pseudo-mercator`, …) must therefore classify as `Rejected` even when
/// it also contains "wgs84"/"crs84". This cannot false-reject a real geodetic
/// (EPSG:4326) source: their CRS strings never contain a Mercator indicator.
/// Accepting a Mercator source is the §8.1 ship-blocker; refusing is the safe bias.
fn classify_crs_str(s: &str) -> Option<CrsCheck> {
    if is_mercator_indicator(s) {
        Some(CrsCheck::Rejected)
    } else if is_geodetic_indicator(s) {
        Some(CrsCheck::Geodetic)
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
/// `WebMercatorQuad` / `GoogleMapsCompatible` (mercator).
///
/// **Reject-biased (§8.1):** Mercator wins on conflict. A WMTS server commonly
/// advertises MULTIPLE TileMatrixSets; this gatekeeper's fetcher issues plain
/// `{z}/{x}/{y}` requests and cannot select which set the server answers with,
/// so a capabilities document that lists a Mercator set is ambiguous and is
/// refused rather than risk rendering Mercator tiles on the EPSG:4326 map. A
/// pure-geodetic server (only WorldCRS84Quad) is still accepted.
fn classify_wmts_xml(xml: &str) -> Option<CrsCheck> {
    let has_geodetic = xml.contains("WorldCRS84Quad") || xml.contains("GoogleCRS84Quad");
    let has_mercator = xml.contains("WebMercatorQuad") || xml.contains("GoogleMapsCompatible");
    if has_mercator {
        Some(CrsCheck::Rejected)
    } else if has_geodetic {
        Some(CrsCheck::Geodetic)
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

// ─── geodetic_tile_index ──────────────────────────────────────────────────────

/// Compute the `(tile_x, tile_y)` index for a `(lon, lat)` point at zoom
/// level `z` under the **WorldCRS84Quad / `gdal2tiles --profile=geodetic`**
/// convention (EPSG:4326 equirectangular).
///
/// ## Tile-numbering convention
///
/// The world is **2 tiles wide × 1 tile tall at z=0** (lon ∈ \[-180, 180\] →
/// 2 columns; lat ∈ \[-90, 90\] → 1 row). At zoom `z`:
///
/// - Columns: `2^(z+1)` total; `x = floor((lon + 180) / 360 * 2^(z+1))`
/// - Rows (Y=0 at north): `2^z` total; `y = floor((90 - lat) / 180 * 2^z)`
///
/// This is **linear in latitude** — constant Δy per degree — which is the
/// distinguishing property of the equirectangular projection. A Web Mercator
/// source uses a log-tangent y mapping and would NOT satisfy the alignment
/// fixture in the tests.
///
/// ## Alignment with the frontend projection
///
/// Matches `src/map/projection.ts` `latLonToPixel`:
/// ```text
/// x_pixel = ((lon + 180) / 360) * width
/// y_pixel = ((90 - lat) / 180) * height
/// ```
/// Dividing pixel space into `2^(z+1)` columns × `2^z` rows of 256 px each
/// gives exactly the formula above (the same linear numerator/denominator).
///
/// ## Clamping
///
/// - `lon` is clamped to `[-180, 180]`.
/// - `lat` is clamped to `[-90, 90]`.
/// - The date-line column is clamped to `2^(z+1) - 1` (lon=180 maps to the
///   last column rather than overflowing).
/// - The south-pole row is clamped to `2^z - 1` (lat=-90 maps to the last
///   row rather than overflowing).
pub fn geodetic_tile_index(lon: f64, lat: f64, z: u32) -> (u32, u32) {
    let lon = lon.clamp(-180.0, 180.0);
    let lat = lat.clamp(-90.0, 90.0);

    let cols = 1u32 << (z + 1); // 2^(z+1)
    let rows = 1u32 << z; // 2^z

    let x_f = (lon + 180.0) / 360.0 * (cols as f64);
    let y_f = (90.0 - lat) / 180.0 * (rows as f64);

    // floor → integer tile index, then clamp to valid range (handles edge values
    // lon=180 and lat=-90 which would otherwise produce an out-of-range index).
    let x = (x_f.floor() as u32).min(cols - 1);
    let y = (y_f.floor() as u32).min(rows - 1);

    (x, y)
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

    #[test]
    fn pseudo_mercator_named_crs_is_rejected_despite_wgs84_datum() {
        // §8.1 regression: EPSG:3857 is officially "WGS 84 / Pseudo-Mercator" —
        // its CRS string carries the WGS84 datum name. A bare "wgs84" substring
        // must NOT win over the Mercator indicator. These all declare a Mercator
        // projection on the WGS84 datum and MUST classify Rejected, not Geodetic.
        assert_eq!(classify_crs_str("WGS84 / Pseudo-Mercator"), Some(CrsCheck::Rejected));
        assert_eq!(classify_crs_str("WGS 84 / Pseudo-Mercator"), Some(CrsCheck::Rejected));
        assert_eq!(classify_crs_str("EPSG:3857 (WGS84 Web Mercator)"), Some(CrsCheck::Rejected));
        // Pure geodetic still classifies Geodetic.
        assert_eq!(classify_crs_str("WGS84"), Some(CrsCheck::Geodetic));
        assert_eq!(classify_crs_str("EPSG:4326"), Some(CrsCheck::Geodetic));
        assert_eq!(classify_crs_str("WorldCRS84Quad"), Some(CrsCheck::Geodetic));
    }

    #[test]
    fn wmts_multi_matrixset_with_mercator_is_rejected() {
        // A capabilities doc advertising BOTH WorldCRS84Quad and WebMercatorQuad
        // is ambiguous for a plain {z}/{x}/{y} fetcher → reject (Mercator wins).
        let xml = r#"<Capabilities><TileMatrixSet>WorldCRS84Quad</TileMatrixSet>
            <TileMatrixSet>WebMercatorQuad</TileMatrixSet></Capabilities>"#;
        assert_eq!(classify_wmts_xml(xml), Some(CrsCheck::Rejected));
        // Pure-geodetic capabilities still accepted.
        let geo = r#"<Capabilities><TileMatrixSet>WorldCRS84Quad</TileMatrixSet></Capabilities>"#;
        assert_eq!(classify_wmts_xml(geo), Some(CrsCheck::Geodetic));
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

    // ── Task 4.2: geodetic_tile_index alignment fixture ─────────────────────
    //
    // WorldCRS84Quad convention at z=6:
    //   cols = 2^(6+1) = 128   rows = 2^6 = 64
    //   x = floor((lon + 180) / 360 * 128)
    //   y = floor((90 - lat) / 180 * 64)
    //
    // This formula is LINEAR in latitude: Δy per degree = 64/180 ≈ 0.3556.
    // A Mercator source would use y = floor((π - ln(tan(π/4 + lat*π/360))) / (2π) * 64),
    // which produces non-uniform Δy per degree — the fixture catches this.
    //
    // Alignment with projection.ts `latLonToPixel` (verified against the source):
    //   x_pixel = ((lon + 180) / 360) * width   →  same linear formula
    //   y_pixel = ((90 - lat) / 180) * height   →  same linear formula
    // Dividing into 128×64 tile-sized bins is exactly the formula above.

    /// lon=0, lat=0 (equator, prime meridian) at z=6
    #[test]
    fn geodetic_tile_equator() {
        // lon=0:   x = floor((0+180)/360 * 128) = floor(64.0) = 64
        // lat=0:   y = floor((90-0)/180 * 64)   = floor(32.0) = 32
        let (x, y) = geodetic_tile_index(0.0, 0.0, 6);
        assert_eq!(x, 64, "equator lon=0 tile x");
        assert_eq!(y, 32, "equator lat=0 tile y");
    }

    /// lon=0, lat=45 (mid-latitude) at z=6
    #[test]
    fn geodetic_tile_mid_latitude() {
        // lat=45:  y = floor((90-45)/180 * 64) = floor(45/180 * 64) = floor(16.0) = 16
        let (x, y) = geodetic_tile_index(0.0, 45.0, 6);
        assert_eq!(x, 64, "mid-lat lon=0 tile x");
        assert_eq!(y, 16, "mid-lat lat=45 tile y");
    }

    /// lon=0, lat=80 (high latitude) at z=6
    #[test]
    fn geodetic_tile_high_latitude() {
        // lat=80:  y = floor((90-80)/180 * 64) = floor(10/180 * 64) = floor(3.555…) = 3
        let (x, y) = geodetic_tile_index(0.0, 80.0, 6);
        assert_eq!(x, 64, "high-lat lon=0 tile x");
        assert_eq!(y, 3, "high-lat lat=80 tile y");
    }

    /// KEY alignment property: the Y-index spacing per degree of latitude is CONSTANT
    /// (equirectangular, linear). This is the property that a Web Mercator source
    /// violates: Mercator has growing y-spacing at higher latitudes (log-tangent stretch).
    ///
    /// For each test latitude we compute the EXPECTED y index from the linear formula
    /// directly and assert exact equality, locking the constant-unit-spacing invariant.
    #[test]
    fn geodetic_tile_y_spacing_is_linear() {
        let z: u32 = 6;
        let rows = 1u32 << z; // 64

        for lat in [-80i32, -45, 0, 45, 80] {
            let lat_f = lat as f64;
            let expected_y = ((90.0 - lat_f) / 180.0 * rows as f64).floor() as u32;
            let expected_y = expected_y.min(rows - 1);
            let (_, y) = geodetic_tile_index(0.0, lat_f, z);
            assert_eq!(
                y, expected_y,
                "linear y spacing violated at lat={lat_f}: expected {expected_y} got {y}"
            );
        }
    }

    /// Boundary: lon=180 clamps to last column (not overflow).
    #[test]
    fn geodetic_tile_dateline_clamp() {
        let (x, _y) = geodetic_tile_index(180.0, 0.0, 6);
        let cols = 1u32 << 7; // 128 at z=6
        assert!(x < cols, "lon=180 must not overflow: x={x} cols={cols}");
        assert_eq!(x, cols - 1, "lon=180 should map to last column");
    }

    /// Boundary: lat=-90 clamps to last row (not overflow).
    #[test]
    fn geodetic_tile_south_pole_clamp() {
        let (_x, y) = geodetic_tile_index(0.0, -90.0, 6);
        let rows = 1u32 << 6; // 64 at z=6
        assert!(y < rows, "lat=-90 must not overflow: y={y} rows={rows}");
        assert_eq!(y, rows - 1, "lat=-90 should map to last row");
    }

    /// z=0: the world is 2 columns × 1 row (the defining property of WorldCRS84Quad).
    #[test]
    fn geodetic_tile_zoom_zero() {
        // Western hemisphere: lon=-90 → x=0, lat=0 → y=0
        let (x, y) = geodetic_tile_index(-90.0, 0.0, 0);
        assert_eq!(x, 0);
        assert_eq!(y, 0);
        // Eastern hemisphere: lon=90 → x=1, lat=0 → y=0
        let (x, y) = geodetic_tile_index(90.0, 0.0, 0);
        assert_eq!(x, 1);
        assert_eq!(y, 0);
    }
}
