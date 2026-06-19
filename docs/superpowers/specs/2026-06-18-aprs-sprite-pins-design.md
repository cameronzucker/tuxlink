# APRS map-face symbol sprites — design

- **Issue:** tuxlink-90xb (fast-follow to tuxlink-gnru / PR #768; part of the full-APRS epic tuxlink-18q2)
- **Date:** 2026-06-18
- **Agent:** marten-owl-poplar
- **Status:** approved (visual treatment, source, and architecture confirmed by the operator on 2026-06-18)

## Problem

The Tac Chat positions map (`src/aprs/AprsPositionsMap.tsx`) plots every heard
station as a coloured circle. The station's true APRS symbol — car, digipeater,
weather station, Igate — is resolved by `lookupAprsSymbol` but surfaces only in
the click popup, and even there as an emoji glyph that renders as tofu in
WebKitGTK. An operator scanning the map cannot tell a moving vehicle from a fixed
digipeater without clicking each pin.

The goal is the authentic APRS symbol drawn **on** each pin, at a glance, the way
aprs.fi and YAAC render it — without losing the position-honesty encoding that
`AprsPositionsMap` already carries.

## The competing-channels constraint

Each pin must now carry two orthogonal things:

- **Identity** — what kind of station (the APRS symbol).
- **Honesty** — whether the fix is current, stale, or position-ambiguous. This
  encoding was built deliberately: stale pins grey out via MapLibre feature-state
  (tuxlink-gq0d moved staleness to feature-state precisely to avoid rebuilding the
  FeatureCollection on every staleness tick), and position-ambiguous fixes draw a
  smaller centroid over a translucent amber uncertainty disc (tuxlink-f717).

A naïve sprite swap renders every station a crisp, confident icon and silently
destroys the stale / ambiguous signalling. The design below adds identity while
preserving honesty.

## Decisions

| Decision | Choice |
|---|---|
| Pin treatment | **Bare authentic sprite** directly on the map (operator chose this over a honesty-ring chip and a corner status badge — "the rest are too busy"). |
| Pin size | ~32px, rendered from 64px source cells at `pixelRatio: 2` for crispness. |
| Stale | Desaturated greyscale + reduced opacity. Carried without FeatureCollection churn (see Architecture). |
| Position-ambiguous | Sprite shrunk onto the existing amber uncertainty disc; disc and centroid layers unchanged. |
| Overlay symbols | Rendered. The alternate-table sprite is composited with the overlay character (e.g. a digipeater advertising `WIDE1-1`, an Igate with an `I` overlay). `lookupAprsSymbol` already returns the overlay character. |
| Unknown / unresolved | Neutral dot fallback — never a blank or a tofu box. |
| Symbol source | `hessu/aprs-symbols` (the de-facto set aprs.fi and YAAC ship), **full set minus the brand-logo cells** (Apple / Microsoft / Kenwood), which map to the fallback. |
| Licensing | CC BY-SA 2.0 attribution. `hessu/aprs-symbols` carries a per-symbol `COPYRIGHT.md`; that file is vendored and `NOTICE` attributes the set. The set is **not** CC0 (the value the issue assumed); dropping the brand-logo cells removes the only subset with active third-party objection risk. |

## Architecture

### Sprite module — `src/map/aprsSprites.ts`

Owns the vendored sheets and all MapLibre image registration. No knowledge of the
positions data model; consumed by `AprsPositionsMap`.

- `spriteIdFor(table, code, overlay): string` — pure. Returns a stable image id
  for a symbol (overlay folded into the id). Returns the fallback id for the
  brand-logo cells and for anything `lookupAprsSymbol` cannot resolve.
- `ensureSymbolImage(map, table, code, overlay): string` — lazy and idempotent.
  On first sight of a symbol it slices the cell from the vendored sheet, bakes a
  **colour** and a **greyscale** variant, composites the overlay character when
  present, and registers both via `map.addImage(id, …, { pixelRatio: 2 })`.
  Returns the colour id; the greyscale id is `id + '~grey'`. Re-registration is a
  no-op. Only symbols actually heard in a session are registered (dozens, not the
  full ~380-cell sheet).

The colour + greyscale bake uses an offscreen canvas (`grayscale` via per-pixel
luma). The overlay composite stacks the overlay-character sheet cell over the
alternate-table cell — verified to produce correct APRS overlays.

### Map layers — `AprsPositionsMap.tsx`

The single circle layer is replaced by **two stacked symbol layers** plus the
unchanged disc, centroid, and callsign-label layers:

- `aprs-pins-color` — `icon-image: ['get', 'spriteId']`,
  `icon-opacity: ['case', ['boolean', ['feature-state', 'stale'], false], 0, 0.95]`
- `aprs-pins-grey` — `icon-image: ['get', 'spriteIdGrey']`,
  `icon-opacity: ['case', ['boolean', ['feature-state', 'stale'], false], 0.55, 0]`

Both layers share `icon-size`, `icon-allow-overlap: true`, and
`icon-ignore-placement: true`.

**Why two layers.** MapLibre cannot recolour or desaturate a raster sprite via
paint (`icon-color` tints SDF icons only), and `icon-image` is a *layout*
property that cannot read feature-state — only paint properties can. Each
feature therefore carries both a stable `spriteId` and a stable `spriteIdGrey`
(neither changes as a station goes stale), and the **paint** property
`icon-opacity` — which *can* read feature-state — cross-fades the colour layer to
the greyscale layer when `stale` flips. Staleness remains a feature-state toggle:
no FeatureCollection rebuild on the staleness tick, preserving tuxlink-gq0d.

**Ambiguity** is a stable per-station property, not a tick-driven state, so it
drives a layout expression directly:
`icon-size: ['case', ['>', ['get', 'ambiguity'], 0], 0.7, 1]`. The amber
uncertainty-disc fill layer and the centroid remain keyed on the same `ambiguity`
data property.

### Data flow

`buildFC` (the FeatureCollection builder) gains two stable per-feature
properties: `spriteId` and `spriteIdGrey`, each computed once via `spriteIdFor`
from the station's `symbolTable` / `symbolCode` / overlay. Before `setData`, the
map ensures every referenced image is registered (`ensureSymbolImage` for each
distinct symbol in the batch), so the symbol layers never reference an
unregistered id. Staleness continues to drive feature-state, exactly as today.

## Assets and licensing

- Vendored under `src/assets/aprs-symbols/`: the 64px primary, alternate, and
  overlay-character sheets (~330KB total) plus the upstream `COPYRIGHT.md`.
- `NOTICE` attributes `hessu/aprs-symbols` under CC BY-SA 2.0 and points to the
  vendored `COPYRIGHT.md` for per-symbol provenance.
- The brand-logo cells are listed in `aprsSprites.ts` and resolve to the fallback
  id; their sheet pixels are still present but never registered as a symbol.

## Testing

Vitest, using the existing `src/aprs/testMapLibreMock.ts`:

- `spriteIdFor` — primary, alternate, overlay, unknown, and brand-logo inputs map
  to the expected ids (brand-logo and unknown → fallback).
- `buildFC` — emits the correct `spriteId`, `spriteIdGrey`, and `ambiguity` for
  representative stations; ambiguous stations carry `ambiguity > 0`.
- Layer wiring — `aprs-pins-color` and `aprs-pins-grey` are added with the
  expected `icon-image` and feature-state `icon-opacity` expressions, and
  `icon-size` shrinks ambiguous pins.
- Registration — `ensureSymbolImage` is idempotent and registers both a colour
  and a greyscale id.

The canvas bake and the real WebGL/WebKitGTK render cannot be verified in jsdom
(no WebGL, limited canvas). Visual confirmation is a **grim smoke on the
converged build** (operator), the same class as the tuxlink-hzwc map items.

## Definition of done (wire-walk flows)

1. A heard station with a known primary-table symbol (e.g. a car) shows that
   authentic icon on its map pin, not a circle.
2. The same station, gone stale (> staleness TTL), shows the greyscale variant
   without a FeatureCollection rebuild.
3. A position-ambiguous station shows a shrunk icon over the amber uncertainty
   disc.
4. A station transmitting an overlay symbol (e.g. `WIDE1-1` digipeater) shows the
   base symbol with the overlay character composited on top.
5. A station with an unresolved or brand-logo symbol shows the neutral dot
   fallback.

## Out of scope

- Animated packet-trace trails, beaconing, and igate (tracked separately under
  the tuxlink-18q2 epic).
- Operator's own-position pin styling (tuxlink-1sro).
- Replacing the emoji glyph in the click popup — the popup already names the
  symbol; this work concerns the map face. The popup may adopt the sprite in a
  follow-up but is not required here.
