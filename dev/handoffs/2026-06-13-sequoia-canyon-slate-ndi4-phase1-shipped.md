# Handoff — tuxlink-ndi4 vector basemap PHASE 1 shipped (CI-green)

**Agent:** sequoia-canyon-slate · **Date:** 2026-06-13
**Branch:** `bd-tuxlink-ndi4/vector-basemap-build` · **Worktree:** `worktrees/bd-tuxlink-ndi4-vector-basemap-build`
**PR:** [#672](https://github.com/cameronzucker/tuxlink/pull/672) (DRAFT) · **HEAD:** `3d43600b`
**Plan:** `docs/superpowers/plans/2026-06-13-vector-basemap-maplibre-swap.md` (SELF-ADREV HARDENING section is authoritative)

## What this session did

Executed **Phase 1** (deps + scaffolding + Rust seam) of the Leaflet→MapLibre vector-basemap
swap. Phase 1 is the sequential **unblocker** — the Rust byte-range seam gates everything visual.
TDD throughout; Rust verified via CI (no cold cargo on the Pi), JS verified locally (RED→GREEN)
+ CI.

### Commits (5, all on the branch, pushed)
- `096e6383` build(basemap): pin maplibre-gl@5.24.0 / pmtiles@4.4.1 / @protomaps/basemaps@5.7.2 + CSP `worker-src 'self' blob:` + `tile:` in connect-src (A6/L7/A9)
- `f1ef39ef` feat(basemap): PMTiles 206-Range seam over `tile://` (Rust) — new `src-tauri/src/basemap/` module (A1/A3/A4/A10)
- `b4b5828c` test(basemap): global `createMapLibreMock` + owned hook layer `useMapSource`/`useMapLayer`/`useMapOverlay` (A14/A15)
- `abc582de` build(basemap): provenanced `scripts/build-basemap-bundle.sh` + `src-tauri/resources/basemap/README.md` (A8/A12)
- `3d43600b` fix(basemap): promote `flate2` to a regular dependency (CI caught dev-dep-only scope)

### CI status — VERIFIED GREEN (all 4 jobs)
`build-linux` arm64 + amd64 PASS · `verify` arm64 + amd64 PASS.
The `verify` job runs `cargo clippy --all-targets --locked -D warnings` + **`cargo test`** (ci.yml:98)
+ full `pnpm vitest run`. So the **25 basemap Rust unit tests RAN and PASSED** on CI (not just
compiled), and the **20 new JS tests + the entire existing vitest suite** passed (the global
maplibre mock in test-setup.ts broke nothing).

## Key technical facts (don't re-derive)

- **Serving (A1):** PMTiles served as RAW bytes over HTTP-206 `Range` on `tile://pmtiles/<archive>`
  (a distinct branch in the existing `lib.rs` `tile://` handler, `host == "pmtiles"` OR
  `path` starts `/pmtiles/`), consumed by pmtiles v4 `FetchSource`. Off the IPC pump; no custom
  Source / getKey / etag contract. Kept OUT of `src-tauri/src/tiles/` (parked for imagery).
- **Concurrency (A3):** one `Arc<File>` per archive in a `PmtilesRegistry` (managed state); reads
  use lock-free `pread` (`read_exact_at`). The `RwLock` is held only to clone the per-archive Arc out.
- **Validation (A4/A10):** `basemap::validate` — magic+spec-v3, extent bounds (truncation),
  `tile_type==MVT`, metadata `vector_layers ⊇` the **13** Protomaps ids (boundaries, buildings,
  earth, landcover, landuse, natural, physical_line, physical_point, places, pois, roads, transit,
  water — extracted from the real planet build, `version "3.7.1"`, `planetiler:version` present),
  size budget. Tests build synthetic PMTiles in-test (the real `sample.pmtiles` is gitignored).
- **Test double (A14):** `src/map/testMapLibreMock.ts`, installed GLOBALLY via
  `vi.mock('maplibre-gl')` in `src/test-setup.ts`. Queryable `__state.sources/layers` + `__emit`.
- **Hooks (A15) + a real finding:** React 19 cleans up effects in **declaration order** (verified:
  `setup A,B` then `cleanup A,B`), NOT reversed. Setup needs source-before-layer, so two separate
  hooks CANNOT also give layer-before-source teardown. Hence **`useMapOverlay`** — one combined
  hook whose single cleanup removes layers-then-source — is the canonical primitive for a coupled
  source+layers (the Maidenhead grid, drag-select fill). `useMapSource`/`useMapLayer` are for
  INDEPENDENT management only (a layer on the always-present basemap source).
- **CSP:** glyphs + sprites will serve under the `'self'` origin (relative paths), so only
  `tile:` (connect-src, for pmtiles) + `worker-src blob:` (MapLibre tile-decode worker) were needed.

## State of the tree

- **Working tree:** clean in the worktree. Branch pushed; up to date with origin.
- **Not user-reachable yet:** Leaflet still renders the map; the `basemap` Rust module is wired but
  inert until phase 2 constructs a `MapLibreMap`. The `world` archive registration in `.setup()`
  gracefully no-ops (resource absent → 404 → empty source, no crash). **No feature done-claim was
  made → the wire-walk gate is NOT yet triggered** (it runs at the phase-2 integration boundary,
  when consumers are flipped and a user flow exists to trace).

## Out-of-band / operator-only (NOT done by an agent shell)

- **Bundle assets (A8/A12):** run `scripts/build-basemap-bundle.sh` on a machine with the ~120 GB
  Protomaps planet → emits `src-tauri/resources/basemap/{world-z0-6.pmtiles, glyphs/, sprites/,
  provenance.json}`. THEN add `"resources/basemap/**/*"` to `src-tauri/tauri.conf.json`
  `bundle.resources` (intentionally omitted now — tauri build errors on a resource path resolving
  to nothing). Confirm/pin the planet build id (default `PLANET_BUILD=20240801`).
- **A13 marker-CSP grim spike (HIGHEST unknown):** before phase 2 commits to `maplibregl.Marker`,
  grim-spike on the PACKAGED `.deb` (not `tauri dev`) whether Marker's inline `transform` survives
  the CSP. If stripped → markers become a GeoJSON `symbol`/`circle` layer (larger
  StationFinderMap rewrite). `WEBKIT_DISABLE_DMABUF_RENDERER=1`.
- **D1 (phase 4):** region-pack hosting/provenance. **D2 (phase 3):** baked-dark aesthetic
  re-approval against the meshmap mock. Surface, don't self-decide.

## PR merge timing — operator call

PR #672 is DRAFT and inert+green. Either (a) keep accumulating phase 2+ on this same branch
(strangler-fig, A2) and flip to ready when the renderer swap makes it user-reachable, or
(b) merge the phase-1 foundation now (low risk: dead-but-tested module + maplibre deps ship unused
until phase 2). Recommend (a) — avoids shipping unused maplibre deps in a release with no user
benefit. Not parked: this is genuinely mid-feature (phase 1 of 6).

## NEXT — Phase 2 (the renderer swap, A2 strangler-fig)

Build the `MapLibreMap` component (renders bundled z0-6 LIGHT via the 206 seam) using the owned
hooks; re-implement MaidenheadOverlay + GridMapPicker as GeoJSON sources/layers via `useMapOverlay`;
drag-select interaction rewrite (dragPan/boxZoom disable, mousedown/move/up off `map`, window-mouseup
abort, post-drag click-suppression); re-point ALL 6 consumers + resolve the C11 break; **zoom-literal
remap to the z0-14 fractional scale** (Phase-0 notes list every literal); add MapLibre
`AttributionControl` "© OpenStreetMap contributors"; drop `tileSource`/`map_tile_source` IN THIS
PHASE (A5 atomic config removal); remove `leaflet`/`react-leaflet` only at the END. Phase-0 blast
radius (16 source + 14 test files) is in `dev/scratch/ndi4-spikes/PHASE0-NOTES.md`. Run wire-walk
at the integration boundary before any done-claim.
