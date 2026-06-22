# Bug Hunt Report — Leaflet map migration, FATAL-focus holistic pass

Agent: marten-dune-granite. Read-only audit of `origin/main` (working tree is
behind; every file read via `git show origin/main:<path>`). Pre-v0.74.0 prod cut.

## Scope

Read in full from `origin/main`: the shared provider (`src/map/LeafletMap.tsx`,
`LeafletMapContext.ts`, `leafletHooks.ts`, `basemapLeaflet.ts`,
`LeafletRecenterControl.tsx`), the lattice overlay
(`LeafletMaidenheadGridLayer.tsx` + pure `gridGeometry.ts`), the four surfaces
(`AprsPositionsMap.tsx` + `DigipeatPathLayer.tsx` + `digipeatAnim.ts`,
`StationFinderMap.tsx`, `PositionMapWidget.tsx`, `LocationMap.tsx`,
`GridPicker.tsx`), the modal mount sites (`PositionPickerOverlay.tsx`,
`GridPickerOverlay.tsx` + their CSS), and the adjacent pure/util modules
(`maidenhead.ts`, `projection.ts`, `useBasemapFlavor.ts`,
`usePersistedViewport.ts`, `sixCharAllowed.ts`, `aprsSprites.ts`,
`frontendErrorLog.ts`).

Approach: the engine inverts MapLibre's GPU-cheap-at-scale cost model to O(N)
DOM. The already-fixed freeze (PR #864 / commit `774a27c4`, the grid LABEL
storm) was bounded at the `gridLines()` source with a label cap + world clamp +
non-finite guard. This pass reasons about whether that cap is *complete* and
whether any OTHER O(N)-DOM / infinite-loop / teardown-crash path survives.

## Bugs

### Lattice LINE count is unbounded at Subsquare level — the PR #864 cap covers labels only
**Location:** `src/map/gridGeometry.ts:150-188` (`gridLines`), consumed by
`src/map/LeafletMaidenheadGridLayer.tsx:85-107` (`draw`).
**Severity:** significant (latent; not reachable from the two shipping consumers — see Impact).
**Evidence:** PR #864 added `MAX_GRID_LABELS = 2000` and gates only the LABEL
cross-product:
```ts
if (lonCells.length * latCells.length <= MAX_GRID_LABELS) { ...push labels... }
```
The `lonLines`/`latLines` arrays are still produced and returned unconditionally,
and `draw()` creates one `L.polyline` (one SVG `<path>` DOM node) per line with
no cap. The world clamp added in #864 bounds Field (≤38 paths) and Square (≤362
paths) views, but **does not bound Subsquare** (step 5′×2.5′). A Subsquare-level
call over a world-width window produces 4321 lon + 4321 lat = **8642 SVG path
nodes** synchronously (verified by re-running the post-fix line math). That is
the same class of synchronous-DOM-storm that froze WebKitGTK with the labels,
just expressed in `<path>` instead of marker `<div>`.
**Impact:** A Subsquare-level recompute over a wide window would re-freeze the
software-GL webview. In the **shipping** configuration this is NOT reachable:
both real consumers (`LocationMap`, `GridPicker`) mount
`<LeafletMaidenheadGridLayer visible />` with no `level`/`bounds` prop, so
`level` is always `levelFromZoom(map.getZoom())`, which only returns Subsquare at
zoom ≥ 9 — where the geographic viewport (even doubled by `padBounds`) spans a
few degrees (~68 paths, fine). The freeze is gated behind the `level`/`bounds`
override props, which exist for "controlled / testing" and are label-protected
but not line-protected. **Disposition:** not a v0.74.0 blocker on its own, but it
is a defense-in-depth asymmetry — the label cap implies the author intended to
bound the storm, and the lines slipped through. Recommend capping lines too
(e.g. bail to no-overlay, or cap each axis) so the controlled-prop path can't
reintroduce the exact bug #864 fixed.

## Design Concerns

### No `invalidateSize` / `ResizeObserver` on any Leaflet map — fragile if a modal ever gains an open-transition
`grep` over `origin/main` finds zero `invalidateSize` calls in the map subsystem.
The two modal-mounted maps (`GridPickerOverlay`, `PositionPickerOverlay`) are
currently SAFE because their map containers carry fixed pixel/vh heights
(`height: 360px` / `height: 46vh; max-height: 460px`) and the dialogs render at
full size immediately (no width/height open-animation in the CSS) — so Leaflet
constructs with correct dimensions. This is a latent trap, not a current bug: if
a future polish pass adds a scale/expand open-transition (common for dialogs),
the map will construct at the pre-animation size and render a half-painted /
mis-tiled viewport with no self-heal. Worth a one-line `map.invalidateSize()` on
a `transitionend`, or a note, before anyone animates these panels.

### `usePersistedViewport` debounce timer is never cleared on unmount
`src/map/usePersistedViewport.ts:82-101` holds a 300 ms `setTimeout` in
`timerRef` but has no `useEffect` cleanup to clear it on unmount. Worst case is a
single stray `localStorage.setItem` firing after the map is gone — the payload is
finite-validated and wrapped in try/catch, so it is harmless (not a leak that
accumulates, since the next `onViewportChange` clears the prior timer). Cosmetic;
noting for completeness, not a defect that needs fixing for the release.

## Cleared (reasoned-through, NOT bugs)

- **Map-event self-loops:** no handler mutates the map in a way that re-fires its
  own trigger. The grid `recompute` (on `moveend`) does `clearLayers` + `draw`
  only; the `GridPicker` rubber-band (`onMove` → `setTemp` → reconcile) is one
  bounded rectangle per `mousemove` (expected). The two `LeafletMap` `moveend`
  handlers (`emitZoom`, viewport-persist) terminate (dedup'd / debounced). The
  MapLibre `setFilter`-on-`styledata` "drunk map" self-loop class is structurally
  gone with the engine (no `styledata`, no per-frame style mutation).
- **DigipeatPathLayer rAF:** bounded — `traceProgress` reaches `phase: 'done'`,
  the loop deletes finished traces and returns without re-scheduling when the set
  empties (`rafRef.current = 0`); canvas teardown cancels the in-flight frame.
  `trimPath`/`lerp` are finite, no loops. No perpetual-rAF / leak.
- **Non-finite projection math:** `clampMapCenter` coerces non-finite to 0;
  `gridLines` bails empty on non-finite bounds (PR #864); viewport reader rejects
  non-finite/out-of-range stored values. The infinite-loop in
  `linesInRange`/`cellStarts` on `Infinity` max is guarded at the source.
- **lat/lon ordering:** `clampMapCenter(lon, lat)` returns `[lng, lat]`,
  destructured `[clLng, clLat]`, then passed to Leaflet as `[clLat, clLng]` —
  correct at both the constructor (`LeafletMap.tsx:113/128`) and the async-arrival
  `flyTo` (`:202-203`).
- **Teardown ordering / `el._leaflet_pos`:** child overlay effect cleanups run
  before the parent `LeafletMap`'s `instance.remove()` (React bottom-up unmount
  with last-committed values, so `map` is still live in the child cleanup);
  `useLeafletLayerGroup` guards with `map.hasLayer(lg)`; every imperative layer
  mutation in the surfaces is wrapped in a `safe()`/try-catch that logs+skips a
  mid-animation throw rather than crashing the ErrorBoundary.
- **Per-station O(N):** `positions` (heard APRS) and `stations` (catalog) are
  bounded by real traffic/catalog size (hundreds), each making a small fixed
  bundle of layers, reconciled in place (no churn). Not a storm.
- **`reportFrontendError`:** fire-and-forget, swallows its own throw — no error
  feedback loop.
