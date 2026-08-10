# Pre-compaction checkpoint — moss-tamarack-taiga (2026-08-10 midday)

Written at the operator's request before context compaction; the session
continues after this doc. Supersedes nothing — read alongside
`2026-08-10-moss-tamarack-taiga-classifier-phase1-inkling-ab-overnight.md`
(the overnight handoff) for the fuller narrative.

## Live state at compaction

- **ADR 0030 drafted and HELD at PR #1325** (Status: Proposed; all eight
  decision points operator-ruled, recorded in efk3k addendum 6; point 5
  ratified after the advisory-not-binding / corpus-generic challenge).
  **The operator's merge is the filing act — no agent merges #1325.**
- **PR #1324** (CPU-viability report + field correction) open, checks
  watcher armed, merge-on-green mine. Field correction is load-bearing:
  full-surface CPU Elmer = impractical today (~15k-token prefix, degrading
  prefill 24→17 tok/s, operator killed hand-poke at 14 min with no output);
  CPU testing line CLOSED until narrowing lands.
- **DRAFT PRs #1319/#1320 (nyyr2 A/B arms): premise falsified** — root
  cause was `ELMER_MAX_TOKENS=3000` in the bench's `launch_inkling1.sh`,
  not the provider (full chain in nyyr2 notes). Recommendation standing:
  close both; Arm-C (suppress+fault-surface) hardening awaits the
  operator's no-thinking POSTURE ruling. One-line bench-side fix
  (`ELMER_MAX_TOKENS=32000` or unset) is the actual nyyr2 unblock — bench
  repo, not this one.
- **Merged today**: #1322 (inkling-dispatch skill + CLAUDE.md/AGENTS.md
  routing), #1321 (A/B comparison + overnight handoff), plus yesterday's
  #1314/#1315/#1317/#1318. bd: y28so CLOSED; 8dkcy filed (prefill warm /
  static-prefix / progress — carries the operator's field measurements);
  nsnre unblocked (its `bd ready` absence is pagination, not blockage);
  pyfv8 parked; nyyr2 in_progress (corrected record); efk3k in_progress
  (canonical design record, addenda 1–6).

## Operator queue (unchanged by compaction)

1. Ratify/edit/merge ADR 0030 (#1325).
2. Rule the no-thinking posture → disposition #1319/#1320 (+ Arm-C priority).
3. pyfv8 human consults (attorney-first).

## Next agent work once #1325 merges

r5jsj (tool-surface survey — operator stress signal on tool gaps) →
ch3e9 (request→catalog prototype; step 1 = Inkling-parseability spike per
the point-4 condition; reuse the T1 spike's template + thresholds).
Route bounded leaf tasks through the `inkling-dispatch` skill (Step-0
serving pre-flight is mandatory).

## Environment / disposal ledger

- Pi: `dev/scratch/cpu-elmer-viability/` (llama.cpp + 2 GGUFs 3.5G +
  results; hand-poke server DOWN, PID freed) — keep until nsnre docs
  written, then disposable. `dev/scratch/{inkling-subagent-eval,sonnet-task-nyyr2}/`
  = eval provenance. `~/.local/node22` + `~/.pi/agent/models.json` standing.
- R2: `~/{inkling,sonnet}-task-nyyr2/` keep until arms disposed;
  `~/efk3k-t{1,2,3}-*` disposable; disk 59G/94% (flagged 3×).
- Worktree `bd-tuxlink-efk3k-classifier-arch` currently on branch
  `agent-moss-tamarack-taiga/precompact-handoff`; hosted all session
  branches (deviation noted in overnight handoff). Other session's
  `bd-tuxlink-qjgx-alpha-logging` still holds `main` (benign gh-merge
  local-step failures; theirs).

## Standing cautions (compressed)

bd needs `--limit 2000` everywhere (588 issues; `bd ready` paginates too);
`bd --notes` REPLACES (read-concat-write); one git mutation per call,
standalone `cd` first, no `git add -A`; docs-PRs merge on the lint:docs
hook; issue root-cause claims are hypotheses — diff working-vs-broken
configs first (nyyr2 lesson, now in auto-memory); subagents invent
monikers and absorb ambient conventions.

Agent: moss-tamarack-taiga
