# find_stations — Agent-Native Redesign (Design Spec)

**Status:** Approved (operator, 2026-07-23). Shape from a GPT-5.6-sol-high consult
(ADR 0028); operator chose the single-tool variant.
**bd:** tuxlink-m0n38. **Relates:** tuxlink-nirxk (general harness result-budget
backstop — a separate safety net this redesign makes unnecessary *for this tool*),
tuxlink-u2qge. **Unblocks:** tuxlink-t3jci (the Build-Carefully qwen battery — broad
`find_stations` currently kills cells).

## Problem

`find_stations` is the **identical code path the GUI station-finder uses**
(`mcp_ports.rs:3037`) — a raw list-dump built for a human scrolling a UI, exposed
unchanged to the agent. `curate_and_rank_gateways` returns **all** matching gateways
with **no count cap**. A broad query (no effective band filter) returns the entire
catalog: **~1,391–1,399 gateways ≈ 560 KB ≈ 250k tokens in one tool-result message**.

That single message + the ~13k-token system prompt exceeds the 262,144-token window.
The provider's transcript-trimmer can drop *other* messages but **cannot shrink one
message below the window**, so a broad `find_stations` result is un-trimmable and
un-survivable: the next turn 400s with context overflow and the agent's task dies.

Empirically confirmed on `qwen35-122b-nvfp4` (34 prior calls scanned): band-scoped
queries return ~206–311 gateways (130–185 KB, fit); broad queries return ~1,400
(560 KB, overflow). `EU3` overflowed repeatedly — those are the `nirxk` instances.

**Two failure modes make simple bounding insufficient:**
1. **Silent top-N** → the agent reasons over a partial set *believing it is complete*
   and confidently picks "the best gateway" from 25 of 1,391. Wrong is worse than
   overflowing.
2. **Blind pagination** → an agent cannot page through 56 pages within its context
   window and has no basis to know which page holds the answer.

## Invariant (the requirement)

A tool call must **never** be able to emit output fatal to the agent, **and** must
never let the agent mistake a subset for the whole. This must hold **by
construction** (as a structural property of the types), not by a heuristic cap.

## Design: one intent-tagged tool

`find_stations` stays a **single MCP tool** (operator decision: one tool so a weak
local model cannot pick the wrong one). The agent states *what it is trying to do*;
the code does the selection. The agent supplies **only semantic intent + user
constraints**; the code injects the facts the app already owns (operator grid,
current time, configured/available transports, operating hours, propagation inputs,
FT8 evidence, connection history). The agent never needs the raw catalog to form a
goal.

### Request (tagged by intent)

```rust
#[serde(tag = "intent", rename_all = "kebab-case")]
enum FindStationsRequest {
    /// "Which gateway should I connect to?" — ranked decision answer.
    Recommend {
        goal: RecommendationGoal,
        filters: StationFilters,
        candidate_count: BoundedU8<1, 8>,          // default 3
        exclude_candidate_ids: BoundedVec<CandidateId, 16>,
    },
    /// Narrow a broad space by property; returns facets, not rows, until small.
    Explore {
        filters: StationFilters,
        snapshot_id: Option<SnapshotId>,           // additive filters must narrow it
    },
    /// Known callsign(s) — exact lookup.
    Lookup {
        snapshot_id: Option<SnapshotId>,
        callsigns: BoundedVec<Callsign, 16>,
    },
    /// Server-side counts/statistics over the FULL matched population.
    Aggregate {
        filters: StationFilters,
        group_by: BoundedVec<StationFacet, 3>,
    },
    /// The full set as a user artifact OUTSIDE the transcript (never model-readable).
    Export {
        filters: StationFilters,
        format: StationExportFormat,               // json | csv
    },
}

enum RecommendationGoal {
    ConnectNow { at_utc_ms: Option<u64>, objective: ConnectObjective },
    BestAt     { at_utc_ms: u64,          objective: ConnectObjective },
}
// ConnectObjective = EstimatedSuccess | Nearest
```

`StationFilters` uses **bounded enums** where possible: modes, bands, bandwidth
classes, FT8 policy (`ignore | prefer | require`), operating-now, distance bucket,
bearing sector, callsign prefix, history window. All optional; omission means "no
constraint," and breadth is handled by the response contract, not by silent capping.

### Response (common envelope + tagged result union)

```jsonc
{
  "snapshot":   { "id": "sq_…", "fetched_at_ms": …, "operator_grid": "DM43", "expires_at_ms": … },
  "population": { "count_unit": "station", "matched_stations": 311,
                  "eligible_stations": 206, "eligible_connection_options": 487 },
  "result":     { "kind": "…", … }   // tagged union, below
}
```

`result` variants (each has hard structural bounds — see §Bounds):

- **`complete-set`** — ≤16 station summaries; `omitted_stations` MUST be 0. Only
  constructible when the *entire* eligible population fits the bounded type.
- **`ranked-subset`** — ≤8 candidates, explicitly `relationship: top-of-all-eligible`,
  with **mandatory exact** `evaluated_stations` / `returned_stations` /
  `omitted_stations`. Carries the versioned ranking policy + `inputs_used` /
  `inputs_unavailable`.
- **`refinement-required`** — **zero station rows**; exact `matched_stations` total,
  **finite per-facet counts** (`remaining_if_applied`), and **bounded**
  `suggested_refinements` (labelled additive filter patches with the resulting count).
- **`aggregate-complete`** — server-side counts/statistics over the whole matched
  population (no rows).
- **`export-ready`** — artifact id, format, total rows, GUI-visible destination;
  **no catalog data inline**.
- **`no-matches`** — an explicitly *complete* empty result.

A ranked candidate is **station-level with one selected connection**, not a raw
catalog row (this is what keeps `recommend` bounded regardless of channel count):

```jsonc
{
  "candidate_id": "sq_…/W1ABC/FN31",
  "callsign": "W1ABC", "grid": "FN31",
  "selected_connection": { "target_callsign": "W1ABC", "mode": "vara-hf",
                           "frequency_khz": 7103.5, "bandwidth_hz": 500 },
  "alternate_connection_count": 4,
  "fitness": { "score": 0.82,
    "components": { "path_reliability": 0.78, "ft8_corroborated": true,
                    "operating_now": true, "prior_success": null },
    "reason_codes": ["PATH_STRONG", "FT8_CORROBORATED"] }
}
```

### Ranking honesty

The ranking policy is **versioned** (e.g. `connect-now-v1`). Its scope MUST be
`evaluated == eligible`: if the code cannot evaluate the full eligible population, it
returns **`refinement-required`**, never an approximate "best." Distance-only ranking
is honestly named **`nearest-v1`** and is never presented as connection fitness.

### Narrowing / serving the long tail (never a page cursor)

- **Broad `explore`** → `refinement-required` with facet counts + suggested filter
  patches. The agent adds a predicate against the stable `snapshot_id`; the tool
  reports the exact consequence. This selects a *meaningful* subset by property — not
  a page. The snapshot pins counts so they don't drift between calls.
- **"Give me another option"** → `recommend` with `exclude_candidate_ids` — decision-
  driven continuation, not "page 2."
- **Known callsign** → `lookup`. **"How many by band/mode/distance?"** → `aggregate`
  (full-set counts). **"Give me everything"** → `export` (a human CSV/JSON artifact
  outside the transcript; **not** a model-readable resource that could recreate the
  560 KB dump).

## Invariant enforcement (by construction)

1. Every output collection is a `BoundedVec` / `BoundedU8`.
2. Every externally-sourced string has a maximum encoded length.
3. `recommend` returns exactly one `selected_connection` per station (never every
   channel row).
4. A broad exploratory query returns **zero** station rows.
5. `complete-set` is constructible **only** when the whole population fits the bound.
6. A subset is a **distinct schema variant** with mandatory evaluated/returned/omitted
   — so "silent partial as complete" is *unrepresentable*.
7. Omitted data is reachable by predicate, identity, aggregation, exclusion, or export.
8. The MCP `outputSchema` advertises `maxItems` + required coverage fields + the
   tagged variants (documentation + validation).
9. Runtime bounded-vector *constructors* enforce the limits (schema alone is docs).
10. A property test serializes the worst legal value (including any compatibility
    text duplication) and proves it stays under the station-tool **byte budget**.

A final station-specific **postcondition** returns a small internal
`contract-violation` error if the impossible ever happens. That is invariant
enforcement — **not** truncation and **not** the general harness backstop. Under
normal operation this tool never invokes a continuation backstop.

## Bounds (concrete, enforced)

| Field | Bound |
|---|---|
| `recommend.candidate_count` | 1–8 (default 3) |
| `ranked-subset` candidates | ≤ 8 |
| `complete-set` summaries | ≤ 16 (`omitted` must be 0) |
| `lookup.callsigns` | ≤ 16 |
| `exclude_candidate_ids` | ≤ 16 |
| `group_by` facets | ≤ 3 |
| `refinements` / `suggested_refinements` | ≤ 12 |
| per-facet value rows | ≤ 24 |
| callsign / grid / reason-code strings | length-capped (existing `sanitize_display` floors + explicit max) |
| **whole tool result** | **≤ 32 KB serialized (~10k tokens)** — the property-test ceiling (tunable; must stay << `window − system_prompt`) |

## Backend architecture

Do **not** cap `catalog_fetch_stations` / `curate_and_rank_gateways` globally — the
**GUI keeps its full scrolling/map list**. Instead split the shared backend so the
agent path and the GUI path diverge *after* a common normalized snapshot:

```text
StationsCache / catalog fetch
            │
      normalized snapshot ──────────────┐
       │                                │
GUI projection                    StationQueryEngine
(full human list, unchanged)      curate → aggregate → filter
                                  → goal-rank / facet → BOUNDED MCP result
```

- Split today's `curate_and_rank_gateways` into **`curate_gateways`** (the shared
  PII/validation curation) and **policy-specific ranking** — "distance sort" must not
  be baked into the generic curation seam.
- `StationQueryEngine` owns: population counting, facet aggregation, filter
  application, goal ranking, and building the bounded tagged result. It is the single
  place the invariant is enforced.
- The **routines `data.find_stations` action** consumes the same `StationQueryEngine`
  and carries the same coverage semantics. Its current unbounded-by-default output +
  undisclosed `limit` repeat the exact defect and must be corrected. It MAY keep a
  routines-specific projection but not its own uncapped path.

## Parity / compatibility

- **ADR 0027:** `find_stations` is an existing command; this is a
  signature/behaviour change, **not** a new command — no MCP tool-budget debit.
  Re-run the parity manifest test.
- **GUI:** unchanged (still consumes the full projection). Verify the finder + map
  still render the whole list.
- **Breaking change** to the `find_stations` MCP response shape — the built-in Elmer
  system prompt's routine-authoring guidance and any docs referencing find_stations
  output must be updated in the same change.

## Error handling

- "Too broad" is **not** an error — it is the normal `refinement-required` path.
- `snapshot_id` has `expires_at_ms`; an expired/unknown snapshot on `explore`/`lookup`
  is a typed, retryable error telling the agent to re-issue the base query.
- Filters that would *widen* a snapshot (non-monotonic) are rejected with a typed
  error (snapshots only narrow).
- The postcondition `contract-violation` error is internal-only and should never fire
  in normal operation; it exists to fail loud rather than emit a fatal payload.

## Testing

- **Property test (load-bearing):** for every `FindStationsRequest`, the serialized
  result is `< 32 KB` — constructed from the worst legal population, exercising every
  variant.
- **Variant coverage:** each `result` kind has unit tests, including
  `complete-set` requires `omitted==0`, `ranked-subset` requires exact
  evaluated/returned/omitted, `refinement-required` has zero rows + finite facets.
- **Ranking honesty:** a test that `evaluated != eligible` forces `refinement-required`
  (never an approximate best); `nearest-v1` is never labelled fitness.
- **Snapshot semantics:** narrowing keeps counts stable; widening is rejected;
  expiry is typed.
- **Parity:** `outputSchema` validates; parity manifest unchanged (no new command);
  routines action goes through the engine (compile the embedded skill example).
- **Regression:** replay the qwen `C2`/`EU3` broad queries against the new tool and
  assert the result is a bounded `refinement-required`, never an overflow.

## Alternatives considered

- **Two tools (`recommend_gateways` + `query_station_directory`).** Smaller per-tool
  schemas (easier for a weak model), but +1 ADR-0027 tool slot and a wrong-tool-
  selection surface. **Declined by operator**: one tool is simpler for the agent to
  not call the wrong one.
- **Faceted-search only.** Safest transitional design; satisfies overflow +
  completeness but costs turns and makes the model choose its own narrowing path —
  weaker for the common "what should I connect to now?" task. Folded in as the
  `explore` intent rather than the whole tool.
- **Silent top-K.** Rejected — misleads the agent.
- **Count-first without facets or server-side ranking.** Rejected — tells the agent
  1,391 matches exist but gives it no principled route to the answer.

## Out of scope (tracked separately)

- The **general harness result-budget backstop** (any tool's oversized result
  truncated with a continuation handle) — `tuxlink-nirxk`. This redesign makes
  `find_stations` correct without needing it, but the backstop remains valuable as a
  crate-wide safety net for other tools.
