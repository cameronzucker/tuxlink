# 2026-06-09 marten-poplar-dahlia — FINISH SHIPPING the map features (complete scope)

## Purpose of this handoff

The operator smoked the merged `dyop` work and found it delivered **no visible value** — a
hardened tile *backend* with no UI wired to it. This handoff is the **complete, prioritized
plan to make the map features FULLY, TOTALLY shipped** — i.e. visible and usable in the
operator's converged build, matching the mocks. Execute it to completion; do not stop at
"merged to a branch."

**The operator's bar (read this first):** "shipped" = *visible and usable in my build*, not
"pushed to a branch." Two sessions in a row produced work the operator couldn't see (dyop =
invisible backend; the first correction = unmerged branch). **Every increment from here must
land in `main` AND be grim/WebKitGTK-verified to actually render, with the operator told
exactly how to reach it.** Bias to small visible increments, not backend/branch piles.

## What is DONE (merged to main)

1. **`dyop` — LAN tile backend** (PR #517, merged). A Rust SSRF-gatekeeper that fetches/
   caches/serves geodetic LAN tiles via a `tile://` scheme; `BaseMap` gained a tile layer;
   commands `configure_tile_source`/`test_tile_source`/`clear_tile_cache`/`tile_source_status`
   exist. **BUT it is invisible:** `MapTileSourceSettings.tsx` (the config UI) and
   `TileStatusPill.tsx` were built+tested but **never mounted anywhere** — so no operator can
   configure a LAN source, so the tile layer never engages, so nothing changed on screen.
2. **PR #541** (this session, `marten-poplar-dahlia`) — the first *visible* increment:
   - **Pin-on-map → Maidenhead grid field** (triage #18): `GridEdit` (ribbon grid value →
     MANUAL → "▸ Pick on map…") opens `GridPickerOverlay` (constrained in-app overlay, 640px
     cap, dimmed backdrop) hosting `GridMapPicker` (pin mode) → "Use this location" commits via
     `config_set_grid`. New files: `src/shell/GridPickerOverlay.{tsx,css,test.tsx}`; edits to
     `GridEdit.tsx` + `AppShell.css`.
   - **GRIB form de-stretch**: `.grib-form` gained `max-width: 700px` (the named stretch
     anti-pattern; matches the position/check-in/ics309 cap).
   - 4 new tests; tsc clean; 398 shell/request tests green. **NOT WebKitGTK-rendered** — the
     overlay panel sizing / map fit must be grim-verified (do this FIRST next session).

## The mocks + triage are the source of truth (DO NOT drift again)

- **Mocks:** `.superpowers/brainstorm/<most-recent map-picker session>/content/*.html` (local
  on the Pi). The shipped dyop work drifted from these — re-open them before building UI.
- **Triage doc:** `dev/scratch/Tuxlink bd Issues-to-File.txt` (the operator's authoritative
  requirement list). Map/position items: **#16b** (offline station-location map viewer),
  **#18** (pin location on map → grid — for operators without Geographica), **#19** (6-digit
  grid selection doesn't work, only 4-digit — inconsistent), **#21** (GRIB location dialog
  should be map-based), **#29** (status-bar local time should update from entered/detected
  grid), **#30** (clicking off the grid entry field should exit + revert to last configured).
- **Design:** `docs/design/2026-06-08-map-picker-v2-design.md` — three pillars: §8 dyop
  (tiles, done-but-unwired), §5+§7 **a1cc** (shared nav control surface + GRIB box handles),
  §6 **sdbd** (Position expand-to-overlay + 4/6-char precision).

## REMAINING WORK to fully ship (prioritized — each item lands in main + is grim-verified)

**P0 — make what's merged actually reachable + verify #541 renders**
1. **Grim-smoke #541** once merged: confirm the "Pick on map…" overlay renders in WebKitGTK,
   panel is constrained (not full-bleed), the map fits, a pin commits a grid. Fix sizing if not.
2. **Mount `MapTileSourceSettings`** somewhere reachable so a LAN tile source can be configured
   (no general prefs surface exists — `SettingsPanel.tsx` is the GPS dialog). Decide its home
   (a map/tiles settings section, or inside the picker control surface). **Until this lands the
   entire dyop backend is dead weight.** De-stretch it too (it has no CSS / unconstrained inputs).
3. **Place `TileStatusPill`** where the map shows (consume it; do NOT reimplement the
   `TileSourceStatus`→display mapping).

**P1 — pin-on-map at the other real usage points (#18 fully) + #19 precision**
4. **"Find a Gateway" (`CatalogBuilderPanel.tsx`, opened from Message → Find a Gateway + the
   radio panels)** — wire a map-pin location picker so the operator sets their location on the
   map to find nearby gateways, instead of typing a grid. Reuse `GridPickerOverlay`/`GridMapPicker`.
5. **Position compose form (`PositionFormV2` / `PositionMapWidget`)** — the §6 **expand-to-
   overlay**: replace the cramped inline map with a small confirm preview + "Pick on map…" →
   large overlay picker (the full §6 surface). This is the core of bd `tuxlink-sdbd` ([BUG]
   "Position-report form map: not usable — rework", P2 in-progress).
6. **4-char/6-char precision selector (#19)** — the named bug. A segmented `4-char | 6-char`
   control (4-char default per APRS precision-reduction). Resolve the existing contradiction:
   `PositionMapWidget` hard-emits 6-char while `GridMapPicker` truncates to 4-char. The
   `sixCharAllowed`/`SIX_CHAR_MIN_ZOOM` gate from dyop (`src/map/tileSource.ts`) ties 6-char to
   validated real tiles.

**P2 — the shared nav control surface (a1cc / Pillar 2) — "navigability" + "match the mocks"**
7. **§5 control surface** on the picker(s): Pan/Draw toggle, zoom-in/out + fit cluster, grid
   toggle, jump-to, scale bar, live cursor coords, tile status pill. **This is what makes the
   map navigable and matches the mocks the operator expected** — the single biggest gap behind
   "didn't increase navigability." Build against the mocks. bd `tuxlink-a1cc` (P3, open).
8. **§7 GRIB region picker box handles** — adjustable 8-handle selection rectangle.

**P3 — triage tail + the station viewer**
9. **#30** — clicking off the grid entry field exits + reverts to last configured value.
10. **#29** — status-bar local time updates from entered/detected Maidenhead grid.
11. **#16b** — offline station-location map viewer (display station locations on the offline
    map; the Geographica-unavailable fallback). Larger; scope separately.

## Sequencing recommendation

Do **P0** first (it makes the merged backend live + verifies #541 renders — fastest path to
the operator SEEING value). Then **P1** (pin-everywhere + precision — finishes the "set my
location" story the operator most wanted). Then **P2** (a1cc control surface — the navigability/
mocks match). **P3** last. Land + grim-verify each before moving on; tell the operator the exact
click-path to reach each one.

## Process guardrails (the lessons that produced this rework)

- **Visible-first.** Don't build a backend/branch and call it done. Land a small visible
  increment in main, grim-verify it renders, tell the operator how to reach it. (`feedback_*`
  alpha-is-vettedness; no-incomplete-or-internal-refs; no-stretched-full-width-ui.)
- **Match the mocks.** Re-open `.superpowers/brainstorm` before UI work; the design decomposed
  the picker into pillars but the operator expects the *whole* picker experience to match the
  mocks, not invisible slices.
- **No full-width stretch** — cap forms/panels to the readable column (700px convention).
- **Grim/WebKitGTK is the proof** for layout (Chromium clips differently). The mock can't prove
  panel sizing / map fit.
- **vitest zombies:** reap by PID after sweeps (a broad `pkill -f vitest` self-matches the
  shell running it — reap the node worker PIDs).

## Worktree / branch state at handoff

- **`bd-tuxlink-sdbd/pin-to-grid-wiring`** — PR **#541** (pin→grid + GRIB de-stretch). Worktree
  `worktrees/bd-tuxlink-sdbd-pin-to-grid-wiring/`. Merge state: see below (this session drove it
  to merge per the operator's directive). Dispose via ADR 0009 after merge.
- **Main checkout** on `bd-tuxlink-xygm/recover-handoffs`: this handoff committed here.
- bd: `tuxlink-sdbd` (P2, in-progress) is the Position-form-map rework umbrella; `tuxlink-a1cc`
  (P3) the control-surface polish; both carry the placement notes. `dyop`/`jx4i` closed.

## No RF path anywhere in the map features (RADIO-1 does not gate this work).
