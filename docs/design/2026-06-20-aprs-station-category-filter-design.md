# APRS map station category filter — design

**Issue:** tuxlink-8fjx (child of the APRS polish epic tuxlink-zi58)
**Date:** 2026-06-20 · **Agent:** bayou-granite-slate
**Status:** approved (operator brainstorm 2026-06-20), ready for implementation plan

## Goal

Let the operator choose which kinds of APRS stations the Tac Chat map draws. The
map plots every heard station today; with dense traffic the operator cannot
isolate, for example, weather stations or emergency assets. The station category
filter adds a multi-select control that toggles whole categories of stations on
and off, aprs.fi-style, without changing what was heard or decoded.

RF-honesty governs the whole feature: the filter only ever **hides or shows**
pins for stations actually heard. It never fabricates, infers, or relocates a
station, and every heard station belongs to exactly one visible category.

## Existing scaffold (origin/main)

The map already carries a minimal single-category filter that this feature
replaces and extends:

- `src/aprs/stationCategories.ts` — a pure-predicate registry with two entries
  (`all`, `weather`) and `categoryByKey()`. The file's own header states the
  intent: "tuxlink-8fjx extends this list by adding predicates — the filter
  mechanism does not change."
- `src/aprs/AprsPositionsMap.tsx` — `WxFilterControl` renders a single `<select>`
  in the map corner; `MapOverlays` holds `category` state (`useState('all')`) and,
  per reconciled station, computes `categoryByKey(category).matches({ call, isWeather })`
  and toggles the whole marker bundle (pin + uncertainty disc + WX badge) in or
  out of the Leaflet layer group.
- `src/aprs/aprsSymbols.ts` — the full APRS symbol space: `PRIMARY_SYMBOLS`
  (94), `ALTERNATE_SYMBOLS` (94), `OVERLAY_MEANINGS` (overlay+code combos such as
  `W#`, `I&`, `Ra`), and `lookupAprsSymbol(table, code)`.
- `src/aprs/aprsTypes.ts` — `HeardPosition` carries `symbolTable`, `symbolCode`,
  and `isObject`, so the classifier has every input it needs already plumbed.

The single-category `<select>` is the throwaway. The predicate registry, symbol
tables, and bundle-visibility application are the reusable foundation.

## Map engine note

`AprsPositionsMap.tsx` renders with **Leaflet** (`L.marker`, `L.featureGroup`),
not MapLibre, on origin/main. The MapLibre engine decision (tracked elsewhere) is
**out of scope** for this feature. The filter is engine-agnostic: classification
is a pure function of a station's symbol, and visibility application is "add or
remove this station's bundle from the layer." Both survive an engine swap
unchanged. Implementation targets the current Leaflet surface.

## The eight buckets

Every heard station classifies into exactly one of these buckets. Each bucket is
a user-facing filter row.

| Bucket | Key | Feeds it |
|---|---|---|
| 🌡️ Weather | `weather` | Weather symbol (`_`, `W`), weather-condition objects (hurricane, tornado, hail, flood, snow, fog, skywarn…), or a station carrying valid WX readings |
| 🌐 iGates & Gateways | `igate` | iGate symbol `&` and its overlays (`I&`, `R&`, `T&`, `W&`, `L&`, `2&`, `P&`), TCP/IP station `/I`, HF gateway `/&` |
| 📡 Digipeaters & Nodes | `digipeater` | Digi symbol `#` and overlays (`W#`, `1#`, `S#`, `I#`, `A#`, `L#`, `X#`), repeater `/r`, Mic-E repeater `/m`, node `/n`, network node `\8`, C4FM/D-STAR overlays (`Ya`, `Da`) |
| 🚑 Emergency / EmComm | `emergency` | Emergency/ELT (`\!`, `E!`), ARES/RACES/SATERN/WinLink/MARS overlays (`Aa`, `Ra`, `Sa`, `Wa`, `\M`), aid station `/A`, EOC `/o`, incident command `/c`, ambulance `/a`, fire truck `/f`, fire dept `/d`, Red Cross `/+`, Coast Guard `\C`, police/sheriff (`/!`, `/P`), crash/incident `\'`, DF/fox triangle (`/\`, `\c`) |
| 🚗 Vehicles & Craft | `vehicles` | Moving machines: car, truck, semi, van, jeep, SUV, motorcycle, bus, RV, farm vehicle, train, snowmobile; **and** aircraft (small/large/heli/glider/balloon) and marine (yacht, boat, ship, canoe) — aircraft and boats stay under Vehicles |
| 🧍 People | `people` | Person, jogger/runner (Mic-E), wheelchair, bicycle, horse/rider |
| 🏠 Fixed & Places | `fixed` | House/QTH, hospital, school, hotel, restaurant, campground, park, store/hamfest, post office, bank/ATM, point of interest, parking, Yagi at QTH, power plant |
| ▫️ Other | `other` | Anything unmatched: undefined/reserved symbols, satellites/space, computers, phone/BBS/file server, waypoints/advisories, area/grid, unknown overlays. The catch-all — a station is never silently dropped |

### Operator decisions baked in (brainstorm 2026-06-20)

- **Police** stays under Emergency / EmComm (no separate "Public safety" bucket).
- **Aircraft and boats** stay under Vehicles & Craft (not split out).
- **Weather-condition objects** (tornado/hail/flood markers) stay under Weather,
  alongside WX stations. Weather is acknowledged as an infrequently-toggled bucket.
- **The Emergency / EmComm bucket is retained** as the one deliberately
  audience-shaped group (ARES/RACES focus), pulling served-agency and incident
  assets out of the generic Vehicles and Fixed piles.

## Classification

Classification is a **curated data table**, authored by hand over the finite
symbol space, not a set of fuzzy heuristics. A new module
`src/aprs/stationBuckets.ts` owns it.

```ts
export type BucketKey =
  | 'weather' | 'igate' | 'digipeater' | 'emergency'
  | 'vehicles' | 'people' | 'fixed' | 'other';

export interface StationBucketCtx {
  symbolTable: string;   // HeardPosition.symbolTable
  symbolCode: string;    // HeardPosition.symbolCode
  isWeather: boolean;    // station carries valid WX readings (from the WX join)
}

export function bucketForStation(ctx: StationBucketCtx): BucketKey;
```

### Algorithm

1. **Weather-readings override.** If `ctx.isWeather` is true, the bucket is
   `weather`. A station transmitting valid weather measurements is Weather
   regardless of the symbol it chose. (This preserves the existing `isWeather`
   semantics derived from the WX join in `MapOverlays`.)
2. **Symbol lookup.** Otherwise resolve `(symbolTable, symbolCode)` against an
   explicit `SYMBOL_BUCKET` map. The map is keyed first by overlay+code combos
   (so `W#` → `digipeater`, `I&` → `igate`, `Ra` → `emergency` win over their
   bases), then by `"/"`+code (primary) and `"\\"`+code (alternate).
3. **Other.** Any symbol with no entry — undefined, reserved, or genuinely
   uncategorized — returns `other`. Nothing falls through to invisibility.

Because the map is explicit and exhaustive, **precedence between buckets is
resolved at table-authoring time, not at runtime**: each symbol (and each known
overlay combo) is assigned its single correct bucket directly. A fire *truck*
(`/f`) is authored into `emergency`, not `vehicles`; a digipeater at a house
(`/#`) is authored into `digipeater`, not `fixed`. The only runtime precedence
rule is the weather-readings override in step 1.

### Test obligation

`stationBuckets.test.ts` asserts the bucket for **every** printable code in both
the primary and alternate tables and for every entry in `OVERLAY_MEANINGS`,
pinning the curated assignment so a future symbol-table edit cannot silently
re-bucket a station. It also asserts the weather-readings override and the
`other` catch-all for an unknown symbol.

## UI — collapsible layers panel

The `<select>` control is replaced by a collapsible layers panel anchored in a
map corner (top-right). Two states:

- **Collapsed (default footprint reclaimed):** a single compact `☰ Layers`
  button. The map is unobstructed. The operator explicitly required the panel be
  collapsible so it does not permanently consume map space.
- **Expanded:** a panel listing the master row plus the eight bucket rows. Each
  bucket row shows a checkbox, the bucket glyph + label, and a **live count** of
  currently-heard stations in that bucket. A `✕` (or re-click of the toggle)
  collapses it.

### Interactions

- **Per-bucket checkbox** toggles that bucket's visibility. Unchecking hides all
  pins whose `bucketForStation` is that bucket.
- **"All stations" master row** is select-all / clear-all. It reads as checked
  when all buckets are on, unchecked when all are off, and indeterminate when
  mixed.
- **Counts are live**, recomputed from the current `positions` set as stations
  are heard or pruned. The master row shows the total heard. A bucket with zero
  heard stations remains listed (stable layout) with a dimmed `0`.

### Default state

Every bucket is **ON** by default (RF-honesty: show everything heard unless the
operator chooses to narrow). First run draws all stations exactly as the map does
today.

## State & persistence

- Enabled-bucket state lives in `AprsPositionsMap` as a `Set<BucketKey>` (or an
  equivalent per-key boolean record), replacing the current `category` string
  `useState`.
- Selection **and** collapsed/expanded state persist across sessions under a
  namespaced key (`tuxlink:map-filter:aprs`), mirroring how
  `usePersistedViewport` (`tuxlink:map-viewport:aprs`) already persists the map
  viewport. A malformed or absent stored value falls back to "all buckets on,
  collapsed."
- Visibility application in `MapOverlays` changes from
  `categoryByKey(category).matches(...)` to
  `enabledBuckets.has(bucketForStation({ symbolTable, symbolCode, isWeather }))`.
  The bundle add/remove logic is otherwise unchanged.

## Components touched

- **New:** `src/aprs/stationBuckets.ts` (+ `stationBuckets.test.ts`) — the
  curated classifier and `BucketKey`/bucket-metadata exports (key, label, glyph).
- **New:** the layers-panel component (collapsible) — either a new
  `AprsLayersPanel.tsx` (+ test, + css) or an expanded replacement of
  `WxFilterControl` inside `AprsPositionsMap.tsx`. Preference: a separate
  `AprsLayersPanel.tsx` for an isolated, independently-testable unit.
- **Edit:** `src/aprs/AprsPositionsMap.tsx` — swap `category` state for the
  bucket set, render the panel, change the per-station visibility predicate, feed
  live counts to the panel.
- **Retire:** `src/aprs/stationCategories.ts`'s `all`/`weather` `CATEGORIES`
  registry and `WxFilterControl`, superseded by `stationBuckets.ts` and the
  panel. Remove or fold the old test file accordingly.

## Edge cases & RF-honesty

- **Objects / items** (`isObject` true — NPS weather objects, event markers)
  classify by **their own transmitted symbol**, identical to any other pin. A
  weather object lands in Weather; an EOC object lands in Emergency. No separate
  "Objects" bucket. (Consistent with tuxlink-zi58.1's rule that an object's pin
  is the object's location — here it only affects which bucket, never path
  tracing.)
- **Overlay symbols** are resolved via the `OVERLAY_MEANINGS`-keyed entries first
  so a digipeater/iGate carrying an overlay character is bucketed correctly
  rather than by its alternate-table base glyph.
- **Unknown / malformed symbols** return `other` and stay visible by default —
  hiding a heard station because the app failed to name its symbol would violate
  RF-honesty.
- **Hidden stations are still heard.** The filter only affects map rendering. The
  Stations view, WX panels, chat feed, and counts of total heard are unaffected.

## Out of scope

- The MapLibre-vs-Leaflet engine decision.
- Overlay *layers* (breadcrumb tracks tuxlink-zi58.2, RF/digipeat paths
  tuxlink-zi58.1, iGate tuxlink-zi58.3) — the layers panel is designed so these
  can later be added as additional rows/sections, but they are separate epic
  children and not built here.
- Per-station search/focus (tuxlink-zi58.5) and chronological rewind
  (tuxlink-zi58.4).
- Filtering by anything other than station category (age, SSID, distance).

## Verification

Per the 2026-06-20 operator correction, CI is the verification surface, not the
Pi: run `pnpm typecheck` and only the touched vitest files locally; let CI run
the full vitest plus all cargo/clippy. Merge current `origin/main` before
trusting local typecheck when parallel sessions are landing related APRS code.
No Rust changes are expected (this is a frontend-only feature). No RF
transmission is involved (RX/UI only; RADIO-1 not implicated).
