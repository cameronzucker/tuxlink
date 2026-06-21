# Design — region packs as the detailed local map (Leaflet engine)

**Date:** 2026-06-20 · **Agent:** glade-gulch-fern · **bd issue:** tuxlink-c973
**Branch:** bd-tuxlink-c973/pack-labels (follow-up off main after PR #846)

## Problem

On the Leaflet map engine (the migrated APRS positions map), an installed region
pack draws its detail geometry but **no placenames**, and at local zoom inside a
pack's coverage the operator sees streets with few or no place labels. The
operator's requirement is detailed placenames at local zoom: city, neighborhood,
and landmark names.

The cause is the Leaflet compositing model, which differs from the retired
MapLibre one:

- **MapLibre** ([basemapStyle.ts](../../../src/map/basemapStyle.ts)) merges the
  world overview and every pack into one GL style and drops each pack's
  `background` and `symbol` layers, so a single GL context orders all geometry
  below and all labels above.
- **Leaflet** ([basemapLeaflet.ts](../../../src/map/basemapLeaflet.ts),
  [LeafletMap.tsx](../../../src/map/LeafletMap.tsx)) stacks the world overview and
  each pack as **separate canvases** by `zIndex`. Each pack layer draws the
  flavor's opaque `earth` polygon (covering all land in coverage) and passes
  `labelRules: []`. The pack's earth occludes the overview's placenames baked into
  the overview canvas beneath it, and the pack contributes none of its own — so
  inside pack coverage no placenames render.

The download → register → serve pipeline is **engine-agnostic** (Rust
`basemap_download_pack` → `install_pack` → manifest, `init_packs` re-registration,
and the `tile://pmtiles/<id>` HTTP-206 seam serve raw bytes to either engine). A
region pack (`continent-na`, schema 4.14.9) did download and render on Leaflet;
its visible failure was the missing placenames, not the download. Pack downloads
have not been exercised on the Leaflet engine since the migration, so the
end-to-end path is verified as part of this work rather than assumed.

PR #846 separately fixed an unrelated overview defect: the world overview's own
placenames were evicted on zoom-out (`maxLabeledTiles: 16` raised to 256). That
fix is merged and is the reason the overview now labels reliably outside pack
coverage.

## Decision

A region pack is the authoritative **detailed local map within its coverage**: it
provides the fine detail and the detailed placenames, in the flavor's colors, and
the bundled world overview fills only outside the pack's coverage and below the
pack's minimum zoom.

## Approach

Give each pack layer the flavor's real label rules instead of an empty array. In
[basemapLeaflet.ts](../../../src/map/basemapLeaflet.ts) `buildBaseLayers`, each
pack layer changes from `labelRules: []` to
`labelRules: pmLabelRules(namedFlavor(flavor), 'en')` — the same label-rule source
the overview already uses (imported as `labelRules as pmLabelRules` from the
vendored `protomaps-leaflet`). The overview layer is unchanged.

Inside coverage, the pack canvas now paints earth, detail geometry, and its own
z6–z13 placenames, in the same flavor as the overview, so the seam is invisible.
The pack's opaque earth continues to cover the overview's labels beneath it, so
the pack's labels are the only land labels — **no double-labeling**, achieved by
the existing occlusion rather than new clipping logic. Outside coverage and below
`REGION_MINZOOM` (z6) the pack is transparent and the overview shows.

### Why not the alternatives

- **Drop the pack's earth so the overview shows through, keep packs label-free.**
  Keeps placenames at the overview's sparse z6 density — fails the requirement.
- **Per-pack coverage clipping of the overview's labels.** Real engine work the
  Leaflet separate-canvas model does not provide for free, and unnecessary while
  the pack's earth already occludes the overview in coverage.

### Reserved fallback (build only if smoke shows it)

The one place an overview label can show through a pack is over water inside the
pack's bbox, where the pack draws no earth. If large-water labels visibly double
(overview water label plus pack water label), cap the **overview's** label rules
to `maxzoom 6` so they never overzoom into pack territory. Hold this in reserve;
do not build it preemptively.

## Components touched

- [basemapLeaflet.ts](../../../src/map/basemapLeaflet.ts) — import `pmLabelRules`;
  pack layers carry the flavor's label rules. One focused change; the overview
  layer, the PMTiles seam wiring, `maxDataZoom`, `minZoom`, and `zIndex` are
  unchanged.
- [basemapLeaflet.test.ts](../../../src/map/basemapLeaflet.test.ts) — update the
  pack assertion (`labelRules` is the flavor's rules, not `[]`) and assert the
  pack's label rules derive from the same flavor as its paint rules.

No Rust, no download-pipeline, and no `LeafletMap.tsx` changes: the pack list,
the packs-changed event (`emitPacksChanged`, already wired at
[OfflineMapsSettings.tsx](../../../src/map/OfflineMapsSettings.tsx) 169/195), and
the seam are all engine-agnostic and already correct.

## Acceptance / verification

Automated gates (the merge gate): `pnpm typecheck`, `pnpm vitest run`,
`pnpm build`, all green.

End-to-end operator smoke (validates the never-exercised Leaflet download path and
this compositing change together): the operator downloads one **small** region
pack, and inside its coverage the map renders as the detailed local map with
placenames (city, neighborhood, landmark), in flavor colors, with no double
labels, and zoom remains smooth. This single run is the wire-walk for the feature.

## Risk to watch (not a blocker)

Pack labels render on the Pi's Canvas2D software path. The MapLibre engine dropped
pack labels for glyph cost on llvmpipe; the Leaflet engine is reported tangibly
faster. Confirm zoom smoothness during the same operator smoke. If label layout is
heavy, the lever is label density (label-rule selection / `maxzoom` gating), not
the architecture.

## Out of scope

- The reserved overview-label `maxzoom` cap (build only if water-doubling shows).
- Any change to the download/registration pipeline or the Rust seam.
- Migrating the other four map surfaces off MapLibre (separate strangler-fig work).
- Lifting `.github/RELEASE_FREEZE` (stays in place).
