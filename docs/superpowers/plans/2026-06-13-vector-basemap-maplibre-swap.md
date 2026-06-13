# Plan — Self-hosted vector OSM basemap (Leaflet→MapLibre swap) — tuxlink-ndi4

**Agent:** mink-kingfisher-magpie · **Date:** 2026-06-13 · **Branch:** `bd-tuxlink-ndi4/vector-basemap-build`
**Design (spec, source of truth):** `docs/design/2026-06-13-self-hosted-vector-osm-basemap-design.md` (APPROVED + AMENDED — read the AMENDMENT block: dark = baked GL style, not CSS filter).
**Eng-review:** this doc (plan-eng-review, mink-kingfisher-magpie). Gating spikes R1+R4 done before this (`dev/scratch/ndi4-spikes/SPIKE-FINDINGS.md`).
**Next gate:** build-robust-features with the mandatory cross-provider Codex adversarial review (no-carveout rule).

---

## Locked decisions (do not re-litigate)

| # | Decision | Source |
|---|---|---|
| L1 | Renderer: **MapLibre GL JS**, used via **raw `maplibre-gl` + a thin owned hook layer** (`useMapLayer`/`useMapSource`). NOT react-map-gl. | Eng-review 2026-06-13 (operator deferred to eng judgment): direct engine access (this project repeatedly needs imperative engine control — DMABUF, baked style, pin zIndex, CSP marker sizing, reactive props); fewer rotting deps; bounded overlay set; style-swap control for dark mode. |
| L2 | Dark mode = **build-time-baked GL-native inverted style** (invert + W3C hue-rotate(180°) + brightness(1.33) on each layer color), NOT a runtime CSS filter. | R4 spike (operator decision). CSS filter ~15fps on Pi; baked = 45fps = light. |
| L3 | Format: **PMTiles**, loaded via `pmtiles` protocol; **primary serving = Tauri IPC byte-range command** returning **raw bytes** (`tauri::ipc::Response`, NOT a JSON number array / base64). `tile://` 206/Range is an optimization spike, not a prerequisite. | Design + R1 spike (PASS). |
| L4 | Coverage: bundled world **z0–6** (~30–60 MB) + downloadable **permanent** per-region packs **z0–14**. Full-planet out of scope. | Design. |
| L5 | Catalog of schema-validated region packs + advanced custom-URL (schema-validated on download). | Design (R3). |
| L6 | #659 raster **basemap** retired; raster transport retain-vs-delete tied to the imagery increment (deferred). | Design. |

## Step 0 — Scope challenge

**What already exists (reuse vs rebuild):**
- `src/map/BaseMap.tsx` — react-leaflet substrate (world raster ImageOverlay + optional LAN TileLayer). **Replaced** by a MapLibre map component.
- `src/map/TileLayerBridge.tsx`, `useTileSource.ts`, `tileSource.ts`, `tileSourceEvent.ts` — LAN raster tile wiring (#659). **Retired as basemap**; Rust transport (`src-tauri/src/tiles/`) parked for imagery.
- `src/map/MaidenheadOverlay.tsx`, `GridMapPicker.tsx` — grid lines + drag-select, **rendered as react-leaflet children**. **Rewritten** as MapLibre GeoJSON sources/layers via the owned hook (L1). Pure-geometry helpers (`projection.ts`, `gridGeometry.ts`, `gribRegion.ts`, `sixCharAllowed.ts`) are renderer-agnostic — **reused as-is**.
- Consumers `src/compose/PositionMapWidget.tsx`, `src/catalog/StationFinderMap.tsx` (+ overlays `PositionPickerOverlay`, the grid pickers) — **re-pointed** to the new map component; the C11 `children` contract is re-expressed via the hook layer.
- `src-tauri/src/lib.rs` `tile://` scheme handler — **generalized** (net-new Range/206 path is optional; for imagery `.jpeg` the `.png` hardcode in `serve.rs:86` + `fetch.rs build_tile_url` must be templated — deferred to imagery).
- `src-tauri/src/tiles/{fetch,host,cache,breaker,serve,commands}.rs` — SSRF/host-pin/cache/breaker. **Parked** for imagery; download path needs its OWN validation (PMTiles header+schema+size), distinct from the per-tile image-magic + `MAX_TILE_BYTES` check (R5).

**Minimum change set:** full renderer swap is unavoidable (the basemap IS Leaflet). The swap's blast radius is the 6 Leaflet-coupled `src/map` + `src/compose` + `src/catalog` files + their tests. New code: a MapLibre map component, the owned hook layer, the pmtiles Rust command + JS Source, the baked-dark-style builder, the region-pack manager (Rust manifest + download + UI). This is >8 files and intrinsically so — it is a renderer migration, not scope creep. No reduction available without abandoning the feature.

**Boring-by-default:** maplibre-gl + pmtiles + @protomaps/basemaps are the proven, Protomaps-recommended stack tuxlink already runs (geographica). No innovation token spent.

## Architecture & data flow

```
 FIRST LAUNCH (offline, no config)                 REGION PACK (operator downloads)
 ─────────────────────────────                     ──────────────────────────────────
 app resources: world-z0-6.pmtiles                 catalog pick / custom URL
        │                                                  │  pre-flight free space
        ▼                                                  ▼  download→temp file
 MapLibreMap (raw maplibre-gl)                      validate: PMTiles magic + R3 schema + size
   addProtocol('pmtiles', Protocol.tile)                   │  atomic rename → packs dir
   PMTiles(new IpcSource('world'))  ◄── R1 seam            ▼  register in manifest.json
        │  getBytes(off,len) → invoke                packs/<id>.pmtiles  (PERMANENT, explicit delete)
        ▼  ('pmtiles_read_range', raw bytes)                │
 src-tauri: pmtiles_read_range(archive_id,off,len)         │ added as a 2nd pmtiles source (R7 compositing)
   → seek+read local file → Response(Vec<u8>)              ▼
        │                                            overview(z0-6) + region(z0-14) co-render:
        ▼                                            "never blank; full detail where downloaded"
 MapLibre renders vector tiles (GPU)
        │   style = light flavor  OR  baked-dark flavor (L2)   ◄── view-mode toggle, persisted
        ▼
 owned hook layer (useMapLayer/useMapSource):
   MaidenheadOverlay → GeoJSON line source+layer
   GridMapPicker     → GeoJSON fill source+layer (drag-select)
   station/operator pins → maplibregl.Marker (operator pinned above via element zIndex)
```

**C11 contract resolution:** the frozen `BaseMapProps` (children/onMapClick/initialCenter/initialZoom/tileSource/onZoomChange) is re-expressed: `onMapClick`→`map.on('click')` (clamped); `onZoomChange`→`map.on('zoomend')`; `initialCenter/Zoom`→map constructor; `children`→overlays consume the map via context+hook (not Leaflet child elements); `tileSource`→**removed** (LAN raster basemap retired). This is a **breaking change to a frozen contract** — coordinated across all 3 consumers in one phase, not piecemeal. The 3 reactive-prop shims (`ApplyMaxZoom`, `RecenterOnOperator`, async-arrival) re-appear as effects driving `map.setMaxZoom`/`map.flyTo`; MapLibre props are equally non-reactive, so the same async-arrival handling is required.

## Failure modes (per new codepath)

| Codepath | Realistic prod failure | Test? | Error handling | User sees |
|---|---|---|---|---|
| `pmtiles_read_range` | offset/len past EOF; file deleted mid-session; handle exhaustion under pan | unit (Rust) | bounded reads, clamp, per-archive handle cap | blank tile → overview beneath (never void) |
| IpcSource.getBytes | invoke rejects / archive unregistered | vitest (mock invoke) | reject → MapLibre tile error → overview fallback | coarse overview, no crash |
| baked-dark style build | a non-hex color (rgba/expression) slips the transform | unit (xform) | skip non-hex, keep value | one mis-colored layer (not a crash) |
| pack download | partial/corrupt write; insufficient space; non-Protomaps schema | unit + integration | pre-flight space, temp+atomic, magic+schema+size validate, startup orphan sweep | clear reject error; no half-pack registered |
| style.setStyle (light↔dark) | overlay sources dropped on style swap | vitest + manual | re-add overlay layers on `styledata` | overlays persist across toggle |
| overview+region compositing (R7) | seam/blank at region boundary | manual (grim) | dual-source layering | full detail in region, overview elsewhere |

**Critical-gap watch:** the pack-download validation path (R5) is the one place a silent failure could persist corrupt state — atomic rename + post-rename manifest registration + startup orphan sweep close it. Must be tested.

## Test coverage plan (TDD against the design's success criteria)

- **Rust unit** (`pmtiles_read_range`): correct bytes for a known range; clamp past-EOF; concurrent reads; missing archive → error.
- **Rust unit** (pack manifest + validate): PMTiles magic accept/reject; schema (R3 layer-id set) accept/reject; size budget; atomic install; orphan sweep deletes partials.
- **vitest** (IpcSource): getBytes resolves ArrayBuffer from mocked invoke; rejects → null path; addProtocol registration.
- **vitest** (baked-dark builder): `xformHex` matches expected for sample colors; non-hex passthrough; full style transform leaves opacities/widths intact.
- **vitest** (MapLibreMap + hooks): mounts; onMapClick/onZoomChange fire; overlays add/remove sources on mount/unmount; style swap re-adds overlay layers (regression test — the react-leaflet pane-occlusion class of bug).
- **vitest** (consumers): PositionMapWidget/StationFinderMap/grid pickers render against the new map; **App-level mount test** (not just scaffolded providers) per [[feedback_test_production_mount_path_not_just_units]].
- **CI gate** ([[feedback_scoped_vitest_misses_contract_tests]]): `cargo clippy --all-targets -D warnings` (re-run to exit 0) + full `pnpm vitest run` before push.
- **grim / operator smoke** (post-merge, opportunistic — NOT a merge gate per [[feedback_browser_smoke_before_ship]]): WebKitGTK render fidelity, light↔dark, pack detail, with `WEBKIT_DISABLE_DMABUF_RENDERER=1`.

## Build phasing (feeds build-robust-features) — REVISED per outside-voice findings

**Phase 0 (do first):** enumerate the REAL Leaflet blast radius, not "3 consumers":
`grep -rl "BaseMap\|useTileSource\|tileSource\|react-leaflet\|leaflet" src/`. Known set:
`src/map/{BaseMap,TileLayerBridge,MaidenheadOverlay,GridMapPicker,TileStatusPill}.tsx` +
`{tileSource,tileSourceEvent,useTileSource}.ts`; `src/compose/{PositionMapWidget,PositionPickerOverlay}.tsx`;
`src/catalog/StationFinderMap.tsx`; `src/settings/MapTileSourceSettings.tsx`; and **every** `*.test.tsx`
that mounts the react-leaflet test mock (BaseMap, GridMapPicker, StationFinderMap, PositionMapWidget,
PositionPickerOverlay, PositionFormV2, TileLayerBridge, useTileSource, MapTileSourceSettings). Tests are
**rewritten**, not re-pointed. Renderer-agnostic geometry (`projection.ts`, `gridGeometry.ts`,
`gribRegion.ts`, `sixCharAllowed.ts`) is reused as-is.

1. **Deps + scaffolding + Rust seam + concurrency:** add `maplibre-gl`, `pmtiles`, `@protomaps/basemaps`
   (KEEP `leaflet`/`react-leaflet` installed — removed at the END of phase 2). Owned hook-layer skeleton.
   Rust `pmtiles_read_range(archive_id, offset, length) → tauri::ipc::Response(Vec<u8>)` (raw bytes).
   Specify the `pmtiles` `Source` contract: stable `getKey()`, etag handling (an unstable key re-reads the
   header every frame). **Bundled-resource resolution (finding 5):** pin how `archive_id="world"` resolves to
   a *seekable* path — verify against the actual `.deb` resource layout (NOT `tauri dev`); if the bundled
   archive is embedded/non-seekable, add a first-launch copy-to-appdata step. **Concurrency spike (finding 4,
   in-phase not deferred):** scripted fast-pan with overview+region both mounted; count concurrent in-flight
   invokes + p95 read latency; set the per-archive file-handle cap from data (a cap that's hit = frozen map).
   The spike's "9 reads/settled-firenze" is struck from the throughput evidence base.
2. **ATOMIC renderer swap (was 2+3 — they are NOT separable; overlays are react-leaflet children OF the map):**
   MapLibre map component renders bundled z0–6 **light** via the IPC seam; re-implement MaidenheadOverlay +
   GridMapPicker as GeoJSON sources/layers on the owned hook; markers (operator pinned above via element
   zIndex); **drag-select interaction rewrite (finding 8):** `dragPan`/`boxZoom` disable, mousedown/mousemove/
   mouseup off `map` with `e.lngLat`, window-mouseup abort, post-drag click-suppression — the historically
   bug-prone half. Re-point ALL consumers (incl. PositionPickerOverlay) + resolve the C11 break + migrate
   MapTileSourceSettings IN THIS PHASE (drop `tileSource`/`map_tile_source` here, not 3 phases later).
   **Zoom-scale remap (finding 2):** re-derive every zoom literal — `OPERATOR_ZOOM`, `initialZoom` defaults,
   `levelFromZoom`, the 6-char Maidenhead gate threshold — against the z0–14 fractional scale (old values were
   z0–3 raster-native = wrong meaning now). **Attribution (finding 10, legal ODbL):** add MapLibre
   `AttributionControl` "© OpenStreetMap contributors" HERE (first OSM render; today's map runs
   `attributionControl=false`, so this is net-new, not deferrable to last). Reactive-prop effects
   (`setMaxZoom`/`flyTo` for async arrival). **Remove `leaflet`/`react-leaflet` at the end of this phase**
   (the tree is non-compiling until then — one atomic landing, no false mid-phase green checkpoint).
3. **Dark mode (L2) — Flavor-slot builder (finding 3), not a hex regex:** derive the dark flavor by transforming
   `@protomaps/basemaps`'s ~12 NAMED `Flavor` color slots (the expressions reference the slots, so this covers
   data-driven paints, halos, background). Handle `text-halo-color` coherently with its fill. **Operator
   aesthetic checkpoint:** grim-render baked-dark vs the approved meshmap mock BEFORE building packs on top —
   per-hex hue-rotate only approximates the composited-canvas look the operator approved; confirm match here,
   not in phase 5. Light/dark/imagery view-mode state, persisted; light↔dark via setStyle with overlay re-add
   (regression test).
4. **Region packs (IN scope — finding 9's split REJECTED per the alpha-completeness bar):** catalog + custom-URL;
   pre-flight space, temp+atomic install, manifest, list/delete UI, startup orphan sweep; **R5 download
   validation** (PMTiles magic + R3 schema + size budget — distinct from the per-tile image-magic/`MAX_TILE_BYTES`
   check); **R3 schema lock** (pin the exact Protomaps layer-id set); **R7 overview+region compositing** (spike
   inside this phase: dual-source vs viewport-switch; "never blank; full detail where downloaded").
5. **Retire/migrate + docs:** finish raster-basemap retirement; migrate residual config (C6); park the raster
   transport (`src-tauri/src/tiles/`) for imagery; document-release (README/CHANGELOG).
6. **(Next increment, SEPARATE feature/bd issue)** imagery/hybrid mode on the parked transport (`.jpeg`/template
   generalization OR direct-HTTP; eng-review call at that time).

## Parallelization (corrected per finding 7)

Mostly **sequential** with one real parallel window. `pmtiles_read_range` (phase 1, Rust) is the **unblocker** — Lane B's basemap can't paint without it, so the lanes are NOT parallel "through 2–4" as first claimed.
- **Sequence first:** phase 1 Rust seam + concurrency spike (the gate for everything visual).
- **Then parallel:** **Lane A** = phase-4 pack download/manifest/validate (`src-tauri/`) ∥ **Lane B** = phase-2 atomic renderer swap + phase-3 dark builder (`src/map`, `src/compose`, `src/catalog`, `src/settings`).
- Converge at phase 4's pack-manager UI (needs both). Conflict flag: phase-4 UI + phase-5 Settings cleanup both touch Settings — sequential.

## NOT in scope (deferred, with rationale)
- **Imagery/hybrid view mode** — follow-up increment; transport retain-vs-delete decided then.
- **`tile://` 206/Range serving** — optimization; IPC raw-bytes is primary and sufficient (L3).
- **Full-planet pack** — YAGNI for EmComm; drives disk/range complexity for an unused path.
- **react-map-gl** — rejected (L1).
- **Pack auto-update / eviction** — packs are permanent resources, explicit delete only.

## Open spikes carried into the build (non-gating)
- **R3** schema lock: pin the exact Protomaps basemap layer-id set against a known-good build before coding the download gate.
- **R7** overview+region compositing: dual-source vs viewport-switch — decided in phase 5.
- **R5** download validation: PMTiles-specific (header+schema+size), distinct from per-tile image checks.

## Outside-voice review (Claude subagent — Codex preserved for build-robust-features adrev)

Independent challenge found the architecture sound (L1/L2/L3 confirmed) but the **phasing
defective**. Dispositions:

| # | Sev | Finding | Disposition |
|---|---|---|---|
| 1 | P0 | "3 consumers" undercounts (Settings, PositionPickerOverlay, TileStatusPill, all tests) | **FIXED** — phase 0 grep + full file list |
| 6 | P0 | phases 2+3 not separable; dep-removal strands them | **FIXED** — collapsed to one atomic phase 2; dep-removal at its end |
| 3 | P0 | baked-dark harder than `xformHex` (Flavor slots, expressions, halos; approximation risk) | **FIXED** — retargeted to ~12 Flavor color slots + operator aesthetic checkpoint |
| 2 | P1 | fractional-zoom literals wrong on z0–14 | **FIXED** — zoom-scale remap in phase 2 |
| 4 | P1 | IpcSource concurrency; 9-reads misleads; Source contract | **FIXED** — concurrency spike + `getKey`/etag in phase 1 |
| 5 | P1 | bundled resource may be non-seekable | **FIXED** — resolution pin + copy-to-appdata fallback, phase 1 |
| 7 | P2 | Lane A/B parallelism overstated | **FIXED** — corrected sequence |
| 8 | P2 | drag-select interaction rewrite unspecced | **FIXED** — explicit in phase 2 |
| 10 | P2 | attribution (ODbL) parked too late | **FIXED** — moved to phase 2 (first OSM render) |
| 9 | P2 | split region packs into separate feature | **REJECTED** — conflicts with the alpha-completeness bar ([[feedback_alpha_is_vettedness_not_built_ness]]); design success-criteria 3–4 include packs. Packs stay in scope, phased last. |

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 0 | — | — |
| Codex Review | `/codex review` | Independent 2nd opinion | 0 | pending build-robust-features (no-carveout) | — |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | CLEAR | architecture fork L1 resolved; outside voice ran (10 findings, 9 fixed, 1 rejected w/ rationale); C11 break coordinated into one atomic phase; R5 critical-gap test required |
| Design Review | `/plan-design-review` | UI/UX gaps | 0 | — | pack-manager UI surfaces in phase 4 |
| DX Review | `/plan-devex-review` | Developer experience gaps | 0 | — | — |

**UNRESOLVED:** none. **VERDICT:** ENG CLEARED — architecture locked + phasing corrected by outside voice; ready for build-robust-features (with mandatory cross-provider Codex adrev).
