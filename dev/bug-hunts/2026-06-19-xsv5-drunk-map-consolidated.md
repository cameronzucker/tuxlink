# tuxlink-xsv5 "Drunk Map" Bug Hunt — Consolidated Findings

**Date:** 2026-06-19 (session moss-bog-cove)
**Scope:** The map tile-LOAD performance regression (tiles 5–20s, areas blank for seconds, basemap-wide, no APRS connection required). Last-good build #819 (`a81c8ac4`, 2026-06-18 17:57); regression surfaced in the `a81c8ac4..main` diff.
**Method:** Empirical-first (operator directive: do NOT root-cause by static code reading — that failed the entire prior session). Trace-log analysis → hypothesis-directed code read → MapLibre source verification → independent adversarial refutation.

---

## Root Cause (CONFIRMED)

**`src/aprs/AprsPositionsMap.tsx`, `WxOverlay` category-filter effect (~line 618).** The effect re-applies the station-category filter on every `styledata` event and, in the default `category === 'all'` state, calls `map.setFilter(layer, null)` for each `FILTERABLE_LAYERS` to "clear" the filter. This created a self-clocking, per-frame, infinite source-reload loop:

1. On mount, `apply()` runs → `setFilter(layer, null)` for each filterable layer.
2. MapLibre `Style.setFilter` guards re-work with `if (deepEqual(layer.filter, filter)) return;`. A cleared filter is stored internally as `undefined`, but the call passes `null`, and **`deepEqual(undefined, null) === false`** (maplibre-gl 5.24.0 `deepEqual$1` falls through to `a === b`). So the guard NEVER trips for the `'all'` case.
3. Each non-guarded call runs `_updateLayer(layer)` → `_updatedSources[source] = 'reload'`, `tileManager.pause()`, `_changed = true`.
4. The per-frame `Style.update()` sees `_changed` → `_reloadSource()` for each marked source (full re-tile on the worker) → fires `data`/`dataType:'style'`.
5. The Map's `data` handler re-emits it as **`styledata`** AND calls `_update(true)` → `triggerRepaint()` + `_styleDirty` — so the next frame is scheduled with **zero external interaction**.
6. `styledata` → `apply()` → `setFilter(layer, null)` → back to step 2. **Loop, once per frame, forever, each iteration force-reloading the filtered layers' sources.**

**Amplification (why even trivial tiles were slow):** every `usePushData` hook in the same file subscribes a `setData` re-push to `styledata` (4 GeoJSON sources: positions, uncertainty, operator, wx-badge). The per-frame `styledata` storm therefore re-dispatched **all four** GeoJSON sources to MapLibre's worker pool for a full re-parse every frame, saturating the pool. That is why the trace shows even a **1-feature in-memory** `aprs-operator` tile taking 5–7s.

The default category is `'all'`, so **100% of sessions** with the APRS map open hit this — no APRS connection needed, basemap-wide. #819 predates the WX overlay entirely → no loop → smooth, which exactly matched the operator's predicted "build #819 → smooth" branch.

### Evidence chain

- **Trace logs** (`~/.local/state/tuxlink/logs/tuxlink.2026-06-20-00.jsonl` + `-01.jsonl`, captured against the *current* 8.9 GB pack): 188 `maplibre-tile-slow` events. Worst offenders are the in-memory overlays — `aprs-wx-badge` (69, median 6.4s) and `aprs-operator` (70, median 5.2s) — alongside `pack-continent-na` (median 11.9s) and `protomaps` (median 13.5s). A 1-feature in-memory tile cannot be slow from disk/pack/parse cost → the bottleneck is a saturated shared client resource (the worker pool), independent of pack data.
- **Temporal density:** median inter-event gap **50 ms**, 145/187 gaps < 0.5s, in 5 burst-clusters separated by >60s idle gaps — the signature of a tight self-clocking loop that pauses when the map idles (no repaint scheduled) and resumes on interaction.
- **MapLibre 5.24.0 source** (`worktrees/bd-tuxlink-xsv5-stall-detect/node_modules/maplibre-gl/dist/maplibre-gl-dev.js`): `Style.setFilter` guard (60680), `_updateLayer` reload+`_changed` (60850–60861), `Style.update` reload+`fire(data/style)` (60192–60260), Map `data`→`styledata` + `_update(true)`→`triggerRepaint` bridge (71459, 73741–73747), `deepEqual$1` (9204).
- **Independent adversarial verification (agent a0537264):** tasked to REFUTE; attempted every failure angle (autonomous repaint scheduling, event-type mismatch, deepEqual semantics, coalescing, mount-guarding) and each refutation failed against the installed source. Verdict: **CONFIRMED**, fix correct, no additional independent loop sites (sprite bake is idempotent via `hasImage`; feature-state is a victim, not a driver).

---

## The Fix

`src/aprs/AprsPositionsMap.tsx`: clear with `undefined`, not `null`.

```diff
- if (cat.key === 'all') m.setFilter?.(layer, null);
+ if (cat.key === 'all') m.setFilter?.(layer, undefined);
```

`deepEqual(undefined, undefined) === true` short-circuits `setFilter` to a true no-op in the default `'all'` state (zero work, no reload, no `styledata`), while still correctly clearing a previously-set category filter (the `filter == null || filter === undefined` branch handles both). The non-`'all'` branch was already loop-safe (a structurally-stable expression `deepEqual`s `true` on the second pass). A self-documenting comment is added so it cannot regress back to `null`.

---

## Disposition of all findings

| # | Finding | Class |
|---|---|---|
| B1 | `setFilter(layer, null)` clear-path → per-frame source-reload loop | **Confirmed bug — fixed** |
| — | `usePushData` styledata→setData re-push (×4 sources) | Amplifier, not root. Correct on *legitimate* (rare) style swaps; harmless once the storm is gone. **No change.** |
| — | `ensureSymbolImage` sprite bake on styledata | False alarm — idempotent (`if (!force && hasImage(id)…) return`). Victim of the storm, not a driver. |
| — | `setFeatureState` apply on styledata | False alarm — `sourcedata`, not `styledata`; victim, not a driver. |
| — | Pack data changed (8.9 GB vs "17.4 GB"); `download.rs` 8g28 detail-tier | **Ruled out.** The "17.4 GB" was the `region-manifest.json` `typical_bytes` *estimate*, not a prior pack; `download.rs` changed only the progress watchdog (k9pg), not tile content; pack `source_build` unchanged (`20260614`). The handoff's "8g28 detail-tier re-download" framing was a misread. |

---

## Test Gap Analysis

**B1 — why missed:** the WX filter effect guards with `if (!m.setFilter) return`, and the shared MapLibre test double (`src/map/testMapLibreMock.ts`) had **no `setFilter`** — so the entire effect was dead code under test. Every existing test silently skipped the buggy path. Compounding: the loop is a MapLibre-internal `styledata` refire that a unit test cannot exercise without a real GL context, and the prior `null` looked idempotent ("clear the filter").

**Catch test added:** `testMapLibreMock` now implements `setFilter` (records calls + models the `null`/`undefined` clear semantics). `AprsPositionsMap.test.tsx` asserts that in the default `'all'` state, the filter clear arg is **never `null`** (and the stored filter is `undefined`). Verified to **fail against the buggy `null` code** and pass against the fix.

**Pitfall coverage:** new MapLibre-specific pitfall — *style-mutating methods (`setFilter`/`setPaintProperty`) re-fire `styledata`; subscribing a handler to `styledata` that calls one of them is a self-clocking loop unless the call is a guaranteed no-op when nothing changed; and `setFilter(layer, null)` is NOT a no-op on an unfiltered layer because `deepEqual(undefined, null) === false` — clear with `undefined`.* Candidate for `docs/pitfalls/implementation-pitfalls.md` (deferred; noted here).

---

## Confirmation still owed (operator)

Static gates can't exercise a runtime perf loop. The empirical confirm is operator-driven: build origin/main + this one-token fix (frontend-only; `tauri dev` HMR suffices — no cargo rebuild) and drive the map. Expectation: smooth tile loads, no `maplibre-tile-slow` lines in `~/.local/state/tuxlink/logs/`.
