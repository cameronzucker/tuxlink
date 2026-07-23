# find_stations Agent-Native Redesign — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Replace the list-dump `find_stations` with an intent-tagged, bounded-by-construction agent query so it can never emit output fatal to (or silently misleading to) the agent.

**Spec (source of truth — do not re-derive):** `docs/superpowers/specs/2026-07-23-find-stations-agent-native-redesign.md`. bd `tuxlink-m0n38`. Operator-approved single-tool shape (GPT-5.6-sol consult).

**Architecture:** `StationsCache`/`catalog_fetch_stations` (unchanged, uncapped) → a normalized snapshot → two consumers: the GUI projection (full list, unchanged) and a new **`StationQueryEngine`** (curate → aggregate/filter → goal-rank/facet → **bounded** tagged result). The MCP `find_stations` tool and the routines `data.find_stations` action both dispatch through the engine.

**Tech Stack:** Rust (`src-tauri`), the `rmcp` MCP surface, serde, the existing `catalog` + `mcp_ports` + `routines` modules.

## Global Constraints

- MSRV 1.75; clippy `-D warnings --all-targets --locked`; no APIs stabilized >1.75. No cargo on this Pi — write + push, CI (both arches) or R2 compiles.
- Conventional commits; `Agent: <moniker>` + `Co-Authored-By`; branch `bd-tuxlink-m0n38/find-stations-redesign`.
- **Invariant (spec):** bounded by construction — every output collection bounded, every string length-capped, subset is a distinct schema variant, worst legal value property-tested `< 32 KB`, postcondition contract-violation error (not truncation).
- **Do NOT cap `catalog_fetch_stations` / the GUI path.** The bound lives only in `StationQueryEngine`'s result builder.
- **Breaking change** to the `find_stations` MCP response — update the Elmer system prompt's routine-authoring guidance + find_stations docs in the same change (P7).
- ADR 0027: signature change, no new command — parity manifest unchanged; re-run its test.

## Grounding (do first, every task assumes it)

Locate the existing types the new code must interoperate with / preserve:
```
git grep -n "StationFilterDto\|StationListDto\|GatewayDto\|StationModeDto\|struct.*Gateway\b" src-tauri/src
git grep -n "fn curate_and_rank_gateways\|fn curate_gateway\|async fn find_stations\|fn sort_gateways_by_distance" src-tauri/src/mcp_ports.rs
git grep -n "catalog_fetch_stations" src-tauri/src
```
Key sites: `mcp_ports.rs` `find_stations` (~L3021), `curate_and_rank_gateways` (~L2809), `curate_gateway`, `sort_gateways_by_distance`; `routines/actions/find_stations.rs` (`execute`, its `limit`); `catalog/commands.rs::catalog_fetch_stations` (shared fetch — stays uncapped).

---

## Decomposition (phases, sequence, gates)

| # | Phase | Depends on | Gate |
|---|---|---|---|
| **P1** | Bounded primitives (`BoundedVec<T,N>`, `BoundedU8`, length-capped string newtypes) | — | none |
| **P2** | Request types (`FindStationsRequest` intent enum + goal/filters/facets) | P1 | none |
| **P3** | Response types (snapshot+population envelope + tagged `result` union + candidate/fitness) | P1 | none |
| **P4** | Split `curate_and_rank_gateways` → `curate_gateways` + `rank_gateways`; normalized snapshot + snapshot store (id/expiry/stable counts) | grounding | none |
| **P5** | `StationQueryEngine` — one intent at a time: explore/refine, recommend/rank, lookup, aggregate, export | P2,P3,P4 | none |
| **P6** | Rewire MCP `find_stations` to dispatch via the engine; `outputSchema` advertises bounds | P5 | none |
| **P7** | Rewire routines `data.find_stations` action to the engine + coverage semantics; update Elmer system prompt + docs (breaking change) | P5 | operator eyes on the new system-prompt text |
| **P8** | Property test (`< 32 KB` worst legal value, all variants) + parity test + qwen C2/EU3 regression replay | P6,P7 | validate on qwen (operator/R2) |

**P5 is detailed after P1–P4 land** (it depends on their concrete types). This document details **P1–P4** in full and pins P5–P8's interfaces + tests.

---

### Task P1.1: Bounded collection + scalar newtypes

**Files:**
- Create: `src-tauri/src/mcp_ports/bounded.rs` (or `src-tauri/src/station_query/bounded.rs` if a new module is preferred; keep it near the query engine)
- Modify: the module's `mod` declaration.

**Interfaces — Produces:**
- `pub struct BoundedVec<T, const N: usize>(Vec<T>)` with `from_capped(iter) -> (Self, usize /*omitted*/)`, `try_from(Vec<T>) -> Result<Self, CapExceeded>`, `as_slice`, `len`, serde `Serialize`/`Deserialize` (deserialize rejects `> N`).
- `pub struct BoundedU8<const MIN: u8, const MAX: u8>(u8)` with clamped/validated constructor.
- `pub struct CappedString<const MAX: usize>(String)` with `from_truncated(&str) -> Self` (grapheme-safe cap) + serde.

- [ ] **Step 1: failing tests**

```rust
#[test]
fn bounded_vec_from_capped_reports_omitted() {
    let (bv, omitted) = BoundedVec::<u32, 3>::from_capped(vec![1,2,3,4,5]);
    assert_eq!(bv.as_slice(), &[1,2,3]);
    assert_eq!(omitted, 2);
}
#[test]
fn bounded_vec_deserialize_rejects_over_cap() {
    let r: Result<BoundedVec<u32,2>,_> = serde_json::from_str("[1,2,3]");
    assert!(r.is_err());
}
#[test]
fn bounded_u8_rejects_out_of_range() {
    assert!(BoundedU8::<1,8>::new(0).is_err());
    assert!(BoundedU8::<1,8>::new(9).is_err());
    assert_eq!(BoundedU8::<1,8>::new(3).unwrap().get(), 3);
}
#[test]
fn capped_string_truncates_on_grapheme_boundary() {
    assert_eq!(CappedString::<4>::from_truncated("abcdef").as_str(), "abcd");
}
```

- [ ] **Step 2: verify fail** (CI/R2). **Step 3: implement** the three newtypes with const-generic caps + serde. **Step 4: verify pass.** **Step 5: commit** (`feat(station-query): bounded primitive newtypes (tuxlink-m0n38)`).

---

### Task P2.1: Request types — intent-tagged `FindStationsRequest`

**Files:**
- Create: `src-tauri/src/station_query/request.rs`
- Modify: module `mod`.

**Interfaces — Consumes:** P1 newtypes. **Produces** (verbatim from the spec §Request):

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "intent", rename_all = "kebab-case")]
pub enum FindStationsRequest {
    Recommend { goal: RecommendationGoal, filters: StationFilters,
                candidate_count: BoundedU8<1,8>, exclude_candidate_ids: BoundedVec<CandidateId,16> },
    Explore   { filters: StationFilters, snapshot_id: Option<SnapshotId> },
    Lookup    { snapshot_id: Option<SnapshotId>, callsigns: BoundedVec<Callsign,16> },
    Aggregate { filters: StationFilters, group_by: BoundedVec<StationFacet,3> },
    Export    { filters: StationFilters, format: StationExportFormat },
}
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RecommendationGoal {
    ConnectNow { at_utc_ms: Option<u64>, objective: ConnectObjective },
    BestAt     { at_utc_ms: u64,          objective: ConnectObjective },
}
// ConnectObjective { EstimatedSuccess, Nearest } (rename_all kebab)
// StationExportFormat { Json, Csv }
// StationFacet { Mode, Band, DistanceBucket, BearingSector, ... } (kebab)
// StationFilters { modes/bands/bandwidths (bounded Vecs of enums), ft8_policy,
//   operating_now: Option<bool>, distance_bucket, bearing_sector,
//   callsign_prefix: Option<CappedString<..>>, history_hours: Option<u32> }
// CandidateId / Callsign / SnapshotId = CappedString newtypes.
```

- [ ] **Step 1: failing tests** — one deserialize test per intent (valid), plus: `candidate-count` of 0 or 9 is rejected; `exclude`/`callsigns` over 16 rejected; an unknown `intent` tag errors.
- [ ] **Step 2–4** implement + verify. **Reuse existing enums** where they already exist (grep `StationModeDto` / band enums) rather than duplicating — the request should share the app's mode/band vocabulary. **Step 5: commit.**

---

### Task P3.1: Response types — envelope + tagged result union

**Files:** Create `src-tauri/src/station_query/response.rs`; modify module `mod`.

**Interfaces — Produces** (verbatim from spec §Response):

```rust
#[derive(Serialize)] pub struct FindStationsResponse { snapshot: SnapshotMeta, population: Population, result: StationResult }
#[derive(Serialize)] pub struct SnapshotMeta { id: SnapshotId, fetched_at_ms: u64, operator_grid: Option<CappedString<8>>, expires_at_ms: u64 }
#[derive(Serialize)] pub struct Population { count_unit: &'static str, matched_stations: u32, eligible_stations: u32, eligible_connection_options: u32 }
#[derive(Serialize)] #[serde(tag="kind", rename_all="kebab-case")]
pub enum StationResult {
    CompleteSet   { stations: BoundedVec<StationSummary,16> },               // constructor asserts omitted==0
    RankedSubset  { ranking: RankingMeta, coverage: SubsetCoverage, top_candidates: BoundedVec<Candidate,8> },
    RefinementRequired { matched: u32, facets: BoundedVec<Facet,8>, suggested_refinements: BoundedVec<Refinement,12> },
    AggregateComplete  { groups: BoundedVec<AggregateGroup,24> },
    ExportReady   { artifact_id: CappedString<64>, format: StationExportFormat, total_rows: u32, destination: CappedString<128> },
    NoMatches,
}
// SubsetCoverage { evaluated_stations, returned_stations, omitted_stations, relationship: "top-of-all-eligible" }
// Candidate { candidate_id, callsign, grid, selected_connection: ConnectionDto,
//   alternate_connection_count: u32, fitness: Fitness }
// Fitness { score: f32, components: FitnessComponents, reason_codes: BoundedVec<CappedString<24>,6> }
// Facet { field, values: BoundedVec<FacetCount,24> }  FacetCount { value, remaining_if_applied }
// Refinement { label, add_filters: StationFilters, remaining: u32 }
```

- [ ] **Step 1: failing tests** — serialize each variant; assert the tags (`kind`) and that `ranked-subset` carries all three coverage counts; `CompleteSet::new` panics/errs if `omitted != 0` (the "silent partial as complete is unrepresentable" guard).
- [ ] **Step 2–4** implement + verify. **Step 5: commit.**

---

### Task P4.1: Split curation from ranking + normalized snapshot

**Files:** Modify `src-tauri/src/mcp_ports.rs` (`curate_and_rank_gateways`, `curate_gateway`, `sort_gateways_by_distance`); create `src-tauri/src/station_query/snapshot.rs`.

- [ ] **Step 1:** Introduce `pub(crate) fn curate_gateways(listings, bands, grid) -> Vec<GatewayDto>` = today's curation + band filter **without** the distance sort. Keep `curate_and_rank_gateways` as a thin wrapper (`curate_gateways` then `sort_gateways_by_distance`) so **existing GUI/routines callers are byte-identical** (grep confirms callers; they must not change behavior). Test: `curate_and_rank_gateways` output unchanged vs a fixture; `curate_gateways` returns the same set unsorted.
- [ ] **Step 2:** `StationSnapshot` = the normalized curated population for a query + a `SnapshotId` + `fetched_at_ms` + `expires_at_ms`. A `SnapshotStore` (in-memory, TTL) keyed by id; `get(id)` returns `Err(Expired|Unknown)`. A snapshot is built from `catalog_fetch_stations` results via `curate_gateways`. Test: store round-trip; expiry is typed; two identical queries against one snapshot give identical counts (stable); a widening filter vs a snapshot is rejected.
- [ ] **Step 3:** verify (CI/R2). **Step 4: commit** (`refactor(station): split curation from ranking + normalized snapshot (tuxlink-m0n38)`).

---

### Task P5.x: `StationQueryEngine` (detailed after P1–P4)

**Files:** Create `src-tauri/src/station_query/engine.rs`.

**Interface — Produces:** `pub fn evaluate(req: FindStationsRequest, ctx: StationContext) -> Result<FindStationsResponse, StationQueryError>`, where `StationContext` carries the **code-injected facts** (operator grid, now, transports, hours, propagation, FT8, history) resolved from `AppHandle` — the agent never supplies these.

Sub-tasks (one per intent; each independently testable):
- [ ] **P5.1 explore/refine:** broad → `RefinementRequired` (zero rows) with facet counts + suggested patches; a query whose eligible set already fits ≤16 → `CompleteSet`. Test: 1,400-gateway fixture → `refinement-required`, matched=1400, facets present, **zero rows**; a 12-gateway eligible set → `complete-set` omitted=0.
- [ ] **P5.2 recommend/rank:** ranked top-K (≤`candidate_count`) via versioned policy; **one** `selected_connection` per station; `exclude_candidate_ids` drops then continues. **If the engine cannot evaluate the full eligible population → `RefinementRequired`, never an approximate best.** `nearest` objective → `nearest-v1`, never labelled fitness. Test: ranking is deterministic on a fixture; exclude yields the next best; un-evaluable-full-set forces refinement.
- [ ] **P5.3 lookup:** exact callsign(s) → `CompleteSet` (≤16) or `NoMatches`. Test.
- [ ] **P5.4 aggregate:** server-side counts over the **full** matched population grouped by `group_by`. Test: counts equal the fixture totals (not a sampled subset).
- [ ] **P5.5 export:** writes a user CSV/JSON artifact to the app's export destination, returns `ExportReady` (id + total_rows + destination), **no catalog data inline**. Test: response carries no gateway rows; artifact exists; not exposed as a model-readable resource.
- Each sub-task: TDD (failing test → impl → verify → commit).

---

### Task P6.1: Rewire MCP `find_stations` to the engine

**Files:** Modify `src-tauri/src/mcp_ports.rs` (`find_stations` method + its `ToolSpec`/`inputSchema`/`outputSchema`).

- [ ] Replace the `filter: StationFilterDto` signature with `FindStationsRequest`; the method resolves `StationContext` from `self.app` and returns `engine::evaluate(...)`. Advertise the new `inputSchema` (intent-tagged) + `outputSchema` (maxItems on every collection, required coverage fields). Tests: end-to-end per intent through the MCP surface; **the C2/EU3 regression** — a broad `explore`/`recommend` with the 1,400-gateway fixture returns a bounded `refinement-required`, serialized `< 32 KB`, never an overflow. Commit.

---

### Task P7.1: Routines action + system prompt + docs

**Files:** Modify `src-tauri/src/routines/actions/find_stations.rs`; the Elmer system prompt (`tuxlink-agent-frontend/src/provider.rs` `ELMER_SYSTEM_PROMPT` routine-authoring section); find_stations docs (`git grep -l find_stations docs/`).

- [ ] Route the routines action through `StationQueryEngine` with the same coverage semantics (it may keep a routines-specific callsign projection but not its own uncapped path; drop the undisclosed default `limit` in favour of the engine's bounds). Update the Elmer system prompt's routine-authoring guidance to the new intent-tagged shape (**operator reviews this text** — it is the agent's primary instruction). Update docs. Tests: routines action returns bounded coverage; embedded skill/system-prompt examples compile/validate. Commit.

---

### Task P8.1: Property test + parity + qwen regression

- [ ] **Property test (load-bearing):** for every `FindStationsRequest` variant built from the worst legal population (max facets, max candidates, longest legal strings, compat duplication), `serde_json::to_vec(&response).len() < 32_768`. 
- [ ] **Parity:** `pnpm vitest run src/parityManifest.test.ts` — no new command; `outputSchema` validates.
- [ ] **qwen regression (operator/R2):** rebuild `elmer_battery`, replay `C2` and `EU3` on `qwen35-122b-nvfp4`; assert the `find_stations` result is a bounded `refinement-required` and the cell no longer `provider_error`s on context overflow. This is the "test it in practice" gate.
- [ ] Commit; open the PR (spec + implementation on one branch); CI green both arches; wire-walk the agent flow (operator supplies flows); merge.

---

## Self-Review

- **Spec coverage:** intents (P2) ↔ result union (P3) ↔ engine per-intent (P5) ↔ MCP (P6) ↔ routines/prompt (P7); invariant enforced by P1 bounds + P3 `CompleteSet` guard + P8 property test; GUI untouched (P4 wrapper preserves callers); ranking honesty (P5.2). Backstop (nirxk) explicitly out of scope.
- **Placeholders:** P1–P4 fully specified; P5–P8 pin exact interfaces + test intents (detailed at their gate per the phased structure, matching the scaffold plan precedent). New types are the spec's verbatim shapes; existing types are grep-grounded to preserve fields.
- **Type consistency:** `FindStationsRequest`/`StationResult`/`FindStationsResponse`/`StationQueryEngine::evaluate`/`StationContext` names are used identically across P2/P3/P5/P6/P7.
