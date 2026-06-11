# Offline-first map foundation — implementation approach

> **CRS DECISION SUPERSEDED (2026-06-11, tuxlink-7h2m).** §1's "EPSG:4326 not
> EPSG:3857 / reject Mercator" choice was a wrong over-rotation of the
> never-public-OSM posture: that posture means no public-tile-server *abuse*, not
> "refuse Web Mercator / no network ingestion." The map is now `L.CRS.EPSG3857`
> and ingests standard Web Mercator XYZ tiles from a self-hosted/LAN source; the
> LAN-only/SSRF host gatekeeper is the real control. The offline-first posture
> itself stands (now a bundled Mercator base raster). See
> [`docs/superpowers/specs/2026-06-11-map-mercator-lan-tiles-design.md`](../superpowers/specs/2026-06-11-map-mercator-lan-tiles-design.md).

> Status: **proposed** (agent `moss-basalt-hawk`, 2026-06-08). Resolves the
> "Open items for the implementation plan" deferred by the **locked** design
> [`2026-06-07-map-pin-grid-design.md`](2026-06-07-map-pin-grid-design.md).
> The locked spec's *posture* (offline-first, never public OSM) was
> re-affirmed by the operator 2026-06-08 and is **not** re-opened here. This
> doc is the target of the build-robust-features 5-round adversarial review
> and the input to `/writing-plans`.
>
> bd: foundation `tuxlink-z9u4` (this worktree) · consumers `tuxlink-mxmx`
> (item 21, GRIB box) · `tuxlink-urbv` (item 18, pin — blocked on `9xy1`) ·
> remediation `tuxlink-714t`.

## 0. Grounding correction (the locked spec is factually wrong here — posture stands)

The locked spec's "Grounding (verified, corrects the audit)" section claims a
`PositionMapWidget` + Maidenhead converter + map library do **not** exist and the
feature is "greenfield." **Verified against `origin/main`, all three exist and
shipped ~2 days before that brainstorm** (almost certainly the brainstorm agent
read the recovery checkout, which is 639 commits behind `origin/main`):

| Locked-spec claim | `origin/main` reality (evidence) |
|---|---|
| "no map library" | `leaflet ^1.9.4`, `react-leaflet ^5.0.0`, `@types/leaflet` in `package.json` |
| "no lat/lon↔Maidenhead converter" | `src-tauri/src/position/maidenhead.rs` + `src/forms/position/maidenhead.ts` (`gridToLatLon`, `latLonToGrid`) |
| "external OSM tiles already forbidden / never public OSM" | CSP **whitelists** `tile.openstreetmap.org` in `img-src` + `connect-src`; `src/compose/PositionMapWidget.tsx` loads `https://tile.openstreetmap.org/{z}/{x}/{y}.png` directly (PR #392; CSP PR #420 = closed `tuxlink-bt2q`) |

**Consequence:** less greenfield than the spec assumed (reuse the converter +
Leaflet), AND a shipped widget + CSP currently **violate** the never-public-OSM
posture and must be remediated (operator-mandated, `tuxlink-714t`). Task 7 below
corrects the locked spec's grounding section in place.

## 1. Map rendering substrate (resolves: projection)

**Decision:** Leaflet with `crs={L.CRS.EPSG4326}` (equirectangular / plate
carrée) + a bundled static world image as an `imageOverlay` spanning
`[[-90,-180],[90,180]]`.

Why EPSG4326 not the Leaflet default EPSG3857 (Web Mercator):
- Pixel↔lat/lon is **linear** → the Maidenhead grid overlay draws as a regular
  lon/lat lattice (no Mercator stretch math).
- A plate-carrée world image aligns to `[[-90,-180],[90,180]]` with no
  reprojection.
- The bundled image loads from the app origin (`'self'`) → **CSP stays locked**;
  the webview makes **zero** external requests.

Click → `map` `latlng` → `latLonToGrid(lat,lon)`. Box-drag → two `latlng`
corners → `(lat0,lon0,lat1,lon1)`. **Reuse the existing `maidenhead.ts`** —
do NOT add a second converter.

## 2. Bundled static asset (resolves: asset set + size budget)

- **One** equirectangular (plate-carrée) world PNG. **No** regional-zoom assets
  in v1 (locked spec: "Bundling high-detail regional maps" is out of scope v1).
- **Source:** Natural Earth raster (Natural Earth is **public domain**, no
  attribution required) — a downsampled land/coastline or shaded-relief world
  map. Vendored into the repo as a **frontend asset** (e.g.
  `src/map/assets/world-equirect.png`), imported by the base-map component so
  Vite serves it from `'self'`. Add a `CREDITS`/source note even though PD
  attribution is optional.
- **"Downloaded one time" = vendored by us, shipped in the bundle.** It is NOT
  fetched on first run (that would fail the offline-first requirement on a
  cold/no-network first launch). It ships inside the `.deb`/AppImage.
- **Size budget:** target a single ~2048×1024 PNG, optimized (oxipng/pngquant),
  **< 600 KB**. Hard ceiling 1.5 MB (locked spec: "a few hundred KB to low-MB").
- **Achievable precision:** a 2048-px-wide world image is ~0.18°/px ≈ ~12 nm/px
  at the equator — finer than a 4-char square but coarser than a 6-char
  subsquare. So **bundled picking is reliable to 4-char** (= the broadcast
  default, `feedback_gps_precision_reduction`). **6-char+ precision is an
  explicit opt-in via the permitted tile server (slice 2)**, which supports true
  zoom. This is consistent, not a gap: the default precision is fully served
  offline; finer precision requires a trusted network source the operator opts
  into.

## 3. Maidenhead grid overlay (resolves: overlay tech)

- **SVG** (Leaflet's default vector renderer) via react-leaflet `Polyline`s for
  field/square lines + `Tooltip`/`DivIcon` labels. At world/coarse zoom the
  field grid is 18×18 (~36 lines) — trivially cheap for SVG; a canvas
  `L.GridLayer` is unwarranted at this density. Toggleable via a checkbox
  (default **on**).
- Lines + labels are computed from the visible bounds + zoom (field-level when
  zoomed out, square-level when zoomed in). Pure function → unit-testable
  independent of Leaflet.

## 4. `GridMapPicker` component (the foundation deliverable)

Composition: `<BaseMap>` (§1 substrate + §2 asset) + `<MaidenheadOverlay>` (§3) +
mode-specific interaction.

- **Pin mode** (item 18): single click → marker + grid-square rectangle; readout
  `latLonToGrid`; **"Use"** stores honoring the 4-char broadcast default (finer
  opt-in only when slice-2 zoom is available).
- **GRIB-box mode** (item 21): drag a rectangle → `(lat0,lon0,lat1,lon1)`;
  live readout of the bbox.
- One map, a mode toggle. Manual text entry (`GridEdit` / GRIB lat-lon fields)
  **remains available alongside** — the picker is never blocking.

## 5. Item 21 wiring — `GribRequestPanel` (`tuxlink-mxmx`)

`src/grib/GribRequestPanel.tsx` currently has raw `LatField`/`LonField`
(`lat0/lat1/lon0/lon1`, degrees + N/S/E/W). Add `GridMapPicker` in **box mode**
above/beside the Region section; box-drag sets the four region fields (still
editable by hand). The `GribRequest` `{lat0,lat1,lon0,lon1: {degrees,dir}}`
shape is unchanged — the map writes into it via the existing
`setLat`/`setLon` setters. No change to `useGrib`/Saildocs body building.

## 6. Remediation (`tuxlink-714t`, operator-mandated)

1. **`src/compose/PositionMapWidget.tsx`:** delete the `<TileLayer ...osm...>`
   and the online/offline branching; render `<BaseMap>` (§1, bundled, no
   network) underneath the existing `Marker` + grid-square `Rectangle`.
   **Refactor `PositionMapWidget` to consume the shared `<BaseMap>`** so there
   is one offline base-map source of truth (DRY).
2. **`src-tauri/tauri.conf.json` CSP:** remove
   `https://tile.openstreetmap.org https://*.tile.openstreetmap.org` from BOTH
   `img-src` and `connect-src`. Resulting CSP makes external tile fetches
   impossible at the webview layer (defense in depth behind "no TileLayer in
   code"). Effectively reverts `tuxlink-bt2q`.
3. Update `PositionMapWidget.test.tsx`: drop the `osm-tile-layer` assertion;
   assert **no** `TileLayer` / no external tile URL is requested; assert the
   bundled base renders and click→grid still works.

## 7. Grounding-correction task

Edit the locked spec's "Grounding" section to record the corrected facts (per
the documentation-propagation contract; the spec is the canonical source, so the
correction lands there, with a one-line pointer from this approach doc). Do NOT
silently rewrite history — add a dated "Correction (2026-06-08)" note.

## Decomposition → tasks (for the plan)

**This PR (foundation worktree `bd-tuxlink-z9u4`):**
1. Vendor + optimize the equirectangular world PNG asset (+ source note).
2. `<BaseMap>` shared component (EPSG4326 + bundled imageOverlay + click→latlon). TDD.
3. `<MaidenheadOverlay>` (pure grid-geometry fn + SVG render + toggle). TDD.
4. `<GridMapPicker>` (BaseMap + overlay + pin + box modes + "Use"/4-char default). TDD.
5. Item 21: wire box mode into `GribRequestPanel` (`tuxlink-mxmx`). TDD.
6. Remediation: `PositionMapWidget` → `<BaseMap>` + CSP revert (`tuxlink-714t`). TDD.
7. Grounding correction in the locked spec.

**Deferred (separate issues, NOT this PR):**
- **Slice 2 — opt-in permitted tile server + Rust tile-gatekeeper + Settings URL
  field + cache.** See the OPEN SCOPING QUESTION below.
- **Item 18 pin into Settings→Location / wizard (`tuxlink-urbv`)** — blocked on
  in-progress `tuxlink-9xy1` (Settings→Location host). Wire once `9xy1` lands.

## OPEN SCOPING QUESTION for the adversarial review

**Is "bundled-only" a complete v1, or must the Rust tile-gatekeeper + opt-in
permitted server ship in this PR?**

- **Argument for bundled-only v1 (proposed default):** the operator's mandate is
  "offline-first, never public OSM, ship a static fallback." Bundled-only
  delivers that *completely* — pick a grid/box on an offline map with **zero
  network** — and removes all OSM at once. The gatekeeper only has something to
  gate once an opt-in network source exists; with no tile fetching in slice 1,
  there is nothing to gate yet, so deferring it is **sequencing, not a partial
  slice**. 4-char (default precision) is fully served offline.
- **Argument against (must-include):** the locked spec lists the gatekeeper as a
  core component; `feedback_no_operator_decision_punts_on_polish` + alpha =
  vettedness warns against disabling-and-deferring. Counter: the opt-in server's
  "which hosts are permitted / Geographica integration / auth" is itself a
  locked-spec *open item* with real design surface — a legitimate follow-up, not
  a polish punt.

The review must converge this. If "must-include," the plan gains the
gatekeeper/Settings/cache tasks; if "bundled-only," they become a filed child of
`tuxlink-z9u4`.

## Adversarial review outcomes (2026-06-08 — BINDING corrections, fold into the plan)

5-round review (1 Codex + 4 Claude lenses) — all four lenses returned
`survives_overall: true` (architecture sound: EPSG4326 + bundled `<ImageOverlay>`
+ reuse converter + CSP revert all confirmed). Cross-provider-consensus
corrections that the plan MUST encode:

**C1 [P1] Testability split — jsdom cannot render Leaflet (it is mocked at the
module boundary; `PositionMapWidget.test.tsx:1-20,33-106`).** The plan must
NAME which logic is which:
- **Pure, jsdom-unit-tested, NO Leaflet:** (a) the §3 grid-geometry function;
  (b) a pure EPSG4326 pixel↔lat/lon + bounds→bbox helper (linear:
  `lat = 90 − (py/H)·180`, `lon = (px/W)·360 − 180`) that click/box code calls —
  test THIS, not `map.mouseEventToLatLng`; (c) `signedBboxToGribRegion()` (C3);
  (d) the `signed→{degrees,dir}` converter; (e) `useLocator.slice(0,4)`
  truncation.
- **Component-shape tests (keep the module-mock, label "shape-only"):**
  `<ImageOverlay>` rendered with `bounds={[[-90,-180],[90,180]]}`, Rectangle
  present, `onGridChange`/region-setters fire.
- **Real projection/click/box-drag correctness: grim on real WebKitGTK ONLY** —
  a REQUIRED gate for the BaseMap/GridMapPicker tasks, not just layout. State
  explicitly in the plan: **vitest green ≠ map-correct.**

**C2 [P1] Box-drag is not a Leaflet primitive** (BoxZoom is shift-drag *zoom* +
`fitBounds()`). Specify: `boxZoom={false}`; custom `mousedown`/`mousemove`/
`mouseup` via `useMap`; `map.dragging.disable()` during draw + `enable()` after;
a state-held temp `<Rectangle>` live preview; **suppress the synthetic click
after a drag** (drag-moved flag) so pin-mode doesn't double-fire; pin-click
no-ops while box mode is armed.

**C3 [P1] GRIB region is WHOLE-degree + ordered + signed-split.** Rust contract:
`Latitude.degrees: u8`, `Longitude.degrees: u16` (`src-tauri/src/grib/composer.rs:42-52`);
`types.ts:9,17` says whole degrees; `setLat/setLon` do NOT floor
(`GribRequestPanel.tsx:49-52`) but keyboard `clampDegrees` does (`:361-365`);
`composer.rs:183-263` does NOT reorder and rejects degenerate equal ranges
(`:184-189`). The "no change to useGrib/Saildocs" claim in §5 is **WRONG**. Add a
named pure `signedBboxToGribRegion(c0, c1)`: abs+hemisphere→`{degrees,dir}`,
**floor/ceil outward to whole degrees** (so a sub-degree drag never collapses to
an empty region), normalize `lat0/lat1` + `lon0/lon1` to Saildocs order, handle
equator (lat 0 → canonical N), prime meridian (lon 0 → canonical E), poles,
antimeridian. Unit-test all edges.

**C4 [P1] CSP: a SECOND test pins OSM IN.** `src/compose/positionMapCsp.test.ts`
(committed by `f0a4558`) asserts `imgSrc`/`connectSrc` CONTAIN openstreetmap —
it will FAIL on the revert and the approach doc missed it. Task 6 must INVERT it
to a never-OSM contract: assert NO openstreetmap host in `img-src`/`connect-src`,
AND assert the retain-list still present.

**C5 [P2] Exact post-revert CSP string (paste verbatim):**
`default-src 'self'; connect-src 'self' http://127.0.0.1:*; img-src 'self' data:; style-src 'self' 'unsafe-inline'`
— `img-src 'data:'` is load-bearing (select-dropdown SVGs: `AppShell.css:208`,
`RadioPanel.css:242`); `connect-src http://127.0.0.1:*` is load-bearing (WLE
forms HTTP server: `src-tauri/src/forms/http_server.rs`). Do NOT drop either.

**C6 [P2] EPSG4326 MapContainer config** the proposal omitted: `crs=L.CRS.EPSG4326`,
`maxBounds=[[-90,-180],[90,180]]` + `maxBoundsViscosity`, explicit `minZoom`/
`maxZoom` (cap so you can't zoom past the image's native resolution into
illusory precision), `zoomSnap`, `worldCopyJump:false`, `attributionControl={false}`
(or local prefix — no leafletjs.com link). The "4-char" precision is
**displayed-px/°-dependent** (container-size-dependent: 240px in
`PositionFormV2` vs larger in GRIB) — state precision at a PINNED initial zoom,
not from source-PNG width.

**C7 [P2] Remediation removes the FULL online apparatus**, not just the
`<TileLayer>`: `isOnline` state, the `online`/`offline` window listeners,
`MapInteractor`'s `onTileError`+`isOnline` props + the `tileerror` effect, and
`handleTileError` (`PositionMapWidget.tsx:57-102`). `MapInteractor` collapses to
the click handler.

**C8 [P2] Marker-icon Vite fix is shared.** The `L.Icon.Default` fix lives ONLY
at `PositionMapWidget.tsx:29-38`. Move it into `<BaseMap>` (or a side-effect
`leaflet-icon-fix.ts` both import) or markers break in GridMapPicker + the
refactored widget.

**C9 [P2] Manual fields = hard acceptance criterion.** Keep `LatField`/`LonField`
(GRIB) and the compose grid `<input type="text">` (`PositionFormV2.tsx:193` — NOT
`GridEdit.tsx`, which is the dashboard ribbon) rendered + functional +
keyboard-editable when the map is mounted; the map is an aid, never the only path
(accessibility — a drag gesture is keyboard/SR-opaque). Test it.

**C10 [P2] SCOPE VERDICT — bundled-only, split the issue.** Consensus: bundled-only
is a sound complete-feature boundary (zero network → nothing to gate), BUT this
PR ships **zero tile-server affordance** (no Settings URL field, no "opt in to
finer precision" button) so it cannot read as disable-and-defer. The Rust
tile-gatekeeper + opt-in permitted server is **split into a separate follow-up
issue** (filed, not a sub-task here). `tuxlink-z9u4` is re-scoped to
bundled-only + remediation and must NOT be closed claiming the gatekeeper.

**C11 [P1] Decomposition sequencing (bd dep edges).** `<BaseMap>` (task 2) is
consumed by GridMapPicker (task 4) AND the remediation (task 6) → race if
parallel. Sequence: **task 1 (asset) → task 2 (BaseMap, prop contract FROZEN) →
then tasks 3/4/5/6 may parallelize.** Record edges so two in-progress worktrees
coordinate.

**C12 [P3] Asset provenance pinned (task 1):** name the EXACT Natural Earth
product (PD, no attribution required) — e.g. *Natural Earth II with Shaded
Relief, 1:50m* — exact target dims **2048×1024 plate carrée, full
`[-180,180]×[-90,90]`, NO crop**, the downsample/optimize command, output bytes,
and a sha256. Mirror the working vendored-asset import pattern
(`src/assets/tuxlink-icon.png` ← `HelpTitleBar.tsx:16`).

**C13 [P3] Grounding correction is an APPENDED dated block** ("## Correction
(2026-06-08, agent moss-basalt-hawk)") after the original Grounding section —
do NOT rewrite the locked brainstorm decisions.

**C14 [P3] `<ImageOverlay>` is the declarative react-leaflet v5 component**
(not `L.imageOverlay`); bounds `[[south,west],[north,east]]`.

**C15 [P3] Grid-square rectangle precision branch.** PositionMapWidget's
`is6Char = grid.length === 6` rectangle sizing (`:114-122`) is per-consumer;
decide whether the grid-square rectangle geometry centralizes into
`<BaseMap>`/`<MaidenheadOverlay>` (computed from active grid+precision) for the
DRY goal, or stays per-consumer — and state it.

## Constraints / non-negotiables for every task

- **No RF/transmit path** anywhere here (GRIB requests queue to outbox; map is
  local) → RADIO-1 does not gate; agents may run frontend tests + `tauri dev`.
- **Branch from `origin/main`; read via the worktree (it is at `origin/main`
  HEAD)** — never the 639-behind recovery checkout.
- **Layout/fit must be grim-verified against real Tauri WebKitGTK**, not
  Playwright/Chromium (`feedback_chromium_not_webkitgtk_proxy`); restart
  `pnpm tauri dev` to load frontend changes (Ctrl+R is a no-op in the webview).
- **CSP stays `'self'` for tiles** — the whole point. No task may re-add an
  external `img-src`/`connect-src` tile host.
