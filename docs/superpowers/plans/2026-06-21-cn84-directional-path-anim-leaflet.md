# cn84 Directional Path Animation (Leaflet) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Faithfully restore the cn84 directional digipeat-path animation (hop-by-hop polyline draw-in + a packet dot riding sender → operator) on the Leaflet/Canvas2D engine.

**Architecture:** A dedicated Canvas2D overlay layer over the Leaflet map runs a bounded `requestAnimationFrame` loop. Each frame it asks two PURE helpers — `traceProgress(elapsed)` (the schedule) and `trimPath(segments, drawProgress)` (the geometry) — what to draw, then strokes the trimmed polyline + dot, projecting lat/lon → container pixels via the live map. The path itself is resolved by the pure `resolveDigipeatPath` ported from draft #838.

**Tech Stack:** TypeScript, React, Leaflet (Canvas2D), Vitest. Reuses the backend via-chain (`HeardPosition.via`, already shipped).

## Global Constraints

- Frontend `pnpm`; tests `pnpm vitest run`.
- Commit trailers REQUIRED: `Agent: glade-gulch-fern` + `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.
- Conventional commits: `feat(aprs): …` for the feature, `test(aprs): …` only if a commit is tests-only.
- Branch `bd-tuxlink-qnu6/digipeat-path-anim` (created off main). Commit relying on the persisted worktree cwd (no inline `cd` in the commit command — the main-checkout hook reads the shell's cwd).
- **No backend changes** — the via-chain is shipped (j5cj #832).
- The Canvas2D render is **operator-grim-smoke-gated on llvmpipe**: open a **DRAFT PR, do NOT merge** until the operator confirms smooth on the real WebKitGTK app. The agent cannot validate software-GL perf (this is what reverted cn84).
- `.github/RELEASE_FREEZE` stays.

---

### Task 1: Port the pure path resolver from draft #838

**Files:**
- Create: `src/aprs/digipeatPath.ts` (port verbatim from `origin/bd-tuxlink-k0zz/trace-fade-rework`)
- Test: `src/aprs/digipeatPath.test.ts` (port verbatim)

**Interfaces:**
- Produces: `resolveDigipeatPath(input: ResolveInput): PathSegment[]`; types `LatLon { lat; lon }`, `PathSegment { kind: 'solid'|'dashed'; from: LatLon; to: LatLon; unknownLabels?: string[] }`, `ResolveInput { src: LatLon & { call: string }; via: ViaHop[]; located: Map<string, LatLon>; operator: LatLon | null }`.

- [ ] **Step 1: Port the two files from the #838 branch**

```bash
git show origin/bd-tuxlink-k0zz/trace-fade-rework:src/aprs/digipeatPath.ts      > src/aprs/digipeatPath.ts
git show origin/bd-tuxlink-k0zz/trace-fade-rework:src/aprs/digipeatPath.test.ts > src/aprs/digipeatPath.test.ts
```

- [ ] **Step 2: Run the ported tests to verify they pass on this branch**

Run: `pnpm vitest run src/aprs/digipeatPath.test.ts`
Expected: PASS (the module is self-contained; it imports only `ViaHop` from `./aprsTypes`, which exists).

- [ ] **Step 3: Commit**

```bash
git add src/aprs/digipeatPath.ts src/aprs/digipeatPath.test.ts
git commit -m "feat(aprs): port pure digipeat-path resolver from #838 (tuxlink-qnu6)
<trailers>"
```

---

### Task 2: Pure animation schedule + path-trim geometry

**Files:**
- Create: `src/aprs/digipeatAnim.ts`
- Test: `src/aprs/digipeatAnim.test.ts`

**Interfaces:**
- Consumes: `PathSegment`, `LatLon` from `./digipeatPath`.
- Produces:
  - `interface TraceTiming { drawMs: number; lingerMs: number; fadeMs: number }`
  - `const DEFAULT_TIMING: TraceTiming` = `{ drawMs: 2000, lingerMs: 2000, fadeMs: 600 }` (cn84 aprs.fi-classic, code-tunable).
  - `traceProgress(elapsedMs: number, timing?: TraceTiming): { phase: 'draw'|'linger'|'fade'|'done'; drawProgress: number; opacity: number }`
  - `trimPath(segments: PathSegment[], drawProgress: number): { drawn: PathSegment[]; dot: LatLon | null }`

- [ ] **Step 1: Write the failing tests**

```typescript
import { describe, it, expect } from 'vitest';
import { traceProgress, trimPath, DEFAULT_TIMING } from './digipeatAnim';
import type { PathSegment } from './digipeatPath';

describe('traceProgress', () => {
  it('draws 0→1 over drawMs, then lingers full, then fades, then done', () => {
    expect(traceProgress(0)).toMatchObject({ phase: 'draw', drawProgress: 0, opacity: 1 });
    expect(traceProgress(1000)).toMatchObject({ phase: 'draw', opacity: 1 });
    expect(traceProgress(1000).drawProgress).toBeCloseTo(0.5, 2);
    expect(traceProgress(2000)).toMatchObject({ phase: 'linger', drawProgress: 1, opacity: 1 });
    expect(traceProgress(3000)).toMatchObject({ phase: 'linger', drawProgress: 1 });
    // fade window is [draw+linger, draw+linger+fade] = [4000, 4600]
    expect(traceProgress(4300).phase).toBe('fade');
    expect(traceProgress(4300).opacity).toBeGreaterThan(0);
    expect(traceProgress(4300).opacity).toBeLessThan(1);
    expect(traceProgress(5000)).toMatchObject({ phase: 'done', opacity: 0 });
  });
});

describe('trimPath', () => {
  // 2 equal-count segments: A→B (solid), B→C (solid). Progress is by SEGMENT
  // COUNT (hop-by-hop), not geographic distance — faithful to cn84.
  const segs: PathSegment[] = [
    { kind: 'solid', from: { lat: 0, lon: 0 }, to: { lat: 0, lon: 2 } },
    { kind: 'solid', from: { lat: 0, lon: 2 }, to: { lat: 0, lon: 4 } },
  ];
  it('progress 0 → nothing drawn, no dot', () => {
    expect(trimPath(segs, 0)).toEqual({ drawn: [], dot: null });
  });
  it('progress 0.25 → first segment half-drawn, dot at the leading edge', () => {
    const r = trimPath(segs, 0.25); // 0.25 of 2 segs = halfway through seg 0
    expect(r.drawn).toHaveLength(1);
    expect(r.drawn[0].to).toEqual({ lat: 0, lon: 1 });
    expect(r.dot).toEqual({ lat: 0, lon: 1 });
  });
  it('progress 1 → both segments full, dot at the end', () => {
    const r = trimPath(segs, 1);
    expect(r.drawn).toHaveLength(2);
    expect(r.drawn[1].to).toEqual({ lat: 0, lon: 4 });
    expect(r.dot).toEqual({ lat: 0, lon: 4 });
  });
  it('empty path → empty', () => {
    expect(trimPath([], 0.5)).toEqual({ drawn: [], dot: null });
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `pnpm vitest run src/aprs/digipeatAnim.test.ts`
Expected: FAIL ("digipeatAnim" not found / functions undefined).

- [ ] **Step 3: Implement `src/aprs/digipeatAnim.ts`**

```typescript
// src/aprs/digipeatAnim.ts
//
// Pure schedule + geometry for the cn84 directional path animation. No DOM, no
// canvas, no Leaflet — the layer (DigipeatPathLayer) calls these per frame and
// does the drawing. Kept pure so the animation logic is unit-tested; only the
// raw Canvas2D draw + projection is smoke-gated.
import type { LatLon, PathSegment } from './digipeatPath';

export interface TraceTiming {
  drawMs: number;
  lingerMs: number;
  fadeMs: number;
}

/** cn84 aprs.fi-classic feel. Tunable. */
export const DEFAULT_TIMING: TraceTiming = { drawMs: 2000, lingerMs: 2000, fadeMs: 600 };

export interface TraceState {
  phase: 'draw' | 'linger' | 'fade' | 'done';
  drawProgress: number; // 0..1 fraction of the path drawn
  opacity: number; // 0..1
}

export function traceProgress(elapsedMs: number, timing: TraceTiming = DEFAULT_TIMING): TraceState {
  const { drawMs, lingerMs, fadeMs } = timing;
  if (elapsedMs <= 0) return { phase: 'draw', drawProgress: 0, opacity: 1 };
  if (elapsedMs < drawMs) {
    return { phase: 'draw', drawProgress: elapsedMs / drawMs, opacity: 1 };
  }
  const lingerEnd = drawMs + lingerMs;
  if (elapsedMs < lingerEnd) {
    return { phase: 'linger', drawProgress: 1, opacity: 1 };
  }
  const fadeEnd = lingerEnd + fadeMs;
  if (elapsedMs < fadeEnd) {
    return { phase: 'fade', drawProgress: 1, opacity: 1 - (elapsedMs - lingerEnd) / fadeMs };
  }
  return { phase: 'done', drawProgress: 1, opacity: 0 };
}

function lerp(a: LatLon, b: LatLon, t: number): LatLon {
  return { lat: a.lat + (b.lat - a.lat) * t, lon: a.lon + (b.lon - a.lon) * t };
}

/** Trim the path to `drawProgress` (0..1). Progress is by SEGMENT COUNT
 * (hop-by-hop, faithful to cn84), not geographic distance: each of the N
 * segments occupies an equal 1/N slice. Returns the drawn segments (the final
 * one possibly partial) and the dot position at the leading edge. */
export function trimPath(
  segments: PathSegment[],
  drawProgress: number,
): { drawn: PathSegment[]; dot: LatLon | null } {
  const total = segments.length;
  if (total === 0 || drawProgress <= 0) return { drawn: [], dot: null };
  const drawn: PathSegment[] = [];
  let dot: LatLon | null = null;
  for (let i = 0; i < total; i++) {
    const segStart = i / total;
    const segEnd = (i + 1) / total;
    if (drawProgress <= segStart) break; // not reached yet
    const s = segments[i];
    if (drawProgress >= segEnd) {
      drawn.push(s);
      dot = s.to;
    } else {
      const frac = (drawProgress - segStart) / (segEnd - segStart);
      const to = lerp(s.from, s.to, frac);
      drawn.push({ ...s, to });
      dot = to;
      break;
    }
  }
  return { drawn, dot };
}
```

- [ ] **Step 4: Run to verify pass**

Run: `pnpm vitest run src/aprs/digipeatAnim.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/aprs/digipeatAnim.ts src/aprs/digipeatAnim.test.ts
git commit -m "feat(aprs): pure trace schedule + path-trim geometry (tuxlink-qnu6)
<trailers>"
```

---

### Task 3: DigipeatPathLayer — Canvas2D overlay render

**Files:**
- Create: `src/aprs/DigipeatPathLayer.tsx`

**Interfaces:**
- Consumes: `traceProgress`, `trimPath`, `DEFAULT_TIMING` from `./digipeatAnim`; `PathSegment` from `./digipeatPath`; the live map from `LeafletMapContext` (`useLeafletMap()` — confirm the exact hook name in `src/map/LeafletMapContext.ts` while implementing).
- Produces: `function DigipeatPathLayer({ path }: { path: PathSegment[] | null }): null` — a render-less React component that, whenever `path` becomes non-null, starts one bounded trace; a new non-null `path` identity restarts the trace.

> **NOT unit-tested** (jsdom has no Canvas2D context). Verification is the operator grim-smoke. Implement carefully; mirror existing Leaflet-overlay patterns in `src/map/leafletHooks.ts` + the `safe()` guard in `AprsPositionsMap.tsx`.

- [ ] **Step 1: Implement the overlay layer**

Key implementation points (write the component to satisfy all of these):
1. Get the map: `const map = useLeafletMap()` (or the context's actual accessor). Bail (`return null`) if no map.
2. Create ONE `<canvas>` once and append it to the map's `overlayPane` (`map.getPanes().overlayPane`), sized to the map container, `position:absolute; pointer-events:none; left:0; top:0`. On every `move`/`zoom`/`resize` and at the start of each animation frame, set the canvas width/height to the current `map.getSize()` and offset it to the pane origin so map-pixel coords line up: translate by `map.containerPointToLayerPoint([0,0])` negated — simplest robust approach: project with `map.latLngToContainerPoint(latlng)` and position the canvas at the container's top-left (overlayPane is transformed; counter it by setting the canvas transform to `L.DomUtil.setPosition(canvas, map.containerPointToLayerPoint([0,0]).multiplyBy(-1))` each frame, OR append the canvas to `map.getContainer()` directly so container coords map 1:1 — PREFER appending to `map.getContainer()` and using `latLngToContainerPoint`, which avoids pane-transform math).
3. Animation: on a new non-null `path`, record `start = performance.now()` and start `requestAnimationFrame(loop)`. Each frame:
   - `const elapsed = performance.now() - start;`
   - `const { phase, drawProgress, opacity } = traceProgress(elapsed);`
   - `const { drawn, dot } = trimPath(path, drawProgress);`
   - Clear: `ctx.clearRect(0, 0, w, h)`.
   - `ctx.globalAlpha = opacity;`
   - For each `seg` in `drawn`: project `from`/`to` via `map.latLngToContainerPoint({lat,lng})` (note Leaflet uses `lng`), then stroke. `solid`: `ctx.setLineDash([])`; `dashed`: `ctx.setLineDash([6, 6])`. Style: `ctx.strokeStyle = '#f0c24a'` (the map's amber accent, matching uncertainty discs), `ctx.lineWidth = 2.5`, `ctx.lineCap = 'round'`.
   - Draw the dot: if `dot`, project it, `ctx.beginPath(); ctx.arc(px, py, 4, 0, 2*Math.PI); ctx.fillStyle = '#ffffff'; ctx.fill();` then a soft glow ring `ctx.arc(px,py,7,…); ctx.strokeStyle='#f0c24a'; ctx.lineWidth=1.5; ctx.stroke()`.
   - If `phase === 'done'`: clear the canvas, stop (do NOT schedule another frame). Else `raf = requestAnimationFrame(loop)`.
4. Reproject while active: also redraw on map `move`/`zoom` by keeping the loop running (the loop already reprojects every frame), so panning during a trace keeps the path glued to the map.
5. Cleanup: on unmount or a new `path`, `cancelAnimationFrame(raf)`, clear + remove listeners. On a new trace, restart `start` and the loop.
6. Wrap every map/canvas mutation in a `try/catch` that logs via `reportFrontendError('digipeat-path-anim', …)` and skips — never throw to the ErrorBoundary (mirror `AprsPositionsMap`'s `safe()`).

- [ ] **Step 2: Typecheck + build (no render test possible)**

Run: `pnpm typecheck && pnpm vitest run src/aprs/` (the existing AprsPositionsMap tests must still pass; this component isn't imported yet so it only needs to typecheck).
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src/aprs/DigipeatPathLayer.tsx
git commit -m "feat(aprs): Canvas2D digipeat-path overlay layer (render) (tuxlink-qnu6)
<trailers>"
```

---

### Task 4: Wire triggers into AprsPositionsMap

**Files:**
- Modify: `src/aprs/AprsPositionsMap.tsx` (add the layer + resolve the path on hover + on a new live frame)

**Interfaces:**
- Consumes: `DigipeatPathLayer` (Task 3), `resolveDigipeatPath` (Task 1). Uses the component's existing `byCall` position map + `HeardPosition.via` + the operator position (find how the map already obtains the operator's own location — grid-square centre — while implementing; reuse it).

- [ ] **Step 1: Add path state + resolve helper**

In `AprsPositionsMap`, add state `const [tracePath, setTracePath] = useState<PathSegment[] | null>(null)`. Add a memoized resolver that, given a station `call`, builds `ResolveInput` from that station's `HeardPosition` (`src` = its lat/lon + call, `via` = its `.via ?? []`), `located` = a `Map(call → {lat,lon})` from `positions`, `operator` = the operator position (or null), and returns `resolveDigipeatPath(input)`. Set `tracePath` to the result (or `null` if empty).

- [ ] **Step 2: Hover trigger**

In the pin marker creation (where `makeCircle`/markers get their handlers), add `marker.on('mouseover', () => setTracePath(resolve(call)))` and `marker.on('mouseout', () => setTracePath(null))`. (Keep using the ref pattern already in the file so the reconcile effect does not re-run on hover — read `resolve` from a ref.)

- [ ] **Step 3: Live trigger**

Where the component reacts to a new position (the `positions` change / the newest frame — reuse the existing "newest" detection if present, else track the max-timestamp call), fire a one-shot: `setTracePath(resolve(newestCall))`. The layer's bounded trace fades and stops on its own; a subsequent identical path is fine (a new trace restarts).

- [ ] **Step 4: Render the layer**

Add `<DigipeatPathLayer path={tracePath} />` inside the map's children (alongside the existing overlays consuming `LeafletMapContext`).

- [ ] **Step 5: Typecheck + map tests**

Run: `pnpm typecheck && pnpm vitest run src/aprs/ src/map/`
Expected: PASS (existing tests green; no new unit test for the render).

- [ ] **Step 6: Commit + open DRAFT PR**

```bash
git add src/aprs/AprsPositionsMap.tsx
git commit -m "feat(aprs): wire digipeat-path animation triggers (hover + live) (tuxlink-qnu6)
<trailers>"
git push -u origin bd-tuxlink-qnu6/digipeat-path-anim
gh pr create --draft --base main --title "[glade-gulch-fern] feat(aprs): restore cn84 directional path animation on Leaflet (tuxlink-qnu6)" --body "..."
```

---

## Acceptance (operator-gated, not a code task)

Automated gates (CI): `pnpm typecheck`, `pnpm vitest run` (pure resolver + schedule + trim geometry), `pnpm build` — all green.

**Operator grim-smoke on llvmpipe (the merge gate):** in a build of this branch, hover a station pin and observe a heard frame arrive. The path draws in hop-by-hop with the packet dot riding sender → operator, lingers, fades — and **zoom/pan stay smooth** (the failure mode that reverted cn84). Solid through located hops, dashed `pos?` across unlocated. Only merge the draft PR once the operator confirms smoothness on the real WebKitGTK app.

## Self-Review

- **Spec coverage:** pure resolver → Task 1; schedule + honest hop-by-hop trim → Task 2; Canvas2D overlay + bounded rAF + reproject + guarded mutation → Task 3; hover + live triggers + honest path resolution → Task 4; grim-smoke gate → Acceptance. No spec requirement without a task.
- **Placeholder scan:** Tasks 1–2 have full code; Task 3's render is enumerated point-by-point with exact ctx calls (it cannot be unit-tested, so it has an implementation spec + smoke gate rather than a red/green cycle); Task 4 cites exact handlers. The two "confirm while implementing" notes (the `useLeafletMap` accessor name, the operator-position source) are lookups in named files, not deferred design.
- **Type consistency:** `PathSegment`/`LatLon`/`ResolveInput` (Task 1) are consumed unchanged by `trimPath` (Task 2) and the resolve helper (Task 4); `traceProgress`/`trimPath`/`DEFAULT_TIMING` (Task 2) are consumed by `DigipeatPathLayer` (Task 3); `DigipeatPathLayer({ path })` (Task 3) is consumed by Task 4.
