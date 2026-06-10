# 2026-06-09 oak-poplar-redwood — dyop tile backend now REACHABLE (PR #548 merged)

## One-sentence frame

Continued "finish shipping the map features": landed **P0 item 2** — mounted the
previously-dead `MapTileSourceSettings` config UI at **Tools → Settings → Map tiles…**,
so the merged dyop LAN-tile backend is no longer invisible dead weight. One clean,
fully-tested, merged increment. The rest of the map-features plan is still open.

## What shipped this session (merged to main)

**PR #548** (`bd-tuxlink-a1cc/mount-tile-settings`, merge commit `f9a6274`) — **MERGED**,
remote branch deleted, worktree disposed (ADR 0009 ritual, inventory clean).

- New **`Tools → Settings → Map tiles…`** menu entry (`menu:tools:settings_map_tiles`)
  opens **`MapTileSettingsPanel`** — a lazy inline overlay (no OS window) reusing the
  shared `tux-settings-*` dialog chrome and wrapping the standalone
  `MapTileSourceSettings` section. New `tux-mts-*` CSS: theme-aware, width-constrained
  (panel capped 560px; no full-width stretch).
- **Why it matters:** before this, no operator could configure a LAN source, so the map
  zoom ceiling never rose (design §8.6) and the tile layer never engaged on ANY map. The
  dyop backend (PR #517) was unreachable. This completes the chain: configure a source →
  tiles engage on the existing `BaseMap` tile layer.
- Files: `src/settings/MapTileSettingsPanel.{tsx,css,test.tsx}` (new); edits to
  `AppShell.tsx`, `chrome/dispatchMenuAction.ts`, `chrome/menuModel.ts` (+ their tests).
- TDD, 4 watched-fail cycles (menu contract → dispatch routing → overlay component →
  **App-level production mount path**). `tsc` clean; **full vitest 2209/2209 green**;
  CI build+verify both arches green.
- **NOT WebKitGTK/grim-rendered.** Overlay sizing / form fit should be smoked in the
  operator's converged build (Tools → Settings → Map tiles…). Per `no-hold-merge-for-
  operator-smoke`, shipped on automated gates; smoke post-merge, fix-forward.

## Operator click-path to SEE it

**Tools → Settings → Map tiles…** → enter a LAN tile URL template
(`http://192.168.1.10:8080/{z}/{x}/{y}.png`, must be **EPSG:4326 / geodetic**) →
**Test source** / **Use this source** / **Clear tile cache**. With a live source, the
existing pickers' zoom ceiling rises (design §8.6 gates 6-char precision on validated
real tiles).

## CANONICAL SOURCES OF TRUTH (re-open before any further map UI)

- **Mocks (the §5/§6/§7 surfaces):**
  `.superpowers/brainstorm/1199268-1780985474/content/` —
  `grib-complete.html`, `grib-controls.html`, `position-overlay.html`,
  `position-realestate.html`. *(Main-checkout only; not in worktrees — that's why a
  worktree grep finds nothing.)*
- **Design:** `docs/design/2026-06-08-map-picker-v2-design.md` (read §5 control surface,
  §6 Position overlay, §7 GRIB, §8 tiles). UX units are "lighter TDD-against-spec path"
  (§9) — no heavy adrev ceremony for plumbing.
- **Triage:** `dev/scratch/Tuxlink bd Issues-to-File.txt` (operator's requirement list:
  #16b, #18, #19, #21, #29, #30).

## REMAINING WORK (prioritized — each lands in main + is grim-verified)

**P0 (only validation left):**
1. **Grim-verify #541's** "Pick on map…" overlay renders in WebKitGTK (constrained, not
   full-bleed; map fits; pin commits a grid). Validation-only unless broken. Needs a full
   build — defer to when `:1420` is uncontended (one tauri dev machine-wide).
2. ~~Mount `MapTileSourceSettings`~~ — **DONE this session (PR #548).**
3. **Place `TileStatusPill`** — belongs in the §5 shared control-surface toolbar (P2,
   not built yet). CONSUME `TileStatusPill` (`src/map/TileStatusPill.tsx`); do NOT
   reimplement the `TileSourceStatus`→display mapping. Blocked on P2.

**P1 — pin-everywhere + precision (the "set my location" story the operator most wanted):**
4. **Find a Gateway** (`CatalogBuilderPanel.tsx`) — wire a map-pin location picker; reuse
   `GridPickerOverlay` / `GridMapPicker`. (Worktree `bd-tuxlink-9525-find-gateway-placement`
   exists — inspect before re-creating.)
5. **Position compose form** (`PositionFormV2` / `PositionMapWidget`) — §6 expand-to-
   overlay: small confirm preview + "Pick on map…" → large overlay picker. Core of bd
   **`tuxlink-sdbd`** ([BUG] P2 in-progress). Design §6 IS the approved spec — buildable.
6. **4/6-char precision selector (#19)** — segmented `4-char (default) | 6-char`; resolve
   the `PositionMapWidget` (hard 6-char) vs `GridMapPicker` (truncates 4-char)
   contradiction. Gate 6-char on `sixCharAllowed`/`SIX_CHAR_MIN_ZOOM` (`src/map/tileSource.ts`).

**P2 — shared nav control surface (`tuxlink-a1cc`, reopened):**
7. **§5 control surface** — Pan/Draw toggle, zoom +/- + fit, grid toggle, jump-to, scale
   bar, live cursor coords, **TileStatusPill**. Build against the mocks above. The biggest
   gap behind "didn't increase navigability."
8. **§7 GRIB 8-handle adjustable region box.**

**P3 — tail:** #30 (click-off grid field reverts), #29 (status-bar local time from grid),
#16b (offline station-location viewer — larger, scope separately).

## bd state

- **`tuxlink-a1cc`** — reopened (status `open`). Its #1 task (place the dyop UI) is DONE
  via #548; progress note records remaining = §5 control surface + §7 handles. Next
  session claims it for P2.
- **`tuxlink-sdbd`** — P2 in_progress, Position-form rework umbrella (P1 #4–6). Design §6
  is captured; buildable.
- bd state is durable in Dolt; the working-tree `.beads/issues.jsonl` is not committed here.

## Worktree / branch state at handoff

- **Main checkout** on `bd-tuxlink-xygm/recover-handoffs`: this handoff committed here.
  (Branch is ~990 commits behind `origin/main` — it's a handoff-recovery branch; map CODE
  lives on `origin/main`, so all code work is done in worktrees off `main`, NOT this branch.)
- **No worktree left in flight by this session** — `bd-tuxlink-a1cc-mount-tile-settings`
  disposed (clean: tracked committed+merged, no untracked, only `node_modules`; the 7
  repo-global stashes are other sessions', left untouched).
- A **concurrent session** was active during this session (running vitest on
  `FolderSidebar` + `AppShell`); not map work. Avoided broad `pkill -f vitest`.

## Process notes (for the learning-sandbox goal)

- One clean **merged, visible** increment beats a half-built second feature: starting P1
  unverified (no grim amid `:1420` contention) would repeat the "pile up invisible work"
  pattern that caused this rework. The operator's bar is visible+usable in their build.
- `ps`-before-`pkill` caught a concurrent vitest run a `pkill -f vitest` would have killed.
