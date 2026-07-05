# Design — e7z7d FIX A: `find_stations` distance + bearing on the agent surface

**Issue:** tuxlink-e7z7d (scoped to FIX A only) · **Agent:** kingfisher-cove-yew · **Date:** 2026-07-05
**Branch:** `bd-tuxlink-e7z7d/find-stations-distance-bearing` (off `origin/main` @ f16fec3c)
**Revision:** 2 — folds the 5-round adversarial review (4 Claude lenses + Codex) and operator decisions
(dual units, 6zkb6 = this-session sim work).

## Problem

The in-app Elmer agent cannot compute distance between stations or rank gateways by distance despite
having their grids (operator, 2026-07-01). The distance/bearing capability **already ships in the
human catalog UI** — `src/catalog/StationRail.tsx:78-79` computes `distanceFromGrids` +
`bearingFromGrids` and renders miles (`${distMi} mi`, line 147) — but the **Rust MCP `GatewayDto`
the agent receives carries neither**. The enrichment lives in the TS/UI layer, a different process
from the agent's tool path. That is the gap.

This is a **mirror job**: reproduce the shipping distance/bearing on the agent-facing DTO.

## Scope split (operator 2026-07-05)

e7z7d is **FIX A only**, and lands as **two coordinated halves in this same session**:

- **App-side (this spec, ships to `main` via this branch):** `find_stations` returns
  `distance_km` + `distance_mi` + `bearing_deg`, sorted by distance.
- **Sim-parity (companion commit on `bd-tuxlink-6zkb6/discriminating-eval`):** the distance-capable
  training simulator (`build_gateways`) exists ONLY on that branch, not `origin/main`. The parity
  edit is done there, this session, so the retrain (tuxlink-48nyh) teaches the shipping surface.
  See "Sim-parity" below. This is NOT a wait on an external branch — 6zkb6 is this session's own
  distillation work.

**Out of scope — non-goals (do NOT):**
- **No place-name labels / reverse geocode** — tuxlink-atnsu.
- **No `predict_path` changes** — FIX B split to tuxlink-0lawk, gated on FT-8 (tuxlink-u3m0g.2).
- **No global units toggle** — tuxlink-25l40 (product-wide). e7z7d serves BOTH units unconditionally
  so it does not depend on that setting.
- **Do NOT edit the FROZEN reference harness** (`dev/elmer-distill/reference/harness.py`) — its
  hardcoded `STATIONS` distances are frozen baseline fixtures, deliberately not grid-derived.
- **Do NOT change the app's shipped UI distance path** (`src/catalog/`, `src/forms/position/`).

## Design — App side

### 1. New Rust geo helper — `src-tauri/src/position/geo.rs`

Sibling to `maidenhead.rs`. Mirrors the **clamped** shipping algorithm.

```rust
const EARTH_RADIUS_KM: f64 = 6371.0;   // matches shipping TS (src/catalog/distance.ts:13)
const KM_TO_MI: f64 = 0.621371;        // matches src/catalog/distance.ts:33 kmToMi

/// Great-circle distance in km (haversine). CLAMP the root argument to [0,1] (mirror
/// src/catalog/distance.ts:23 `Math.min(1, ...)`) — the un-clamped src/forms/position
/// variant can yield NaN near-antipodal; use the clamped form.
pub fn haversine_km(a: (f64, f64), b: (f64, f64)) -> f64;

/// Great-circle initial bearing a→b, degrees in [0,360) (0=N, 90=E, cw). Standard atan2
/// azimuth. Tuple contract is (lat, lon) — MUST match grid_to_lat_lon's return order.
pub fn bearing_deg(a: (f64, f64), b: (f64, f64)) -> f64;

/// Distance (km) + bearing (deg) between two Maidenhead grids (each → square center via
/// maidenhead::grid_to_lat_lon). Returns None if EITHER grid is absent/malformed.
/// Bearing is None when distance == 0.0 (co-located / same square) — atan2(0,0)=0 would
/// otherwise read as a spurious due-North. Returns (distance_km, Option<bearing_deg>).
pub fn distance_bearing_between_grids(a: Option<&str>, b: Option<&str>)
    -> Option<(f64, Option<f64>)>;

pub fn km_to_mi(km: f64) -> f64;   // km * KM_TO_MI
```

- Tuple contract is **`(lat, lon)`** everywhere — `grid_to_lat_lon` returns `Some((lat, lon))`
  (`maidenhead.rs:31`); a lat/lon swap is the highest-probability implementer bug (see Testing).
- Registered in `src-tauri/src/position/mod.rs`.

### 2. `GatewayDto` extension — `src-tauri/tuxlink-mcp-core/src/ports.rs:359`

```rust
/// Great-circle distance from the operator's grid to this gateway, in km. None when the
/// gateway grid is absent/invalid OR the operator grid is unresolved.
pub distance_km: Option<f64>,
/// Same distance in statute miles (km * 0.621371). Served alongside km so the agent never
/// does unit math (audience is largely US / miles-preferred; see tuxlink-25l40). None with distance_km.
pub distance_mi: Option<f64>,
/// Great-circle initial bearing degrees [0,360) from the operator to this gateway. None when
/// distance is unknown OR zero (co-located). Asymmetry vs the non-Option bearing_deg on the
/// sibling PathPredictionDto (ports.rs:415) is intentional: gateway grids can be absent.
pub bearing_deg: Option<f64>,
```

- **Always serialized** (no `skip_serializing_if`) — present-but-null teaches the model the field
  exists and is currently unknowable (groundable); missing cannot be distinguished from
  "tool can't do it." These DTOs do NOT derive `schemars::JsonSchema` (verified: only input param
  structs do), so no *advertised* output schema changes — but see §5 (docs) so the model learns them.

### 3. `find_stations` impl — `src-tauri/src/mcp_ports.rs` (`curate_gateway` @ :1985, called from `find_stations` @ :2009)

1. **Resolve operator grid ONCE** per call, before the gateway loop, via a new
   `resolve_operator_grid(&self) -> Option<String>` extracted from the predict_path pattern
   (`mcp_ports.rs:2178-2187`). It MUST:
   - use `effective_broadcast_locator(&cfg, Some(arbiter))` then `broadcast_grid(.., FourCharGrid)`
     — the **4-char** broadcast grid, consistent with predict_path/position_status privacy posture
     (the agent surface is a privacy boundary; a malicious agent must not extract fine location).
     NOTE: this deliberately does NOT match the UI's full-precision grid, so distances are
     4-char-square-center based — the same basis predict_path's distance already uses.
   - **swallow `read_config()` errors to `None`** (config unreadable → distances null, tool still
     returns gateways) — do NOT propagate `PortError`; find_stations must never fail on grid resolution.
   - **map `""` → `None`** (`effective_broadcast_locator` returns `String`, empty when unresolved).
   - `log::debug!` once when the operator grid is unresolved so the all-null degraded state is diagnosable.
2. For each gateway: `distance_bearing_between_grids(operator_grid.as_deref(), gateway.grid.as_deref())`
   → set `distance_km`, `distance_mi` (`km_to_mi`), `bearing_deg`. All `None` when either grid absent.
3. **Sort** gateways ascending by `distance_km` using **stable `sort_by`** with
   `a.partial_cmp(b).unwrap_or(Ordering::Equal)` and **`None` → `Ordering::Greater`** (unknown last).
   Stable sort preserves existing order for ties AND for the all-`None` degraded case (invariant:
   haversine post-clamp never yields NaN, so no NaN poisons the compare).
4. **Echo the resolved operator grid** on `StationListDto` (`ports.rs:376`) as
   `operator_grid: Option<String>` (mirrors the existing `fetched_at_ms` provenance pattern). Lets the
   agent explain *why* all distances are null ("your grid isn't set") instead of looking broken.

### 4. Construction-site sweep (all THREE `GatewayDto` literals must set the new fields)

- `src-tauri/src/mcp_ports.rs:1985` — real impl (`curate_gateway`): sets computed values.
- `src-tauri/tuxlink-mcp-testserver/src/mocks.rs:534` — testserver mock: `distance_km: None, distance_mi: None, bearing_deg: None`.
- `src-tauri/tuxlink-mcp-core/src/lib.rs:651` — mcp-core mock (`MockStation`): same `None` triple.
  **This site is a compile-breaker if missed** — the first spec revision omitted it.
- `StationListDto` construction sites also gain `operator_grid` (real impl: resolved grid; mocks: `None`).

### 5. Discoverability — tool description + agent docs

- Update the `find_stations` `#[tool(description=...)]` in `router.rs:386` to state each gateway
  carries `distance_km`/`distance_mi`/`bearing_deg` from the operator's grid, distance-sorted, null
  when the operator grid is unset. (The model learns the OUTPUT shape from description + examples,
  not from a schema — these DTOs aren't `JsonSchema`.)
- Note the new fields in `docs/mcp-knowledge/agents-guide.md` if it enumerates find_stations output.

## Design — Sim parity (companion commit on `bd-tuxlink-6zkb6/discriminating-eval`)

Done this session on the 6zkb6 branch (where the distance-capable `simulator.py` `build_gateways`
lives). Edits target `dev/elmer-distill/src/elmer_distill/simulator.py`, NOT the frozen `harness.py`.

- **Add `bearing_deg`** to each synthetic gateway in `build_gateways`, computed from `_OPERATOR_GRID`
  via the **same `grid_to_lat_lon` square-center basis** and the **same atan2 convention (0=N, cw)**
  as the Rust `bearing_deg`, `None` at zero distance. `_public_gateway` (simulator.py:285) passes it
  through automatically.
- **Add `distance_mi`** alongside `distance_km` so the sim mirrors the dual-unit surface.
- **Constant:** the sim already uses `R≈6371.0088`; align to `6371.0` for exact-match parity. This is
  cosmetic (~1.4 ppm; `distance_km` is `round()`ed) — NOT a determinism fix (seeds are SHA-256,
  constant-independent; judge re-derives the directory live). State it as cosmetic alignment.
- **Float vs int:** the sim stores `round(dist)` (integer). Decide once: round the Rust agent DTO
  distances too (matches the UI's rounded miles) OR drop the sim's `round()`. **Chosen: round in the
  sim stays; Rust serves full `f64`** — the sim teaches *format/capability*, not memorized values
  (un-memorizable directory is the whole point of 74at8), so integer-vs-float gold is acceptable;
  do NOT round the Rust DTO (callers/UI can round for display).
- `tools.json` / `build_tools.py` need **NO regen** — they curate INPUT schemas only; distance is output.
- Add sim tests: bearing cardinal cases + a Rust↔Python bearing parity fixture (bearing has no other
  cross-impl guard).

## Testing (TDD — failing test first)

- **`geo.rs` unit tests:** haversine known pairs; **zero-distance = 0.0 not NaN**; antimeridian /
  near-antipodal (clamped, no NaN); bearing cardinal cases (due-N≈0, due-E≈90); **bearing None at
  zero distance**; `distance_bearing_between_grids` null cases (absent/malformed grid).
- **Cross-language parity:** feed the **same grid strings** to Rust `haversine_km` and TS `haversineKm`
  and assert `|Δ| < 1e-6` km (tests the algorithm, precision-agnostic; absorbs last-ULP libm drift).
  Use a fixture differing in **both** lat and lon (e.g. `DM43`↔`EM19`) so a `(lat,lon)`↔`(lon,lat)`
  swap changes the km materially. Sanity-check against `src-tauri/tests/propagation_live.rs:87`
  (DM43→DM34 ≈ 215.2 km / 301.65°, 4-char-based, VOACAP great-circle) within a loose tolerance.
- **`find_stations` impl test** over the REAL `MonolithStationPort` (mirror `router.rs:1680`), seeded
  operator grid + mixed present/absent gateway grids: asserts dual-unit + bearing enrichment, ascending
  sort, `None` for absent grids, and **all-`None` preserves input order** when operator grid unresolved.
- **Python sim tests:** dual-unit + `bearing_deg` presence, `6371.0` constant, bearing cardinal + parity fixture.

## Acceptance

- `find_stations` output: per gateway `distance_km` + `distance_mi` + `bearing_deg`, distance-sorted,
  `None` when grid/operator-grid absent; `StationListDto.operator_grid` echoed.
- Rust haversine == TS haversine for identical grid inputs (|Δ|<1e-6 km).
- `cargo test` (mcp-core + src-tauri) green; all three GatewayDto sites compile.
- Sim (on 6zkb6) exposes `distance_km`+`distance_mi`+`bearing_deg` with `6371.0`; sim tests green.
- No `predict_path`, place-label, global-units, UI, or frozen-harness changes.
