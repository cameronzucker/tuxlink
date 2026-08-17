# Classifier program PARKED by operator order — spruce-birch-dune, 2026-08-17

Operator directive (2026-08-16 late night AZT, verbatim intent): **"Durably
park everything."** No work on the classifier line — wiring, specs,
measurement, catalog exposure, any of it — resumes without explicit
operator direction. This document is the parking record: what exists,
where it sits, and why it stopped.

## Why it stopped (the operator's finding, recorded so it isn't lost)

The operator halted classifier wiring work twice in one evening:

1. First stop: the session's architecture decision brief was jargon
   referencing artifacts he cannot see or easily find — not a brief he
   could rule on.
2. Second stop, the substantive one: the wiring was proceeding with **no
   spec, no plan, and no build-robust-features workflow** — nothing but
   the bd issue notes trail. His words: *"If we aren't actually
   architecting a durable plan with a falsifiable spec as a complete
   workflow we can close the loop on, you're not engineering, you're just
   doing stuff."* Invoking the workflow after the fact does not repair
   this: *"brf is a prophylactic, not the cure. This is now far, far too
   late."*
3. The root finding, in his words: *"I was allowing you to self-iterate
   based on evidence from the bench and a goal design parameter. That was
   clearly unsound since when things go off the rails I'm in the dark."*
   The bench evidence DID go off the rails — the three-way narrowing A/B
   was retracted wholesale (see
   `dev/spikes/2026-08-10-tool-narrowing-inkling-recovery/FINDINGS-THREEWAY.md`,
   retraction header) — and the program's owner had no position from
   which to notice, audit, or steer.

Nothing unspecced shipped: the stops landed before any wiring code was
written. Zero changes to Elmer, the agent runner, or the MCP surface.

## Where every piece sits

- **Branch `bd-tuxlink-ch3e9/classifier-wiring` @ `aeaf31ef` — pushed, NO
  PR, fate undecided.** One commit: the threshold recalibration described
  below. It merges only if the operator says so.
- **Main is untouched** by this session's classifier work. Main's
  `src-tauri/resources/catalog/classify-thresholds.json` still carries
  the stale pre-regen tools entry (reject floor 0.582). This is NOT a
  live defect: the classifier is not wired into anything; no code path
  consumes those numbers at runtime today. The corrected number (0.587)
  sits on the parked branch.
- **Worktree `worktrees/bd-tuxlink-efk3k-classifier-arch`**: re-parked
  detached at main (`b342bb98`) after this record merged. Still claimed
  by bd tuxlink-efk3k as the classifier program's worktree.
- **R2**: the temporary recal worktree was disposed after inventory
  (clean — nothing to propagate). The pre-existing `~/tuxlink-ch3e9-build`
  clone is untouched, in the state prior sessions left it. Raw eval log
  at `~/tuxlink-recal-eval.log` on R2.
- **bd**: tuxlink-ch3e9 and tuxlink-efk3k stay `in_progress` as the
  program's design record; a `bd remember` entry records this park.

## The one piece of work this session produced (before the stop)

A threshold recalibration — the debt the surface-repair campaign's row 8
recorded as owed before any classifier wiring. In plain terms: the
classifier compares an operator ask against tool descriptions and uses
two calibrated numbers to decide "no tool fits" vs "several fit — ask."
Those numbers were measured against an older tool list; the tool list has
since changed (92 → 95 tools plus one description rewrite). Re-measured
on R2 against the current list: the old "no tool fits" line had actually
gone wrong — a question about ham license requirements now scores above
it (it collides with the new classifier-weights status tool's
description), so the old calibration would have treated a general
knowledge question as a tool request. The re-measured numbers and the
full evidence table are in
`dev/evals/2026-08-16-ch3e9-tools-threshold-recalibration.md` (on the
parked branch). Selection accuracy itself held steady (93.6% top-12,
unchanged from the last measurement).

## Evidence status at park time (so the next reader does not repeat the mistake)

- **Retracted, cite nothing:** the bench three-way A/B
  (`FINDINGS-THREEWAY.md`) — the document that claimed narrowing +
  schema furnishing improves task outcomes. Operator-voided; fixture
  validity failures plus a serving-stack streaming bug documented in
  `FLOOR-AUTOPSY.md` in the same directory.
- **Standing but caveated:** the v4 selection-layer battery
  (`FINDINGS-v4.md`, same directory) — never retracted, but it ran on
  the same serving stack whose streaming bug the autopsy later found.
- **Standing:** the in-repo, model-free measurements — the catalog and
  tools evals in `dev/evals/` (2026-08-10 and 2026-08-16), and the
  merged substrate (classifier crate, corpora, weights hosting), all of
  which shipped through normal reviewed PRs with CI.

## Recovery (offered, NOT started — the operator drives)

The proposed first move, unexecuted: a state-of-the-classifier-program
review written for the operator to read in one sitting — what he ruled
(dated, quoted) versus what agents decided; what is merged and
load-bearing today; which evidence stands versus retracted, claim by
claim; what was built ahead of any spec. Every claim carrying a pointer
(PR number, file, commit) so any line can be spot-checked without
trusting the author; optionally cross-checked by a Codex pass. Whether
that review happens, and everything after it, is the operator's call.

## Everything else — unchanged

The operator queue is exactly as the compaction anchor left it: IR
one-pager read, bench relay (ledger row 21), four design calls (ledger
rows 16–19), sideload ratification, readback wording, stale
`[model_providers.openrouter]` block in `~/.codex/config.toml`. The
surface-repair campaign remains complete and closed (anchor:
`dev/handoffs/2026-08-17-spruce-birch-dune-compaction-campaign-mechanical-complete-1362-in-ci.md`).

Session: spruce-birch-dune, 2026-08-16 late night AZT (2026-08-17 UTC).
