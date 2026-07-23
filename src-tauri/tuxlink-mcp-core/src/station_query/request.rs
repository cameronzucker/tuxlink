//! The intent-tagged `find_stations` request (spec §Request).
//!
//! The agent supplies **only semantic intent + user constraints**. It never
//! supplies app-owned facts (operator grid, current time, configured transports,
//! operating hours, propagation inputs, FT-8 evidence, connection history) — the
//! engine injects those from live app state ([`crate::station_query`] module
//! docs). So the agent never needs the raw catalog to form a goal, and the tool
//! is a single intent-tagged entry point (operator decision: one tool so a weak
//! local model cannot pick the wrong one).
//!
//! Every collection is a [`BoundedVec`]; every scalar count is a [`BoundedU8`];
//! every free-ish string is a [`CappedString`]. Bounded *input* is the mirror of
//! the bounded *output*: an out-of-set band / an over-long candidate list fails
//! deserialization with a message the model can correct from, rather than
//! silently matching nothing.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::bounded::{BoundedU8, BoundedVec, CappedString};

/// Deserialize a field that a weak model may send either natively OR as a JSON
/// **string** encoding the value. Small/quantized models (observed on
/// qwen-3.5-122b, 2026-07-23) stringify typed and nested tool arguments —
/// `candidate_count: "5"`, `filters: "{…}"`, `goal: "{…}"` — which a strict
/// schema rejects (`invalid type: string "5", expected u8`), leaving the agent
/// looping on deserialize errors. This absorbs that quirk at the boundary (a
/// bounded tool the model cannot format is still unusable): if the value is a
/// string, parse it as JSON into `T`; otherwise deserialize `T` directly. The
/// bounds still apply — a stringified over-cap value is still rejected.
fn de_stringy_or_native<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::String(s) => {
            serde_json::from_str(&s).map_err(serde::de::Error::custom)
        }
        other => serde_json::from_value(other).map_err(serde::de::Error::custom),
    }
}

/// An app-minted snapshot handle. `explore`/`lookup` narrow *against* it so
/// population counts stay stable between calls. App-generated, so already within
/// bound; the cap only guards a malformed echo.
pub type SnapshotId = CappedString<32>;
/// An amateur callsign for `lookup`. Cap covers the longest portable form.
pub type Callsign = CappedString<16>;
/// A candidate identity (`sq_…/W1ABC/FN31`) for `recommend`'s `exclude` list.
pub type CandidateId = CappedString<96>;

// ---------------------------------------------------------------------------
// Bounded constraint enums (spec §Request: "bounded enums where possible")
// ---------------------------------------------------------------------------

/// An amateur HF band the directory classifies. Variants mirror the `BANDS`
/// table in `mcp_ports.rs` exactly so the engine can map a `Band` back onto the
/// existing string band filter (`any_freq_in_bands`). Kebab/label tokens on the
/// wire (`"40m"`, `"160m"`), so the advertised schema lists exactly the legal set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum Band {
    #[serde(rename = "160m")]
    B160m,
    #[serde(rename = "80m")]
    B80m,
    #[serde(rename = "60m")]
    B60m,
    #[serde(rename = "40m")]
    B40m,
    #[serde(rename = "30m")]
    B30m,
    #[serde(rename = "20m")]
    B20m,
    #[serde(rename = "17m")]
    B17m,
    #[serde(rename = "15m")]
    B15m,
    #[serde(rename = "12m")]
    B12m,
    #[serde(rename = "10m")]
    B10m,
}

impl Band {
    /// The `BANDS`-table label this band maps to (what the engine feeds the
    /// existing string band filter).
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Band::B160m => "160m",
            Band::B80m => "80m",
            Band::B60m => "60m",
            Band::B40m => "40m",
            Band::B30m => "30m",
            Band::B20m => "20m",
            Band::B17m => "17m",
            Band::B15m => "15m",
            Band::B12m => "12m",
            Band::B10m => "10m",
        }
    }
}

/// An occupied-bandwidth class in Hz. Mirrors the classes the existing
/// `bandwidth_class` / `channel_passes_bandwidth` filter recognizes (VARA
/// 500/2300/2750, ARDOP 500/1000/2000). Serialized as the numeric token so the
/// wire reads `["500","2750"]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum BandwidthClass {
    #[serde(rename = "500")]
    Hz500,
    #[serde(rename = "1000")]
    Hz1000,
    #[serde(rename = "2000")]
    Hz2000,
    #[serde(rename = "2300")]
    Hz2300,
    #[serde(rename = "2750")]
    Hz2750,
}

impl BandwidthClass {
    #[must_use]
    pub fn hz(self) -> u32 {
        match self {
            BandwidthClass::Hz500 => 500,
            BandwidthClass::Hz1000 => 1000,
            BandwidthClass::Hz2000 => 2000,
            BandwidthClass::Hz2300 => 2300,
            BandwidthClass::Hz2750 => 2750,
        }
    }
}

/// How to treat FT-8 corroboration when ranking/filtering. Replaces the old
/// `ft8_evidence: Option<bool>`: `require` keeps only corroborated gateways,
/// `prefer` corroborates + ranks up but does not exclude, `ignore` (default)
/// serves gateways without evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Ft8Policy {
    #[default]
    Ignore,
    Prefer,
    Require,
}

/// A coarse distance bucket in statute miles (imperial-default audience). The
/// engine filters `distance_mi` against [`DistanceBucket::upper_mi`]; `Beyond*`
/// has no upper bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum DistanceBucket {
    Within100mi,
    Within300mi,
    Within600mi,
    Within1200mi,
    Within2500mi,
    Beyond2500mi,
}

impl DistanceBucket {
    /// Inclusive upper bound in miles, or `None` for the open `Beyond*` bucket.
    #[must_use]
    pub fn upper_mi(self) -> Option<f64> {
        match self {
            DistanceBucket::Within100mi => Some(100.0),
            DistanceBucket::Within300mi => Some(300.0),
            DistanceBucket::Within600mi => Some(600.0),
            DistanceBucket::Within1200mi => Some(1200.0),
            DistanceBucket::Within2500mi => Some(2500.0),
            DistanceBucket::Beyond2500mi => None,
        }
    }

    /// True when `distance_mi` falls in this bucket. `Beyond2500mi` is everything
    /// past 2500; the bounded buckets are `(prev_upper, upper]`, so the engine
    /// treats a filter as "at most this bucket's upper" — a single upper-bound
    /// predicate, which is what the `refinement-required` suggestions produce.
    #[must_use]
    pub fn contains_mi(self, distance_mi: f64) -> bool {
        match self.upper_mi() {
            Some(upper) => distance_mi <= upper,
            None => distance_mi > 2500.0,
        }
    }
}

/// An 8-point compass bearing sector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum BearingSector {
    N,
    Ne,
    E,
    Se,
    S,
    Sw,
    W,
    Nw,
}

impl BearingSector {
    /// The sector an initial bearing in degrees `[0, 360)` falls into.
    #[must_use]
    pub fn from_bearing(deg: f64) -> BearingSector {
        // Normalize into [0, 360), rotate by half a sector so N straddles 0.
        let norm = deg.rem_euclid(360.0);
        let idx = (((norm + 22.5) % 360.0) / 45.0) as usize % 8;
        [
            BearingSector::N,
            BearingSector::Ne,
            BearingSector::E,
            BearingSector::Se,
            BearingSector::S,
            BearingSector::Sw,
            BearingSector::W,
            BearingSector::Nw,
        ][idx]
    }

    /// True when `deg` falls in this sector.
    #[must_use]
    pub fn contains_deg(self, deg: f64) -> bool {
        Self::from_bearing(deg) == self
    }
}

/// A property a population can be grouped/faceted by (`aggregate.group_by`,
/// `explore` facets).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum StationFacet {
    Mode,
    Band,
    BandwidthClass,
    DistanceBucket,
    BearingSector,
    OperatingNow,
}

/// Export artifact format for the `export` intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum StationExportFormat {
    Json,
    Csv,
}

/// What a `recommend` call optimizes for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ConnectObjective {
    /// Estimated connect success (path reliability + FT-8 + operating-now +
    /// history), ranked by the versioned `connect-now-v1` policy.
    EstimatedSuccess,
    /// Pure great-circle distance, honestly named `nearest-v1` — never presented
    /// as connection fitness.
    Nearest,
}

// ---------------------------------------------------------------------------
// Filters
// ---------------------------------------------------------------------------

/// Agent-supplied narrowing constraints. All optional; omission means "no
/// constraint" on that axis. Breadth is handled by the response contract
/// (`refinement-required`), never by silently capping the result.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StationFilters {
    /// Restrict to these transports; empty = all.
    #[serde(default)]
    pub modes: BoundedVec<crate::ports::StationModeDto, 6>,
    /// Restrict to these HF bands; empty = all.
    #[serde(default)]
    pub bands: BoundedVec<Band, 16>,
    /// Restrict to these occupied-bandwidth classes; empty = all.
    #[serde(default)]
    pub bandwidths: BoundedVec<BandwidthClass, 5>,
    /// FT-8 corroboration policy; default `ignore`.
    #[serde(default)]
    pub ft8_policy: Ft8Policy,
    /// When `Some(true)`, keep only gateways operating now (per advertised
    /// operating hours vs the injected current time); `Some(false)` keep only
    /// off-hours; `None` no constraint.
    #[serde(default)]
    pub operating_now: Option<bool>,
    /// Keep only gateways at most this far away.
    #[serde(default)]
    pub distance: Option<DistanceBucket>,
    /// Keep only gateways in this bearing sector from the operator.
    #[serde(default)]
    pub bearing: Option<BearingSector>,
    /// Keep only gateways whose callsign starts with this prefix (case-folded).
    #[serde(default)]
    pub callsign_prefix: Option<CappedString<12>>,
    /// Only gateways heard within this many hours; `None` = no bound.
    #[serde(default)]
    pub history_hours: Option<u32>,
}

impl Ft8Policy {
    /// Ordinal restrictiveness (`ignore` < `prefer` < `require`) for the
    /// snapshot-narrowing check — a stricter FT-8 policy narrows the population.
    fn restrictiveness(self) -> u8 {
        match self {
            Ft8Policy::Ignore => 0,
            Ft8Policy::Prefer => 1,
            Ft8Policy::Require => 2,
        }
    }
}

impl StationFilters {
    /// True when `self` is at least as restrictive as `base` on **every** axis —
    /// i.e. applying `self` to any population can only ever return a SUBSET of
    /// what `base` returns. Snapshots may only narrow: the engine rejects a
    /// follow-up filter for which this is `false` (spec §Error handling —
    /// "Filters that would widen a snapshot are rejected"). Reflexive: a filter
    /// always narrows itself.
    #[must_use]
    pub fn is_narrowing_of(&self, base: &StationFilters) -> bool {
        // A set constraint narrows iff base is unconstrained (empty = all) OR
        // self restricts to a subset of base's allowed set.
        fn set_narrows<T: PartialEq>(this: &[T], base: &[T]) -> bool {
            base.is_empty() || (!this.is_empty() && this.iter().all(|x| base.contains(x)))
        }
        // An equality constraint (operating_now, bearing) narrows iff base is
        // unset or self pins the same value.
        fn eq_narrows<T: PartialEq>(this: Option<T>, base: Option<T>) -> bool {
            match base {
                None => true,
                Some(b) => this == Some(b),
            }
        }
        // Distance narrows iff self's inclusive upper bound is <= base's (an
        // unset/`Beyond*` upper is +inf).
        // Map each bucket to its inclusive upper bound in miles; an unset filter
        // or an open `Beyond*` bucket is +inf. Then self narrows base on distance
        // iff self's ceiling is at or below base's — this single comparison covers
        // every None/Some combination correctly.
        let upper = |bucket: Option<DistanceBucket>| -> f64 {
            bucket.and_then(DistanceBucket::upper_mi).unwrap_or(f64::INFINITY)
        };
        let distance_ok = upper(self.distance) <= upper(base.distance);
        let prefix_ok = match &base.callsign_prefix {
            None => true,
            Some(bp) => self
                .callsign_prefix
                .as_ref()
                .is_some_and(|tp| tp.as_str().starts_with(bp.as_str())),
        };
        let hours_ok = match base.history_hours {
            None => true,
            Some(bh) => self.history_hours.is_some_and(|th| th <= bh),
        };

        set_narrows(self.modes.as_slice(), base.modes.as_slice())
            && set_narrows(self.bands.as_slice(), base.bands.as_slice())
            && set_narrows(self.bandwidths.as_slice(), base.bandwidths.as_slice())
            && self.ft8_policy.restrictiveness() >= base.ft8_policy.restrictiveness()
            && eq_narrows(self.operating_now, base.operating_now)
            && distance_ok
            && eq_narrows(self.bearing, base.bearing)
            && prefix_ok
            && hours_ok
    }
}

// ---------------------------------------------------------------------------
// Recommendation goal
// ---------------------------------------------------------------------------

/// The decision `recommend` is answering.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RecommendationGoal {
    /// "Which gateway should I connect to right now?" `at_utc_ms` overrides the
    /// injected current time (else the engine uses now).
    ConnectNow {
        #[serde(default)]
        at_utc_ms: Option<u64>,
        objective: ConnectObjective,
    },
    /// "Which gateway should I connect to at this future time?"
    BestAt {
        at_utc_ms: u64,
        objective: ConnectObjective,
    },
}

// ---------------------------------------------------------------------------
// The request
// ---------------------------------------------------------------------------

/// Default `recommend.candidate_count` when the agent omits it.
fn default_candidate_count() -> BoundedU8<1, 8> {
    // 3 is inside [1, 8] by construction, so `expect` cannot fire.
    BoundedU8::new(3).expect("3 is within [1, 8]")
}

/// The single intent-tagged `find_stations` request. The `intent` tag selects
/// what the agent is trying to do; the engine does the selection and bounds the
/// result by the tag's contract.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(tag = "intent", rename_all = "kebab-case")]
pub enum FindStationsRequest {
    /// "Which gateway should I connect to?" — a ranked decision answer.
    Recommend {
        #[serde(deserialize_with = "de_stringy_or_native")]
        goal: RecommendationGoal,
        #[serde(default, deserialize_with = "de_stringy_or_native")]
        filters: StationFilters,
        #[serde(default = "default_candidate_count", deserialize_with = "de_stringy_or_native")]
        candidate_count: BoundedU8<1, 8>,
        #[serde(default, deserialize_with = "de_stringy_or_native")]
        exclude_candidate_ids: BoundedVec<CandidateId, 16>,
    },
    /// Narrow a broad space by property; returns facets, not rows, until small.
    Explore {
        #[serde(default, deserialize_with = "de_stringy_or_native")]
        filters: StationFilters,
        #[serde(default)]
        snapshot_id: Option<SnapshotId>,
    },
    /// Known callsign(s) — exact lookup.
    Lookup {
        #[serde(default)]
        snapshot_id: Option<SnapshotId>,
        #[serde(deserialize_with = "de_stringy_or_native")]
        callsigns: BoundedVec<Callsign, 16>,
    },
    /// Server-side counts/statistics over the FULL matched population.
    Aggregate {
        #[serde(default, deserialize_with = "de_stringy_or_native")]
        filters: StationFilters,
        #[serde(deserialize_with = "de_stringy_or_native")]
        group_by: BoundedVec<StationFacet, 3>,
    },
    /// The full set as a user artifact OUTSIDE the transcript (never model-readable).
    Export {
        #[serde(default, deserialize_with = "de_stringy_or_native")]
        filters: StationFilters,
        format: StationExportFormat,
    },
}

/// rmcp tool-input wrapper for [`FindStationsRequest`].
///
/// An internally-tagged enum's natural JSON Schema is a bare `oneOf` with **no**
/// root `type`, but the MCP spec requires a tool's `inputSchema` to have root
/// `type: "object"` — rmcp rejects the enum directly. This transparent wrapper
/// deserializes straight to the enum while forcing `type: "object"` onto the
/// advertised schema (every intent variant IS a JSON object, so the assertion is
/// correct and merely more precise).
#[derive(Debug, Clone, Deserialize)]
#[serde(transparent)]
pub struct FindStationsParams(pub FindStationsRequest);

impl JsonSchema for FindStationsParams {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "FindStationsRequest".into()
    }

    // Inline so the object-rooted schema is what the tool advertises at the root,
    // not a `$ref` (which would again leave the root without a `type`).
    fn inline_schema() -> bool {
        true
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let inner = <FindStationsRequest as JsonSchema>::json_schema(generator);
        let mut value = serde_json::Value::from(inner);
        if let Some(map) = value.as_object_mut() {
            map.entry("type".to_string())
                .or_insert_with(|| serde_json::Value::String("object".to_string()));
        }
        schemars::Schema::try_from(value)
            .expect("FindStationsRequest schema is a JSON object with type injected")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- serde-rename shape locks (explicit-rename discipline) -------------

    #[test]
    fn band_renames_to_label_token() {
        assert_eq!(serde_json::to_string(&Band::B40m).unwrap(), "\"40m\"");
        assert_eq!(serde_json::to_string(&Band::B160m).unwrap(), "\"160m\"");
        let b: Band = serde_json::from_str("\"20m\"").unwrap();
        assert_eq!(b, Band::B20m);
        assert_eq!(Band::B20m.label(), "20m");
        // An out-of-set band is a hard error, not a silent no-match.
        assert!(serde_json::from_str::<Band>("\"40meters\"").is_err());
    }

    #[test]
    fn bandwidth_class_renames_to_numeric_token() {
        assert_eq!(serde_json::to_string(&BandwidthClass::Hz2750).unwrap(), "\"2750\"");
        assert_eq!(
            serde_json::from_str::<BandwidthClass>("\"500\"").unwrap(),
            BandwidthClass::Hz500
        );
        assert_eq!(BandwidthClass::Hz2300.hz(), 2300);
    }

    #[test]
    fn ft8_policy_defaults_to_ignore() {
        assert_eq!(Ft8Policy::default(), Ft8Policy::Ignore);
        assert_eq!(serde_json::to_string(&Ft8Policy::Require).unwrap(), "\"require\"");
    }

    #[test]
    fn distance_bucket_contains_mi() {
        assert!(DistanceBucket::Within300mi.contains_mi(250.0));
        assert!(!DistanceBucket::Within300mi.contains_mi(301.0));
        assert!(DistanceBucket::Beyond2500mi.contains_mi(3000.0));
        assert!(!DistanceBucket::Beyond2500mi.contains_mi(100.0));
    }

    #[test]
    fn bearing_sector_from_bearing_straddles_north() {
        assert_eq!(BearingSector::from_bearing(0.0), BearingSector::N);
        assert_eq!(BearingSector::from_bearing(350.0), BearingSector::N);
        assert_eq!(BearingSector::from_bearing(10.0), BearingSector::N);
        assert_eq!(BearingSector::from_bearing(45.0), BearingSector::Ne);
        assert_eq!(BearingSector::from_bearing(90.0), BearingSector::E);
        assert_eq!(BearingSector::from_bearing(180.0), BearingSector::S);
        assert_eq!(BearingSector::from_bearing(270.0), BearingSector::W);
        assert!(BearingSector::N.contains_deg(359.0));
    }

    // ---- one deserialize test per intent (valid) ---------------------------

    #[test]
    fn deserialize_recommend_intent() {
        let json = r#"{
            "intent": "recommend",
            "goal": { "kind": "connect-now", "objective": "estimated-success" },
            "filters": { "bands": ["40m"], "modes": ["vara-hf"] },
            "candidate_count": 5
        }"#;
        let req: FindStationsRequest = serde_json::from_str(json).unwrap();
        match req {
            FindStationsRequest::Recommend {
                candidate_count,
                filters,
                exclude_candidate_ids,
                ..
            } => {
                assert_eq!(candidate_count.get(), 5);
                assert_eq!(filters.bands.as_slice(), &[Band::B40m]);
                assert!(exclude_candidate_ids.is_empty());
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn recommend_candidate_count_defaults_to_three() {
        let json = r#"{
            "intent": "recommend",
            "goal": { "kind": "connect-now", "objective": "nearest" }
        }"#;
        let req: FindStationsRequest = serde_json::from_str(json).unwrap();
        match req {
            FindStationsRequest::Recommend { candidate_count, .. } => {
                assert_eq!(candidate_count.get(), 3);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn deserialize_explore_intent_bare() {
        // A bare broad explore — the overflow case the whole redesign targets.
        let req: FindStationsRequest =
            serde_json::from_str(r#"{ "intent": "explore" }"#).unwrap();
        assert!(matches!(req, FindStationsRequest::Explore { .. }));
    }

    #[test]
    fn deserialize_lookup_intent() {
        let json = r#"{ "intent": "lookup", "callsigns": ["W1ABC", "K2XYZ"] }"#;
        let req: FindStationsRequest = serde_json::from_str(json).unwrap();
        match req {
            FindStationsRequest::Lookup { callsigns, .. } => {
                assert_eq!(callsigns.len(), 2);
                assert_eq!(callsigns.as_slice()[0].as_str(), "W1ABC");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn deserialize_aggregate_intent() {
        let json = r#"{ "intent": "aggregate", "group_by": ["band", "mode"] }"#;
        let req: FindStationsRequest = serde_json::from_str(json).unwrap();
        match req {
            FindStationsRequest::Aggregate { group_by, .. } => {
                assert_eq!(
                    group_by.as_slice(),
                    &[StationFacet::Band, StationFacet::Mode]
                );
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn deserialize_export_intent() {
        let json = r#"{ "intent": "export", "format": "csv" }"#;
        let req: FindStationsRequest = serde_json::from_str(json).unwrap();
        assert!(matches!(
            req,
            FindStationsRequest::Export {
                format: StationExportFormat::Csv,
                ..
            }
        ));
    }

    // ---- bound rejections --------------------------------------------------

    #[test]
    fn recommend_candidate_count_out_of_range_rejected() {
        let zero = r#"{ "intent": "recommend",
            "goal": { "kind": "connect-now", "objective": "nearest" },
            "candidate_count": 0 }"#;
        assert!(serde_json::from_str::<FindStationsRequest>(zero).is_err());
        let nine = r#"{ "intent": "recommend",
            "goal": { "kind": "connect-now", "objective": "nearest" },
            "candidate_count": 9 }"#;
        assert!(serde_json::from_str::<FindStationsRequest>(nine).is_err());
    }

    #[test]
    fn lookup_over_sixteen_callsigns_rejected() {
        let many: Vec<String> = (0..17).map(|i| format!("W{i}ABC")).collect();
        let json = serde_json::json!({ "intent": "lookup", "callsigns": many });
        assert!(serde_json::from_value::<FindStationsRequest>(json).is_err());
    }

    #[test]
    fn aggregate_over_three_facets_rejected() {
        let json = r#"{ "intent": "aggregate",
            "group_by": ["band", "mode", "distance-bucket", "bearing-sector"] }"#;
        assert!(serde_json::from_str::<FindStationsRequest>(json).is_err());
    }

    #[test]
    fn unknown_intent_tag_errors() {
        let json = r#"{ "intent": "teleport" }"#;
        assert!(serde_json::from_str::<FindStationsRequest>(json).is_err());
    }

    #[test]
    fn params_wrapper_schema_has_object_root_type() {
        // MCP requires a tool inputSchema to have root `type: object`; the bare
        // intent-tagged enum's `oneOf` lacks it (rmcp rejects it). The wrapper
        // must inject it while staying a tagged union.
        let schema = serde_json::Value::from(schemars::schema_for!(FindStationsParams));
        assert_eq!(
            schema["type"], "object",
            "inputSchema root must be type object; got {schema}"
        );
        assert!(
            schema["oneOf"].is_array(),
            "still a tagged union of intents"
        );
    }

    #[test]
    fn params_wrapper_deserializes_transparently() {
        let FindStationsParams(req) =
            serde_json::from_str(r#"{ "intent": "explore" }"#).unwrap();
        assert!(matches!(req, FindStationsRequest::Explore { .. }));
    }

    #[test]
    fn lenient_deserialize_absorbs_stringified_model_args() {
        // The exact shape qwen-3.5-122b emitted (typed + nested args as JSON strings).
        let json = r#"{
            "intent": "recommend",
            "candidate_count": "5",
            "filters": "{\"modes\": [\"vara-hf\", \"vara-fm\"]}",
            "goal": "{\"kind\": \"connect-now\", \"objective\": \"estimated-success\"}"
        }"#;
        match serde_json::from_str::<FindStationsRequest>(json).unwrap() {
            FindStationsRequest::Recommend {
                candidate_count,
                filters,
                goal,
                ..
            } => {
                assert_eq!(candidate_count.get(), 5);
                assert_eq!(filters.modes.len(), 2);
                assert!(matches!(
                    goal,
                    RecommendationGoal::ConnectNow {
                        objective: ConnectObjective::EstimatedSuccess,
                        ..
                    }
                ));
            }
            _ => panic!("wrong variant"),
        }

        // The bound still applies to a stringified over-cap value.
        let over = r#"{ "intent": "recommend",
            "goal": {"kind":"connect-now","objective":"nearest"},
            "candidate_count": "9" }"#;
        assert!(serde_json::from_str::<FindStationsRequest>(over).is_err());

        // Native (non-stringified) form still works, and a stringified array too.
        assert!(serde_json::from_str::<FindStationsRequest>(
            r#"{ "intent": "aggregate", "group_by": ["band"] }"#
        )
        .is_ok());
        assert!(serde_json::from_str::<FindStationsRequest>(
            r#"{ "intent": "aggregate", "group_by": "[\"band\",\"mode\"]" }"#
        )
        .is_ok());
    }

    // ---- snapshot monotonicity (widening rejection) ------------------------

    fn with_bands(bands: Vec<Band>) -> StationFilters {
        StationFilters {
            bands: BoundedVec::from_capped(bands).0,
            ..Default::default()
        }
    }

    #[test]
    fn narrowing_is_reflexive() {
        let f = with_bands(vec![Band::B40m]);
        assert!(f.is_narrowing_of(&f));
        assert!(StationFilters::default().is_narrowing_of(&StationFilters::default()));
    }

    #[test]
    fn adding_a_band_narrows_but_dropping_one_widens() {
        let base = StationFilters::default(); // no band constraint
        let narrowed = with_bands(vec![Band::B40m]);
        assert!(narrowed.is_narrowing_of(&base));
        // Dropping the constraint widens — rejected.
        assert!(!base.is_narrowing_of(&narrowed));
        // A subset of the base's allowed bands narrows; a superset widens.
        let base_two = with_bands(vec![Band::B40m, Band::B20m]);
        assert!(with_bands(vec![Band::B40m]).is_narrowing_of(&base_two));
        assert!(!with_bands(vec![Band::B40m, Band::B20m, Band::B80m]).is_narrowing_of(&base_two));
    }

    #[test]
    fn distance_and_ft8_monotonicity() {
        let closer = StationFilters {
            distance: Some(DistanceBucket::Within100mi),
            ..Default::default()
        };
        let farther = StationFilters {
            distance: Some(DistanceBucket::Within300mi),
            ..Default::default()
        };
        assert!(closer.is_narrowing_of(&farther));
        assert!(!farther.is_narrowing_of(&closer));

        let require = StationFilters {
            ft8_policy: Ft8Policy::Require,
            ..Default::default()
        };
        let ignore = StationFilters::default(); // Ft8Policy::Ignore
        assert!(require.is_narrowing_of(&ignore));
        assert!(!ignore.is_narrowing_of(&require));
    }
}
