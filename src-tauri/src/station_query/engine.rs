//! `StationQueryEngine` — the single place the `find_stations` invariant is
//! enforced. A pure function over `(FindStationsRequest, StationContext)`:
//! the P6/P7 adapters resolve the app-owned facts into a [`StationContext`]
//! (operator grid, current time, curated population, prior-success history) and
//! the engine turns intent + constraints into a **bounded** tagged response.
//!
//! The engine never fetches, never touches an `AppHandle`, and never blocks — so
//! it unit-tests against a fixture population with no mocking. It groups the raw
//! per-`(callsign, mode, channel)` catalog rows into **stations** (distinct
//! callsigns) so the population counts and every result variant are honest at the
//! station level (a broad query reports "1391 matched / 206 eligible", never a
//! silent row dump).

use std::collections::BTreeMap;
use std::collections::HashSet;
use std::sync::Arc;

use tuxlink_mcp_core::ports::{GatewayDto, StationModeDto};
use tuxlink_mcp_core::station_query::{
    AggregateBucket, AggregateGroup, Band, BoundedVec, Candidate, CappedString, ConnectObjective,
    ConnectionDto, ContractViolation, DistanceBucket, Facet, FacetCount, FindStationsRequest,
    FindStationsResponse, Fitness, FitnessComponents, Ft8Policy, Population, RankingMeta,
    RecommendationGoal, Refinement, SnapshotMeta, StationExportFormat, StationFacet, StationFilters,
    StationResult, StationSummary, SubsetCoverage,
};

use super::snapshot::{SnapshotError, SnapshotStore};

/// The `complete-set` upper bound — an eligible population at or below this is
/// returned whole; above it, `explore` returns `refinement-required` and
/// `recommend` returns a ranked subset.
const COMPLETE_SET_CAP: usize = 16;

/// App-owned facts the engine needs, resolved by the adapter (never agent-supplied).
pub struct StationContext {
    /// Operator's broadcast grid; distances/bearings are already stamped on the
    /// gateways by curation. Echoed as provenance.
    pub operator_grid: Option<String>,
    /// Current time (unix ms) — operating-now evaluation + snapshot minting.
    pub now_ms: u64,
    /// The freshly curated FULL population for a base (no-snapshot) query. The
    /// adapter fetches + curates this; the engine never fetches. Ignored when the
    /// request carries a `snapshot_id` (the pinned population is used instead).
    pub population: Vec<GatewayDto>,
    /// Callsigns the operator previously connected to successfully — the
    /// `prior_success` fitness input.
    pub prior_success_callsigns: HashSet<String>,
    /// Fitness inputs the engine could not evaluate this run (e.g. `path_reliability`
    /// when no propagation model ran) — surfaced honestly in `RankingMeta`.
    pub unavailable_inputs: Vec<&'static str>,
    /// Sink that writes an `export` artifact OUTSIDE the transcript. `None` makes
    /// `export` a typed error rather than a silent empty.
    pub export_sink: Option<Arc<dyn ExportSink>>,
}

/// Writes a `find_stations` export artifact to the app's export destination and
/// returns `(artifact_id, human-visible destination)`. Never returns catalog
/// rows to the caller — the artifact is user-facing, never model-readable.
pub trait ExportSink: Send + Sync {
    fn write(&self, rows: &[ExportRow], format: StationExportFormat) -> Result<ExportArtifact, String>;
}

/// One exported station row (written to the artifact, never inlined in the response).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ExportRow {
    pub callsign: String,
    pub grid: Option<String>,
    pub mode: StationModeDto,
    pub frequency_khz: f64,
    pub bandwidth_hz: Option<u32>,
    pub distance_mi: Option<f64>,
    pub bearing_deg: Option<f64>,
}

/// Where an export landed.
#[derive(Debug, Clone)]
pub struct ExportArtifact {
    pub artifact_id: String,
    pub destination: String,
}

/// Why an `evaluate` failed. All are typed + retryable/diagnosable by the agent.
#[derive(Debug)]
pub enum StationQueryError {
    /// The referenced snapshot is unknown or expired — re-issue the base query.
    Snapshot(SnapshotError),
    /// A follow-up filter would WIDEN the snapshot (non-monotonic) — snapshots
    /// only narrow.
    SnapshotWiden,
    /// An impossible internal state (should never fire) — fail loud, not silent.
    Contract(ContractViolation),
    /// Export was requested but no sink is available, or the sink failed.
    Export(String),
}

impl std::fmt::Display for StationQueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StationQueryError::Snapshot(e) => write!(f, "{e}"),
            StationQueryError::SnapshotWiden => write!(
                f,
                "filters would widen the snapshot; snapshots only narrow. Re-issue the base query to broaden."
            ),
            StationQueryError::Contract(e) => write!(f, "{e}"),
            StationQueryError::Export(m) => write!(f, "export failed: {m}"),
        }
    }
}

impl std::error::Error for StationQueryError {}

impl From<SnapshotError> for StationQueryError {
    fn from(e: SnapshotError) -> Self {
        StationQueryError::Snapshot(e)
    }
}
impl From<ContractViolation> for StationQueryError {
    fn from(e: ContractViolation) -> Self {
        StationQueryError::Contract(e)
    }
}

/// The engine. Holds a reference to the snapshot store; `evaluate` is otherwise pure.
pub struct StationQueryEngine<'a> {
    store: &'a SnapshotStore,
}

impl<'a> StationQueryEngine<'a> {
    #[must_use]
    pub fn new(store: &'a SnapshotStore) -> Self {
        Self { store }
    }

    /// Turn an intent-tagged request into a bounded response.
    pub fn evaluate(
        &self,
        req: FindStationsRequest,
        ctx: &StationContext,
    ) -> Result<FindStationsResponse, StationQueryError> {
        match req {
            FindStationsRequest::Lookup {
                snapshot_id,
                callsigns,
            } => {
                // Lookup ignores StationFilters (exact callsign match): no widen
                // check (None), empty base filter for a freshly minted snapshot.
                let (gateways, snap_meta) =
                    self.resolve_population(snapshot_id, None, &StationFilters::default(), ctx)?;
                let wanted: HashSet<String> = callsigns
                    .as_slice()
                    .iter()
                    .map(|c| c.as_str().to_ascii_uppercase())
                    .collect();
                let matched: Vec<&GatewayDto> = gateways
                    .iter()
                    .filter(|g| wanted.contains(&g.callsign.to_ascii_uppercase()))
                    .collect();
                self.finish_lookup(&matched, ctx.now_ms, snap_meta)
            }
            FindStationsRequest::Explore {
                filters,
                snapshot_id,
            } => {
                let (gateways, snap_meta) =
                    self.resolve_population(snapshot_id, Some(&filters), &filters, ctx)?;
                let filtered = apply_filters(&gateways, &filters, ctx.now_ms);
                self.finish_explore(&filtered, &filters, ctx.now_ms, snap_meta)
            }
            FindStationsRequest::Recommend {
                goal,
                filters,
                candidate_count,
                exclude_candidate_ids,
            } => {
                let (gateways, snap_meta) = self.resolve_population(None, None, &filters, ctx)?;
                let filtered = apply_filters(&gateways, &filters, ctx.now_ms);
                // Exclude by CALLSIGN parsed from each id, not the whole id: every
                // recommend call mints a fresh snapshot (different id prefix), so
                // "give me another option" must match across snapshots.
                let excluded: HashSet<String> = exclude_candidate_ids
                    .as_slice()
                    .iter()
                    .filter_map(|c| callsign_from_candidate_id(c.as_str()))
                    .map(str::to_ascii_uppercase)
                    .collect();
                self.finish_recommend(
                    &filtered,
                    goal,
                    candidate_count.get() as usize,
                    &excluded,
                    ctx,
                    snap_meta,
                )
            }
            FindStationsRequest::Aggregate { filters, group_by } => {
                let (gateways, snap_meta) = self.resolve_population(None, None, &filters, ctx)?;
                let filtered = apply_filters(&gateways, &filters, ctx.now_ms);
                self.finish_aggregate(&filtered, group_by.as_slice(), ctx.now_ms, snap_meta)
            }
            FindStationsRequest::Export { filters, format } => {
                let (gateways, snap_meta) = self.resolve_population(None, None, &filters, ctx)?;
                let filtered = apply_filters(&gateways, &filters, ctx.now_ms);
                self.finish_export(&filtered, format, ctx, snap_meta)
            }
        }
    }

    /// Load the working population (from a snapshot, or a freshly minted one over
    /// `ctx.population`) plus the response's snapshot metadata.
    ///
    /// `narrowing` is the follow-up filter to widen-check against a loaded
    /// snapshot: `Some(f)` rejects a non-monotonic `f`; `None` skips the check
    /// (used by `lookup`, which narrows by exact callsign, not by filter, and is
    /// always a valid narrowing of any snapshot). `mint_base` is the base filter
    /// recorded when a fresh snapshot is minted.
    fn resolve_population(
        &self,
        snapshot_id: Option<CappedString<32>>,
        narrowing: Option<&StationFilters>,
        mint_base: &StationFilters,
        ctx: &StationContext,
    ) -> Result<(Vec<GatewayDto>, SnapshotMeta), StationQueryError> {
        match snapshot_id {
            Some(id) => {
                let snap = self.store.get(id.as_str(), ctx.now_ms)?;
                if let Some(f) = narrowing {
                    if !f.is_narrowing_of(&snap.base_filters) {
                        return Err(StationQueryError::SnapshotWiden);
                    }
                }
                let meta = snapshot_meta(&snap.id, snap.fetched_at_ms, snap.expires_at_ms, &snap.operator_grid);
                Ok((snap.gateways, meta))
            }
            None => {
                let snap = self.store.create(
                    ctx.population.clone(),
                    ctx.operator_grid.clone(),
                    mint_base.clone(),
                    ctx.now_ms,
                );
                let meta = snapshot_meta(&snap.id, snap.fetched_at_ms, snap.expires_at_ms, &snap.operator_grid);
                Ok((snap.gateways, meta))
            }
        }
    }

    fn finish_lookup(
        &self,
        matched: &[&GatewayDto],
        now_ms: u64,
        snap_meta: SnapshotMeta,
    ) -> Result<FindStationsResponse, StationQueryError> {
        let stations = group_stations(matched, now_ms);
        let population = population_of(&stations);
        let result = if stations.is_empty() {
            StationResult::NoMatches
        } else {
            let (bv, omitted) =
                BoundedVec::<StationSummary, COMPLETE_SET_CAP>::from_capped(
                    stations.iter().map(station_summary),
                );
            // Lookup callsigns are capped at 16, so matched stations <= 16 and
            // omitted is always 0; complete_set enforces that.
            StationResult::complete_set(bv, omitted)?
        };
        Ok(FindStationsResponse::new(snap_meta, population, result))
    }

    fn finish_explore(
        &self,
        filtered: &[&GatewayDto],
        _filters: &StationFilters,
        now_ms: u64,
        snap_meta: SnapshotMeta,
    ) -> Result<FindStationsResponse, StationQueryError> {
        let stations = group_stations(filtered, now_ms);
        let population = population_of(&stations);
        let result = if stations.is_empty() {
            StationResult::NoMatches
        } else if stations.len() <= COMPLETE_SET_CAP {
            let (bv, omitted) = BoundedVec::<StationSummary, COMPLETE_SET_CAP>::from_capped(
                stations.iter().map(station_summary),
            );
            StationResult::complete_set(bv, omitted)?
        } else {
            let facets = build_facets(&stations, now_ms);
            let suggested = build_refinements(&stations);
            StationResult::RefinementRequired {
                matched_stations: stations.len() as u32,
                facets,
                suggested_refinements: suggested,
            }
        };
        Ok(FindStationsResponse::new(snap_meta, population, result))
    }

    fn finish_recommend(
        &self,
        filtered: &[&GatewayDto],
        goal: RecommendationGoal,
        candidate_count: usize,
        excluded: &HashSet<String>,
        ctx: &StationContext,
        snap_meta: SnapshotMeta,
    ) -> Result<FindStationsResponse, StationQueryError> {
        let stations = group_stations(filtered, ctx.now_ms);
        let population = population_of(&stations);
        let objective = match goal {
            RecommendationGoal::ConnectNow { objective, .. }
            | RecommendationGoal::BestAt { objective, .. } => objective,
        };
        let at_ms = match goal {
            RecommendationGoal::ConnectNow { at_utc_ms, .. } => at_utc_ms.unwrap_or(ctx.now_ms),
            RecommendationGoal::BestAt { at_utc_ms, .. } => at_utc_ms,
        };

        // Score every eligible station (evaluated == eligible, always — the
        // population is in memory so we never return an approximate best).
        let snap_id = snap_meta.id.as_str().to_string();
        let mut scored: Vec<(f32, Candidate)> = stations
            .iter()
            .filter(|s| !excluded.contains(&s.callsign.to_ascii_uppercase()))
            .filter_map(|s| score_station(s, objective, at_ms, ctx, &snap_id))
            .collect();

        // Descending by score; stable so equal scores keep group (nearest-first) order.
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        let evaluated = population.eligible_stations;
        let result = if scored.is_empty() {
            StationResult::NoMatches
        } else {
            let returned = scored.len().min(candidate_count);
            let (top, omitted) = BoundedVec::<Candidate, 8>::from_capped(
                scored.into_iter().take(returned).map(|(_, c)| c),
            );
            debug_assert_eq!(omitted, 0, "take(returned<=8) can't exceed the cap");
            let returned_u32 = top.len() as u32;
            let coverage = SubsetCoverage::top_of_all_eligible(
                evaluated,
                returned_u32,
                evaluated.saturating_sub(returned_u32),
            );
            StationResult::RankedSubset {
                ranking: ranking_meta(objective, ctx),
                coverage,
                top_candidates: top,
            }
        };
        Ok(FindStationsResponse::new(snap_meta, population, result))
    }

    fn finish_aggregate(
        &self,
        filtered: &[&GatewayDto],
        group_by: &[StationFacet],
        now_ms: u64,
        snap_meta: SnapshotMeta,
    ) -> Result<FindStationsResponse, StationQueryError> {
        let stations = group_stations(filtered, now_ms);
        let population = population_of(&stations);
        let (groups, _) = BoundedVec::<AggregateGroup, 3>::from_capped(
            group_by
                .iter()
                .map(|facet| aggregate_group(&stations, *facet, now_ms)),
        );
        let result = StationResult::AggregateComplete { groups };
        Ok(FindStationsResponse::new(snap_meta, population, result))
    }

    fn finish_export(
        &self,
        filtered: &[&GatewayDto],
        format: StationExportFormat,
        ctx: &StationContext,
        snap_meta: SnapshotMeta,
    ) -> Result<FindStationsResponse, StationQueryError> {
        let stations = group_stations(filtered, ctx.now_ms);
        let population = population_of(&stations);
        let sink = ctx
            .export_sink
            .as_ref()
            .ok_or_else(|| StationQueryError::Export("no export sink configured".into()))?;
        let rows: Vec<ExportRow> = stations.iter().flat_map(export_rows).collect();
        let total_rows = rows.len() as u32;
        let artifact = sink
            .write(&rows, format)
            .map_err(StationQueryError::Export)?;
        let result = StationResult::ExportReady {
            artifact_id: CappedString::from_truncated(&artifact.artifact_id),
            format,
            total_rows,
            destination: CappedString::from_truncated(&artifact.destination),
        };
        Ok(FindStationsResponse::new(snap_meta, population, result))
    }
}

// ---------------------------------------------------------------------------
// Station grouping
// ---------------------------------------------------------------------------

/// A station (distinct callsign) with its connection options, grouped from the
/// per-`(callsign, mode, channel)` catalog rows.
struct Station<'a> {
    callsign: &'a str,
    grid: Option<&'a str>,
    distance_mi: Option<f64>,
    bearing_deg: Option<f64>,
    ft8_corroborated: Option<bool>,
    /// True when ANY of the station's rows advertises operating now (per its
    /// `HH-HH` hours vs the injected time; a row with no hours is always-on).
    operating_now: bool,
    connections: Vec<Conn>,
}

#[derive(Clone)]
struct Conn {
    mode: StationModeDto,
    frequency_khz: f64,
    bandwidth_hz: Option<u32>,
}

/// Group filtered gateway rows into stations, preserving first-seen order (which,
/// for the fresh-query path, is nearest-first because curation feeds the sorted
/// population... but the engine never relies on incoming order for correctness).
fn group_stations<'a>(gateways: &[&'a GatewayDto], now_ms: u64) -> Vec<Station<'a>> {
    let mut order: Vec<String> = Vec::new();
    let mut by_call: BTreeMap<String, Station<'a>> = BTreeMap::new();
    for gw in gateways {
        let key = gw.callsign.to_ascii_uppercase();
        let row_operating = is_operating_now(gw, now_ms);
        let entry = by_call.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            Station {
                callsign: gw.callsign.as_str(),
                grid: gw.grid.as_deref(),
                distance_mi: gw.distance_mi,
                bearing_deg: gw.bearing_deg,
                ft8_corroborated: gw.ft8_corroborated,
                operating_now: false,
                connections: Vec::new(),
            }
        });
        // Fill in missing location/evidence from any row that has it.
        if entry.grid.is_none() {
            entry.grid = gw.grid.as_deref();
        }
        if entry.distance_mi.is_none() {
            entry.distance_mi = gw.distance_mi;
        }
        if entry.bearing_deg.is_none() {
            entry.bearing_deg = gw.bearing_deg;
        }
        if gw.ft8_corroborated == Some(true) {
            entry.ft8_corroborated = Some(true);
        }
        entry.operating_now = entry.operating_now || row_operating;
        entry.connections.extend(connections_of(gw));
    }
    // Return in first-seen order.
    order
        .into_iter()
        .filter_map(|k| by_call.remove(&k))
        .filter(|s| !s.connections.is_empty())
        .collect()
}

/// Extract a row's connection options: one per channel-detail row, or one per
/// bare dial when the gateway has no channel details.
fn connections_of(gw: &GatewayDto) -> Vec<Conn> {
    if gw.channels.is_empty() {
        gw.frequencies_khz
            .iter()
            .map(|&f| Conn {
                mode: gw.mode,
                frequency_khz: f,
                bandwidth_hz: None,
            })
            .collect()
    } else {
        gw.channels
            .iter()
            .map(|c| Conn {
                mode: gw.mode,
                frequency_khz: c.frequency_khz,
                bandwidth_hz: c.bandwidth_hz,
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Filtering
// ---------------------------------------------------------------------------

/// Post-filter the population by every axis the engine owns. `history_hours` is
/// NOT applied here — it is a fetch-time parameter (the catalog fetch bounds
/// last-heard), and `GatewayDto` carries no timestamp to re-filter on.
fn apply_filters<'a>(
    gateways: &'a [GatewayDto],
    f: &StationFilters,
    now_ms: u64,
) -> Vec<&'a GatewayDto> {
    gateways.iter().filter(|gw| passes_filters(gw, f, now_ms)).collect()
}

fn passes_filters(gw: &GatewayDto, f: &StationFilters, now_ms: u64) -> bool {
    if !f.modes.is_empty() && !f.modes.as_slice().contains(&gw.mode) {
        return false;
    }
    if !f.bands.is_empty() || !f.bandwidths.is_empty() {
        let bands: Vec<String> = f.bands.as_slice().iter().map(|b| b.label().to_string()).collect();
        let bws: Vec<u32> = f.bandwidths.as_slice().iter().map(|b| b.hz()).collect();
        if !crate::mcp_ports::gateway_dto_passes_band_and_bandwidth(gw, &bands, &bws) {
            return false;
        }
    }
    if let Some(d) = f.distance {
        match gw.distance_mi {
            Some(mi) if d.contains_mi(mi) => {}
            _ => return false,
        }
    }
    if let Some(sector) = f.bearing {
        match gw.bearing_deg {
            Some(deg) if sector.contains_deg(deg) => {}
            _ => return false,
        }
    }
    if let Some(want) = f.operating_now {
        if is_operating_now(gw, now_ms) != want {
            return false;
        }
    }
    if let Some(prefix) = &f.callsign_prefix {
        if !gw
            .callsign
            .to_ascii_uppercase()
            .starts_with(&prefix.as_str().to_ascii_uppercase())
        {
            return false;
        }
    }
    if f.ft8_policy == Ft8Policy::Require && gw.ft8_corroborated != Some(true) {
        return false;
    }
    true
}

/// True when a gateway advertises operating now (per any channel's `HH-HH`
/// hours vs the current UTC hour). A gateway with NO hours info is treated as
/// always operating (many 24/7 gateways omit hours).
fn is_operating_now(gw: &GatewayDto, now_ms: u64) -> bool {
    let hour = ((now_ms / 3_600_000) % 24) as u32;
    let mut had_hours = false;
    for c in &gw.channels {
        if let Some(hours) = &c.operating_hours {
            if let Some((lo, hi)) = parse_hours(hours) {
                had_hours = true;
                let inside = if lo <= hi {
                    hour >= lo && hour <= hi
                } else {
                    hour >= lo || hour <= hi // wraps midnight
                };
                if inside {
                    return true;
                }
            }
        }
    }
    // No parseable hours anywhere → assume always-on.
    !had_hours
}

/// Parse a `"HH-HH"` operating-hours string into `(lo, hi)` hours in `0..=23`.
fn parse_hours(s: &str) -> Option<(u32, u32)> {
    let (lo, hi) = s.trim().split_once('-')?;
    let lo: u32 = lo.trim().parse().ok()?;
    let hi: u32 = hi.trim().parse().ok()?;
    if lo <= 23 && hi <= 23 {
        Some((lo, hi))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Population + summaries
// ---------------------------------------------------------------------------

fn population_of(stations: &[Station<'_>]) -> Population {
    let matched = stations.len() as u32;
    // Every grouped station has >=1 connection (dial-less rows were dropped), so
    // eligible == matched here; connection options sum the per-station channels.
    let eligible = matched;
    let conn_options: u32 = stations
        .iter()
        .map(|s| s.connections.len() as u32)
        .sum();
    Population::new(matched, eligible, conn_options)
}

fn station_summary(s: &Station<'_>) -> StationSummary {
    let (freqs, _) = BoundedVec::<f64, 8>::from_capped(s.connections.iter().map(|c| c.frequency_khz));
    StationSummary {
        callsign: CappedString::from_truncated(s.callsign),
        grid: s.grid.map(CappedString::from_truncated),
        mode: s.connections.first().map_or(StationModeDto::VaraHf, |c| c.mode),
        frequencies_khz: freqs,
        distance_mi: s.distance_mi,
        bearing_deg: s.bearing_deg,
        operating_now: Some(s.operating_now),
    }
}

fn export_rows(s: &Station<'_>) -> Vec<ExportRow> {
    s.connections
        .iter()
        .map(|c| ExportRow {
            callsign: s.callsign.to_string(),
            grid: s.grid.map(str::to_string),
            mode: c.mode,
            frequency_khz: c.frequency_khz,
            bandwidth_hz: c.bandwidth_hz,
            distance_mi: s.distance_mi,
            bearing_deg: s.bearing_deg,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Ranking (recommend)
// ---------------------------------------------------------------------------

/// Mode preference for the single `selected_connection` (more capable first).
fn mode_rank(mode: StationModeDto) -> u8 {
    match mode {
        StationModeDto::VaraHf => 0,
        StationModeDto::Pactor => 1,
        StationModeDto::ArdopHf => 2,
        StationModeDto::VaraFm => 3,
        StationModeDto::RobustPacket => 4,
        StationModeDto::Packet => 5,
    }
}

/// Pick the station's single recommended connection: most-capable mode, then
/// lowest dial (a stable, explainable choice).
fn select_connection<'s>(s: &'s Station<'_>) -> Option<&'s Conn> {
    s.connections.iter().min_by(|a, b| {
        mode_rank(a.mode)
            .cmp(&mode_rank(b.mode))
            .then(a.frequency_khz.partial_cmp(&b.frequency_khz).unwrap_or(std::cmp::Ordering::Equal))
    })
}

fn candidate_id(snapshot_id: &str, callsign: &str, grid: Option<&str>) -> String {
    format!("{snapshot_id}/{callsign}/{}", grid.unwrap_or("----"))
}

/// Parse the callsign segment out of a `"snapshot/callsign/grid"` candidate id.
/// Used for snapshot-independent exclusion (`recommend`'s "give me another").
fn callsign_from_candidate_id(id: &str) -> Option<&str> {
    let mut parts = id.rsplit('/');
    let _grid = parts.next()?;
    parts.next()
}

/// Score a station under the objective, producing `(score, Candidate)`. Returns
/// `None` only if the station has no selectable connection (already filtered out).
fn score_station(
    s: &Station<'_>,
    objective: ConnectObjective,
    _at_ms: u64,
    ctx: &StationContext,
    snapshot_id: &str,
) -> Option<(f32, Candidate)> {
    let conn = select_connection(s)?;
    let prior_success = ctx
        .prior_success_callsigns
        .contains(&s.callsign.to_ascii_uppercase());
    let operating = Some(s.operating_now);

    let (score, components, reason_codes) = match objective {
        ConnectObjective::Nearest => {
            // Pure distance: closer = higher, unknown distance = worst.
            let score = s
                .distance_mi
                .map_or(0.0_f32, |mi| 1.0 / (1.0 + (mi as f32) / 100.0));
            let reasons = vec!["NEAREST"];
            (
                score,
                FitnessComponents {
                    path_reliability: None,
                    ft8_corroborated: None,
                    operating_now: None,
                    prior_success: None,
                },
                reasons,
            )
        }
        ConnectObjective::EstimatedSuccess => {
            // Weighted sum of the available signals (path_reliability is
            // unavailable in v1 — surfaced in inputs_unavailable, not faked).
            // Max = 0.35 + 0.20 + 0.25 + 0.20 = 1.0.
            let mut score = 0.0_f32;
            let mut reasons: Vec<&'static str> = Vec::new();
            if s.ft8_corroborated == Some(true) {
                score += 0.35;
                reasons.push("FT8_CORROBORATED");
            }
            if s.operating_now {
                score += 0.20;
                reasons.push("OPERATING_NOW");
            }
            if prior_success {
                score += 0.25;
                reasons.push("PRIOR_SUCCESS");
            }
            if let Some(mi) = s.distance_mi {
                let near = 0.20 * (1.0 / (1.0 + (mi as f32) / 500.0));
                score += near;
                if mi <= 300.0 {
                    reasons.push("NEAR");
                }
            }
            (
                score.min(1.0),
                FitnessComponents {
                    path_reliability: None, // unavailable in v1 (see inputs_unavailable)
                    ft8_corroborated: s.ft8_corroborated,
                    operating_now: operating,
                    prior_success: Some(prior_success),
                },
                reasons,
            )
        }
    };

    let (reason_codes_bv, _) = BoundedVec::<CappedString<24>, 6>::from_capped(
        reason_codes.into_iter().map(CappedString::from_truncated),
    );
    let candidate = Candidate {
        candidate_id: CappedString::from_truncated(&candidate_id(snapshot_id, s.callsign, s.grid)),
        callsign: CappedString::from_truncated(s.callsign),
        grid: s.grid.map(CappedString::from_truncated),
        selected_connection: ConnectionDto {
            target_callsign: CappedString::from_truncated(s.callsign),
            mode: conn.mode,
            frequency_khz: conn.frequency_khz,
            bandwidth_hz: conn.bandwidth_hz,
        },
        alternate_connection_count: (s.connections.len().saturating_sub(1)) as u32,
        fitness: Fitness {
            score,
            components,
            reason_codes: reason_codes_bv,
        },
    };
    Some((score, candidate))
}

fn ranking_meta(objective: ConnectObjective, ctx: &StationContext) -> RankingMeta {
    let (policy, used): (&'static str, Vec<&'static str>) = match objective {
        ConnectObjective::Nearest => ("nearest-v1", vec!["distance"]),
        ConnectObjective::EstimatedSuccess => (
            "connect-now-v1",
            vec!["ft8_corroborated", "prior_success", "distance"],
        ),
    };
    let (used_bv, _) =
        BoundedVec::<CappedString<32>, 8>::from_capped(used.into_iter().map(CappedString::from_truncated));
    let (unavail_bv, _) = BoundedVec::<CappedString<32>, 8>::from_capped(
        ctx.unavailable_inputs
            .iter()
            .copied()
            .map(CappedString::from_truncated),
    );
    RankingMeta {
        policy,
        inputs_used: used_bv,
        inputs_unavailable: unavail_bv,
    }
}

// ---------------------------------------------------------------------------
// Faceting (explore refinement + aggregate)
// ---------------------------------------------------------------------------

/// The distinct band labels a station operates on (via `khz_to_band`).
fn station_bands(s: &Station<'_>) -> Vec<&'static str> {
    let mut bands: Vec<&'static str> = s
        .connections
        .iter()
        .filter_map(|c| crate::mcp_ports::khz_to_band(c.frequency_khz))
        .collect();
    bands.sort_unstable();
    bands.dedup();
    bands
}

fn station_modes(s: &Station<'_>) -> Vec<StationModeDto> {
    let mut modes: Vec<StationModeDto> = s.connections.iter().map(|c| c.mode).collect();
    modes.sort_unstable_by_key(|m| mode_rank(*m));
    modes.dedup();
    modes
}

fn mode_token(m: StationModeDto) -> &'static str {
    match m {
        StationModeDto::VaraHf => "vara-hf",
        StationModeDto::VaraFm => "vara-fm",
        StationModeDto::Packet => "packet",
        StationModeDto::ArdopHf => "ardop-hf",
        StationModeDto::Pactor => "pactor",
        StationModeDto::RobustPacket => "robust-packet",
    }
}

/// Count stations per band (a station counts toward each band it operates on).
fn count_bands(stations: &[Station<'_>]) -> Vec<(String, u32)> {
    let mut counts: BTreeMap<&'static str, u32> = BTreeMap::new();
    for s in stations {
        for b in station_bands(s) {
            *counts.entry(b).or_insert(0) += 1;
        }
    }
    sorted_desc(counts.into_iter().map(|(k, v)| (k.to_string(), v)))
}

fn count_modes(stations: &[Station<'_>]) -> Vec<(String, u32)> {
    let mut counts: BTreeMap<&'static str, u32> = BTreeMap::new();
    for s in stations {
        for m in station_modes(s) {
            *counts.entry(mode_token(m)).or_insert(0) += 1;
        }
    }
    sorted_desc(counts.into_iter().map(|(k, v)| (k.to_string(), v)))
}

/// Cumulative "within X" distance counts (a station with distance d counts in
/// every bucket whose ceiling is >= d) — the honest `remaining_if_applied` for a
/// distance ceiling.
fn count_distance_cumulative(stations: &[Station<'_>]) -> Vec<(String, u32, DistanceBucket)> {
    const BUCKETS: [DistanceBucket; 5] = [
        DistanceBucket::Within100mi,
        DistanceBucket::Within300mi,
        DistanceBucket::Within600mi,
        DistanceBucket::Within1200mi,
        DistanceBucket::Within2500mi,
    ];
    BUCKETS
        .iter()
        .map(|b| {
            let n = stations
                .iter()
                .filter(|s| s.distance_mi.is_some_and(|mi| b.contains_mi(mi)))
                .count() as u32;
            (distance_token(*b), n, *b)
        })
        .filter(|(_, n, _)| *n > 0)
        .collect()
}

fn distance_token(b: DistanceBucket) -> String {
    match b {
        DistanceBucket::Within100mi => "within-100mi",
        DistanceBucket::Within300mi => "within-300mi",
        DistanceBucket::Within600mi => "within-600mi",
        DistanceBucket::Within1200mi => "within-1200mi",
        DistanceBucket::Within2500mi => "within-2500mi",
        DistanceBucket::Beyond2500mi => "beyond-2500mi",
    }
    .to_string()
}

fn sorted_desc(iter: impl Iterator<Item = (String, u32)>) -> Vec<(String, u32)> {
    let mut v: Vec<(String, u32)> = iter.collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v
}

/// Build the `refinement-required` facets: band, mode, and distance breakdowns.
fn build_facets(stations: &[Station<'_>], _now_ms: u64) -> BoundedVec<Facet, 8> {
    let mut facets: Vec<Facet> = Vec::new();
    let band_counts = count_bands(stations);
    if !band_counts.is_empty() {
        facets.push(facet_from(StationFacet::Band, band_counts));
    }
    let mode_counts = count_modes(stations);
    if !mode_counts.is_empty() {
        facets.push(facet_from(StationFacet::Mode, mode_counts));
    }
    let dist = count_distance_cumulative(stations);
    if !dist.is_empty() {
        facets.push(facet_from(
            StationFacet::DistanceBucket,
            dist.into_iter().map(|(t, n, _)| (t, n)).collect(),
        ));
    }
    BoundedVec::from_capped(facets).0
}

fn facet_from(field: StationFacet, counts: Vec<(String, u32)>) -> Facet {
    let (values, _) = BoundedVec::<FacetCount, 24>::from_capped(counts.into_iter().map(|(v, n)| {
        FacetCount {
            value: CappedString::from_truncated(&v),
            remaining_if_applied: n,
        }
    }));
    Facet { field, values }
}

/// Build bounded, labelled additive-filter suggestions (top bands, then tightest
/// productive distance ceilings). Each carries the exact resulting count.
fn build_refinements(stations: &[Station<'_>]) -> BoundedVec<Refinement, 12> {
    let mut out: Vec<Refinement> = Vec::new();

    // Top bands by population.
    for (band_label, count) in count_bands(stations).into_iter().take(6) {
        if let Some(band) = band_from_label(&band_label) {
            out.push(Refinement {
                label: CappedString::from_truncated(&format!("Only {band_label} ({count})")),
                add_filters: StationFilters {
                    bands: BoundedVec::from_capped(vec![band]).0,
                    ..Default::default()
                },
                remaining: count,
            });
        }
    }

    // A couple of distance ceilings (tightest productive first).
    for (token, count, bucket) in count_distance_cumulative(stations).into_iter().take(3) {
        out.push(Refinement {
            label: CappedString::from_truncated(&format!("Within {token} ({count})")),
            add_filters: StationFilters {
                distance: Some(bucket),
                ..Default::default()
            },
            remaining: count,
        });
    }

    BoundedVec::from_capped(out).0
}

fn band_from_label(label: &str) -> Option<Band> {
    match label {
        "160m" => Some(Band::B160m),
        "80m" => Some(Band::B80m),
        "60m" => Some(Band::B60m),
        "40m" => Some(Band::B40m),
        "30m" => Some(Band::B30m),
        "20m" => Some(Band::B20m),
        "17m" => Some(Band::B17m),
        "15m" => Some(Band::B15m),
        "12m" => Some(Band::B12m),
        "10m" => Some(Band::B10m),
        _ => None,
    }
}

/// One `aggregate` group: exact per-value counts over the matched population.
fn aggregate_group(stations: &[Station<'_>], facet: StationFacet, now_ms: u64) -> AggregateGroup {
    let counts: Vec<(String, u32)> = match facet {
        StationFacet::Band => count_bands(stations),
        StationFacet::Mode => count_modes(stations),
        StationFacet::DistanceBucket => count_distance_partition(stations),
        StationFacet::BearingSector => count_bearing(stations),
        StationFacet::BandwidthClass => count_bandwidths(stations),
        StationFacet::OperatingNow => count_operating(stations, now_ms),
    };
    let (buckets, _) = BoundedVec::<AggregateBucket, 24>::from_capped(counts.into_iter().map(|(v, n)| {
        AggregateBucket {
            value: CappedString::from_truncated(&v),
            count: n,
        }
    }));
    AggregateGroup { facet, buckets }
}

/// Distance as a mutually-exclusive histogram: each station lands in its tightest
/// bucket (so the counts partition the population).
fn count_distance_partition(stations: &[Station<'_>]) -> Vec<(String, u32)> {
    const ORDER: [DistanceBucket; 6] = [
        DistanceBucket::Within100mi,
        DistanceBucket::Within300mi,
        DistanceBucket::Within600mi,
        DistanceBucket::Within1200mi,
        DistanceBucket::Within2500mi,
        DistanceBucket::Beyond2500mi,
    ];
    let mut counts: BTreeMap<usize, u32> = BTreeMap::new();
    for s in stations {
        let Some(mi) = s.distance_mi else { continue };
        if let Some(idx) = ORDER.iter().position(|b| b.contains_mi(mi)) {
            *counts.entry(idx).or_insert(0) += 1;
        }
    }
    let mut out: Vec<(String, u32)> = counts
        .into_iter()
        .map(|(idx, n)| (distance_token(ORDER[idx]), n))
        .collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    out
}

fn count_bearing(stations: &[Station<'_>]) -> Vec<(String, u32)> {
    use tuxlink_mcp_core::station_query::BearingSector;
    let mut counts: BTreeMap<&'static str, u32> = BTreeMap::new();
    for s in stations {
        if let Some(deg) = s.bearing_deg {
            let token = bearing_token(BearingSector::from_bearing(deg));
            *counts.entry(token).or_insert(0) += 1;
        }
    }
    sorted_desc(counts.into_iter().map(|(k, v)| (k.to_string(), v)))
}

fn bearing_token(s: tuxlink_mcp_core::station_query::BearingSector) -> &'static str {
    use tuxlink_mcp_core::station_query::BearingSector as B;
    match s {
        B::N => "n",
        B::Ne => "ne",
        B::E => "e",
        B::Se => "se",
        B::S => "s",
        B::Sw => "sw",
        B::W => "w",
        B::Nw => "nw",
    }
}

fn count_bandwidths(stations: &[Station<'_>]) -> Vec<(String, u32)> {
    let mut counts: BTreeMap<u32, u32> = BTreeMap::new();
    for s in stations {
        let mut seen: HashSet<u32> = HashSet::new();
        for c in &s.connections {
            if let Some(bw) = c.bandwidth_hz {
                if [500, 1000, 2000, 2300, 2750].contains(&bw) && seen.insert(bw) {
                    *counts.entry(bw).or_insert(0) += 1;
                }
            }
        }
    }
    sorted_desc(counts.into_iter().map(|(k, v)| (k.to_string(), v)))
}

fn count_operating(stations: &[Station<'_>], _now_ms: u64) -> Vec<(String, u32)> {
    // `operating_now` was computed against the injected time at grouping.
    let yes = stations.iter().filter(|s| s.operating_now).count() as u32;
    let no = stations.len() as u32 - yes;
    let mut out = Vec::new();
    if yes > 0 {
        out.push(("yes".to_string(), yes));
    }
    if no > 0 {
        out.push(("no".to_string(), no));
    }
    out
}

// ---------------------------------------------------------------------------
// Snapshot meta
// ---------------------------------------------------------------------------

fn snapshot_meta(
    id: &CappedString<32>,
    fetched_at_ms: u64,
    expires_at_ms: u64,
    operator_grid: &Option<String>,
) -> SnapshotMeta {
    SnapshotMeta {
        id: id.clone(),
        fetched_at_ms,
        operator_grid: operator_grid.as_deref().map(CappedString::from_truncated),
        expires_at_ms,
    }
}

#[cfg(test)]
mod tests;
