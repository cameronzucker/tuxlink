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
adversarial-review C10 split that deferred it.

**Gatekeeper boundary.** The webview never fetches tiles directly. A Rust backend
gatekeeper fetches tiles **only** from the single operator-configured permitted LAN host
and serves them to the picker through a Tauri-local mechanism, so the webview's CSP stays
`'self'` and no external `img-src` / `connect-src` tile host is ever whitelisted. Public
OSM remains a non-option in the chain.

**Defaults carried into adversarial review** (plumbing — documented here, to converge in
adrev rather than be decided by the operator):

- **Serving mechanism:** a custom Tauri protocol / local asset endpoint that the picker's
  tile layer targets (the backend returns tile bytes); the webview addresses a local
  scheme, never the LAN host.
- **Tile scheme:** standard XYZ `{z}/{x}/{y}` (TMS y-flip handled if a source advertises
  it).
- **Cache:** an on-disk tile cache to avoid re-fetching panned/zoomed tiles; size cap and
  eviction policy to be set in the `dyop` plan.
- **Fallback:** when no source is configured or the configured host is unreachable or a
  tile 404s, the picker falls back to the bundled raster and the status pill reflects it.
- **Zoom ceiling:** `BaseMap`'s `maxZoom` rises to the configured source's advertised max
  (capped at a sane point-picking level, e.g. ~16) when a source is live, and stays at 2
  when bundled-only. The "no illusory precision past the raster" rule (C6) is preserved:
  zoom only exceeds 2 when real tiles back it.
- **Auth:** none by default (open LAN tile server). If a source needs auth, credentials go
  to the OS keyring, never to disk config (per the no-disk-creds default).
- **Host configuration:** a Settings field, "Map tile server URL (permitted source only)".
  Whether the gatekeeper enforces a private/LAN address (rejecting public hosts) or trusts
  the operator-of-record is an **open question for the `dyop` plan / adrev** — the project's
  no-added-safeguards posture and the operator-of-record principle both bear on it.

**Security surface (the reason `dyop` warrants cross-provider adversarial review):** the
gatekeeper introduces an outbound fetch to an operator-named host and a local serving
endpoint. SSRF-shaped concerns (what the gatekeeper will fetch and from where), cache
poisoning, and the CSP-preservation guarantee are the attack angles the review must cover.

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

## 12. Open items for the plans / adversarial review

- `dyop`: gatekeeper host-validation posture (enforce LAN-only vs trust operator); cache
  size/eviction; exact local serving mechanism; SSRF/cache-poisoning review angles.
- `a1cc`: whether the GRIB region picker gains an expand-to-overlay affordance or stays
  inline-taller (§7).
- All: visual mockups produced during the 2026-06-08 brainstorm live locally under
  `.superpowers/brainstorm/` (gitignored); this document is the authoritative description.
