# Overnight handoff — moss-tamarack-taiga, 2026-08-11 (Inkling-only measurement night)

UPDATE (morning): the bench A/B COMPLETED and is analyzed — see
`FINDINGS-BENCH-AB.md` (floor/routines/collaborator/elmer-ultra improve
narrowed; elmer diagnostics regress via argument-blind by-name calls;
step-3 hypothesis = furnish the schema on first by-name call). Judge was
272/288 at the findings write and still draining — `python3
bench-arm/analyze_bench_ab.py` refreshes every table. PR #1338 marked
ready, merging on green. The section below is the mid-night snapshot.

Written mid-night as compaction insurance; the bench A/B is IN FLIGHT.
If you are a fresh session: the operator is asleep; the standing directive
is his pre-sleep message — "Please just figure out if we're breaking
Inkling. We're so close to something really great here." The small-model
arm and six-model panel are ABANDONED (his ruling); Inkling is the only
model that matters.

## The answer so far (selection layer): we are NOT breaking Inkling

`dev/spikes/2026-08-10-tool-narrowing-inkling-recovery/FINDINGS-v4.md`
(committed, PR #1338 draft) — 612-row, 3-condition, 3-rep battery at the
real wire, temp 0.2: narrowed-net 101/204 vs everything 82/204; two-turn
34/39 vs 23/39; recovery BETTER narrowed (20/24 vs 17/24); prompt cut
68%; zero fabricated names; pin-set REFUTED as default (96/204 — it
re-invites pre-flighting). Reasoning-stream autopsies show every failure
class is defensible agentic behavior or one hard phrasing case, none
narrowing-caused.

Classifier lane: non-weather floor 29/30 (96.7%); combined labeled floor
64/66 (97.0%); the operator's "lots of non-weather things" challenge is
answered — accuracy is index-design-generic.

## IN FLIGHT: the bench outcome A/B (the instrument that settles it)

- Stock arm: `~/bench-overnight/inkling-v25-full` on R2 (405 bundles,
  judged 403, bucket tallies in this session's transcript).
- Narrowed arm: `~/bench-overnight/inkling-v25-narrowed` — 339 units
  (113 gate-clean cells × 3 attempts), launched ~01:25 AZT, serial
  width 1, ~55s/unit early pace → completion mid-morning. Endpoint =
  narrow rewrite proxy on R2 127.0.0.2:8892 (`narrow_proxy.py`, ledger
  `proxy-ledger-inkling-v25-narrowed`, model-map absorbs the dead
  `inkling-small-nvfp4` alias). Fixture = frozen per-cell classifier
  shortlists + pins, replay-proven 2873/2873 against the stock ledger;
  FAIL-CLOSED on any miss (zero so far).
- Judge: `contract_judge.py` (pulled from R2's bench-dev-mtr — the Pi
  bench repo's judge is STALE) running on the Pi in watch mode
  (`--until-idle 120`), same contract + gpt-5.6-luna-high as stock;
  store = `inkling-v25-narrowed/judgments.contract-v25-silent-cannot-
  false-succeed.gpt-5.6-luna-high.jsonl`. Log:
  scratchpad/judge-narrowed.log.
- Analysis: `bench-arm/analyze_bench_ab.py` (committed; validated on
  partials — early paired floor delivered-rate IDENTICAL 97.4%, by-name
  lazy calls executing, e.g. message_read ×3).
- WALL-CLOCK CONFOUND: units before index ~93 of the narrowed run log
  ran concurrent with the v4 battery on the same serving (stock ran
  alone) — timing comparisons use units AFTER that cutoff only
  (scratchpad/bench-cutoff-at-v4-exit.txt). Bucket outcomes unaffected.
- When run + judge complete: `python3 analyze_bench_ab.py` from
  bench-arm/, append results to FINDINGS (or a FINDINGS-BENCH-AB.md),
  commit to the PR branch, `gh pr ready 1338`, merge on verified green
  (exact-SHA discipline).

## State inventory

- Worktree `worktrees/bd-tuxlink-efk3k-classifier-arch`, branch
  `bd-tuxlink-ch3e9/inkling-overnight-battery` (PR #1338 draft), clean
  at last checkpoint; results-v4 committed.
- PRs #1335 (Spark guardrail) + #1336 (precompact handoff) MERGED early
  tonight. Release freeze still LIVE.
- bd: ch3e9 notes carry the pivot + tonight's program (appended, not
  replaced). Epic efk3k unchanged.
- Memory updated: classifier-shape (pivot), acceptance-platform (R2
  fragility), elmer program untouched tonight.
- **R2 HARD-RESET at ~01:04 AZT** during an 8-thread AVX2 candle eval —
  journal ends mid-line, no shutdown sequence; the same eval at
  RAYON_NUM_THREADS=4 + nice ran clean. Everything heavy on R2 is now
  thread-capped. The reboot killed the stale v25 tee proxy + the bench
  dashboard (neither needed; NOT restarted). PSU/thermal suspect —
  operator should know.
- Spark: Inkling serving TP2 under `thinkingmachines/Inkling-Small-NVFP4`
  all night (never touched); the OLD alias 404s — **Elmer's provider
  config still points at the dead alias** (operator item, one-line fix).
- Bench repo (Pi): has ANOTHER session's uncommitted WIP in crates/ —
  untouched. My two bench scripts live in the tuxlink spike dir
  (bench-arm/) + deployed copies in r2:~/bench-overnight/; bench-repo
  adoption belongs to a bench-rooted session.
- R2 deployed artifacts: ~/bench-overnight/{narrow-fixture.json,
  corpus-shortlists.jsonl, corpus-queries.jsonl, gen_narrowed_plan.py,
  plan-narrowed.gated.json, narrow_proxy.py, gen_narrow_fixture.py};
  ~/classify-overnight/ = rsync'd worktree sources + release eval
  binaries (thread-capped invocations only!).

## The operator's five questions (answers assembled for the morning report)

1. Distance to the "ham radio Jarvis": request classifier (1 of 5) is
   built + calibrated + now evidence-backed, NOT wired (step 3 = the
   wiring decisions, now unblocked by tonight's data). Content/inbox
   classifier (tuxlink-8zq7u) UNBUILT — with the typed conversion
   schema, quarantine reader, security classifier, and capability-grant
   adjudicator, all design-record only (efk3k addenda + ADR 0030).
   Elmer Advanced (shell) exists only as the scoped-grants design.
2. Tested while he sleeps: YES (v4 + bench A/B + floor).
3. Breaking it? NO at selection layer — narrowing HELPS; reasoning
   streams read and autopsied; what it needs = the narrowed frame +
   name inventory, nothing more (pins refuted).
4. Inbox classifier + conversion schema: NOT built (8zq7u open, P1;
   schema design recorded in efk3k addendum 2 / ADR 0030).
5. Missing items surfaced: Elmer provider config → new served name
   (his manual play this morning 404s otherwise!); R2 hardware health;
   6vyk4 inventory tool now unblocked by the corpus artifact; 8dkcy
   prefill work compounds narrowing; trend classifier has no bd issue
   yet; freeze lift at epic end.
