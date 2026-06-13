# Handoff — mink-shoal-maple — self-hosted vector basemap DESIGN approved (tuxlink-ndi4)

**Date:** 2026-06-13
**Agent:** mink-shoal-maple
**Branch (main checkout):** `bd-tuxlink-xygm/recover-handoffs`
**Type:** feature DESIGN session (office-hours). No code written (design-only by gate).

---

## What this session did

Drove the office-hours/brainstorming design for **tuxlink-ndi4** — tuxlink's own
self-hosted dark/light OSM vector basemap (meshmap.net look, no cross-service runtime
dependency). Output is an APPROVED design doc; build is the next phase.

**Canonical artifact:** `docs/design/2026-06-13-self-hosted-vector-osm-basemap-design.md`
(Status: APPROVED). bd `tuxlink-ndi4` notes updated with the decision summary.

## Two premise reversals (the session's real value — both from primary sources)

1. **meshmap.net is NOT MapLibre+vector.** Verified against `brianshea2/meshmap.net`
   source + the live site: meshmap is **Leaflet + RASTER OSM tiles + one CSS rule**
   `filter: invert(1) hue-rotate(180deg) brightness(1.33)`. The handoff's "confirmed
   fact #3" (meshmap = MapLibre + dark vector style) was wrong. Consequence: the dark
   look is a renderer-agnostic CSS filter (free); vector is justified by **storage** for
   offline, not by the look.
2. **protomaps-leaflet is in upstream maintenance mode.** Per Protomaps' own docs:
   "in maintenance mode," "recommended only for legacy projects … otherwise use MapLibre
   GL JS," "designed for non-interactive layers." The first renderer pick
   (protomaps-leaflet) was reversed → **MapLibre GL JS**.

## Approved decisions (do NOT re-litigate; operator-confirmed this session)

- **Renderer:** MapLibre GL JS (full swap from the current Leaflet stack).
- **Format/serving:** PMTiles via the `pmtiles` protocol. **Primary** = Tauri IPC
  byte-range Source (local file reads). `tile://` 206/Range serving is an *optimization
  spike*, not a prerequisite (the current `tile://` handler has no Range support).
- **Coverage:** bundle world **z0–6** (~30–60 MB, never-blank offline) + downloadable
  **permanent** per-region packs **z0–14** (~hundreds of MB) via a **curated catalog**
  plus a schema-validated custom-URL option. Full-planet (~120 GB) is **out of scope**.
- **Styles:** light = `@protomaps/basemaps` `namedFlavor('light')`; dark = same style +
  the meshmap CSS invert filter on `.maplibregl-canvas` (in-canvas labels invert with
  the map — intended; DOM pins/UI do not). Three view modes: light vector / dark vector
  / imagery-hybrid (geographica's pattern).
- **#659 raster basemap:** retired *as the basemap*. Whether its raster transport
  (`tile://` + SSRF/host pinning + cache + breaker) is **retained+generalized for
  imagery** or **deleted** is an explicit eng-review call, tied to the imagery transport
  choice (see doc §Imagery extension).
- **Imagery overlay** (operator-requested): satellite from the local Geographica
  tileserver, as a MapLibre raster source, as a follow-up increment. Routed through the
  retained `tile://` transport (recommended, needs `.jpeg`/template generalization) OR a
  direct HTTP source (bypasses safety). Imagery is its own view mode — never under the
  dark filter (inverting a photo looks wrong).

## Gating spikes before any real build (from adversarial review)

- **R1:** wire the `pmtiles` lib's custom Source to a Tauri IPC byte-range command and
  feed MapLibre via `addProtocol`.
- **R4:** WebKitGTK/Pi-5 performance of the full-canvas CSS `invert()` filter over a
  live-repainting MapLibre WebGL canvas. The desktop raster mock proves the aesthetic
  only, NOT this. Tune the canonical filter values here. ("Chromium is not a WebKitGTK
  proxy.")

## Process artifacts

- Visual mock (raster proof-of-aesthetic, light vs dark): `dev/scratch/meshmap-look-mock.html`
  + `.png` (throwaway; proves the look + view-mode concept, not the vector renderer).
- Spec-review: 2 adversarial rounds (6/10 → 8/10; all 14 round-1 findings resolved + 6
  round-2 follow-ups applied). Metrics in `~/.gstack/analytics/spec-review.jsonl`.

## State / cleanup

- **Branch:** `bd-tuxlink-xygm/recover-handoffs`, was in sync with origin (0/0) at start.
- **Code baseline for the build:** `origin/main` (the map subsystem `src/map/` +
  `src-tauri/src/tiles/` and #659 live on main, commit `f3ba5bb9`). They are NOT on the
  `recover-handoffs` working tree (stale branch). **Build tuxlink-ndi4 from a branch off
  `main`, not off recover-handoffs.**
- **How it was committed:** the main-checkout-race hook denied the commit — **3 live
  sessions** were active on `bd-tuxlink-xygm/recover-handoffs` (main checkout). Per
  ADR 0008, routed to a worktree: `worktrees/bd-tuxlink-ndi4-vector-basemap-design`,
  branch `bd-tuxlink-ndi4/vector-basemap-design` (off `main`), bd `tuxlink-ndi4` claimed.
  The **design doc + this handoff** were committed there and pushed as a PR to `main`.
- **The dahlia-spruce-osprey handoff was LEFT untracked in the main checkout** for the
  active recover-handoffs sessions to commit — committing it on my branch would collide
  with theirs. Other untracked files (`dev/tools/`, `docs/design/managed-modem-*`, the
  findstation PNGs, other sessions' handoffs) likewise left for their owners.
- No code, no quality gates needed (design-only).
- **Worktree disposal:** dispose `worktrees/bd-tuxlink-ndi4-vector-basemap-design` per
  ADR 0009 after the PR merges (only the committed docs + gitignored `.beads` state).

## NEXT SESSION

Next step is **engineering review** of the approved architecture (lock data flow, the
R1/R4 spikes, edge cases), then build via build-robust-features with the mandatory
cross-provider Codex adversarial review. The design doc is the spec input.
