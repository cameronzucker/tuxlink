# Handoff — mink-yew-osprey — APRS WX shipped (disappointing), cn84 reverted, MapLibre is the real problem

**Agent:** mink-yew-osprey · **Dates:** 2026-06-19 → 2026-06-20
**Headline for next session:** **Evaluate map-engine alternatives to MapLibre (tuxlink-ez7t)** — start-to-finish, office-hours → eng-review → decision doc. Everything else map-UI is downstream of it.

## TL;DR

A very long session. Net result:
- **ni5b (on-map WX overlay) shipped to main** — but the operator's verdict on the rendered result is **"not remotely what I approved in the mocks."** The badge shipped as plain GL text, not the chip design. See **tuxlink-s587**.
- **cn84 (animated digipeat path) was merged, then fully reverted.** The map-pegging CPU bug was NOT the animation — it was **`preserveDrawingBuffer: true`** (which I added in ni5b for PNG export) strangling the tile pipeline on llvmpipe. A parallel agent owns that fix.
- **The real, strategic conclusion: MapLibre is the wrong engine for this edge-compute situational display.** The operator wants alternatives explored. This is the next session's main job.
- A **fade-based trace-line rework is drafted (PR #838, tuxlink-k0zz)** but is now arguably moot pending the engine decision — do NOT invest more in MapLibre map UI until ez7t is decided.

## What merged to main this session

- **ni5b — WX overlay + category filter** (PR #831). Joins heard WX (`useEnvStations`) with positions; renders a temperature-led badge (honest: temp-led, condition glyph only from real fields, never an assumed ☀), a hover card, click → Station Data, a generic category filter ("weather mode"), an optional PNG snapshot. **Functionally wired, but visually far below the approved mocks (s587).**
- **wb4c — WX control positions** (PR #837). The filter/export/hover-card were rendering behind the recenter/nav/scale controls; repositioned into clear space. CSS-only.
- **cn84 REVERT** (PR #834). Removed `DigipeatPathLayer` + `digipeatPath.ts` + `pathTrace.ts`. **Kept** the inert backend via-chain (`decode_digi_hbits`, `InboundPos.via`) + frontend via plumbing for a future rework.
- (Decoder fix #832 / tuxlink-j5cj, by a parallel agent: lifted the AX.25 path cap 2→8 digis, so multi-hop paths now decode — relevant to any future trace work.)

## In flight (DO NOT merge without thought)

- **PR #838 — tuxlink-k0zz — fade-based trace-line rework** (draft, worktree `worktrees/bd-tuxlink-k0zz-trace-fade-rework`). Restores `resolveDigipeatPath` (honest multi-hop) + a NEW `DigipeatFadeLayer` that fades paths via bounded maplibre `line-opacity` transitions — **no rAF, no per-frame setData, no preserveDrawingBuffer** (Codex-confirmed). Codex-reviewed + 5 lifecycle bugs fixed. 37 aprs tests green. **It was never operator-smoked, and it's MapLibre — if the engine decision (ez7t) is to switch, this gets rebuilt in the new engine, not merged.** If the decision is to stay on MapLibre, it's a reasonable, smoke-then-merge candidate.

## The two substantive threads for the next session

### 1. tuxlink-ez7t — Map-engine evaluation (HEADLINE, do this first)
MapLibre is a GPU vector engine on a CPU rasterizer (llvmpipe). It has fought this project repeatedly: the maxBounds crash (rwo6), continuous perf gymnastics (vnk7/gq0d, `fadeDuration:0`, feature-state-not-FC-rebuild), the cn84 animation peg, the `preserveDrawingBuffer` tile strangle, and styled-chip overlays being awkward (the s587 badge). **The APRS map is a local-area situational display, not a general slippy map.** Evaluate: (a) **Leaflet** (DOM/canvas, no WebGL — the project used react-leaflet before ndi4; establish *why* it was dropped and whether that still outweighs the Pi pain); (b) a **purpose-built 2D `<canvas>` situational renderer** (basemap image + pins + fade lines + styled chips — trivial in 2D canvas). NOT a from-scratch general map engine. Cost spans all maps (StationFinderMap + AprsPositionsMap), offline tiles, migration. Run it as **office-hours → plan-eng-review → decision doc.** This decision **gates** s587.

### 2. tuxlink-s587 — WX display fidelity (gated on ez7t)
The shipped WX badge is a maplibre text symbol, not the approved treatment-A chip (rounded dark bg, border, colored temp, condition glyph). Rebuild to mock fidelity — HTML overlay chips on MapLibre, OR natively in the chosen new engine. **Re-audit the WHOLE WX display** (hover card, filter, export, click→Station Data) against the approved mocks. The approved brainstorm mocks are at `worktrees/bd-tuxlink-cn84-aprs-animated-path/.superpowers/brainstorm/15301-1781893173/content/*.html` (wx-mode-treatment.html = the badge treatments; path-render-approach / animation-trigger / trace-feel = the cn84 trace design).

**Process lesson (worth internalizing):** when a high-fidelity mock can't be faithfully rendered in the chosen tech, **flag the gap at implementation time** — never silently downgrade to what the layer does natively. That + the unverifiable-on-llvmpipe problem (agent has no software-GL to smoke) are why this session produced disappointing map UI.

## Branch / working-tree state

- Main has: ni5b, wb4c, cn84-revert, the decoder fix. cn84 animation is GONE from main (the via-chain backend remains, inert).
- This handoff is committed on `bd-tuxlink-xsn9/session-handoff-0620` off main (per the no-PR-for-handoffs rule). FF to main when convenient so the session-start hook surfaces it.

## In-flight worktrees + disposal owed (ADR 0009)

Several merged-dead / superseded worktrees need disposal (gitignored `node_modules/`, `target/`, `dev/scratch/`, `.beads/embeddeddolt/`, and `.superpowers/` mocks on disk):
- `worktrees/bd-tuxlink-ni5b-wx-overlay` — MERGED-DEAD.
- `worktrees/bd-tuxlink-cn84-aprs-animated-path` — MERGED then reverted; **holds the approved WX + cn84 mocks in `.superpowers/`** (referenced by s587/ez7t) — archive the mocks before disposing.
- `worktrees/bd-tuxlink-2xhe-revert-cn84-anim`, `worktrees/bd-tuxlink-wb4c-wx-control-positions` — MERGED-DEAD.
- `worktrees/bd-tuxlink-xl4d-aprs-path-anim-rework` — superseded/closed (line-trim approach, obsolete).
- `worktrees/bd-tuxlink-k0zz-trace-fade-rework` — KEEP until the ez7t decision (holds PR #838).

## bd issues
- **tuxlink-ez7t** (P1, open) — map-engine evaluation. **Start here.**
- **tuxlink-s587** (P1, open, blocked-by ez7t) — WX display fidelity rebuild.
- **tuxlink-k0zz** (in_progress) — fade trace rework (PR #838 draft).
- Closed this session: ni5b, cn84, wb4c, 2xhe (revert), xl4d (superseded).
