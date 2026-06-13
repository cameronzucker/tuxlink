# Handoff — mink-kingfisher-magpie — tuxlink-ndi4 gating spikes + eng-review done

**Date:** 2026-06-13
**Agent:** mink-kingfisher-magpie
**bd issue:** tuxlink-ndi4 (in_progress)
**Build worktree:** `worktrees/bd-tuxlink-ndi4-vector-basemap-build`
**Branch:** `bd-tuxlink-ndi4/vector-basemap-build` (off `main`), pushed, **no PR yet**.

---

## What this session did (continuing tuxlink-ndi4 — self-hosted vector OSM basemap)

1. **Merged PR #666** (the APPROVED design doc, off main; CI-green, docs-only, merge commit `3577962d`).
2. **Disposed** the old design worktree `worktrees/bd-tuxlink-ndi4-vector-basemap-design` per ADR 0009 (inventory clean; design content safe on main; dead branch deleted local+remote).
3. **Created the build worktree** off updated `main` via `new_tuxlink_worktree.py` (claims ndi4).
4. **Ran the two GATING SPIKES** on the Pi's real WebKitGTK 4.1 engine, via a `python3-gi` + `WebKit2 4.1` harness (NO Tauri cold build needed — same `libwebkit2gtk-4.1` engine; `dev/scratch/ndi4-spikes/`, gitignored):
   - **R1 (PASS):** pmtiles custom `Source.getBytes()` over a JS↔native byte-range bridge (same IPC class as Tauri `invoke`) → `addProtocol` → MapLibre rendered real tiles. Use Tauri v2 **raw-bytes** `Response`, not base64.
   - **R4 (premise-correcting):** the meshmap **CSS invert filter is too slow** on the Pi (~15 fps during pan/zoom; even bare `invert(1)` ~26 fps; GPU-layer hint doesn't help — it's a CPU per-pixel pass on the `WEBKIT_DISABLE_DMABUF_RENDERER=1` path the app requires). The first 0%-overhead reading was on the broken DMA-BUF path ("TV static" — operator caught it; canonical fix is `src-tauri/src/lib.rs:66`, bd tuxlink-wfw). **Operator decided: bake the inverted look into a GL-native dark style** (invert + W3C hue-rotate(180) + brightness(1.33) on layer colors at build time). PROVEN: baked-GL-dark = 45 fps = light baseline, zero filter cost.
5. **Amended the design doc** (`docs/design/2026-06-13-self-hosted-vector-osm-basemap-design.md` — AMENDMENT block at top) + memory (`project_webkitgtk_css_filter_cost`). Commit `a8c3eb89`, pushed.
6. **Ran /plan-eng-review** → architecture-lock plan `docs/superpowers/plans/2026-06-13-vector-basemap-maplibre-swap.md`. Commit `7d488388`, pushed. Locked **L1 = raw `maplibre-gl` + a thin owned hook layer (NOT react-map-gl)** — operator deferred to eng judgment; reasons in the plan. Ran an **outside-voice** challenge (Claude subagent — Codex deliberately preserved for the build adrev): 10 findings, **9 fixed** (real blast radius incl. `MapTileSourceSettings`/`PositionPickerOverlay`/`TileStatusPill`/all tests; collapsed the non-separable basemap+overlay swap into ONE atomic phase; Flavor-slot dark builder not a hex regex; fractional-zoom literal remap; IpcSource concurrency spike; bundled-resource seekable-path; drag-select interaction rewrite; ODbL attribution to first-render), **1 rejected** (region-pack split — conflicts with the alpha-completeness bar; packs stay in scope).

## State / working tree

- Branch pushed, 2 commits ahead of where it branched (design amendment + plan). No PR opened (build hasn't started).
- `node_modules` installed in the build worktree (needed for the pre-push link-linter; pnpm hard-linked, fast).
- **Gitignored spike artifacts** in `dev/scratch/ndi4-spikes/` (throwaway, local-only): `harness.html`, `webkit_harness.py`, `vendor/` (maplibre-gl 4.7.1, pmtiles 4.3.0), `sample.pmtiles` (6.6 MB firenze, Protomaps schema), `SPIKE-FINDINGS.md`. Re-runnable: `WEBKIT_DISABLE_DMABUF_RENDERER=1 python3 dev/scratch/ndi4-spikes/webkit_harness.py`.
- No other in-flight worktrees created/owned by this session. The old `bd-tuxlink-ndi4-vector-basemap-design` worktree was disposed.

## NEXT SESSION — build-robust-features (the actual implementation)

The architecture is LOCKED and outside-voice-validated. The plan doc is the spec input. Build the 6-phase MapLibre swap via **build-robust-features WITH the mandatory cross-provider Codex adversarial review** (no-carveout rule — `feedback_no_carveout_on_cross_provider_adrev`). Critical reminders:

- **Read the plan FIRST** (`docs/superpowers/plans/2026-06-13-vector-basemap-maplibre-swap.md`) — esp. the REVISED phasing (phase 0 grep the real file list; phases 2+3 are ONE atomic phase; dark builder targets `@protomaps/basemaps` Flavor color slots; zoom literals remap to z0–14).
- **Dark mode is BAKED GL style, not a CSS filter** (design AMENDMENT). Any WebKitGTK validation must set `WEBKIT_DISABLE_DMABUF_RENDERER=1` (the app does; standalone harnesses must too — `project_webkitgtk_css_filter_cost`).
- **Preserve Codex quota** for the build adrev (don't burn it on side errands; `feedback_codex_quota_gotcha`).
- Wire-walk gate before any "done" claim; CI (clippy --all-targets + full vitest) is the merge gate; smoke opportunistically post-merge.
