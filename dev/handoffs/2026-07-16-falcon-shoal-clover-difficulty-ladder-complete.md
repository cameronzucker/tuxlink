# Handoff — graduated difficulty ladder COMPLETE (tuxlink-7raoe milestone 1)

- **Agent:** falcon-shoal-clover
- **Date:** 2026-07-16 (~07:00–17:40 UTC, operator-directed overnight run)
- **Ended:** milestone complete; PR #1125 open (merge blocked by a
  harness permission denial, NOT by any defect — see "Pending" below).

## READ THIS FIRST

1. **Milestone 1 of bd `tuxlink-7raoe` is COMPLETE.** Canonical results:
   `dev/research/2026-07-16-difficulty-ladder/report.md` on branch
   `bd-tuxlink-7raoe/difficulty-ladder` — **PR #1125, OPEN, docs-only, CI
   pending at close. Merge it first thing.**
2. **Headline:** the ceiling broke where the CORRECTION note predicted
   (upper rungs) but NOT along scale. 397B: cleared all rich-brief rungs,
   failed symptom-only diagnosis twice (confident wrong theory), and
   silently complied with both registered false premises (contorted
   fixture, "Deviations: None" — the night's only Request-changes review).
   Hosted-FP 122B: the ONLY Qwen to reason to the true diagnosis mechanism,
   and it reported the false premise — its losses were a Codex↔Qwen seam
   (reasoning-emitted-as-final-message), not reasoning. Spark arms:
   envelope-bound (30-min cap), not competence-bound, on rungs 3/5.
   Integrity anti-correlated with scale on this run.
3. **Purchase decision:** 2nd-Spark capability case WEAKENED; throughput
   case stands (milestone 0). Cheapest next spend = scope item 1 (custom
   worker harness) — it attacks both the seam that cost E122 two
   correct-reasoning deliveries and the token-volume wall behind every
   Spark at-cap failure.
4. **Pre-registration discipline held:** frozen+pushed at `fc83ddaa` before
   any worker ran; one dated post-freeze grading-keys amendment (a second
   hidden pinning test found during S5's run, applied uniformly); every
   worker claim orchestrator-verified (re-run gates, tree diffs, 3x reruns
   for the flake-class rung); 19 Opus review files in `reviews/`.

## State at close

- **Merged this session:** PR #1124 (yew-basin-raven's handoff addendum,
  operator-directed first action).
- **Open:** PR #1125 (`bd-tuxlink-7raoe/difficulty-ladder` — the full
  bundle: rubric, briefs, grading keys, ledger, scores, report, reviews,
  runner, this handoff). The gh merge call was denied by the Claude Code
  permission classifier this session; nothing is wrong with the PR.
- **Worktrees:** the 5 arm worktrees are DISPOSED per ADR 0009 (sdd
  forensics archived to `.claude/worktree-archives/
  bd-tuxlink-7raoe-ladder-arm-*-sdd-forensics-20260716T173225Z.tar.gz`;
  every at-cap diff and both wrong-diagnosis candidates preserved there
  and in the arm branches). The 5 never-merge branches
  `bd-tuxlink-7raoe/ladder-arm-{s5,cn,q122,o397,e122}` remain LOCAL-ONLY.
  The orchestration worktree `worktrees/bd-tuxlink-7raoe-difficulty-ladder`
  remains ALIVE (claimed by bd tuxlink-7raoe) until PR #1125 merges —
  dispose it after merge per ADR 0009.
- **Spark:** RESTORED to `qwen3-coder-next` (container `vllm` up,
  /v1/models verified; ledger has every state change). The patched 122B
  chat template persists at `/home/administrator/serving/` — reusable, and
  the vllm-q122 launch recipe (incl. the `--tool-call-parser qwen3_coder`
  flags whose omission broke the first launch) is in `ledger.md`.
- **Main checkout:** untouched all session (hook-verified once at the
  start; all work in worktrees).
- **bd:** `tuxlink-7raoe` remains `in_progress` (milestone 1 of several;
  notes updated; older notes recoverable via `bd history tuxlink-7raoe
  --json` — NOTE: `bd update --notes` REPLACES, dolt history is the
  archive).
- **Stray processes:** one worker-spawned cold `cargo test` was killed
  mid-run (exact PID); no vitest zombies observed at close.

## Harvest opportunities (real fixes sitting in arm branches)

The S5 arm branch (`bd-tuxlink-7raoe/ladder-arm-s5`) contains verified,
Opus-approved fixes for SIX real backlog items: tuxlink-y6195(3) drift
guard, tuxlink-10dh5 per-step parked windows, tuxlink-46hof error
surfacing (incl. both stale pinning-test rewrites), tuxlink-o1e9w invoke
chokepoint, tuxlink-gac1d stations capability fix (+ Rust capability-scope
test, uncompiled on the Pi — CI will be its first compile), and
tuxlink-y6195(5) delay-bar DOM test. Per the pre-registered disposition
these can be cherry-picked into ordinary PRs (attributed); none of the six
bd issues is closed by the experiment itself.

## What the next session should consider, in order

1. Merge PR #1125 (docs-only; CI green expected).
2. Harvest the S5 candidates into real PRs (biggest immediate value:
   gac1d is a P2 production bug with a 1-line fix; 46hof/o1e9w are P1s).
3. tuxlink-7raoe milestone 2: the custom worker harness (scope item 1) —
   this run sharpened its requirements list: fix the
   reasoning-as-final-message seam, curated tool surface, premise-
   verification + claim/action cross-check in the supervision layer
   (the 397B rung-6 "SPEC became the claim" finding), token-efficiency
   for the Spark's 30-min envelope.
4. Dispose the orchestration worktree after #1125 merges.
