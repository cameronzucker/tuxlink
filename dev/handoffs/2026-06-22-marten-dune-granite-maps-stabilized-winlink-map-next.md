# Handoff — marten-dune-granite — Maps stabilized (0.74.1); NEXT: Winlink-on-map feature

**Agent:** marten-dune-granite · **Date:** 2026-06-22
**Headline:** The just-merged Leaflet map migration shipped with three real bugs; all are now **fixed and in v0.74.1**. The operator confirmed 0.74.1 is good. **Next session's directed work is the "Winlink stations + contacts on the map" feature (bd tuxlink-s1o1)** — a UI feature, so it **MUST start with brainstorming**, not code.

---

## What shipped this session (all merged to main, all gated green)

| PR | Fix | In release |
|---|---|---|
| **#864** (tuxlink-u4k2) | LocationMap/GridPicker **whole-app freeze**: the Maidenhead grid layer rendered one DOM marker per cell label; zooming out at Square level made 10k–130k DOM nodes → WebKitGTK freeze. Fix: clamp grid gen to the world + cap labels + non-finite guard in `gridGeometry.ts`. | v0.74.0 |
| **#865** (tuxlink-gf5s) | Map hardening from the bug hunt: B1 LocationMap follow-cam (flyTo on every GPS tick → stable initial center), B2 `_leaflet_pos` teardown crash (`map.stop()` before `remove()`), D1 grid **line** cap, D2 `ResizeObserver`→`invalidateSize`, B3 OperatorPin removal guard. | v0.74.0 |
| **#866** (tuxlink-ivfr) | **Heard-station APRS sprites rendered ~2× (huge + offset)** — the headline operator bug. | **v0.74.1** |

**Release state:** `v0.74.0` is a **PRE-RELEASE that still has the huge-sprite bug** (it was cut at 04:11Z, before #866 merged at 06:49Z) — **do NOT promote v0.74.0**. `v0.74.1` (release PR #867, merged) carries all three fixes and is the one to promote to stable. `v0.73.0` was "Latest" throughout, so the buggy 0.74.0 never reached end users as stable.

## The CSP sprite bug — durable knowledge (this was the hard one)

**Root cause:** the production **Tauri CSP injects a `style-src` nonce** (template `__TAURI_STYLE_NONCE__` in the binary; Tauri nonces the app's inline `<style>` tags — here the cold-start pre-paint styles in `index.html`). Per the CSP spec, **a nonce in `style-src` makes `'unsafe-inline'` inert**, so **parsed inline `style="..."` attributes are blocked**. A Leaflet `divIcon`'s html is assigned via `innerHTML`, so the browser *parses* its `style="width:32px"` → CSP drops it → the img falls back to its intrinsic 64px.

**Why it hid from everything:** production-only (dev servers + a bare-WebKit harness have no such CSP); React layout is unaffected because React sets styles via the **CSSOM (JS)**, which CSP does not govern — only *parsed-HTML* style attributes. The sprite code is byte-identical to 0.73.0 (same latent defect; the operator just never triggered it there).

**PITFALL for the future (applies to ALL Leaflet divIcons / any `innerHTML`):** never size/style a `divIcon`'s elements with an inline `style="..."` attribute — it is CSP-blocked in the production bundle. Use HTML presentational **attributes** (`width`/`height` for imgs) or **CSS classes**. The fix touched all three offenders: AprsPositionsMap sprite (→ width/height attrs), LocationMap "you are here" pin (→ `.location-pin` in `LocationMap.css`), Maidenhead grid labels (→ `.maidenhead-grid-label` in `LeafletMaidenheadGridLayer.css`). Regression tests assert each divIcon emits no parsed inline `style`.

**Repro harness (reusable):** `dev/scratch/leaflet-spike/run_webkit.py` (PyGObject WebKit2 4.1 launcher, real engine) + `serve.py` (range server). The faithful component repro mounts the *real* `AprsPositionsMap` with mock `positions` prop, stubbing only `window.__TAURI_INTERNALS__.invoke`. Adding a nonce'd `style-src` meta to the harness html reproduced the 64px; that is how this was caught after static analysis came up empty. (All under `dev/scratch/`, gitignored.)

## Bug hunt (3 hunters, consolidated — committed this session)

`dev/bug-hunts/2026-06-21-leaflet-migration-fatal-{exploratory,holistic,multipass,consolidated}.md`. Verdict: **no second fatal freeze**; the non-fatal findings (B1/B2/B3/D1/D2) all shipped in #865. One false positive (GridPicker dragging — map is per-mount destroyed). FP/out-of-scope all enumerated in the consolidated report.

## State at handoff

- **main = `64ce6390`** (post-0.74.1). All this session's work is merged.
- **Worktrees: NONE in flight** — `bd-tuxlink-{u4k2,gf5s,ivfr}` all disposed (ADR 0009); their bd issues closed.
- **Main checkout** on `bd-tuxlink-xygm/recover-handoffs`. Working tree: this handoff + the 4 bug-hunt reports (committed alongside). `dev/scratch/` repro artifacts are gitignored. Pre-existing shared-repo stashes (7, dated 05-31…06-03) left untouched — not mine.
- **`.github/RELEASE_FREEZE`**: not present (released).

## Open follow-ups (filed)

- **tuxlink-2uba (P2, dev-only):** `converge-build.sh` doesn't stage the `go-pmtiles`/`pmtiles` sidecar → offline-map *download* fails in `dev:converged` with `spawn go-pmtiles: No such file or directory`. **Production CI bundles it (release.yml/ect-build.yml), so shipped builds are fine.** Fix = mirror the voacapl staging in the converge script.
- **Obsolete perf-harness:** `dev/perf-harness/harness.tsx` still imports the deleted MapLibre substrate (`maplibre-gl` + `MapLibreMap`/`MapContext`/`mapHooks`/`MaidenheadGridLayer`). It errors on every `dev:converged` / `vite` dev startup (it measured MapLibre GPU cost, now gone) — obsolete, delete-forward. Not filed as bd; noted here.

## Pending / next

**tuxlink-s1o1 — Winlink stations + contacts on the map** (operator-directed). Plot Winlink RMS/CMS stations + the operator's contacts on the shared Leaflet map, with animations for incoming/outgoing connections + contact-history-on-hover.

- **GATE: brainstorm FIRST** (office-hours; visual companion + high-fidelity dark mocks — launch immediately, don't ask, per project prefs). Do NOT jump to code on a UI feature.
- **Reuse, don't reinvent:** the **cn84 Canvas2D path-animation layer (PR #849)** already animates connection traces over Leaflet (schedule/geometry/triggers/concurrent coexisting traces) — built for APRS digipeat paths, directly applicable to connection animations. Plus the `AprsPositionsMap` theater-of-ops engine, `StationFinderMap` station catalog + reachability tiers, the contacts/address-book + Winlink connection-history data model, and the `LeafletMap` substrate.
- Memory context: the APRS map is an unbounded pan/zoom situational index that "will draw station-connection lines"; ONE map engine is reused across all surfaces.
