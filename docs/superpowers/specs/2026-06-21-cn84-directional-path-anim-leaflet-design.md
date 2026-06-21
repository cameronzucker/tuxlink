# Design — cn84 directional digipeat-path animation on Leaflet (faithful restore)

**Date:** 2026-06-21 · **Agent:** glade-gulch-fern · **bd issue:** tuxlink-qnu6
**Branch:** bd-tuxlink-qnu6/digipeat-path-anim (off main)
**Supersedes:** tuxlink-k0zz + draft PR #838 (the no-rAF FADE rework was a MapLibre-limitation cope). Implements the intent of tuxlink-zi58.1 ("revive cn84").

## Problem

On the APRS Tac Chat positions map, the path a heard packet traveled (sender →
repeated digipeater hops → operator) is not visualized. The original cn84
animation — the polyline drawing in hop-by-hop while a bright "packet" dot rides
sender → operator — shipped (#829) and was reverted (#834) because the **MapLibre**
implementation called `setData` (re-parse GeoJSON → re-tessellate the line →
re-upload to WebGL) **every animation frame**, which pegged the llvmpipe software
renderer and made the map unusable.

The map engine has since migrated to **Leaflet + Canvas2D** (#845/#847). On
Canvas2D, animating a short polyline + a dot is a per-frame `clearRect` + `stroke`
+ `arc` on a canvas — no tessellation, no WebGL, no readback. The perf wall that
killed cn84 is gone.

This restores the cn84 animation faithfully on the new engine. Enhancements
(comet-trail, pulsing packet, concurrent multi-path) are a deferred follow-up.

## What already exists (do NOT rebuild)

- **Backend via-chain (intact, from j5cj #832 — never reverted):** `engine.rs`
  surfaces `ViaHop { call, repeated }` (the AX.25 H-bit; `repeated == true` = the
  digi actually relayed) on `InboundPos.via`, which flows to the frontend store as
  `HeardPosition.via?: ViaHop[]` ([aprsTypes.ts](../../../src/aprs/aprsTypes.ts),
  [useAprsPositions.ts](../../../src/aprs/useAprsPositions.ts)). No backend work.
- **Pure path resolver (reuse from draft #838):** `src/aprs/digipeatPath.ts`
  (`resolveDigipeatPath`) + `digipeatPath.test.ts` exist on branch
  `bd-tuxlink-k0zz/trace-fade-rework`. It is engine-agnostic — turns a via-chain +
  the known station positions into ordered segments tagged located / unlocated. It
  is **ported as-is** with its tests (it was written for the fade layer but the
  resolution logic is render-independent). The `DigipeatFadeLayer.tsx` (MapLibre
  render) from that branch is **not** reused.

## Decision

Faithfully restore the cn84 animation on the Leaflet engine via a **dedicated
Canvas2D overlay layer** driven by a **bounded `requestAnimationFrame`** loop.

## Approach

A custom Leaflet layer owns its **own `<canvas>`** in an overlay pane above the
map. On trigger it runs a bounded rAF: each frame it `clearRect`s its canvas and
draws (a) the polyline trimmed to the current `progress` (the hop-by-hop draw-in)
and (b) the bright packet dot at the `progress` position along the path —
projecting each path lat/lon to container pixels with `map.latLngToContainerPoint`.
The loop is bounded (draw → linger → fade → stop); it never runs perpetually. The
canvas reprojects/redraws on map `move`/`zoom` while a trace is active.

Rejected alternatives: animating `L.Polyline`/`L.CircleMarker` per frame (Leaflet
repaints *all* vector layers each frame — recreates the cn84 perf trap); SVG
`stroke-dashoffset` (the declarative-transition flavor the operator rejected, and
reprojection on pan/zoom is awkward).

## Components

1. **`src/aprs/digipeatPath.ts`** (pure, ported from #838 with tests) —
   `resolveDigipeatPath(via, positionsByCall, operator)` → ordered `PathSegment[]`
   each tagged located (both endpoints known) or unlocated (`pos?`). Honest hybrid:
   solid through located hops; dashed `pos?` connector across an unlocatable hop;
   degrade to a direct sender → operator line when no intermediate hop is locatable.
   Uses only the **repeated == true** hops as traversed (non-repeated requested
   digis are not drawn); falls back to all via entries if a frame lacks H-bits.

2. **`src/aprs/digipeatAnim.ts`** (pure, new) — `traceProgress(elapsedMs, timing)`
   → `{ phase: 'draw'|'linger'|'fade'|'done', drawProgress: 0..1, opacity: 0..1 }`.
   The animation schedule (≈2 s draw, ≈2 s linger, fade out), code-tunable. Pure
   and unit-tested; the layer just calls it per frame with `performance.now()`.

3. **`src/aprs/DigipeatPathLayer.tsx`** (new) — a Leaflet Canvas2D overlay layer
   (a custom `L.Layer` subclass, or a hook that manages a canvas in an overlay
   pane) consumed via `LeafletMapContext`. Given a resolved `PathSegment[]` and a
   start time, runs the bounded rAF using `traceProgress` + the segment geometry to
   stroke the trimmed polyline (solid/dashed per segment) and the packet dot.
   Reprojects on `move`/`zoom`. Cancels its rAF on unmount / new trace. Guarded so
   a transient throw mid zoom/pan is logged + skipped, never crashed to the
   ErrorBoundary (mirrors `AprsPositionsMap`'s `safe()` reconcile wrapper).

4. **Wiring in [AprsPositionsMap.tsx](../../../src/aprs/AprsPositionsMap.tsx)** —
   add the layer; triggers (cn84-faithful): **hover a station pin** draws/clears
   that station's latest-frame path; a **new `aprs-position:new`** frame triggers a
   single one-shot trace that fades. Resolve the path from the station's
   `HeardPosition.via` + the component's existing position map (`byCall`). Skip
   object/item reports (their via is the relayer's, not the transmitter's — already
   handled in `useAprsPositions`).

## Behavior

- **Feel (cn84 / aprs.fi-classic, code-tunable):** ~2 s hop-by-hop draw, bright
  packet dot rides sender → operator, ~2 s linger, then fade out.
- **Honest path:** solid through located hops; dashed `pos?` across unlocated; a
  `pos?` marker rather than a fabricated intermediate pin; direct sender → operator
  line when no intermediate hop is locatable.
- **Triggers:** hover (draw/clear) + new live frame (one-shot fade). Concurrent
  multi-path dwell and a feed-row hover trigger are deferred follow-ups.

## Performance + acceptance

- Bounded rAF (stops after the trace; no perpetual loop), isolated small canvas,
  plain Canvas2D ops. The per-frame cost is independent of the main map's vector
  layers.
- **Operator-grim-smoke-gated on llvmpipe:** the render is opened as a **draft PR
  and NOT merged** until the operator confirms it is smooth on the real WebKitGTK
  app (the agent cannot validate software-GL perf — this is the exact failure mode
  that reverted cn84). Mirrors how cn84/k0zz were gated.

## Testing

- **Pure, unit-tested (vitest, CI):** `resolveDigipeatPath` (ported tests +
  honest-path cases) and `traceProgress` (phase boundaries, draw 0→1, fade
  opacity).
- **Not unit-tested:** the Canvas2D render (jsdom has no 2D context) — covered by
  the operator grim-smoke, the acceptance gate.

## Out of scope

- Backend / via-chain changes (already shipped, j5cj #832).
- Enhancements beyond the cn84 feel: comet-trail, pulsing packet, concurrent
  multi-path, always-on faint paths, feed-row hover trigger.
- Migrating the other four map surfaces off MapLibre.
- `.github/RELEASE_FREEZE` stays in place.
