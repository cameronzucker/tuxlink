# Handoff — Find-a-Station antenna Phase 1: RF core shipped, runtime/UI wiring remains

**Agent:** cardinal-moraine-glade · **Date:** 2026-06-15
**bd:** tuxlink-bl01 (in_progress) · **Branch:** `bd-tuxlink-bl01/phase1-picker-library`
**Worktree:** `worktrees/bd-tuxlink-bl01-phase1-picker-library`

## Arc

Brainstormed → spec'd → planned → built the **RF-critical core** of Phase 1 (the real NEC
pattern library). The hard, drift-prone part is done and physics-verified. What remains is
mechanical runtime/UI wiring that lives in the full Tauri lib and is best done against the **CI
loop** (the Pi can't finish a cold app build — `feedback_no_cold_cargo_on_the_contended_pi`).

## Context (how we got here)

- Session opened to "merge 3 PRs so bl01 unblocks" — all three (#738/#716/#735) were already
  merged by concurrent sessions; closed j394 → bl01 unblocked.
- Brainstorm (visual companion): operator chose **B** (snapping height slider + live polar
  elevation preview) and **collapse to defensible models** (drop random-wire + magnetic-loop).
- Spec: `docs/design/2026-06-15-find-a-station-antenna-phase1-picker.md`.
- Plan: `docs/design/2026-06-15-find-a-station-antenna-phase1-picker-PLAN.md` (has an
  **Implementation status banner** at the top — read it; it lists exactly what's done vs remaining).

## SHIPPED (committed + pushed)

1. **Generator — `tools/pattern-gen/`** (standalone lightweight crate, NOT `src-tauri/src/bin/`).
   Path-includes `src-tauri/src/propagation/type14.rs` (one source of truth) + deps = `thiserror`
   only → builds in ~15s without the Tauri lib. `run_nec2c` / `parse_total_gains` / `clamp_gain` /
   `elevation_vector` / `build_pattern`. 22 tests pass (incl. the 15 path-included type14 golden tests).
2. **Corrected frequency table** — `FREQS_MHZ = [1..30]`, **block i = i MHz**. Verified against
   voacapl `voacapw/antcalc.for:183` (`ifreq = freqarea(1)`; record # = integer MHz; interpolates
   between integer-MHz blocks). The plan's drafted `[2..31]` was off-by-one and would have
   misaligned **every** pattern — a silent physics bug. **Do not "fix" this back.**
3. **20-pattern NEC library** — `src-tauri/src/propagation/patterns/*.voa` (484 KB, committed,
   pinned `-text` in `.gitattributes` so CRLF survives for include_str!/voacapl). 8 antennas:
   - horizontals × {2.5/4/6/9 m}: `efhw-sloper` (sloping end-fed, tilted lobe), `nvis-wire-dipole`
     (flat, high-angle), `resonant-portable-dipole` (inverted-V), `beam-yagi` (monoband 14 MHz design);
   - verticals (ground-mounted): `portable-vertical-whip` 3 m, `base-vertical-radials` 10 m, `mobile-hf-whip` 1.5 m;
   - `unknown` (flat 0 dBi neutral).
   Physics verified honest @14 MHz (verticals null overhead/peak 26–42°; low wires peak overhead;
   nvis 2.5 m zenith +4.92 > 9 m +1.40; yagi +10.4 dBi). To regenerate: `cd tools/pattern-gen && cargo run` (needs nec2c).
4. **`read_block_gains(voa, block)`** in `type14.rs` — the `.voa` → 91-pt elevation slice for the
   preview (inverse of `to_voa`). Round-trip tested locally in the gen crate (0.04s).

`nec2c` was installed this session (`sudo apt install nec2c`, 1.3.1; operator-approved).

## REMAINING (Groups C/D/E/F — see the plan's status banner for the precise task list)

All Rust pieces (C/D) are in the **full Tauri lib → verify via a CI draft PR, do NOT cold-build locally.**
- **C** enum curation + `#[serde(other)]` migration; **D1** `patterns.rs` lookup (`pattern_voa`,
  `snap_height`, `is_height_variable`, `HEIGHT_GRID_M`); **D2** rewrite `operator_voa_content` to
  return precomputed patterns + delete dead `ioncap()`/`voa_title()`/`IoncapAntenna`; **D3**
  `antenna_pattern_preview` command (uses `type14::read_block_gains`).
- **E** frontend (locally testable via `pnpm test`): curate `propagationPrefs.ts`, `PolarPattern.tsx`,
  `AntennaControl.tsx` snapping slider + conditional vertical state + live preview + ground-limitation label.
- **F** distinctness/height-sensitivity tests (can reuse `read_block_gains` over the committed `.voa`),
  **Codex RF round** (geometries + clamp + freq table — runs locally, no build), **wire-walk gate**, PR.

## Open decision — RESOLVED this session

**Ground selector is inert** under precomputed single-ground patterns (ground entered only via the
IONCAP card; no separate VOACAP path-ground card; `deck.rs`/`engine.rs` never used it). **Operator
chose: keep the Ground dropdown, labeled "Phase 1 models poor-desert ground regardless of selection."**
Keep the prefs field + `ground` param (rename `_ground`) for a future ground × pattern matrix. Spec's
"Single-ground limitation" section corrected accordingly.

## Worktree state (per ADR 0009)

- **Tracked:** clean (all committed). Branch pushed through `f3a0b5ba` (after the final push below).
- **Untracked/gitignored-stateful:**
  - `node_modules/` — installed this session (pre-push `lint:docs` needs tsx); per-worktree, disposable.
  - `tools/pattern-gen/target/` — gitignored build cache (added to `.gitignore`); `rm -rf` on disposal.
  - `dev/scratch/nec-probe/{dipole20m.nec,dipole20m.out}` — gitignored reference deck (the nec2c
    output-format probe). Reusable; not needed in git.
  - `src-tauri/target/` — NOT built (the lib was never cold-built locally, by design).
- **Stale sibling worktree to dispose:** `worktrees/bd-tuxlink-bl01-antenna-realpatterns-design`
  (the OLD design-doc branch `bd-tuxlink-bl01/antenna-realpatterns-design`, now merged-dead via #735,
  27 commits behind). Clean; dispose via the ADR 0009 ritual when convenient.

## Verification provenance

All "tested" claims = **branch-local runs in the pattern-gen crate** (`cargo test` in
`tools/pattern-gen/`) + the real generator run (600 nec2c runs). The **Tauri lib was never compiled
this session** — Groups C/D have NOT been compiled; their correctness is pending the CI draft PR.
