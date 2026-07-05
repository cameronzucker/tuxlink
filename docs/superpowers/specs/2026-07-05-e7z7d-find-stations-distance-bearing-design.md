# Design — e7z7d FIX A: `find_stations` distance + bearing on the agent surface

**Issue:** tuxlink-e7z7d (scoped to FIX A only) · **Agent:** kingfisher-cove-yew · **Date:** 2026-07-05
**Branch:** `bd-tuxlink-e7z7d/find-stations-distance-bearing` (off `origin/main` @ f16fec3c)

## Problem

The in-app Elmer agent repeatedly cannot compute distance between stations despite
having their grids, and cannot rank gateways by distance. Operator observed this
2026-07-01.

The distance capability **already ships** — but only in the human UI. `src/catalog/distance.ts`
("Local great-circle distance for distance-sorted station results") and the canonical
`src/forms/position/distance.ts` (`distanceBetweenGrids`, `haversineKm`, `R=6371`) compute
gateway distance client-side in TypeScript for the catalog view. The **Rust MCP `GatewayDto`
the agent receives over the tool surface carries no distance or bearing** — the enrichment
lives in a different layer (TS/UI) and process than the agent's tool path. That gap is the bug.

This is therefore a **mirror job, not an invention**: reproduce the shipping TS distance
algorithm in Rust on the agent-facing DTO, numerically identical so the agent's kilometers
match what the operator sees in the catalog UI.

## Scope

**In scope (FIX A):** `find_stations` returns `distance_km` + `bearing_deg` from the operator's
grid for each gateway; results sorted by distance; mirrored into the elmer-distill simulator
tool_surface (parity gate).

**Out of scope — explicit non-goals (do NOT):**
- **No place-name labels / reverse geocode** — that is tuxlink-atnsu.
- **No `predict_path` changes** — FIX B (optional `frequencies_khz`) is split to tuxlink-0lawk,
  tabled and gated on the FT-8 epic (tuxlink-u3m0g.2) per operator 2026-07-05.
- **No new UI** — the human catalog UI already shows distance.
- **Do NOT change the app's shipped UI distance constant** (`R=6371` in the TS modules). The
  agent side conforms to it; the UI number does not move.

## Design

### 1. New Rust geo helper — `src-tauri/src/position/geo.rs`

Sibling to `maidenhead.rs` (which stays focused on grid↔lat/lon). Mirrors the shipping TS
algorithm exactly.

```rust
/// Great-circle distance in km (haversine). R = 6371.0 km — the exact constant the
/// shipping TS UI uses (src/forms/position/distance.ts), so the agent's km == the UI's km.
pub fn haversine_km(a: (f64, f64), b: (f64, f64)) -> f64;

/// Great-circle initial bearing a→b, degrees in [0,360) (0=N, 90=E). NEW — no TS/sim
/// reference exists; standard atan2 azimuth formula.
pub fn bearing_deg(a: (f64, f64), b: (f64, f64)) -> f64;

/// Distance + bearing between two Maidenhead grids (each → square center via
/// maidenhead::grid_to_lat_lon). Mirrors TS distanceBetweenGrids null semantics:
/// None if EITHER grid is absent OR malformed (grid_to_lat_lon → None).
pub fn distance_bearing_between_grids(a: Option<&str>, b: Option<&str>) -> Option<(f64, f64)>;
```

- `EARTH_RADIUS_KM = 6371.0` (const), matching `src/forms/position/distance.ts:5`.
- Reuses `crate::position::maidenhead::grid_to_lat_lon` (`src-tauri/src/position/maidenhead.rs:32`),
  which returns the square/subsquare **center** — same basis as the TS `gridToLatLon`.
- Registered in `src-tauri/src/position/mod.rs`.

### 2. `GatewayDto` extension — `src-tauri/tuxlink-mcp-core/src/ports.rs:359`

Add two fields:

```rust
/// Great-circle distance in km from the operator's grid to this gateway's grid.
/// None when the gateway grid is absent/invalid OR the operator grid is unresolved.
pub distance_km: Option<f64>,
/// Great-circle initial bearing in degrees [0,360) from the operator to this gateway.
/// None under the same conditions as distance_km.
pub bearing_deg: Option<f64>,
```

- **Always serialized** (no `skip_serializing_if`) — a present-but-null field teaches the model
  the capability exists and is currently unknowable (groundable state); a missing field cannot be
  distinguished from "tool can't do distance," which is a confabulation surface.
- `Option` because `grid` is already `Option<String>` and the operator grid can be unresolved.

### 3. `find_stations` impl enrichment — `src-tauri/src/mcp_ports.rs:2009`

The concrete `StationsPort::find_stations` impl (behind the thin router adapter at
`router.rs:386`) is where the operator grid can be resolved. Steps:

1. Resolve the operator's 4-char grid using the **exact pattern already used at
   `mcp_ports.rs:237-252`**: `PositionArbiter` state → `effective_broadcast_locator(&cfg, Some(arbiter))`
   → `broadcast_grid(.., PositionPrecision::FourCharGrid)`. Extract this into a shared
   `resolve_operator_grid(&self) -> Option<String>` helper reused by both this and the
   predict_path site (light DRY cleanup in code we are touching; do NOT refactor unrelated code).
2. For each `GatewayDto`, set `distance_km`/`bearing_deg` from
   `geo::distance_bearing_between_grids(operator_grid.as_deref(), gateway.grid.as_deref())`.
3. **Sort `gateways` ascending by `distance_km`**, `None` (unknown) sorted last — matching the
   UI's distance-sorted order. Stable secondary order = existing order.

### 4. Simulator parity — `dev/elmer-distill`

Two aligning edits so the sim tool_surface reflects the new shipping capability (parity gate):

- **Align the haversine constant**: `simulator.py` haversine uses `R=6371.0088`; change to
  `6371.0` to match the shipping app (never touch the app's shipped UI number). This removes the
  ~1 km/1000 km drift that could false-fail a citation/replay check.
- **Add `bearing_deg`** to the sim's synthetic gateway records / `find_stations` mock output
  (currently only `distance_km`), computed from the operator grid the same way, so gold-gen
  teaches the surface the app actually ships.
- Regenerate/verify `dev/elmer-distill/reference/tools.json` via `build_tools.py` if the
  curated `find_stations` param/output shape is reflected there.

### 5. Testing (TDD — write failing tests first)

- **Rust `geo.rs` unit tests**: haversine against known grid pairs (include a fixture whose
  expected km is computed from the TS module so the two languages are asserted equal to a
  documented tolerance); bearing cardinal cases (due-N≈0, due-E≈90); `distance_bearing_between_grids`
  null cases (absent grid, malformed grid).
- **`find_stations` impl test** (`mcp_ports.rs`): given an operator grid + gateways with mixed
  present/absent grids, output carries `distance_km`/`bearing_deg`, is sorted ascending, and
  `None` for gateways with absent/invalid grids; and `None` for all when operator grid unresolved.
- **Cross-language parity fixture**: one shared grid pair (e.g. operator `DM43` ↔ `DM33`) with a
  pinned expected km, asserted in both the Rust test and a TS test, so drift is caught.
- **Python sim tests** (`test_simulator_directory.py` / `test_simulator_rich.py`): updated for the
  `6371.0` constant and the presence of `bearing_deg`.

### 6. Construction-site sweep (context for the plan)

Adding fields to `GatewayDto` breaks every construction site. Known sites to update:
- `src-tauri/src/mcp_ports.rs:2009` — the real impl (sets real values).
- `src-tauri/tuxlink-mcp-testserver/src/mocks.rs:532` — testserver mock (set `None` or fixture).
- Any test fixtures constructing `GatewayDto` (grep `GatewayDto {` across the workspace).

## Acceptance

- `find_stations` MCP output includes `distance_km` + `bearing_deg` per gateway, sorted by distance.
- Rust distance == TS UI distance for the shared fixture (numeric parity).
- Sim `find_stations` mock exposes `distance_km` + `bearing_deg` with the `6371.0` constant.
- `cargo test` (mcp-core + src-tauri) and the elmer-distill pytest suite green.
- No `predict_path`, place-label, or UI changes; shipped UI distance constant unchanged.
