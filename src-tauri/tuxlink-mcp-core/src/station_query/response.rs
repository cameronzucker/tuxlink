//! The `find_stations` response: a common `snapshot` + `population` envelope plus
//! a tagged `result` union (spec §Response).
//!
//! The union is where the invariant becomes structural. Two facts make "silent
//! partial as complete" *unrepresentable*:
//!
//! 1. [`StationResult::CompleteSet`] has **no** omitted/total field — its very
//!    shape asserts the returned rows are the whole eligible population. There is
//!    no field in which to hide "there were more."
//! 2. The only subset-bearing variant, [`StationResult::RankedSubset`], carries a
//!    **mandatory** [`SubsetCoverage`] with exact evaluated/returned/omitted
//!    counts and an explicit `relationship: "top-of-all-eligible"`.
//!
//! So an engine that has extra rows cannot express them as a `CompleteSet`; the
//! only shapes that admit "more exist" are `RankedSubset` (counted) and
//! `RefinementRequired` (zero rows + facet counts). The guarded
//! [`StationResult::complete_set`] constructor (refuses a non-zero `omitted`) is
//! belt-and-suspenders on top of that structural guarantee.
//!
//! Every collection is a [`BoundedVec`] and every string a [`CappedString`], so a
//! worst legal value is small (property-tested `< 32 KB` in P8) — a broad query
//! can never emit output fatal to the agent's context window.

use std::fmt;

use schemars::JsonSchema;
use serde::Serialize;

use super::bounded::{BoundedVec, CappedString};
use super::request::{Callsign, CandidateId, SnapshotId, StationExportFormat, StationFacet, StationFilters};
use crate::ports::{GatewayDto, StationModeDto};

/// Raised only if the engine ever tries to build a result that would violate the
/// invariant (e.g. a `complete-set` with omitted rows). Internal-only; it should
/// never fire in normal operation — it exists to fail loud rather than emit a
/// misleading payload (spec §"postcondition contract-violation error").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractViolation {
    pub detail: String,
}

impl fmt::Display for ContractViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "find_stations contract violation: {}", self.detail)
    }
}

impl std::error::Error for ContractViolation {}

// ---------------------------------------------------------------------------
// Envelope
// ---------------------------------------------------------------------------

/// The full `find_stations` response.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct FindStationsResponse {
    pub snapshot: SnapshotMeta,
    pub population: Population,
    pub result: StationResult,
}

impl FindStationsResponse {
    #[must_use]
    pub fn new(snapshot: SnapshotMeta, population: Population, result: StationResult) -> Self {
        Self {
            snapshot,
            population,
            result,
        }
    }

    /// A fixed single-station `complete-set` — for test doubles / seeded fixtures
    /// that stand in for the real engine.
    #[must_use]
    pub fn single_station(
        snapshot_id: &str,
        callsign: &str,
        grid: Option<&str>,
        mode: StationModeDto,
        frequencies_khz: &[f64],
    ) -> Self {
        let (freqs, _) = BoundedVec::<f64, 8>::from_capped(frequencies_khz.iter().copied());
        let (stations, _) = BoundedVec::<StationSummary, 16>::from_capped(std::iter::once(StationSummary {
            callsign: CappedString::from_truncated(callsign),
            grid: grid.map(CappedString::from_truncated),
            mode,
            frequencies_khz: freqs,
            distance_mi: None,
            bearing_deg: None,
            operating_now: None,
        }));
        Self::new(
            SnapshotMeta {
                id: CappedString::from_truncated(snapshot_id),
                fetched_at_ms: 0,
                operator_grid: None,
                expires_at_ms: 0,
            },
            Population::new(1, 1, frequencies_khz.len() as u32),
            StationResult::CompleteSet { stations },
        )
    }

    /// An explicitly complete, empty result — for a void fixture world.
    #[must_use]
    pub fn empty(snapshot_id: &str) -> Self {
        Self::new(
            SnapshotMeta {
                id: CappedString::from_truncated(snapshot_id),
                fetched_at_ms: 0,
                operator_grid: None,
                expires_at_ms: 0,
            },
            Population::new(0, 0, 0),
            StationResult::NoMatches,
        )
    }

    /// Build a bounded result directly from a raw gateway list — for the scenario
    /// testserver, which seeds `GatewayDto`s (not the real engine's grouped
    /// stations). One summary per gateway: `<= 16` -> complete-set, more ->
    /// refinement-required (count only), none -> no-matches. Never inlines more
    /// than the bound.
    #[must_use]
    pub fn from_gateways(snapshot_id: &str, gateways: &[GatewayDto]) -> Self {
        let meta = SnapshotMeta {
            id: CappedString::from_truncated(snapshot_id),
            fetched_at_ms: 0,
            operator_grid: None,
            expires_at_ms: 0,
        };
        let total = gateways.len() as u32;
        if gateways.is_empty() {
            return Self::new(meta, Population::new(0, 0, 0), StationResult::NoMatches);
        }
        let conn: u32 = gateways.iter().map(|g| g.frequencies_khz.len() as u32).sum();
        if gateways.len() <= 16 {
            let (stations, _) =
                BoundedVec::<StationSummary, 16>::from_capped(gateways.iter().map(gateway_summary));
            return Self::new(
                meta,
                Population::new(total, total, conn),
                StationResult::CompleteSet { stations },
            );
        }
        Self::new(
            meta,
            Population::new(total, total, conn),
            StationResult::RefinementRequired {
                matched_stations: total,
                facets: BoundedVec::empty(),
                suggested_refinements: BoundedVec::empty(),
            },
        )
    }
}

/// Flatten one seeded `GatewayDto` into a compact summary (no callsign grouping;
/// the real engine groups, but the scenario seeds are already one-row-per-station).
fn gateway_summary(g: &GatewayDto) -> StationSummary {
    let (freqs, _) = BoundedVec::<f64, 8>::from_capped(g.frequencies_khz.iter().copied());
    StationSummary {
        callsign: CappedString::from_truncated(&g.callsign),
        grid: g.grid.as_deref().map(CappedString::from_truncated),
        mode: g.mode,
        frequencies_khz: freqs,
        distance_mi: g.distance_mi,
        bearing_deg: g.bearing_deg,
        operating_now: None,
    }
}

/// Provenance for the population this result was computed over. `explore` /
/// `lookup` narrow against `id` so counts stay stable between calls.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct SnapshotMeta {
    pub id: SnapshotId,
    pub fetched_at_ms: u64,
    pub operator_grid: Option<CappedString<8>>,
    pub expires_at_ms: u64,
}

/// Counts over the FULL matched/eligible population — always exact, never a
/// sampled subset. `matched_stations` is everything the filters matched;
/// `eligible_stations` is those that survived eligibility (e.g. a resolvable
/// connection); `eligible_connection_options` counts channels, not stations.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Population {
    /// Always `"station"` — the unit the counts are in (stations, not channel rows).
    pub count_unit: &'static str,
    pub matched_stations: u32,
    pub eligible_stations: u32,
    pub eligible_connection_options: u32,
}

impl Population {
    #[must_use]
    pub fn new(matched_stations: u32, eligible_stations: u32, eligible_connection_options: u32) -> Self {
        Self {
            count_unit: "station",
            matched_stations,
            eligible_stations,
            eligible_connection_options,
        }
    }
}

// ---------------------------------------------------------------------------
// The tagged result union
// ---------------------------------------------------------------------------

/// The tagged result. Each variant has hard structural bounds (spec §Bounds).
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum StationResult {
    /// The ENTIRE eligible population fit the bound. No omitted field exists —
    /// the shape asserts completeness. Build via [`StationResult::complete_set`].
    CompleteSet { stations: BoundedVec<StationSummary, 16> },
    /// A ranked top-K of all eligible. Carries mandatory exact coverage so a
    /// subset can never read as complete.
    RankedSubset {
        ranking: RankingMeta,
        coverage: SubsetCoverage,
        top_candidates: BoundedVec<Candidate, 8>,
    },
    /// Too broad: ZERO rows, an exact `matched_stations` total, finite per-facet
    /// counts, and bounded additive-filter suggestions. The agent narrows by
    /// predicate against the snapshot, never by paging.
    RefinementRequired {
        matched_stations: u32,
        facets: BoundedVec<Facet, 8>,
        suggested_refinements: BoundedVec<Refinement, 12>,
    },
    /// Server-side counts/statistics over the whole matched population (no rows).
    AggregateComplete { groups: BoundedVec<AggregateGroup, 3> },
    /// A user CSV/JSON artifact OUTSIDE the transcript — no catalog data inline.
    ExportReady {
        artifact_id: CappedString<64>,
        format: StationExportFormat,
        total_rows: u32,
        destination: CappedString<128>,
    },
    /// An explicitly *complete* empty result.
    NoMatches,
}

impl StationResult {
    /// Build a `complete-set`, refusing (with a [`ContractViolation`]) if any
    /// stations were omitted — a complete set must be the whole population.
    /// `omitted` is the count [`BoundedVec::from_capped`] reported.
    pub fn complete_set(
        stations: BoundedVec<StationSummary, 16>,
        omitted: usize,
    ) -> Result<Self, ContractViolation> {
        if omitted != 0 {
            return Err(ContractViolation {
                detail: format!(
                    "complete-set requires omitted == 0, got {omitted}; use ranked-subset or refinement-required"
                ),
            });
        }
        Ok(StationResult::CompleteSet { stations })
    }
}

// ---------------------------------------------------------------------------
// Result payload sub-types
// ---------------------------------------------------------------------------

/// A compact station row for `complete-set` / `lookup`.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct StationSummary {
    pub callsign: Callsign,
    pub grid: Option<CappedString<8>>,
    pub mode: StationModeDto,
    pub frequencies_khz: BoundedVec<f64, 8>,
    pub distance_mi: Option<f64>,
    pub bearing_deg: Option<f64>,
    pub operating_now: Option<bool>,
}

/// Exact coverage for a `ranked-subset` — the field set that makes a subset
/// impossible to mistake for the whole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SubsetCoverage {
    pub evaluated_stations: u32,
    pub returned_stations: u32,
    pub omitted_stations: u32,
    /// Always `"top-of-all-eligible"` — these are the top of the full eligible
    /// set, not the top of a partially-scanned window.
    pub relationship: &'static str,
}

impl SubsetCoverage {
    #[must_use]
    pub fn top_of_all_eligible(evaluated_stations: u32, returned_stations: u32, omitted_stations: u32) -> Self {
        Self {
            evaluated_stations,
            returned_stations,
            omitted_stations,
            relationship: "top-of-all-eligible",
        }
    }
}

/// The versioned ranking policy + which inputs it could and could not use. Its
/// scope MUST be `evaluated == eligible`; if the engine cannot evaluate the full
/// eligible population it returns `refinement-required`, never an approximate
/// best (spec §"Ranking honesty").
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct RankingMeta {
    /// e.g. `"connect-now-v1"` (fitness) or `"nearest-v1"` (distance-only, never
    /// labelled fitness).
    pub policy: &'static str,
    pub inputs_used: BoundedVec<CappedString<32>, 8>,
    pub inputs_unavailable: BoundedVec<CappedString<32>, 8>,
}

/// One ranked candidate — a station with exactly ONE selected connection (this is
/// what keeps `recommend` bounded regardless of a gateway's channel count).
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Candidate {
    pub candidate_id: CandidateId,
    pub callsign: Callsign,
    pub grid: Option<CappedString<8>>,
    pub selected_connection: ConnectionDto,
    pub alternate_connection_count: u32,
    pub fitness: Fitness,
}

/// The single connection a candidate recommends dialing.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct ConnectionDto {
    pub target_callsign: Callsign,
    pub mode: StationModeDto,
    pub frequency_khz: f64,
    pub bandwidth_hz: Option<u32>,
}

/// A candidate's fitness under the versioned policy.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Fitness {
    pub score: f32,
    pub components: FitnessComponents,
    pub reason_codes: BoundedVec<CappedString<24>, 6>,
}

/// The evidence that fed a fitness score. `None` = the input was unavailable
/// (surfaced in [`RankingMeta::inputs_unavailable`]), not "zero".
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct FitnessComponents {
    pub path_reliability: Option<f32>,
    pub ft8_corroborated: Option<bool>,
    pub operating_now: Option<bool>,
    pub prior_success: Option<bool>,
}

/// One facet in a `refinement-required` result: how many stations would REMAIN
/// if each value were added as a filter (so the agent narrows by consequence).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Facet {
    pub field: StationFacet,
    pub values: BoundedVec<FacetCount, 24>,
}

/// One facet value + the population that would remain if it were applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct FacetCount {
    pub value: CappedString<32>,
    pub remaining_if_applied: u32,
}

/// A labelled additive-filter patch the agent can echo into its next `explore`,
/// with the exact resulting count. Narrows by predicate, never by page cursor.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Refinement {
    pub label: CappedString<48>,
    pub add_filters: StationFilters,
    pub remaining: u32,
}

/// One `aggregate` group: a facet and its exact per-value counts over the FULL
/// matched population (not a sampled subset).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct AggregateGroup {
    pub facet: StationFacet,
    pub buckets: BoundedVec<AggregateBucket, 24>,
}

/// One value + its exact count within the matched population.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct AggregateBucket {
    pub value: CappedString<32>,
    pub count: u32,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::station_query::bounded::BoundedVec;

    fn summary(callsign: &str) -> StationSummary {
        let (freqs, _) = BoundedVec::<f64, 8>::from_capped(vec![7103.5]);
        StationSummary {
            callsign: Callsign::from_truncated(callsign),
            grid: Some(CappedString::from_truncated("FN31")),
            mode: StationModeDto::VaraHf,
            frequencies_khz: freqs,
            distance_mi: Some(120.0),
            bearing_deg: Some(45.0),
            operating_now: Some(true),
        }
    }

    #[test]
    fn complete_set_rejects_nonzero_omitted() {
        let (stations, omitted) = BoundedVec::<StationSummary, 16>::from_capped(
            (0..20).map(|i| summary(&format!("W{i}AA"))),
        );
        assert_eq!(omitted, 4);
        assert!(StationResult::complete_set(stations, omitted).is_err());
    }

    #[test]
    fn complete_set_accepts_zero_omitted_and_tags_kind() {
        let (stations, omitted) =
            BoundedVec::<StationSummary, 16>::from_capped(vec![summary("W1ABC")]);
        let result = StationResult::complete_set(stations, omitted).unwrap();
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["kind"], "complete-set");
        assert_eq!(json["stations"][0]["callsign"], "W1ABC");
        // No omitted/total field to lie with.
        assert!(json.get("omitted_stations").is_none());
    }

    #[test]
    fn ranked_subset_carries_all_three_coverage_counts() {
        let result = StationResult::RankedSubset {
            ranking: RankingMeta {
                policy: "connect-now-v1",
                inputs_used: BoundedVec::from_capped(vec![CappedString::from_truncated("path")]).0,
                inputs_unavailable: BoundedVec::empty(),
            },
            coverage: SubsetCoverage::top_of_all_eligible(206, 8, 198),
            top_candidates: BoundedVec::empty(),
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["kind"], "ranked-subset");
        assert_eq!(json["coverage"]["evaluated_stations"], 206);
        assert_eq!(json["coverage"]["returned_stations"], 8);
        assert_eq!(json["coverage"]["omitted_stations"], 198);
        assert_eq!(json["coverage"]["relationship"], "top-of-all-eligible");
    }

    #[test]
    fn refinement_required_has_zero_rows() {
        let result = StationResult::RefinementRequired {
            matched_stations: 1400,
            facets: BoundedVec::from_capped(vec![Facet {
                field: StationFacet::Band,
                values: BoundedVec::from_capped(vec![FacetCount {
                    value: CappedString::from_truncated("40m"),
                    remaining_if_applied: 311,
                }])
                .0,
            }])
            .0,
            suggested_refinements: BoundedVec::empty(),
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["kind"], "refinement-required");
        assert_eq!(json["matched_stations"], 1400);
        // Zero station rows — the whole point.
        assert!(json.get("stations").is_none());
        assert_eq!(json["facets"][0]["field"], "band");
        assert_eq!(json["facets"][0]["values"][0]["remaining_if_applied"], 311);
    }

    #[test]
    fn no_matches_is_a_bare_tag() {
        let json = serde_json::to_value(StationResult::NoMatches).unwrap();
        assert_eq!(json["kind"], "no-matches");
    }

    // ---- P8 load-bearing property test: worst legal value < 32 KB ----------

    const BYTE_BUDGET: usize = 32_768;

    fn cap<const N: usize>() -> CappedString<N> {
        CappedString::from_truncated(&"X".repeat(N))
    }

    fn max_filters() -> StationFilters {
        use crate::station_query::request::{Band, BandwidthClass, BearingSector, DistanceBucket, Ft8Policy};
        StationFilters {
            modes: BoundedVec::from_capped([
                StationModeDto::VaraHf,
                StationModeDto::VaraFm,
                StationModeDto::Packet,
                StationModeDto::ArdopHf,
                StationModeDto::Pactor,
                StationModeDto::RobustPacket,
            ])
            .0,
            // Only 10 bands exist — that IS the worst legal value (cap is 16).
            bands: BoundedVec::from_capped([
                Band::B160m,
                Band::B80m,
                Band::B60m,
                Band::B40m,
                Band::B30m,
                Band::B20m,
                Band::B17m,
                Band::B15m,
                Band::B12m,
                Band::B10m,
            ])
            .0,
            bandwidths: BoundedVec::from_capped([
                BandwidthClass::Hz500,
                BandwidthClass::Hz1000,
                BandwidthClass::Hz2000,
                BandwidthClass::Hz2300,
                BandwidthClass::Hz2750,
            ])
            .0,
            ft8_policy: Ft8Policy::Require,
            operating_now: Some(true),
            distance: Some(DistanceBucket::Within2500mi),
            bearing: Some(BearingSector::Nw),
            callsign_prefix: Some(cap::<12>()),
            history_hours: Some(720),
        }
    }

    fn max_envelope(result: StationResult) -> FindStationsResponse {
        FindStationsResponse::new(
            SnapshotMeta {
                id: cap::<32>(),
                fetched_at_ms: u64::MAX,
                operator_grid: Some(cap::<8>()),
                expires_at_ms: u64::MAX,
            },
            Population::new(u32::MAX, u32::MAX, u32::MAX),
            result,
        )
    }

    fn max_summary() -> StationSummary {
        StationSummary {
            callsign: cap::<16>(),
            grid: Some(cap::<8>()),
            mode: StationModeDto::RobustPacket,
            frequencies_khz: BoundedVec::from_capped([1e9_f64; 8]).0,
            distance_mi: Some(f64::MAX),
            bearing_deg: Some(359.999),
            operating_now: Some(true),
        }
    }

    fn max_candidate() -> Candidate {
        Candidate {
            candidate_id: cap::<96>(),
            callsign: cap::<16>(),
            grid: Some(cap::<8>()),
            selected_connection: ConnectionDto {
                target_callsign: cap::<16>(),
                mode: StationModeDto::VaraHf,
                frequency_khz: 1e9,
                bandwidth_hz: Some(u32::MAX),
            },
            alternate_connection_count: u32::MAX,
            fitness: Fitness {
                score: 1.0,
                components: FitnessComponents {
                    path_reliability: Some(1.0),
                    ft8_corroborated: Some(true),
                    operating_now: Some(true),
                    prior_success: Some(true),
                },
                reason_codes: BoundedVec::from_capped([
                    cap::<24>(),
                    cap::<24>(),
                    cap::<24>(),
                    cap::<24>(),
                    cap::<24>(),
                    cap::<24>(),
                ])
                .0,
            },
        }
    }

    /// Serialize the WORST legal value of every result variant and prove each
    /// stays under the byte budget — the invariant that makes a broad query
    /// un-survivable-overflow impossible by construction (spec §Testing).
    #[test]
    fn worst_legal_value_is_under_byte_budget() {
        let mut worst = 0usize;

        // complete-set: 16 max summaries.
        let (stations, _) = BoundedVec::<StationSummary, 16>::from_capped((0..16).map(|_| max_summary()));
        worst = worst.max(check(max_envelope(StationResult::CompleteSet { stations })));

        // ranked-subset: 8 max candidates + full ranking meta.
        let (cands, _) = BoundedVec::<Candidate, 8>::from_capped((0..8).map(|_| max_candidate()));
        let (used, _) = BoundedVec::<CappedString<32>, 8>::from_capped((0..8).map(|_| cap::<32>()));
        let (unavail, _) = BoundedVec::<CappedString<32>, 8>::from_capped((0..8).map(|_| cap::<32>()));
        worst = worst.max(check(max_envelope(StationResult::RankedSubset {
            ranking: RankingMeta {
                policy: "connect-now-v1",
                inputs_used: used,
                inputs_unavailable: unavail,
            },
            coverage: SubsetCoverage::top_of_all_eligible(u32::MAX, 8, u32::MAX),
            top_candidates: cands,
        })));

        // refinement-required: 8 facets x 24 values + 12 refinements w/ max filters.
        let (facets, _) = BoundedVec::<Facet, 8>::from_capped((0..8).map(|_| {
            let (values, _) = BoundedVec::<FacetCount, 24>::from_capped((0..24).map(|_| FacetCount {
                value: cap::<32>(),
                remaining_if_applied: u32::MAX,
            }));
            Facet {
                field: StationFacet::Band,
                values,
            }
        }));
        let (refinements, _) = BoundedVec::<Refinement, 12>::from_capped((0..12).map(|_| Refinement {
            label: cap::<48>(),
            add_filters: max_filters(),
            remaining: u32::MAX,
        }));
        worst = worst.max(check(max_envelope(StationResult::RefinementRequired {
            matched_stations: u32::MAX,
            facets,
            suggested_refinements: refinements,
        })));

        // aggregate-complete: 3 groups x 24 buckets.
        let (groups, _) = BoundedVec::<AggregateGroup, 3>::from_capped((0..3).map(|_| {
            let (buckets, _) = BoundedVec::<AggregateBucket, 24>::from_capped((0..24).map(|_| AggregateBucket {
                value: cap::<32>(),
                count: u32::MAX,
            }));
            AggregateGroup {
                facet: StationFacet::Mode,
                buckets,
            }
        }));
        worst = worst.max(check(max_envelope(StationResult::AggregateComplete { groups })));

        // export-ready + no-matches (small, for completeness).
        worst = worst.max(check(max_envelope(StationResult::ExportReady {
            artifact_id: cap::<64>(),
            format: StationExportFormat::Csv,
            total_rows: u32::MAX,
            destination: cap::<128>(),
        })));
        worst = worst.max(check(max_envelope(StationResult::NoMatches)));

        assert!(
            worst < BYTE_BUDGET,
            "worst legal value serialized to {worst} bytes; budget is {BYTE_BUDGET}"
        );
    }

    fn check(resp: FindStationsResponse) -> usize {
        let n = serde_json::to_vec(&resp).unwrap().len();
        assert!(n < BYTE_BUDGET, "a variant serialized to {n} bytes (>= {BYTE_BUDGET})");
        n
    }

    #[test]
    fn full_envelope_serializes() {
        let response = FindStationsResponse::new(
            SnapshotMeta {
                id: SnapshotId::from_truncated("sq_abc123"),
                fetched_at_ms: 1_700_000_000_000,
                operator_grid: Some(CappedString::from_truncated("DM43")),
                expires_at_ms: 1_700_000_060_000,
            },
            Population::new(311, 206, 487),
            StationResult::NoMatches,
        );
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["population"]["count_unit"], "station");
        assert_eq!(json["population"]["matched_stations"], 311);
        assert_eq!(json["snapshot"]["operator_grid"], "DM43");
        assert_eq!(json["result"]["kind"], "no-matches");
    }
}
