# Map-Picker v2 — unified picker control language, expand-to-overlay, LAN tiles

> **Status:** Approved (visual brainstorm, operator + agent `shoal-magnolia-fjord`, 2026-06-08).
> **Umbrella issue:** `tuxlink-jx4i`. **Decomposes into:** `tuxlink-dyop` (LAN tiles —
> foundation), `tuxlink-a1cc` (shared navigation controls), `tuxlink-sdbd` (Position
> overlay picker). **Supersedes** the first-pass map UX shipped in #481
> (`tuxlink-z9u4`); does not revert it — the offline `BaseMap` substrate, the
> Maidenhead math, and the bundled world raster all stand.
> **RADIO-1:** not engaged — no RF/transmit path is touched. **CSP:** stays `'self'`;
> the webview never fetches tiles directly.

## 1. Frame

The offline-map foundation (#481) shipped a reachable but first-pass picker on two
surfaces. An operator smoke (2026-06-09) accepted the ship and deferred the UX. The
follow-up walkthrough (2026-06-08) enumerated concrete problems; a visual-companion
brainstorm produced the approved design recorded here. Map-Picker v2 makes both
surfaces usable, gives them one shared control language, and adds the LAN tile source
that the locked 2026-06-07 design always specified but the #481 scope split out.

## 2. Problems enumerated (operator walkthrough, 2026-06-08)

**GRIB region picker** (`Message → GRIB File Request… → Region`):

- The view cannot be repositioned intuitively. Dragging draws the selection box and
  cannot pan, so the map cannot be moved to draw a box elsewhere.
- No physical navigation controls are present. The only way to recompose the view is
  to pinch-zoom out and back in.

**Position report map** (`Message → New Message → GPS Position Report`):

- The map is tiny: it renders inline inside the Compose message flow, so when the
  Compose window is not full-screen the map is unusable, and full-screen is marginal.
- No usable zoom is available.
- No path exists to ingest offline tiles from a LAN source for higher zoom — a
  specific spec request, the actual enabler for the precision the surface needs.

## 3. Grounding (verified against `origin/main`, 2026-06-08)

This section is factually checked against the merged #481 code, not assumed.

- `src/map/BaseMap.tsx` — offline EPSG4326 substrate: a single bundled
  2048×1024 equirectangular world raster as an `<ImageOverlay>`, `maxZoom={2}`
  (raster-native 1:1), `maxBounds` world rectangle, `boxZoom={false}` (native
  Shift-drag box-zoom disabled because it collided with the picker's drag-to-select).
  Default Leaflet zoom control is left at its default (small `+/−`, never customized).
- `src/map/GridMapPicker.tsx` — `'pin' | 'box'` picker. Box mode: `mousedown`
  calls `map.dragging.disable()` and rubber-bands a **new** rectangle from scratch on
  every drag; there is no concept of selecting and adjusting the existing box. Pin
  mode: click drops a marker and reports the **4-char** grid (`.slice(0, 4)`).
- `src/compose/PositionMapWidget.tsx` — a **separate, thinner** widget used only by
  the Position form. Click-to-drop on `BaseMap`, no Maidenhead overlay, no controls
  beyond Leaflet defaults; emits a **6-char** grid on every click.
- `src/compose/PositionFormV2.tsx` — embeds `PositionMapWidget` in a fixed
  `240px` container (`.position-form-v2__map--active`) that collapses to `0` when no
  grid is set. The manual grid `<input>` is the always-available path.
- `src/grib/GribRequestPanel.tsx` — embeds `GridMapPicker` (box mode) in a `260px`
  region map inside the GRIB overlay panel; four signed lat/lon fields stay type-in
  editable.

**Two consequences of the grounding drive this design:**

1. The two surfaces use **different** picker components (`GridMapPicker` vs
   `PositionMapWidget`) with **contradictory** precision policies (4-char vs 6-char).
   v2 unifies the control language and makes precision an explicit, defaulted choice.
2. The single bundled raster physically resolves to roughly 0.18°/pixel and is 1:1 at
   `maxZoom={2}`. Region boxes (multi-degree) are well served; point-picking
   (a 6-char locator is arcminutes) is not resolvable at any zoom. **Higher zoom
   requires real tiles** — hence the LAN tile source as the foundation, not a polish item.

## 4. Design overview — three pillars

Map-Picker v2 is one design across three implementation units with a hard dependency
on the tile foundation:

- **Pillar 1 — LAN tile source (`tuxlink-dyop`, foundation).** A Rust tile-gatekeeper
  fetches map tiles only from an operator-configured permitted LAN host and serves them
  to the webview; the webview never fetches tiles directly, so CSP stays `'self'`.
  Raises `BaseMap`'s zoom ceiling when a source is live. Falls back to the bundled
  raster when no source is configured or the source is unreachable.
- **Pillar 2 — Shared navigation control surface (`tuxlink-a1cc`).** A complete,
  reusable control language on `BaseMap`, instantiated by both pickers.
- **Pillar 3 — Position overlay picker (`tuxlink-sdbd`).** The Position map becomes an
  expand-to-overlay in-app picker launched from the Compose form, reusing the shared
  control surface in pin mode, with an explicit precision selector.

The shared control surface (Pillar 2) is described once in §5; §6 and §7 record only
each surface's deltas from it.

## 5. The shared picker control surface (Pillar 2 — `tuxlink-a1cc`)

The control surface is one component family rendered over `BaseMap`. It carries every
control a competent map picker needs; the two surfaces differ only in mode-specific
affordances (§6, §7).

**Toolbar (above the map):**

- **Mode toggle** — segmented `✋ Pan | ▢ Draw region`. **Pan is the default**: a
  map drag moves the map. The box-draw gesture lives behind the explicit `Draw region`
  mode and returns to `Pan` after one box is drawn. This replaces the current
  overload where a drag can only draw a box and never pans. The mode toggle is present
  only on surfaces that draw a box (the GRIB region picker); the Position picker omits
  it (§6).
- **Maidenhead-grid overlay toggle** — shows/hides the Maidenhead lattice. Default on.
- **Jump-to input** — accepts a Maidenhead grid (`CN87`) or a decimal `lat,lon` pair
  and recenters the view.

**On-map controls:**

- **Zoom `＋` / `−`** — explicit, prominently sized (not Leaflet's default small
  control). Bound to the active zoom ceiling (raster `maxZoom` when bundled-only;
  higher when a tile source is live, §8).
- **Fit-world** — zooms out to the full world rectangle.
- **Fit-selection / Center-on-pin** — frames the current region box (GRIB) or recenters
  on the pin (Position).
- **Live cursor coordinate readout** — the lat/lon under the pointer.
- **Scale bar** — a distance reference appropriate to the current zoom.
- **Tile-source status pill** — shows the active source and zoom, e.g.
  `z4 · bundled raster` or `z13 · LAN tiles`; reflects fallback state when a configured
  source is unreachable.

**Selection state (below the map):** a surface-specific summary (region chip + size +
`Clear region` for GRIB; locator readout + precision selector for Position) plus the
surface's editable fields, all of which stay type-in editable and stay synchronized
with the map.

**Interaction-model note (load-bearing, not CSS):** the region box becomes a
**persistent, adjustable object** with eight drag handles (four corners + four edge
midpoints). Dragging a handle nudges that edge; the box is no longer redraw-from-scratch
only. This is a new interaction (handle hit-testing + per-handle drag math) over the
current `GridMapPicker`, which resets `startRef` and rubber-bands a fresh rectangle on
every `mousedown`. The plan must treat handle editing as first-class behavior.

## 6. Position picker — expand-to-overlay + precision (Pillar 3 — `tuxlink-sdbd`)

**Real-estate model: expand-to-overlay.** The Position form no longer hosts a cramped
inline map. Instead:

- The Compose Position form shows a small **confirm-only preview** strip plus a
  **`Pick on map…`** button.
- `Pick on map…` opens a large **in-app overlay picker** — the same overlay pattern as
  `GribRequestPanel` (a dimmed backdrop + centered panel), **not** an OS pop-up window.
  This honors the inline-UI / no-window-clutter rule; the Compose window is the settled
  exception the overlay rides inside.
- The overlay picker fills its panel, carries the full shared control surface (§5), and
  on confirm returns the chosen grid to the form and closes.

**Pin mode, no mode toggle.** The Position picker drops a pin on click, lets the pin be
dragged to fine-tune, and pans on map drag. Click and drag are distinct gestures, so
there is no box-vs-pan overload to resolve; the `Pan | Draw region` toggle is therefore
absent on this surface.

**Precision selector.** A segmented `4-char (default) | 6-char` control owns report
precision. **4-char is the default** (the broadcast / APRS precision-reduction default;
see the GPS-precision memory). The operator opts into 6-char for finer reports. This
control replaces the current silent contradiction where `PositionMapWidget` hard-emits
6-char while `GridMapPicker` truncates to 4-char — precision becomes an explicit form
decision, not a buried `.slice()`. Higher-zoom LAN tiles are what make a 6-char placement
accurate to begin with.

**Reset to GPS fix.** A control returns the pin to the `PositionArbiter`'s current fix,
so an operator who pans away can recover the GPS-derived location.

**Deltas from §5:** omit the `Pan | Draw region` toggle; `Fit-selection` becomes
`Center on pin`; the below-map summary is the locator readout + precision selector +
`Reset to GPS fix` rather than a region chip.

## 7. GRIB region picker — the locked complete surface (Pillar 2 consumer — `tuxlink-a1cc`)

The GRIB region picker is the reference instantiation of §5 and is **locked** (operator,
2026-06-08). It keeps box mode with the `Pan | Draw region` toggle (Pan default), the
eight-handle adjustable region box, the four signed lat/lon fields (type-in editable,
synchronized with the box), the grid-spacing fields, a region summary chip with size and
`Clear region`, and the full on-map control cluster from §5.

**Open question for the plan:** whether the GRIB region picker also gains an
expand-to-overlay affordance for a larger drawing surface, or stays inline at an
increased height. The GRIB panel is already a large overlay, so the inline map has more
room than the Position form's did; this is deferred to the GRIB plan, not decided here.

## 8. LAN tile architecture (Pillar 1 — `tuxlink-dyop`, foundation)

The tile source is a **LAN HTTP tile server** (operator decision, 2026-06-08), not a
local archive file. The design honors the locked 2026-06-07 spec §3 and the #481
adversarial-review C10 split that deferred it. **This section is post-adversarial-review
(5 rounds, 2026-06-08, incl. cross-provider Codex — see §8.9 for round provenance and
dispositions); the resolutions below are decisions, not open defaults.**

**Gatekeeper boundary.** The webview never fetches tiles directly. A Rust backend
gatekeeper fetches tiles **only** from the operator-configured permitted LAN source and
serves them to the picker through a Tauri-local mechanism. No network or LAN host is ever
added to `img-src` / `connect-src`. Public OSM remains a non-option in the chain.

### 8.1 CRS contract — the source MUST serve EPSG:4326 / geodetic tiles (P1, ship-blocker)

`BaseMap` runs `L.CRS.EPSG4326` (equirectangular). "Standard XYZ `{z}/{x}/{y}`" servers
almost always serve `EPSG:3857` (Web Mercator); overlaying 3857 tiles on a 4326 map
produces **silently wrong, plausible-looking coordinates** — the worst failure class for a
position-reporting tool. Leaflet cannot switch CRS on a live map (it is a full remount), so
a "4326-when-bundled / 3857-when-live" hybrid is rejected.

**Decision (Option A):** the map stays `EPSG:4326` for the app's lifetime; the LAN source
**must serve a geodetic / EPSG:4326 tile pyramid** (`gdal2tiles --profile=geodetic`,
MapProxy `GLOBAL_GEODETIC`, etc.). This preserves `projection.ts`, the bundled raster, and
the Maidenhead math unchanged (the §10 "reused, not rebuilt" promise). Option B (switch the
app to 3857) is rejected: it forces re-rendering the bundled raster to Mercator, rebuilding
`projection.ts`, and loses ±90° pole coverage.

**Mandatory CRS-mismatch guard.** Because a mismatched source renders plausible-but-wrong
rather than failing loudly, the gatekeeper MUST positively verify the source's CRS/tiling
before showing tiles (probe TileJSON/WMTS/`mbtiles` metadata) and refuse on mismatch
(status pill → "incompatible tile source — expected EPSG:4326"). A test fixture MUST prove
alignment at equator, mid-latitude, and high-latitude points before any source is trusted.
The Settings UI states the geodetic requirement explicitly.

### 8.2 Serving mechanism — pinned by a WebKitGTK CSP spike (FIRST plan task) (P1)

The claim "CSP stays `'self'`" is **false as written**: every viable mechanism adds one
token to `img-src`. The honest, binding guarantee is: **no network/LAN host is ever added
to `img-src` or `connect-src`; the webview reaches tiles only through a Tauri-local source,
and the gatekeeper remains the sole network egress for tiles.**

Two candidates remain, and the cross-provider review split on them (Claude favored
`invoke`+`blob:`; Codex cautioned against adding `blob:` without proof). They are therefore
**not decided here** — the `dyop` plan's FIRST task is a WebKitGTK spike that pins the
mechanism against the *packaged* (not dev) CSP:

- **(a) custom `tile` URI scheme** behind a Leaflet `TileLayer`: on Linux/WebKitGTK this
  resolves to `http://tile.localhost`, so `img-src` gains `tile: http://tile.localhost`;
  requires an async URI-scheme handler and `subdomains: []` (no `{s}` rotation). Bespoke
  `tile` scheme ONLY — never the general asset protocol.
- **(b) `invoke` returning tile bytes** → `blob:` object URLs via a custom `GridLayer`:
  `img-src` gains only `blob:`; requires `revokeObjectURL` on Leaflet `tileunload` as
  first-class behavior (un-revoked blobs are a Pi-class OOM) with a leak-assertion test.

**Forbidden:** a loopback-HTTP tile server (would require `img-src http://127.0.0.1:*`,
turning any webview script into a localhost-port probe; the existing forms-scoped
`connect-src http://127.0.0.1:*` must NOT be relied on or widened). The spike's output is a
decision + a real packaged-CSP test asserting tiles render and no external host is listed.

### 8.3 SSRF enforcement — socket-layer, invisible, load-bearing (P1)

SSRF egress hygiene is implementation correctness, NOT a WLE-parity / Part-97 UX safeguard;
it does **not** conflict with the no-added-safeguards posture (which governs app/UX
behavior and the operator's deliberate choices, not coercion of a backend deputy). The
operator is not a boundary against their own webview. Config-time string validation is
insufficient (DNS rebinding defeats it). The gatekeeper MUST:

- Accept only `http`/`https` schemes; reject URL-embedded credentials; never accept a
  caller-supplied full URL — only validated integer `{z}/{x}/{y}` against a stored source.
- **Resolve DNS at fetch time and validate the connected IP** (rebinding defense); reject
  public, loopback, unspecified, multicast, link-local, cloud-metadata, and IPv4-mapped/
  link-local IPv6 addresses. **Allow only RFC1918 IPv4 + ULA IPv6** by default; loopback is
  gated behind an explicit dev opt-in (tests / local tileserver).
- Build the reqwest client with `redirect::Policy::none()` (a tile 3xx is a hard error →
  fallback); apply a short timeout; cap response size and require an image `Content-Type` +
  magic-byte check before caching/serving.

**Config UX stays trusting:** no modals, no hard-blocking the operator's chosen LAN host;
on a *public*-resolving host, warn (do not block). Enforcement lives at the socket layer,
invisibly. Model on `forms/updater.rs::classify_transport` + its reqwest client, but write
a dyop-specific `classify_tile_host` (private-IP `http` OR `https`) — do not reuse the
updater's `https_only`-only posture unchanged.

### 8.4 Cache — traversal-safe, bounded, poison-resistant (P1/P2)

- **Keys are validated integers only.** Parse `z`/`x`/`y` as `u32`; enforce `z ∈ [0,
  max_zoom]`, `x,y ∈ [0, 2^z)`; reject otherwise *before* any path or fetch. Compute the
  TMS flip (if configured) *after* validation and re-assert range; the fetch key and cache
  key must use the same coordinate.
- **Namespace per source** = `sha256(normalized source URL + CRS + scheme)` hex digest as
  the directory name (filesystem-safe, collision-free, and a different source ⇒ a different
  subtree, so changing servers can't serve stale tiles). Build paths from validated
  integers, then canonicalize and assert the result is under `cache_root`.
- **Bounded:** hard total-byte cap (default 256–512 MB, operator-configurable) + LRU
  eviction by last-access; evict-before-write; a failed/ENOSPC write degrades silently to
  fetch-through, never a user-facing error. Cache **only** `200 + image magic-bytes +
  non-empty`; write temp-file + atomic rename; single-flight de-dup per key (thundering
  herd + racing-write corruption). Location: `app_data_dir()` →
  `~/.local/share/tuxlink/tile-cache/`. Settings exposes **"Clear tile cache"**; removing a
  source purges its subtree.

### 8.5 Fallback — source-level states, no UI hang, no mixed precision (P2)

No synchronous network on startup or map mount; the map renders fully from the bundled
raster with zero network, and the zoom ceiling rises *lazily* only after the first
validated tile. Per-tile fetches are timeout-bounded (short, ~3–5 s) and cancelled on
pan/zoom. A source-level **circuit-breaker** (K consecutive failures → "degraded" →
cooldown, re-probe on expiry) prevents per-tile timeout storms. Source state drives
behavior, not per-tile mixing:

| Source state | Behavior | Status pill |
|---|---|---|
| No source configured | No fetch; ceiling stays z2; bundled only | `z{n} · bundled` |
| Configured, validated, live | Serve live/cached tiles | `z{n} · LAN live` |
| Cache hit (offline) | Serve cached | `z{n} · LAN cached as of …` |
| Tile 404 above raster-native zoom | **No stretched-raster fill** (that is illusory precision, C6) — explicit "no coverage" treatment or clamp pannable zoom to covered extent | `z{n} · LAN live (partial)` |
| Host unreachable / timeout | Drop tile layer, clamp ceiling to z2, bundled | `tiles unreachable — bundled` |

The bundled raster backstops live tiles only at/below raster-native zoom; above it, a 404
must not be papered over with upscaled raster.

### 8.6 Zoom ceiling + precision gating (P1)

`maxZoom` does NOT rise merely because a source is "configured." It rises only on
**validated source metadata + successful tile probes for the current CRS/scheme**, capped
at a configured maximum (~16), never at server claims alone. **6-char placement in the
Position picker is disabled/warned unless the view under the pin is backed by validated
real tiles** (ties precision to proven pixels, not a zoom number). `MaidenheadOverlay`'s
`levelFromZoom` and `zoomSnap` are re-tuned for the full zoom range (finer lattice levels
or fade-out at high zoom; consider `zoomSnap=1` once real tiles exist for crisp 1:1
rendering). Raising the ceiling deliberately widens the frozen `BaseMapProps` contract
(C11) — coordinated in the `dyop` plan, not bumped ad hoc by `a1cc`/`sdbd`.

### 8.7 Source configuration (Settings) — more than a URL (P2/P3)

A single URL field is insufficient. The source config carries: URL, **source type/CRS**
(geodetic required), **XYZ vs TMS** scheme flag (default XYZ; `.mbtiles`-backed sources are
usually TMS and it cannot be auto-detected), min/max zoom, cache budget, an **optional
local attribution string** (LAN tiles may be OSM-derived and attribution-bound even when
self-hosted; `BaseMap` currently disables attribution), and a source label. Auth: none by
default; if ever needed, credentials go to the OS keyring, never to disk config. A
TileJSON/WMTS-capabilities mode, if added, is a separate supported mode under the same SSRF
validation.

### 8.8 Offline-first contract (binding)

The feature is **strictly opt-in, invisible until a source is configured, and the app's
full functionality — including the map — never depends on it.** This sentence is the
contract that keeps §8 consistent with §10's "works fully offline" guarantee: tiles are a
pure enhancement over a fully-functional offline base.

### 8.9 Adversarial review outcomes (5 rounds, 2026-06-08)

Rounds: 4 Claude agents (SSRF/host-validation; CSP/serving; projection/CRS/zoom;
cache/fallback/offline) + 1 cross-provider Codex round. Raw transcripts local-only under
`dev/adversarial/` (gitignored). Cross-provider convergence on all four P1s (CRS, SSRF,
CSP-asserted-not-designed, maxZoom-gating) — strong real-not-artifact signal. Unique Codex
catches folded in: attribution/licensing (§8.7), and gating 6-char precision on
validated-real-tiles rather than a zoom number (§8.6). Genuine cross-provider tension on
the serving mechanism (§8.2) is resolved by deferring to a packaged-CSP WebKitGTK spike
rather than picking blind.

## 9. Decomposition and sequencing

One design, three implementation units, one hard dependency edge:

| Unit | Issue | Depends on | Scope |
|---|---|---|---|
| LAN tiles (foundation) | `tuxlink-dyop` | `jx4i` (this design) | Gatekeeper, local serving, Settings URL, cache, fallback, `maxZoom` raise |
| Shared nav controls | `tuxlink-a1cc` | `jx4i` | §5 control surface + GRIB box handles (§7) |
| Position overlay | `tuxlink-sdbd` | `jx4i`, benefits from `dyop` + `a1cc` | §6 expand-to-overlay + precision selector |

**Implementation order: tiles-first.** `dyop` is the foundation because the zoom
controls and the Position precision are only fully meaningful once real tiles exist.
`a1cc` (controls) and `sdbd` (Position overlay) can proceed against the raster-only
ceiling and gain their full value when `dyop` lands; the control surface and the precision
selector are designed to degrade gracefully to bundled-only (zoom capped at 2, status pill
reads `bundled raster`).

**Each unit gets its own implementation plan** (writing-plans) derived from this design.
The tile foundation (`dyop`), carrying the security boundary, additionally gets the
cross-provider adversarial-review treatment (build-robust-features) before build; the two
UX units are largely plumbing against this approved design and follow the lighter
TDD-against-spec path unless the plan surfaces a hard-to-undo decision.

## 10. Non-goals and constraints

- **No RF/transmit path** is touched anywhere in Map-Picker v2; RADIO-1 does not gate.
- **CSP stays `'self'`**; no task may re-add an external tile host to `img-src` /
  `connect-src`. Public OSM is never a permitted source.
- **WLE parity is features, not UX:** the picker copies the *capability* of a map-based
  region/position selector while rejecting WLE's interaction model. The control surface is
  intentionally better than WLE's.
- **No in-app GRIB rendering** (unchanged): GRIB responses remain external-viewer files.
- **The offline foundation stands:** `BaseMap`, the Maidenhead math, and the bundled
  raster are reused, not rebuilt.

## 11. Approved decisions (this brainstorm)

1. Position real-estate model: **expand-to-overlay in-app picker** (not bigger-inline).
2. GRIB navigation: **explicit `Pan | Draw region` mode toggle** (Pan default), not a
   Shift-drag modifier.
3. GRIB region box: **eight-handle adjustable object**, not redraw-from-scratch only.
4. Position precision: **explicit `4-char default / 6-char opt-in` selector**, resolving
   the current 4-vs-6 contradiction.
5. Tile source: **LAN HTTP tile server** behind a Rust gatekeeper, not a local archive
   file.
6. Shared control surface across both surfaces (§5), instantiated per mode.

**Post-adversarial-review decisions (2026-06-08, §8.9):**

7. CRS: **require EPSG:4326/geodetic LAN tiles + a mandatory CRS-mismatch guard** (Option A);
   no runtime CRS switch (§8.1).
8. SSRF: **socket-layer, fetch-time resolved-IP enforcement** (RFC1918/ULA allow,
   default-deny public/loopback/link-local/metadata, no-redirect, integer-validated coords);
   config UX stays trusting — warn-not-block on public host (§8.3).
9. `maxZoom` + 6-char precision **gated on validated metadata + real tile probes**, not a
   source being merely "configured" (§8.6).
10. Serving mechanism (custom `tile` scheme vs `invoke`+`blob:`) **deferred to a packaged-CSP
    WebKitGTK spike as the `dyop` plan's first task**; loopback-HTTP serving forbidden (§8.2).

## 12. Open items for the plans / adversarial review

- `dyop`: items previously open here (host-validation posture, cache size/eviction, serving
  mechanism) are **now resolved in §8.1–8.8**. The one genuinely deferred item is the
  serving-mechanism **spike** (§8.2) — a *plan task*, not an undecided design question.
- `a1cc`: whether the GRIB region picker gains an expand-to-overlay affordance or stays
  inline-taller (§7).
- All: visual mockups produced during the 2026-06-08 brainstorm live locally under
  `.superpowers/brainstorm/` (gitignored); this document is the authoritative description.
