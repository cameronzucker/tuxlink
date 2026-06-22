# Leaflet Migration FATAL-class Bug Hunt — Consolidated Findings

**Date:** 2026-06-21
**Scope:** The just-merged Leaflet map migration on `origin/main` (PRs #855/#857/#858/#861 + substrate teardown #862) — shared substrate (`LeafletMap`, `LeafletMapContext`, `leafletHooks`, `basemapLeaflet`, `LeafletRecenterControl`, `LeafletMaidenheadGridLayer`) + four surfaces (`StationFinderMap`, `PositionMapWidget`, `LocationMap`, `GridPicker`) + `gridGeometry`. Target: Tauri + React 19 + Leaflet on the Pi's software-GL WebKitGTK.
**Hunters:** Exploratory, Holistic, Multipass (agent marten-dune-granite, 3 parallel)
**Question being answered:** Before promoting v0.74.0, is there a SECOND fatal bug (whole-app freeze / crash / data loss) beyond the grid-label freeze already fixed in PR #864?

## Headline

**No second reachable whole-app freeze.** All three hunters independently cleared the freeze class. The #864 label-storm fix is the only must-land-before-tag item. The remaining findings are real but non-fatal (recoverable UX, a logged uncaught throw, low-probability StrictMode throw) or latent defense-in-depth not reachable from any shipping consumer.

**Release-gating fact (all three flagged):** PR #864 is not yet merged to `origin/main`; the freeze is still in the branch a v0.74.0 tag would cut from. The tag MUST come after #864 merges.

---

## Confirmed Bugs (new — beyond #864)

### B1. LocationMap is a live "follow-cam" — flyTo churn on every GPS fix
**Consensus:** Multipass (significant) + Exploratory (minor).
**Location:** `src/location/LocationMap.tsx` (`center` from live `fixLatLon` → `initialCenter`) + `src/map/LeafletMap.tsx` async-arrival recenter effect (`flyTo` keyed on `initialCenter?.lat/lon`).
**Evidence:** With a GPS source active, `fixLatLon` updates every tick; LocationMap passes it as `initialCenter`, so the recenter effect fires `flyTo` on every fix. The sibling maps (`StationFinderMap`, `AprsPositionsMap`) deliberately pass a STABLE center; LocationMap omits that guard.
**Impact:** Camera yanks back to the fix continuously — the operator can't pan to hand-set location until switching to Manual. NOT a freeze (each `flyTo` `_stop()`s the prior), recoverable. Significant UX defect for a location *picker*.
**Blast radius:** LocationMap only; fix = derive a stable initial center (grid-based) and let the marker (not the camera) track the live fix.
**Fix approach:** Pass a one-time/stable `initialCenter`; the overlay already moves the marker via `markerLat/markerLon`.

### B2. Teardown-during-zoom-animation crash (`_leaflet_pos`) — recenter-then-close
**Consensus:** Multipass. **Corroborated by the operator's runtime log** (`undefined is not an object (evaluating 'el._leaflet_pos')` at `_onZoomTransitionEnd`).
**Location:** `src/map/LeafletMap.tsx:179-180` (cleanup `instance.remove()` with no `instance.stop()` first); `src/map/LeafletRecenterControl.tsx:36` (bare animated `map.flyTo([...], zoom)`).
**Evidence:** Closing the map (unmount) while a recenter `flyTo` zoom animation is in flight: Leaflet's animation-completion callback fires from its own rAF after the pane is gone → `el._leaflet_pos` undefined throw, outside every `safe()` wrapper.
**Impact:** Uncaught throw → `window.error` (logged, observed). Animation aborts. Not confirmed to crash React (fires outside render), so moderate — but it's a real, observed defect.
**Blast radius:** Shared substrate; affects every surface with the recenter control.
**Fix approach:** `instance.stop()` before `instance.remove()` in the construct-effect cleanup (one line); optionally guard the recenter `flyTo`.

### B3. AprsPositionsMap OperatorPin removal unguarded (sibling divergence)
**Consensus:** Multipass.
**Location:** `AprsPositionsMap.tsx` (~`:550`) bare `group.removeLayer(marker)` vs `StationFinderMap.tsx:265-267` which guards with `group.hasLayer(marker)`; the Aprs path is not inside a `safe()` wrapper.
**Impact:** An unmount-ordering throw (StrictMode double-unmount / fast tab switch) escapes to the ErrorBoundary. Low probability, not fatal. (AprsPositionsMap is the migration *template*, shipped earlier — adjacent, not one of the four PR surfaces.)
**Fix approach:** Guard with `hasLayer` or wrap in `safe()` (match the StationFinderMap sibling).

---

## Latent / Defense-in-Depth (NOT reachable in shipping config; NOT v0.74.0 blockers)

### D1. `gridLines` LINE count uncapped — #864 capped only labels
**Location:** `src/map/gridGeometry.ts` (post-#864: world-clamp + `MAX_GRID_LABELS` cap on labels, none on `lonLines`/`latLines`).
**Concern:** At Subsquare level over a world-width window, `linesInRange` returns ~8,640 values → one SVG `<path>` each (same DOM-storm class as the fixed label bug).
**Reachability:** NOT reachable from the two shipping consumers (`LocationMap`, `GridPicker` mount `<LeafletMaidenheadGridLayer visible />` with no `level`/`bounds` override; Subsquare only fires at zoom ≥9 where the viewport is geographically tiny → ~68 paths). Only the controlled-prop override path (testing) can hit it.
**Recommendation:** Add a symmetric line cap for defense-in-depth. Cheap; closes the asymmetry the author clearly intended to bound.

### D2. No `invalidateSize`/`ResizeObserver` anywhere in the map subsystem
**Location:** `src/map/LeafletMap.tsx` (construct-once; no resize tracking). Confirmed absent project-wide by all three hunters.
**Concern:** Leaflet (unlike MapLibre) does not auto-track container resize. A map mounted in a zero-size / late-sizing / open-animated container gets stale pane offsets → blank/offset render + mis-targeted clicks (would be fatal for a location picker) and, pre-#864, a degenerate `getBounds()` feeding the infinite loop.
**Reachability:** Current mount sites (PositionPickerOverlay, GridPicker host, GpsSourcePicker, StationFinderPanel) are fixed-height pre-sized portals with no open-animation → SAFE NOW. Latent trap for any future accordion/dialog-scale/hidden-tab consumer.
**Recommendation:** Add a `ResizeObserver`→`invalidateSize` to LeafletMap (engine-level, not per-consumer). Cheap; removes a whole future class.

---

## False Positives

### FP1. "GridPicker leaves map.dragging/boxZoom disabled on unmount"
**Flagged by:** Multipass (hedged: "verify whether the L.Map is reused").
**Why invalid:** `LeafletMap.tsx:179-180` `instance.remove()` destroys the map per-mount; a fresh instance is constructed on the next mount. Disabled interaction state on a removed instance is never observed. The map is NOT reused across surfaces. (Exploratory independently cleared this.)

---

## Out of Scope / Pre-existing

### O1. `lastKnownPacks` zeroes on transient pack-fetch failure
**Location:** `src/map/LeafletMap.tsx:90-99`. Documented latent (tuxlink-kepz, faithful port). Drops region detail to overview-only until reload; recoverable display-data loss, not a freeze.

### O2. `usePersistedViewport` debounce timer not cleared on unmount
**Location:** `usePersistedViewport.ts:~82-101`. Harmless (one finite-validated, try/catch'd localStorage write at worst); cosmetic.

---

## Verdict for v0.74.0

- **Must land before the tag:** PR #864 (the grid-label freeze fix). In progress; `verify` green both arches, `build-linux` finishing.
- **No second fatal freeze blocks the release.**
- **B1 + B2 are cheap and B2 was observed in the operator's logs** → recommend bundling into one quick hardening PR before promote for a genuinely clean 0.74.0; B3 + D1 + D2 are cheap same-area additions. Alternatively ship 0.74.0 on #864 alone and fast-follow — defensible since none of B1/B2/B3 are fatal.

## Test Gap Analysis (brief)

- **B1/B2:** jsdom can't render a real Leaflet map or run zoom animations, so unit tests (which assert wiring) can't catch a follow-cam flyTo loop or an animation-teardown race. These are grim/real-app-only behaviors → the gate is code review + the operator's converged-build smoke, not CI. Catch-tests are limited to asserting the *wiring* (LocationMap passes a stable center; cleanup calls `stop()` before `remove()`).
- **#864 (already done):** caught by a pure-geometry test asserting bounded `gridLines` output + termination on non-finite bounds — the right level, since the DOM cost lives in the pure cell-count.
