# 2026-06-08 moss-basalt-hawk — smoke-walk triage → offline-map foundation (design + plan SHIPPED to branch)

## One-sentence frame

Resumed the 40-item smoke-walk triage; found the mechanically-tractable backlog
essentially shipped, picked item 21 (map-based GRIB region), discovered its
"locked" design rested on a factual error, got the operator's architecture call
(**stay offline-first / never public OSM + remediate the shipped OSM widget**),
and took it through the full build-robust-features design pipeline (5-round
cross-provider adrev + 3-round plan review) to a pushed, subagent-proof plan —
**no implementation code yet; that is the next session.**

## What completed this session

1. **Triage reconstruction (40 items).** 40-agent read-only workflow vs bd + git
   + current `origin/main`. Result: **25 merged**, 8 partial/in-flight, 6 filed,
   1 unaddressed. The remaining items are all gated (design / RF-greenlight /
   5-round-adrev / blocked-on-in-progress / finish-in-flight). No clean
   autonomous coding item remained — which is why item 21 became a design arc.
2. **ADR 0009 disposal of `bd-tuxlink-813d` worktree** (FZ-M1, merged PR #470).
   **7.1 GB reclaimed.** Codex transcript archived to
   `.claude/worktree-archives/bd-tuxlink-813d-fzm1-compact-polish-codex-20260608.md`.
   Registry pruned. (The other ~94 worktrees were left untouched.)
3. **Item 21 architecture decision (operator).** The locked spec
   `docs/design/2026-06-07-map-pin-grid-design.md` claims "greenfield / no map
   lib / never OSM," but `origin/main` already has `leaflet`+`react-leaflet`, a
   Maidenhead converter (`src/forms/position/maidenhead.ts` + `.rs`), AND a CSP
   that whitelists public OSM tiles which `src/compose/PositionMapWidget.tsx`
   loads directly (shipped PR #392/#420 ~2 days BEFORE the brainstorm, which read
   a 639-behind checkout). **Operator decision 2026-06-08:** stay locked to the
   offline-first / never-public-OSM posture AND remediate everything ingesting
   public OSM, replacing with a one-time-downloaded bundled static world map.
4. **bd arc created + wired** (all depend on the foundation):
   - `tuxlink-z9u4` (P1, **IN_PROGRESS**, this worktree) — foundation: bundled
     static world map + GridMapPicker (pin+box) + compose OSM remediation.
   - `tuxlink-mxmx` (item 21, GRIB box wiring) → closes with this PR.
   - `tuxlink-714t` (P1, compose OSM remediation) → closes with this PR.
   - `tuxlink-urbv` (item 18, pin into Settings/wizard) — **follow-up**, also
     blocked on in-progress `tuxlink-9xy1`.
   - `tuxlink-dyop` (P3, Rust tile-gatekeeper + opt-in permitted server) —
     **follow-up split out** of z9u4 (this PR ships bundled-only, zero
     tile-server affordance).
5. **build-robust-features design pipeline — complete through planning:**
   - Brainstorm: satisfied by the locked spec (operator said "stay locked").
   - **5-round adversarial review** (1 Codex + 4 Claude lenses) → architecture
     confirmed sound (EPSG4326 + bundled `<ImageOverlay>` + reuse converter + CSP
     revert), **15 binding corrections C1–C15** folded into the approach doc.
   - **Subagent-proof plan** + **3-round plan review** (caught 2 blockers + a real
     Task-3 correctness bug — degenerate/over-range GRIB region — all fixed).

## Branch / worktree state (READ before disposing anything)

- **Feature branch `bd-tuxlink-z9u4/offline-map-foundation`** — PUSHED to origin,
  commit `8f1f607` (docs only: approach + plan). **No PR yet** (code lands next
  session, then PR). Branch is **active** (lifecycle hooks permit commits).
- **In-flight worktree** `worktrees/bd-tuxlink-z9u4-offline-map-foundation/`
  (off `origin/main`, bd-claimed by `z9u4`). Clean working tree after the commit.
  Gitignored-but-stateful on disk: `node_modules/` (~300 MB, installed this
  session — ready for next session's vitest/build/tauri dev), and
  `dev/adversarial/2026-06-08-offline-map-foundation-codex.md` (the Codex review
  transcript, gitignored, local-only). No `src-tauri/target/` yet (no Rust built).
  **Do NOT dispose — this is active work.**
- **Main checkout** on `bd-tuxlink-xygm/recover-handoffs`: `.beads/issues.jsonl`
  dirty (bd auto-manages → Dolt; do not hand-commit the JSONL). This handoff
  added + committed on this branch. A prior untracked handoff
  (`2026-06-08-bison-lupine-sycamore-fzm1-compact-polish.md`) was also committed
  here to close that loose end.

## What is NOT done (next session)

**Execute the plan** `docs/plans/2026-06-08-offline-map-foundation-plan.md` (in
the z9u4 worktree) via `/executing-plans`. 10 tasks, pure-math-first TDD ladder:
1. Vendor the Natural Earth equirectangular world PNG (provenance pinned).
2–3. Pure `projection.ts` + `gridGeometry.ts` (jsdom-tested, NO Leaflet).
3. Pure `gribRegion.ts` — `signedBboxToGribRegion` (whole-degree, ordered,
   clamped, non-degenerate; correctness-critical).
4. `<BaseMap>` (EPSG4326 + bundled `<ImageOverlay>` + shared marker-icon fix) —
   **FREEZE `BaseMapProps`**; create the canonical react-leaflet test mock here.
5–6. `<MaidenheadOverlay>` + `<GridMapPicker>` (pin + custom box-drag).
7. Wire GRIB box mode into `GribRequestPanel` (item 21 / `mxmx`).
8. Remediate `PositionMapWidget` → `<BaseMap>` + revert CSP + **invert**
   `positionMapCsp.test.ts` (`714t`).
9. Append dated grounding-correction to the locked spec.

## Gates the next session MUST respect (do not skip)

- **`vitest green ≠ map-correct`.** jsdom CANNOT render Leaflet (it's mocked).
  Pure math is unit-tested; components are shape-tested via the ONE canonical
  mock; **real render / projection / box-drag is grim-on-WebKitGTK ONLY**
  (`feedback_chromium_not_webkitgtk_proxy`). Restart `pnpm tauri dev` to load
  frontend changes — Ctrl+R is a no-op in the webview.
- **No RF path** — GRIB requests only queue to the outbox; RADIO-1 does NOT gate.
- **Branch from `origin/main`; work in the worktree** (it's at origin/main HEAD).
  Never the 639-behind recovery checkout. Pin `pnpm -C` / `cargo --manifest-path`.
- **CSP stays `'self'` for tiles.** Exact post-remediation CSP is in the plan
  (C5). Run `pnpm build` (CI gate) — the new PNG asset import is the most likely
  CI break.

## Pending decisions / loose ends

- None blocking. Architecture is operator-decided.
- `tuxlink-urbv` (item 18) is blocked on in-progress `tuxlink-9xy1`
  (Settings→Location) — wire the pin once 9xy1 lands.
- ~94 other worktrees exist (mostly stale/orphaned-WIP from prior sessions); a
  worktree hygiene sweep is a separate future task, not this arc.
