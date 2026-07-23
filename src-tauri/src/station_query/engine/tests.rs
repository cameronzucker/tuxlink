//! Per-intent engine tests (P5.1–P5.5) + snapshot narrowing/expiry, all against
//! an in-memory fixture population (no `AppHandle`, no mocking).

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tuxlink_mcp_core::ports::{GatewayDto, StationModeDto};
use tuxlink_mcp_core::station_query::{
    Band, BoundedU8, BoundedVec, CandidateId, Callsign, ConnectObjective, FindStationsRequest,
    RecommendationGoal, StationExportFormat, StationFacet, StationFilters,
};

use super::{ExportArtifact, ExportRow, ExportSink, StationContext, StationQueryEngine, StationQueryError};
use crate::station_query::snapshot::{SnapshotError, SnapshotStore};

const NOW: u64 = 1_700_000_000_000;

fn gw(callsign: &str, freq_khz: f64, dist_mi: Option<f64>) -> GatewayDto {
    GatewayDto {
        mode: StationModeDto::VaraHf,
        channel: format!("{freq_khz} VARA HF"),
        callsign: callsign.into(),
        grid: Some("FN31".into()),
        frequencies_khz: vec![freq_khz],
        channels: Vec::new(),
        antenna: None,
        distance_km: dist_mi.map(|m| m / 0.621_371),
        distance_mi: dist_mi,
        bearing_deg: Some(90.0),
        ft8_corroborated: None,
    }
}

fn ctx(population: Vec<GatewayDto>) -> StationContext {
    StationContext {
        operator_grid: Some("DM43".into()),
        now_ms: NOW,
        population,
        prior_success_callsigns: HashSet::new(),
        unavailable_inputs: vec!["path_reliability"],
        export_sink: None,
    }
}

fn count(n: u8) -> BoundedU8<1, 8> {
    BoundedU8::new(n).unwrap()
}

// --------------------------------------------------------------------------
// P5.1 explore / refine
// --------------------------------------------------------------------------

#[test]
fn explore_broad_returns_refinement_required_zero_rows_and_stays_small() {
    // 1,400 distinct 40m gateways — the overflow case the whole redesign targets.
    let pop: Vec<_> = (0..1400)
        .map(|i| gw(&format!("W{i}AA"), 7100.0, Some(100.0)))
        .collect();
    let store = SnapshotStore::new(60_000);
    let engine = StationQueryEngine::new(&store);
    let resp = engine
        .evaluate(
            FindStationsRequest::Explore {
                filters: StationFilters::default(),
                snapshot_id: None,
            },
            &ctx(pop),
        )
        .unwrap();

    let json = serde_json::to_value(&resp).unwrap();
    assert_eq!(json["result"]["kind"], "refinement-required");
    assert_eq!(json["result"]["matched_stations"], 1400);
    assert_eq!(json["population"]["matched_stations"], 1400);
    assert!(
        json["result"].get("stations").is_none(),
        "refinement-required must carry ZERO station rows"
    );
    let facets = json["result"]["facets"].as_array().unwrap();
    assert!(facets.iter().any(|f| f["field"] == "band"));

    // The invariant in miniature: the broad result is tiny, never the ~250k-token dump.
    let bytes = serde_json::to_vec(&resp).unwrap().len();
    assert!(bytes < 32_768, "serialized {bytes} bytes must be < 32 KB");
}

#[test]
fn explore_small_eligible_returns_complete_set() {
    let pop: Vec<_> = (0..12)
        .map(|i| gw(&format!("W{i}AA"), 7100.0, Some(100.0)))
        .collect();
    let store = SnapshotStore::new(60_000);
    let engine = StationQueryEngine::new(&store);
    let resp = engine
        .evaluate(
            FindStationsRequest::Explore {
                filters: StationFilters::default(),
                snapshot_id: None,
            },
            &ctx(pop),
        )
        .unwrap();
    let json = serde_json::to_value(&resp).unwrap();
    assert_eq!(json["result"]["kind"], "complete-set");
    assert_eq!(json["result"]["stations"].as_array().unwrap().len(), 12);
    assert!(json["result"].get("omitted_stations").is_none());
}

// --------------------------------------------------------------------------
// P5.2 recommend / rank
// --------------------------------------------------------------------------

fn recommend(
    objective: ConnectObjective,
    exclude: Vec<String>,
    n: u8,
) -> FindStationsRequest {
    let (excl, _) = BoundedVec::from_capped(exclude.iter().map(|s| CandidateId::from_truncated(s)));
    FindStationsRequest::Recommend {
        goal: RecommendationGoal::ConnectNow {
            at_utc_ms: None,
            objective,
        },
        filters: StationFilters::default(),
        candidate_count: count(n),
        exclude_candidate_ids: excl,
    }
}

fn scored_population() -> Vec<GatewayDto> {
    let mut ft8 = gw("W1FT8", 7100.0, Some(200.0));
    ft8.ft8_corroborated = Some(true);
    let prior = gw("W2PRI", 7100.0, Some(200.0));
    let plain = gw("W3PLN", 7100.0, Some(200.0));
    vec![ft8, prior, plain]
}

#[test]
fn recommend_ranks_ft8_over_prior_over_plain() {
    let store = SnapshotStore::new(60_000);
    let engine = StationQueryEngine::new(&store);
    let mut c = ctx(scored_population());
    c.prior_success_callsigns.insert("W2PRI".into());

    let resp = engine
        .evaluate(recommend(ConnectObjective::EstimatedSuccess, vec![], 3), &c)
        .unwrap();
    let json = serde_json::to_value(&resp).unwrap();
    assert_eq!(json["result"]["kind"], "ranked-subset");
    let cands = json["result"]["top_candidates"].as_array().unwrap();
    assert_eq!(cands[0]["callsign"], "W1FT8");
    assert_eq!(cands[1]["callsign"], "W2PRI");
    assert_eq!(cands[2]["callsign"], "W3PLN");
    assert_eq!(json["result"]["coverage"]["evaluated_stations"], 3);
    assert_eq!(json["result"]["coverage"]["returned_stations"], 3);
    assert_eq!(json["result"]["coverage"]["omitted_stations"], 0);
    assert_eq!(json["result"]["coverage"]["relationship"], "top-of-all-eligible");
    // FT8 candidate carries its reason code + one selected connection.
    let reasons = cands[0]["fitness"]["reason_codes"].as_array().unwrap();
    assert!(reasons.iter().any(|r| r == "FT8_CORROBORATED"));
    assert!(cands[0]["selected_connection"]["frequency_khz"].is_number());
}

#[test]
fn recommend_broad_is_bounded_ranked_subset_with_omitted() {
    let pop: Vec<_> = (0..1400)
        .map(|i| gw(&format!("W{i}AA"), 7100.0, Some(100.0)))
        .collect();
    let store = SnapshotStore::new(60_000);
    let engine = StationQueryEngine::new(&store);
    let resp = engine
        .evaluate(recommend(ConnectObjective::Nearest, vec![], 3), &ctx(pop))
        .unwrap();
    let json = serde_json::to_value(&resp).unwrap();
    assert_eq!(json["result"]["kind"], "ranked-subset");
    assert_eq!(json["result"]["top_candidates"].as_array().unwrap().len(), 3);
    assert_eq!(json["result"]["coverage"]["evaluated_stations"], 1400);
    assert_eq!(json["result"]["coverage"]["omitted_stations"], 1397);
    assert_eq!(json["result"]["ranking"]["policy"], "nearest-v1");
    let bytes = serde_json::to_vec(&resp).unwrap().len();
    assert!(bytes < 32_768, "serialized {bytes} bytes must be < 32 KB");
}

#[test]
fn recommend_exclude_yields_next_best_across_snapshots() {
    let store = SnapshotStore::new(60_000);
    let engine = StationQueryEngine::new(&store);
    let mut c = ctx(scored_population());
    c.prior_success_callsigns.insert("W2PRI".into());

    // First call: the top is W1FT8 (ft8-corroborated).
    let first = engine
        .evaluate(recommend(ConnectObjective::EstimatedSuccess, vec![], 1), &c)
        .unwrap();
    let first_json = serde_json::to_value(&first).unwrap();
    let top_id = first_json["result"]["top_candidates"][0]["candidate_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(first_json["result"]["top_candidates"][0]["callsign"], "W1FT8");

    // Excluding it (by an id minted under the PREVIOUS snapshot) still drops it —
    // exclusion matches on callsign, not the volatile snapshot prefix.
    let second = engine
        .evaluate(
            recommend(ConnectObjective::EstimatedSuccess, vec![top_id], 1),
            &c,
        )
        .unwrap();
    let second_json = serde_json::to_value(&second).unwrap();
    assert_eq!(second_json["result"]["top_candidates"][0]["callsign"], "W2PRI");
}

// --------------------------------------------------------------------------
// P5.3 lookup
// --------------------------------------------------------------------------

fn lookup(callsigns: Vec<&str>) -> FindStationsRequest {
    let (cs, _) = BoundedVec::from_capped(callsigns.iter().map(|c| Callsign::from_truncated(c)));
    FindStationsRequest::Lookup {
        snapshot_id: None,
        callsigns: cs,
    }
}

#[test]
fn lookup_matches_case_insensitively() {
    let pop = vec![
        gw("W1ABC", 7100.0, Some(50.0)),
        gw("K2XYZ", 7100.0, Some(60.0)),
        gw("N3QRS", 7100.0, None),
    ];
    let store = SnapshotStore::new(60_000);
    let engine = StationQueryEngine::new(&store);
    let resp = engine.evaluate(lookup(vec!["W1ABC", "k2xyz"]), &ctx(pop)).unwrap();
    let json = serde_json::to_value(&resp).unwrap();
    assert_eq!(json["result"]["kind"], "complete-set");
    let stations = json["result"]["stations"].as_array().unwrap();
    assert_eq!(stations.len(), 2);
}

#[test]
fn lookup_no_match_is_no_matches() {
    let pop = vec![gw("W1ABC", 7100.0, Some(50.0))];
    let store = SnapshotStore::new(60_000);
    let engine = StationQueryEngine::new(&store);
    let resp = engine.evaluate(lookup(vec!["ZZ9ZZZ"]), &ctx(pop)).unwrap();
    let json = serde_json::to_value(&resp).unwrap();
    assert_eq!(json["result"]["kind"], "no-matches");
}

// --------------------------------------------------------------------------
// P5.4 aggregate
// --------------------------------------------------------------------------

#[test]
fn aggregate_counts_the_full_population_not_a_subset() {
    let mut pop = Vec::new();
    for i in 0..200 {
        pop.push(gw(&format!("A{i}"), 7100.0, Some(100.0))); // 40m
    }
    for i in 0..100 {
        pop.push(gw(&format!("B{i}"), 14100.0, Some(100.0))); // 20m
    }
    let store = SnapshotStore::new(60_000);
    let engine = StationQueryEngine::new(&store);
    let (group_by, _) = BoundedVec::from_capped(vec![StationFacet::Band]);
    let resp = engine
        .evaluate(
            FindStationsRequest::Aggregate {
                filters: StationFilters::default(),
                group_by,
            },
            &ctx(pop),
        )
        .unwrap();
    let json = serde_json::to_value(&resp).unwrap();
    assert_eq!(json["result"]["kind"], "aggregate-complete");
    let buckets = json["result"]["groups"][0]["buckets"].as_array().unwrap();
    let get = |band: &str| -> u64 {
        buckets
            .iter()
            .find(|b| b["value"] == band)
            .map(|b| b["count"].as_u64().unwrap())
            .unwrap_or(0)
    };
    assert_eq!(get("40m"), 200);
    assert_eq!(get("20m"), 100);
}

// --------------------------------------------------------------------------
// P5.5 export
// --------------------------------------------------------------------------

struct FakeSink {
    rows_written: AtomicUsize,
}

impl ExportSink for FakeSink {
    fn write(&self, rows: &[ExportRow], _format: StationExportFormat) -> Result<ExportArtifact, String> {
        self.rows_written.store(rows.len(), Ordering::SeqCst);
        Ok(ExportArtifact {
            artifact_id: "art-1".into(),
            destination: "~/Downloads/stations.csv".into(),
        })
    }
}

#[test]
fn export_writes_artifact_and_inlines_no_rows() {
    let pop = vec![gw("W1ABC", 7100.0, Some(50.0)), gw("K2XYZ", 7100.0, Some(60.0))];
    let sink = Arc::new(FakeSink {
        rows_written: AtomicUsize::new(0),
    });
    let mut c = ctx(pop);
    c.export_sink = Some(sink.clone());
    let store = SnapshotStore::new(60_000);
    let engine = StationQueryEngine::new(&store);
    let resp = engine
        .evaluate(
            FindStationsRequest::Export {
                filters: StationFilters::default(),
                format: StationExportFormat::Csv,
            },
            &c,
        )
        .unwrap();
    let json = serde_json::to_value(&resp).unwrap();
    assert_eq!(json["result"]["kind"], "export-ready");
    assert_eq!(json["result"]["total_rows"], 2);
    assert_eq!(json["result"]["destination"], "~/Downloads/stations.csv");
    // The artifact is out-of-transcript: no catalog rows inline.
    assert!(json["result"].get("stations").is_none());
    assert!(json["result"].get("top_candidates").is_none());
    assert_eq!(sink.rows_written.load(Ordering::SeqCst), 2);
}

#[test]
fn export_without_sink_is_a_typed_error() {
    let pop = vec![gw("W1ABC", 7100.0, None)];
    let store = SnapshotStore::new(60_000);
    let engine = StationQueryEngine::new(&store);
    let err = engine
        .evaluate(
            FindStationsRequest::Export {
                filters: StationFilters::default(),
                format: StationExportFormat::Csv,
            },
            &ctx(pop),
        )
        .unwrap_err();
    assert!(matches!(err, StationQueryError::Export(_)));
}

// --------------------------------------------------------------------------
// Snapshot narrowing / widening / expiry (through the engine)
// --------------------------------------------------------------------------

fn mixed_band_population() -> Vec<GatewayDto> {
    let mut pop = Vec::new();
    for i in 0..200 {
        pop.push(gw(&format!("A{i}"), 7100.0, Some(100.0))); // 40m
    }
    for i in 0..100 {
        pop.push(gw(&format!("B{i}"), 14100.0, Some(100.0))); // 20m
    }
    pop
}

#[test]
fn explore_then_narrow_against_snapshot_counts_are_stable() {
    let store = SnapshotStore::new(60_000);
    let engine = StationQueryEngine::new(&store);
    let c = ctx(mixed_band_population());

    let first = engine
        .evaluate(
            FindStationsRequest::Explore {
                filters: StationFilters::default(),
                snapshot_id: None,
            },
            &c,
        )
        .unwrap();
    let snap_id = first.snapshot.id.clone();

    // A different ctx (empty population) proves the snapshot's pinned population
    // is what gets narrowed — not a re-fetch.
    let mut c2 = ctx(Vec::new());
    c2.now_ms = NOW + 1000;
    let narrowed = engine
        .evaluate(
            FindStationsRequest::Explore {
                filters: StationFilters {
                    bands: BoundedVec::from_capped(vec![Band::B40m]).0,
                    ..Default::default()
                },
                snapshot_id: Some(snap_id),
            },
            &c2,
        )
        .unwrap();
    let json = serde_json::to_value(&narrowed).unwrap();
    assert_eq!(json["population"]["matched_stations"], 200);
}

#[test]
fn widening_a_snapshot_is_rejected() {
    let store = SnapshotStore::new(60_000);
    let engine = StationQueryEngine::new(&store);
    let c = ctx(mixed_band_population());

    let first = engine
        .evaluate(
            FindStationsRequest::Explore {
                filters: StationFilters {
                    bands: BoundedVec::from_capped(vec![Band::B40m]).0,
                    ..Default::default()
                },
                snapshot_id: None,
            },
            &c,
        )
        .unwrap();
    let snap_id = first.snapshot.id.clone();

    // Dropping the 40m constraint would WIDEN — rejected.
    let err = engine
        .evaluate(
            FindStationsRequest::Explore {
                filters: StationFilters::default(),
                snapshot_id: Some(snap_id),
            },
            &c,
        )
        .unwrap_err();
    assert!(matches!(err, StationQueryError::SnapshotWiden));
}

#[test]
fn expired_snapshot_is_a_typed_error() {
    let store = SnapshotStore::new(1000);
    let engine = StationQueryEngine::new(&store);
    let c = ctx(vec![gw("W1ABC", 7100.0, None)]);

    let first = engine
        .evaluate(
            FindStationsRequest::Explore {
                filters: StationFilters::default(),
                snapshot_id: None,
            },
            &c,
        )
        .unwrap();
    let snap_id = first.snapshot.id.clone();

    let mut c2 = ctx(Vec::new());
    c2.now_ms = NOW + 2000; // past the 1000ms TTL
    let err = engine
        .evaluate(
            FindStationsRequest::Explore {
                filters: StationFilters::default(),
                snapshot_id: Some(snap_id),
            },
            &c2,
        )
        .unwrap_err();
    assert!(matches!(
        err,
        StationQueryError::Snapshot(SnapshotError::Expired)
    ));
}
