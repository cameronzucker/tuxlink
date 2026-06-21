# Handoff — yew-butte-larch — START HERE: migrate the remaining map surfaces to Leaflet (UNSUPERVISED 4–6h)

**Agent:** yew-butte-larch · **Date:** 2026-06-21
**Headline:** Next session's job is to **immediately begin migrating the four remaining MapLibre map surfaces to Leaflet**, completing the strangler-fig that started with `AprsPositionsMap`. The **operator is OUT running errands and you will work UNSUPERVISED for 4–6 hours.** All the up-front decisions you'd otherwise need to ask have already been answered (below) — so do not block; execute. Surface anything genuinely new in the handoff for the operator's return, but keep migrating.

---

## THE TASK (do this immediately, no warm-up)

Migrate these four surfaces off MapLibre to Leaflet, **faithful 1:1 behavior ports**, one per branch/worktree/PR off `origin/main`:

| Surface | File | bd issue |
|---|---|---|
| Find-a-Station map | `src/catalog/StationFinderMap.tsx` | **tuxlink-mncq** |
| Compose position picker | `src/compose/PositionMapWidget.tsx` | **tuxlink-kkd3** |
| Location / GPS map | `src/location/LocationMap.tsx` | **tuxlink-4hol** |
| Maidenhead grid picker | `src/map/GridPicker.tsx` | **tuxlink-rqvk** |

Then, **only if all four merged cleanly**, delete the MapLibre substrate: **tuxlink-lru7**.
Epic: **tuxlink-u3qe** (carries the full decision record). All four surfaces are `bd ready` now.

## OPERATOR DECISIONS (locked 2026-06-21 — do not re-litigate)

1. **Merge policy: merge each surface on CI-green** (`pnpm typecheck` + `pnpm vitest run` + `pnpm build`). Ship continuously; fix-forward — exactly how `AprsPositionsMap` shipped. Don't park PRs waiting for the operator.
2. **Port scope: faithful 1:1 behavior port.** Engine swap only — preserve each surface's current behavior, features, and reachability. **No redesign** (so no brainstorming gate is triggered). If a surface seems to *need* a redesign to port, that's the rare thing to flag and defer — port the rest.
3. **Basemap: share the Leaflet dark basemap, but keep light/dark switching by THEME** (like MapLibre did). **This is already handled by the substrate** — see below. The trap to avoid: do **not** pin a `flavor` prop.
4. **Substrate teardown: ONLY if all four merge cleanly this session.** If any surface didn't make it, skip tuxlink-lru7 and leave MapLibre intact.

## The template + substrate (everything you need is already built)

`AprsPositionsMap` is the worked example. Reuse:
- `src/map/LeafletMap.tsx` — the provider component. **Render `<LeafletMap initialCenter initialZoom …>` WITHOUT a `flavor` prop.** It calls `useBasemapFlavor()` internally and uses `effectiveFlavor = flavor ?? themeFlavor`, so omitting `flavor` gives you **theme-reactive light/dark basemap switching for free** — satisfying decision #3. Pinning `flavor` would defeat it.
- `src/map/LeafletMapContext.ts` — `useLeafletMap(): L.Map | null`.
- `src/map/leafletHooks.ts` — `useLeafletLayerGroup(map)` for owning an overlay LayerGroup.
- `src/map/basemapLeaflet.ts` — `buildBaseLayers(flavor, packs)`, `BasemapFlavor = 'light'|'dark'`, `flavorBackground()`. Already light/dark capable.
- `src/map/LeafletRecenterControl.tsx` — recenter control pattern.
- Pattern in `AprsPositionsMap`: inner component reads `useLeafletMap()`, owns markers via `useLeafletLayerGroup`, wraps every Leaflet mutation in a `safe(what, fn)` try/catch → `reportFrontendError` (never throw to the ErrorBoundary).

The MapLibre side you're replacing per surface: `src/map/MapLibreMap.tsx`, `src/map/MapContext.ts`, `src/map/mapHooks.ts`, `src/map/basemapStyle.ts` (+ `darkStyle`, `tuxlinkFlavor`), `src/map/testMapLibreMock.ts`. Leave these in place until **all** their consumers migrate (tuxlink-lru7 deletes them last).

## Workflow (recommended)

For each surface: short written plan → `superpowers:subagent-driven-development` (or do it directly if simple — GridPicker/LocationMap are likely small). One **bd issue + worktree + branch off `origin/main` (157ac619)** per surface (`python3 .claude/scripts/new_tuxlink_worktree.py --slug <s> --issue <id> --base main --moniker <yours>`; the script prepends `origin/` — pass `--base main`, NOT `origin/main`). Claim the bd issue, migrate, gate, push, PR, **merge on green**, dispose the worktree (ADR 0009 ritual).

## Verification while unsupervised

- **Gate = CI** (`typecheck`/`vitest`/`build`). jsdom can't render a real map, so map unit tests assert wiring/structure (see `AprsPositionsMap.test.tsx` for the mock pattern; there's a Leaflet test setup already).
- **Bonus, and worth doing:** you CAN grim-self-validate the Leaflet render on pandora — **Leaflet's 2D-canvas renders fine under WebKitGTK**, unlike MapLibre's WebGL which blanks (see memory `project_webkitgtk_dmabuf_static_and_webgl_blank` + `reference_grim_realapp_validation_pandora`). So a fresh `tauri dev` + grim screenshot is a real visual check you can run yourself. NOTE the `:1420` strictPort collision — only one `tauri dev` runs machine-wide; the operator's dev server may be up (don't fight it; build in your worktree and grim only when the port is free, or rely on CI).
- **Wire-walk for a faithful port** = confirm the migrated surface is still rendered by its existing caller(s) with equivalent reachable behavior (grep the call sites; the behavior contract is "same as before the swap"). The operator does the visual/UX confirmation on return — that's the backstop, not a blocker to merge.

## Pitfalls / guardrails (you're alone — these matter)

- **Faithful port** means no behavior change — if you're tempted to "improve" the UX, stop; that's deferred.
- **Don't pin `flavor`** on `<LeafletMap>` (kills theme switching — decision #3).
- **Don't delete the MapLibre substrate** until all four surfaces are merged (tuxlink-lru7 is dep-blocked on all four for exactly this reason).
- Main-checkout writes: if the `block-main-checkout-race` hook denies, make a worktree (don't fight it).
- `git commit --amend` and all destructive-git are hook-banned — new commits only.
- Each merged surface: dispose its worktree (ADR 0009), delete the remote branch.

## Session state at handoff

- **main = `157ac619`** (post-merge of #849 path animation + #850 layers panel). Local `main` ref is STALE (ancient) — branch worktrees off `origin/main`, and the worktree script's `--base main` resolves to `origin/main` correctly.
- **Shipped this session:** #849 (cn84 path animation: port → schedule/geometry → Canvas2D layer → triggers → 2× speed → concurrent coexisting traces) and #851 (map stale grey-out 15m→1h, drop TTL 60m→3h). Both merged, branches + worktrees disposed.
- **tuxlink-ul9l (TABLED):** the "implausible single hop" RF-honesty thread — fully diagnosed, root cause = a digipeater relaying without inserting its callsign (non-tracing WIDE = a real-world Part 97 ID gap, NOT a Tuxlink bug). Findings recorded durably in its bd `--design`. The in-frame fix signal (WIDEn-N consumed vs traced) is documented there. **Do not work it** — operator tabled it.
- **Stale worktree `worktrees/bd-tuxlink-c973-placenames-packs`** is on the merged-dead `bd-tuxlink-qnu6/digipeat-path-anim` branch and holds an **uncommitted `[digipeat-resolve]` console diagnostic** + the `dev/scratch/aprs-raw.log` capture. Harmless (never committable on a dead branch); self-cleans when that worktree is disposed. You may dispose it (ADR 0009) to tidy up.
- Follow-ups also open: tuxlink-k808 (user-configurable map timings), tuxlink-ae8s (empty-start first-frame), a P3 for the cn84 `pos?` label.
- `.github/RELEASE_FREEZE` STAYS (unfreeze plan unchanged: after qnu6 landed AND Delete/tuxlink-wl7n ships → big-bang release; qnu6 has landed, Delete still pending).

## Pending / next after the migration

1. Migrate all four surfaces (this handoff) → merge each on green.
2. If all four clean → delete MapLibre substrate + drop `maplibre-gl` (tuxlink-lru7).
3. Operator returns → visual confirmation of the migrated surfaces (the backstop).
4. Then: Delete (tuxlink-wl7n), then unfreeze + big release.
