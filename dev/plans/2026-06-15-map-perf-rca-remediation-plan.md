# Vector Map Performance Remediation Plan (tuxlink-vnk7)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the shipped vector-map performance defects (B1-B11 + overview-maxzoom clamp) so panning/loading/responsiveness on the Pi recover toward the software-GL budget, and add a real on-Pi frame-timing gate so the gap can't recur — all **within the settled architecture**.

**Architecture (SETTLED — do NOT reconsider):** MapLibre GL JS vector renderer, forced through **software GL (llvmpipe)** because Pi-5 hardware WebGL is non-functional (magenta static — commit `0fea60f7`). Tiles are PMTiles read via the `pmtiles` JS Protocol over a custom `tile://pmtiles/<id>` URI scheme (Rust 206 `read_range` seam). Dark mode is a baked-inverted GL-native style derived from the `@protomaps/basemaps` flavor. **No renderer pivot. No raster basemap. No new minimal style. Every fix below is look-preserving** (B1 removes *duplicate* layers, not the chosen base style; B6 re-enables collision culling). The meshmap/Protomaps aesthetic and the baked-dark look are unchanged.

**Tech Stack:** React 19, TypeScript, `maplibre-gl` 5.24.0, `pmtiles`, `@protomaps/basemaps@5`, Vitest (frontend); Rust + Tauri v2 (backend); pnpm.

**Source of truth:** [`dev/bug-hunts/2026-06-15-map-perf-rca-consolidated.md`](../bug-hunts/2026-06-15-map-perf-rca-consolidated.md) (full evidence + RCA + per-defect file:line + cross-validation).

**Working location:** worktree `worktrees/bd-tuxlink-vnk7-map-perf-rca` (branch `bd-tuxlink-vnk7/map-perf-rca`, off `main`; node deps installed). bd issue: `tuxlink-vnk7` (in_progress).

---

## Per-task discipline (applies to EVERY task)

**BEFORE starting any task:**
1. Read/invoke `superpowers:test-driven-development`.
2. Read `docs/pitfalls/testing-pitfalls.md` and `docs/pitfalls/implementation-pitfalls.md`.
3. Follow TDD: write the failing test → run it red → implement the minimal fix → run it green.

**BEFORE marking any task complete:**
1. Review your tests against `docs/pitfalls/testing-pitfalls.md`.
2. Confirm the fix's error/edge paths are covered.
3. Run the relevant test subset and confirm green (commands per task).

**After every logical group of tasks (each Tier below):** You MUST carefully review the batch of work from multiple perspectives and revise/refine as appropriate. Repeat this review loop (minimum three review rounds; if you still find substantive issues in the third review, keep going until there are no findings). Then update your private journal and continue.

**Hard reachability gate (end of plan):** Before any "map perf is fixed / shipped" claim, run the `wire-walk` skill on the map-open → pan → zoom → pack-installed flows AND run the on-Pi frame-timing smoke (Task 13). Partially-wired or unmeasured = not done.

**Critical context every subagent needs (no conversation history):**
- The MapLibre **mock** (`src/map/testMapLibreMock.ts`) is what unit tests run against — `setStyle`/`setData`/render are all free in the mock, so unit tests assert *call counts / shapes / identities*, NOT visual or timing behavior. Real timing is grim/Pi-only.
- `buildBasemapStyle(flavor, packs)` is a **pure** function — test it directly (the existing `basemapStyle.test.ts` exercises the REAL `@protomaps/basemaps`, not a mock).
- Do NOT remove the pack `background`-layer drop (`basemapStyle.ts:132-138`) or the opaque-origin URL handling (`absoluteBasemapUrl`) — both are load-bearing prior fixes with regression tests.
- **Composed-seam caution (testing-pitfalls tuxlink-k61j):** the maplibre mock can pass a per-layer unit test while the composed runtime seam is broken. Unit tests here prove *call counts / shapes / identities*; the **only** evidence that the cumulative perf actually improved is the on-Pi frame-timing smoke (Task 13). Do not claim a perf win from green unit tests alone.

---

## File Structure (what each task touches)

| File | Tasks | Responsibility |
|---|---|---|
| `src/map/darkStyle.ts` | T1 (B3) | memoized baked-dark transform |
| `src/map/basemapStyle.ts` | T1, T2, T6 (B3, B1, maxzoom) | style assembly + pack compositing + overview clamp |
| `src/map/MapLibreMap.tsx` | T3, T4, T7 (B7, B2, B8) | render profile, construct-with-packs, zoom-emit |
| `src-tauri/src/lib.rs` | T5 (B4) | Cache-Control header on pmtiles responses |
| `src-tauri/src/basemap/mod.rs` | T8 (B11) | read buffer allocation |
| `src/map/MaidenheadGridLayer.tsx` | T9 (B6) | grid recompute gating + collision culling |
| `src/catalog/StationFinderMap.tsx` | T10 (B9) | push-effect deps + setFeatureState selection |
| `src/location/LocationMap.tsx` | T11 (B6/B10) | drag rAF throttle + push-effect dep |
| `scripts/build-basemap-bundle.sh` + `docs/` | T12 (B5) | uncompressed-tile bundle decision |
| `dev/perf-harness/` (new) | T13 (D4) | on-Pi frame-timing smoke |

Tiers are ordered for measurement: **Tier 1 (cheap) → Tier 2 (structural) → measure → Tier 3 (churn) → Tier 4 (bundle/gate).**

> **Task numbering:** task numbers (T1, T2, …) are stable labels tied to the consolidated-report defect grouping, so they appear out of numeric order within tiers. **Execution order is strictly top-to-bottom within each tier, tiers in order.** Read and execute the plan sequentially as written; ignore the numeric labels for sequencing.

---

# TIER 1 — Cheap, isolated, high-leverage (no design dependency)

## Task 1 (B3): Memoize the baked-dark style so it is computed once per flavor, not per build

**Files:**
- Modify: `src/map/darkStyle.ts` (add a memo around `bakeDarkColors` usage) and/or `src/map/basemapStyle.ts:115-127`
- Test: `src/map/darkStyle.test.ts`, `src/map/basemapStyle.test.ts`

**Current behavior:** `buildBasemapStyle('dark')` calls `bakeDarkColors(layers(...))` on every invocation (mount, the post-load `setStyle`, every theme toggle, and once per pack), each time deep-copying every layer and regex-transforming every `*-color`. Unmemoized. Docstrings falsely claim "baked once at build time."

**Desired behavior:** the baked **base** dark layer array (the world-overview layers for `BASEMAP_SOURCE_ID`) is computed once and reused across calls (referential identity stable). The transform itself is unchanged (look-preserving). Per-pack layers are still derived per pack (different source id) but should reuse a cached transform result where the input is identical — at minimum, the base overview layers must be memoized.

- [ ] **Step 1: Write the failing test** (`src/map/basemapStyle.test.ts`, new `describe`)

```ts
describe('buildBasemapStyle (dark) memoization (B3)', () => {
  it('returns the SAME baked base layer array across calls (no per-build re-bake)', () => {
    const a = buildBasemapStyle('dark');
    const b = buildBasemapStyle('dark');
    // The overview (non-pack) layers must be the identical cached array reference,
    // proving bakeDarkColors ran once for the base, not on every call.
    expect(a.layers).toBe(b.layers);
  });
  it('light path is unaffected and still differs from dark per-color', () => {
    const dark = buildBasemapStyle('dark');
    const light = buildBasemapStyle('light');
    const bg = (s: typeof dark) =>
      (s.layers.find((x) => x.type === 'background') as { paint?: Record<string, unknown> })
        .paint?.['background-color'];
    expect(bg(dark)).not.toBe(bg(light));
  });
});
```

- [ ] **Step 2: Run red** — `pnpm -C . vitest run src/map/basemapStyle.test.ts` → first test FAILS (different array refs).

- [ ] **Step 3: Implement** — memoize the baked base layer set in `basemapStyle.ts`. Add a module-level cache keyed by flavor for the **base** (no-pack) layers:

```ts
// Module-level memo of the per-flavor base (overview) layer array. The protomaps
// layers() generator + bakeDarkColors are pure for a fixed (source, flavor), so
// compute once and reuse — the dark transform is multi-hundred-ms on the Pi (B3).
const baseLayerCache = new Map<BasemapFlavor, ReturnType<typeof layers>>();
function baseLayers(flavor: BasemapFlavor): ReturnType<typeof layers> {
  const hit = baseLayerCache.get(flavor);
  if (hit) return hit;
  const built = layers(BASEMAP_SOURCE_ID, tuxlinkFlavor(), { lang: 'en' });
  const baked = flavor === 'dark' ? bakeDarkColors(built) : built;
  baseLayerCache.set(flavor, baked);
  return baked;
}
```

Then in `buildBasemapStyle`, replace the base-layer construction so that when `packs` is empty it returns the cached array **by reference**:

```ts
export function buildBasemapStyle(flavor: BasemapFlavor, packs: PackSource[] = []): StyleSpecification {
  const base = baseLayers(flavor);
  const styleLayers = packs.length === 0 ? base : [...base];   // copy only when appending packs
  for (const pack of packs) { /* …unchanged pack loop (see Task 2) … */ }
  return { version: 8, glyphs: glyphsUrl(), sprite: absoluteBasemapUrl(`/basemap/sprites/${flavor}`), sources, layers: styleLayers };
}
```

Update the now-accurate docstrings in `darkStyle.ts:4-6` and `basemapStyle.ts:6` to say "baked once per flavor and memoized at runtime" (remove the false "build time" claim).

- [ ] **Step 4: Run green** — `pnpm -C . vitest run src/map/basemapStyle.test.ts src/map/darkStyle.test.ts` → all PASS. Verify the existing dark tests (lines 50-83) still pass (the transform output is unchanged — only its caching changed).

- [ ] **Step 5: Commit**

```bash
git add src/map/basemapStyle.ts src/map/darkStyle.ts src/map/basemapStyle.test.ts
git commit -m "perf(map): memoize baked-dark base layers — bake once per flavor, not per build (B3)

Agent: raven-poplar-clover
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

**Do NOT** change the color math (`transformChannels`/`HUE180`/`BRIGHTNESS`) — the look is settled.

---

## Task 2 (B1): Composite region packs as DETAIL-only layers (drop duplicated label/symbol layers), coverage-aware

**Files:**
- Modify: `src/map/basemapStyle.ts:139-154` (the pack loop)
- Test: `src/map/basemapStyle.test.ts` (extend the R7 `describe`)

**Current behavior:** each pack appends the FULL `@protomaps/basemaps` layer set (~68 after dropping `background`, ~13 of them `symbol`/label layers), gated only by `minzoom >= 6`, evaluated globally. N packs ≈ N× the entire style; labels (the most expensive llvmpipe primitive) are duplicated per pack and re-collide globally.

**Desired behavior (look-preserving):** a pack contributes only the **fill/line detail** that the overzoomed overview lacks; **labels/symbols come from the single base overview layer set only** (one label pass, not N+1). The base overview already owns places/POI/road labels; duplicating them per pack adds cost with no visual gain (same OSM names). So filter each pack's appended layers to non-`symbol` types (in addition to the existing `background` drop). This preserves the map's appearance (labels still render, from the base) while removing the per-pack label-collision storm.

> Rationale recorded for the executor: this is NOT a style redesign — the base style and its labels are untouched; we stop *duplicating* the label layers per pack. The detail (roads/water/landuse fills + lines) still wins inside pack coverage because those layers are appended on top at z≥6.

- [ ] **Step 1: Write the failing test**

```ts
it('a pack contributes NO duplicate symbol/label layers (labels come from the base only) (B1)', () => {
  const style = buildBasemapStyle('light', [{ id: 'continent-na' }]);
  const packLayers = style.layers.filter(
    (l): l is typeof l & { source: string } => 'source' in l && l.source === 'pack-continent-na',
  );
  expect(packLayers.length).toBeGreaterThan(0);          // detail layers present
  expect(packLayers.every((l) => l.type !== 'symbol')).toBe(true);  // but NO label layers
});

it('total label (symbol) layer count does NOT grow with pack count (B1)', () => {
  const symbolCount = (s: ReturnType<typeof buildBasemapStyle>) =>
    s.layers.filter((l) => l.type === 'symbol').length;
  const zero = symbolCount(buildBasemapStyle('light', []));
  const three = symbolCount(buildBasemapStyle('light', [{ id: 'a' }, { id: 'b' }, { id: 'c' }]));
  expect(three).toBe(zero);   // labels are owned by the base overview only
});
```

- [ ] **Step 2: Run red** — `pnpm -C . vitest run src/map/basemapStyle.test.ts` → both new tests FAIL.

- [ ] **Step 3: Implement** — in the pack loop, add a `symbol` filter alongside the existing `background` filter. Packs use a different source id than the base, so they do NOT reuse Task 1's base cache — `layers(sid, …)` is still called per pack, but now produces far fewer (detail-only) layers. Replace the loop body's filter (`basemapStyle.ts:146-152`) with:

```ts
    const packLayers = layers(sid, tuxlinkFlavor(), { lang: 'en' })
      .filter((layer) => layer.type !== 'background' && layer.type !== 'symbol')
      .map((layer) => ({
        ...(flavor === 'dark' ? bakeDarkColors([layer])[0] : layer),
        id: `${sid}-${layer.id}`,
        minzoom: Math.max(layer.minzoom ?? 0, REGION_MINZOOM),
      }));
    styleLayers.push(...packLayers);
```

(Keeps per-pack dark baking only for the retained detail layers — far fewer, and only colors. The base label layers are baked once via Task 1's cache.)

- [ ] **Step 4: Run green** — `pnpm -C . vitest run src/map/basemapStyle.test.ts` → all PASS, including the existing R7 tests (lines 121-183): "draws pack layers AFTER overview", "no extra background per pack", "source per pack" must still hold (the detail layers are still source-bound and appended on top).

- [ ] **Step 5: Commit**

```bash
git add src/map/basemapStyle.ts src/map/basemapStyle.test.ts
git commit -m "perf(map): region packs contribute detail-only layers, labels from base (B1)

Agent: raven-poplar-clover
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

**Boundary:** do NOT also bbox-gate pack layers in this task (the manifest has `bbox` but wiring per-layer `filter` by bbox is a larger change and the symbol drop captures most of the cost). If after measurement (Task 13) packs are still heavy, bbox-gating is the documented follow-up.

---

## Task 3 (B7): Add a software-GL render profile to the MapLibre constructor

**Files:**
- Modify: `src/map/MapLibreMap.tsx:130-145`
- Test: `src/map/MapLibreMap.test.tsx`

**Current behavior:** the constructor sets `renderWorldCopies:false` + `attributionControl:false` but leaves `pixelRatio` (defaults to `devicePixelRatio` — quadratic fill cost at DPR>1), `fadeDuration` (defaults to 300ms), and `validateStyle` at defaults.

**Desired behavior:** on this software-GL substrate, construct with `pixelRatio: 1`, `fadeDuration: 0`, and (production) `validateStyle: false`. These are universally safe on the Pi target; `pixelRatio:1` is the single biggest lever if the panel reports DPR>1.

- [ ] **Step 1: Write the failing test** (extend `MapLibreMap.test.tsx`'s construction test)

```ts
it('constructs with the software-GL render profile (B7)', () => {
  render(<MapLibreMap />);
  const opts = vi.mocked(maplibregl.Map).mock.calls[0][0];
  expect(opts.pixelRatio).toBe(1);
  expect(opts.fadeDuration).toBe(0);
});
```

- [ ] **Step 2: Run red** — `pnpm -C . vitest run src/map/MapLibreMap.test.tsx` → FAIL (undefined options).

- [ ] **Step 3: Implement** — add to the `new maplibregl.Map({ … })` options (after `renderWorldCopies: false`):

```ts
        // Software-GL (llvmpipe) render profile (B7). pixelRatio:1 avoids the
        // quadratic fill cost of a HiDPI canvas the CPU rasterizer can't afford;
        // fadeDuration:0 drops per-tile/symbol cross-fade passes during loads.
        pixelRatio: 1,
        fadeDuration: 0,
```

(Leave `validateStyle` out unless the build distinguishes prod/dev cleanly — adding it risks masking style errors in dev. `pixelRatio` + `fadeDuration` are the safe, high-value pair.)

- [ ] **Step 4: Run green** — `pnpm -C . vitest run src/map/MapLibreMap.test.tsx` → PASS.

- [ ] **Step 5: Commit**

```bash
git add src/map/MapLibreMap.tsx src/map/MapLibreMap.test.tsx
git commit -m "perf(map): software-GL render profile (pixelRatio:1, fadeDuration:0) (B7)

Agent: raven-poplar-clover
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 5 (B4): Add `Cache-Control: immutable` to the pmtiles 206 responses

**Files:**
- Modify: `src-tauri/src/lib.rs` (the pmtiles response builder, ~`:271-296`)
- Test: `src-tauri/src/basemap/mod.rs` (if `read_range`'s response struct carries headers) OR a `lib.rs` unit test asserting the header set

**Current behavior:** the 206/200 response sets Content-Type, Accept-Ranges, CORS, Expose-Headers, Content-Length, ETag, Content-Range — but no `Cache-Control`. The pmtiles JS client may refetch immutable directory/leaf ranges repeatedly during pan/zoom.

**Desired behavior:** every successful pmtiles response carries `Cache-Control: public, max-age=31536000, immutable`. Bytes are immutable for the session (ETag is length-derived), so this is safe and changes no behavior except caching.

- [ ] **Step 1: Write the failing test** — add to the `mod.rs` test module (or a new lib.rs test) an assertion. If `read_range` returns a struct without headers, the header is added at the lib.rs builder; in that case add a small pure helper `pmtiles_cache_control() -> &'static str` in `basemap/mod.rs` and unit-test it, then use it in lib.rs:

```rust
#[test]
fn pmtiles_responses_are_cacheable_immutable() {
    assert_eq!(
        crate::basemap::PMTILES_CACHE_CONTROL,
        "public, max-age=31536000, immutable"
    );
}
```

- [ ] **Step 2: Run red** — `cargo test -p <crate> --manifest-path src-tauri/Cargo.toml pmtiles_responses_are_cacheable` → FAIL (const missing). (Per project policy, the cold cargo build runs in CI; locally just confirm it compiles-or-fails fast. If local cargo is impractical, push and let CI gate — see implementation note.)

- [ ] **Step 3: Implement** — add the const in `basemap/mod.rs` near `PMTILES_CONTENT_TYPE`:

```rust
/// Cache directive for pmtiles range responses. The archive bytes are immutable
/// for the session (the ETag is length-derived), so the webview/pmtiles client
/// may cache directory + leaf ranges indefinitely instead of refetching them on
/// every tile resolution during pan/zoom (B4, tuxlink-vnk7).
pub const PMTILES_CACHE_CONTROL: &str = "public, max-age=31536000, immutable";
```

Then add the header in the lib.rs success builder (alongside ETag, ~`:289`):

```rust
                                .header(
                                    tauri::http::header::CACHE_CONTROL,
                                    crate::basemap::PMTILES_CACHE_CONTROL,
                                )
```

- [ ] **Step 4: Run green** — `cargo test --manifest-path src-tauri/Cargo.toml pmtiles_responses_are_cacheable` (or via CI). Confirm existing basemap tests still pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/basemap/mod.rs
git commit -m "perf(basemap): Cache-Control immutable on pmtiles range responses (B4)

Agent: raven-poplar-clover
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

**Implementation note (project policy):** do not cold-build cargo locally on the contended Pi to "verify" — push and let GitHub CI compile (memory: no-cold-cargo). The unit test is the gate; CI is the compiler.

---

## Task 8 (B11): Avoid zero-initializing the read buffer on the hot path

**Files:**
- Modify: `src-tauri/src/basemap/mod.rs:81-92` (`read_range_capped`)
- Test: existing `read_range` tests (behavior unchanged) cover correctness

**Current behavior:** `vec![0u8; n]` zero-inits a buffer the kernel immediately overwrites via `read_exact_at`/positional read — one alloc + a needless memset per request on `spawn_blocking` threads contending with llvmpipe.

**Desired behavior:** read into an uninitialized-capacity buffer (e.g. read into a `Vec::with_capacity` via `take().read_to_end`, or `read_exact` into a slice of a `Vec` sized but not zeroed using safe APIs). Keep it SAFE Rust — no `unsafe`. The simplest safe win: use `(&file).take(len).read_to_end(&mut buf)` with `buf = Vec::with_capacity(n)`.

- [ ] **Step 1: Confirm tests exist** — the existing `read_range_*` tests (mod.rs:402-450) assert correctness of the bytes/status/content-range. They must continue to pass unchanged (this is a perf-only refactor).

- [ ] **Step 2: Implement** — replace the `vec![0u8; n]` + positional-read with a capacity-based read that does not pre-zero. Keep the exact same returned bytes and clamping behavior. (Executor: read the current body and translate; do not change the cap logic or the `content_range` computation.)

- [ ] **Step 3: Run green** — `cargo test --manifest-path src-tauri/Cargo.toml read_range` (or CI) → all existing read_range tests PASS.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/basemap/mod.rs
git commit -m "perf(basemap): read range into uninit-capacity buffer, drop zero-memset (B11)

Agent: raven-poplar-clover
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

**Boundary:** SAFE Rust only. If a clean safe rewrite isn't obvious, SKIP this task (B11 is minor, dominated by B4/B5) and note it in the deferred appendix rather than introducing `unsafe`.

### ▶ Tier 1 review loop (≥3 rounds) before Tier 2.

---

# TIER 2 — Structural compositing (coordinated; same files as Tier 1 style work)

## Task 4 (B2): Construct the map with installed packs already known — drop the post-load `setStyle`

**Files:**
- Modify: `src/map/MapLibreMap.tsx:109-145` (construct effect) + `:219-228` (rebuild effect)
- Test: `src/map/MapLibreMap.test.tsx`

**Current behavior:** the map constructs overview-only (no packs); `fetchPacks()` resolves async, `setPacks()` flips the style key, and the rebuild effect calls `map.setStyle(...)` moments after first paint whenever any pack is installed — a full teardown/reparse/re-decode/overlay-re-add storm on the busiest substrate.

**Desired behavior:** gate map construction on the first `fetchPacks()` result so the initial `buildBasemapStyle(flavor, packs)` already includes installed packs. `setStyle` then fires ONLY for genuine post-mount changes (flavor swap, or a `BASEMAP_PACKS_CHANGED_EVENT` after the user installs/deletes a pack) — never as a cold-open artifact. Preserve the existing key-guard so a redundant render never reloads.

- [ ] **Step 1: Write the failing test** — assert that on mount with packs present, `setStyle` is NOT called (construction already carried them), and that a later packs-changed event DOES call `setStyle` once:

```ts
it('does not setStyle on cold open when packs are present at construct time (B2)', async () => {
  vi.mocked(invoke).mockResolvedValueOnce({ packs: [{ id: 'continent-na' }] });
  render(<MapLibreMap />);
  // allow fetchPacks + load to settle
  await waitFor(() => expect(vi.mocked(maplibregl.Map)).toHaveBeenCalled());
  // Capture the constructed map handle the SAME way the existing tests in this
  // file do (the maplibre mock returns a handle from `new maplibregl.Map()`;
  // reuse that file's established capture pattern — do not invent a new one).
  const inst = vi.mocked(maplibregl.Map).mock.results[0].value;
  // The initial style passed to the constructor includes the pack source…
  const opts = vi.mocked(maplibregl.Map).mock.calls[0][0];
  expect(Object.keys((opts.style as any).sources)).toContain('pack-continent-na');
  // …and no post-load setStyle fired for cold open.
  expect(inst.setStyle).not.toHaveBeenCalled();
});
```

(Executor: adapt to the existing mock's handle-capture pattern in `MapLibreMap.test.tsx`. If gating construction on async packs is too invasive for the mock's `load` simulation, the acceptable alternative is to seed the rebuild effect's `styleKeyRef` to the REAL initial key including packs so the first packs resolution is a no-op — assert `setStyle` not called on cold open either way.)

- [ ] **Step 2: Run red** → FAIL (currently setStyle fires).

- [ ] **Step 3: Implement** — two viable shapes; pick the simpler that passes:
  - (a) Defer construction until the first `fetchPacks` resolves (render a loading container; construct in an effect that depends on a `packsLoaded` flag), building the style with packs; OR
  - (b) Keep construction immediate but make the **first** packs resolution reconcile into the existing style WITHOUT setStyle when the packs equal what construction used — i.e. seed `styleKeyRef.current` to the full initial key `${flavor}|${initialPackIds}` and construct with those packs.

Prefer (a) for correctness (the construct-time style truly carries packs). Keep the `BASEMAP_PACKS_CHANGED_EVENT` → `fetchPacks` → key-change → `setStyle` path for genuine later changes.

- [ ] **Step 4: Run green** — `pnpm -C . vitest run src/map/MapLibreMap.test.tsx src/map/MapLibreMap.errorFallback.test.tsx` → all PASS.

- [ ] **Step 5: Commit**

```bash
git add src/map/MapLibreMap.tsx src/map/MapLibreMap.test.tsx
git commit -m "perf(map): construct map with installed packs; drop post-load setStyle (B2)

Agent: raven-poplar-clover
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

**Boundary:** do NOT switch pack compositing to incremental `addSource`/`addLayer` in this task (larger change, interacts with the owned-hook overlay re-add). Construct-with-packs + reserving setStyle for real changes is the scoped fix. Incremental compositing is a documented follow-up if measurement shows the flavor-swap setStyle is still too costly.

## Task 6 (maxzoom / D3): Clamp the overview source so it doesn't overzoom 8 levels outside pack coverage

**Files:**
- Modify: `src/map/basemapStyle.ts:118-124` (overview source) — set `maxzoom`
- Test: `src/map/basemapStyle.test.ts`

**Current behavior:** the overview vector source is unclamped; with interactive `MAP_MAX_ZOOM=14`, outside any pack the CPU rasterizes up to 8 overzoom levels of stretched z6 geometry for no added detail.

**Desired behavior:** set the overview source `maxzoom` to the overview's real max (`REGION_MINZOOM`/the bundle's z6, i.e. `maxzoom: 7` to allow one overzoom step for smoothness) so MapLibre stops requesting/over-rasterizing deep overview tiles. Pack sources keep their own (z14) detail. The "never blank" behavior is preserved — overzoom of the clamped source still fills the viewport, just capped.

- [ ] **Step 1: Write the failing test**

```ts
it('clamps the overview source maxzoom so it does not overzoom to z14 (D3/maxzoom)', () => {
  const style = buildBasemapStyle('light');
  const src = style.sources[BASEMAP_SOURCE_ID] as { maxzoom?: number };
  expect(src.maxzoom).toBeLessThanOrEqual(7);
});
```

- [ ] **Step 2: Run red** → FAIL (no maxzoom).

- [ ] **Step 3: Implement** — add `maxzoom` to the overview source:

```ts
    [BASEMAP_SOURCE_ID]: {
      type: 'vector',
      url: PMTILES_SOURCE_URL,
      attribution: OSM_ATTRIBUTION,
      maxzoom: 7, // overview is z0–6; allow one overzoom step, no deep overzoom (D3)
    },
```

- [ ] **Step 4: Run green** — `pnpm -C . vitest run src/map/basemapStyle.test.ts` → PASS (existing tests unaffected).

- [ ] **Step 5: Commit**

```bash
git add src/map/basemapStyle.ts src/map/basemapStyle.test.ts
git commit -m "perf(map): clamp overview source maxzoom to stop 8-level overzoom (D3)

Agent: raven-poplar-clover
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

### ▶ Tier 2 review loop (≥3 rounds). ▶▶ MEASURE on-Pi (Task 13) before Tier 3 if the operator runs the smoke — Tier 1+2 are the bulk of the cheap wins.

---

# TIER 3 — Overlay / interaction churn

## Task 9 (B6): Gate the grid recompute to actual level/bounds change; subscribe `styledata` once; restore collision culling

**Files:**
- Modify: `src/map/MaidenheadGridLayer.tsx:96-140` + `:45-58` (label layout)
- Test: `src/map/mapHooks.test.tsx` or a new `MaidenheadGridLayer.test.tsx`

**Current behavior:** every `moveend` bumps a tick → re-render → fresh `getBounds()` object → the bounds-keyed `useMemo` rebuilds the full lattice + `setData` on every pan; the push effect depends on the new `geojson` object so it re-subscribes `styledata` each move; labels force `text-allow-overlap:true` + `text-ignore-placement:true` (no collision culling).

**Desired behavior:** (a) recompute only when the **rounded** bounds or grid **level** actually change (skip pans that don't cross a cell boundary); (b) subscribe `styledata` ONCE (read the latest geojson from a ref inside the handler); (c) remove the forced `text-allow-overlap`/`text-ignore-placement` so MapLibre culls overlapping labels (large rasterization win at wide zoom).

- [ ] **Step 1: Write the failing test** — assert `gridToGeoJSON`/`setData` is NOT called again when a `moveend` leaves the rounded bounds + level unchanged. (Executor: use the maplibre mock; spy on the source's `setData`; fire two `moveend`s with the same mocked bounds; assert `setData` call count does not increase on the second.)

- [ ] **Step 2: Run red** → FAIL (recomputes every moveend).

- [ ] **Step 3: Implement** —
  - Round bounds before keying the memo (e.g. round each edge to the grid cell size for the current level), so identical rounded bounds reuse the memo.
  - Replace the `geojson`-dep push effect with a ref-based one: keep `geojsonRef.current = geojson`; subscribe `styledata` once in an effect keyed on `[map]`; the handler reads `geojsonRef.current`. Push imperatively in a separate `useEffect([geojson])` (setData only when geojson changes).
  - Remove `'text-allow-overlap': true` and `'text-ignore-placement': true` from `LABEL_LAYER.layout` (lines 54-55).

- [ ] **Step 4: Run green** — `pnpm -C . vitest run src/map/` → all map tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/map/MaidenheadGridLayer.tsx src/map/*.test.tsx
git commit -m "perf(map): grid recompute gated to level/bounds change; cull labels (B6)

Agent: raven-poplar-clover
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

## Task 7 (B8): Emit `onZoomChange` only when zoom actually changes

**Files:**
- Modify: `src/map/MapLibreMap.tsx:156,161`
- Test: `src/map/MapLibreMap.test.tsx`

**Current behavior:** `emitZoom` fires on every `moveend` (pans included) straight into consumer `useState` setters (e.g. `PositionPickerOverlay` `setViewZoom`), re-rendering the modal+map subtree at the end of every drag even when zoom is unchanged.

**Desired behavior:** emit only when the rounded zoom differs from the last emitted value.

- [ ] **Step 1: Write the failing test** — fire two `moveend`s at the same zoom; assert `onZoomChange` called once (after load) not twice.

- [ ] **Step 2: Run red** → FAIL.

- [ ] **Step 3: Implement** — hold a `lastZoomRef`; in `emitZoom`, compare and early-return if unchanged:

```ts
      const lastZoom = { current: NaN as number };
      const emitZoom = () => {
        const z = instance.getZoom();
        if (z === lastZoom.current) return;
        lastZoom.current = z;
        onZoomRef.current?.(z);
      };
```

(Keep emitting on `load` to seed; keep the separate `moveend` clamp handler unchanged.)

- [ ] **Step 4: Run green** — `pnpm -C . vitest run src/map/MapLibreMap.test.tsx` → PASS.

- [ ] **Step 5: Commit**

```bash
git add src/map/MapLibreMap.tsx src/map/MapLibreMap.test.tsx
git commit -m "perf(map): emit onZoomChange only on real zoom change, not every pan (B8)

Agent: raven-poplar-clover
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

## Task 10 (B9): Stabilize StationFinderMap overlay pushes; use `setFeatureState` for selection

**Files:**
- Modify: `src/catalog/StationFinderMap.tsx:109-159`
- Test: `src/catalog/StationFinderMap.test.tsx` (existing)

**Current behavior:** `usePushData` has `data` in its effect deps (`:126`) → re-subscribes `styledata` and full-replaces the source on every `fc` change; `fc` rebuilds on `[stations, tiers, selectedKey]` where `tiers` is a Map prop (identity churn); a selection click rebuilds the ENTIRE FeatureCollection just to flip one `selected` flag.

**Desired behavior:** (a) `usePushData` subscribes `styledata` once (ref the latest data; setData on change in a separate effect); (b) selection toggling uses `map.setFeatureState({source, id}, {selected})` + a `feature-state`-driven paint expression instead of rebuilding the FC; (c) `fc` excludes `selectedKey` from its deps once selection is feature-state-driven (rebuild only when `stations`/`tiers` change).

- [ ] **Step 1: Write the failing test** — assert that changing `selectedKey` does NOT trigger a `setData` (selection now via setFeatureState). Assert `setData` is called when `stations` change. (Executor: adapt to the existing StationFinderMap test + maplibre mock; the mock must expose `setFeatureState` — extend `testMapLibreMock.ts` if needed, mirroring how it exposes `setData`.)

- [ ] **Step 2: Run red** → FAIL.

- [ ] **Step 3: Implement** —
  - Give each station feature a stable `id` (the `key`) so `setFeatureState` can target it; set `promoteId` on the source or use the feature `id` field.
  - Change the `circle-radius`/`circle-stroke-width` `['get','selected']` expressions to `['feature-state','selected']`.
  - On select, clear the previous selected feature-state and set the new one; drop `selected` from `buildStationFC` and from the `fc` memo deps.
  - Refactor `usePushData` to subscribe `styledata` once + setData-on-change (same pattern as Task 9).

- [ ] **Step 4: Run green** — `pnpm -C . vitest run src/catalog/` → PASS.

- [ ] **Step 5: Commit**

```bash
git add src/catalog/StationFinderMap.tsx src/map/testMapLibreMock.ts src/catalog/StationFinderMap.test.tsx
git commit -m "perf(map): StationFinderMap selection via setFeatureState; push once (B9)

Agent: raven-poplar-clover
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

**Boundary:** preserve the exact pin colors/radii/stroke values (look-preserving) — only the *trigger* changes from `get` to `feature-state`.

## Task 11 (B10 + B6-push): rAF-throttle the LocationMap drag; subscribe `styledata` once

**Files:**
- Modify: `src/location/LocationMap.tsx:124-183`
- Test: `src/location/LocationMap.test.tsx` (existing)

**Current behavior:** the marker drag `onMove` calls `setData(buildFC(...))` on every pointer `mousemove` (`:159-163`); the push effect depends on `fc` (`:135`) so it re-subscribes `styledata` each data change.

**Desired behavior:** (a) coalesce drag `setData` to one update per `requestAnimationFrame`; (b) subscribe `styledata` once (ref latest fc).

- [ ] **Step 1: Write the failing test** — fire 5 synchronous `mousemove`s during a drag; assert `setData` is called at most once per frame (mock `requestAnimationFrame` to a manual flush; assert coalescing). And assert the `styledata` handler is added once across fc changes.

- [ ] **Step 2: Run red** → FAIL.

- [ ] **Step 3: Implement** — in `onMove`, store the latest lngLat and schedule a single rAF that calls `setData`; cancel any pending frame on `onUp`/cleanup. Refactor the push effect to the ref-based once-subscribe pattern (as Task 9/10).

- [ ] **Step 4: Run green** — `pnpm -C . vitest run src/location/` → PASS.

- [ ] **Step 5: Commit**

```bash
git add src/location/LocationMap.tsx src/location/LocationMap.test.tsx
git commit -m "perf(map): rAF-throttle LocationMap drag; subscribe styledata once (B10)

Agent: raven-poplar-clover
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

> **GridPicker drag (B10, second site):** apply the same rAF-throttle to `GridPicker.tsx:169-172,216-233` if present in scope. Same pattern; same test approach. If GridPicker is out of the active feature surface, note it in the deferred appendix.

### ▶ Tier 3 review loop (≥3 rounds).

---

# TIER 4 — Bundle decode + the perf gate

## Task 12 (B5): Decide + apply the per-tile decompression mitigation

**Files:**
- Modify: `scripts/build-basemap-bundle.sh` (the `pmtiles extract`/build invocation) and any region-pack build; `docs/design/2026-06-13-self-hosted-vector-osm-basemap-design.md` (note the decision)
- Test: a check that the produced bundle's tiles are uncompressed (or a documented decision that DecompressionStream is present + adequate)

**Current behavior:** the bundle ships gzip-compressed MVT tiles; the `pmtiles`/MapLibre JS path inflates each newly-visible tile on the JS thread (sync `fflate.decompressSync` if `DecompressionStream` is absent), competing with llvmpipe.

**Desired behavior:** EITHER (a) verify `DecompressionStream` exists on the Pi's WebKitGTK and that worker-side decode is adequate (document the finding), OR (b) bake the bundle + region packs with **uncompressed** tiles (`pmtiles` build flag to disable per-tile gzip) so zero main-thread inflate happens — trading larger local bytes (cheap on disk; 442GB free) for CPU.

- [ ] **Step 1: Probe** — on the Pi's WebKitGTK (via the render-harness or a one-liner in the dev webview console), check `typeof DecompressionStream`. Record the result in the design doc + this plan.

- [ ] **Step 2: Decide + implement** —
  - If `DecompressionStream` is **absent** (likely the perf cause): rebuild the bundle uncompressed. Find the extract/convert step in `scripts/build-basemap-bundle.sh` and add the no-tile-compression flag (e.g. `pmtiles convert --no-tile-compression` or the `tippecanoe`/build equivalent for the pinned pipeline). Region packs are extracted on-demand from the public planet via `go-pmtiles` — note that those stay gzip unless the extractor can decompress; if so, document that region-pack tiles remain compressed and the mitigation is bundle-only + the operator-side decision.
  - If present + adequate: document that B5 is accepted-as-is with the probe evidence; no bundle change.

- [ ] **Step 3: Verify** — if rebuilt: confirm the bundle still renders (grim/Pi smoke) and that the world overview loads. Confirm `REQUIRED_LAYER_IDS` validation (the 9 protomaps ids) still passes for the rebuilt bundle.

- [ ] **Step 4: Commit** — the build-script change + the design-doc decision note:

```bash
git add scripts/build-basemap-bundle.sh docs/design/2026-06-13-self-hosted-vector-osm-basemap-design.md
git commit -m "perf(basemap): ship uncompressed bundle tiles to drop main-thread inflate (B5)

Agent: raven-poplar-clover
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

**Boundary:** the bundle rebuild is an out-of-band build the OPERATOR runs (it regenerates a binary resource). The task delivers the build-script change + the decision; the operator regenerates + commits the bundle artifact per the existing bundle-build process.

## Task 13 (D4): Add a real on-Pi frame-timing perf smoke (the new gate)

**Files:**
- Create: `dev/perf-harness/README.md`, `dev/perf-harness/measure.py` (or extend `dev/render-harness/`)
- Modify: `docs/pitfalls/testing-pitfalls.md` (add the perf-forecast pitfall)

**Current behavior:** the only "perf" evidence was the mocked, front-end-only render-harness — it omits real decode/markers/packs, so its fps number was never an app-level prediction. No real frame-timing gate exists.

**Desired behavior:** a dev harness that drives the REAL packaged/dev app in WebKitGTK on the Pi under software GL, at the real window resolution, with the world pack + ≥1 region pack + station pins + the grid mounted, scripts a pan/zoom sequence, and reports p50/p95 frame time (and input latency if feasible). It is a manual/operator-run smoke (like the grim visual smoke), NOT a CI gate (no Pi compute in CI).

- [ ] **Step 1: Build the harness** — mirror `dev/render-harness/`'s approach (Vite-served front end + real WebKitGTK via GObject introspection, software-GL env) but: (a) load a route with a pack installed + pins + grid; (b) drive a deterministic pan/zoom script via `map` events or synthetic input; (c) sample `requestAnimationFrame` deltas over the run and emit p50/p95.

- [ ] **Step 2: Document** — `dev/perf-harness/README.md` with the exact run command (software-GL env vars, the route, the expected output) and the pass threshold (e.g. p95 frame time target). Note PNGs/outputs are gitignored.

- [ ] **Step 3: Add the testing-pitfall** — append to `docs/pitfalls/testing-pitfalls.md`:

> **MAP-PERF-1 — A perf forecast from the mocked render-harness is not an app-level prediction.** The front-end render-harness uses canned Tauri data and a trivial scene (no real tile decode, markers, or pack compositing). Its fps number MUST NOT gate a perf-sensitive map ship. Map perf claims require the on-Pi frame-timing smoke (`dev/perf-harness/`) with a region pack + pins + grid mounted at real resolution under software GL, measuring p50/p95 frame time during scripted pan/zoom. (tuxlink-vnk7, 2026-06-15.)

- [ ] **Step 4: Commit**

```bash
git add dev/perf-harness/ docs/pitfalls/testing-pitfalls.md
git commit -m "test(map): on-Pi frame-timing perf harness + perf-forecast pitfall (D4)

Agent: raven-poplar-clover
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

### ▶ Tier 4 review loop (≥3 rounds). Then run the on-Pi smoke (operator) to measure the cumulative effect and decide whether any deferred items (bbox-gating, incremental compositing, GL-probe gate for desktop Linux) are needed.

---

## Appendix: Bugs / levers identified but deferred this cycle

### A1. Per-layer bbox gating of pack detail layers
**Location:** `basemapStyle.ts` pack loop. **Why deferred:** Task 2's symbol-drop captures most of the per-pack cost; per-layer `filter`-by-bbox is a larger change. **Recommended:** if Task 13 shows packs still heavy, add a bbox `filter` (or source `bounds`) per pack using the manifest `bbox`.

### A2. Incremental pack compositing via addSource/addLayer (avoid setStyle on pack change)
**Location:** `MapLibreMap.tsx` rebuild effect. **Why deferred:** Task 4 removes the cold-open setStyle; the remaining setStyle only fires on explicit flavor swap or pack add/delete (rare, user-initiated). **Recommended:** convert pack add/delete to incremental source/layer ops if the flavor-swap/pack-change setStyle is still a visible hitch.

### A3. Desktop-Linux hardware-GL probe gate
**Location:** `lib.rs` `LINUX_WEBVIEW_GL_ENV`. **Why deferred:** does NOT help the Pi (the target — its hardware GL is broken). **Recommended (already noted in commit 0fea60f7):** a startup GL-probe so desktop-Linux users with working GPUs keep hardware accel; pure win for that audience, no effect on the Pi.

### A4. GridPicker drag rAF-throttle
**Location:** `GridPicker.tsx:169-233`. **Why deferred:** same pattern as Task 11; included there if in active scope, else here.

### A5. B11 if no safe rewrite
If the zeroed-`Vec` removal can't be done in safe Rust cleanly, leave it — minor, dominated by B4/B5.

---

## Self-review (completed by author)
- **Coverage:** every confirmed defect B1-B11 maps to a task (B1→T2, B2→T4, B3→T1, B4→T5, B5→T12, B6→T9+T11, B7→T3, B8→T7, B9→T10, B10→T11+A4, B11→T8) + D3→T6 + D4→T13. ✔
- **Architecture guardrail:** no task pivots the renderer, introduces raster, or authors a new style; all are look-preserving. ✔
- **Type consistency:** `buildBasemapStyle(flavor, packs)`, `PMTILES_CACHE_CONTROL`, `baseLayers`, `feature-state` selection names are consistent across tasks. ✔
- **No placeholders:** each code step shows the concrete change or names the exact file:line + pattern to translate (Rust tasks defer the cold compile to CI per project policy). ✔
