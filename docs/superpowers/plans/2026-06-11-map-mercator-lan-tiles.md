# Map: Web Mercator (EPSG:3857) LAN tiles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**bd:** tuxlink-7h2m · branch `bd-tuxlink-7h2m/mercator-lan-tiles` · worktree `worktrees/bd-tuxlink-7h2m-mercator-lan-tiles/`
**Spec:** [docs/superpowers/specs/2026-06-11-map-mercator-lan-tiles-design.md](../specs/2026-06-11-map-mercator-lan-tiles-design.md)

**Goal:** Switch the tuxlink map subsystem to EPSG:3857 (Web Mercator) so it ingests standard XYZ tiles from a self-hosted/LAN tile server (e.g. Geographica), correcting the wrong dyop spec that forced EPSG:4326-only and rejected Mercator. The LAN-only/SSRF host gatekeeper stays as the only real "no public-OSM-abuse" control; the CRS gate is deleted.

**Architecture:** One CRS everywhere (`L.CRS.EPSG3857`). The bundled offline base becomes a low-zoom Mercator world raster. The Rust tile pipeline keeps its SSRF/LAN host policy untouched, drops CRS validation entirely (a non-image/bad source still fails the reachability probe), and fixes the tile-coordinate x-bound to the WebMercatorQuad convention. The 4 map consumers are wired to pass a validated `tileSource` into `BaseMap` so the zoom cap rises and tiles render.

**Tech Stack:** Rust (Tauri tiles module, `cargo test --manifest-path src-tauri/Cargo.toml`), React 18 + react-leaflet v5 + Leaflet, Vitest (`pnpm vitest run`), `pnpm typecheck` / `pnpm build` / `pnpm lint:docs`, GDAL + Pillow + oxipng for the asset.

---

## Pre-flight (read before Task 1)

- **All commands from the worktree:** `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-7h2m-mercator-lan-tiles/`. `node_modules` is symlinked. Use absolute paths / `pnpm -C <wt>` / `git -C <wt>` — never `cd` to the main checkout. Never put git keywords (HEAD/status/diff/commit/add) inside echo/ls strings (the main-checkout hook substring-matches; in a worktree it's fine but run a standalone `cd <wt>` first if needed).
- **Gates:** Rust `cargo test --manifest-path src-tauri/Cargo.toml`; frontend `pnpm vitest run` (full — CI parity), `pnpm typecheck`, `pnpm build`, `pnpm lint:docs`. Run clippy `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` before any push (memory `scoped_vitest_misses_contract_tests` — CI runs `--all-targets`; re-run till exit 0). Reap stray vitest workers after sweeps (`pgrep -f "node.*vitest"`; `pkill -9` only your own — a concurrent session may be running its own).
- **Render gate (CRITICAL):** any BaseMap/UI change must be grim-verified in real WebKitGTK at 1920×1080 before "done" (memory `reference_webkitgtk_render_harness`, `grim_realapp_validation_pandora`). jsdom/react-leaflet mock CANNOT prove projection/render correctness (the dyop C1 rule).
- **Commit discipline:** `Agent: <moniker>` + `Co-Authored-By:` trailers on every commit; conventional types; push the instant a unit is green (memory `never_hold_a_push`); no `--no-verify`/destructive git.
- **CRS math fact:** under `L.CRS.EPSG3857`, the world at z0 is 256×256 px and doubles per level. A 2048px-wide base raster is 1:1 at **z3** (256·2³). So the raster-native zoom cap changes from 2 (the old 4326 value: 512·2²=2048) to **3**.

---

## File structure

| File | Change | Responsibility |
|---|---|---|
| `src-tauri/src/tiles/coord.rs` | Modify | `TileCoord::new` x-bound `2^(z+1)`→`2^z` (WebMercatorQuad); replace geodetic-count tests. |
| `src-tauri/src/tiles/crs.rs` | **Delete** | The false-accepting CRS probe + `geodetic_tile_index` + ~500 lines of 4326 tests — dead once Mercator is valid. |
| `src-tauri/src/tiles/mod.rs` | Modify | Remove `pub mod crs;`, the `crs: Crs` field, and `enum Crs`. |
| `src-tauri/src/tiles/commands.rs` | Modify | Delete the CRS-probe block in `validate`; drop `Crs`/`probe_source_crs`/`CrsCheck` uses; delete/rewrite the CRS tests; drop `crs` from test `source()`. |
| `src-tauri/src/tiles/serve.rs` | Modify | Drop `crs` from the test `source()` helper (TileCoord call already correct via Task 1). |
| `src/map/tileSource.ts` | Modify | Remove `TileCrs` + the `crs` field from `TileSource`. |
| `src/settings/MapTileSourceSettings.tsx` | Modify | Drop `crs:'Geodetic'` from `buildSource`; invert help text + `incompatible` message to EPSG:3857. |
| `src/map/assets/world-mercator-2048.png` | Create | New Mercator base raster (GDAL-reprojected Natural Earth II). |
| `src/map/assets/world-mercator.CREDITS.md` | Create | Provenance for the new asset. |
| `src/map/projection.ts` | Modify | Add `MERCATOR_BOUNDS` (±85.0511°). Keep `clampLatLon`. (Vestigial pixel helpers may be deleted.) |
| `src/map/BaseMap.tsx` | Modify | `crs={L.CRS.EPSG3857}`; Mercator base raster + bounds; `RASTER_MAX_ZOOM` 2→3; add `onZoomChange?` prop; accept `tileSource`. |
| `src/map/testMapMock.ts` | Modify | Add `EPSG3857` to the CRS mock so BaseMap `data-crs` assertions work. |
| `src/map/useTileSource.ts` | Create | Shared hook: fetch validated source+status for consumers. |
| `src/catalog/StationFinderMap.tsx` | Modify | Pass `tileSource` from `useTileSource`. |
| `src/map/GridMapPicker.tsx` | Modify | Pass `tileSource`. |
| `src/compose/PositionMapWidget.tsx` | Modify | Pass `tileSource`; forward `onZoomChange`. |
| `src/compose/PositionPickerOverlay.tsx` | Modify | Use live zoom (via `onZoomChange`) for `sixCharAllowed`; drop `RASTER_VIEW_ZOOM`. |
| `docs/design/2026-06-08-offline-map-foundation-approach.md`, `docs/plans/2026-06-09-dyop-lan-tiles-plan.md` | Modify | Superseding notes. |

---

## Task 1: Fix `coord.rs` x-bound for WebMercatorQuad

**Files:** Modify `src-tauri/src/tiles/coord.rs`

- [ ] **Step 1: Replace the geodetic-bound tests with Mercator-bound tests (RED).** In the `#[cfg(test)]` module, replace `geodetic_x_bound_is_2_pow_z_plus_1_y_bound_is_2_pow_z` and `geodetic_boundaries_at_higher_zoom` with:
```rust
#[test]
fn mercator_x_and_y_bounds_are_2_pow_z() {
    // WebMercatorQuad: square grid. z0 = 1×1, z1 = 2×2, z6 = 64×64.
    assert!(TileCoord::new(0, 0, 0).is_ok());
    assert!(TileCoord::new(0, 1, 0).is_err()); // x=1 invalid at z0 (was valid under geodetic)
    assert!(TileCoord::new(1, 1, 1).is_ok());
    assert!(TileCoord::new(1, 2, 0).is_err()); // x=2 invalid at z1
    assert!(TileCoord::new(6, 63, 63).is_ok());
    assert!(TileCoord::new(6, 64, 0).is_err());
}
```
(Match `TileCoord::new`'s actual arg order/signature from the file — `new(z, x, y)` per the context pack.)

- [ ] **Step 2: Run — verify FAIL.** Run: `cargo test --manifest-path src-tauri/Cargo.toml tiles::coord` → FAIL (x=1@z0 currently OK under the `2^(z+1)` bound).

- [ ] **Step 3: Flip the x-bound.** In `TileCoord::new`, change the x upper bound from `2^(z+1)` to `2^z`: remove the `checked_add(1)` on the x-shift so x uses `1u32.checked_shl(z)` (same as y). Update the doc-comment + module header ("serves ONLY geodetic … WorldCRS84Quad" → "WebMercatorQuad / standard slippy XYZ; world is 2^z × 2^z").

- [ ] **Step 4: Run — verify PASS.** Run: `cargo test --manifest-path src-tauri/Cargo.toml tiles::coord` → PASS.

- [ ] **Step 5: Commit.**
```bash
git -C <wt> add src-tauri/src/tiles/coord.rs
git -C <wt> commit -m "fix(tiles): TileCoord x-bound to WebMercatorQuad (2^z) for EPSG:3857 (tuxlink-7h2m)\n\nAgent: <moniker>\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

## Task 2: Delete the CRS gate (Rust)

**Files:** Delete `src-tauri/src/tiles/crs.rs`; Modify `mod.rs`, `commands.rs`, `serve.rs`

- [ ] **Step 1: Delete the failing CRS tests + update mocks first (so the suite compiles toward the new shape).** In `commands.rs` `#[cfg(test)]`: delete `validate_mercator_is_incompatible`, `validate_unknown_crs_with_geodetic_override_proceeds`; rewrite `configure_does_not_activate_on_incompatible` to assert a NON-IMAGE source (e.g. a server returning `text/html` for the tile) yields `Incompatible` (the reachability/image probe, not CRS). Remove the `crs: Crs::Geodetic` field from the `source()` test helpers in `commands.rs` AND `serve.rs`.

- [ ] **Step 2: Remove the CRS-probe block from `validate`.** Delete lines ~98–116 (the `match probe_source_crs(...) { Rejected => Incompatible, Unknown => match source.crs {...}, Geodetic => proceed }`). `validate` flows URL-shape check → reachability probe directly.

- [ ] **Step 3: Drop the imports + types.** In `commands.rs`: delete `use super::crs::{probe_source_crs, CrsCheck};` and remove `Crs` from the `use super::{...}` line. In `mod.rs`: delete `pub mod crs;`, the `crs: Crs` field from `TileSource`, and `enum Crs { Geodetic }`. Delete the file `src-tauri/src/tiles/crs.rs`.

- [ ] **Step 4: Run the full Rust suite + clippy.** Run: `cargo test --manifest-path src-tauri/Cargo.toml` then `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`. Both clean. (A 3857-serving source now validates as `lan-live`; a non-image source → `Incompatible`; a public/denied host → `Unreachable` via the untouched `host.rs` gate.)

- [ ] **Step 5: Commit.**
```bash
git -C <wt> rm src-tauri/src/tiles/crs.rs
git -C <wt> add src-tauri/src/tiles/mod.rs src-tauri/src/tiles/commands.rs src-tauri/src/tiles/serve.rs
git -C <wt> commit -m "feat(tiles): delete CRS gate; accept standard tiles, keep LAN/SSRF control (tuxlink-7h2m)\n\nAgent: <moniker>\nCo-Authored-By: ..."
```

## Task 3: Remove `crs` from the TS wire + Settings copy

**Files:** Modify `src/map/tileSource.ts`, `src/settings/MapTileSourceSettings.tsx` (+ its test)

- [ ] **Step 1: Update the failing test (RED).** In `MapTileSourceSettings.test.tsx`, find the test asserting the configured `TileSource` payload / the EPSG:4326 help copy; update it to expect NO `crs` field and the new EPSG:3857 copy. Run the test → FAIL.

- [ ] **Step 2: Implement.** `tileSource.ts`: delete `export type TileCrs = 'Geodetic';` and the `crs: TileCrs;` field from `interface TileSource`. `MapTileSourceSettings.tsx`: remove `crs:'Geodetic',` from `buildSource()`; change the help text (lines ~151–154) to "The source serves standard Web Mercator (EPSG:3857) XYZ tiles, e.g. a self-hosted TileServer GL or OSM-format LAN server." and the `incompatible` message (line ~39) to "incompatible tile source — expected standard Web Mercator (EPSG:3857) image tiles".

- [ ] **Step 3: Run — verify PASS + tsc.** `pnpm vitest run src/settings/MapTileSourceSettings.test.tsx && pnpm tsc --noEmit` → PASS/clean.

- [ ] **Step 4: Commit.** `feat(map): drop crs field from tile source wire; settings copy → EPSG:3857 (tuxlink-7h2m)`

## Task 4: Bundled Mercator base raster (may need GDAL — flag if blocked)

**Files:** Create `src/map/assets/world-mercator-2048.png`, `src/map/assets/world-mercator.CREDITS.md`

> **This task needs GDAL + the public-domain Natural Earth II source.** If `gdalwarp` is absent, installing it is a `sudo apt` system change — STOP and ask the operator (memory `sudo_apt_explicit_approval`); do not install unprompted. The source raster (Natural Earth II 1:50m, "NE2_50M_SR_W") is public domain — the agent may download it.

- [ ] **Step 1: Acquire + reproject.** Download NE2_50M_SR_W (public domain, naturalearthdata.com 50m raster). Reproject to Web Mercator and crop to the Mercator extent:
```bash
gdalwarp -t_srs EPSG:3857 -te -20037508.34 -20037508.34 20037508.34 20037508.34 -ts 2048 2048 -r lanczos NE2_50M_SR_W.tif /tmp/world-mercator.tif
```
(`-te` is the full WebMercatorQuad extent in meters; `-ts 2048 2048` makes a square raster 1:1 at z3.)

- [ ] **Step 2: Encode + optimize** (mirror the existing `world-equirect.CREDITS.md` Pillow+oxipng recipe):
```python
from PIL import Image
im = Image.open("/tmp/world-mercator.tif").convert("RGB").resize((2048, 2048), Image.LANCZOS)
q = im.quantize(colors=256, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.FLOYDSTEINBERG)
q.save("src/map/assets/world-mercator-2048.png", format="PNG", optimize=True)
import oxipng; oxipng.optimize("src/map/assets/world-mercator-2048.png", level=6, strip=oxipng.StripChunks.safe())
```
Verify `ls -lh src/map/assets/world-mercator-2048.png` < 1.5 MB.

- [ ] **Step 3: Write `world-mercator.CREDITS.md`** mirroring the equirect CREDITS (source, license=public domain, the exact gdalwarp + Pillow + oxipng commands, the square 2048×2048 / z3-native note, ±85.0511° clip).

- [ ] **Step 4: Commit.** `feat(map): bundle Mercator (EPSG:3857) world base raster (tuxlink-7h2m)`

## Task 5: `BaseMap` → EPSG:3857

**Files:** Modify `src/map/BaseMap.tsx`, `src/map/projection.ts`, `src/map/testMapMock.ts` (+ BaseMap.test.tsx)

- [ ] **Step 1: Add `MERCATOR_BOUNDS` to projection.ts.** `export const MERCATOR_BOUNDS: [[number, number], [number, number]] = [[-85.0511, -180], [85.0511, 180]];` Keep `clampLatLon` (clamp lat to ±85.0511 now). The unused `pixelToLatLon`/`latLonToPixel` may be deleted (zero live callers) — optional cleanup in this commit.

- [ ] **Step 2: Add `EPSG3857` to the test mock + update BaseMap test (RED).** In `testMapMock.ts` `createLeafletMock()`, add `EPSG3857: { code: 'EPSG:3857' }` to the `CRS` object. In `BaseMap.test.tsx`, change the `data-crs` assertion to `'EPSG:3857'` and the raster-max-zoom assertion to 3. Run → FAIL.

- [ ] **Step 3: Implement BaseMap.** `import worldMercatorPng from './assets/world-mercator-2048.png';` (replace the equirect import); `crs={L.CRS.EPSG3857}`; `maxBounds={MERCATOR_BOUNDS}`; `<ImageOverlay url={worldMercatorPng} bounds={MERCATOR_BOUNDS} />`; `const RASTER_MAX_ZOOM = 3;` (was 2). Add an optional `onZoomChange?: (zoom: number) => void;` prop to `BaseMapProps` and a small `useMapEvents({ zoomend(e){ onZoomChange?.(e.target.getZoom()); } })` bridge component (mirrors `MapClickHandler`). Keep the `tileSource`-raises-maxZoom logic (now relative to RASTER_MAX_ZOOM 3).

- [ ] **Step 4: Run — verify PASS + tsc.** `pnpm vitest run src/map && pnpm tsc --noEmit` → PASS/clean.

- [ ] **Step 5: Commit.** `feat(map): BaseMap on EPSG:3857 with Mercator base + zoom bridge (tuxlink-7h2m)`

## Task 6: `useTileSource` shared hook

**Files:** Create `src/map/useTileSource.ts` (+ test)

- [ ] **Step 1: Failing test.** `useTileSource.test.ts`: mock `getTileSourceStatus` + a config read; assert the hook returns `{ source, status }` when a source is configured+validated and `null` otherwise. (Follow the existing Tauri-invoke mock pattern.)

- [ ] **Step 2: Implement.** A hook that, on mount, calls `getTileSourceStatus()` (and reads the persisted source config) and returns `{ source: TileSource; status: TileSourceStatus } | null` suitable for `BaseMap`'s `tileSource` prop. Returns null on bundled/unreachable/incompatible.

- [ ] **Step 3: Run — PASS + tsc. Commit.** `feat(map): useTileSource hook feeding BaseMap (tuxlink-7h2m)`

## Task 7: Wire `tileSource` into StationFinderMap, GridMapPicker, PositionMapWidget

**Files:** Modify the three consumers (+ their tests)

- [ ] **Step 1: Failing tests.** For each, assert that when `useTileSource` returns a `lan-live` source, `BaseMap` receives a `tileSource` prop (testid/data-attr on the mock). Run → FAIL.

- [ ] **Step 2: Implement.** In each consumer, call `useTileSource()` and pass `tileSource={ts ?? undefined}` to `<BaseMap>`. PositionMapWidget also accepts + forwards an `onZoomChange` prop to BaseMap.

- [ ] **Step 3: Run — full src/request + src/map + src/compose + src/catalog vitest + tsc. Commit.** `feat(map): wire validated tile source into all map panes (tuxlink-n6xu, tuxlink-24px) (tuxlink-7h2m)`

## Task 8: PositionPickerOverlay live-zoom gate

**Files:** Modify `src/compose/PositionPickerOverlay.tsx` (+ test)

- [ ] **Step 1: Failing test.** Assert `sixCharAllowed` is evaluated against the LIVE zoom (driven by `onZoomChange`), not the constant: with a `lan-live` status at zoom ≥12 the 6-char path unlocks; at zoom <12 it does not. Run → FAIL.

- [ ] **Step 2: Implement.** Remove `const RASTER_VIEW_ZOOM = 2;`. Hold `const [viewZoom, setViewZoom] = useState(initialZoom)`; pass `onZoomChange={setViewZoom}` down through `PositionMapWidget`→`BaseMap`; compute `sixCharAllowed(status, { zoom: viewZoom })`.

- [ ] **Step 3: Run — PASS + tsc. Commit.** `fix(map): 6-char gate reads live tile-backed zoom (tuxlink-7h2m)`

## Task 9: Supersede the wrong docs + parity

**Files:** Modify the two spec/plan docs; AGENTS.md parity check

- [ ] **Step 1:** Add a top-of-file superseding note to `docs/design/2026-06-08-offline-map-foundation-approach.md` §1 and `docs/plans/2026-06-09-dyop-lan-tiles-plan.md`: "SUPERSEDED (2026-06-11, tuxlink-7h2m): the EPSG:4326-only / reject-Mercator decision was wrong — see docs/superpowers/specs/2026-06-11-map-mercator-lan-tiles-design.md. The map is now EPSG:3857 and ingests standard Web Mercator LAN tiles; the LAN/SSRF gatekeeper is the control." Run `pnpm lint:docs` (the pre-push gate) → passes.

- [ ] **Step 2:** AGENTS.md parity check (no CLAUDE.md rule changed here, so likely no-op — confirm + note). Commit. `docs(map): supersede 4326-only CRS decision (tuxlink-7h2m)`

## Task 10: Full gate + render gate + Geographica smoke + PR

- [ ] **Step 1: Full CI parity.** `cargo test --manifest-path src-tauri/Cargo.toml && cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings && pnpm typecheck && pnpm vitest run && pnpm build && pnpm lint:docs` — all green. Reap vitest workers.

- [ ] **Step 2: WebKitGTK render gate (CRITICAL).** Build + run the app (or the render harness if it covers a map pane) and grim-capture a map pane at 1920×1080 (memory `grim_realapp_validation_pandora`): confirm the Mercator base renders correctly (no plate-carrée stretch), the Maidenhead grid lines sit right, pan/zoom works, and zoom is capped at 3 with no tile source. Read the PNG. Iterate Task 5/7 ↔ here on any defect.

- [ ] **Step 3: Stage the Geographica live smoke (operator-run).** Surface a paste-ready check for the operator: in Settings → Map tile source, set `http://localhost:8090/styles/darkmatter/{z}/{x}/{y}.png`, scheme XYZ; confirm status → `lan-live`, tiles render, zoom unlocks past 3; and confirm a public URL is still rejected (LAN/SSRF gate intact). (Operator runs the GUI; the agent cannot.)

- [ ] **Step 4: Push + PR (ready, not draft).**
```bash
git -C <wt> push -u origin bd-tuxlink-7h2m/mercator-lan-tiles
gh pr create --base main --head bd-tuxlink-7h2m/mercator-lan-tiles --title "[<moniker>] feat(map): Web Mercator (EPSG:3857) LAN tiles; correct 4326-only spec (tuxlink-7h2m)" --body "<summary + the deleted-CRS-gate rationale + render PNG noted local + the Geographica smoke steps>"
```

---

## Self-Review

**Spec coverage:** coord fix (T1) ✓; delete CRS gate (T2, resolves the spec's resolved open question) ✓; TS wire + settings copy (T3) ✓; Mercator base asset (T4) ✓; BaseMap 3857 + WORLD/MERCATOR_BOUNDS + RASTER_MAX_ZOOM 2→3 + zoom bridge (T5) ✓; consumer wiring incl. n6xu/24px (T6,T7) ✓; 6-char live-zoom gate (T8) ✓; supersede docs (T9) ✓; gates + render + Geographica smoke + PR (T10) ✓. Premises: LAN/SSRF gate untouched (T2 keeps host.rs) ✓; offline fallback preserved (T4/T5 Mercator base) ✓; grid/GRIB/clicks CRS-agnostic (no task needed — verified) ✓.

**Placeholder scan:** asset task (T4) is GDAL-gated and flagged for possible operator approval; everything else has concrete code/commands. No "TODO/handle edge cases".

**Type consistency:** `tileSource` prop shape `{ source, status }` consistent T5/T6/T7; `onZoomChange` threaded BaseMap→PositionMapWidget→PositionPickerOverlay (T5/T7/T8); `MERCATOR_BOUNDS` defined T5 before use; `RASTER_MAX_ZOOM=3` consistent.

**Watch:** T4 (GDAL) is the only operator-gated step — if blocked, the code tasks (T1-3,5-9) still proceed; the render gate (T10 S2) and a correct offline base need the asset, so T4 blocks final sign-off, not the build.
