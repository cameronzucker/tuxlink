# 2026-06-09 marten-poplar-dahlia — tuxlink-dyop LAN tiles shipped (PR #517)

## One-sentence frame

Executed **tuxlink-dyop** (Map-Picker v2 Pillar 1, LAN tile-server ingest) **end-to-end** —
all 10 phases TDD'd in a fresh worktree, per-phase Claude adversarial reviews + a
cross-provider Codex round (9 real findings fixed, incl. 3 P1 SSRF/concurrency), every
CI-equivalent gate green, opened **PR #517** off `main`.

## What completed this session

1. **Merged PR #495** (dyop design + plan) on green CI; caught the dyop branch up to `main` @ 0.39.1.
2. **Full dyop implementation** in `worktrees/bd-tuxlink-dyop-dyop-lan-tiles` (branch `bd-tuxlink-dyop/dyop-lan-tiles`), ~32 commits:
   - **Phase 0 (GATE):** packaged-WebKitGTK CSP spike → **decision: custom `tile` URI scheme** (NOT invoke+blob:); production CSP delta is exactly **`tile:`** on `img-src`. Corrected the design's §8.2 Linux-form error (`tile://localhost/{z}/{x}/{y}`, not `http://tile.localhost`). Decision doc: `docs/plans/dyop-phase0-csp-spike.md`. (This spike ran ~3.4h — two real arm64 packaged builds.)
   - **Phases 1–5 (Rust gatekeeper core):** SSRF host policy (`host.rs`), `TileCoord` (geodetic `2^(z+1)×2^z`, panic-safe), gatekeeper fetch + DNS-rebind defense (`fetch.rs`), CRS-mismatch guard + alignment fixture (`crs.rs`), bounded/atomic/single-flight cache (`cache.rs`). Each independently adversarially reviewed.
   - **Phase 6:** `tile://` serving (`serve.rs`) + `TileGatekeeper` managed state + the `tile:` CSP token in `tauri.conf.json` + the async URI-scheme handler in `lib.rs`.
   - **Phase 8.1:** config persistence (`Config::map_tile_source`) + `configure_tile_source`/`test_tile_source`/`clear_tile_cache`/`tile_source_status` commands.
   - **Phase 7:** frontend `tileSource.ts` (types + invoke wrappers), `TileLayerBridge.tsx` (stock Leaflet TileLayer — tile scheme means NO blob/GridLayer), `BaseMap.tsx` tile-layer-over-raster + validated zoom-raise, Maidenhead `levelFromZoom` retune, `sixCharAllowed` gate (z12). Extended the shared `testMapMock`.
   - **Phase 8.2/8.3:** `MapTileSourceSettings.tsx` (inline panel, warn-not-block) + standalone `TileStatusPill.tsx`.
   - **Phase 9:** source circuit breaker (`breaker.rs`, K=3/30s, injected clock) + cancel-on-pan + `partial`-state BaseMap reconcile.
   - **Phase 10:** `SSRF-1` + `TRAVERSAL-1` pitfalls entries.
3. **Review findings fixed** (all real):
   - Per-phase Claude: `checked_shl` zoom-overflow panic; IPv6-literal egress routing; **CRS cross-field false-accept** (`{crs:4326, tileMatrixSet:WebMercatorQuad}`); **cache cross-coord concurrency BLOCKER** (unserialized meta.json RMW blew the cap 3.9× → per-namespace lock).
   - **Cross-provider Codex** (5, raw transcript local-only in `dev/adversarial/`, gitignored): **[P1]** CRS probe egressed before the resolved-IP gate → both fetch + probe now share one `build_vetted_client` chokepoint; **[P1]** ambient-proxy bypass → `no_proxy` on all tile clients; **[functional]** geodetic x-bound was `2^z` not `2^(z+1)` → eastern hemisphere 400'd; over-budget tile caching; unbounded probe bodies.
4. **Gates (CI-equivalent, local, all green):** `cargo clippy --all-targets -D warnings` clean; `cargo test -p tuxlink --lib` **1574 passed** (117 in `tiles`); `tsc` clean; full `vitest run` **1959 passed (176 files)**.
5. **Opened PR #517** off `main`. CI running at handoff.

## Branch / worktree state (READ before disposing anything)

- **`bd-tuxlink-dyop/dyop-lan-tiles`** — PR **#517 OPEN**. Worktree `worktrees/bd-tuxlink-dyop-dyop-lan-tiles/` — clean working tree, all committed + pushed. Gitignored-on-disk: `node_modules/`, `src-tauri/target/` (~6.7 GB — `rm -rf` on disposal), and the local-only Codex transcript `dev/adversarial/2026-06-09-dyop-impl-codex.md`. **Dispose via the ADR 0009 ritual after #517 merges.**
- **Main checkout** on `bd-tuxlink-xygm/recover-handoffs`: this handoff committed here. `.beads/issues.jsonl` is bd-auto-managed (do not hand-commit).
- **`bd-tuxlink-753p` worktree** — dead, clean; disposal still **pending operator authorization** (unchanged from the prior handoff).

## What is NOT done / next session

1. **Merge PR #517** on green CI (CI is the gate per no-hold-merge-for-smoke; this session left it watching — if it merged, dispose the dyop worktree per ADR 0009).
2. **tuxlink-a1cc** (shared nav controls) — the next feature. **Its #1 task: place the dyop UI** — mount `<MapTileSourceSettings />` in a reachable surface and put `<TileStatusPill status=… zoomCapReason=… />` in the shared picker toolbar. The pill ships standalone here; **a1cc consumes it, must NOT reimplement** the `TileSourceStatus`→display mapping. (There is no general preferences surface today — `SettingsPanel.tsx` is the GPS-privacy dialog — so the tile-source settings belong in the picker control surface a1cc builds.)
3. **tuxlink-sdbd** (Position overlay) — after a1cc. Consumes `sixCharAllowed`/`SIX_CHAR_MIN_ZOOM` from `tileSource.ts` for the 4-char-default/6-char-opt-in precision selector.
4. **Optional:** dispose the dead `bd-tuxlink-753p` worktree (operator OK given previously).

## Gates respected / loose ends

- No RF/transmit path touched anywhere; RADIO-1 did not gate. CSP gained exactly one token (`tile:`); no network/LAN host on `img-src`/`connect-src`; `no_proxy` enforced.
- **Operator dev-build was killed** during the Phase-0 spike cleanup (a broad `pkill -f "release/tuxlink"` reaped the operator's running `tauri dev` session — dev process only, no data loss). Recorded as memory `feedback_no_broad_pkill_on_shared_pi`. If your dev build vanished hours ago, that's why — relaunch it.
- The full-app WebKitGTK render of a live LAN source was NOT smoke-tested (Phase 0 proved the `tile:` scheme renders packaged; an end-to-end live-source smoke is post-merge / a1cc-era once the settings UI is placed and a real geodetic LAN tile server is available).
