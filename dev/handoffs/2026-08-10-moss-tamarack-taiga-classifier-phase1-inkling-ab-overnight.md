# Handoff — classifier epic Phase 1 + Inkling subagent program (moss-tamarack-taiga)

Session span: 2026-08-09 evening → 2026-08-10 ~01:30 AZT (operator post-DefCon
kickoff, then traveling, then asleep; heavy autonomous stretches). Everything
below is pushed; three items await operator decisions.

## OPERATOR DECISION QUEUE (the morning list)

1. **ADR 0030 decision brief** — five-classifier architecture, eight decision
   points with recommendations, delivered in-chat (2026-08-09 ~19:40 AZT
   message). ADR authorship/filing is operator-only (standing rule created
   this session after a course-correction — see memory
   `adrs-are-operator-decisions`). Evidence: PRs #1314/#1315/#1317 (all
   merged), `dev/spikes/2026-08-09-t1-catalog-embedding/FINDINGS.md`,
   efk3k notes addenda 1–5 (full record restored from dolt history after
   `--notes` overwrite; `bd show tuxlink-efk3k` is complete again).
2. **Pick an arm for tuxlink-nyyr2** — DRAFT PR #1319 (Arm A, Inkling,
   adapter-minimal) vs DRAFT PR #1320 (Arm B, Sonnet, COR-4 runner contract).
   Parent review verdict + economics: `dev/evals/2026-08-10-nyyr2-ab-inkling-vs-sonnet.md`
   (PR #1321, watcher merging on green). Recommendation: Arm B's design,
   with its own five uncertainty items as the review conversation. nyyr2
   stays in_progress until an arm (or hybrid) lands.
3. **tuxlink-pyfv8** — hosted virtual-Tuxlink demo endpoint, PARKED at your
   request for human consults (attorney-first; AGPL §13 forces vstation
   publication; full consult record in the issue).

## Shipped this session (all merged to main)

- #1314 classifier kickoff doc; #1315 T1 catalog-embedding spike (two-host);
  #1317 Pi gte straggler completing the 8-arm dataset; #1318 Inkling
  graded-eval report (tuxlink-ja6ix CLOSED, 3/3 clean sweep). #1321
  (A/B comparison report) merges on green via watcher.
- bd: efk3k retitled + claimed + 5 addenda; ch3e9 dep-blocked behind efk3k;
  nyyr2 claimed with full A/B record; pyfv8 filed; ja6ix filed+closed.

## Key findings (details in the linked docs)

- T1 spike: bge-small-en-v1.5 97.2% top-1 / 100% section zero-shot on the
  real catalog, zero-overlap reject gap, ~14ms true single-query on R2
  (operator platform ruling: R2/x86 is the realistic target; Pi = floor);
  torch RSS ~1.1GB means native runtime for the 60–300MB budget; gte-small
  disqualified for Pi-floor (72min/template batch pathology).
- Research sweep (addendum 4): Jun-2026 adaptive-eval paper empirically
  endorses deterministic-policy-decides; four residual weaknesses folded into
  the threat model; Prompt Guard 2 license needs an operator call.
- Inkling program: graded evals 3/3; full-task A/B verdict = free local lane
  for bounded leaf tasks, frontier for contract-level design; harness recipe
  in `dev/evals/2026-08-09-inkling-pi-subagent-eval.md`.

## Worktree / branch state

- `worktrees/bd-tuxlink-efk3k-classifier-arch` (claimed by efk3k, in_progress)
  hosted ALL session branches — a deviation from strict one-issue-one-worktree
  (ja6ix/nyyr2 branches rode it); flagged, not repeated without cause. Current
  branch: `bd-tuxlink-nyyr2/ab-comparison-report` (merged-dead once #1321
  lands). Live remote branches: `bd-tuxlink-nyyr2/arm-a-inkling`,
  `bd-tuxlink-nyyr2/arm-b-sonnet` (draft PRs — keep until pick).
- Another session's worktree `worktrees/bd-tuxlink-qjgx-alpha-logging` holds
  `main` checked out (caused benign gh-merge local-step failures all session).
  Not touched; that session's owner should resolve.
- Operator main checkout untouched on `bd-tuxlink-ant8s/ardop-connect-fixes`
  with its 102 dirty files, including an untracked copy of the (now-merged)
  kickoff plan doc — may collide on a future checkout; operator's to clear.

## Environment state (dispose-after-pick list)

- R2: `~/inkling-task-nyyr2/` + `~/sonnet-task-nyyr2/` (A/B trees + session
  JSONLs) — KEEP until the arm pick lands, then delete; `~/efk3k-t2-verify/`
  + `~/efk3k-t3-verify/` (eval verify crates) — disposable now;
  `~/efk3k-t1-spike/` (spike, venv already deleted) — disposable.
  Userland installs kept as standing tooling: `~/.local/node22`, pi provider
  config at `~/.pi/agent/models.json` (spark provider). **R2 disk: 59G free
  (94%) — flagged twice; the bench lives there and it is trending tight.**
- Pi: `dev/scratch/inkling-subagent-eval/` + `dev/scratch/sonnet-task-nyyr2/`
  (gitignored eval artifacts + session JSONLs) — keep as eval provenance or
  archive at will; `~/.local/node22` standing.
- gstack upgrade 1.61.0.0 available (deferred mid-session).

## Standing cautions for the next session

- bd has 588 issues: ALWAYS `--limit 2000` (default truncates silently).
- `bd --notes` REPLACES — append by read-concat-write (bit efk3k this session).
- Current CLAUDE.md bans: one git mutation per shell call, no `cd && git`,
  no `git add -A`. Docs-PRs merge on the lint:docs hook, not full CI.
- Subagents (both model families) invent commit-trailer monikers and absorb
  repo conventions from any context file in reach — verify trailers, expect
  handoff-doc cosplay.

Agent: moss-tamarack-taiga
