# LAN offline-tiles + Find-a-Station — Consolidated Bug Hunt

**Date:** 2026-06-12
**Scope:** the full path "operator configures a LAN raster XYZ tile server → bind → serve over `tile://` → map renders + zooms past the bundled-raster z3 cap." Rust `tiles/*`, `config.rs`, `ui_commands.rs`, `lib.rs`; FE `map/*`, `catalog/StationFinderMap.tsx`, `settings/MapTileSourceSettings.tsx`.
**Hunters:** Exploratory, Holistic, Multipass (all three reports in this dir).
**bd:** tuxlink-k61j (fixes), tuxlink-w5xn (deferred follow-up).

## Why it failed ~5 times — the meta-finding

**No test ever crossed the `tile://` boundary with the real composed chain.** Unit tests mocked `config_read` (papering over the DTO drop), hand-built `TileCoord` (hiding the TMS double-flip), and asserted props the mock mirrors (hiding the mount-once `maxZoom`). Each layer looked correct in isolation; the failures lived in the seams between them. Plus an operator-side mismatch: the test server (`:8093`) is **Geographica, a MapLibre vector-tile SPA**, not a raster XYZ server — it physically cannot feed this feature. A `testing-pitfalls.md` §7 entry was added (boundary-crossing test requirement).

## Confirmed bugs — FIXED this cycle (in tuxlink-k61j / PR #654)

| # | Bug | Sev | Fix |
|---|-----|-----|-----|
| B-DTO | `ConfigViewDto` (what `config_read` returns) dropped `map_tile_source` → FE `useTileSource` always null → map never tile-backed → zoom stuck at 3; Settings hydration silently dead | **critical** | added the field to the DTO + `From` impl + test |
| B-ZOOM | `BaseMap` `maxZoom` is mount-once in react-leaflet; the async tile descriptor never raised the live cap | **critical** | `ApplyMaxZoom` child → `map.setMaxZoom` imperatively |
| B1 | TMS sources request the **wrong (Y-flipped) tile** — Leaflet flips Y, backend flips again | **critical** | `tms={false}` on the Leaflet layer; backend is the sole flip; test updated (it had encoded the bug) |
| B-TEMPLATE | malformed placeholder (`{z]`, `{Z}`) → `is_template` false → base-dir branch → 404 → probe maps to LanLive → false "source active" | significant | reject any stray brace in `build_tile_url`; client-side template check with an honest message |
| B7 | `TileLayerBridge` set `maxNativeZoom` but no `maxZoom`/`minZoom` → up-scale band dead, spurious coverage-404 noise | minor | `maxZoom={appMaxZoom}` + `minZoom={source.minZoom}` |
| B6 | cleared Maximum-zoom field → `0` → binds and pins map at z0 | minor | reject `maxZoom < 1` / `< minZoom` in the form |
| B-COPY | "incompatible — server did not return image tiles" shown for a malformed URL the server never received | minor | client-side template error message |
| B-DEBUG | temporary `MapDebugOverlay` broke 10 catalog tests (mock lacks `getMaxZoom`) + would ship a debug HUD | ship-blocker | removed |

## Deferred — tracked in tuxlink-w5xn (separable; online path works without them)

- **B2 (offline cache):** breaker checked before cache; `lan-cached` is dead code → a source that goes offline collapses to bundled raster instead of serving cache.
- **B4:** `test_tile_source` dry-run pollutes the on-disk cache.
- **Probe philosophy (design decision):** single-coord `0/0/0` 404 → LanLive validates a wrong-path/empty server. Needs operator input on the right tradeoff (positive-fetch requirement vs sparse-server tolerance vs serve-time downgrade).

## Operator-side (not a code bug)

The `:8093` test server is a **vector-tile** app; tuxlink consumes **raster** `{z}/{x}/{y}.png`. A throwaway raster server (`dev/scratch/raster_tile_server.py`) was stood up at `pandora.local:8099` to validate the feature end-to-end.
