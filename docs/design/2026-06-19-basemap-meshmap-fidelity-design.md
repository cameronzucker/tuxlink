# Basemap MeshMap-fidelity redesign — design spec

- **Date:** 2026-06-19
- **Agent:** fen-savanna-grouse
- **Status:** Direction locked (brainstorm). Implementation + final fidelity pending.
- **Supersedes/encompasses:** `tuxlink-uebb` (narrow "orange arterial spaghetti" fix) and the `tuxlink-hzwc` #8 road-density work, which this whole-basemap pass absorbs.
- **Brainstorm artifacts:** `.superpowers/brainstorm/356269-*/content/map-direction-{1..12}.html` (live side-by-side previews); grim captures `/tmp/cmp-v*.png`; MeshMap reference `/tmp/meshmap-ref.png`.

## Problem

The vector basemap is becoming a core feature of a professional EmComm workspace, but it reads as inferior to the intended bar. The "orange spaghetti" surface streets are one symptom of a broader drift away from the original north star: MeshMap's dark mode — sophisticated, slightly tactical, polished, lightweight.

## Ground truth (the MeshMap recipe)

MeshMap.net is **Leaflet + standard OSM Carto raster tiles** with a single CSS filter on the tile pane:

```css
filter: invert(1) hue-rotate(180deg) brightness(1.33);
```

Two consequences:

1. MeshMap's quality is inherited from OSM Carto — a mature, decades-tuned general-purpose cartography — simply inverted. It is not a bespoke dark style.
2. Tuxlink's [`src/map/darkStyle.ts`](../../src/map/darkStyle.ts) already bakes the **identical** transform (`invert → hue-rotate(180°) → brightness(1.33)`) GL-natively. The transform was never the problem.

## Root cause of the drift

Tuxlink applies the correct invert transform to the **wrong source**. [`src/map/tuxlinkFlavor.ts`](../../src/map/tuxlinkFlavor.ts) is a "punched-up, high-contrast" palette built for bright-sunlight outdoor legibility (bold saturated roads: highway `#e85d3a`, major `#f2933a`, minor `#f7c948`). Inverting a deliberately loud source yields a loud, garish dark. MeshMap inverts a restrained source and yields restraint.

## North star + scope decision

- **Dark mode** is the primary quality bar: MeshMap-class restraint.
- **Light mode** must remain legible in bright daylight field use (a real requirement).
- **Decision (operator):** decouple the two modes and purpose-build each, rather than deriving dark purely by inverting the light palette. The exact realization on Protomaps tiles is the one open implementation question (see below).

## Target dark palette (converged via 11 visual iterations vs MeshMap)

Colors are the locked design target. Final values get a fidelity pass on the real renderer (see Validation).

### Canvas (neutral, medium-dark — not near-black)

- Earth / background: `#2b2b29` (neutral dark gray)
- Residential landuse fill: `#4a4a47` (neutral medium gray — carries urban structure as gray blocks; sampled MeshMap urban `#41`–`#6e`)
- Water: `#1f5a6b` (tune toward sampled MeshMap `#335b70`)

The canvas must read **neutral gray, not warm-brown**. A brown cast is the "muddy" failure mode.

### Greens (dark olive; rendered at zoom-out)

- Wood / forest: `#2c4a18` ~0.55 opacity
- Grass / scrub / meadow: `#354517` ~0.42 opacity
- Park: `#3f7a1c` ~0.6 opacity (parks pop; general veg subdued — sampled MeshMap veg `#303916`, parks up to `#5af556`)

Green landcover **renders at low zoom**. The current Tuxlink Protomaps map already does this; coverage is a styling parameter we control, not a tile limitation. (An earlier preview looked sparse only because the OpenFreeMap proxy tiles generalize landcover away at low zoom; our Protomaps tiles do not.)

### Roads — hierarchy and structure (this is what defeats the spaghetti)

The structural principle, not just the colors, is load-bearing:

- **Major roads = solid warm fill** (crisp, the clear network):
  - Motorway: `#d55e73` salmon (tune toward sampled MeshMap `#d95a35`), thin dark casing `#1c140e`
  - Trunk: `#a14225` rust
  - Primary: `#c8881f` amber
- **Collectors / arterials = cased** (dark recessive body + thin warm outline — the MeshMap "outlined road" look, NOT a solid warm line):
  - Secondary: body `#2c2820` + casing `#bc8419` gold
  - Tertiary: body `#2a2620` + casing `#8a6e2c` muted gold
- **Minor / service = recede** (neutral, near-canvas): `#4a463c`, thin

The earlier "gold spaghetti" failure came from painting whole collector roads the casing color. The casing is an **outline**, not a fill.

### Road widths — non-adaptive prominence

Road widths stay thin and proportionate at zoom-out (no boldening of arterials when zoomed out) and grow only as the map zooms in. Width scaling must not let arterials dominate the macro view.

### Render-onset schedule (true to OSM Carto)

Feature onsets match OSM Carto's schedule so detail arrives at MeshMap's timing:

| Feature | minzoom |
|---|---|
| Motorway | 5 |
| Trunk | 6 |
| Primary | 7 |
| Secondary | 9 |
| Tertiary | 10–11 |
| Minor / residential road | 12 |
| Residential landuse | 10 |
| Buildings | 13 |
| Road-name labels (major) | 12 |
| Road-name labels (minor) | 14 |

### Buildings

Render from z13 with **higher contrast** than canvas (lighter fill ~`#6a6358` + a subtle darker outline from ~z15), fading in by opacity.

### Admin boundaries (currently missing — must add)

- State (admin_level 4): dashed, ~`#6b7888`, subtle
- National (admin_level ≤ 2): solid, ~`#909cab`

### Labels

Clean light text (place `#dadee4`, road `#c2c8d0`) with a dark halo. Labels must stay crisp and legible; they must not be inverted to mud (MeshMap keeps its label pane unfiltered).

## Light mode

Light mode is a restrained, OSM-Carto-class daylight cartography (the same structure, light palette), legible in direct sun. Detailed light-mode tuning is a follow-up pass; the dark mode is the immediate priority.

## Open implementation question (for eng-review)

How to realize this on Protomaps PMTiles:

- **(A) Restrained source + keep bake-invert.** Replace the punched-up `tuxlinkFlavor` overrides with a restrained OSM-Carto-class palette; the existing `darkStyle` invert then yields the MeshMap-class dark automatically. Smallest change; couples light and dark.
- **(B) Purpose-build dark directly** (operator's stated preference). Full control of both modes; more surface to maintain.

Evidence gathered: a **naive invert of an arbitrary complete style** (OpenFreeMap "Liberty") looked **worse** than the hand-tuned candidate — invert quality depends on the source being OSM-Carto-like. Hand-tuning beat naive-invert. The realization choice (A vs B) should be validated against real Protomaps tiles before committing.

## Validation method

The OpenFreeMap proxy tiles and Chromium preview are not the ship stack. Final fidelity is confirmed by **grim on the actual app** (WebKitGTK, Protomaps PMTiles) against the MeshMap reference at multiple zoom levels. Chromium is not a WebKitGTK proxy for layout, and proxy tiles differ in data coverage.

## Constraints

- Offline-first: no online raster-tile dependency (MeshMap's raster approach is not directly available to Tuxlink).
- ODbL: "© OpenStreetMap contributors" attribution required.
- Performance: GL-native bake (not a runtime CSS filter) on the Pi's software-GL budget.

## Out of scope

- Detailed light-mode tuning (follow-up).
- APRS pin / overlay styling (separate work).
