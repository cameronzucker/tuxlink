# Handoff — location hero shipped (PR #587) + Mercator LAN tiles built (PR #606)

**Agent:** gully-cardinal-gulch · **Date:** 2026-06-11 · Two features in flight.

## One-line

Shipped the location-aware Request Center hero (PR #587, ready, awaiting review + one coverage decision), then investigated a map-tile bug → corrected a wrong spec → designed → built the EPSG:3857 Web-Mercator LAN-tiles re-architecture (PR #606, MERGEABLE, CI running, operator smokes pending).

## PR #587 — location-aware "For your location" hero (tuxlink-96lu)

- **State:** OPEN, ready (not draft), branch `bd-tuxlink-loc/location-hero`, worktree `worktrees/bd-tuxlink-loc-location-hero/`. All gates green, WebKitGTK render-verified (1920×1080, CN87uo → "City of Seattle"). Awaiting your review/merge.
- **Built:** 216 vetted NWS-zone→Winlink-filename mappings + 136 unmapped-by-design (completeness + referential-integrity gates), 161 radar bboxes, geometry pruned 4.7 MB→552 KB; adaptive zone/radar/marine section + primary zone-card UI.
- **⚠️ OPERATOR DECISION (in PR body + bd tuxlink-96lu):** the primary zone card resolves for **8 states** (WA/OR/ME/NJ/VT/NH/DE/AK) — the Winlink catalog only carries per-public-zone forecasts for those; the other 41 states have only office-area ZFPs (multi-zone), so the zone card correctly omits there (radar nationwide + marine on coasts still resolve). Upstream catalog limit, not unfinished work. Confirm 8-state coverage is OK for alpha.

## PR #606 — Web Mercator (EPSG:3857) LAN tiles (tuxlink-7h2m)

- **State:** OPEN, **MERGEABLE** (main conflicts in StationFinderMap resolved 2026-06-11 — union of main's RecenterOnOperator/OPERATOR_ZOOM + this branch's tileSource). Branch `bd-tuxlink-7h2m/mercator-lan-tiles`, worktree `worktrees/bd-tuxlink-7h2m-mercator-lan-tiles/`. CI (build-linux + verify) running on the merge commit; local full CI-parity was green (cargo test, clippy --all-targets, tsc, vitest 2380, build, lint:docs).
- **Why it exists:** you corrected the spec — "never public OSM" means no public-server *abuse*, NOT "refuse Web Mercator / no network ingestion." A self-hosted LAN server speaking standard 3857 (Geographica) is the intended case. Memory `project_lan_tiles_mercator_ok` captures this; the dyop 4326-only spec was superseded.
- **Built (10 tasks + cleanup):** coord.rs x-bound → WebMercatorQuad (2^z); deleted the CRS gate (crs.rs + Crs enum/field) — the LAN/SSRF host gate is the only control (final review verified host.rs is 0-diff, gate intact); BaseMap → L.CRS.EPSG3857 + bundled Mercator base raster (Natural Earth → gdalwarp, 1.5 MB) + RASTER_MAX_ZOOM 3 + onZoomChange bridge; `useTileSource` hook wired into all 4 map panes (absorbs tuxlink-n6xu + tuxlink-24px); 6-char gate reads live zoom; deleted the orphaned equirect asset.
- **⚠️ OPERATOR SMOKE PENDING (GUI — agent can't run; in PR body):**
  1. **grim render gate @1920×1080** (memory `grim_realapp_validation_pandora`): Mercator base renders without plate-carrée stretch, grid sits right, pan stays ±85.0511°, zoom caps at 3 with no source. (Base is a 60-color palette to fit <1.5 MB — eyeball low-zoom fidelity.)
  2. **Geographica live config:** Settings → Map tile source → `http://localhost:8090/styles/darkmatter/{z}/{x}/{y}.png`, XYZ → expect `lan-live`, tiles render, zoom unlocks past 3; confirm a **public** URL is still rejected.

## State / worktrees

- **Two active worktrees** (both bd-claimed, both with open PRs): `bd-tuxlink-loc-location-hero` (#587), `bd-tuxlink-7h2m-mercator-lan-tiles` (#606). node_modules symlinked in both. Dispose via ADR 0009 ritual after their PRs merge.
- **Gitignored-but-on-disk:** `dev/scratch/location-hero-mock/*.png` (render PNGs), `dev/scratch/request-geo/{raw,geom}/` (NWS fetch caches — regenerable via `pnpm tsx scripts/build-request-geo.ts`), `dev/scratch/*-pr-body.md`.
- **Concurrent session note:** a `bd-tuxlink-gife/u3-station-map-ui` session was running its own tests during this session — untouched.
- **bd:** 96lu (PR #587, reopened from a wrong grounding-close), 7h2m (PR #606), n6xu + 24px (absorbed into #606, close on merge). Follow-ups filed: dynamic "Browse all <ST> · N zones" label (P3), antimeridian Aleutian radar (P4).

## Next-session pending decisions
1. Review/merge #587 + confirm the 8-state zone coverage.
2. Run the two #606 operator smokes, then merge #606 (after CI green) + close n6xu/24px.
3. Dispose both worktrees post-merge.
