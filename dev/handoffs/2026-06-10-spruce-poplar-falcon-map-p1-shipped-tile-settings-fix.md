# 2026-06-10 spruce-poplar-falcon — Map-Picker v2 P1 shipped + Map-tiles overlay bug fixed

## One-sentence frame

Finished the "set my location" P1 trio from the map-features plan (pin-on-map at
Find-a-Gateway, Position expand-to-overlay, 4/6-char precision selector) and then
root-caused + fixed an operator-reported production regression where the Map-tiles
settings panel rendered inline and compressed the whole app. Three PRs merged,
all CI-green and unit-tested. **The one open thread is WebKitGTK/grim render
verification of all three surfaces** — blocked this session by `:1420` contention,
not by any code concern.

## What merged this session (all on main)

| PR | Merge | bd | What |
|---|---|---|---|
| **#550** | `6e27724` | tuxlink-3iav (closed) | **Find-a-Gateway pin-on-map.** "Your location" field gains a "Pick on map…" button → reuses the existing `GridPickerOverlay` (pin, 4-char). Triage #18. |
| **#551** | `df6c54e` | tuxlink-sdbd (closed) | **Position expand-to-overlay + precision selector.** New `PositionPickerOverlay`; the cramped inline 240px map → small confirm preview + "Pick on map…" → large overlay. Segmented 4-char (default) \| 6-char, gated on `sixCharAllowed`. Triage #18/#19, design §6. |
| **#554** | `d8ec2cd` | tuxlink-jgom (closed) | **Fix: Map-tiles settings rendered inline.** Operator report — panel spawned under the bottom bar, compressed the app. |

All three: TDD (RED→GREEN), `tsc` + `lint:docs` clean, CI build+verify both arches green. No Rust touched.

## The bug fix (tuxlink-jgom) — root cause worth remembering

`MapTileSettingsPanel` (PR #548) reuses the shared `.tux-settings-*` overlay chrome
but is **lazy-loaded into its own chunk** and imported only its own CSS. The
load-bearing `.tux-settings-backdrop { position: fixed; inset: 0; z-index: 100 }`
lives in `src/shell/SettingsPanel.css`, imported only by the GPS `SettingsPanel`
(a *different* lazy chunk). Opening **Tools → Settings → Map tiles…** without first
opening GPS Settings left the chrome CSS unloaded → the backdrop rendered as a
normal in-flow block at `AppShell.tsx:1278` (below the bottom bar), shoving the app
up and leaving the form unstyled. jsdom unit tests missed it (no layout/`position:fixed`).

**Fix:** `MapTileSettingsPanel.tsx` now imports `../shell/SettingsPanel.css`, so the
chrome ships with its own chunk. Regression test asserts the panel's own CSS import
graph contains the backdrop positioning. **Pattern for the future:** any lazy panel
reusing `tux-settings-*` must import the chrome CSS itself.

## OPEN — the one thing the next session must finish: grim-verify

**None of the three surfaces have been WebKitGTK/grim-rendered.** Blocked this
session: `:1420` (single dev-server machine-wide) was held by the unrelated
`bd-tuxlink-hbbw-request-center-reskin` session's vite build the whole time; earlier
the Pi was also at loadavg 17 with a concurrent cargo build. The Pi later freed
(loadavg ~4) but `:1420` stayed occupied. Per project posture, smoke is post-merge /
opportunistic, not a merge gate — so all three shipped on automated gates.

**Smoke click-paths (one converged/dev build covers all three):**

1. **Map-tiles fix (highest priority — the reported bug):** Tools → Settings → Map
   tiles… opens as a **centered fixed overlay** (dimmed backdrop), the app behind is
   **not** compressed, the form is styled. Critically, test this **on a fresh app
   without opening GPS Settings first** (that was the broken path).
2. **Find-a-Gateway pin:** open Find a Gateway → "Pick on map…" → the GridPickerOverlay
   renders constrained (not full-bleed), map fits, pin sets a 4-char locator into "Your location".
3. **Position overlay:** Message → New Message → GPS Position Report → "Pick on map…" →
   the large `PositionPickerOverlay` renders (constrained), precision selector shows
   **4-char active / 6-char disabled-with-hint**, the inline preview strip reads the grid.

## Other open work (bd)

- **tuxlink-n6xu** (new, P2, filed this session, **depends on tuxlink-a1cc**):
  6-char precision is correctly **gated off** in the Position overlay because
  `PositionMapWidget` passes no `tileSource` to `BaseMap` (frozen C11) and has no
  zoom controls, so the live view can't reach `SIX_CHAR_MIN_ZOOM=12`. Unlocking
  real 6-char needs the a1cc §5 control surface (zoom + live view.zoom) + a
  coordinated C11 widening so `PositionMapWidget` can pass a validated tile source.
- **tuxlink-a1cc** (P2, reopened earlier): §5 shared nav control surface (zoom +/-,
  fit, jump-to, scale bar, TileStatusPill) + §7 GRIB 8-handle box. The biggest
  remaining map gap behind "didn't increase navigability."
- **P3 tail:** #30 (click-off grid revert), #29 (status-bar local time from grid),
  #16b (offline station-location viewer).

## Repo / worktree state at handoff

- **Main checkout** on `bd-tuxlink-xygm/recover-handoffs` (this branch is far behind
  origin/main by design — map CODE lives on origin/main; all code work this session
  was done in worktrees off `main`).
- **All three of my worktrees disposed** (ADR 0009 ritual, inventory clean — tracked
  committed+merged, no untracked, only gitignored `node_modules`): `bd-tuxlink-3iav-find-gateway-pin`,
  `bd-tuxlink-sdbd-position-overlay-picker`, `bd-tuxlink-jgom-map-tiles-overlay-css`.
  **No worktree left in flight by this session.**
- Concurrent sessions observed (left untouched): `bd-tuxlink-hbbw-request-center-reskin`
  (vite on :1420), and an earlier `bd-tuxlink-uodl` cargo build. The 7 repo-global
  stashes are other sessions' — not mine.

## Process notes (learning-sandbox)

- The jgom bug is the textbook case for **why grim matters and why it isn't a merge
  gate here**: a real WebKitGTK render bug that every unit test passed (jsdom can't
  compute `position: fixed`). The fix is regression-tested at the *import-graph* level
  (the true invariant), but the operator-facing proof is the rebuild. On a single-`:1420`,
  shared Pi, the honest move is ship-on-CI + smoke-when-the-port-frees, not block.
- Stacked three independent PRs off `main` in parallel (read-while-CI-builds) rather
  than serially — kept wall-clock down without piling unverified work, since each
  reuses already-grim-verified building blocks.
