# Bug Hunt Report — LAN offline-tiles + Find-a-Station (multipass)

Agent: dahlia-spruce-osprey
Date: 2026-06-12
Worktree: `worktrees/bd-tuxlink-k61j-tile-maxzoom-reactive/`
Branch: `bd-tuxlink-k61j/tile-maxzoom-reactive`

## Scope

Full "operator configures a LAN raster XYZ tile server → binds → serves over
`tile://` → map renders + zooms past the bundled raster z3 cap" path.

Rust (`src-tauri/src/`): `tiles/{mod,commands,fetch,serve,cache,host,coord,breaker}.rs`,
`config.rs` (`map_tile_source`), `ui_commands.rs` (`config_read` → `ConfigViewDto`),
`lib.rs` (tile scheme handler + boot-seed).
Frontend (`src/`): `map/{BaseMap,TileLayerBridge,useTileSource,tileSource,tileSourceEvent,projection}`,
`catalog/StationFinderMap.tsx`, `settings/MapTileSourceSettings.tsx`.

All five passes performed: (1) contract violations, (2) cross-sibling patterns,
(3) failure-mode reasoning, (4) concurrency, (5) error propagation. Test files
were not audited for correctness (per skill); they were read only to confirm
intended contracts.

---

## Bugs

### B1 — TMS sources double-flip Y: every TMS tile request fetches the wrong tile

**Location:** `src/map/TileLayerBridge.tsx:54` (`tms={source.scheme === 'Tms'}`) ⇄ `src-tauri/src/tiles/fetch.rs:262-263` + `src-tauri/src/tiles/coord.rs:87-93`
**Severity:** critical
**Evidence:**
Leaflet's `TileLayer.getTileUrl` already inverts Y when `tms: true`. Verified in
the bundled Leaflet 1.9.4 source (`node_modules/.pnpm/leaflet@1.9.4/.../leaflet-src.js:12207-12213`):

```js
if (this._map && !this._map.options.crs.infinite) {
  var invertedY = this._globalTileRange.max.y - coords.y;  // (2^z - 1) - y_xyz
  if (this.options.tms) { data['y'] = invertedY; }          // {y} = TMS y
  data['-y'] = invertedY;
}
```

So for a TMS source the webview requests `tile://localhost/{z}/{x}/{tms_y}` where
`tms_y = (2^z - 1) - y_xyz` — the URL already carries the TMS-flipped Y.

That path lands in `serve.rs::parse_zxy` → `TileCoord::from_parts(z, x, tms_y, max_zoom)`,
so `coord.y` is **already the TMS Y**. `build_tile_url` then computes
`coord.upstream_y(tms=true)` (`fetch.rs:262-263`), which flips a **second** time
(`coord.rs:89`: `(1<<z) - 1 - self.y`). Net effect: `y_upstream = (2^z-1) - ((2^z-1) - y_xyz) = y_xyz`.

The backend therefore requests the **XYZ** Y from a **TMS** server — the
vertically-mirrored wrong tile at every coordinate (except the grid's vertical
centre row, which is its own mirror). The unit test `tms_scheme_flips_y_in_url`
(`fetch.rs:912`) only proves the backend flips once in isolation; it cannot see
Leaflet's prior flip because the test calls `fetch_tile_bytes` directly with a
hand-built XYZ coord, bypassing the webview→`tms`-option flip. The end-to-end
contract (Leaflet `tms` + backend `upstream_y`) is never exercised together, so
the double-flip is invisible to the suite.

**Impact:** Every TMS source (the doc explicitly calls out `.mbtiles`-backed
sources as "usually TMS", `MapTileSourceSettings.tsx:243-245`) renders a
vertically-scrambled map — each tile shows content from the mirror-image row.
The operator sees tiles load (so it is not "blank map") but the geography is
wrong, which is harder to diagnose than a blank. XYZ sources are unaffected.

**Fix approach:** Pick ONE flip site. Cleanest: set `tms={false}` on the Leaflet
`TileLayer` unconditionally (let the `tile://` template always carry the XYZ Y),
and keep the backend's `upstream_y(tms)` as the sole flip. The webview is an
internal transport to our own handler, not a real TMS server, so Leaflet's TMS
convenience flip is the wrong layer to do it at. Then add an end-to-end test that
drives a TMS source through the `tile://` path shape (z/x/y string → `serve_tile`
→ asserts the upstream path the mock receives is the TMS Y). Alternative (worse):
strip the backend flip and rely on Leaflet — but the backend is the documented
SSRF/coordinate authority, so the flip belongs there.

---

### B2 — Offline-with-cache regresses to the bundled raster instead of serving cached tiles (`lan-cached` is dead code)

**Location:** `src-tauri/src/tiles/serve.rs:162-167` (breaker check precedes cache read) + `src-tauri/src/tiles/commands.rs:184-187` (`status_core`) + `src-tauri/src/tiles/mod.rs:81` (`LanCached` never produced)
**Severity:** significant
**Evidence:**
Two coordinated gaps defeat the entire "offline tiles" value proposition:

1. **Serving:** `serve_tile_with` consults the breaker BEFORE the cache. When the
   source has tripped (`Degraded`), `serve.rs:163-165` returns
   `ServeError::SourceDegraded` (→ HTTP 503) *without ever calling*
   `fetch_tile_single_flight`, whose step 1 is the cache-first lookup
   (`fetch.rs:461-465`). So once a source goes offline and the breaker trips
   (K=3 consecutive host failures), **previously-cached tiles stop being served**
   even though they are sitting on disk and require no network. The webview gets
   503 → falls back to the bundled raster for tiles it has a perfectly good cached
   copy of.

2. **Status:** `status_core` (`commands.rs:184-187`) maps `BreakerHealth::Degraded`
   → `StatusKind::Unreachable`. There is no code path anywhere in the backend that
   produces `StatusKind::LanCached` — `grep -rn LanCached src-tauri/src` returns
   only the enum definition (`mod.rs:81`). The `cached_at` field is likewise never
   populated (`grep cached_at` shows only `None`/field-decl). So "configured but
   offline, serving from cache" — the headline offline-EmComm scenario — is
   unreachable as a status.

3. **Frontend:** `useTileSource` treats only `lan-live`/`lan-cached`/`partial` as
   tile-backed (`useTileSource.ts:27-31`). With the source reporting `Unreachable`,
   the hook returns `null`, `BaseMap` drops `TileLayerBridge`, and the zoom cap
   collapses to `RASTER_MAX_ZOOM=3` (`BaseMap.tsx:161-163`). The operator who
   cached their area while the shack server was up loses all of it the moment the
   server goes down.

**Impact:** The feature's core offline promise is non-functional. A field operator
who pre-caches at home and then goes off-grid sees the map degrade to the 4-tile
bundled raster, not their cached high-zoom tiles. This is arguably the single most
important user story for "LAN offline tiles" and it is broken end-to-end.

**Fix approach:** (a) In `serve_tile_with`, attempt the cache read *before* the
breaker short-circuit (or have the degraded branch fall through to a cache-only
fetch): a degraded source should still serve `cache::get` hits with zero network.
(b) Teach `status_core` to report `LanCached` when the breaker is `Degraded` AND
the active source's namespace has cached entries (the cache meta already tracks
this), reserving `Unreachable` for "degraded AND no cache". (c) `useTileSource`
already accepts `lan-cached` as tile-backed, so the frontend needs no change once
(a)+(b) land. This is the highest-value fix after B1 — it is *why* the feature
exists.

---

### B3 — `min_zoom` is collected, persisted, and validated but never applied to the layer

**Location:** `src/map/TileLayerBridge.tsx:50-60` (no `minZoom` passed) + `src/settings/MapTileSourceSettings.tsx:248-255` (collected) + `src-tauri/src/tiles/mod.rs:37` (stored)
**Severity:** minor
**Evidence:** `TileSource.minZoom` is collected in Settings, persisted in config,
and crosses the wire (`tileSource.ts:45`), but `TileLayerBridge` only ever sets
`maxNativeZoom` (`TileLayerBridge.tsx:51,57`). No `minZoom`/`minNativeZoom` is
threaded to the Leaflet `TileLayer`. A source that only has tiles at z≥8 (common
for a regional `.mbtiles`) will have Leaflet request `0/0/0`…`7/x/y` from it,
every one of which 404s. Per the breaker classification those 404s are
`Outcome::Coverage` (not host failures) so they don't trip the breaker — but they
do set the `partial_coverage` flag (`mod.rs:188`), so a source that is actually
fully healthy at its real zoom range gets reported as `StatusKind::Partial`
whenever the operator is zoomed out below `min_zoom`.
**Impact:** Confusing "partial" status and wasted 404 round-trips for any source
with a non-zero `min_zoom`. Not a blocker for a z0-capable raster server, but the
field is a no-op promise to the operator.
**Fix approach:** Pass `minNativeZoom={source.minZoom}` to the `TileLayer` so
Leaflet upscales the source's lowest native tiles instead of requesting
nonexistent low-zoom tiles. Optionally suppress the `partial` flag for 404s below
`min_zoom`.

---

### B4 — `0/0/0` reachability probe is a *cache-polluting* mutation inside a "dry-run" test path

**Location:** `src-tauri/src/tiles/commands.rs:91` (`validate` calls `fetch_tile_single_flight`) ⇄ `src-tauri/src/tiles/fetch.rs:485-489` (leader writes cache)
**Severity:** significant
**Evidence:** `validate` (shared by both `configure_tile_source` AND the dry-run
`test_tile_source`, `commands.rs:73-107`) runs the reachability probe through
`fetch_tile_single_flight`. That function's leader path performs `cache::put` on a
successful fetch (`fetch.rs:487-489`). So `test_tile_source` — documented as
"probe, no persist" (`commands.rs:216-217`, `MapTileSourceSettings.tsx:12`) —
**writes the probed `0/0/0` tile into the on-disk cache** for the candidate
source's namespace. The module header for `validate` even claims it returns the
status "WITHOUT mutating any state" (`commands.rs:66-67`), which is false: the
filesystem cache is mutated.
**Impact:** A "Test source" click against a source the operator decides NOT to use
leaves a cached tile (and a `meta.json` namespace dir) on disk for a never-bound
source. More subtly, it interacts with the false-positive in the docstring: a
reviewer trusting "no mutation" could build on that guarantee. Low data-risk (a
single small tile) but it violates the stated dry-run contract and is exactly the
class of "test ≠ pure" surprise the explicit-referents memory warns about.
**Fix approach:** Give `validate` a probe-only fetch that bypasses the cache `put`
(e.g. call `fetch_tile_bytes` directly rather than `fetch_tile_single_flight`, or
thread a `cache: bool` flag). The reachability probe does not need single-flight
de-dup or caching — it is one tile, once.

---

### B5 — `RecenterOnOperator` forces zoom 3 on every operator-grid change, overriding the raised LAN-tile cap and re-clamping the view

**Location:** `src/catalog/StationFinderMap.tsx:73,87-92,146`
**Severity:** significant
**Evidence:** `OPERATOR_ZOOM = 3` is hard-coded (`StationFinderMap.tsx:73`) with
the comment "clamped by BaseMap's raster-native maxZoom" — written when the cap was
always 3. `RecenterOnOperator` calls `map.setView([lat,lon], zoom=3)` whenever
`lat`/`lon`/`zoom`/`map` change (`:89-92`). With a LAN tile source bound, the
operator can now zoom to z16, but any later change to the operator grid (or a
re-fire of the effect) snaps the view back to z3 — the bundled-raster zoom — even
though the whole point of binding the source was to zoom past 3. Worse for the
first-load ordering: `useTileSource` resolves asynchronously. If the operator grid
is already present at mount, `RecenterOnOperator` sets z3; when the tile source
later arrives and `ApplyMaxZoom` raises the cap to 16, the view stays at z3 (the
cap rose but nothing re-zooms), so the operator must manually zoom in to see any
benefit — and they may reasonably conclude "tiles aren't working" because at z3
the bundled raster and the LAN tiles are visually indistinguishable.
**Impact:** Even with B1/B2 fixed, a freshly-bound source presents at z3 (looks
identical to bundled) and any grid edit re-clamps to z3. This is plausibly a
contributor to the "feature appears to do nothing" reports across the prior
remediation attempts — the cap is raised but the view is pinned low.
**Fix approach:** Derive the recenter zoom from the tile-backed state (e.g. zoom to
a mid-range like `min(source.maxZoom, 12)` when tile-backed, else 3), and/or only
auto-recenter on the *initial* operator-location resolution, not on every dep
change, so a later grid edit doesn't yank an operator who has manually zoomed in.

---

### B6 — Debug overlay marked "REMOVE before merge" is live in the shipping component

**Location:** `src/catalog/StationFinderMap.tsx:95-128` (`MapDebugOverlay`) + `:133-141,145`
**Severity:** significant (ship-blocker, not a logic bug)
**Evidence:** `MapDebugOverlay` is a green-on-black monospace HUD that renders
`tileSource`, `computedMax`, and live `map.getMaxZoom()/getZoom()` over the map. It
is explicitly annotated `// TEMP DIAGNOSTIC (bd tuxlink-k61j) … REMOVE before
merge.` (`:95-98`) and is unconditionally mounted at `:145` plus a duplicated
`computedMax` computation at `:133-141`. There is no `import.meta.env.DEV` /
feature-flag guard.
**Impact:** If this branch merges as-is, the production Find-a-Station map ships a
debug overlay covering the top-left corner of the map. Given the prompt notes this
feature "has failed end-to-end across ~5 remediation attempts," this is exactly the
kind of left-behind diagnostic that must be caught at the gate.
**Fix approach:** Remove `MapDebugOverlay`, the `tsKind`/`computedMax` locals, and
the mount before merge. (The `computedMax` duplication also re-implements
`BaseMap`'s cap logic — a second source of truth that will drift.)

---

### B7 — Single-flight cleanup races: a host-failure result is cached as a *negative* by being absent, and the cleanup-remove window lets a second caller re-fetch a known-failing tile

**Location:** `src-tauri/src/tiles/fetch.rs:483-505`
**Severity:** minor
**Evidence:** The leader removes its `FLIGHTS` entry *inside* the shared future,
after the fetch+cache step (`fetch.rs:494-497`). For a FAILED fetch nothing is
cached (correct — cache-only-good), and the entry is removed. The docstring claims
"a NEW caller after this point re-fetches (correct: the result is now cached, so it
short-circuits at step 1 anyway)" (`:491-493`) — but that reasoning only holds for
*successful* fetches. For a failing source, there is no cache entry, so every new
caller after the flight clears re-launches a fresh upstream fetch. The breaker is
the actual backstop here (it trips after K failures and `serve_tile` short-circuits
upstream of the flight), so this is not unbounded — but the docstring's stated
invariant is wrong for the failure path, and during the breaker's `Live` window
(failures 1..K-1) concurrent panning across a dead tile does re-fetch.
**Impact:** Minor wasted fetches for a failing source before the breaker trips;
mainly a correctness-of-reasoning defect in the comment that could mislead a future
maintainer into removing the breaker thinking single-flight covers it.
**Fix approach:** No code change strictly required (breaker bounds it). Correct the
docstring to state the bound comes from the breaker, not from caching, for the
failure path.

---

## Design Concerns

### D1 — Breaker state is per-`TileGatekeeper`-instance but coordinate validation uses `source.max_zoom`; a stale boot-seeded source can silently cap zoom

`lib.rs:328-330` boot-seeds the gatekeeper from `config.map_tile_source`. `serve.rs:160`
validates each coordinate against `source.max_zoom`. If the operator lowers `max_zoom`
in a new config but a different worktree build / stale process holds the old source,
the served cap and the validated cap diverge. Not a bug in this code, but the
per-instance source + the documented worktree-port-collision footgun
(`project_worktree_dev_port_collision` memory) mean "the build I'm testing isn't the
one serving" is a live risk for this feature specifically — worth a provenance probe
in any on-device validation.

### D2 — `validate`'s probe coordinate is `0/0/0`, but a source with `min_zoom > 0` legitimately 404s it

`commands.rs:87-99` probes `0/0/0`. A regional source (`min_zoom: 8`) returns 404
for `0/0/0`; `validate` maps that 404 → `LanLive` (`commands.rs:99`), which is the
*right* call for activation — but it means the probe never actually confirms the
source serves a real image at any zoom it covers. Combined with B3, a source whose
only real tiles are at z≥8 validates as `LanLive` purely on a 404, then renders
nothing until the operator zooms in past `min_zoom`. The probe should arguably
target a coordinate at `min_zoom` (e.g. `(min_zoom, 0, 0)`) so a "validated"
source has actually served one real tile.

### D3 — `incompatible`/`bundled` copy reachable from `bindMessage` but no bind ever yields them

`MapTileSourceSettings.tsx:60-69`: `bindMessage` falls through to `statusMessage`
for non-tile-backed kinds, but `configure_core` only ever returns
`LanLive`/`Unreachable`/`Incompatible` (never `Bundled`/`LanCached`/`Partial` from
a *configure* call). The `partial` case in `bindMessage` (`:64`) can't occur from a
fresh configure (the probe is a single tile; `partial` requires a recorded coverage
404 from the *serving* path). Dead branches that imply states the configure path
can't produce — low risk, but they obscure which statuses are actually reachable
where, and that ambiguity is part of why the feature is hard to reason about.

### D4 — No end-to-end test crosses the `tile://` URL boundary

The most damaging bug (B1) and the offline regression (B2) both live in the *seam*
between layers — Leaflet's `tms` flip vs the backend flip; the breaker-vs-cache
ordering. Every existing test exercises one side in isolation (`build_tile_url`
with a hand-built coord; `serve_tile_with` with an injected fetch closure that
never touches the real cache+breaker+coord-flip chain together). The contract that
keeps breaking is the *composed* one. A single integration test that feeds a real
`{z}/{x}/{y}` *string* (as the webview would, post-Leaflet-flip) through
`serve_tile` against a mock upstream, for both Xyz and Tms, would have caught B1
immediately.

---

## Contract-Violation Pass Summary (the highest-value pass per the brief)

Wire-shape audit, Rust serde ⇄ TS:

- `TileSource` `rename_all=camelCase` (`mod.rs:31`) ⇄ TS `interface TileSource`
  (`tileSource.ts:42-50`): **MATCH** (`url, scheme, minZoom, maxZoom,
  cacheBudgetMb, attribution, label`). `MapTileSourceSettings.buildSource`
  (`:143-153`) emits exactly these keys. ✓
- `TileScheme` PascalCase variants `"Xyz"`/`"Tms"` (`mod.rs:48-54`, no `rename_all`)
  ⇄ TS `type TileScheme = 'Xyz' | 'Tms'` (`tileSource.ts:22`): **MATCH**. ✓
- `StatusKind` `rename_all=kebab-case` (`mod.rs:74`) ⇄ TS union (`tileSource.ts:33-39`):
  **MATCH** for all six (`bundled, lan-live, lan-cached, partial, unreachable,
  incompatible`). ✓ — but `lan-cached` is **never produced** (see B2).
- `TileSourceStatus` `rename_all=camelCase` (`mod.rs:58`) ⇄ TS (`tileSource.ts:53-58`):
  **MATCH** (`kind, zoom, label, cachedAt`). ✓ — `cachedAt` never populated (B2).
- `ConfigViewDto.map_tile_source` (snake_case key, `ui_commands.rs:3201`) ⇄
  `useTileSource`/`MapTileSourceSettings` reads `config.map_tile_source` with inner
  camelCase `TileSource` (`useTileSource.ts:54`, `MapTileSourceSettings.tsx:122`):
  **MATCH** — this is the ORIGINAL primary bug (DTO dropped the field), now FIXED.
  The fix is correct: snake_case outer key, camelCase inner struct. ✓
- `tile://localhost/{z}/{x}/{y}` template (`TileLayerBridge.tsx:41`, no `.png`) ⇄
  `serve.rs::parse_zxy` (tolerates `{y}` or `{y}.png`, `:85-89`) ⇄ `build_tile_url`
  (template vs base-dir, `fetch.rs:257-322`): path parsing is consistent — BUT the
  Y *value* in that path is wrong for TMS (B1). The string *shape* matches; the
  *semantics* do not.

Net: the explicit field-name/casing contracts are now clean (the DTO fix held). The
surviving contract violation is **semantic, not nominal** — the TMS Y-coordinate
crosses the `tile://` boundary already-flipped and the backend flips it again (B1).
