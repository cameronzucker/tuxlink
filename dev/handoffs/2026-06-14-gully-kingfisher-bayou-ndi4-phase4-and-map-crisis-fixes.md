# Handoff — tuxlink-ndi4 Phase 4 shipped + the "map won't display" crisis root-caused & fixed

**Agent:** gully-kingfisher-bayou · **Date:** 2026-06-14
**Session arc:** started on ndi4 Phase 4 (region packs); pivoted to a live operator crisis
("map cannot be displayed" → then blank map). Root-caused both via real-WebKitGTK repro.

## TL;DR for the next session
1. **MERGE PR #693 when CI is green** (`gh pr merge 693 --merge --delete-branch`). It's the
   blank-map fix (CORS + sprite). The local-git post-merge step errors because the main checkout
   is held by another session — that's harmless; verify with `gh pr view 693 --json state`.
2. **Then the operator re-runs `bash scripts/converge-build.sh`** (incremental now — see Build deps)
   to confirm the world basemap renders in Find a Station.
3. **Deferred hard gate:** the Codex cross-provider adrev still owes a run on the merged map-fix
   diffs (#689 + #693) — quota-blocked this session. Run `codex review` per the no-carveout rule.

## What shipped to main this session (all CI-green both arches)
- **PR #685** — ndi4 Phase 4: D1 spec doc, bundled world z0-6 + glyphs/sprites wired (render fix),
  region manifest + `planet_url` SSRF allowlist, bbox math, R5 download/validate/atomic-install,
  Tauri commands + go-pmtiles sidecar (CI-only `externalBin` in release.yml), pack-manager UI
  (Tools→Settings→Offline maps), R7 compositing.
- **PR #688** — self-adrev fixes: **P0** opaque-`background` pack layer (filtered out), gzip-bomb
  cap in validate.rs, concurrent-download manifest lock, manifest fetch timeout, honest SSRF doc,
  D1 doc A11→unclamped-overview correction.
- **PR #689** — **map crash fix**: dropped `maxBounds`. maplibre-gl **5.24.0** crashes on
  construction when any camera-bounds constraint is set (`_calcMatrices` null `n[0]`), on this
  WebKitGTK/ANGLE context. That throw was what PR #686's ErrorBoundary surfaced as "map cannot be
  displayed on this system." (#686, by another agent, is the *blank-screen* fix — NOT this work.)

## In flight (NOT yet merged)
- **PR #693 (tuxlink-56ki)** — **blank-map fix** (CI pending at handoff). Two bugs, both found by
  grim'ing the live devtools console:
  1. `tile://pmtiles/world` served 206 but had **no `Access-Control-Allow-Origin`** → maplibre's
     `fetch()` CORS-blocked it → no vector tiles → blank. Added ACAO:* + Expose-Headers to the
     pmtiles response + error paths in `src-tauri/src/lib.rs`. (Old Leaflet used `<img>`, never
     hit CORS; the maplibre swap to fetch() exposed it. First real end-to-end use of the tile://
     206 seam.)
  2. maplibre v5 rejects **root-relative sprite/glyphs URLs** (`'/basemap/sprites/dark' must be
     absolute`) → killed icons/labels. `src/map/basemapStyle.ts` now origin-prefixes both
     (`selfOrigin()`); tests in basemapStyle.test.ts + MapLibreMap.test.tsx updated.

## Critical facts / do-not-relitigate
- **0.61.0 is the BROKEN build** — both map bugs are in it; the fixes are unreleased. `git tag
  --contains <fix-sha>` is empty. Don't re-test 0.61.0 expecting the map; test a build from main
  (converge or the next pre-release ~0.62.0 via `gh workflow run release-merge.yml` — operator-only).
- **The diagnosis method that worked:** reproduce in the REAL WebKit2GTK 4.1 engine (PyGObject),
  not Chromium. Probe harness preserved (gitignored): `dev/scratch/webgl-probe.py`,
  `dev/scratch/csprepro/`, `dev/scratch/maplibre-repro*`. WebGL1+WebGL2 both work here — the
  failures were maxBounds + CORS + sprite-URL, NOT WebGL availability.

## Build-environment changes made on the Pi (sudo, operator-approved)
The mg4s image-attachment feature added native build deps the dev Pi lacked (CI had them). Installed:
`libheif-dev`, `libde265-dev`, `clang-19`, `libclang-common-19-dev`. Local source builds now
compile. **Worth documenting as a dev-setup prereq** (CI installs libheif/libde265/libwebp; the
clang resource headers are needed for libheif-sys's bindgen). The converge cargo cache is now WARM,
so the next converge build is incremental (~5-10 min), not the 30+ min cold build.

## Open follow-ups (bd issues filed)
- **tuxlink-rwo6** (P0, the maxBounds bug — fix merged): follow-up = **restore a pan-constraint**
  (pin/upgrade maplibre off the broken 5.24.0, or a manual `moveend` center-clamp). Dropping
  maxBounds means the user can pan slightly past the world edges (cosmetic).
- **P3 polish issue** (filed): overview maxzoom translucent-bleed, resolve_sidecar PATH-fallback
  hardening in packaged builds, missing-sidecar UX, first-paint flash, manifest-load-error UX.

## Worktrees (ADR 0009 — all bd-claimed; dispose when their PRs are merged)
- `bd-tuxlink-56ki-basemap-cors-sprite` — ACTIVE (PR #693, merge then dispose).
- `bd-tuxlink-rwo6-map-maxbounds-crash`, `bd-tuxlink-ndi4-ndi4-phase4-packs`,
  `bd-tuxlink-ndi4-ndi4-phase4-adrev-fixes` — merged-dead, dispose per the ritual. Each has its own
  `node_modules/`, cargo `target/`, and `dev/scratch/` (gitignored — incl. the repro harness +
  go-pmtiles binary in the phase4-packs/old-ndi4 scratch).
