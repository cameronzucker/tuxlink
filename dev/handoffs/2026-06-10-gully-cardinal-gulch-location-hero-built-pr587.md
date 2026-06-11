# Handoff — location-aware "For your location" hero BUILT + PR #587 open

**Agent:** gully-cardinal-gulch · **Date:** 2026-06-10 · **bd:** tuxlink-96lu

## One-line

The location-aware Request Center hero (tuxlink-96lu) — designed/specced last session — is now **fully built, tested, render-verified, and PR'd** (#587). One operator decision pends (zone coverage = 8 states; see below), but nothing blocks merge-review.

## What happened this session

Resumed on branch `bd-tuxlink-loc/location-hero` (worktree `worktrees/bd-tuxlink-loc-location-hero/`). The bd issue had been wrongly auto-closed by a grounding sweep (mis-attributed to PR #559/#576 = the re-skin + dialog); **reopened + claimed it**. Read the locked spec + mock, wrote a 13-task plan (`docs/superpowers/plans/2026-06-10-request-center-location-hero.md`), executed it via subagent-driven-development (fresh subagent per task + spec/quality review).

### Shipped (PR #587, branch `bd-tuxlink-loc/location-hero`, 25 commits)

- **Data layer (the heavy part):**
  - `src/request/nws-zones.geo.json` — NWS public-zone polygons fetched per-zone from `api.weather.gov`, DP-simplified, **pruned to the 216 mapped zones** (4.7 MB → 552 KB; lazy-loaded with the Request Center chunk).
  - `src/request/nws-zone-to-catalog.json` — **216 vetted** zone-id→catalog-filename mappings (auto-matched + abbreviated tail hand-resolved against the live NWS zone list across WA/ME/NH/NJ/OR/VT/AK + small states).
  - `src/request/nws-zone-unmapped.json` — **136 explicit** unmapped-by-design entries (multi-zone regionals, NWS-office rollups, state-level, duplicates), each reasoned.
  - `src/request/radar-regions.json` — **161 region bboxes** (derived from state extents + directional parsing; 34 hand overrides).
  - `scripts/build-request-geo.ts` (+ `.md`) — committed reproducible generation script.
- **Resolvers:** `gridToNwsZone`, `gridToRadarRegion` (geo.ts); `zoneForecastEntry`, `radarEntry` (catalogMap.ts).
- **Section:** `buildSections` location block rewritten → adaptive zone(primary)+radar+marine; section id stays `weather`, title → "For your location".
- **UI:** primary `.zone` card + supporting `.locgrid` of `.feat` cards with mono `meta` lines; CSS ported from the mock; harness catalog updated. Action buttons right-aligned per mock (fixed after render-gate caught mis-placement).
- **Gates (all test-enforced + green):** mapping-completeness (DoD #5), referential-integrity (mapped id ∈ geometry), radar-coverage (161 bboxes), resolver unit tests, adaptive-section tests, app-level production-path mount test. `pnpm typecheck` + full `pnpm vitest run` + `pnpm build` green. **WebKitGTK render gate PASSED** at 1920×1080, grid CN87uo → "City of Seattle · WAZ315 · WA_ZON_SEA" (matches mock). Render PNG: `dev/scratch/location-hero-mock/build-coastal-1920-v2.png` (gitignored, openable in VS Code).
- Final independent review (opus): no Critical/Important code issues; one framing item (below). Applied its cleanups (dropped dead `bestStateForecast`; clamped 2 out-of-range AK radar bboxes).

## OPERATOR DECISION PENDING (does not block review)

The **primary zone card resolves for 8 states only — WA, OR, ME, NJ, VT, NH, DE, AK** — because the bundled Winlink catalog carries *per-public-zone* forecasts only for those 8. The other 41 catalog states offer only multi-zone NWS-office-area ZFPs; mapping one to a single zone would violate DoD #1 ("the exact zone"), so the zone card correctly **omits** there (those stay in Browse, unmapped-by-design). **This is an upstream catalog limit, not unfinished work** — DoD #5's completeness gate accounts for every catalog filename. **Radar (nationwide) + marine (coasts) resolve everywhere**, so every US operator's hero shows at least radar; it's never empty within the US. Confirm 8-state zone coverage is acceptable for alpha. (Spec DoD framing already corrected to say this.)

## State

- **Branch state:** `bd-tuxlink-loc/location-hero` pushed, up to date with origin, PR #587 OPEN (ready, not draft). Awaiting review/merge.
- **Working tree (worktree):** clean except gitignored `node_modules` symlink + `dev/scratch/` artifacts (render PNGs, PR-body file, request-geo raw/geom caches). The geom/raw caches in `dev/scratch/request-geo/` are gitignored and local-only (regenerate via `pnpm tsx scripts/build-request-geo.ts`).
- **No worktree disposal needed yet** — the worktree is the live PR branch; dispose after merge via the ADR 0009 ritual.
- **Concurrent session note:** another live session (`bd-tuxlink-gife/u3-station-map-ui`) was running its own full vitest during this session — left untouched.

## Pending / follow-ups (bd filed)

- tuxlink-96lu: **close on merge** of #587 (+ after operator confirms 8-state coverage).
- New bd: dynamic "Browse all <ST> local forecasts · N zones" reveal label (spec line 138; shipped static). P3.
- New bd: antimeridian split for far-western Aleutian radar (AK bboxes clamped; mainland resolves). P4.
