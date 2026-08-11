# Precompact handoff — moss-tamarack-taiga, 2026-08-11 (~04:30 AZT)

Compaction 3 of this session. The classifier epic's measurement night is
COMPLETE and committed; two PRs are merging on watchers; the operator is
exhausted and his faith in the epic is openly shaken — treat go/no-go as
HIS open question for daylight, never cheerlead it.

## Branch / worktree state

- Worktree `worktrees/bd-tuxlink-efk3k-classifier-arch` on branch
  `bd-tuxlink-ch3e9/t1-classifier`, clean after this handoff commit
  (which also adds the two missed v2 result dirs qwen15/qwen30b).
- **PR #1332** (the epic: classifier crate, both corpora + drift gates,
  calibrated thresholds, all battery evidence v1/v2/v3, approved design
  doc). Last head 4e963fce + this commit. CI watcher `bletm15zx` was
  armed pre-handoff; on wake: verify conclusions against the EXACT head
  SHA via the check-runs API (never a padded SHA — that failed once),
  then `gh pr merge 1332 --merge --delete-branch --match-head-commit <sha>`.
- **PR #1335** (Spark guardrail: block-spark-oob-serving hook +
  dev/runbooks/spark-serving/README.md + CLAUDE/AGENTS pointers) on
  branch `agent-moss-tamarack-taiga/spark-serving-hook`. NO watcher armed
  — check its CI and merge on verified green. NOTE: the hook only
  protects sessions after merge reaches the checkout they run from.
- Main checkout: OPERATOR STATE, untouched (one stray commit early in
  the night was fully reverted; never touch it).

## What the night established (all committed; read, don't re-derive)

- `dev/spikes/2026-08-10-tool-narrowing-inkling-recovery/FINDINGS-v3.md`
  = the four-tier real-wire table + litigated conclusions. Headlines:
  ~70% prompt cut every tier, full surface doesn't fit common local
  contexts at all; reachability is per-tier (Luna never leaves the
  array, 30B rarely+perfectly, Inkling freely+principled — its
  outside-array calls are server_info pre-flights/docs grounding —
  1.5B never); ASK-affordance collapses Inkling (v2); zero fabricated
  names in 280 real-wire calls; PIN-SET idea (always-include
  server_info/abort/docs_search) is a HYPOTHESIS, not a decision.
- Instrument ladder v1→v2→v3 with retractions in FINDINGS/FINDINGS-v2;
  nothing below the real wire is "validated"; absolute rates await the
  bench agentic loop with FROZEN precomputed shortlist fixtures (the
  classifier never runs inside the benchmark — trenchcoat rule).
- Coverage honesty: catalog floor is weather-heavy; operator's SIX-MODEL
  PANEL (GLM-4.7-Flash, Qwen3.5-9B, Qwen3.5-27B, Nemotron Nano, Mistral
  Nemo 12B, Gemma 4) is UNRUN — tonight's models were ad-hoc.
- STEP 3 IS NOT DESIGNED. Operator: "wildly premature, we're literally
  still testing real behavior." All wiring ideas are hypotheses until
  the measurement program (panel + non-weather floor + bench cells +
  wall-clock worth-it case) completes.

## Spark / serving state

- Inkling SERVING (two-node TP2) under FULL name
  `thinkingmachines/Inkling-Small-NVFP4` (old alias gone — Elmer's
  provider config needs updating, operator item).
- Head dashboard FIXED tonight: stale cluster container names patched to
  `vllm_node` (backups beside app.py), 2-day orphan uvicorn killed by
  literal pid, systemd unit ACTIVE owning 8090, runbook footer live.
  Unit2's dashboard instance still runs OLD code (footer staged on disk,
  constants unpatched there — dead code on a non-head; fine).
- GLM-4.7-Flash-AWQ CACHED (40GB) via the control-plane fetch; bring-up
  = `~/spark-vllm-docker/run-recipe.sh glm-4.7-flash-awq --solo` on the
  head + the 3-call canary (completion / tools-array call / reasoning);
  operator forewarns GLM issues from OpenRouter history.
- IRON RULE (hook-enforced post-#1335): Spark serving lifecycle via the
  control-plane API / recipes ONLY. Runbook:
  dev/runbooks/spark-serving/README.md.

## Session cautions (all bled for tonight)

Standalone `cd` + `pwd`-guard before EVERY git mutation (cwd silently
resets to the main checkout); one git op per call; grep here is ugrep
(brace patterns silently match nothing — use -F); never pad a SHA;
`gh pr merge` needs --merge non-interactively; bd --notes REPLACES;
CI builds the PR MERGE ref (regenerate registry assets on the merged
tree); serde json! key order flips with workspace feature unification
(use Serialize structs); vLLM needs tool-call parser flags or 400s on
tools; classifier grading: agentic models read-before-act — single-shot
scoring undercounts them; plain-language reports, no invented labels,
decisions-only to the operator.

## Next session queue (in order)

1. Merge both PRs on verified green; then close the loop on tuxlink-ch3e9
   step-2 in bd notes (evidence pointer, no restating).
2. The measurement program: six-model panel via the (now working)
   control plane; non-weather labeled-floor extension (classifier lane);
   frozen-shortlist fixture spec (BENCH REPO session, own root);
   wall-clock worth-it case ("pull the weather for my local area",
   narrowed vs not, one real Elmer turn).
3. Operator items (surface ONLY with decidable questions): epic
   go/no-go sentiment in daylight; Elmer provider → new served name;
   knsw8 authority brief (delivered, awaiting his lines); no-thinking
   posture (disposes #1319/#1320); n8syt ARDOP CODEC fix (premise
   confirmed at HEAD — BRF, modem lane); bench ELMER_MAX_TOKENS
   one-liner; R2 tier-2 cleanup; freeze lift at epic end.
