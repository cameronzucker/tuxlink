# Handoff — plover-bison-delta — map-engine decided: Leaflet + protomaps-leaflet (ez7t)

**Agent:** plover-bison-delta · **Date:** 2026-06-20
**Headline for next session:** ez7t is DECIDED and PR'd. **Start the build-out (tuxlink-6kdw) — Phase 1, behind a `.github/RELEASE_FREEZE`.** Merge PR #843 first.

## TL;DR

- **tuxlink-ez7t resolved:** adopt **Leaflet + protomaps-leaflet**, retain the vector PMTiles backend unchanged. Evidence-backed (spike measured on this Pi), not assumed.
- **Decision doc:** `docs/design/2026-06-20-map-engine-leaflet-decision.md` → **PR #843** (open, approved by operator). Merging it lands the doc AND this handoff on main.
- **plan-eng-review (partial)** locked the phasing; build-out is **tuxlink-6kdw**.
- Do **not** rebuild any map UI in MapLibre.

## What shipped this session

- Decision doc committed off main in worktree `worktrees/bd-tuxlink-ez7t-map-engine-decision` (branch `bd-tuxlink-ez7t/map-engine-decision`) → PR #843.
- bd: **ez7t** updated with decision + evidence + eng-review outcome; **tuxlink-6kdw** created (migration Phase 1, depends on ez7t, blocks s587).
- Auto-memory saved: `project_webkitgtk_dmabuf_static_and_webgl_blank`, `project_aprs_map_is_unbounded_situational` (+ index).

## The evidence (measured, real WebKitGTK on pandora, quiet Pi, `WEBKIT_DISABLE_DMABUF_RENDERER=1`)

- **Vector render confirmed:** protomaps-leaflet@5 `flavor:'dark'` renders the project's real PMTiles (`phoenix.pmtiles`) as a correct dark street grid. No raster. Disk-efficient vector stack preserved.
- **~100ms** first paint · **63fps** drag (p95 25ms) · **56fps** animated fade-trace (proper single-canvas — the cn84/k0zz case) · **~1%** idle CPU. Animation holds ~0.8 core *while playing*, idles to ~1%.
- **No valid MapLibre head-to-head:** the MapLibre twin would not composite WebGL in a bare harness on this host (catch-22: DMABUF on → "TV static"; DMABUF off → `glReadnPixelsRobustANGLE` readback fails → blank). So the case is **robustness + capability + good perf, NOT proven-faster.** See the dmabuf/webgl memory before re-measuring anything in WebKitGTK.

## Decisions locked / open (eng-review)

- **✅ Phasing (locked):** strangler-fig — **AprsPositionsMap first** — all behind a `.github/RELEASE_FREEZE` so release-please's nightly auto-merge can't ship a half-migrated map. adrev + wire-walk each seam. Lifts as one clean "big bang" to users when the full migration lands + validates.
- **↩️ World+pack composite (retracted):** main runs effectively **one data source** — bundled overview (z0–6) + the `continent-na` pack (z0–14, full-range; overview is the no-pack fallback). Render the pack as one protomaps-leaflet layer + overview as a trivial fallback layer. No go/no-go composite risk (an earlier overstatement).
- **◻️ Vendoring (open, recommended):** vendor protomaps-leaflet (~120KB MIT) per ADR-0011 fork-and-own — an offline/EmComm-critical path shouldn't ride a maintenance-mode npm dep. Confirm at Phase 1 based on how much you patch it.
- **Deferred:** code-quality / test-coverage / perf review sections — do them next session against actual Phase-1 code (none exists yet; reviewing in a saturated context produced stale-architecture noise, so I stopped).

## The spike harness (LOCAL, gitignored — won't survive a checkout clean)

`dev/scratch/leaflet-spike/` on pandora (gitignored per CLAUDE.md; NOT in any commit). Reusable for Phase 1. Contents:
- `index.html` (Leaflet twin), `maplibre.html` (MapLibre twin — renders blank in bare harness, see memory), `leaflet-anim.html` (56fps animated-trace test), `serve.py` (range-capable static server), `run_webkit.py` (PyGObject WebKit2 4.1 launcher; pass `fs` arg for fullscreen), `measure.sh` (load-gated CPU/fps capture), `lib/` (leaflet, protomaps-leaflet@5, pmtiles, maplibre-gl, protomaps-basemaps.mjs), `phoenix.pmtiles` + `az-state.pmtiles` (sample packs), `DECISION-DOC.md` (draft source of the committed doc).
- **Always** launch WebKitGTK with `WEBKIT_DISABLE_DMABUF_RENDERER=1`, else "TV static". `get_snapshot` + event-metrics CANNOT verify a WebGL render — trust grim of a live foreground window / operator eyes.

## Branch / worktree state

- Session ran on `bd-tuxlink-xygm/recover-handoffs` (the main checkout) — **stale** (no `src/map`, missing ndi4 docs). Do not build map code there.
- `worktrees/bd-tuxlink-ez7t-map-engine-decision` — branch `bd-tuxlink-ez7t/map-engine-decision`, off main, **PR #843 open**, has `node_modules` installed (for the docs-link pre-push hook). This handoff + the decision doc are its only tracked content.
- ez7t worktree disposal is NOT yet due (PR #843 open). After #843 merges, dispose per ADR-0009.

## Next session — build-out (tuxlink-6kdw)

1. **Merge PR #843** (lands decision doc + this handoff on main; start-hook then surfaces this).
2. **Set `.github/RELEASE_FREEZE`** before any migration code merges.
3. Worktree off main for tuxlink-6kdw. **Phase 1:** vendor protomaps-leaflet → build the Leaflet substrate (replace `MapLibreMap`/`mapHooks`) → migrate **AprsPositionsMap** → prove overview+pack layers render against the real `tile://` Rust seam + packaged CSP **in the Tauri app** (Leaflet IS agent-validatable via grim). adrev + wire-walk the seam.
4. Then surfaces 2–5 (StationFinderMap, LocationMap, compose picker, GridPickerOverlay + Maidenhead grid), each adrev + wire-walk.
5. Downstream once map is on Leaflet: **s587** (WX badge as `divIcon` chips to the approved mocks; re-audit whole WX display) and **k0zz** (rebuild fade trace as the single-canvas overlay — the 56fps approach; **close PR #838**, do not merge MapLibre code).
6. **Lift the release freeze** when all surfaces land + validate.

Approved mocks for s587/k0zz: `worktrees/bd-tuxlink-cn84-aprs-animated-path/.superpowers/brainstorm/*/content/` (archive before disposing that worktree).
