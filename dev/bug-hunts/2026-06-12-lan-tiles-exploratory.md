# Bug Hunt Report — LAN offline tiles + Find-a-Station

Agent: dahlia-spruce-osprey
Date: 2026-06-12
Worktree: `worktrees/bd-tuxlink-k61j-tile-maxzoom-reactive/`
Method: exploratory (code-bug-hunter-exploratory), depth-first along the
configure→bind→persist→serve→render data path.

## Scope

Read in full and traced:

- Rust: `tiles/mod.rs` (TileGatekeeper + serde wire types), `tiles/commands.rs`
  (validate / configure_core / status_core), `tiles/fetch.rs` (build_tile_url +
  SSRF + single-flight), `tiles/serve.rs` (parse_zxy + serve_tile + breaker
  feed), `tiles/coord.rs`, `tiles/breaker.rs`, `tiles/host.rs`, `tiles/cache.rs`
  (scanned), `lib.rs` (tile:// scheme registration + boot-seed), `config.rs` +
  `ui_commands.rs` (ConfigViewDto projection), `tauri.conf.json` (CSP).
- Frontend: `map/useTileSource.ts`, `map/tileSource.ts`, `map/BaseMap.tsx`,
  `map/TileLayerBridge.tsx`, `map/tileSourceEvent.ts`, `map/projection.ts`,
  `map/testMapMock.ts`, `catalog/StationFinderMap.tsx`,
  `settings/MapTileSourceSettings.tsx`, plus the BaseMap / StationFinder tests.

Ran: `vitest run src/map src/settings src/catalog` (10 failures, see Bug 1).
Cross-boundary serde casing, the tile:// template ↔ parse_zxy ↔ build_tile_url
contract, and the breaker/status_core interactions were all checked.

The four already-found-and-fixed items (DTO field, ApplyMaxZoom imperative cap,
brace guard, 404→LanLive probe) were verified rather than re-reported; findings
below are STILL-broken or adjacent.

---

## Bugs

### 1. `MapDebugOverlay` calls `map.getMaxZoom()` — crashes 10 tests; ships a "REMOVE before merge" diagnostic on the production render path
**Location:** `src/catalog/StationFinderMap.tsx:99-128` (specifically `:102` and `:106`)
**Severity:** significant (blocks the CI `verify` gate; also a UX defect if it ever shipped)
**Evidence:** `MapDebugOverlay` is rendered unconditionally inside `<BaseMap>` at
`StationFinderMap.tsx:145`. It calls `map.getMaxZoom()` at lines 102 and 106. The
canonical react-leaflet test double (`map/testMapMock.ts`) implements `getZoom`,
`setView`, `setMaxZoom`, `getBounds` — but NOT `getMaxZoom`. Result of
`vitest run src/catalog`:

```
TypeError: map.getMaxZoom is not a function
 ❯ src/catalog/StationFinderMap.tsx:102:52
 Test Files  2 failed | 31 passed
      Tests  10 failed | 219 passed
```

All 10 failures (5 in `StationFinderMap.test.tsx`, 5 in `StationFinderPanel.test.tsx`)
trace to this one call. The full `pnpm vitest run` that CI's `verify` gate runs
will be RED on this branch.

**Impact:** (a) CI fails — this is a hard merge blocker, and likely a contributor
to "failed end-to-end across ~5 attempts" if those attempts were judged by a
green local-scoped vitest that didn't include `src/catalog`. (b) The block is a
self-described temporary diagnostic ("TEMP DIAGNOSTIC … REMOVE before merge") that
is wired into the real Find-a-Station render — it paints a green-on-black debug
HUD over the live map. If merged, the operator sees the debug overlay.
**Fix approach:** Delete `MapDebugOverlay` and its mount at `:145` (it has served
its diagnostic purpose). If retained temporarily, add `getMaxZoom(){ return 18; }`
to `testMapMock.ts`'s `fakeMap` so the suite is at least green — but removal is
correct: a diagnostic overlay must not be on the production render path.

---

### 2. A wrong-but-reachable server that 404s EVERY tile validates as `lan-live` and binds — operator gets a raised zoom cap over a permanently-empty map
**Location:** `src/tiles/commands.rs:96-99` (`validate`), and the design choice it implements
**Severity:** significant
**Evidence:** The reachability probe fetches exactly one tile, `0/0/0` at
`source.max_zoom`, and maps `FetchError::NotFound` (HTTP 404) → `StatusKind::LanLive`:

```rust
Err(FetchError::NotFound) => status(StatusKind::LanLive, source, source.max_zoom),
```

The rationale (a sparse regional tileset legitimately lacks the world tile
`0/0/0`) is sound for that case. But it cannot distinguish "valid server, world
tile absent" from "operator pointed at the wrong path / wrong port / a server that
404s everything." Both 404 on `0/0/0`; both bind as `lan-live` + persist. Downstream:
`status_core` reports `lan-live` (the probe never touches the breaker, so
`is_partial_coverage()` is false), `useTileSource` returns a descriptor,
`BaseMap` raises `maxZoom` to 16 (`BaseMap.tsx:161-163`), and every real tile the
map then requests also 404s → `Outcome::Coverage` → `partial` (still tile-backed,
cap stays raised, `BaseMap.tsx:96-102`). Net: the operator sees "source active —
map zoom reaches level 16" (`MapTileSourceSettings.tsx:65`), can zoom to 16, and
sees nothing but the coarse bundled raster the whole way up.

**Impact:** This is precisely the "binds successfully but the map is empty"
end-to-end failure. The bind UX actively misleads ("source active"). For the most
common misconfiguration (wrong base path / port typo) there is NO error surfaced.
**Fix approach:** Make the probe positively confirm the server serves at least one
real image tile before reporting `lan-live`. Options, in preference order:
(a) Probe a tile the operator's `min_zoom`/`max_zoom` range guarantees exists for
a world-covering set — but a regional set has no such universal coord, so this is
imperfect. (b) Treat a `0/0/0` 404 as `Incompatible` (or a new "reachable but no
sample tile" status) UNLESS a small probe sweep (e.g. `0/0/0`, then
`min_zoom/0/0`) yields at least one `Ok`. (c) At minimum, distinguish the
all-404 case at serve time: if the FIRST N live tile requests after a bind all
404, downgrade the surfaced status from `partial` to `incompatible`/`unreachable`
so the pill stops claiming an active source. The current single-coord,
404-is-success probe is the root design weakness behind the empty-map bind.

---

### 3. Malformed-template (`BadUrl`) bind reports `Incompatible` with copy that claims "the server responded" — but the server was never contacted
**Location:** `src/tiles/commands.rs:101-105` (BadUrl → Incompatible) + `src/settings/MapTileSourceSettings.tsx:40-41` (copy)
**Severity:** minor (misleading diagnostics; slows operator self-correction)
**Evidence:** `build_tile_url` (`fetch.rs:282-286`) returns `FetchError::BadUrl`
for a malformed placeholder (`{z]`, leftover brace) BEFORE any network I/O.
`validate` maps `BadUrl` → `Incompatible` (`commands.rs:101-105`). The
`Incompatible` copy is:

```
'incompatible tile source — the server responded but did not return standard image tiles'
```

For the `BadUrl` path the server was never contacted, so "the server responded"
is false. The operator's actual error is a typo in the URL template, which this
copy does not point at.
**Impact:** An operator who typo's the template (the exact shape bd tuxlink-k61j
says shipped in a real config) is told the server returned bad tiles, sending them
to debug the server instead of the URL string. This is the FE half of the
already-fixed backend brace guard — the guard now rejects correctly, but the
surfaced message misdiagnoses.
**Fix approach:** Give `BadUrl` its own `StatusKind` (e.g. `bad-url`) or carry a
distinct reason string so the FE can show "the tile URL template is malformed —
expected `{z}/{x}/{y}` placeholders." Today `Incompatible` conflates "server
returned non-image" with "we couldn't even build a URL," and the copy assumes the
former.

---

### 4. `buildSource()` coerces a blank/zero Maximum-zoom field to `maxZoom: 0`, binding a source that pins the map at zoom 0 (un-zoomable)
**Location:** `src/settings/MapTileSourceSettings.tsx:147-149`
**Severity:** minor
**Evidence:** `maxZoom: parseInt(maxZoom, 10) || 0`. If the operator clears the
field (or types `0`/non-numeric), `maxZoom` becomes 0. The backend probe
`TileCoord::new(0,0,0,0)` is valid (0 ≤ 0), so the source can bind `lan-live`.
`BaseMap` then computes `maxZoom = Math.min(0, 16) = 0` (`BaseMap.tsx:161-163`)
and `ApplyMaxZoom` calls `setMaxZoom(0)` with `minZoom={0}` — the operator cannot
zoom at all, and `maxNativeZoom` on the TileLayer is 0 so only the z0 world tile
is ever requested.
**Impact:** A cleared/0 max-zoom field silently produces an un-zoomable bound
source instead of an input-validation error. Low likelihood (operator must clear a
pre-filled `16`), but the failure is silent and looks like "the feature is broken."
**Fix approach:** Reject `maxZoom < minZoom` or `maxZoom < 1` in the form before
calling `configureTileSource`, with a field-level message. Same class of `|| 0`
coercion exists for `minZoom` (harmless) and `cacheBudgetMb` (0 → backend default,
benign per `cache.rs:376-379`).

---

## Design Concerns

- **The single-coordinate reachability probe is the structural weak point**
  (Bug 2). Because `0/0/0`-404 is indistinguishable from a totally-wrong URL, the
  bind step can never be trustworthy as currently shaped. Every "fix" that stays
  inside the 404→LanLive mapping will keep producing the empty-map bind for the
  most common misconfiguration. A positive "we fetched at least one real tile"
  signal (probe sweep, or a serve-time all-404 downgrade) is the durable fix.

- **`status_core` never reflects "bound but serving nothing."** Once a source is
  active and the breaker is `Live`, status is `lan-live`/`partial` regardless of
  whether ANY tile has ever rendered. `partial` is defined as "live source with
  SOME 404s," but the all-404 case is also surfaced as `partial`, so the status
  surface cannot tell the operator their source serves zero tiles. The breaker
  only trips on *host* failures (`Outcome::HostFailure`), and 404s are
  `Outcome::Coverage` (never trip), by design — so a dead-content server stays
  `partial`/`lan-live` forever. Consider a "consecutive coverage-404 since last
  success" counter that, past a threshold, surfaces `incompatible`.

- **`min_zoom` is collected, persisted, and then unused on the render path.**
  `TileLayerBridge` sets only `maxNativeZoom` (`TileLayerBridge.tsx:51`); no
  `minZoom`/`minNativeZoom`. Not a correctness bug for the happy path, but it
  means a regional source's below-range tiles are still requested and 404, adding
  to the coverage-404 noise that Bug 2 / the concern above care about.

- **Diagnostic code on the production render path** (Bug 1). The `MapDebugOverlay`
  pattern — a live overlay reading map internals — is fine as a throwaway, but it
  was committed wired-in with a "REMOVE before merge" comment and broke the suite.
  A guard (e.g. `import.meta.env.DEV`) or, better, deletion is the discipline.

## Note on already-found items (verified, not re-reported)

- DTO fix (`ui_commands.rs:3201` snake_case `map_tile_source`, inner `TileSource`
  camelCase via its own `rename_all`) is consistent with both consumers
  (`useTileSource.ts:54` and `MapTileSourceSettings.tsx:122-131`). Correct.
- `ApplyMaxZoom` (`BaseMap.tsx:136-142`) imperative `setMaxZoom` is correct and
  well-tested (`BaseMap.test.tsx:159-182`).
- Brace guard (`fetch.rs:282-286`) is correct for the leftover-brace typo and the
  authority-injection case; the all-caps `{Z}{X}{Y}`-only template (no lowercase
  placeholder) would bypass `is_template` and fall to base-dir append, but that is
  an exotic input and still ends in a 404 (Bug 2 territory), not a security hole.
- `tile://localhost/{z}/{x}/{y}` ↔ `request.uri().path()` (`/{z}/{x}/{y}`) ↔
  `parse_zxy` (`serve.rs:75-91`, tolerates leading `/` and optional `.png`) ↔
  `TileCoord::from_parts` is a consistent contract. CSP `img-src … tile:`
  (`tauri.conf.json:24`) is present. Boot-seed (`lib.rs:328-330`) sets the source
  from `config.map_tile_source`. These are all correct.
