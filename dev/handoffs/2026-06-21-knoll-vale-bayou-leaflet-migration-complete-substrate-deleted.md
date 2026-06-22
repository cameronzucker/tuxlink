# Handoff — knoll-vale-bayou — Leaflet map migration COMPLETE (4/4 surfaces + substrate deleted)

**Agent:** knoll-vale-bayou · **Date:** 2026-06-21 · **Mode:** unsupervised (operator out 4–6h)
**Headline:** The Leaflet map-engine migration is **done end-to-end.** All four remaining MapLibre surfaces were ported to Leaflet (faithful 1:1, merged on CI-green), and the MapLibre substrate was deleted + `maplibre-gl` dropped. **Tuxlink now has a single map engine: Leaflet.**

---

## What shipped this session (epic tuxlink-u3qe)

| Surface | bd | PR | Merge commit |
|---|---|---|---|
| `src/catalog/StationFinderMap.tsx` | tuxlink-mncq | **#855** | 52c9f80 |
| `src/compose/PositionMapWidget.tsx` | tuxlink-kkd3 | **#857** | e0566cf |
| `src/location/LocationMap.tsx` + new `src/map/LeafletMaidenheadGridLayer.tsx` | tuxlink-4hol | **#858** | 67ebd09 |
| `src/map/GridPicker.tsx` | tuxlink-rqvk | **#861** | 80d5b0f |
| Substrate teardown (delete MapLibre + drop `maplibre-gl`) | tuxlink-lru7 | **#862** | `<pending merge>` |

All four surface bd issues are **closed**; their worktrees disposed (ADR 0009). Each PR merged with a merge commit (no squash, ADR 0010) and CI-green on all four jobs (verify ×2 arch + build-linux ×2 arch + deb-install-test).

### Decisions honored (locked 2026-06-21, not re-litigated)
- **Faithful 1:1 behavior ports** — engine swap only, no redesign; every surface's props + call sites unchanged (no consumer edits beyond test seams).
- **Shared Leaflet dark basemap, theme-reactive** — every `<LeafletMap>` rendered WITHOUT a `flavor` prop, so light/dark follows the app theme automatically via `useBasemapFlavor()`.
- **Merged each on CI-green**, fix-forward, no parked PRs.

### New shared component
`src/map/LeafletMaidenheadGridLayer.tsx` — the Leaflet twin of `MaidenheadGridLayer` (no Leaflet lattice existed; both LocationMap and GridPicker compose it). Reuses the pure `gridGeometry`, preserves the B6 padded-extent recompute gating, needs no `styledata` re-push (Leaflet overlays survive basemap swaps).

## Substrate teardown (tuxlink-lru7) — what was removed/changed
**Deleted:** `MapLibreMap.tsx` (+3 tests), `MapContext.ts`, `mapHooks.ts` (+test), `basemapStyle.ts` (+2 tests), `testMapLibreMock.ts` (+test), `MaidenheadGridLayer.tsx` (+test), `RecenterControl.tsx` (orphaned, imported the dead `MapContext`).
**Edited (shared deps that touched the substrate):**
- `useBasemapFlavor.ts` — `BasemapFlavor` type import repointed `basemapStyle` → `basemapLeaflet` (identical `'light'|'dark'`). `LeafletMap` depends on this hook.
- `aprsSprites.ts` — removed the now-dead `SpriteMap` interface + `ensureSymbolImage` (the MapLibre `addImage` sprite-registration path; the Leaflet path uses `spriteDataUrl`/`whenSheetsReady`). `renderSymbolBitmap`/`renderFallbackBitmap` KEPT (live, used by `spriteDataUrl`).
- `aprsSprites.test.ts` — dropped the `ensureSymbolImage` describe block + the `testMapLibreMock` import.
- `src/test-setup.ts` — removed the global `vi.mock('maplibre-gl', …)` + `resetMapLibreMock` afterEach (no engine left to mock).
- `package.json` — `maplibre-gl@^5` removed; lockfile updated.
**Kept on purpose:** `RecenterControl.css` (reused by `LeafletRecenterControl`); `pmtiles` + `@protomaps/basemaps` + vendored `protomaps-leaflet` (the Leaflet basemap stack).

## Verification (provenance)
- Per surface + lru7: `pnpm typecheck` + `pnpm vitest run` (full suite, ~286 files / ~3212 tests) + `pnpm build` green **locally** in each bd worktree, then **CI-green** on every PR (verify + build-linux on amd64 + arm64).
- jsdom runs Leaflet REAL (it's Canvas2D/DOM, not WebGL): tests capture the live `L.Map` via `vi.spyOn(L,'map')`, mock `buildBaseLayers` + the tauri `invoke`, and assert on layer objects + DOM — the pattern `AprsPositionsMap.test.tsx` established.
- **Not done (by design):** on-device grim screenshots — the `:1420` dev port was contended by other live sessions this session, and the operator does the visual/UX confirmation on return (the agreed backstop). Leaflet's 2D canvas DOES render under WebKitGTK (unlike the old MapLibre WebGL blank), so a grim self-check is possible when the port is free.

## ← VISUAL SURFACES TO REVIEW ON RETURN (the backstop)
Build origin/main and eyeball each migrated Leaflet surface:
```
pnpm dev:converged     # builds origin/main in a disposable worktree
```
Then exercise:
1. **Find-a-Station map** — Connections/catalog → the station map (pins coloured by reachability tier; click selects; "you" pin; recenter ⌖).
2. **Compose position picker** — Compose → Position form → "Pick on map…" overlay (click drops a pin → grid; zoom gates 6-char precision).
3. **Location/GPS map** — Settings/GPS source picker → the location map (drag the green pin or click to set; Maidenhead lattice).
4. **Maidenhead grid picker** — GRIB request form (box-drag a region) AND the grid-edit "Pick on map" overlay (pin mode).
Confirm: dark basemap follows the theme (toggle light/dark), pins/labels render, pan/zoom is smooth, no white-flash on the software renderer.

## State at handoff
- **main** advanced by 5 merges this session (#855, #857, #858, #861, + lru7). Local `main` ref in the main checkout is the operator's; I worked entirely in `worktrees/` off `origin/main`.
- **In-flight worktrees:** the lru7 worktree (`worktrees/bd-tuxlink-lru7-maplibre-teardown`) is the only one of mine left at handoff time; dispose it (ADR 0009) once #862 merges. No untracked/gitignored-stateful content beyond `node_modules`/`dist`.
- **Stale local branches:** `gh pr merge --delete-branch` couldn't delete the *local* merged branches (their worktrees held them at merge time) — `bd-tuxlink-{mncq,kkd3,4hol,rqvk}/...` may linger as local refs in the shared repo. Remotes ARE deleted. Harmless; `git branch -d` them from a worktree cwd when convenient (the main-checkout hook blocks branch ops from the main checkout while other sessions are live).
- **Shared repo stashes (NOT mine):** 7 pre-existing stashes (dates 2026-05-31…06-03) from prior sessions remain — left untouched.
- `.github/RELEASE_FREEZE` unchanged (still frozen).

## Follow-ups (filed/notable)
- **`dev/scratch/leaflet-migration-read/`** + **`dev/scratch/rqvk-draft/`** are local scratch (gitignored) — safe to delete.
- Cosmetic dead CSS: `.location-map .maplibregl-map` in `GpsSourcePicker.css` is now a dead selector (Leaflet uses `.leaflet-container`); a few historical code comments still say "maplibre" (App.tsx, ErrorBoundary, projection.ts). Harmless; a tidy-up is optional.
- Next per the prior handoff's roadmap: Message **Delete** (tuxlink-wl7n) is the remaining gate before unfreeze + big release. (qnu6 path-anim already landed.)
