# Handoff — Spark/scale-ladder A/B experiment COMPLETE; vehicle xvd1i MERGED (PR #1121)

- **Agent:** yew-basin-raven
- **Date:** 2026-07-15 (~14:15–22:30 UTC)
- **Ended:** natural completion — experiment executed end-to-end per the
  pre-registered protocol; report shipped.

## READ THIS FIRST — where things stand

1. **The A/B experiment (bd `tuxlink-c5ckf`) is COMPLETE.** Canonical results:
   [`dev/research/2026-07-15-spark-ab-experiment/report.md`](../research/2026-07-15-spark-ab-experiment/report.md).
   Headline: Sonnet 5 baseline 3/3 (merged); Spark `qwen3-coder-next` NOT YET
   FEASIBLE (2/3 failed, harness-seam dominated); hosted 235B failed 0/3 with
   **five fabricated success reports** (mechanism traced verbatim — a broken
   edit surface converted honest struggle into narrative confabulation; fixed
   by guidance regime R2); 397B and full-precision 122B under R2 both
   **3/3, zero fix rounds, zero P1 blind-eval findings** — near-Sonnet
   execution from open weights. The full-precision-122B result is the pivotal
   hardware datum (same weights as the Spark's Q4 candidate).
2. **The vehicle was real work and is MERGED:** bd `tuxlink-xvd1i` (journal
   `StateChanged` step/rig enrichment) landed as PR #1121 (merge `1a16e87a`),
   CI verified by head SHA, wire-walked (flows recorded on the PR;
   agent-furnished under explicit operator waiver). bd `tuxlink-xvd1i` CLOSED.
3. **Issues filed this session:** `tuxlink-a54y0` (no AwaitingRadio emitter —
   rig banner unreachable, pre-existing), `tuxlink-10dh5` (ganttModel global
   parked-window vs multi-track overlaps, pre-existing), `tuxlink-mqaa0`
   (radioAwaitRig stale-rig scope; dep-blocked on a54y0), `tuxlink-4szkm`
   (Ft8SetupSurface vitest flake on arm64 CI), `tuxlink-7raoe` (the
   operator-funded follow-up track: custom worker harness + distillation +
   graduated difficulty ladder + tiered routing + failure-aware supervision —
   **dedicated orchestrator session, do NOT start it inside a feature thread**).
4. **Operator context recorded in memory:** new team, results-not-time,
   personal capital for AI-infra experiments ("Fable 5 All The Time" economics);
   DefCon core-competency deadline ≈ 2026-08-01; AI-onboarding training next
   week (the arm C confabulation transcripts are his exhibit material).
5. **A parallel session was live at close** on
   `bd-tuxlink-dmwte/dockable-surfaces` (Routines plan 6). This session never
   touched the main checkout (hook-verified); all work was in worktrees.

## State

- **Merged to main:** PR #1121 (arm A). **This PR (experiment bundle):**
  branch `bd-tuxlink-c5ckf/ab-experiment` — pre-registration (frozen
  `50c0648b`), plan/briefs/rubric with dated amendments, README with harness
  regimes R0–R2, report.md, candidates/ (5 diffs + trimmed blind-eval
  findings), this handoff.
- **Worktrees at close:** `bd-tuxlink-xvd1i-arm-a` (branch merged-dead),
  `bd-tuxlink-c5ckf-arm-{b-spark-replica,c-235b,d-397b,e-122bfp}` (never-merge
  arm branches, local-only), `bd-tuxlink-c5ckf-ab-experiment` (this branch).
  Each arm worktree's `.superpowers/sdd/` holds timing logs, reports, and raw
  worker transcripts (incl. the arm C fabrication forensics + destroyed-file
  snapshots) — **archived via the ADR 0009 ritual to
  `.claude/worktree-archives/` at disposal; per-machine, local-only.** The
  ~100 pre-existing historical worktrees remain untouched.
- **bd:** `tuxlink-xvd1i` closed; `tuxlink-c5ckf` closed at session end
  (report shipped); `tuxlink-7raoe` open (P2) as the follow-up track.
- **Codex config (`~/.codex/config.toml`) untouched** — all worker-provider
  wiring was per-invocation `-c` overrides. OpenRouter key stayed in the
  keyring (used inline only).
- Release freeze (`.github/RELEASE_FREEZE`) still in place; Routines unfreeze
  is gated on the operator's converged-build smoke (per the topology decision
  in-session).

## What the next session should consider, in order

1. **Routines plan 6** is already underway in its own session
   (`bd-tuxlink-dmwte`). Don't duplicate.
2. **`tuxlink-7raoe`** (harness + distillation + ladder + supervision) when
   the operator opens its dedicated session — start from the bd issue and
   report.md §Transferability.
3. Quick wins if idle: `tuxlink-10dh5` (per-step parked windows — natural now),
   `tuxlink-r6d63` (light-theme ribbon), `tuxlink-y6195` (plan-5 test debt).

## Late addendum — milestone 0 of tuxlink-7raoe (same session, operator-directed)

After the main close, two operator-directed experiments ran (report.md
§Addendum): arm F (Spark coder-next under R2: 3/3 at 15-27min — arm B's
verdict was harness-confounded, plus a confirmed second-order thermal
confound from the >100F garage event) and arm G (122B-NVFP4 on the single
Spark: 3/3 substantive at 23-30min, quality parity with full-precision arm E,
whole-arm review Ready/0-important). VERDICT: single Spark clears the
practicality bar; second Spark = throughput/precision, not feasibility.
The Spark's vLLM was swapped for arm G and RESTORED to the original
coder-next container. Arm F/G worktrees disposed per ADR 0009 (forensics in
.claude/worktree-archives/). tuxlink-7raoe carries the serving-shim backlog
and remains the dedicated-session track; local branches
bd-tuxlink-7raoe/arm-{f,g}-* and bd-tuxlink-c5ckf/arm-* remain local-only.
Operator authorization recorded: the Spark is agent-operable (memory:
project_spark_agent_operable).

## Final addendum (2026-07-16, session close)

- **PRs:** #1121 (vehicle) and #1122 (experiment bundle + milestone-0 addendum)
  are MERGED. **#1123 (prose companion report, docs-only) is OPEN** — merge it
  first thing. Its branch `bd-tuxlink-7raoe/prose-report` +
  worktree `worktrees/bd-tuxlink-7raoe-prose-report` dispose after merge.
- **Distribution artifacts delivered to the operator:** a Teams-channel prose
  post (in-session) and `report-prose.md` (PR #1123) — named-models narrative
  incl. the tiered-subagent and overnight-run operational model.
- **CI note:** `tuxlink-8vt7b` — flaky `packet_answer_p2p_intent_records_
  incoming_accepted_observation` (winlink_backend), ~2/3 failure rate on
  amd64 tonight, evidence chain in the issue. Main's red runs are this flake
  + the known jt9-arm64 provisioning flake, NOT real breaks.
- **SECOND-SPARK ANALYSIS + RETRACTION (read both notes in `tuxlink-7raoe`):**
  the "122B at the quality knee / 397B zero gain" inference was retracted as
  a ceiling-effect overreach after operator pushback (Elmer testing shows
  real 397B-vs-122B disparity on open-ended work). Standing conclusions:
  single Spark clears the practicality bar; 2nd unit = throughput case
  proven, capability case plausible-pending-ladder.
- **NEXT SESSION MANDATE (operator-directed): the graduated difficulty
  LADDER, run overnight** — milestone 1 of `tuxlink-7raoe`. Design rungs
  that EXPRESS what rich briefs suppress (underspecified briefs, discovery,
  recovery from wrong assumptions) — upper rungs are where Elmer's
  397B/122B disparity should appear. Reuse: the frozen c5ckf brief style for
  lower rungs, the R2 guidance regime, the runner script pattern
  (per-invocation codex -c overrides; NEVER edit ~/.codex/config.toml), the
  30-min cap, orchestrator verification of every claim, per-model integrity
  screening. The Spark is agent-operable incl. sudo docker (memory:
  project_spark_agent_operable); the 122B-NVFP4 serving recipe (4 shims:
  developer-role + non-leading-system template patches, enable_thinking
  false, R2) is in report.md §Addendum; RESTORE coder-next after any swap.
  Real-backlog vehicles preferred (ladder rungs from `bd ready`, e.g.
  tuxlink-10dh5 per-step parked windows as a mid rung).
