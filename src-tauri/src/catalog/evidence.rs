//! FT-8 evidence corroboration (Station Intelligence redesign, spec §2).
//!
//! Rust twin of the frontend `src/catalog/ft8Evidence.ts` (Task 6). Pure math:
//! decides which catalog gateways are "corroborated" by recent FT-8 decode
//! evidence, so the MCP `find_stations` surface reports the SAME corroboration
//! an operator sees in the panel.
//!
//! THE FIXTURE AT `tests/fixtures/evidence/basic.json` IS A CROSS-LANGUAGE
//! CONTRACT: it is a byte-identical copy of the frontend's
//! `src/catalog/__fixtures__/evidence/basic.json`, and this module reproduces
//! its `expectCorroborated` / `expectSampledBands` exactly (the inline `tests`
//! module asserts both). A repo-level `cmp` guard keeps the two copies in sync.
//!
//! Semantics mirrored verbatim from the TS module (each is load-bearing):
//! - A decode with NO grid is skipped (an ungridded decode cannot be geo-anchored).
//! - A decode whose grid is present but UNPARSEABLE is DROPPED entirely (the
//!   great-circle helper returns `None`, exactly as `distanceFromGrids` returns
//!   `null` through `gridToLatLon`).
//! - Distances are the great-circle km the existing catalog surface uses
//!   (`crate::position::geo`), converted to statute miles via the same
//!   `km_to_mi` the UI's `kmToMi` mirrors.

use std::collections::HashSet;

use crate::position::geo::{distance_bearing_between_grids, km_to_mi};

/// A decode more than this old relative to `EvidenceInput::now_ms` cannot
/// corroborate anything. (TS `EVIDENCE_RECENCY_MS = 30 * 60 * 1000`.)
pub const EVIDENCE_RECENCY_MS: u64 = 30 * 60 * 1000;

/// Default SNR floor a caller (the UI) starts the threshold at.
/// (TS `EVIDENCE_SNR_MIN_DB_DEFAULT = -24`.)
pub const EVIDENCE_SNR_MIN_DB_DEFAULT: i32 = -24;

/// `evidence_radius_mi` scales the operator-to-decode distance by this factor.
/// (TS `EVIDENCE_RADIUS_FACTOR = 0.15`.)
pub const EVIDENCE_RADIUS_FACTOR: f64 = 0.15;

/// `evidence_radius_mi` floor, miles. (TS `EVIDENCE_RADIUS_MIN_MI = 50`.)
pub const EVIDENCE_RADIUS_MIN_MI: f64 = 50.0;

/// `evidence_radius_mi` cap, miles. (TS `EVIDENCE_RADIUS_MAX_MI = 750`.)
pub const EVIDENCE_RADIUS_MAX_MI: f64 = 750.0;

/// Caller context for one corroboration pass.
pub struct EvidenceInput<'a> {
    /// The operator's own Maidenhead grid (the recency-window reference point's
    /// spatial anchor). An empty or unparseable grid drops every decode.
    pub operator_grid: &'a str,
    /// Unix millis "now": the reference point for the recency window.
    pub now_ms: u64,
    /// Caller-supplied SNR floor (the UI threshold; not necessarily the default).
    pub snr_min_db: i32,
}

/// Output of [`corroborate`]: the corroborated gateway keys plus the bands that
/// carried at least one qualifying decode in-window (independent of any gateway
/// match). Mirrors the TS `EvidenceResult`'s `corroborated` + `sampledBands`
/// (the `considered` count is not surfaced on the agent side).
pub struct Corroboration {
    /// Keys of every gateway corroborated by at least one qualifying decode.
    pub corroborated: HashSet<String>,
    /// Bands with >= 1 qualifying decode in-window, in first-occurrence order.
    pub sampled_bands: Vec<String>,
}

/// A decode's plausible corroboration radius scales with how far the operator
/// heard the SAME decode: a short operator-to-decode path implies a tight local
/// band opening (radius floors at [`EVIDENCE_RADIUS_MIN_MI`]), while a long DX
/// path implies conditions could plausibly carry evidence over a wider radius
/// (capped at [`EVIDENCE_RADIUS_MAX_MI`] so a single very-long DX contact cannot
/// "corroborate" an entire continent). Mirrors TS `evidenceRadiusMi`.
pub fn evidence_radius_mi(operator_to_heard_mi: f64) -> f64 {
    let raw = EVIDENCE_RADIUS_FACTOR * operator_to_heard_mi;
    EVIDENCE_RADIUS_MAX_MI.min(EVIDENCE_RADIUS_MIN_MI.max(raw))
}

/// Great-circle km between two grids, or `None` when either is absent/malformed.
/// The Rust twin of the TS `distanceFromGrids`: `distance_bearing_between_grids`
/// null-guards through `grid_to_lat_lon` exactly as `gridToLatLon` does.
fn distance_km(grid_a: &str, grid_b: &str) -> Option<f64> {
    distance_bearing_between_grids(Some(grid_a), Some(grid_b)).map(|(km, _bearing)| km)
}

/// A decode that passed the grid/recency/SNR gate, with its operator distance
/// memoized once (computed per decode, not per decode×gateway).
struct QualifyingDecode {
    band: String,
    grid: String,
    operator_dist_mi: f64,
}

/// Decide which gateways are corroborated by recent FT-8 decode evidence.
///
/// `gateways` are `(key, grid, bands)` triples: `key` uniquely identifies the
/// gateway to the caller (it is echoed back in [`Corroboration::corroborated`]),
/// `grid` is the gateway's Maidenhead locator, and `bands` are the amateur-band
/// labels (e.g. `"20m"`) the gateway operates on.
///
/// `decodes` are `(grid, band, snr_db, slot_utc_ms)` tuples straight off the
/// FT-8 decode ring: `grid` is `None` for an ungridded decode, `band` is the
/// slot's band (the decode itself carries only an audio offset).
///
/// A gateway is corroborated iff some qualifying decode D has a band matching one
/// of the gateway's bands AND the decode-to-gateway distance (miles) is within
/// [`evidence_radius_mi`] of the operator-to-decode distance (miles). One
/// qualifying decode is enough. Mirrors TS `corroborateStations`.
pub fn corroborate(
    gateways: &[(String, String, Vec<String>)],
    decodes: &[(Option<String>, String, i32, u64)],
    input: &EvidenceInput,
) -> Corroboration {
    // 1. Collect qualifying decodes (grid present + parseable, in-window, above
    //    the SNR floor), memoizing each one's operator distance in miles.
    let mut qualifying: Vec<QualifyingDecode> = Vec::new();
    for (grid_opt, band, snr_db, slot_utc_ms) in decodes {
        // Ungridded decodes cannot be geo-corroborated (TS `if (!decode.grid)`).
        let Some(grid) = grid_opt.as_deref() else {
            continue;
        };
        // Stale: `nowMs - slotUtcMs > EVIDENCE_RECENCY_MS`. `saturating_sub`
        // matches the TS `<= 0` (future slot) branch: a future-stamped decode
        // yields 0, which is never `>` the window, so it is retained just as the
        // TS negative difference is.
        if input.now_ms.saturating_sub(*slot_utc_ms) > EVIDENCE_RECENCY_MS {
            continue;
        }
        // Below the caller's SNR floor.
        if *snr_db < input.snr_min_db {
            continue;
        }
        // An unparseable operator grid OR decode grid yields `None` here and the
        // decode is DROPPED (it can anchor no radius). Twin of `distanceFromGrids`
        // returning `null`.
        let Some(operator_dist_km) = distance_km(input.operator_grid, grid) else {
            continue;
        };
        qualifying.push(QualifyingDecode {
            band: band.clone(),
            grid: grid.to_string(),
            operator_dist_mi: km_to_mi(operator_dist_km),
        });
    }

    // 2. Sampled bands: unique bands of qualifying decodes, first-occurrence
    //    order (TS `[...new Set(qualifying.map((d) => d.band))]`).
    let mut sampled_bands: Vec<String> = Vec::new();
    for q in &qualifying {
        if !sampled_bands.contains(&q.band) {
            sampled_bands.push(q.band.clone());
        }
    }

    // 3. Per gateway: corroborated iff a qualifying decode on a shared band lands
    //    within the decode's evidence radius.
    let mut corroborated: HashSet<String> = HashSet::new();
    for (key, grid, bands) in gateways {
        let gateway_bands: HashSet<&str> = bands.iter().map(String::as_str).collect();
        if gateway_bands.is_empty() {
            continue;
        }
        for d in &qualifying {
            if !gateway_bands.contains(d.band.as_str()) {
                continue;
            }
            let Some(gateway_dist_km) = distance_km(&d.grid, grid) else {
                continue;
            };
            if km_to_mi(gateway_dist_km) <= evidence_radius_mi(d.operator_dist_mi) {
                corroborated.insert(key.clone());
                break; // one qualifying decode is enough
            }
        }
    }

    Corroboration {
        corroborated,
        sampled_bands,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    /// BYTE-IDENTICAL copy of the frontend fixture (Task 6). The repo-level `cmp`
    /// guard (wire-walk, Task 13) proves the two files never drift.
    const FIXTURE: &str = include_str!("../../tests/fixtures/evidence/basic.json");

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FxStation {
        key: String,
        grid: String,
        bands: Vec<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FxDecode {
        grid: Option<String>,
        band: String,
        snr_db: i32,
        slot_utc_ms: u64,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Fixture {
        operator_grid: String,
        now_ms: u64,
        snr_min_db: i32,
        stations: Vec<FxStation>,
        decodes: Vec<FxDecode>,
        expect_corroborated: Vec<String>,
        expect_sampled_bands: Vec<String>,
    }

    fn run_fixture(fx: &Fixture) -> Corroboration {
        let gateways: Vec<(String, String, Vec<String>)> = fx
            .stations
            .iter()
            .map(|s| (s.key.clone(), s.grid.clone(), s.bands.clone()))
            .collect();
        let decodes: Vec<(Option<String>, String, i32, u64)> = fx
            .decodes
            .iter()
            .map(|d| (d.grid.clone(), d.band.clone(), d.snr_db, d.slot_utc_ms))
            .collect();
        let input = EvidenceInput {
            operator_grid: &fx.operator_grid,
            now_ms: fx.now_ms,
            snr_min_db: fx.snr_min_db,
        };
        corroborate(&gateways, &decodes, &input)
    }

    #[test]
    fn fixture_reproduces_expect_corroborated_exactly() {
        let fx: Fixture = serde_json::from_str(FIXTURE).expect("fixture parses");
        let out = run_fixture(&fx);

        let mut got: Vec<String> = out.corroborated.into_iter().collect();
        got.sort();
        let mut want = fx.expect_corroborated.clone();
        want.sort();
        assert_eq!(
            got, want,
            "corroborated set must match the cross-language fixture exactly"
        );
    }

    #[test]
    fn fixture_reproduces_expect_sampled_bands_exactly() {
        let fx: Fixture = serde_json::from_str(FIXTURE).expect("fixture parses");
        let out = run_fixture(&fx);
        assert_eq!(
            out.sampled_bands, fx.expect_sampled_bands,
            "sampled bands must match the cross-language fixture exactly"
        );
    }

    // The same three boundary cases the TS module asserts.
    #[test]
    fn evidence_radius_floors_at_50_miles() {
        // 0.15 * 100 = 15, clamped up to the 50-mile floor.
        assert_eq!(evidence_radius_mi(100.0), 50.0);
    }

    #[test]
    fn evidence_radius_scales_linearly_inside_the_band() {
        // 0.15 * 1500 = 225, inside [50, 750].
        assert_eq!(evidence_radius_mi(1500.0), 225.0);
    }

    #[test]
    fn evidence_radius_caps_at_750_miles() {
        // 0.15 * 10000 = 1500, clamped down to the 750-mile cap.
        assert_eq!(evidence_radius_mi(10000.0), 750.0);
    }

    #[test]
    fn ungridded_decode_is_skipped_not_corroborating() {
        // A decode with no grid names no location: it cannot corroborate the
        // co-located gateway even on a shared band, and it contributes no
        // sampled band (mirrors TS `if (!decode.grid) continue`).
        let gateways = vec![("gw".to_string(), "DN17".to_string(), vec!["20m".to_string()])];
        let decodes = vec![(None, "20m".to_string(), 0, 1_000_000_000u64)];
        let input = EvidenceInput {
            operator_grid: "DN17",
            now_ms: 1_000_000_000,
            snr_min_db: EVIDENCE_SNR_MIN_DB_DEFAULT,
        };
        let out = corroborate(&gateways, &decodes, &input);
        assert!(out.corroborated.is_empty());
        assert!(out.sampled_bands.is_empty());
    }

    #[test]
    fn unparseable_grid_decode_is_dropped_entirely() {
        // A decode whose grid is PRESENT but unparseable is dropped (the distance
        // helper returns None), so it neither corroborates nor samples a band.
        let gateways = vec![("gw".to_string(), "DN17".to_string(), vec!["20m".to_string()])];
        let decodes = vec![(Some("ZZ99".to_string()), "20m".to_string(), 0, 1_000_000_000u64)];
        let input = EvidenceInput {
            operator_grid: "DN17",
            now_ms: 1_000_000_000,
            snr_min_db: EVIDENCE_SNR_MIN_DB_DEFAULT,
        };
        let out = corroborate(&gateways, &decodes, &input);
        assert!(out.corroborated.is_empty());
        assert!(out.sampled_bands.is_empty());
    }
}
