# Handoff — dyop Phase 0 CSP spike COMPLETE (marten-poplar-dahlia, 2026-06-09)

## One-sentence frame
Phase 0 of the tuxlink-dyop LAN-tile-server plan is done: the tile-serving
mechanism is **pinned to the custom `tile` URI scheme** (Linux img-src token
`tile:`), backed by real packaged-build WebKitGTK CSP evidence, with all spike
scaffolding reverted — the decision doc is the sole surviving artifact.

## Branch / working-tree state
- Branch: `bd-tuxlink-dyop/dyop-lan-tiles`, pushed to origin, up to date (no ahead/behind).
- Two commits this session (the only commits on the branch since merge-base `cebf7a6`):
  - `aae19e8` docs: dyop phase-0 CSP spike plan
  - `27be1d5` chore: revert dyop CSP spike scaffolding; decision recorded
- `git diff $(git merge-base origin/main HEAD) HEAD --stat` = **doc-only**
  (`docs/plans/dyop-phase0-csp-spike.md`, +228). Zero residual scaffolding in
  `src-tauri/src/lib.rs`, `src-tauri/tauri.conf.json`, `src/main.tsx` (all
  byte-identical to branch HEAD).
- NOTE: plain `git diff origin/main` shows a large delta — that is origin/main's
  OWN advances since the branch point (read-unread tuxlink-etxt et al. landed
  after branching), NOT this branch's changes. The branch will need a
  `git rebase origin/main` (non-interactive) before/at Phase 6 integration.

## The DECISION (what Phase 6 implements)
- **Mechanism: custom `tile` URI scheme.**
- **Exact Linux production CSP token Phase 6 adds to `img-src`: `tile:`**
  → `img-src 'self' data: tile:`. Add NOTHING else (no host, no widening of the
  forms-scoped `connect-src http://127.0.0.1:*`).
- The design §8.2 claim that Linux uses `http://tile.localhost` is **WRONG for
  Linux** — that is Tauri's Windows/Android custom-protocol form. Linux uses
  `tile://localhost/{z}/{x}/{y}` and the scheme token `tile:`. (Verified against
  Tauri 2.11.2 docs AND empirically.) If tuxlink ever ships Windows, that target
  additionally needs `http://tile.localhost`.
- Phase 6 layer: stock Leaflet `TileLayer` with template `tile://localhost/{z}/{x}/{y}`
  and `subdomains: []`. NO custom `GridLayer`, NO `blob:`, NO `revokeObjectURL`
  lifecycle — the scheme path is structurally leak-free.

## Evidence (packaged WebKitGTK builds, on the labwc rig)
Read via a `spike_report(result)` Tauri command → binary stdout (because **Tauri
does NOT propagate `document.title` to the OS/Wayland window title** — `wlrctl`
title reading is unreliable; this was discovered mid-spike). `grim` corroborated.
- Build A (`img-src … tile: http://tile.localhost blob:`):
  `tileScheme(tile://localhost)=LOADED | tileHttp(http://tile.localhost)=BLOCKED | blob=LOADED`
- Build B (production CSP, no spike tokens):
  `tileScheme=BLOCKED | tileHttp=BLOCKED | blob=BLOCKED`
- Both candidates passed render + no-host; tie-breaker exception fired (blob:'s
  mandatory revokeObjectURL is the named Pi-OOM hazard) → `tile` scheme wins.
- Screenshots (gitignored, local-only): `dev/scratch/spike-buildA-positive.png`
  (page body showed the LOADED/BLOCKED/LOADED line), `dev/scratch/spike-buildB-negative.png`.
- Build timing: clean Build 1 was inflated to 50m by a kill+restart (harness fix);
  warm rebuilds (lib/conf change → tuxlink crate recompile + link) ~17–20m each on
  this arm64 Pi. `--no-bundle` was used (binary alone enforces the CSP).

## Quality gates
- Rust spike unit tests passed (2/2) before revert (`spike_tests`).
- Frontend `tsc --noEmit` passed at each harness iteration.
- Final state: code files identical to branch HEAD (known-good); only a doc added.
- Pre-push doc-link lint gate passed on push.

## In-flight worktree state (this worktree)
- Path: `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-dyop-dyop-lan-tiles`
  (bd issue tuxlink-dyop, IN_PROGRESS, claims this worktree).
- Untracked/gitignored on disk: `dev/scratch/spike-buildA-positive.png`,
  `dev/scratch/spike-buildB-negative.png` (gitignored evidence), and a 6.7G
  `src-tauri/target/` (release build cache). The target was LEFT intact because
  Phase 6 immediately follows and a cold rebuild costs ~50m; disk is not tight
  (269G free). If the worktree is disposed before Phase 6, `rm -rf src-tauri/target`.
- `.beads/` dolt state holds a new keyed memory `dyop-phase0-csp-decision` (local;
  no dolt remote configured for this worktree — the canonical record is the
  committed+pushed decision doc).

## CONCERN — operator dev build was terminated
During spike-binary cleanup, a broad `pkill -f "release/tuxlink"` plus the spike
launches coincided with the disappearance of the operator's running
`tauri dev` session (the converge-build-worktree debug build,
`target/debug/tuxlink` + its `tauri.js dev` + Vite). It is GONE. No data loss
(dev process only; recoverable by relaunching the dev session), but flagging it
honestly: if you (operator) had a dev build up, you'll need to restart it. Future
spike sessions should kill ONLY their own PID by exact match, never a broad
`pkill -f release/tuxlink`.

## What is pending / next
- Phases 1–5 (Rust types, gatekeeper, SSRF, cache, config) then Phase 6 (serving +
  Tauri wiring, instantiates this decision) per
  `docs/plans/2026-06-09-dyop-lan-tiles-plan.md`. Phase 6 now has its blocking
  input (the `tile:` decision).
- Execution model: `superpowers:subagent-driven-development` per the plan; Phases
  6+ frontend tasks are flagged expand-before-dispatch in the plan.
- Recommend `git rebase origin/main` (non-interactive) early next session to pick
  up origin/main's post-branch advances before Phases 1–5 add code.
