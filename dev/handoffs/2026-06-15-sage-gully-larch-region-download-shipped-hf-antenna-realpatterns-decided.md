# Handoff — region-download fix shipped · HF-prediction engine fixed · antenna real-patterns decided

**Agent:** sage-gully-larch · **Date:** 2026-06-15
**Arc:** Continued the region-pack download thread → finished it → then a cascade of HF-prediction
issues (engine not firing → antenna picker is cosmetic) that resolved into an engine/architecture
decision + a proven foundation. Long session; ends at a clean design-complete + Phase-0-spec'd point.

## SHIPPED / landed (committed + pushed)

1. **Region-pack download — PR #716 READY (awaiting operator merge).** Branch
   `bd-tuxlink-3y7g/fix-region-download`, worktree `worktrees/bd-tuxlink-3y7g-fix-region-download`.
   All 5 Codex follow-ups fixed (commit `7a27a7e`): HIGH backend pre-download manifest refresh
   (race-free); 3 MED (free-space inside install_lock + mkdir; drain-thread join on all exit paths;
   duplicate-reject no longer poisons the original row); LOW `requiresRestart` advisory. **Wire-walk
   PASSED** both operator flows (continent + tiers) incl. fresh-install/first-download/post-upgrade.
   All 4 CI jobs green. **Operator action: merge #716** (I was correctly blocked from merging — out
   of scope).

2. **converge-build voacapl+itshfbc staging — `ea85f089`, pushed on `bd-tuxlink-xygm/recover-handoffs`.**
   Operator-confirmed "that fixed it." Root cause: commit `16fe98d6` moved the voacapl externalBin +
   itshfbc resources glob out of committed `tauri.conf.json` (to unbreak CI); CI's `release.yml`
   re-stages them but `scripts/converge-build.sh` never did the equivalent for the local dev build,
   so the dev app found no voacapl next to the exe → HF prediction degraded to "no forecast —
   distance only". The fix stages voacapl → `target/debug/voacapl` + injects the itshfbc resources
   glob via an ephemeral `tauri dev --config` (no committed-config change → no dirty-guard trip).

3. **Antenna real-patterns DESIGN — PR #735 (awaiting operator merge).** Branch
   `bd-tuxlink-bl01/antenna-realpatterns-design`. Design doc
   `docs/design/2026-06-15-find-a-station-antenna-real-patterns.md`. **Operator action: merge #735.**

## DIAGNOSED + DECIDED (no code change, but load-bearing)

- **"HF prediction values are bullshit — antenna selections all the same."** Root cause: `antenna.rs::ioncap()`
  maps all 10 presets onto 3 IONCAP type-codes (5 horizontals byte-identical, 3 verticals identical).
  PR #707 only partially fixed it. Deeper truth (operator): a *product name* can't determine an
  elevation pattern (height/ground/config dominate), so any preset→pattern mapping is a fiction.
- **Deep research (`wf_d521fe59-160`, 102 agents):** the engine was never the lever — voacapl AND
  ITU-R P.533 both ingest real Type-13/14 patterns; switching engines doesn't help; voacapl is the
  license-clean choice. **Decision: keep voacapl, stop emitting IONCAP type-codes, feed it real
  Type-14 patterns** from 3 sources priority **C > B > A** (precomputed NEC library / embedded NEC /
  file import). Modeling environment operator-confirmed: recycle the 10 hamexandria presets; ground
  default = poor/dry desert (selectable); verticals over poor soil WITH a radial field; low no-tree
  height grid (Tech-Prepper-grounded — see memory `project_rf_deployment_environment`).

## bd epic (decomposed, dep edges set)

- **`tuxlink-j394`** Phase 0 — voacapl real Type-14 ingestion foundation. **bd-READY.** **Architecture
  PROVEN** this session: fed voacapl `sample.14` via the existing ANTENNA card; REL changed sensibly
  (low-angle sample → ~0 on the 215 km NVIS path). `bd show tuxlink-j394` has the **exact byte-format
  spec** (CRLF · F7.3 · 10/line · 30 blocks; the `antcalc.for:184` direct-access detail; the `a21`
  filename ≤13-char gotcha) + the voacapl round-trip verification method. NOT YET BUILT.
- `tuxlink-bl01` Phase 1/C (precomputed NEC library) · `tuxlink-k1jn` Phase 2/B (embedded NEC) ·
  `tuxlink-eybc` Phase 3/A (file import) — all depend on j394.

## Worktrees (all KEEP)

- `worktrees/bd-tuxlink-3y7g-fix-region-download` — PR #716 (ready). Keep until merged.
- `worktrees/bd-tuxlink-bl01-antenna-realpatterns-design` — PR #735 (design doc). node_modules in.
- `worktrees/bd-tuxlink-j394-type14-ingestion` — Phase 0 build home. node_modules in. **NO code
  committed yet** (architecture proven + spec captured in bd-j394; emitter is the next slice).

## Why Phase 0 emitter was NOT built this session

The Type-14 `.voa` is fixed-format Fortran (not list-directed). A byte-exact emitter where an error
produces a *silently wrong* antenna pattern is exactly the plausible-but-wrong RF failure the project
guards against, and a Rust emitter can't be verified locally without a cold cargo build (no-cold-cargo
rule). So: architecture proven + spec captured, emitter deferred to a focused session. The verify path
(documented in bd-j394): emit a high-angle pattern → run voacapl on the NVIS deck → assert REL is HIGH.

## State

- Main checkout `bd-tuxlink-xygm/recover-handoffs`; `ea85f089` is the latest pushed commit there.
- Working tree carries an untracked copy of the antenna design doc (the canonical copy is on PR #735).
- Two live sessions shared the main checkout at handoff time — this handoff may be uncommitted on disk
  if the commit was hook-denied; commit it on `bd-tuxlink-xygm/recover-handoffs` when contention clears.
- Pending operator: **merge #716**, **merge #735**.
