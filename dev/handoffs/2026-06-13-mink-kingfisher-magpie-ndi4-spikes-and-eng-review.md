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

## UPDATE — build-robust-features adrev DONE (Codex unavailable → self-adrev exception)

Continued into build-robust-features this session. Brainstorm/plan/eng-review were already done; the remaining gate was the cross-provider adversarial review. **Codex was unavailable** (operator authorized a one-time self-adrev exception to the no-carveout rule). Ran a **3-agent diverse-lens self-adrev** (Rust-serving / React-lifecycle / style-offline-distribution) — it found **2 locked-decision reversals + 4 P0 gaps**, all folded into the plan's new **SELF-ADREV HARDENING** section (`docs/superpowers/plans/2026-06-13-vector-basemap-maplibre-swap.md`, commit `d6422d51`):

- **A1 FLIP L3:** serve PMTiles via **HTTP-206 `Range` on the existing `tile://` scheme** (wry 0.55.1 verified to forward Range + emit 206) + pmtiles `FetchSource` — off the serialized IPC pump, no custom JS Source. The R1 IPC-Source path is now the **proven fallback**, not primary.
- **A2 strangler-fig phasing** (both renderers transient, per-consumer flip, leaflet removed last) — the eng-review "one atomic phase" was unbuildable on the CI-only loop.
- **A6/L7 pin `maplibre-gl@5` + `@protomaps/basemaps@5` + `pmtiles@4`** — the spikes ran maplibre 4.7.1 (directional; re-validate the seam on v5 in phase 1).
- **A8 offline glyphs** (Noto Sans `.pbf` from `protomaps/basemaps-assets`, local glyph-serving path ≠ `pmtiles_read_range`) — labels silently 404 otherwise. **A9 CSP** `worker-src 'self' blob:` — blank map on WebKitGTK otherwise. **A7** dark sprite + non-slot color pass. **A10** R3 = the real 13-id Protomaps v4 schema. Plus A3 lock-free `read_at`, A13 packaged-`.deb` marker-CSP grim spike, A14 imperative `createMapLibreMock` test double, A4/A5/A11/A12/A15–A18.

**DEFERRED OPERATOR DECISIONS (do not self-decide):**
- **D1 (phase 4) — catalog-pack hosting/provenance.** Protomaps ships only a ~120 GB planet, not region packs; tuxlink must build + host packs. Operator decides where/budget/cadence + pins ONE planet build hash for bundle+packs.
- **D2 (phase 3) — baked-dark aesthetic re-approval.** Operator approved the CSS-filter look; the baked per-slot path differs at translucent overlaps/halos and is unproven with labels. Re-approve from a real labeled render before phase 4.

## NEXT SESSION — phase 1 implementation (TDD, on the corrected approach)

Read `docs/superpowers/plans/2026-06-13-vector-basemap-maplibre-swap.md` FIRST — esp. the **SELF-ADREV HARDENING** section (it supersedes parts of the earlier phasing). Phase 1:
- Pin deps (L7/A6); amend CSP (A9); bundle world z0–6 PMTiles + glyphs + light/dark sprites (A8/A12 build script).
- Rust: a **`tile://pmtiles/<archive>` 206-Range handler branch** in `lib.rs` (A1, NOT through `serve_tile`), lock-free `read_at` on a shared `Arc<File>` (A3); PMTiles-v3 + 13-id-v4-schema validation (A4/A10). TDD with Rust unit tests (CI-compiled — no cold cargo on the contended Pi, `feedback_no_cold_cargo_on_contended_pi`).
- Build the `createMapLibreMock` test double (A14, global in `test-setup.ts`) before the React swap.
- Render bundled z0–6 **light** via the 206 path; re-validate the seam on maplibre@5.
- Run the **packaged-`.deb` marker-CSP grim spike** (A13) before committing to `maplibregl.Marker`.
- WebKitGTK validation must set `WEBKIT_DISABLE_DMABUF_RENDERER=1`. Wire-walk before any "done". CI (clippy --all-targets + full vitest) is the merge gate; smoke post-merge.
- Spike artifacts + the proven IpcSource pattern + `xformHex` reference: `dev/scratch/ndi4-spikes/` (gitignored).
