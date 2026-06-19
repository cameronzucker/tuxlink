# APRS on-map weather overlay + category filter (ni5b) — design

- **Issue:** tuxlink-ni5b (epic tuxlink-18q2, "Full APRS rich experience")
- **Date:** 2026-06-19
- **Status:** approved (design); pending implementation plan
- **Author:** mink-yew-osprey

## Summary

On the APRS Tac Chat positions map, render heard weather stations with their
**real** readings instead of an information-less symbol pin, and add a
station-category filter ("weather mode") that shows only weather stations for a
clean dashboard. A compact temperature-led **badge** is the default render;
**hover** expands to a full WX card; **click** navigates to the station's
existing Station Data telemetry card (history + full detail). An optional PNG
snapshot of the map (basemap + badges + a situation-report header) is
exportable.

The category filter is built **generic** — categories are a list of
`{ key, label, predicate }` — so tuxlink-8fjx (vehicles / digipeaters / iGates /
…) later extends it by adding predicates, not rewriting. Weather is the first
category.

## Decisions locked in brainstorm (visual companion + terminal, 2026-06-19)

1. **Badge (A) is the default render** for weather stations, replacing the
   generic `_`-symbol pin. **Hover → full WX card (B).** **Click → Station Data
   telemetry card** for that callsign.
2. **"Weather mode" = a station-category filter + export**, not a separate pin
   style. Filtering to Weather hides non-weather stations → clean dashboard.
   Built category-generic (the tuxlink-8fjx generalization).
3. **No heatmap / interpolation** — plotting a smooth surface between stations
   fabricates data the radio never carried (RF-honesty, same bar as cn84).
4. **Export split:** ni5b ships the optional **PNG map snapshot**; the heard-WX →
   Winlink **text report** is **tuxlink-hepq** (next phase), which may attach
   this PNG. Images are large, so the PNG is on-demand/optional.

## RF-honesty: the badge carries only what was heard

APRS weather reports (`WeatherReportDto`) carry temperature, wind
(dir/speed/gust), humidity, pressure, rain (1h/24h/since-midnight), luminosity,
and snow — **but no sky-condition field**. There is no honest source for
"sunny / cloudy". Therefore:

- The badge is **temperature-led**: `72°F` when `temperatureF` is present.
- A condition glyph appears **only when a real field supports it**: `🌧` when
  `rain1hIn > 0` (actively raining); a wind glyph when wind is notable
  (`windSpeedMph >= 20`, say). **Never an assumed ☀.**
- When `temperatureF` is null, the badge shows the most salient present reading
  (wind, else the first available channel), so the pin still says something true.
- A weather station with a position but a not-yet-decoded reading shows its
  ordinary symbol pin until a WX frame arrives.

This mirrors the map's existing honesty posture (ambiguity regions, stale-dimming,
cn84's dashed unknown-hop markers): show exactly what the wire carried, nothing more.

## Components

### 1. Data join + classification — `src/aprs/wxStations.ts` + `useWxStations`

`EnvStation` (`useEnvStations`) carries WX/telemetry by callsign but **no
position**; `HeardPosition` (`useAprsPositions`) carries lat/lon by callsign.

- Pure `joinWxStations(envStations, positions): WxStation[]` — inner-join on
  callsign; a `WxStation { call, lat, lon, wx: WeatherReportView, at }` exists
  only when a station has **both** a position and a weather reading.
- `useWxStations()` — thin hook composing `useEnvStations()` + `useAprsPositions()`
  through the pure join (no new backend subscription; both hooks already mount at
  the shell top level).
- A pure `badgeContent(wx): { primary: string; glyph: string | null }` encodes
  the honesty rules above (temp-led; glyph only when derivable).

### 2. Category filter — `src/aprs/stationCategories.ts`

- `Category { key: string; label: string; matches(p: HeardPosition, wx?: WeatherReportView): boolean }`.
- v1 defines two entries: `all` (always true) and `weather` (`wx != null`).
  Designed so 8fjx adds `vehicles` / `digipeaters` / `igates` (symbol-derived
  predicates) without touching the filter mechanism.
- A small `WxFilterControl` (map control) selects the active category; default
  `all`. Selecting `weather` is "weather mode".

### 3. Map render — extend `AprsPositionsMap.tsx`

- **Badge layer:** an additional symbol layer over the positions source,
  **filtered to weather stations**, rendering `badgeContent` anchored just above
  each weather pin. The pin (the existing sprite, which also cross-fades on stale)
  remains the location anchor; the badge gives the previously information-less WX
  marker its actual reading. Non-weather pins are unchanged.
- **Filter:** the active category drives a maplibre layer `filter` (and the badge
  layer's filter), hiding non-matching pins. Weather mode → only weather pins +
  their badges.
- **Hover (B):** reuse the cn84-style hover seam (`mouseenter`/`mouseleave` on the
  pin layer) to show an inline WX card (an HTML overlay like the existing popup,
  not a maplibre layer — richer formatting). The card lists only the fields heard.
- **Click → Station Data:** the existing click-popup seam instead (or in addition)
  invokes an `onFocusStation(call)` callback threaded from `AppShell`, which sets
  `dockTab='stations'` and focuses that call.

### 4. Station Data focus — `AppShell.tsx` + `EnvPanel`/`StationsView`

- `AppShell` passes `onFocusStation(call)` to `AprsPositionsMap` →
  `setDockTab('stations')` + a `focusCall` state.
- `EnvPanel`/`StationsView` accept an optional `focusCall` and scroll that
  station's `EnvStationCard` into view (+ a transient highlight). When `focusCall`
  is unset, behaviour is unchanged.

### 5. PNG export — `src/aprs/wxSnapshot.ts` + an export button

- An "Export weather snapshot" action (in the map controls, enabled in weather
  mode) captures the maplibre canvas (`map.getCanvas().toDataURL()`), composites a
  header strip (operator grid, UTC timestamp, weather-station count) onto a 2D
  canvas, and downloads a PNG.
- Requires `preserveDrawingBuffer: true` on the maplibre init so the GL canvas is
  readable; verify it does not regress the existing map (small memory cost). If it
  regresses, fall back to a transient on-demand re-render before capture.
- Pure `composeSnapshotHeader(meta): string` builds the header text; the canvas
  compositing + download is the thin imperative shell.

## Data flow

```
aprs-weather:new ─ useEnvStations ─┐
                                   ├─ useWxStations (joinWxStations, by call) ─ WxStation[]
aprs-position:new ─ useAprsPositions┘                                          │
                                                                               ├─ badge layer (badgeContent, honest)
                                                                               ├─ category filter (stationCategories)
                                                                               ├─ hover → WX card (B)
                                                                               ├─ click → onFocusStation → dockTab=stations + focusCall
                                                                               └─ export → wxSnapshot (canvas + header → PNG)
```

## Error handling / honesty

- No WX reading → no badge; the station keeps its ordinary symbol pin.
- Null fields → omitted from badge + card (never a fabricated 0 or assumed
  condition).
- No operator grid → snapshot header omits the grid line; export still works.
- Stale WX (station not re-heard) → the existing stale-dimming applies; the badge
  dims with the pin. No special-casing in v1.
- Positionless WX reports (no position) → not on the map (they have no location);
  they remain in the Station Data panel as today.

## Testing

- **Pure (vitest):** `joinWxStations` (inner-join, missing-position/ missing-WX
  excluded, latest-wins); `badgeContent` (temp-led; `🌧` only when raining; wind
  glyph threshold; temp-absent fallback; all-null → minimal); `stationCategories`
  predicates (`all`, `weather`); `composeSnapshotHeader`.
- **Component:** weather-mode filter hides non-WX pins / shows WX badges; clicking
  a WX pin invokes `onFocusStation(call)`; `EnvPanel` scrolls to `focusCall`.
- **Operator (grim/smoke):** badge legibility, hover card, the Station Data
  navigation, and the PNG export (jsdom has no WebGL or canvas image export).
- **Wire-walk (done-time):** (a) a heard WX station renders a temp badge on the
  real map; (b) weather mode hides non-WX stations; (c) clicking a WX badge opens
  its Station Data card; (d) export produces a PNG with the header.

## Out of scope (v1 / deferred)

- The heard-WX → Winlink **text situation report** (tuxlink-hepq, next phase;
  attaches this PNG).
- Non-weather categories in the filter (tuxlink-8fjx; the mechanism is built to
  accept them).
- Any heatmap / interpolated surface (rejected — fabricates data).

## Definition of done

The four wire-walk flows pass on a real build; `joinWxStations`, `badgeContent`,
`stationCategories`, and `composeSnapshotHeader` are unit-tested; the filter +
focus-station component tests pass; CI green (typecheck, vitest, build, clippy,
cargo test). No backend change is required — the WX + position event seams
already exist.
