# Bug Hunt Report — LAN offline-tiles + Find-a-Station zoom (bd tuxlink-k61j)

Agent: dahlia-spruce-osprey
Date: 2026-06-12
Worktree: `worktrees/bd-tuxlink-k61j-tile-maxzoom-reactive/`

## Scope

Read the full bind→persist→boot-seed→serve→render→zoom data path:

- Rust: `tiles/{mod,commands,fetch,serve,cache,host,coord,breaker}.rs`,
  `config.rs` (`map_tile_source`), `ui_commands.rs` (`ConfigViewDto`),
  `lib.rs` (`tile` scheme registration + boot-seed), `tauri.conf.json` (CSP).
- FE: `map/{BaseMap,TileLayerBridge,useTileSource,tileSource,tileSourceEvent,projection}`,
  `catalog/StationFinderMap.tsx`, `settings/MapTileSourceSettings.tsx`.

Approach: traced one tile end-to-end for each status, then re-traced the
freshly-bound case through the breaker/status state machine and the
mount-time-vs-event re-read ordering. The three fixes already in the worktree
(ConfigViewDto field, `ApplyMaxZoom`, `{z]` brace guard, bindMessage copy) are
correct as far as they go. The findings below are what those fixes do NOT
cover and are the live reasons a valid raster XYZ server can still fail to
render end-to-end.

---

## Bugs

### 1. `TileLayer` has no `maxZoom`; above `maxNativeZoom` the layer stops drawing tiles — the operator zooms past z-native into blank/raster, defeating the whole feature

**Location:** `src/map/TileLayerBridge.tsx:50-61` (no `maxZoom` prop) interacting with `src/map/BaseMap.tsx:161-163,195` (map `maxZoom` raised to `source.maxZoom`).

**Severity:** significant

**Evidence:** The map's max zoom is raised to the SOURCE's `maxZoom` (e.g. 16),
and `TileLayerBridge` sets `maxNativeZoom = min(source.maxZoom, appMaxZoom)` —
i.e. also 16. A Leaflet `GridLayer` only requests/upscales tiles up to its own
`maxZoom`; the react-leaflet `TileLayer` here sets `maxNativeZoom` but never
`maxZoom`. When the map and the source share the same ceiling (16 == 16) this
is benign. But it is fragile and the comment block (lines 10-12) claims Leaflet
"up-scales its OWN native tiles … above the source's native resolution" — which
only happens *between* `maxNativeZoom` and the layer's `maxZoom`. With
`maxNativeZoom === map maxZoom` there is no upscale band, and the layer's
default `maxZoom` (18, > the map's 16) means the band that matters
(native→displayed) is exactly the levels the map can reach. So today the tiles
DO request at every reachable level — but the documented "upscale above native"
behavior is non-functional because the two ceilings are pinned together. The
real defect surfaces the moment a source advertises `maxZoom > 16`
(`TILE_MAX_ZOOM_CAP`): the map caps at 16, `maxNativeZoom` caps at 16, fine —
but if a future change raises the map cap above `maxNativeZoom`, no `maxZoom` on
the layer means the layer silently inherits 18 and there is an undocumented
upscale band that was never validated on WebKitGTK.

**Impact:** Not the current empty-map cause, but a latent correctness gap: the
layer's zoom behavior depends on an implicit equality (`appMaxZoom` ===
map-cap) that nothing enforces. Recommend setting `maxZoom={appMaxZoom}`
explicitly on the `TileLayer` so the layer's displayed-zoom ceiling matches the
map's, making the native/upscale bands explicit rather than relying on Leaflet
defaults. Fix approach: add `maxZoom={appMaxZoom}` to `<TileLayer>`.

---

### 2. `ApplyMaxZoom` raises the *cap* but never re-zooms the view, and `RecenterOnOperator` pins the view at z3 — after a bind the operator still sees a z3 view and must MANUALLY zoom in to see any LAN tile

**Location:** `src/map/BaseMap.tsx:136-142` (`ApplyMaxZoom` only calls `setMaxZoom`), `src/catalog/StationFinderMap.tsx:87-93,146` (`RecenterOnOperator` forces `zoom = OPERATOR_ZOOM = 3`).

**Severity:** significant (this is very likely the "appeared to do nothing" symptom the operator reported, only partially addressed by the committed fix)

**Evidence:** The committed fix makes the cap reactive: `map.setMaxZoom(16)`
now fires when the source binds. But `setMaxZoom` does NOT change the current
zoom — it only lifts the ceiling. The map remains at whatever zoom it was
(`OPERATOR_ZOOM = 3`, or `initialZoom`). At z3 the LAN tile layer requests
tiles `3/x/y` — which on a typical city/region LAN tile server (e.g. a single
metro extract) are almost all **404** (the server has no world-coverage z3
tiles). Those 404s fall back to the bundled raster (correct per §8.5), so the
map at z3 looks IDENTICAL to the bundled-only map. The operator sees the same
coarse world raster and concludes "the source did nothing," because nothing
visibly changes until they manually zoom to the levels the LAN server actually
covers. There is no auto-zoom-to-source-min on bind, and `RecenterOnOperator`
actively re-asserts z3 on every operator-grid change.

Worse: a LAN metro tile server commonly has `minZoom` ~10+. At z3 EVERY tile is
404 → the breaker records `Outcome::Coverage` (correct, no trip) and the status
goes `partial`. The map stays raster-backed. The feature only becomes visible if
the operator already knows to zoom to ~z10+ over the covered region.

**Impact:** End-to-end, a correctly-bound, reachable source produces a map that
is visually indistinguishable from no-source until the operator manually zooms
in over the exact geographic area the server covers. Across 5 remediation
attempts this is the kind of "it's bound but does nothing" symptom that looks
like a binding failure but is actually a view-positioning failure. Fix
approach: on a successful tile-backed bind, set the view to the source's usable
zoom (e.g. `map.setView(center, Math.min(source.maxZoom, appMaxZoom))` or at
least to the source `minZoom`) so the operator immediately lands on a level the
server serves; or surface guidance ("zoom in to see LAN tiles") in the bind
message. At minimum, `RecenterOnOperator`'s hard `zoom=3` should not override a
freshly-raised cap.

---

### 3. Reachability probe fetches `0/0/0` at the source's `max_zoom`, but a metro LAN server with no world-coverage z0 tile returns 404 → validates as `LanLive` → persists a source whose ONLY reachable zooms are unknown and possibly never produce a visible tile at the default view

**Location:** `src/tiles/commands.rs:83-99` (`validate`: probes `TileCoord::new(0,0,0, max_zoom)`; `NotFound → LanLive`).

**Severity:** significant (design-level; this is the "404→LanLive validation philosophy" the prompt flags)

**Evidence:** The probe is `0/0/0` (the whole-world tile). Comment at
commands.rs:96-98 deliberately treats a 404 here as `LanLive` ("a missing
world tile is a coverage gap"). But for the COMMON LAN case — a regional/metro
tile server with `minZoom` well above 0 — `0/0/0` is ALWAYS 404. So the probe
validates `LanLive` purely on "the host answered with a 404," learning nothing
about whether ANY tile the operator will actually view exists. Combined with
Bug #2 (view stays at z3), the operator binds, sees `source active — map zoom
reaches level 16`, and the map shows nothing different. The probe gives a false
positive of usefulness.

A more honest probe would fetch a tile at the source's `min_zoom` (or a
mid-range zoom), or fetch several candidate tiles, to confirm the server
actually serves image bytes somewhere — not merely that it 404s politely. The
current probe cannot distinguish "reachable real tile server" from "any HTTP
server that 404s `/0/0/0.png`" (e.g. a misconfigured nginx, a wrong path
prefix, a server that needs `{z}/{x}/{y}` but got base-dir form). A plain
nginx with no tiles returns 404 → `LanLive` → bind succeeds → empty map.

**Impact:** The validation gate that is supposed to prevent "bound but empty"
is exactly the gate that lets it through. This is plausibly why multiple
remediation attempts "fixed" binding yet the map stayed empty: binding was
never the failure; the probe was rubber-stamping unusable sources. Fix
approach: probe at `min_zoom` (or both `min_zoom` and `max_zoom`), and treat a
404 at `min_zoom` as a stronger signal (Incompatible/Unreachable) than a 404 at
`max_zoom`. At minimum probe a zoom the source claims to cover, not the world
tile it almost certainly lacks.

---

### 4. The malformed-placeholder guard (`{z]`) rejects a typo, but the base-directory fallback silently mis-builds a templated-intent URL when the operator's template uses a NON-standard token

**Location:** `src/tiles/fetch.rs:265-322` (`build_tile_url`: `is_template` is true only if the raw URL contains the literal substrings `{z}`/`{x}`/`{y}`).

**Severity:** minor-to-significant (depends on operator input)

**Evidence:** `is_template = raw.contains("{z}") || contains("{x}") ||
contains("{y}")`. The new brace guard (line 282) only fires AFTER substitution,
i.e. only when at least one of the three exact tokens was present. But a URL
like `http://host/{Z}/{X}/{Y}.png` (uppercase) contains NONE of the
lowercase tokens, so `is_template` is FALSE → it takes the base-directory
branch → appends `/3/5/2.png` onto a path that already ends in
`{Z}/{X}/{Y}.png`, producing
`http://host/%7BZ%7D/%7BX%7D/%7BY%7D.png/3/5/2.png` (url-encoded braces +
appended triple). That 404s every tile → `partial`/empty map, with NO
BadUrl signal — the operator gets `source active` (probe `0/0/0` also 404s →
LanLive). The guard only catches the case where a valid token is ALSO present
(`{z]/{x}/{y}` has a real `{x}`/`{y}`). A wholly-wrong placeholder style slips
past as a "base directory."

**Impact:** A second class of typo (wrong-case or wrong-style placeholders)
still produces the silent empty-map the guard was meant to eliminate. Less
likely than `{z]` but the placeholder field has no validation feedback. Fix
approach: in `build_tile_url`, after taking the base-directory branch, reject
(BadUrl) if the resulting URL still contains `%7B`/`%7D` (encoded braces) or
detect any `{`/`}` in the raw URL that did not match the standard tokens and
surface Incompatible. Alternatively validate the template shape in the Settings
form before bind.

---

### 5. `looksPublic` is the ONLY hostname-shape feedback, but a bare single-label hostname (the operator's actual LAN server name) is treated as LAN with no DNS check — and the backend's named-host resolve path can still HostDenied it at fetch time → the FE shows `source active` but every tile 502s

**Location:** `src/settings/MapTileSourceSettings.tsx:96-98` (bare hostname → not public → no warning) vs `src/tiles/fetch.rs:203-234` (named host resolved + every addr must pass `ip_is_permitted`) and `commands.rs:93` (HostDenied → Unreachable at probe).

**Severity:** minor (the probe DOES catch it; this is a consistency note)

**Evidence:** A bare hostname like `tileserver` resolves via the system
resolver at probe time. If it resolves to a public IP, or to nothing, the probe
returns `Unreachable` — correct. But if it resolves to a LAN IP at probe time
and later (e.g. VPN/DNS change) to something denied, serving 502s while the
persisted status the FE last saw was `lan-live`. The FE `useTileSource` re-reads
status via `tile_source_status`, which is breaker-driven and will eventually
show `unreachable` after K host failures — so this self-heals. Noting it as a
consistency seam, not a hard bug. No fix required beyond awareness.

---

## Design Concerns

### A. The whole feature's "did it work" signal is decoupled from "will the operator see tiles"

Bugs #2 and #3 compound: `bindMessage` says `source active — map zoom reaches
level 16`, the status says `lan-live`, yet the view sits at z3 over a server
that only has tiles at z10+. Three independent "success" signals
(`LanLive` status, raised cap, distinct bind copy) ALL fire while the operator
sees nothing change. The feature needs ONE signal tied to actual rendered tiles
— e.g. land the view on a covered zoom on bind, or report the source's covered
zoom range so the operator knows where to look. This decoupling is the most
likely reason 5 attempts each "fixed" a real defect yet the operator still saw
an empty map: each fix addressed a true-but-insufficient link in the chain.

### B. `0/0/0` probe is structurally weak for the dominant deployment (regional LAN servers)

§8.3's probe choice optimizes for "don't reject a working server because its
world tile is absent," but the dominant real source (a metro/region extract)
has NO world tile, so the probe degenerates to "host answered 404" =
`LanLive`. The probe should exercise a zoom the source claims (`min_zoom`), so
"validated" means "served at least one real image tile," not "404'd politely."

### C. Mount-time `useTileSource` load races a not-yet-managed gatekeeper only on the failure path

`lib.rs:306-332` seeds the gatekeeper inside the `app_data_dir()` match arm; if
`app_data_dir()` errors, the gatekeeper is NEVER `.manage()`d and the `tile`
scheme handler responds 503 for every tile (lib.rs:131-138). `tile_source_status`
would also fail to resolve `State<Arc<TileGatekeeper>>` and the command errors →
`useTileSource` catches → null → bundled. Self-consistent (degrades to bundled),
but the boot-seed + serving + status commands all silently vanish together on a
single `app_data_dir()` failure with no operator-visible cause. Low likelihood,
worth a log line.

### D. Temp diagnostic + uncommitted state must not ship

`StationFinderMap.tsx:95-128,133-145` is the `MapDebugOverlay` (marked "REMOVE
before merge"). `fetch.rs` and `ui_commands.rs` carry uncommitted (correct)
changes. The committed HEAD has the BaseMap/testMapMock/bindMessage fix but the
ConfigViewDto field, the `{z]` guard, and the debug overlay are all
working-tree-only — the feature is NOT fully committed. If only HEAD were built,
`config_read` would still drop `map_tile_source` and the FE would still see
null. Verify the build under test includes the working tree, not just HEAD.

---

## Note for testing-pitfalls

The recurring miss across these bugs is that the react-leaflet test mock mirrors
props as data-attributes and cannot model: (a) Leaflet's mount-once `maxZoom`
(caught late, fixed via `setMaxZoom` spy), (b) the difference between raising a
zoom CAP and changing the zoom VIEW (Bug #2 — no test can catch "operator still
parked at z3 over an uncovered area"), and (c) actual 404-vs-image tile
behavior at a given zoom (Bug #3 — the probe's `0/0/0` weakness). These are all
grim/WebKitGTK-or-real-server concerns. A useful testing-pitfalls entry: "tile
zoom features need a real tile server at a known coverage zoom range; a
prop-shape unit test proves wiring but cannot prove the operator sees a tile —
validate by binding a metro server and confirming a tile renders at its
min_zoom, not just that status === lan-live."
