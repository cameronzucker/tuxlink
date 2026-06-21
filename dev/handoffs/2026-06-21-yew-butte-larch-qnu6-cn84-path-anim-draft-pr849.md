# Handoff — yew-butte-larch — cn84 digipeat-path animation built; DRAFT PR #849 awaits operator grim-smoke

**Agent:** yew-butte-larch · **Date:** 2026-06-21
**Headline:** Built **tuxlink-qnu6** end-to-end via subagent-driven-development — the faithful cn84 directional digipeat-path animation restored on the Leaflet/Canvas2D engine. Opened as **DRAFT PR #849**. The render is **operator-grim-smoke-gated on llvmpipe** — DO NOT MERGE until the operator confirms the animation is smooth on the real WebKitGTK app (per-frame CPU is exactly what reverted the original cn84; the agent cannot validate software-GL perf).

## What shipped to the branch (PR #849, draft, base main)
Branch `bd-tuxlink-qnu6/digipeat-path-anim`, built in worktree `worktrees/bd-tuxlink-c973-placenames-packs` (warm node_modules + target/). Commits beyond main (`f09b40c5`):
- `1fd982f9` docs(design) + `2d85103c` docs(plan) — written by the prior session (glade-gulch-fern).
- `e36772d7` feat(aprs): port pure `resolveDigipeatPath` resolver + tests from #838 (engine-agnostic; honest hybrid path).
- `67dc1815` feat(aprs): pure `traceProgress` schedule + `trimPath` by-segment-count geometry (TDD, +DEFAULT_TIMING contract assertion).
- `f3d521e4` feat(aprs): `DigipeatPathLayer` — Canvas2D overlay, bounded rAF (stops at phase `done`), reprojects every frame via `latLngToContainerPoint`, `safe()`-guarded.
- `9fc23dff` feat(aprs): wire hover + live triggers into `AprsPositionsMap` (resolve behind a ref; operator position threaded into MapOverlays).
- `404d5b68` fix(aprs): seed live-trace high-water mark on mount, **don't auto-play** a stale backlog frame (Task-4 review finding).
- `529d439d` fix(aprs): **never trace a path from an object/item pin** — RF-honesty (final-review finding; the store only *tags* `isObject`, the design's "handled in useAprsPositions" note was stale).

## Process
- Executed via **superpowers:subagent-driven-development**: one implementer + one task-review per task, then a whole-branch review on opus. Progress ledger at `.superpowers/sdd/progress.md` (in the worktree, gitignored).
- Subagent commit constraint honored: implementers coded + gated + **stopped uncommitted**; the parent (this session) committed each task (worktree-cwd hook denies subagent commits).
- Controller-applied fixes were re-verified against gates before commit; `--amend` is hook-banned so the seed fix is a separate follow-up commit, not a fold-in.

## Gates (all green, local + will run in CI)
`pnpm typecheck` clean · `pnpm vitest run` **3129/3129 pass** · `pnpm build` succeeds. Pure modules unit-tested; the Canvas2D render + wiring are NOT unit-tested by design (jsdom has no 2D context) — that is the grim-smoke's job. No backend changes, so no Rust gate impact.

## THE GATE — operator grim-smoke on llvmpipe (the merge condition)
Build the branch, open the APRS Tac Chat positions map, and verify:
1. **Smoothness is the gate:** hover a station pin / watch a live frame arrive — path draws in hop-by-hop, dot rides sender→operator, lingers, fades, and **zoom/pan stay smooth** throughout (the per-frame-CPU check that reverted cn84).
2. Solid through located hops, dashed across unlocatable hops; no fabricated intermediate pins.
3. Hovering an **object/item** pin draws nothing (RF-honesty guard).
4. **Busy-net:** the live trace re-fires on every advancing beacon (each bounded) — note if rapid cycling is distracting on a busy net.
5. **Empty-start edge** (tuxlink-ae8s): map opened before any traffic → the very first frame won't animate; subsequent ones do. Low impact.

Only mark PR #849 ready + merge once the operator confirms smoothness. (Worktree dev-port note: all worktree `tauri dev` builds bind Vite :1420 strictPort — only one runs machine-wide at a time; verify the running build is THIS branch.)

## Post-open: merge conflict resolved (branch current with main)
PR #849 reported CONFLICTING right after open — main had advanced by `53faa73b` (tuxlink-8fjx: retire the single-select category filter, wire `AprsLayersPanel`), which touched `AprsPositionsMap.tsx`. Merged `origin/main` into the branch (`d507cf71`); the sole conflict was the `<MapOverlays>` call site — kept main's `enabledBuckets={filter.enabled}` and re-applied this branch's `operator={me}`. MapOverlays' bucket-visibility + reconcile deps are main's; the digipeat additions are preserved (ref-decoupled). typecheck clean, 447/447 aprs+map tests pass. PR #849 is now **MERGEABLE / CLEAN** (still draft, still grim-smoke-gated). Branch HEAD: `d507cf71`.

## Worktree state
- `worktrees/bd-tuxlink-c973-placenames-packs` is on `bd-tuxlink-qnu6/digipeat-path-anim`, **clean** (all work committed + pushed). Keep alive for the grim-smoke build + any PR-feedback iteration. Dispose (ADR 0009 ritual) after qnu6 lands. Note the dir-name vs branch mismatch (repurposed from c973).
- Untracked/gitignored-stateful: standard `node_modules/`, `target/`, `.beads/embeddeddolt/`, `.superpowers/sdd/`. No session stashes.

## Supersedes / follow-ups filed
- **Supersedes** tuxlink-k0zz + draft PR #838 (no-rAF FADE was a MapLibre cope). Close them when qnu6 lands.
- **tuxlink-ae8s** (P4 bug): empty-start first-frame edge.
- P3 feature: render the `pos?` text label on dashed segments (currently the dashed style alone carries the honesty signal — deferred per faithful-restore scope).

## Release state (freeze STAYS — unchanged)
`.github/RELEASE_FREEZE` stays. Latest release v0.72.0 (pre-release); main at 0.72.0. Unfreeze plan (operator, 2026-06-21): after **qnu6 lands AND Delete (tuxlink-wl7n) is implemented** → delete RELEASE_FREEZE → release-please repopulates 0.73.0 → big-bang pre-release → promote. **Do NOT unfreeze yet.**

## Pending / next
1. Operator grim-smoke PR #849 → mark ready + merge on confirmation.
2. Implement Delete (tuxlink-wl7n).
3. Then: unfreeze + big release.
4. Backlog: continue the strangler-fig (migrate StationFinderMap, LocationMap, compose position picker, GridPicker off MapLibre; delete the MapLibre substrate).
