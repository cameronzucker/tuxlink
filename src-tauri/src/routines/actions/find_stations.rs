//! `data.find_stations` — spec compat-tree rank 2 (routines-round2). A distance-
//! sorted Winlink gateway-directory query whose deduped callsign array feeds
//! `radio.connect`'s `stations` list (the marquee `$s.callsigns` → connect
//! composition proved in `tuxlink-routines`'s `composability_proof.rs`).
//!
//! Unlike `data.read` (a params-selected read source), this is a NEW ACTION: it
//! takes query params (`modes` / `bands` / `history_hours` / `limit`) and is
//! declared `needs_internet: true` (it polls the Winlink status API through the
//! polite cache). It reuses the EXACT MCP `find_stations` path — the shared
//! [`crate::mcp_ports::curate_and_rank_gateways`] curation + ranking + the
//! shared [`crate::mcp_ports::resolve_operator_broadcast_grid`] — so the curated
//! gateway rows are byte-identical to the MCP tool by construction, then layers
//! the find_stations-specific dedup + `limit`:
//!
//! 1. Fetch raw listings via the [`super::StationQueryService`] seam.
//! 2. Curate + band-filter + distance-sort (shared fn).
//! 3. Dedup callsigns preserving the distance-sorted order.
//! 4. Truncate the DEDUPED callsign list to `limit` (over DISTINCT callsigns,
//!    never rows — the nearest station occupying N rows counts as one).
//! 5. `gateways` = every gateway ROW whose callsign survived the truncation
//!    (a surviving callsign keeps ALL its rows/modes).
//!
//! Output: `{"gateways":[...GatewayDto...], "fetched_at_ms": u64|null,
//! "operator_grid": str|null, "callsigns":[...]}`.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

use tuxlink_routines::action::{Action, ActionDescriptor, OutputSpec, ParamSpec, ValueType};
use tuxlink_routines::error::StepError;

use super::{StationDirectory, StationQueryService};

const DATA_FIND_STATIONS: &str = "data.find_stations";

/// Shape-true dry-run output for `data.find_stations` (D6): a params-blind,
/// empty-but-well-shaped directory result whose single `DRYRUN-1` callsign lets
/// a routine's `$s.callsigns` → `radio.connect` composition take its "found a
/// station" arm in a dry run without hitting the Winlink status API.
fn find_stations_dry_run_shape(_params: &Value) -> Value {
    json!({
        "gateways": [],
        "callsigns": ["DRYRUN-1"],
        "fetched_at_ms": null,
        "operator_grid": null,
        "dry_run": true
    })
}

/// `data.find_stations` params. All optional. `modes` uses the same kebab-case
/// [`ListingMode`](crate::catalog::stations::ListingMode) enum as
/// `data.stationlist_update` (a `Vec<String>` would silently accept garbage
/// mode tokens); an empty/absent `modes` resolves to every transport, VARA FM
/// included (`ListingMode::expand_selector`, shared with the MCP
/// `find_stations` tool), in the seam.
#[derive(Debug, Deserialize)]
struct FindStationsParams {
    #[serde(default)]
    modes: Vec<crate::catalog::stations::ListingMode>,
    #[serde(default)]
    bands: Vec<String>,
    /// Only gateways heard within this many hours; `None` means no bound.
    /// Validated `<= 720` up front via the same `validate_history_hours` helper
    /// the MCP `find_stations` port uses — a 721 is rejected verbatim.
    #[serde(default)]
    history_hours: Option<u32>,
    /// Cap the number of DISTINCT callsigns returned. `Some(0)` is invalid
    /// params (a request for zero stations is a mistake, not an empty result).
    #[serde(default)]
    limit: Option<usize>,
}

/// `data.find_stations` — distance-sorted gateway query feeding `radio.connect`.
/// `needs_internet: true` only (reads the public directory; never transmits,
/// never touches the rig, never writes config).
pub struct FindStations {
    station_query: Arc<dyn StationQueryService>,
}

impl FindStations {
    pub fn new(station_query: Arc<dyn StationQueryService>) -> Self {
        Self { station_query }
    }
}

#[async_trait]
impl Action for FindStations {
    fn descriptor(&self) -> ActionDescriptor {
        ActionDescriptor {
            writes_config: false,
            name: DATA_FIND_STATIONS,
            label: "Find gateway stations",
            description:
                "Query the Winlink gateway directory (distance-sorted) for radio.connect to dial.",
            needs_radio: false,
            transmits: false,
            needs_internet: true,
            example_params: Some(r#"{"modes":["vara-hf"],"limit":3}"#),
            allowed_values: None,
            params: &[
                ParamSpec {
                    key: "modes",
                    ty: ValueType::StringList,
                    required: false,
                    description: "Listing modes to include (all when omitted)",
                    allowed: Some(&["vara-hf", "packet", "ardop-hf", "pactor", "robust-packet"]),
                    example: r#"["vara-hf"]"#,
                },
                ParamSpec {
                    key: "bands",
                    ty: ValueType::BandList,
                    required: false,
                    description: "Bands to include (all when omitted)",
                    allowed: None,
                    example: r#"["20m","40m"]"#,
                },
                ParamSpec {
                    key: "history_hours",
                    ty: ValueType::Number,
                    required: false,
                    description: "Directory history window, hours",
                    allowed: None,
                    example: "6",
                },
                ParamSpec {
                    key: "limit",
                    ty: ValueType::Number,
                    required: false,
                    description: "Cap on DISTINCT callsigns after distance-sorted dedup",
                    allowed: None,
                    example: "10",
                },
            ],
            outputs: &[
                OutputSpec {
                    key: "callsigns",
                    ty: ValueType::StationList,
                    description: "Distance-sorted deduped callsigns — feed radio.connect's \
                                  stations as a whole-value ref: \"$sN.callsigns\"",
                    nullable: false,
                },
                OutputSpec {
                    key: "gateways",
                    ty: ValueType::ObjectList,
                    description: "Full gateway records (callsign, bands, distance, grid)",
                    nullable: false,
                },
                OutputSpec {
                    key: "fetched_at_ms",
                    ty: ValueType::Number,
                    description: "Directory snapshot timestamp, unix ms",
                    nullable: true,
                },
                OutputSpec {
                    key: "operator_grid",
                    ty: ValueType::String,
                    description: "The operator grid distances were computed from; may be null",
                    nullable: true,
                },
            ],
            dry_run_shape: Some(find_stations_dry_run_shape),
        }
    }

    async fn execute(&self, params: Value, cancel: CancellationToken) -> Result<Value, StepError> {
        let parsed: FindStationsParams =
            serde_json::from_value(params).map_err(|e| StepError::Action {
                action: DATA_FIND_STATIONS.to_string(),
                cause: format!("invalid params: {e}"),
            })?;

        // `limit: Some(0)` is a nonsense request — reject as invalid params
        // BEFORE any fetch. An empty result would be indistinguishable from a
        // real empty directory, silently hiding the author's mistake.
        if parsed.limit == Some(0) {
            return Err(StepError::Action {
                action: DATA_FIND_STATIONS.to_string(),
                cause: "invalid params: limit must be >= 1 (0 requests zero stations)".to_string(),
            });
        }

        // Validate the optional history bound (cap 720 h) up front via the SAME
        // helper the MCP `find_stations` port uses — a 721 is rejected verbatim,
        // identically to the tool, before any fetch happens.
        tuxlink_mcp_core::validate::validate_history_hours(parsed.history_hours).map_err(|e| {
            StepError::Action {
                action: DATA_FIND_STATIONS.to_string(),
                cause: format!("invalid params: {e}"),
            }
        })?;

        let directory = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(StepError::Cancelled),
            res = self
                .station_query
                .fetch_directory(parsed.modes, parsed.history_hours) => res,
        }
        .map_err(|cause| StepError::Action {
            action: DATA_FIND_STATIONS.to_string(),
            cause,
        })?;

        // SAME path as the MCP `find_stations` tool: curate (PII/free-text
        // dropped, bogus callsigns dropped, grid validated) + band filter +
        // distance sort, via the SHARED `curate_and_rank_gateways`.
        let gateways = crate::mcp_ports::curate_and_rank_gateways(
            &directory.listings,
            &parsed.bands,
            directory.operator_grid.as_deref(),
        );

        // Dedup callsigns preserving the distance-sorted order. `callsigns` is
        // derived ONLY from the POST-curation `gateways` above, so a gateway
        // curation dropped (bogus callsign) contributes no callsign here.
        let mut seen: HashSet<&str> = HashSet::new();
        let mut callsigns: Vec<String> = Vec::new();
        for g in &gateways {
            if seen.insert(g.callsign.as_str()) {
                callsigns.push(g.callsign.clone());
            }
        }

        // Truncate the DEDUPED callsign list to `limit` (over DISTINCT
        // callsigns, never rows).
        if let Some(limit) = parsed.limit {
            callsigns.truncate(limit);
        }

        // `gateways` = every gateway ROW whose callsign survived the truncation
        // (a surviving callsign keeps ALL its rows/modes), preserving sort order.
        let kept: HashSet<&str> = callsigns.iter().map(String::as_str).collect();
        let gateways: Vec<_> = gateways
            .into_iter()
            .filter(|g| kept.contains(g.callsign.as_str()))
            .collect();

        // Serialize the curated rows explicitly (GatewayDto: Serialize) so the
        // JSON build never depends on `json!`'s Serialize-interpolation.
        let gateways_json =
            serde_json::to_value(&gateways).map_err(|e| StepError::Action {
                action: DATA_FIND_STATIONS.to_string(),
                cause: format!("output serialize: {e}"),
            })?;

        Ok(json!({
            "gateways": gateways_json,
            "fetched_at_ms": directory.fetched_at_ms,
            "operator_grid": directory.operator_grid,
            "callsigns": callsigns,
        }))
    }
}

// ============================================================================
// Real seam adapter — MonolithStationQueryService. Follows the same `AppHandle`
// + `.state::<T>()`-resolved-fresh-per-call pattern as `data.rs`'s
// `MonolithDataService`. The fetch routes through the SAME polite cache-backed
// poll (`catalog_fetch_stations`) the MCP `find_stations` port and the
// Find-a-Station UI use; the operator grid resolves through the SHARED
// `mcp_ports::resolve_operator_broadcast_grid`.
// ============================================================================

pub struct MonolithStationQueryService {
    app: AppHandle,
}

impl MonolithStationQueryService {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

#[async_trait]
impl StationQueryService for MonolithStationQueryService {
    async fn fetch_directory(
        &self,
        modes: Vec<crate::catalog::stations::ListingMode>,
        history_hours: Option<u32>,
    ) -> Result<StationDirectory, String> {
        use crate::catalog::stations::ListingMode;
        use tauri::Manager;

        // Empty ⇒ all transports (VARA FM included), via the SHARED
        // `ListingMode::expand_selector` seam: the SAME expansion the MCP
        // `find_stations` tool routes through, so the two agent surfaces are
        // identical by construction (Codex P2 parity fix).
        let modes = ListingMode::expand_selector(modes);
        // The SAME polite cache-backed poll the `catalog_fetch_stations` command
        // (and the MCP `find_stations` port) route through.
        let cache = self
            .app
            .state::<Arc<crate::catalog::stations_cache::StationsCache>>();
        let channels_cache = self
            .app
            .state::<Arc<crate::catalog::channels_cache::ChannelsCache>>();
        let listings = crate::catalog::commands::catalog_fetch_stations(
            modes,
            history_hours,
            cache,
            channels_cache,
        )
        .await
        .map_err(|e| format!("{e:?}"))?;
        // Most-recent fetch stamp across the fetched modes (None on a fresh parse).
        let fetched_at_ms = listings.iter().filter_map(|l| l.fetched_at_ms).max();
        // Operator's own 4-char grid for distance ranking (None when unresolved).
        let operator_grid = crate::mcp_ports::resolve_operator_broadcast_grid(&self.app);
        Ok(StationDirectory {
            listings,
            operator_grid,
            fetched_at_ms,
        })
    }
}

// ============================================================================
// Tests — trait fake, no hardware/tauri. The seam returns RAW listings so the
// action's real curate → dedup → limit ordering is exercised over the SHARED
// `mcp_ports::curate_and_rank_gateways`.
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::stations::{Gateway, ListingMode, StationListing};

    // ---- FakeStationQueryService ------------------------------------------
    // Panics if `fetch_directory` is called when a test didn't expect it (the
    // history_hours-721 rejection test relies on this: the reject happens
    // BEFORE any fetch, so the fake must never be invoked).

    type FetchFn = dyn Fn(Vec<ListingMode>, Option<u32>) -> Result<StationDirectory, String>
        + Send
        + Sync;

    struct FakeStationQueryService {
        fetch: Box<FetchFn>,
    }

    impl Default for FakeStationQueryService {
        fn default() -> Self {
            Self {
                fetch: Box::new(|_, _| panic!("fetch_directory not expected in this test")),
            }
        }
    }

    impl FakeStationQueryService {
        fn with_fetch(
            mut self,
            f: impl Fn(Vec<ListingMode>, Option<u32>) -> Result<StationDirectory, String>
                + Send
                + Sync
                + 'static,
        ) -> Self {
            self.fetch = Box::new(f);
            self
        }

        /// Convenience: always return `directory`, ignoring the query params.
        fn returning(directory: StationDirectory) -> Self {
            // StationDirectory is not Clone (StationListing is Clone, but the
            // struct isn't derived Clone); rebuild from cloned parts each call.
            let listings = directory.listings.clone();
            let operator_grid = directory.operator_grid.clone();
            let fetched_at_ms = directory.fetched_at_ms;
            Self::default().with_fetch(move |_, _| {
                Ok(StationDirectory {
                    listings: listings.clone(),
                    operator_grid: operator_grid.clone(),
                    fetched_at_ms,
                })
            })
        }
    }

    #[async_trait]
    impl StationQueryService for FakeStationQueryService {
        async fn fetch_directory(
            &self,
            modes: Vec<ListingMode>,
            history_hours: Option<u32>,
        ) -> Result<StationDirectory, String> {
            (self.fetch)(modes, history_hours)
        }
    }

    // ---- fixture builders --------------------------------------------------

    /// A minimal, plausible gateway with all PII/free-text fields set to a
    /// non-`None` value so curation's DROP behavior is observable in tests.
    fn gw(callsign: &str, channel: &str, grid: Option<&str>) -> Gateway {
        Gateway {
            channel: channel.to_string(),
            callsign: callsign.to_string(),
            sysop_name: Some("Jane Operator".to_string()),
            grid: grid.map(str::to_string),
            location: Some("Somewhere, XX".to_string()),
            frequencies_khz: vec![7100.0],
            last_update: Some("Sat, 06 Jun 2026 08:10:00 GMT".to_string()),
            email: Some("op@example.com".to_string()),
            homepage: Some("http://example.com".to_string()),
            antenna: None,
            channel_details: Vec::new(),
        }
    }

    fn listing(gateways: Vec<Gateway>) -> StationListing {
        StationListing {
            mode: ListingMode::VaraHf,
            title: None,
            gateways,
            raw: String::new(),
            parsed_ok: true,
            fetched_at_ms: Some(1_760_000_000_000),
        }
    }

    fn directory(gateways: Vec<Gateway>, operator_grid: Option<&str>) -> StationDirectory {
        StationDirectory {
            listings: vec![listing(gateways)],
            operator_grid: operator_grid.map(str::to_string),
            fetched_at_ms: Some(1_760_000_000_000),
        }
    }

    fn action(fake: FakeStationQueryService) -> FindStations {
        FindStations::new(Arc::new(fake))
    }

    fn callsigns(out: &Value) -> Vec<String> {
        out["callsigns"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect()
    }

    fn gateway_callsigns(out: &Value) -> Vec<String> {
        out["gateways"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| g["callsign"].as_str().unwrap().to_string())
            .collect()
    }

    // ---- limit over DISTINCT callsigns (not rows) --------------------------

    #[tokio::test]
    async fn limit_truncates_over_distinct_callsigns_not_rows() {
        // The nearest station occupies THREE rows (same callsign, three
        // channels). With all grids null, distances are all None, so the
        // stable sort preserves directory order → the 3 AA1AA rows are first.
        // `limit: 3` must yield 3 DISTINCT callsigns (AA1AA, BB2BB, CC3CC),
        // NOT 3 ROWS (which would be all AA1AA).
        let dir = directory(
            vec![
                gw("AA1AA", "AA1AA-A.WINLINK", None),
                gw("AA1AA", "AA1AA-B.WINLINK", None),
                gw("AA1AA", "AA1AA-C.WINLINK", None),
                gw("BB2BB", "BB2BB.WINLINK", None),
                gw("CC3CC", "CC3CC.WINLINK", None),
                gw("DD4DD", "DD4DD.WINLINK", None),
            ],
            None,
        );
        let out = action(FakeStationQueryService::returning(dir))
            .execute(json!({ "limit": 3 }), CancellationToken::new())
            .await
            .unwrap();

        assert_eq!(callsigns(&out), vec!["AA1AA", "BB2BB", "CC3CC"]);
        // Every ROW of a surviving callsign is kept: 3×AA1AA + BB2BB + CC3CC.
        assert_eq!(
            gateway_callsigns(&out),
            vec!["AA1AA", "AA1AA", "AA1AA", "BB2BB", "CC3CC"]
        );
        // DD4DD fell outside the distinct-callsign limit → gone entirely.
        assert!(!gateway_callsigns(&out).contains(&"DD4DD".to_string()));
    }

    // ---- null-grid directory-order truncation ------------------------------

    #[tokio::test]
    async fn null_grid_truncation_follows_directory_order() {
        // All grids null (and no operator grid) → all distances null → the
        // stable sort preserves directory order, so `limit` truncates in that
        // exact order rather than by an accidental distance ranking.
        let dir = directory(
            vec![
                gw("W1FIRST", "W1FIRST.WINLINK", None),
                gw("W2SECOND", "W2SECOND.WINLINK", None),
                gw("W3THIRD", "W3THIRD.WINLINK", None),
            ],
            None,
        );
        let out = action(FakeStationQueryService::returning(dir))
            .execute(json!({ "limit": 2 }), CancellationToken::new())
            .await
            .unwrap();

        assert_eq!(callsigns(&out), vec!["W1FIRST", "W2SECOND"]);
        assert_eq!(out["operator_grid"], Value::Null);
        // Every returned gateway has a null distance (no operator grid).
        for g in out["gateways"].as_array().unwrap() {
            assert_eq!(g["distance_km"], Value::Null);
        }
    }

    // ---- empty result is not an error --------------------------------------

    #[tokio::test]
    async fn empty_directory_is_not_an_error() {
        let dir = StationDirectory {
            listings: vec![],
            operator_grid: None,
            fetched_at_ms: None,
        };
        let out = action(FakeStationQueryService::returning(dir))
            .execute(json!({}), CancellationToken::new())
            .await
            .expect("an empty directory is a normal result, never an error");

        assert_eq!(out["gateways"], json!([]));
        assert_eq!(out["callsigns"], json!([]));
        assert_eq!(out["fetched_at_ms"], Value::Null);
        assert_eq!(out["operator_grid"], Value::Null);
    }

    // ---- history_hours 721 rejected verbatim, before any fetch -------------

    #[tokio::test]
    async fn history_hours_over_720_rejected_verbatim_without_fetching() {
        // The default fake panics if fetch_directory is ever called — proving
        // the reject happens BEFORE the fetch, identical to the MCP tool.
        let err = action(FakeStationQueryService::default())
            .execute(json!({ "history_hours": 721 }), CancellationToken::new())
            .await
            .expect_err("721 h exceeds the 720 h cap");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "data.find_stations");
                // Verbatim validator message (via validate_history_hours).
                assert!(
                    cause.contains("history_hours must be <= 720"),
                    "cause must carry the verbatim validator message, got: {cause}"
                );
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn history_hours_720_is_accepted() {
        // Boundary: exactly 720 is valid (the cap is inclusive).
        let dir = directory(vec![gw("K7OK", "K7OK.WINLINK", None)], None);
        let out = action(FakeStationQueryService::returning(dir))
            .execute(json!({ "history_hours": 720 }), CancellationToken::new())
            .await
            .expect("720 h is within the inclusive cap");
        assert_eq!(callsigns(&out), vec!["K7OK"]);
    }

    // ---- limit: Some(0) is invalid params ----------------------------------

    #[tokio::test]
    async fn limit_zero_is_invalid_params_without_fetching() {
        let err = action(FakeStationQueryService::default())
            .execute(json!({ "limit": 0 }), CancellationToken::new())
            .await
            .expect_err("limit 0 is a nonsense request");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "data.find_stations");
                assert!(cause.contains("invalid params"), "got: {cause}");
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    // ---- PII omission pin vs MCP curation ----------------------------------

    #[tokio::test]
    async fn pii_and_free_text_are_omitted_matching_mcp_curation() {
        // The raw gateway carries sysop_name/email/homepage/location/last_update;
        // the curated output must carry NONE of them — identical to the MCP tool
        // (both go through the shared `curate_and_rank_gateways`).
        let raw = vec![gw("N0PII", "N0PII.WINLINK", Some("CN87"))];
        let dir = directory(raw.clone(), None);
        let out = action(FakeStationQueryService::returning(dir))
            .execute(json!({}), CancellationToken::new())
            .await
            .unwrap();

        let g = &out["gateways"].as_array().unwrap()[0];
        for pii in ["sysopName", "sysop_name", "email", "homepage", "location", "lastUpdate", "last_update"] {
            assert!(
                g.get(pii).is_none(),
                "curated gateway must omit PII/free-text field {pii}, got: {g}"
            );
        }
        // And the curated rows are byte-identical to the shared MCP curation fn
        // over the SAME raw listings.
        let mcp = crate::mcp_ports::curate_and_rank_gateways(&[listing(raw)], &[], None);
        assert_eq!(out["gateways"], serde_json::to_value(&mcp).unwrap());
    }

    // ---- callsigns derived only from POST-curation gateways ----------------

    #[tokio::test]
    async fn callsigns_derived_only_from_post_curation_gateways() {
        // A gateway with a bogus (implausible) callsign is DROPPED by curation;
        // it must contribute NEITHER a gateway row NOR a callsign. The valid
        // rows around it survive and are the only source of callsigns.
        let dir = directory(
            vec![
                gw("W7GOOD", "W7GOOD.WINLINK", None),
                gw("!!bogus!!", "junk.WINLINK", None), // curation drops this
                gw("K7ALSO", "K7ALSO.WINLINK", None),
            ],
            None,
        );
        let out = action(FakeStationQueryService::returning(dir))
            .execute(json!({}), CancellationToken::new())
            .await
            .unwrap();

        assert_eq!(callsigns(&out), vec!["W7GOOD", "K7ALSO"]);
        assert!(!callsigns(&out).iter().any(|c| c.contains('!')));
        assert!(!gateway_callsigns(&out).iter().any(|c| c.contains('!')));
    }

    // ---- band filter narrows via the shared path ---------------------------

    #[tokio::test]
    async fn band_filter_keeps_only_gateways_with_a_dial_in_a_requested_band() {
        // W7FORTY dials 7100 kHz (40m); W7TWENTY dials 14100 kHz (20m). A
        // `bands: ["40m"]` filter keeps only W7FORTY (same client-side band
        // filter the MCP tool applies).
        let mut forty = gw("W7FORTY", "W7FORTY.WINLINK", None);
        forty.frequencies_khz = vec![7100.0];
        let mut twenty = gw("W7TWENTY", "W7TWENTY.WINLINK", None);
        twenty.frequencies_khz = vec![14100.0];
        let dir = directory(vec![forty, twenty], None);

        let out = action(FakeStationQueryService::returning(dir))
            .execute(json!({ "bands": ["40m"] }), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(callsigns(&out), vec!["W7FORTY"]);
    }

    // ---- cancellation before fetch is prompt -------------------------------

    #[tokio::test]
    async fn pre_cancelled_token_returns_cancelled_without_fetching() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let err = action(FakeStationQueryService::default())
            .execute(json!({}), cancel)
            .await
            .expect_err("a pre-cancelled token must not fetch");
        assert!(matches!(err, StepError::Cancelled));
    }

    // ---- descriptor flags --------------------------------------------------

    #[test]
    fn descriptor_flags_needs_internet_only() {
        let a = action(FakeStationQueryService::default());
        let d = a.descriptor();
        assert_eq!(d.name, "data.find_stations");
        assert_eq!(d.label, "Find gateway stations");
        assert!(!d.label.is_empty() && !d.description.is_empty());
        assert!(d.needs_internet);
        assert!(!d.needs_radio);
        assert!(!d.transmits);
        assert!(!d.writes_config);
    }

    // ---- D6: authoring affordances + dry-run shape -------------------------

    #[test]
    fn descriptor_advertises_example_params_and_dry_run_shape() {
        let d = action(FakeStationQueryService::default()).descriptor();
        assert_eq!(d.example_params, Some(r#"{"modes":["vara-hf"],"limit":3}"#));
        assert!(d.dry_run_shape.is_some());
    }

    #[test]
    fn dry_run_shape_pins_callsigns_and_marks_dry_run() {
        let out = find_stations_dry_run_shape(&json!({}));
        assert_eq!(out["callsigns"], json!(["DRYRUN-1"]));
        assert_eq!(out["gateways"], json!([]));
        assert_eq!(out["fetched_at_ms"], Value::Null);
        assert_eq!(out["operator_grid"], Value::Null);
        assert_eq!(out["dry_run"], json!(true));
    }

    /// tuxlink-3nvvl: every descriptor's example_params must pass its own
    /// declared ParamSpecs — locks the registry backfill mechanically.
    #[test]
    fn descriptor_examples_pass_their_own_param_specs() {
        use tuxlink_routines::validate::params::example_self_check;
        let d = FindStations::new(Arc::new(FakeStationQueryService::default())).descriptor();
        let f = example_self_check(&d);
        assert!(f.is_empty(), "{}: {f:?}", d.name);
    }

    // ---- Codex P2 parity: empty modes expansion includes VARA FM -----------

    #[test]
    fn empty_modes_expansion_includes_vara_fm() {
        // The routines seam's empty-selector expansion is the SHARED
        // `ListingMode::expand_selector` (the exact call `fetch_directory`
        // makes), so `data.find_stations` with no `modes` fetches every
        // transport, VARA FM included, identical to the MCP `find_stations`
        // tool. Mirrors the MCP-side `expand_find_stations_modes` tests.
        let modes = ListingMode::expand_selector(Vec::new());
        assert!(
            modes.contains(&ListingMode::VaraFm),
            "routines empty selector must include VARA FM (agent-surface parity)"
        );
        for m in ListingMode::ALL {
            assert!(modes.contains(&m), "must retain every confirmed mode");
        }
    }
}
