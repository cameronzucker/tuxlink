# Handoff — 2026-07-13 (dune-willow-clover): v0.90.0 shipped, first-run tour SHIPPED (#1114), r788i design awaiting approval

Session arc: resumed crag-fox-savanna's three-PR pipeline → #1089 + #1090
merged on green → v0.90.0 released (operator-approved manual release-merge
dispatch after the 14:00 UTC cron silently skipped) → r788i arm/session UX
designed + mocked (gate holds) → operator asked for a first-run tour →
office-hours design (3 adrev rounds) → operator approved → full SDD build →
**PR #1114 MERGED** with every gate passed.

## Shipped

- **PR #1089** (contacts consolidation) + **PR #1090** (jt9 deflake) merged
  on green, verified by headSha. **v0.90.0 pre-release published** carrying
  both. Release note: the 14:00 UTC release-merge cron did NOT fire
  (GitHub dropped the schedule); operator approved `gh workflow run
  release-merge.yml` — same code path, documented manual override.
- **First-run tour + spatial hint system (tuxlink-10bkw, PR #1114, MERGED
  21:56Z).** Full pipeline: office-hours spec (31 issues fixed across 3
  adversarial spec reviews) → operator approval → 8-task SDD (fresh
  implementer + independent reviewer per task) → WebKitGTK renders
  operator-approved (8 states, dev/scratch/tour-renders/) → final
  whole-branch review READY after a 6-finding fix wave (2 final-review
  Importants incl. a live display:contents zero-rect bug; 4 Codex P2s) →
  **wire-walk on operator-supplied flows: both FULLY WIRED** (fresh-install
  tour firing/dismissal; Elmer point_at menu walkthroughs — the menu-anchor
  gap was FOUND by the wire-walk and built in-branch, menuAnchors.ts derived
  from MENU_TREE) → CI green both arches → merged. Ships: offer card →
  5-stop tour, 4 first-open tips, `point_at` MCP tool with honest acks,
  config schema v7 with upgrade-cohort sentinel, Help → Replay tour, and a
  new mailbox "+ Compose" toolbar button (none existed; approved in renders).
- **tuxlink-r788i design phase** (NOT code): unified arm/dial/session UX
  spec + WebKitGTK mock on branch `bd-tuxlink-r788i/arm-session-ux`
  (worktree `worktrees/bd-tuxlink-r788i-arm-session-ux`). Folds nvgjy +
  P0s kw873/4u43s. **GATE: operator approval of
  /home/administrator/Code/tuxlink/dev/scratch/r788i-renders/arm-session-footer-v1.png
  + the spec BEFORE any transport code.** Two flagged judgment calls in the
  bd notes (Connect-while-armed suspend/re-arm; TTL-expiry lets exchange
  finish).

## Open / pending decision

- **r788i approval** — the render + spec above. Next session's first gate.
- **Contacts wire-walk on r2-poe** — operator decision: deferred to an
  aggregate test session; he installs a build carrying everything he wants
  to batch-test (v0.90.0 has contacts; the NEXT nightly will also carry the
  tour). Recorded on tuxlink-sbf03.
- **gstack upgrade 0.15.16 → 1.60.1** — deferred mid-session (upgrade swaps
  shared skill files; unsafe while sessions are mid-skill). Run at the START
  of a fresh session before invoking any gstack skill.
- **Anchor-catalog doc** rides the Elmer knowledge tier (tuxlink-0mudm,
  noted there); shipped degraded mode (valid-ID self-discovery) covers it.

## Repo / worktree state

- origin/main: #1114 merged on top of #1089/#1090/v0.90.0 + concurrent WWV
  and FT8 MCP surfaces (merge union resolved in c3fecd70).
- Worktrees: `worktrees/bd-tuxlink-10bkw-first-run-tour` — branch
  merged-dead, currently parked on `agent-dune-willow-clover/session-handoff`;
  disposable per ADR 0009 (gitignored: node_modules, .superpowers/sdd
  ledger+reports — archive-worthy only as forensics).
  `worktrees/bd-tuxlink-r788i-arm-session-ux` — LIVE, claimed by
  tuxlink-r788i (in_progress), holds the pushed design branch.
- bd: tuxlink-10bkw CLOSED; tuxlink-r788i in_progress (design pending);
  sbf03 note current; 0mudm carries the catalog obligation.
- Session mechanics warning for next agent: Bash cwd RESETS to the main
  checkout after hook/permission denials — cd-only call first, then bare
  git; pin absolute paths in file-writing scripts (memory
  worktree-git-mechanics has the full rules; it bit this session once,
  recovered via git restore of the two named files).

Agent: dune-willow-clover
