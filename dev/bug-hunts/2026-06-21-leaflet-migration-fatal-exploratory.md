# Bug Hunt Report — Leaflet Map Migration (FATAL-class audit, pre-v0.74.0)

Agent: marten-dune-granite · 2026-06-21 · exploratory depth-first
Read target: **origin/main** (working tree is behind; all citations are `git show origin/main:<path>`).

## Scope

Audited the merged Leaflet map substrate + all migrated consumers for FATAL bugs
(whole-app freeze, crash, data loss) on the Pi's software-GL WebKitGTK (llvmpipe),
where synchronous DOM explosions or infinite loops hard-freeze the app.

Files read in full from origin/main: `src/map/LeafletMap.tsx`,
`LeafletMapContext.ts`, `leafletHooks.ts`, `basemapLeaflet.ts`,
`LeafletRecenterControl.tsx`, `LeafletMaidenheadGridLayer.tsx`, `gridGeometry.ts`,
`projection.ts`, `useBasemapFlavor.ts`, `usePersistedViewport.ts`,
`forms/position/maidenhead.ts`, `catalog/StationFinderMap.tsx`,
`compose/PositionMapWidget.tsx`, `location/LocationMap.tsx`, `map/GridPicker.tsx`,
`aprs/AprsPositionsMap.tsx`, `aprs/DigipeatPathLayer.tsx`, `aprs/digipeatAnim.ts`,
`aprs/digipeatPath.ts`. Mount sites read: `PositionPickerOverlay.tsx`,
`GridPickerOverlay.tsx`, `GribForm.tsx`, `GridEdit.tsx`, `StationFinderPanel.tsx`,
`GpsSourcePicker.tsx`, `AppShell.tsx` (relevant fragments).

Deep-explored threads: (1) the Maidenhead grid O(N) DOM explosion class
(the #864 family); (2) map-event self-loops; (3) the DigipeatPathLayer rAF
termination contract; (4) teardown-during-animation; (5) the GridPicker drag
gesture stuck-state class; (6) flyTo storms from live data; (7) StationFinder pin
fan-out.

---

## Bugs

### 1. Maidenhead grid label explosion → whole-app freeze IS PRESENT on origin/main (PR #864 NOT merged)
**Location:** `src/map/LeafletMaidenheadGridLayer.tsx:94-100` (`padBounds`), `:103-109` (`draw` → one `L.marker` per label), `src/map/gridGeometry.ts:122-135` (`gridLines` / `cellStarts`, no cap)
**Severity:** critical
**Evidence:** The prompt states the freeze "is fixed by clamping/capping gridLines in PR #864." Verification shows **PR #864 is OPEN, not merged** (`gh pr view 864` → `"state":"OPEN","mergedAt":null`, head `bd-tuxlink-u4k2/grid-label-freeze`). `git show origin/main:src/map/gridGeometry.ts` and `LeafletMaidenheadGridLayer.tsx` contain **no cap/clamp** — `git grep -niE "cap|clamp|max|limit"` over both returns only the `padBounds` lat clamp and prose. At Square level (`levelFromZoom` z3–8) with a wide viewport, `padBounds` doubles the span and does NOT clamp longitude, so `cellStarts(-360,360,2) × cellStarts(-90,90,1)` ≈ **64,800 `L.marker` divIcons** created synchronously in `draw`. That is the exact freeze the prompt describes.
**Impact:** Whole-app hard freeze. **NOT a new finding** — this is the #864 bug. It is flagged here only because **the fix is not on the branch being released**: if v0.74.0 ships from current origin/main, the freeze ships with it. The release MUST include the #864 merge (or its equivalent cap) before tagging. Reachable via `LocationMap` and `GridPicker` (both mount `LeafletMaidenheadGridLayer`) once the operator zooms into the z3–8 band and pans to a wide extent.

### 2. `padBounds` does not clamp longitude → over-generation even within the capped level
**Location:** `src/map/LeafletMaidenheadGridLayer.tsx:94-100`
**Severity:** significant (amplifier of #1; independently worth fixing)
**Evidence:** `padBounds` clamps latitude to ±90 (`Math.max(-90, …)`, `Math.min(90, …)`) but leaves longitude unbounded: `west: b.west - dLon, east: b.east + dLon`. A world-spanning view (`west≈-180, east≈180`, `dLon≈180`) yields a padded `[-360, 360]` lon window — **double** the real world width — so `gridLines`/`cellStarts` generate ~2× the lon lines and labels that the actual antimeridian-bounded world needs. Even after #864 caps the label count, the line polylines (`draw`, lines 103-109) are uncapped and this still doubles their count. At Square level that's 360 lon polylines instead of ~180.
**Impact:** 2× the geometry the viewport needs at every level; compounds #1 and wastes paint on llvmpipe. Lower-severity on its own (lines are cheap vs. divIcon markers) but a latent multiplier. Fix: clamp padded lon to [-180, 180] mirroring the lat clamp.

### 3. `LeafletMap` never calls `invalidateSize` / observes container resize
**Location:** `src/map/LeafletMap.tsx:104-188` (construct effect — no `ResizeObserver`, no `invalidateSize`); confirmed absent project-wide (`git grep invalidateSize\|ResizeObserver origin/main` hits only `WebviewFormHost`/`WebviewFormViewer`, unrelated form-embed code, and test stubs)
**Severity:** significant (not fatal in the audited mount sites; latent for future ones)
**Evidence:** Leaflet (unlike MapLibre) does NOT auto-track container size changes. If a map is constructed in a zero-size or later-resized container, its panes get wrong pixel offsets and tiles/markers render clipped or mispositioned until a manual `invalidateSize()`. The migrated consumers currently dodge this: the two overlays (`PositionPickerOverlay`, `GridPickerOverlay`) mount the map only when the portal is open inside a fixed-height panel (`min-height:280-320px`, `height:360px`/`46vh` per their CSS), and `AprsPositionsMap`/`StationFinderMap` mount in already-laid-out grid slots. So at present this is not a live freeze/crash.
**Impact:** No fatal symptom in the shipped mount sites today, BUT the contract is fragile: any future consumer that mounts `LeafletMap` inside a collapsed accordion, a `display:none` tab that flips visible, or a flex child that sizes-up after creation will get a broken/blank map with offset panes and no recovery path. Recommend adding a `ResizeObserver`-driven `invalidateSize()` in `LeafletMap` as a structural guard. Marked significant because the engine's known footgun is left unguarded at the substrate level.

### 4. Live-GPS `flyTo` churn in LocationMap (camera yank, not freeze)
**Location:** `src/map/LeafletMap.tsx:194-205` (flyTo effect, deps `[map, initialCenter?.lat, initialCenter?.lon]`); `src/location/LocationMap.tsx:535-545` (`center = showFix ? fixLatLon : ll`)
**Severity:** minor
**Evidence:** The flyTo effect dedupes only on *identical* primitive coords. With a GPS source selected, `LocationMap` passes the live `fixLatLon` as `initialCenter`; a moving/jittering fix produces genuinely-different lat/lon on each update, re-firing `map.flyTo(...)` every time. Each `flyTo` calls Leaflet's `_stop()` on the prior animation, so they do **not** stack into an unbounded animation queue (no freeze).
**Impact:** Camera repeatedly yanks back to the fix while the operator tries to pan, until they switch to Manual. Annoying UX, recoverable, not fatal. Noted for completeness; a small distance threshold before flyTo would fix it.

---

## Non-bugs verified (threads that looked risky but are sound)

- **Map-event self-loops:** `LeafletMap`'s two `moveend` handlers (`emitZoom`, viewport-persist; lines 152-167) only READ the map (`getZoom`/`getCenter`) and call out — they never `setView`/`flyTo`/`fitBounds`, so no moveend→mutate→moveend loop. The grid layer's `moveend` `recompute` (`LeafletMaidenheadGridLayer.tsx:154-180`) mutates only the overlay LayerGroup, not the map camera — no self-loop. The MapLibre "drunk map" `setFilter`-on-`styledata` class is structurally absent (Leaflet has no `setStyle`/`styledata`).
- **DigipeatPathLayer rAF termination:** `traceProgress` (`digipeatAnim.ts:25-42`) always returns `phase:'done'` past `fadeEnd`; the loop prunes done traces and sets `rafRef.current = 0` + returns when `tracesRef.size === 0` (`DigipeatPathLayer.tsx:809-817`). It is pure arithmetic and cannot throw, so the `safe('draw frame')` wrapper can't strand a non-prunable trace. Loop is genuinely bounded.
- **Teardown during animation:** `LeafletMap`'s cleanup sets `removed = true` then `instance.remove()` (lines 174-177); `map.remove()` internally `_stop()`s in-flight pan/zoom animations, and `whenReady`/the moveend handlers guard on `removed`. Overlay layer mutations are wrapped in `safe()`/try-catch that route to `reportFrontendError` rather than the ErrorBoundary. The `_leaflet_pos undefined` crash class is mitigated.
- **StationFinder pin fan-out:** one `L.circleMarker` (SVG path, one element, no per-frame relayout) per *band/mode-filtered* Winlink RMS gateway (`StationFinderMap.tsx:180-208`, fed by `StationFinderPanel` `visible`, lines 142-160). Real RMS gateway counts are hundreds–low-thousands and pre-filtered; SVG circles at that scale are not a freeze. Bounded by real data, not by zoom/bounds. Watch, not a bug.
- **GridPicker drag stuck-state:** `usePickerInteractions` (`GridPicker.tsx:626-689`) re-enables `map.dragging` on on-map mouseup AND on a window-level mouseup for off-canvas release. `mode` is static per consumer (GribForm `'box'`, GridEdit `'pin'`) — no mid-drag mode switch is reachable, so the `startRef`/`draggedRef` stuck path doesn't trigger. The only residual gap (a child calling `stopImmediatePropagation` on the release mouseup before it reaches window) is theoretical and minor.
- **Non-finite projection math:** `clampMapCenter` (`projection.ts:88-97`) coerces NaN/Infinity to 0 before clamping; `usePersistedViewport.readSaved` rejects non-finite/out-of-range stored values; the viewport-persist handler skips non-finite centers (`LeafletMap.tsx:162-167`). The map uses default CRS (EPSG:3857) — `projection.ts`'s EPSG4326 prose is vestigial but does not feed the live map (protomaps-leaflet handles tiles), and `maxBounds` ±85.0511 matches `MERCATOR_MAX_LAT`. No NaN→infinite-loop path found.
- **Listener leaks:** every `map.on` in the audited overlays has a matching `map.off` in cleanup (grid layer 178-180, GridPicker 681-688); window listeners removed (`LeafletMap.tsx:98-101`, GridPicker 644/682). `useLeafletLayerGroup` guards `map.hasLayer(lg)` before removal. No leak found.

---

## Design Concerns

- **`gridGeometry`/`LeafletMaidenheadGridLayer` have no intrinsic output cap** (Bug 1/2). Even post-#864, the only guard is the per-call cap inside the pending PR; the geometry functions themselves remain happy to enumerate an unbounded grid if a caller passes wide bounds or a controlled `bounds`/`level` prop. A defense-in-depth cap inside `gridLines` (return early past N lines/labels) would make the freeze class structurally impossible regardless of caller, instead of relying on each consumer staying within safe zoom/bounds.
- **`invalidateSize` gap at the substrate** (Bug 3): the safety currently rides on every consumer happening to mount in a pre-sized container. That invariant is undocumented and easy to violate in the next migration. The guard belongs in `LeafletMap`, not in each call site's discipline.
- **`lastKnownPacks` module-scope cache zeroes on transient backend failure** (`LeafletMap.tsx:32, 90-94`): the `catch` sets `lastKnownPacks = []`, so a transient `basemap_list_packs` failure mid-session drops installed region packs from the cached composite for the next remount until a successful refetch. Self-noted in the source as a faithful port of bd tuxlink-kepz (latent, not a freeze/crash); flagged only as a known fragility, not a new bug.

## Release gate

The single must-fix before tagging v0.74.0 is **landing PR #864 (or an equivalent
grid-output cap) — it is OPEN, and origin/main currently ships the freeze.** Bugs
2-3 are strong follow-ups; Bug 4 and the design concerns are non-blocking.
