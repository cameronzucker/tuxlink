# find_stations Distance + Bearing (e7z7d FIX A) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The MCP `find_stations` tool returns per-gateway `distance_km` + `distance_mi` + `bearing_deg` from the operator's grid, distance-sorted, so the Elmer agent can rank/answer distance questions it currently can't.

**Architecture:** A new `position/geo.rs` mirrors the shipping TS haversine (R=6371, clamped) + adds a great-circle bearing. The `find_stations` impl resolves the operator's 4-char grid once and enriches each `GatewayDto`. A companion commit on the `bd-tuxlink-6zkb6` branch mirrors the same surface into the distillation simulator so the retrain teaches the shipping shape.

**Tech Stack:** Rust (`src-tauri`, `tuxlink-mcp-core`), TypeScript (vitest parity test), Python (elmer-distill sim).

## Global Constraints

- Spec of record: `docs/superpowers/specs/2026-07-05-e7z7d-find-stations-distance-bearing-design.md` (rev2).
- `EARTH_RADIUS_KM = 6371.0`, `KM_TO_MI = 0.621371` — copy verbatim from `src/catalog/distance.ts`.
- Tuple order is **`(lat, lon)`** everywhere (matches `maidenhead::grid_to_lat_lon` → `Some((lat, lon))`).
- Operator grid is resolved to **4-char** (privacy; matches predict_path/position_status). `find_stations` MUST NEVER fail on grid resolution — config errors and empty grid both degrade to `None`.
- New DTO fields are **always serialized** (no `skip_serializing_if`); `None` when unknown.
- Commit trailer on EVERY commit: `Agent: kingfisher-cove-yew` + `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`. Conventional-commit types.
- Worktree cwd reverts between shells — every `git`/`cargo` command must `cd` into the worktree root first (`/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-e7z7d-find-stations-distance-bearing`) and verify `pwd`.
- Do NOT touch: `predict_path`, `src/catalog/` or `src/forms/position/` UI distance paths, the frozen `dev/elmer-distill/reference/harness.py`, any global-units work (tuxlink-25l40).
- **Canonical parity fixture** (hand-derived, matches `src-tauri/tests/propagation_live.rs:87`): `DM43` center `(33.5, -111.0)`, `DM34` center `(34.5, -113.0)` → haversine `≈ 215.28 km`, bearing `≈ 301.5°`.

---

## Deliverable A — App side (branch `bd-tuxlink-e7z7d/find-stations-distance-bearing` → main)

### Task 1: `position/geo.rs` — haversine + bearing + grid helper

**Files:**
- Create: `src-tauri/src/position/geo.rs`
- Modify: `src-tauri/src/position/mod.rs` (add `pub mod geo;`)
- Test: inline `#[cfg(test)]` in `geo.rs`

**Interfaces:**
- Produces: `haversine_km(a: (f64,f64), b: (f64,f64)) -> f64`; `bearing_deg(a: (f64,f64), b: (f64,f64)) -> f64`; `distance_bearing_between_grids(a: Option<&str>, b: Option<&str>) -> Option<(f64, Option<f64>)>`; `km_to_mi(km: f64) -> f64`.
- Consumes: `super::maidenhead::grid_to_lat_lon(&str) -> Option<(f64,f64)>`.

- [ ] **Step 1: Write the failing tests**

```rust
// in src-tauri/src/position/geo.rs
#[cfg(test)]
mod tests {
    use super::*;

    // Hand-derived + cross-checked against propagation_live.rs:87 (VOACAP 215.2km / 301.65°).
    const DM43: (f64, f64) = (33.5, -111.0);
    const DM34: (f64, f64) = (34.5, -113.0);

    #[test]
    fn haversine_matches_shipping_fixture() {
        let km = haversine_km(DM43, DM34);
        assert!((km - 215.28).abs() < 0.5, "DM43->DM34 haversine {km} != ~215.28");
    }

    #[test]
    fn haversine_identical_points_is_zero_not_nan() {
        let km = haversine_km(DM43, DM43);
        assert_eq!(km, 0.0);
    }

    #[test]
    fn haversine_antipodal_no_nan() {
        // near-antipodal: clamp must prevent asin domain overflow
        let km = haversine_km((0.0, 0.0), (0.0, 179.9999999));
        assert!(km.is_finite(), "antipodal produced non-finite {km}");
    }

    #[test]
    fn bearing_cardinals() {
        // due north: same lon, higher lat -> ~0
        assert!(bearing_deg((0.0, 0.0), (1.0, 0.0)).abs() < 1e-6);
        // due east: same lat, higher lon -> ~90
        assert!((bearing_deg((0.0, 0.0), (0.0, 1.0)) - 90.0).abs() < 1e-6);
        // range is [0,360)
        let b = bearing_deg((0.0, 0.0), (-1.0, 0.0)); // due south -> 180
        assert!((b - 180.0).abs() < 1e-6);
    }

    #[test]
    fn bearing_fixture() {
        assert!((bearing_deg(DM43, DM34) - 301.5).abs() < 1.0);
    }

    #[test]
    fn grids_distance_and_bearing() {
        let (km, brg) = distance_bearing_between_grids(Some("DM43"), Some("DM34")).unwrap();
        assert!((km - 215.28).abs() < 0.5);
        assert!((brg.unwrap() - 301.5).abs() < 1.0);
    }

    #[test]
    fn grids_zero_distance_bearing_is_none() {
        let (km, brg) = distance_bearing_between_grids(Some("DM43"), Some("DM43")).unwrap();
        assert_eq!(km, 0.0);
        assert_eq!(brg, None); // co-located: no spurious due-North
    }

    #[test]
    fn grids_absent_or_malformed_is_none() {
        assert_eq!(distance_bearing_between_grids(None, Some("DM43")), None);
        assert_eq!(distance_bearing_between_grids(Some("DM43"), None), None);
        assert_eq!(distance_bearing_between_grids(Some("ZZ99"), Some("DM43")), None);
    }

    #[test]
    fn km_to_mi_conversion() {
        assert!((km_to_mi(100.0) - 62.1371).abs() < 1e-4);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-e7z7d-find-stations-distance-bearing && cargo test -p tuxlink --lib position::geo 2>&1 | tail -20`
Expected: FAIL / does not compile (`geo` module missing).

- [ ] **Step 3: Implement `geo.rs`**

```rust
//! Great-circle distance + bearing over geographic points and Maidenhead grids.
//! Mirrors the shipping catalog UI distance (`src/catalog/distance.ts`, R=6371, clamped
//! haversine) so the agent surface and the human catalog report the same kilometers.

use super::maidenhead::grid_to_lat_lon;

const EARTH_RADIUS_KM: f64 = 6371.0;
const KM_TO_MI: f64 = 0.621371;

/// Great-circle distance in km between two `(lat, lon)` points (degrees), haversine.
/// The root argument is clamped to `<= 1.0` (mirror `src/catalog/distance.ts:23`) so
/// near-antipodal float error cannot push `asin` out of its domain and yield `NaN`.
pub fn haversine_km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (lat1, lon1) = a;
    let (lat2, lon2) = b;
    let d_lat = (lat2 - lat1).to_radians();
    let d_lon = (lon2 - lon1).to_radians();
    let h = (d_lat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (d_lon / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_KM * h.sqrt().min(1.0).asin()
}

/// Initial great-circle bearing from `a` to `b` in degrees `[0, 360)` (0=N, 90=E, clockwise).
pub fn bearing_deg(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (lat1, lon1) = a;
    let (lat2, lon2) = b;
    let r1 = lat1.to_radians();
    let r2 = lat2.to_radians();
    let d_lon = (lon2 - lon1).to_radians();
    let y = d_lon.sin() * r2.cos();
    let x = r1.cos() * r2.sin() - r1.sin() * r2.cos() * d_lon.cos();
    (y.atan2(x).to_degrees() + 360.0) % 360.0
}

/// Distance (km) + optional bearing (deg) between two Maidenhead grids, each taken at its
/// square center. `None` if either grid is absent or malformed. Bearing is `None` when the
/// distance is exactly 0 (co-located / identical square) — `atan2(0,0)=0` would otherwise
/// read as a spurious due-North.
pub fn distance_bearing_between_grids(
    a: Option<&str>,
    b: Option<&str>,
) -> Option<(f64, Option<f64>)> {
    let ga = grid_to_lat_lon(a?)?;
    let gb = grid_to_lat_lon(b?)?;
    let km = haversine_km(ga, gb);
    let bearing = if km == 0.0 { None } else { Some(bearing_deg(ga, gb)) };
    Some((km, bearing))
}

/// Kilometers to statute miles (matches `src/catalog/distance.ts:33` `kmToMi`).
pub fn km_to_mi(km: f64) -> f64 {
    km * KM_TO_MI
}
```

Then add to `src-tauri/src/position/mod.rs` (alphabetical among the existing `pub mod` lines):

```rust
pub mod geo;
```

- [ ] **Step 4: Run to verify pass**

Run: `cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-e7z7d-find-stations-distance-bearing && cargo test -p tuxlink --lib position::geo 2>&1 | tail -20`
Expected: PASS (10 tests).

- [ ] **Step 5: Commit**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-e7z7d-find-stations-distance-bearing
git add src-tauri/src/position/geo.rs src-tauri/src/position/mod.rs
git commit -m "$(cat <<'EOF'
feat(position): great-circle geo helper (haversine + bearing) for station distance

Mirrors the shipping TS catalog haversine (R=6371, clamped) and adds a
great-circle bearing, plus grid-pair distance/bearing with None-at-zero-distance.
Substrate for find_stations distance enrichment (tuxlink-e7z7d).

Agent: kingfisher-cove-yew
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: TypeScript parity guard (Rust == TS haversine)

**Files:**
- Test: `src/catalog/distance.parity.test.ts` (new)

**Interfaces:**
- Consumes: `distanceFromGrids` from `src/catalog/distance.ts`.

- [ ] **Step 1: Write the parity test**

```ts
import { describe, it, expect } from 'vitest';
import { distanceFromGrids } from './distance';

// Parity anchor shared with the Rust geo.rs test (haversine_matches_shipping_fixture)
// and src-tauri/tests/propagation_live.rs:87. If the Rust and TS haversines diverge,
// one of these two assertions moves off 215.28 and the mismatch is caught in CI.
describe('haversine cross-language parity', () => {
  it('DM43->DM34 matches the shared 215.28 km fixture', () => {
    const km = distanceFromGrids('DM43', 'DM34');
    expect(km).not.toBeNull();
    expect(Math.abs((km as number) - 215.28)).toBeLessThan(0.5);
  });
});
```

- [ ] **Step 2: Run to verify pass** (TS is already correct; this pins the shared constant)

Run: `cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-e7z7d-find-stations-distance-bearing && pnpm vitest run src/catalog/distance.parity.test.ts 2>&1 | tail -15`
Expected: PASS. (If it FAILS, the fixture math is wrong — STOP and recompute before proceeding.)

- [ ] **Step 3: Commit**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-e7z7d-find-stations-distance-bearing
git add src/catalog/distance.parity.test.ts
git commit -m "$(cat <<'EOF'
test(catalog): pin Rust<->TS haversine parity fixture (DM43->DM34 = 215.28 km)

Agent: kingfisher-cove-yew
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: Extend `GatewayDto` + `StationListDto` (all construction sites, compile-safe)

**Files:**
- Modify: `src-tauri/tuxlink-mcp-core/src/ports.rs` (`GatewayDto` @ 359, `StationListDto` @ 376)
- Modify: `src-tauri/src/mcp_ports.rs` (`GatewayDto` literal in `curate_gateway` @ ~1985; `StationListDto` construction in `find_stations` @ ~2009)
- Modify: `src-tauri/tuxlink-mcp-testserver/src/mocks.rs` (`GatewayDto` @ ~534)
- Modify: `src-tauri/tuxlink-mcp-core/src/lib.rs` (`GatewayDto` in `MockStation` @ ~651)

**Interfaces:**
- Produces: `GatewayDto.distance_km: Option<f64>`, `.distance_mi: Option<f64>`, `.bearing_deg: Option<f64>`; `StationListDto.operator_grid: Option<String>`.

- [ ] **Step 1: Add fields to the DTOs**

In `ports.rs`, inside `pub struct GatewayDto { ... }` (after the `antenna` field):

```rust
    /// Great-circle distance in km from the operator's grid to this gateway. `None` when the
    /// gateway grid is absent/invalid OR the operator grid is unresolved.
    pub distance_km: Option<f64>,
    /// Same distance in statute miles (km * 0.621371). Served alongside km so the agent never
    /// does unit math (US/miles-preferred audience; global toggle tracked in tuxlink-25l40).
    pub distance_mi: Option<f64>,
    /// Great-circle initial bearing in degrees [0,360) from the operator to this gateway.
    /// `None` when distance is unknown OR zero. (Sibling PathPredictionDto's bearing_deg is
    /// non-optional; the asymmetry is intentional — gateway grids can be absent.)
    pub bearing_deg: Option<f64>,
```

In `ports.rs`, inside `pub struct StationListDto { ... }` (after `fetched_at_ms`):

```rust
    /// The operator's own 4-char grid used to compute per-gateway distances (provenance).
    /// `None` when unresolved — lets the agent explain why all distances are null.
    pub operator_grid: Option<String>,
```

- [ ] **Step 2: Set the new fields at all four construction sites (temporary `None`)**

`mcp_ports.rs` `curate_gateway` (~1985), inside the `GatewayDto { ... }` literal — add (real values wired in Task 4):

```rust
                distance_km: None,
                distance_mi: None,
                bearing_deg: None,
```

`mcp_ports.rs` `find_stations` (~2009), the `StationListDto { ... }` literal — add:

```rust
            operator_grid: None,
```

`mocks.rs` (~534) `GatewayDto { ... }` — add the same three `None` lines; if that mock builds a `StationListDto`, add `operator_grid: None`.

`lib.rs` (~651) `GatewayDto { ... }` in `MockStation` — add the same three `None` lines. **(Missing this site is a hard compile error.)**

- [ ] **Step 3: Verify workspace compiles + existing tests green**

Run: `cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-e7z7d-find-stations-distance-bearing && cargo test -p tuxlink-mcp-core -p tuxlink --lib 2>&1 | tail -25`
Expected: PASS (no test asserts on the new fields yet; all existing green). If a whole-struct equality assertion fails, set the same `None` values on both compared literals.

- [ ] **Step 4: Commit**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-e7z7d-find-stations-distance-bearing
git add src-tauri/tuxlink-mcp-core/src/ports.rs src-tauri/src/mcp_ports.rs \
        src-tauri/tuxlink-mcp-testserver/src/mocks.rs src-tauri/tuxlink-mcp-core/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(mcp): add distance_km/distance_mi/bearing_deg to GatewayDto + operator_grid to StationListDto

Fields default None at all four construction sites (real impl, core mock, testserver
mock, mcp-core MockStation); enrichment wired next. (tuxlink-e7z7d)

Agent: kingfisher-cove-yew
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: Wire enrichment — resolve operator grid, compute, sort

**Files:**
- Modify: `src-tauri/src/mcp_ports.rs`:
  - `curate_gateway` FREE fn @ 1973 (`fn curate_gateway(mode: StationModeDto, g: &Gateway) -> Option<GatewayDto>`) — add an `operator_grid: Option<&str>` param and enrich using its local `grid: Option<String>`.
  - `impl MonolithStationPort` block @ 2001 — add `resolve_operator_grid(&self)` (the struct is `MonolithStationPort { app: AppHandle }`).
  - `impl StationPort for MonolithStationPort::find_stations` @ 2008 — resolve once, thread through, sort, echo.
  - **Existing `curate_gateway` tests @ 2514, 2530, 2540** — update each call to pass the new `operator_grid` arg (`None`, or `Some("DM43")` where the test wants distances).
- Test: `src-tauri/src/mcp_ports.rs` `#[cfg(test)]` (mirror the existing `find_stations` test at `tuxlink-mcp-core/src/router.rs:1680`)

**Interfaces:**
- Consumes: `crate::position::geo::{distance_bearing_between_grids, km_to_mi}`; the predict_path grid-resolution pattern at `mcp_ports.rs:2178-2187`.
- Produces: enriched, distance-sorted `StationListDto` with `operator_grid` echo.

- [ ] **Step 1: Write the failing impl test**

Add a test that drives the REAL stations port. Read the existing `find_stations` impl + the `router.rs:1680` test first to match the harness (AppHandle/state setup). The assertions:

```rust
// Pseudocode-shaped but concrete: adapt to the real port constructor used at router.rs:1680.
#[tokio::test]
async fn find_stations_enriches_distance_bearing_and_sorts() {
    // operator grid resolves to DM43 (seed config/arbiter as the router.rs:1680 test does).
    // gateways: A grid "DM34" (~215km), B grid "CN87" (far), C grid None.
    let list = /* call the real find_stations impl with a filter matching all three */;
    // sorted ascending by distance, None last:
    assert_eq!(list.gateways[0].callsign, /* nearest (DM34) */);
    assert!(list.gateways[0].distance_km.unwrap() > 0.0);
    assert!((list.gateways[0].distance_mi.unwrap()
             - list.gateways[0].distance_km.unwrap() * 0.621371).abs() < 1e-6);
    assert!(list.gateways[0].bearing_deg.is_some());
    assert_eq!(list.gateways.last().unwrap().distance_km, None); // grid-None sorts last
    assert_eq!(list.operator_grid.as_deref(), Some("DM43"));
}

#[tokio::test]
async fn find_stations_unresolved_operator_grid_all_none_preserves_order() {
    // seed NO operator grid -> resolve_operator_grid returns None.
    let list = /* call find_stations */;
    assert!(list.gateways.iter().all(|g| g.distance_km.is_none()));
    assert_eq!(list.operator_grid, None);
    // original listing order preserved (stable sort, all-None):
    assert_eq!(list.gateways.iter().map(|g| g.callsign.clone()).collect::<Vec<_>>(),
               /* the unsorted input order */);
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-e7z7d-find-stations-distance-bearing && cargo test -p tuxlink --lib mcp_ports::tests::find_stations 2>&1 | tail -20`
Expected: FAIL (distances still `None`; `operator_grid` still `None`).

- [ ] **Step 3: Implement `resolve_operator_grid` + enrichment + sort**

Add to the same `impl` block as `find_stations` (mirror predict_path's pattern at 2178-2187, but NEVER error):

```rust
    /// Resolve the operator's own 4-char broadcast grid for local distance ranking.
    /// NEVER errors — config-read failure and an empty/unresolved grid both degrade to
    /// `None` so `find_stations` still returns gateways (with null distances).
    fn resolve_operator_grid(&self) -> Option<String> {
        use crate::config::PositionPrecision;
        let arbiter_state = self.app.state::<Arc<crate::position::PositionArbiter>>();
        let arbiter: &crate::position::PositionArbiter = &arbiter_state;
        let cfg = crate::config::read_config().ok()?; // unreadable -> None, not an error
        let raw = crate::position::effective_broadcast_locator(&cfg, Some(arbiter));
        let grid = crate::config::broadcast_grid(&raw, PositionPrecision::FourCharGrid);
        if grid.is_empty() {
            log::debug!("find_stations: operator grid unresolved; distances will be null");
            None
        } else {
            Some(grid)
        }
    }
```

Change the free fn `curate_gateway` (1973) to accept `operator_grid: Option<&str>` and set the fields (replace the three `None`s from Task 3). Then update its three existing callers/tests (2514, 2530, 2540) to pass the new arg (`None` where they don't assert distance). The `grid` local is the `Option<String>` computed at the top of the fn:

```rust
    // signature: add `operator_grid: Option<&str>` param
    // inside, before building the DTO:
    let (distance_km, distance_mi, bearing_deg) =
        match crate::position::geo::distance_bearing_between_grids(operator_grid, grid.as_deref()) {
            Some((km, brg)) => (Some(km), Some(crate::position::geo::km_to_mi(km)), brg),
            None => (None, None, None),
        };
    // ... GatewayDto { ..., distance_km, distance_mi, bearing_deg }
```

In `find_stations`, resolve once, thread it through, then sort and echo:

```rust
    let operator_grid = self.resolve_operator_grid();
    // pass operator_grid.as_deref() into each curate_gateway(...) call
    let mut gateways: Vec<GatewayDto> = /* existing mapping, now passing operator_grid.as_deref() */;
    gateways.sort_by(|a, b| match (a.distance_km, b.distance_km) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    // StationListDto { gateways, fetched_at_ms, operator_grid }
```

- [ ] **Step 4: Run to verify pass**

Run: `cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-e7z7d-find-stations-distance-bearing && cargo test -p tuxlink --lib mcp_ports 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Full workspace test + clippy**

Run: `cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-e7z7d-find-stations-distance-bearing && cargo test -p tuxlink -p tuxlink-mcp-core 2>&1 | tail -15 && cargo clippy -p tuxlink -p tuxlink-mcp-core --all-targets 2>&1 | tail -15`
Expected: tests PASS, clippy clean (CI runs `-D warnings`).

- [ ] **Step 6: Commit**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-e7z7d-find-stations-distance-bearing
git add src-tauri/src/mcp_ports.rs
git commit -m "$(cat <<'EOF'
feat(mcp): find_stations returns distance/bearing from the operator grid, sorted

Resolve the operator's 4-char grid once (never erroring), enrich each gateway with
distance_km/distance_mi/bearing_deg via position::geo, stable-sort ascending with
unknowns last, and echo operator_grid on StationListDto. (tuxlink-e7z7d)

Agent: kingfisher-cove-yew
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: Tool description + agent docs

**Files:**
- Modify: `src-tauri/tuxlink-mcp-core/src/router.rs` (`find_stations` `#[tool(description = ...)]` @ ~386)
- Modify: `docs/mcp-knowledge/agents-guide.md` (if it enumerates `find_stations` output; otherwise skip)

- [ ] **Step 1: Update the tool description**

Extend the `find_stations` `#[tool(description = "...")]` to state: "Each gateway includes `distance_km`, `distance_mi`, and `bearing_deg` from the operator's grid (null when the operator grid is unset); results are sorted nearest-first." Keep existing wording; append this sentence.

- [ ] **Step 2: Update agents-guide (only if it documents find_stations output)**

Run: `cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-e7z7d-find-stations-distance-bearing && grep -n "find_stations" docs/mcp-knowledge/agents-guide.md`
If present, add the three new fields to that section. If absent, skip (no placeholder edit).

- [ ] **Step 3: Verify build (description is compile-checked)**

Run: `cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-e7z7d-find-stations-distance-bearing && cargo check -p tuxlink-mcp-core 2>&1 | tail -8`
Expected: OK.

- [ ] **Step 4: Commit**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-e7z7d-find-stations-distance-bearing
git add src-tauri/tuxlink-mcp-core/src/router.rs docs/mcp-knowledge/agents-guide.md 2>/dev/null
git commit -m "$(cat <<'EOF'
docs(mcp): advertise find_stations distance/bearing output fields to the agent

Agent: kingfisher-cove-yew
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 6: Review loop (app side)

- [ ] After Tasks 1-5: review the batch from multiple perspectives. Minimum three review rounds; if the third still finds substantive issues, keep going. Check against `docs/pitfalls/testing-pitfalls.md` and `docs/pitfalls/implementation-pitfalls.md` (esp. the composed-seam entry — confirm the Task 4 test drives the REAL port, not a reimplementation). Then push and open the PR to `main`.

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-e7z7d-find-stations-distance-bearing
git push -u origin bd-tuxlink-e7z7d/find-stations-distance-bearing
gh pr create --base main --head bd-tuxlink-e7z7d/find-stations-distance-bearing \
  --title "[kingfisher-cove-yew] find_stations distance+bearing for the Elmer agent (tuxlink-e7z7d FIX A)" \
  --body "..."
```

---

## Deliverable B — Sim parity (branch `bd-tuxlink-6zkb6/discriminating-eval`, SEPARATE worktree)

> **Execute ONLY after standing up a worktree off `bd-tuxlink-6zkb6/discriminating-eval`** (the distance-capable `simulator.py` exists only there). Do NOT run these steps in the app-side worktree. Bind them to tuxlink-6zkb6.

### Task 7: Mirror distance_mi + bearing_deg into the simulator

**Files:**
- Modify: `dev/elmer-distill/src/elmer_distill/simulator.py` (`build_gateways`, the haversine constant, `_public_gateway`)
- Test: `dev/elmer-distill/tests/test_simulator_directory.py`
- **Do NOT touch** `dev/elmer-distill/reference/harness.py` (frozen baseline).

- [ ] **Step 1: Write failing sim tests** — assert each synthetic gateway carries `distance_mi ≈ distance_km * 0.621371` and a `bearing_deg` in `[0,360)` or `None` (zero distance); assert the module constant is `6371.0`; add a cardinal bearing case and a Rust↔Python fixture (`DM43`→`DM34` bearing ≈ 301.5, distance ≈ 215).

- [ ] **Step 2: Run to verify failure.** `cd <6zkb6-worktree>/dev/elmer-distill && python -m pytest tests/test_simulator_directory.py -x 2>&1 | tail -20`

- [ ] **Step 3: Implement** — in `simulator.py`: change the haversine radius to `6371.0` (cosmetic parity, NOT a determinism change — note it in a comment); add a `bearing_deg` helper mirroring the Rust atan2 convention (0=N, cw, `None` at zero distance) computed from `_OPERATOR_GRID` via the same `grid_to_lat_lon` center; add `distance_mi = round-consistent(distance_km * 0.621371)`; ensure `_public_gateway` passes both through (it already forwards non-stripped keys).

```python
def _bearing_deg(a, b):  # a,b = (lat, lon); mirror Rust position::geo::bearing_deg
    import math
    r1, r2 = math.radians(a[0]), math.radians(b[0])
    dlon = math.radians(b[1] - a[1])
    y = math.sin(dlon) * math.cos(r2)
    x = math.cos(r1) * math.sin(r2) - math.sin(r1) * math.cos(r2) * math.cos(dlon)
    return (math.degrees(math.atan2(y, x)) + 360.0) % 360.0
```

- [ ] **Step 4: Run to verify pass.** Full sim suite: `python -m pytest tests/ 2>&1 | tail -20`. Expected: PASS (seed determinism unaffected — SHA-256 seeds are constant-independent).

- [ ] **Step 5: Commit** on the 6zkb6 branch (trailer + `Agent: kingfisher-cove-yew`). Push to `origin/bd-tuxlink-6zkb6/discriminating-eval`.

---

## Self-Review (completed by planner)

- **Spec coverage:** geo helper (T1), dual units (T3/T4), bearing incl None-at-zero (T1/T4), 4-char grid + error-swallowing resolve (T4), stable sort None-last (T4), operator_grid echo (T3/T4), 3 construction sites incl lib.rs:651 (T3), clamped haversine (T1), cross-language parity (T1+T2), propagation_live anchor (T1), tool docs (T5), sim parity on 6zkb6 (T7). No spec section unmapped.
- **Placeholder scan:** Task 4's test body is shaped-pseudocode by necessity (the port constructor/state harness must be read from the live `router.rs:1680` test) — the implementer is directed to that exact reference; all assertions are concrete. No `TODO`/`TBD`.
- **Type consistency:** `distance_bearing_between_grids -> Option<(f64, Option<f64>)>` consumed consistently in T4; field names `distance_km`/`distance_mi`/`bearing_deg`/`operator_grid` identical across T3/T4/T5.
