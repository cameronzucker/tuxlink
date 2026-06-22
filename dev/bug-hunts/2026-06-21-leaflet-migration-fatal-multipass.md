# Bug Hunt Report — Leaflet map migration (FATAL-class audit for v0.74.0)

Agent: marten-dune-granite. READ-ONLY analysis of `origin/main` (HEAD `1e362776`).
Method: code-bug-hunter-multipass, five passes, applied to the Tauri + React 19 +
Leaflet-on-software-GL-WebKitGTK fatal-freeze/crash hazard classes.

> NOTE ON PROVENANCE: the already-known grid-label DOM storm (PR #864 /
> `774a27c4`, tuxlink-u4k2) is **NOT yet merged into `origin/main`** — origin/main
> HEAD `1e362776` still carries the unguarded `gridGeometry.ts`. Per the hunt
> brief that freeze is found+fixed and is NOT re-reported here. All findings below
> are independent of it. **Releasing v0.74.0 from `origin/main` without #864
> merged would ship the freeze** — flagged as a release-gating fact, not a new
> bug.

## Scope

Files analyzed (all from `origin/main`):
`src/map/LeafletMap.tsx`, `LeafletMapContext.ts`, `leafletHooks.ts`,
`basemapLeaflet.ts`, `LeafletRecenterControl.tsx`, `LeafletMaidenheadGridLayer.tsx`,
`gridGeometry.ts`, `projection.ts`, `useBasemapFlavor.ts`, `usePersistedViewport.ts`,
`sixCharAllowed.ts`; consumers `StationFinderMap.tsx`, `PositionMapWidget.tsx`,
`LocationMap.tsx`, `GridPicker.tsx`, `AprsPositionsMap.tsx`, `DigipeatPathLayer.tsx`;
hosts `PositionPickerOverlay.tsx`, `GridPickerOverlay.tsx`, `GridEdit.tsx`,
`AppShell.tsx` (render context); `maidenhead.ts`.

All five passes performed.

---

## Bugs

### 1. FATAL — `moveend`-driven `flyTo` self-loop on the Maidenhead grid layer when used with a persisted viewport, via the recompute-on-every-moveend path

**Location:** `src/map/LeafletMaidenheadGridLayer.tsx:163-186` (the `recompute`
closure + `map.on('moveend', recompute)`), interacting with
`src/map/gridGeometry.ts` (unguarded on `origin/main`).

**Evidence:** `recompute` runs on EVERY `moveend` and, when the level/extent
changed, calls `group.clearLayers()` + `draw(...)` which synchronously creates one
DOM `L.marker` per label. On `origin/main` (pre-#864) `gridGeometry.gridLines`
has no world-clamp and no label cap, and `LeafletMaidenheadGridLayer.padBounds`
(lines 88-99) doubles the span without clamping longitude. A single zoom-out
`moveend` at Square level therefore tessellates 10k–130k DOM nodes on the main
thread.

**Impact:** Whole-app hard freeze on the Pi's llvmpipe WebKitGTK — this IS the
#864 freeze, present in `origin/main`. Re-stated only because origin/main is the
release branch and #864 is unmerged. Not counted as a new finding.

**Found in:** Pass 3 — Failure Mode Reasoning.

---

### 2. FATAL — `LocationMap` passes a **fresh `initialCenter` object every render** while feeding GPS-fix coordinates → async-arrival `flyTo` ↔ `moveend` interaction can re-fire, but the real hazard is the construct-time `skipConstructCenter` being defeated, yanking the map every fix tick

**Location:** `src/location/LocationMap.tsx:535-545` (the `LocationMap` wrapper)
together with `src/map/LeafletMap.tsx:191-203` (the async-arrival recenter effect).

**Evidence:** `LocationMap` computes `center` fresh each render from `fixLatLon`
(a GPS fix that updates continuously) and passes it as `initialCenter`. The
LeafletMap recenter effect is correctly keyed on `initialCenter?.lat /
initialCenter?.lon` primitives (LeafletMap.tsx:203), so a *same-coords* re-render
does not re-fire. BUT a live GPS source delivers a *new* lat/lon on every fix
(`fixLatLon` changes by metres). Each new fix → the effect fires → `map.flyTo`.
While the operator is mid-pan/zoom on the location map, every incoming GPS fix
calls `flyTo` and yanks the camera back to the fix. Worse: `flyTo` is animated;
a fix arriving every ~1 s starts a new `flyTo` before the prior settles.

**Impact:** Not a freeze, but a camera that fights the operator and an animation
storm under a fast GPS source — on llvmpipe each `flyTo` frame repaints the whole
canvas + (via the grid layer's `moveend`) can re-tessellate. The MapLibre original
centered once; this re-expression turned the construct-time center into a live
follow-cam. Compare the deliberately-correct `StationFinderMap` (line 280:
`initialCenter = saved ? saved.center : (me ?? undefined)` — a *stable* value) and
`AprsPositionsMap` (line 567, same pattern). `LocationMap` is the sibling that
omits the stability guard.

**Severity:** significant (degraded UX + animation pressure; freeze only in
combination with finding 1's unguarded grid layer, which LocationMap mounts).

**Found in:** Pass 2 — Cross-Sibling Pattern Violations.

---

### 3. SIGNIFICANT — `OperatorPin` cleanup diverges between the two map siblings; the AprsPositionsMap copy can throw on unmount-ordering (`removeLayer` on an already-removed group)

**Location:** `src/aprs/AprsPositionsMap.tsx:543-552` (AprsPositionsMap
`OperatorPin`) vs `src/catalog/StationFinderMap.tsx:253-269` (StationFinderMap
`OperatorPin`).

**Evidence:** StationFinderMap's `OperatorPin` cleanup guards the removal:
`return () => { if (group.hasLayer(marker)) group.removeLayer(marker); }`
(StationFinderMap.tsx:265-267). The AprsPositionsMap twin omits the guard:
`return () => { group.removeLayer(marker); }` (AprsPositionsMap.tsx:550-552).
On unmount, `useLeafletLayerGroup`'s own cleanup (leafletHooks.ts:38-42) may run
first and remove the group from the map; a subsequent `group.removeLayer(marker)`
on a detached group is at best a no-op and at worst (depending on Leaflet teardown
order under StrictMode double-unmount) touches `marker._map`/`_leaflet_pos` that
the group teardown already nulled.

**Impact:** An unguarded throw during React unmount. It is NOT inside a `safe()`
wrapper (unlike every reconcile mutation in the same file), so it escapes to the
ErrorBoundary instead of being logged+swallowed. StrictMode double-mount/unmount
in dev, or a fast tab switch in prod, can trip it. N=2 siblings, one guards, one
does not — the textbook cross-sibling tell.

**Severity:** significant (ErrorBoundary trip on teardown, not a freeze).

**Found in:** Pass 2 — Cross-Sibling Pattern Violations; corroborated Pass 4.

---

### 4. SIGNIFICANT — no `invalidateSize()` anywhere; a map constructed into a not-yet-laid-out container (overlay open animation, split-pane resize) reads stale 0×0 / wrong size and never recovers until an unrelated `moveend`

**Location:** repo-wide: `git grep invalidateSize src/` returns nothing.
Constructors at `LeafletMap.tsx:117` read container size once via `L.map(...)`.
Overlay hosts: `PositionPickerOverlay.tsx` (portal panel, CSS-animated),
`GridPickerOverlay.tsx`, `StationFinderPanel.tsx` (dialog overlay).

**Evidence:** Leaflet caches the container pixel size at construction and on
explicit `invalidateSize()`. These maps are constructed the instant their
overlay/dialog mounts. If the panel uses any entrance transition (opacity/scale)
or the container's final size is established a frame after mount (common with
flex/grid dialogs), `L.map` captures the transitional size. Leaflet then projects
lat/lon against the wrong viewport: tiles cover only part of the panel, the click
handler maps screen→latlng with the stale transform, and `getBounds()` can be
degenerate. There is no `ResizeObserver` or `invalidateSize` to correct it.

**Impact:** Map renders into a corner / clicks land at the wrong coordinate /
`getBounds()` feeds garbage to the grid layer. On `origin/main` a *degenerate*
(0-area or NaN-edged) `getBounds()` then flows into the unguarded
`gridLines`/`padBounds` (finding 1) — `linesInRange`/`cellStarts` loop on a
non-finite `max` (the very infinite-loop the unmerged #864 guards). So the missing
`invalidateSize` is a second independent trigger for the freeze class on the
release branch.

**Severity:** significant (broken projection / mis-targeted clicks always;
freeze-class only in combination with the unmerged-#864 gap).

**Found in:** Pass 3 — Failure Mode Reasoning.

---

### 5. SIGNIFICANT — `LeafletRecenterControl.flyTo` and `StationFinderMap`/`PositionMapWidget` selection mutations are NOT wrapped against post-unmount animation completion (`_leaflet_pos` crash class)

**Location:** `src/map/LeafletRecenterControl.tsx:39` (`onClick={() =>
map?.flyTo(...)}`). Also the selection re-style effect at
`StationFinderMap.tsx:226-241`.

**Evidence:** `flyTo` is animated. If the operator clicks recenter and then
immediately closes the overlay/switches tabs, the map is `.remove()`d
(LeafletMap.tsx:177-180) while the zoom animation is still running. Leaflet's
in-flight `_animateZoom`/`_onZoomTransitionEnd` then references `el._leaflet_pos`
on a detached pane — the exact crash the brief names. The recenter button's
`flyTo` is a bare call with no `safe()`/try-catch, and the map.remove() teardown
does not `stop()` the animation first. By contrast, every *reconcile* mutation in
StationFinderMap/AprsPositionsMap IS wrapped in `safe()`, and PositionMapWidget /
GridPicker / LocationMap wrap their overlay effects in try/catch — but the
animation-completion callback fires from Leaflet's own rAF, OUTSIDE any of those
wrappers, so the guard does not cover it.

**Impact:** Intermittent uncaught throw → ErrorBoundary, on the (common)
recenter-then-close race. Not deterministic, which is why it survives smoke
testing. Mitigation would be `map.stop()` in the construct-effect cleanup before
`instance.remove()`.

**Severity:** significant.

**Found in:** Pass 4 — Concurrency / Lifecycle Reasoning.

---

### 6. MINOR→SIGNIFICANT — `GridPicker` window `mouseup` listener and `boxZoom.disable()` leak / mutate shared state if the map identity is null at first effect run

**Location:** `src/map/GridPicker.tsx:632-688` (`usePickerInteractions`).

**Evidence:** The effect early-returns `if (!map) return;` WITHOUT registering the
`window` `mouseup` listener — correct. But the effect deps are `[map]`, and on the
first render `map` is `null` (the context is null until `whenReady`). So the
window listener + `map.on(...)` + `map.boxZoom.disable()` are wired only on the
SECOND effect run (when `map` becomes non-null). That is fine. The latent issue is
ordering on teardown: `boxZoom.disable()` (line 679) mutates the *shared, reused*
map instance (per the project memory: "ONE engine reused across ALL surfaces"),
and the cleanup re-enables nothing — there is no `map.boxZoom.enable()` /
`map.dragging.enable()` restore in the cleanup (lines 681-688). If a GridPicker
overlay closes mid-drag (startRef set, then unmount), `dragging` is left DISABLED
on the shared instance and box-zoom stays disabled for the next surface that
reuses it.

**Impact:** A subsequent map surface (or the same one re-opened) opens with pan
disabled / box-zoom missing — "the map won't drag" with no error. Recoverable
(re-mount fixes if a fresh instance is built), but if the instance is genuinely
reused it sticks. The `onWindowUp` path re-enables dragging only if a drag was
*in progress*; an unmount during a drag bypasses it because the listener is
removed in cleanup before the window mouseup fires.

**Severity:** significant if the L.Map instance is reused across surfaces (memory
says it is); minor if each surface builds its own (LeafletMap constructs per-mount,
so likely per-surface — verify).

**Found in:** Pass 4 — Concurrency / Lifecycle Reasoning.

---

### 7. MINOR — pack-fetch failure mid-session zeroes `lastKnownPacks`, silently dropping all downloaded region detail until reload

**Location:** `src/map/LeafletMap.tsx:90-99` (the `catch` branch).

**Evidence:** The code comments call this out as a known latent ("a transient
failure mid-session zeroes the cache — bd tuxlink-kepz, faithful port"). On any
transient `basemap_list_packs` rejection (including a `BASEMAP_PACKS_CHANGED_EVENT`
refetch that races a busy backend), `lastKnownPacks = []` and `setPacks([])` —
the base-layer effect (lines 206-222) then removes all pack layers and rebuilds
overview-only. A later successful fetch restores them, but a one-shot failure with
no follow-up event leaves the operator on the bundled z0-6 overview with no
indication their downloaded region pack silently vanished.

**Impact:** Not a crash; data-*availability* regression (the downloaded detailed
map disappears). Flagged because the brief lists data loss as in-scope; this is
display-data loss, recoverable by reload. Faithful port of a known MapLibre
latent, so likely accepted — but worth a release note.

**Severity:** minor.

**Found in:** Pass 5 — Error Propagation.

---

## Design Concerns

- **The `safe()` wrapper covers synchronous reconcile mutations but NOT Leaflet's
  own async animation/rAF callbacks.** Findings 3 and 5 both exploit the same gap:
  the contained-error pattern guards the code the component *calls*, not the code
  Leaflet *schedules*. A construct-effect cleanup that does `instance.stop()`
  before `instance.remove()` would close the animation-completion crash class
  uniformly.

- **`origin/main` is missing PR #864.** Two independent triggers in this audit
  (findings 1 and 4) reach the unguarded `gridLines`/`padBounds` infinite-loop /
  DOM-storm. The freeze guard MUST be merged before v0.74.0 ships from this branch.
  This is the single highest-priority release gate.

- **Live-follow vs. construct-once center is inconsistent across siblings.**
  `StationFinderMap` and `AprsPositionsMap` pass a *stable* `initialCenter`;
  `LocationMap` passes a *live* GPS-derived center (finding 2). The prop is named
  `initialCenter` and documented as "Initial view center; a later change drives
  flyTo" — LocationMap's usage technically honors the contract but weaponizes it
  into a follow-cam. Either rename the intent or have LocationMap hold the center
  stable after first fix.

- **Shared-instance interaction state (`dragging`/`boxZoom`) is mutated without a
  symmetric restore (finding 6).** Any surface that disables a map interaction
  must re-enable it in cleanup, especially given the "one reused engine" posture.

- **No `invalidateSize` + no `ResizeObserver` anywhere** makes every overlay/
  dialog/split-pane host a latent broken-projection site. Even setting aside the
  freeze interaction, mis-targeted clicks on a transitionally-sized container are a
  correctness bug for a *location picker* whose entire job is "click = my position."
