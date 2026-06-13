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
| L3 | **FLIPPED by self-adrev (see HARDENING A1):** PMTiles served via **HTTP-206 `Range` on the existing `tile://` URI scheme** (wry 0.55.1 verified to forward `Range` + emit `206`/`Content-Range`), consumed by pmtiles' native `FetchSource` — off the serialized IPC pump, no custom JS Source, no `getKey`/etag contract. The Tauri-IPC byte-range Source (R1 spike, PASS) is the **proven fallback**, not primary. | Design said IPC-primary/206-optimization; adrev reversed it. |
| L4 | Coverage: bundled world **z0–6** (~30–60 MB) + downloadable **permanent** per-region packs **z0–14**. Full-planet out of scope. | Design. |
| L5 | Catalog of schema-validated region packs + advanced custom-URL (schema-validated on download). | Design (R3). |
| L6 | #659 raster **basemap** retired; raster transport retain-vs-delete tied to the imagery increment (deferred). | Design. |
| L7 | **Versions pinned (adrev A6):** `maplibre-gl@^5` + `@protomaps/basemaps@^5` + `pmtiles@^4` (internally consistent; v5 basemaps require maplibre v5). The R1/R4 spikes ran maplibre 4.7.1 — **directional, re-validate the seam on v5 in phase 1.** | Self-adrev. |

## SELF-ADREV HARDENING (2026-06-13, mink-kingfisher-magpie — Codex unavailable, 3 diverse-lens Claude agents; documented exception to no-carveout)

Three independent adversarial agents (Rust-serving / React-lifecycle / style-offline-distribution) found 2 locked-decision reversals + 4 P0 gaps + supporting P1/P2s. Dispositions:

**Architecture refinements (incorporated):**
- **A1 (flip L3):** primary serving = HTTP-206 `Range` on `tile://` + pmtiles `FetchSource` (off the IPC pump, no custom Source/getKey/etag). Add a **distinct path-prefix branch** in the existing `lib.rs` `tile://` handler (e.g. `tile://pmtiles/<archive>`) — do NOT thread through `serve_tile`/`fetch.rs` (those are HTTP-tile/image-magic shaped, parked for imagery). Serve **raw bytes, zero content decoding** (no `gzip(true)`). IPC-Source = proven fallback.
- **A2 (strangler-fig phasing):** NOT "one atomic phase." BaseMap transiently exports both a Leaflet variant (old) and a MapLibre variant (new); flip the 6 consumers one at a time (each independently green); remove `leaflet`/`react-leaflet` only after the last flip. Supersedes the eng-review "atomic" framing (it overcorrected → unbuildable single red landing on the CI-only loop).
- **A3 (file reads):** one long-lived `Arc<File>` per archive in managed state; **lock-free `read_at`/pread** (no per-read open, no mutex, no handle cap, NO mmap for region packs — page-cache thrash vs the tight GPU/WebKit budget). Short final read at EOF → clamped 206 with real length, not an error.
- **A4 (download validation, R5):** validate the **temp file post-download, pre-rename**: PMTiles magic `"PMTiles"`+version `0x03` (reject v1/v2); header-declared root-dir/tile-data offsets within file size (catch truncation); metadata `vector_layers ⊇` the R3-locked id set + planetiler schema version; size budget. Reuse `forms::updater` streaming-abort + `cache.rs:302` atomic pattern (NamedTempFile→sync_all→persist→parent-dir fsync); write manifest entry AFTER rename+dir-fsync; startup orphan-sweep mirrors `forms::import::sweep_stale_staging`.
- **A5 (atomic config removal):** dropping `config.map_tile_source` MUST land in the same commit as removing the gatekeeper-rehydration (`config.rs:330`) + `persist_source` (`commands.rs:136`) or it won't compile. The 4 `tiles::commands::*` stay registered (parked, not dead) OR are removed together — no half-wired command surface (wire-walk).
- **A6 (versions):** see L7. Pin maplibre-gl@5 / @protomaps/basemaps@5 / pmtiles@4; re-validate `addProtocol` + Source signatures + the seam on v5 in phase 1.
- **A7 (dark style completeness):** inverted Flavor slots **+ swap to protomaps' authored `dark` sprite** (icons are raster PNGs, not slot-derived; do NOT invert the light sprite) **+ a belt-and-suspenders pass** over the generated layer array transforming any non-slot literal/`hsl()`/`rgba()` `*-color` (incl. `text-halo-color` derived coherently with the inverted text + bg). Test: every `*-color` in the final dark style is transformed-hex or an expression with transformed color leaves.
- **A8 (offline glyphs — P0 blocker):** bundle the exact fontstacks the pinned basemaps light flavor references (verify via `grep text-font` on the generated style; expect `Noto Sans Regular/Medium/Italic` from the separate `protomaps/basemaps-assets` repo), bundle Latin `{range}.pbf` (CJK ballons — **Latin-only is the documented EmComm default**), resolve `glyphs:` to a **local serving path distinct from `pmtiles_read_range`** (glyphs are `{fontstack}/{range}`-keyed, not byte-range). Without this, labels silently 404 → unlabeled map.
- **A9 (CSP — P0 blocker):** amend `tauri.conf.json` CSP in phase 1/2: `worker-src 'self' blob:` (maplibre's tile-decode worker — blank map otherwise on WebKitGTK), plus the scheme(s) glyphs/sprite/PMTiles resolve through added to `connect-src`/`img-src`/`font-src`. Reason about this up front — a too-tight CSP is a blank-map failure that only shows at the (non-merge-gating) WebKitGTK smoke.
- **A10 (R3 schema lock):** the real sample is PMTiles-spec-v3 carrying Protomaps-schema-**v4** with **13** `vector_layers` (`boundaries,buildings,earth,landcover,landuse,natural,physical_line,physical_point,places,pois,roads,transit,water`) — the design's 9-id and the spike's 10-id lists are incomplete. Lock R3 against the actual id set extracted from the pinned pack's metadata (store a fixture, don't hand-maintain prose) + the planetiler schema version. **Pin ONE planet build hash** for both the bundled z0–6 AND every catalog pack (divergent schemas → R7 seam/blank).
- **A11 (R7 compositing):** prefer **per-source zoom-range clamping** (overview source layers `maxzoom 6`, region source layers `minzoom 6`) so each source owns a disjoint zoom band — avoids both double-drawn translucent fills/labels and the viewport-switch flash. Test the z6 boundary.
- **A12 (build tooling provenance):** checked-in pinned `scripts/build-basemap-bundle.sh` recording source planet build hash + `pmtiles` CLI version + bbox/zoom + checksum; runs out-of-band (needs the 120 GB planet — NOT in PR CI); treat the bundled PMTiles like `ssn-forecast.json` (provenanced, not a mystery blob).
- **A13 (marker-CSP spike — highest unknown):** before committing to `maplibregl.Marker`, grim-spike on the **packaged `.deb`** (not `tauri dev` — the CSP/inline-style delta only appears packaged, cf. the s0r1 black-blob incident): confirm Marker's inline `transform` positioning survives the CSP. If stripped → markers must become a GeoJSON `symbol`/`circle` layer (larger StationFinderMap rewrite). Operator-pin z-order + CSP-safe sizing ride on this.
- **A14 (test double):** re-architect the map test-double from the declarative react-leaflet renderer (`testMapMock.ts`) into an **imperative `createMapLibreMock()`** — a fake `maplibregl.Map` with `addSource`/`addLayer`/`removeLayer` spies + a queryable `{sources,layers}` registry + event registry. Put it **global in `src/test-setup.ts`** (the constructor touches WebGL on instantiate → per-file mocks are a footgun; App-level/PositionFormV2 transitively mount it). Accept explicitly: unit tests verify **wiring only**; style-load-ordering / setStyle-re-add / StrictMode correctness is **grim-only, post-merge, fix-forward** (jsdom has no WebGL/workers; over-faking shipped 2 map bugs before — ku2b, k61j).
- **A15 (canonical hook):** `useMapLayer`/`useMapSource` MUST: gate every add on `isStyleLoaded()` AND re-subscribe on `styledata` (idempotent — guard `getLayer/getSource` before add, since `styledata` fires repeatedly and after every `setStyle`); tolerate StrictMode double-invoke (production `main.tsx` keeps `<StrictMode>`); teardown `removeLayer` before `removeSource`. Spec one canonical body + a unit test per hazard. "Thin" = small API, large lifecycle correctness.
- **A16 (6-char precision gate):** dropping `tileSource` removes the input to `sixCharAllowed` (today it unlocks on a bound LAN raster source). Re-derive the unlock predicate ("a z14 region pack covers this point + zoomed in") and decide **phase-2 interim behavior** (default 4-char until packs land) so `PositionPickerOverlay` doesn't ship a dead precision selector for two phases.
- **A17 (onZoomChange seeding):** re-expressed `onZoomChange` must fire once on `load`/`moveend` with `map.getZoom()` (not just `zoomend`) so consumers seed from the real fractional zoom, not a stale literal `1`.
- **A18 (TileStatusPill):** its `StatusKind` vocabulary (lan-live/cached/partial/…) is raster-LAN-only; re-author for the pack world (bundled / region-detail). The `_exhaustive: never` guard will force the compile — budget the rewrite + the design §8.5 table re-author.

**DEFERRED OPERATOR DECISIONS (surface at their phase, do not self-decide):**
- **D1 (phase 4) — catalog-pack hosting/provenance.** Protomaps publishes only a ~120 GB whole-planet file, not per-region z0–14 packs. tuxlink must build each pack (`pmtiles extract` a bbox) and **host them** for runtime download. Operator decides: where/what budget to host; rebuild cadence; one pinned planet-build hash for bundle+packs. Until decided, "curated catalog" is aspirational.
- **D2 (phase 3) — baked-dark aesthetic re-approval.** The operator approved the meshmap look from the **CSS-filter-over-composited-canvas** mock. The baked per-slot path is mathematically different at translucent overlaps + label halos (`invert(a over b) ≠ invert(a) over invert(b)`), and the spike had NO labels — so the baked *aesthetic* is unproven. Phase 3 produces a real baked-dark render (with labels) for the operator to re-approve before phase 4 builds packs on top.

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

## Build phasing (feeds build-robust-features) — REVISED per outside-voice findings, then HARDENED by self-adrev

> **Read the SELF-ADREV HARDENING section above first — it supersedes parts of this phasing.** Net deltas: phase 1 also pins versions (A6/L7), adds the **HTTP-206 `tile://` serving branch** (A1, replacing IPC-Source-primary), bundles **glyphs + light/dark sprites** (A7/A8), amends **CSP** `worker-src blob:` (A9), builds the **`createMapLibreMock` test double** (A14), and runs the **packaged-`.deb` marker-CSP grim spike** (A13). Phase 2 is **strangler-fig** (A2: both renderers transient, per-consumer flip, leaflet removed last) — NOT "one atomic phase." Phase 3 ends with the **D2 operator aesthetic re-approval**; phase 4 starts with the **D1 hosting decision**.

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
