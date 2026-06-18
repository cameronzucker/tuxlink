//! Region-pack distribution manifest (tuxlink-ndi4, D1 / phase 4).
//!
//! Design: docs/design/2026-06-13-ndi4-d1-region-pack-distribution.md
//!
//! The manifest names the current Protomaps planet build URL (builds rotate
//! ~monthly; old ones 404) and the tunable per-tier coverage boxes. A small
//! default is bundled in the `.deb` so the app works offline on first launch; a
//! Rust command refreshes it from the canonical raw URL (the webview never fetches
//! it, so the CSP stays closed).
//!
//! SECURITY: the `planet_url` and tier degree values are *attacker-influenceable*
//! — they come from a remotely-fetched manifest and flow into the `go-pmtiles`
//! sidecar's argv. [`RegionManifest::parse`] allowlists `planet_url` to
//! `https://build.protomaps.com` (no other scheme, host, credentials, or port) and
//! range-checks every tier/continent BEFORE the manifest is accepted, so a
//! malicious manifest cannot *name* a non-Protomaps host (`file://`,
//! link-local/metadata IPs, a LAN host) and cannot smuggle a bbox the bbox math
//! (`super::packs`) would turn into a degenerate sidecar argument.
//!
//! HONEST SCOPE (do not overstate): this is a string allowlist on the *named* host.
//! It does NOT constrain redirect-following inside go-pmtiles' own HTTP client — if
//! `build.protomaps.com` itself were compromised/MITM'd and 3xx-redirected to an
//! internal target, the sidecar (a separate process) would follow it; the Rust
//! allowlist cannot police an egress it does not perform. The mitigation here is
//! "can't be pointed at a non-Protomaps host by name"; transport-level redirect
//! defense would require pinning go-pmtiles to a no-redirect/resolved-IP mode,
//! which is out of scope for this layer. (Cf. `tiles::fetch`, which DOES set
//! `redirect::Policy::none()` because it performs the egress itself.)

use serde::{Deserialize, Serialize};

/// The only manifest schema string this build understands. A manifest carrying a
/// different schema is rejected so a future schema bump can never brick the pack
/// UI — the caller falls back to [`RegionManifest::bundled_default`].
pub const MANIFEST_SCHEMA: &str = "tuxlink-basemap-manifest/1";

/// The only host a pack may be extracted from. See the module SECURITY note.
pub const ALLOWED_PLANET_HOST: &str = "build.protomaps.com";

/// Hard ceiling on a tier's `maxzoom`. A tier's detail level flows into the
/// go-pmtiles `--maxzoom` argv (the continent path, tuxlink-8g28), so — exactly
/// like `planet_url` and the bbox degrees — it is an *attacker-influenceable*
/// manifest value and MUST be bounded by an app constant: a hostile manifest
/// setting `maxzoom: 30` would otherwise turn a continent extract into a
/// planet-scale runaway download. [`RegionManifest::parse`] rejects any tier
/// outside `1..=MAX_TIER_MAXZOOM`, so the "manifest can't request an oversized
/// extract" property survives moving maxzoom from a code constant into the
/// manifest. This is the same z14 ceiling the on-demand pack path enforces
/// (`commands.rs::PACK_MAXZOOM`); kept here because `parse` is the gate.
pub const MAX_TIER_MAXZOOM: u8 = 14;

/// Compiled-in default manifest — guarantees a usable manifest even if the bundled
/// resource file is absent, and is the fallback when a refresh yields garbage.
const DEFAULT_MANIFEST_JSON: &str = include_str!("../../resources/basemap/region-manifest.json");

/// The current Protomaps planet build + the coverage tiers and continents offered
/// in the pack manager. Validated on construction ([`RegionManifest::parse`]).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RegionManifest {
    pub schema: String,
    pub planet_build: String,
    pub planet_url: String,
    pub pmtiles_schema: PmtilesSchema,
    pub tiers: Vec<Tier>,
    #[serde(default)]
    pub continents: Vec<Continent>,
}

/// The vector schema the tiers were sized against. Recorded for manifest reviewers;
/// the runtime gate is [`super::validate`] against the actual downloaded archive.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PmtilesSchema {
    pub planetiler_version: u32,
    pub vector_layers: Vec<String>,
}

/// A coverage tier — two roles, one struct (tuxlink-8g28):
///   1. *area* download (`DownloadArgs::Tier`): a fixed box centered on the operator
///      grid, sized by `half_deg`, always extracted at full detail (z14) since the
///      box is small. `maxzoom` is ignored on this path.
///   2. *continent detail* (`DownloadArgs::Continent`): the operator picks a tier as
///      a DETAIL level for a continent-scale bbox, where the size lever is `maxzoom`
///      (not bbox). Local/Regional/Wide map to escalating `maxzoom` ceilings; the
///      continent path uses `maxzoom` and ignores `half_deg`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tier {
    pub id: String,
    pub label: String,
    /// `[half-width-longitude-degrees, half-width-latitude-degrees]`. Area path only.
    pub half_deg: [f64; 2],
    /// Detail ceiling for a continent-scale extract at this tier (`1..=MAX_TIER_MAXZOOM`,
    /// validated by [`RegionManifest::parse`]). The area path always uses z14 (a small
    /// box at full detail); this drives only the continent path's `--maxzoom`.
    pub maxzoom: u8,
    pub typical_bytes: u64,
    /// The tier offered by default at location-set (exactly one should set this).
    #[serde(default)]
    pub default: bool,
}

/// A named continent pack as a fixed `[west, south, east, north]` box.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Continent {
    pub id: String,
    pub label: String,
    /// `[west, south, east, north]` degrees.
    pub bbox: [f64; 4],
    pub typical_bytes: u64,
}

/// Why a manifest was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    /// serde could not parse the bytes as the manifest shape.
    Json(String),
    /// The `schema` string is not [`MANIFEST_SCHEMA`].
    UnknownSchema(String),
    /// `planet_url` is not `https://build.protomaps.com/…` with no creds/port.
    BadPlanetUrl(String),
    /// No tiers — the manager would have nothing to offer.
    NoTiers,
    /// A tier's id was empty or its `half_deg` was non-finite / out of range.
    BadTier(String),
    /// A continent's bbox was non-finite, out of range, or had `west >= east` /
    /// `south >= north`.
    BadContinent(String),
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestError::Json(e) => write!(f, "manifest is not valid JSON: {e}"),
            ManifestError::UnknownSchema(s) => write!(f, "unknown manifest schema {s:?}"),
            ManifestError::BadPlanetUrl(u) => write!(f, "disallowed planet URL {u:?}"),
            ManifestError::NoTiers => write!(f, "manifest has no tiers"),
            ManifestError::BadTier(id) => write!(f, "invalid tier {id:?}"),
            ManifestError::BadContinent(id) => write!(f, "invalid continent {id:?}"),
        }
    }
}

impl std::error::Error for ManifestError {}

/// Validate that `raw` is `https://build.protomaps.com/…` with no embedded
/// credentials and no non-default port. This is the SSRF / argv-injection gate for
/// the value that becomes the go-pmtiles source argument.
pub fn validate_planet_url(raw: &str) -> Result<(), ManifestError> {
    let url = reqwest::Url::parse(raw).map_err(|e| ManifestError::BadPlanetUrl(format!("{raw}: {e}")))?;
    if url.scheme() != "https" {
        return Err(ManifestError::BadPlanetUrl(format!("{raw}: scheme must be https")));
    }
    if url.host_str() != Some(ALLOWED_PLANET_HOST) {
        return Err(ManifestError::BadPlanetUrl(format!(
            "{raw}: host must be {ALLOWED_PLANET_HOST}"
        )));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(ManifestError::BadPlanetUrl(format!("{raw}: credentials not allowed")));
    }
    // Pin the default https port — a manifest must not redirect to an alternate
    // port on the allowed host.
    if url.port().is_some() {
        return Err(ManifestError::BadPlanetUrl(format!("{raw}: explicit port not allowed")));
    }
    Ok(())
}

/// True for a half-width that is finite, positive, and within a hemisphere span.
fn half_deg_ok(v: f64, max: f64) -> bool {
    v.is_finite() && v > 0.0 && v <= max
}

impl RegionManifest {
    /// Parse + fully validate a manifest. Returns an error (not a partial manifest)
    /// if the schema is unknown, the planet URL is disallowed, or any tier/continent
    /// is malformed — the caller then keeps the previous good manifest or the
    /// bundled default.
    pub fn parse(json: &str) -> Result<Self, ManifestError> {
        let m: RegionManifest =
            serde_json::from_str(json).map_err(|e| ManifestError::Json(e.to_string()))?;
        if m.schema != MANIFEST_SCHEMA {
            return Err(ManifestError::UnknownSchema(m.schema));
        }
        validate_planet_url(&m.planet_url)?;
        if m.tiers.is_empty() {
            return Err(ManifestError::NoTiers);
        }
        for t in &m.tiers {
            // half_deg[0] = longitude half-width (≤180), [1] = latitude (≤85, the
            // web-mercator clamp). maxzoom is bounded 1..=MAX_TIER_MAXZOOM — it
            // reaches the `--maxzoom` argv on the continent path, so an unbounded
            // value from a hostile manifest could force a planet-scale extract
            // (tuxlink-8g28; see MAX_TIER_MAXZOOM).
            if t.id.trim().is_empty()
                || !half_deg_ok(t.half_deg[0], 180.0)
                || !half_deg_ok(t.half_deg[1], 85.0)
                || t.maxzoom < 1
                || t.maxzoom > MAX_TIER_MAXZOOM
            {
                return Err(ManifestError::BadTier(t.id.clone()));
            }
        }
        for c in &m.continents {
            let [w, s, e, n] = c.bbox;
            let ranged = [w, e].iter().all(|v| v.is_finite() && (-180.0..=180.0).contains(v))
                && [s, n].iter().all(|v| v.is_finite() && (-85.0..=85.0).contains(v));
            if c.id.trim().is_empty() || !ranged || w >= e || s >= n {
                return Err(ManifestError::BadContinent(c.id.clone()));
            }
        }
        Ok(m)
    }

    /// The compiled-in default manifest. Infallible: a build whose own bundled
    /// manifest does not parse is a build defect caught by the test below.
    pub fn bundled_default() -> Self {
        Self::parse(DEFAULT_MANIFEST_JSON).expect("bundled default manifest must be valid")
    }

    /// The tier offered by default at location-set: the one flagged `default`, else
    /// the first tier (guaranteed present — `parse` rejects an empty list).
    pub fn default_tier(&self) -> &Tier {
        self.tiers
            .iter()
            .find(|t| t.default)
            .unwrap_or(&self.tiers[0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_json() -> String {
        DEFAULT_MANIFEST_JSON.to_string()
    }

    #[test]
    fn bundled_default_is_valid() {
        let m = RegionManifest::bundled_default();
        assert_eq!(m.schema, MANIFEST_SCHEMA);
        assert_eq!(m.planet_build, "20260614");
        // tuxlink-4o9r: the pinned planet build emits 9 vector_layers (not the
        // obsolete 13); matches REQUIRED_LAYER_IDS + the @protomaps/basemaps@5 style.
        assert_eq!(m.pmtiles_schema.vector_layers.len(), 9);
        assert!(!m.tiers.is_empty());
        assert!(!m.continents.is_empty());
    }

    #[test]
    fn default_tier_is_the_flagged_one() {
        let m = RegionManifest::bundled_default();
        assert_eq!(m.default_tier().id, "wide");
    }

    #[test]
    fn bundled_tiers_carry_bounded_escalating_maxzoom() {
        // tuxlink-8g28: every tier carries a continent-detail maxzoom, each within
        // the security ceiling, and they escalate Local→Regional→Wide so picking a
        // bigger detail tier means a deeper (larger) continent extract.
        let m = RegionManifest::bundled_default();
        for t in &m.tiers {
            assert!(t.maxzoom >= 1 && t.maxzoom <= MAX_TIER_MAXZOOM, "tier {} maxzoom out of bounds", t.id);
        }
        let z = |id: &str| m.tiers.iter().find(|t| t.id == id).unwrap().maxzoom;
        assert!(z("local") < z("regional"), "local detail must be shallower than regional");
        assert!(z("regional") < z("wide"), "regional detail must be shallower than wide");
    }

    #[test]
    fn parse_rejects_tier_maxzoom_over_ceiling() {
        // SECURITY: a hostile/rotated manifest must not be able to request a
        // planet-scale extract by naming an oversized maxzoom — parse rejects it and
        // the caller falls back to the bundled default.
        let json = valid_json().replace("\"maxzoom\": 8", "\"maxzoom\": 30");
        assert!(matches!(
            RegionManifest::parse(&json),
            Err(ManifestError::BadTier(_))
        ));
    }

    #[test]
    fn parse_rejects_tier_maxzoom_zero() {
        let json = valid_json().replace("\"maxzoom\": 8", "\"maxzoom\": 0");
        assert!(matches!(
            RegionManifest::parse(&json),
            Err(ManifestError::BadTier(_))
        ));
    }

    #[test]
    fn parse_rejects_tier_missing_maxzoom() {
        // maxzoom is required (no serde default): a manifest lacking it is rejected
        // so the app falls back to the bundled default rather than silently treating
        // a continent download as full-detail (the tuxlink-8g28 bug).
        let json = valid_json().replace(", \"maxzoom\": 8", "");
        assert!(matches!(
            RegionManifest::parse(&json),
            Err(ManifestError::Json(_))
        ));
    }

    #[test]
    fn default_tier_falls_back_to_first_when_none_flagged() {
        let json = valid_json().replace(", \"default\": true", "");
        let m = RegionManifest::parse(&json).unwrap();
        assert_eq!(m.default_tier().id, "local");
    }

    #[test]
    fn rejects_unknown_schema() {
        let json = valid_json().replace("tuxlink-basemap-manifest/1", "tuxlink-basemap-manifest/2");
        assert!(matches!(
            RegionManifest::parse(&json),
            Err(ManifestError::UnknownSchema(_))
        ));
    }

    #[test]
    fn rejects_non_json() {
        assert!(matches!(
            RegionManifest::parse("{not json"),
            Err(ManifestError::Json(_))
        ));
    }

    // ── planet_url allowlist (the SSRF / argv-injection gate) ──────────────────

    #[test]
    fn accepts_https_protomaps_url() {
        assert!(validate_planet_url("https://build.protomaps.com/20260608.pmtiles").is_ok());
    }

    #[test]
    fn rejects_http_scheme() {
        assert!(validate_planet_url("http://build.protomaps.com/x.pmtiles").is_err());
    }

    #[test]
    fn rejects_file_scheme() {
        assert!(validate_planet_url("file:///etc/passwd").is_err());
    }

    #[test]
    fn rejects_other_host() {
        assert!(validate_planet_url("https://evil.example.com/x.pmtiles").is_err());
    }

    #[test]
    fn rejects_link_local_metadata_ip() {
        assert!(validate_planet_url("https://169.254.169.254/latest/meta-data").is_err());
        assert!(validate_planet_url("http://169.254.169.254/").is_err());
    }

    #[test]
    fn rejects_lan_host() {
        assert!(validate_planet_url("https://pandora.local:8090/x.pmtiles").is_err());
    }

    #[test]
    fn rejects_embedded_credentials() {
        // A classic confused-deputy: real host after an @, attacker host before it.
        assert!(validate_planet_url("https://build.protomaps.com@evil.example.com/x").is_err());
        assert!(validate_planet_url("https://user:pass@build.protomaps.com/x").is_err());
    }

    #[test]
    fn rejects_explicit_port_on_allowed_host() {
        assert!(validate_planet_url("https://build.protomaps.com:8443/x.pmtiles").is_err());
    }

    #[test]
    fn rejects_lookalike_and_suffix_hosts() {
        // Allowlist must be exact-host, not substring/suffix — these are the classic
        // bypasses against a naive `contains`/`ends_with` check.
        assert!(validate_planet_url("https://build.protomaps.com.evil.com/x").is_err());
        assert!(validate_planet_url("https://notbuild.protomaps.com/x").is_err());
        assert!(validate_planet_url("https://build.protomaps.com./x").is_err()); // trailing dot
        assert!(validate_planet_url("https://xn--build-protomaps.com/x").is_err()); // punycode lookalike
    }

    #[test]
    fn accepts_uppercase_host_via_normalization() {
        // WHATWG URL parsing lowercases the host, so an uppercase host still matches.
        assert!(validate_planet_url("https://BUILD.PROTOMAPS.COM/20260608.pmtiles").is_ok());
    }

    #[test]
    fn parse_rejects_manifest_with_bad_url() {
        let json = valid_json().replace(
            "https://build.protomaps.com/20260614.pmtiles",
            "file:///etc/passwd",
        );
        assert!(matches!(
            RegionManifest::parse(&json),
            Err(ManifestError::BadPlanetUrl(_))
        ));
    }

    // ── tier / continent range checks ──────────────────────────────────────────

    #[test]
    fn rejects_out_of_range_tier_half_deg() {
        // 200° latitude half-width exceeds the 85° hemisphere clamp → BadTier.
        let json = valid_json().replace("[7.5, 6.0]", "[7.5, 200.0]");
        assert!(matches!(
            RegionManifest::parse(&json),
            Err(ManifestError::BadTier(_))
        ));
    }

    #[test]
    fn rejects_negative_tier_half_deg() {
        let json = valid_json().replace("[1.0, 0.75]", "[-1.0, 0.75]");
        assert!(matches!(
            RegionManifest::parse(&json),
            Err(ManifestError::BadTier(_))
        ));
    }

    #[test]
    fn rejects_out_of_range_continent_bbox() {
        let json = valid_json().replace("[-170.0, 5.0, -50.0, 84.0]", "[-170.0, 5.0, -50.0, 999.0]");
        assert!(matches!(
            RegionManifest::parse(&json),
            Err(ManifestError::BadContinent(_))
        ));
    }

    #[test]
    fn rejects_inverted_continent_bbox() {
        // west >= east
        let json = valid_json().replace("[-170.0, 5.0, -50.0, 84.0]", "[-50.0, 5.0, -170.0, 84.0]");
        assert!(matches!(
            RegionManifest::parse(&json),
            Err(ManifestError::BadContinent(_))
        ));
    }

    #[test]
    fn rejects_empty_tiers() {
        let json = serde_json::json!({
            "schema": MANIFEST_SCHEMA,
            "planet_build": "20260608",
            "planet_url": "https://build.protomaps.com/20260608.pmtiles",
            "pmtiles_schema": { "planetiler_version": 4, "vector_layers": [] },
            "tiers": [],
            "continents": []
        })
        .to_string();
        assert_eq!(RegionManifest::parse(&json), Err(ManifestError::NoTiers));
    }
}
