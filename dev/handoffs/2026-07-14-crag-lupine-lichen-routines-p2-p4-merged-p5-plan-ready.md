# Handoff — Routines: plans 2 + 4 MERGED, plan-5 UI plan drafted and ready to execute

- **Agent:** crag-lupine-lichen
- **Date:** 2026-07-14
- **Ended:** natural completion — every task in the operator's session brief landed; next session starts plan-5 execution fresh.

## READ THIS FIRST — where to resume

1. **Plans 1–4 are ALL merged to main** (#1105 spec+engine, #1115 plan 2, #1113 plan 3, #1117 plan 4 @ `ed69cc60`). The whole Routines backend — engine, actions/arbiter/consent/scheduler, validator+dry-run, and the 10-tool MCP surface with the efcc8 security fixes — is on main. Nothing routines-related is in flight on any branch.
2. **Next work item: execute plan 5 (operator UI) — bd `tuxlink-fdmg9`.** The 15-task implementation plan is at `dev/scratch/routines-p5-ui-plan-draft.md` (gitignored; on this machine). Its companion inputs: `dev/scratch/routines-p5-flows.md` (7 flows = the definition of done; **wire-walk remains WAIVED for this epic**) and the approved mocks at `dev/scratch/routines-ui-mocks/`. Branch from main; the plan is grounded in main's post-merge state.
3. **Plan-5 contract that did not exist when the mocks were approved:** the C1 security fix makes `save_routine` discard caller-supplied `transmit_ack` (preserving the on-disk value). The consent dialog therefore stamps the ack ONLY via a new UI-only `routines_acknowledge_automatic` Tauri command (plan Task 1), never via save; the MCP tool list stays closed at exactly 10 (router test to be extended, task 1). Do not "simplify" this away.

## What merged today (PR #1117 — plan 4 + the efcc8 fixes)

bd `tuxlink-efcc8` (P1, CLOSED): all 5 findings fixed, five logical commits `ba3d4292..e7aa9aea`:

- **C1 SECURITY** (`ba3d4292`): save path discards caller `transmit_ack` BEFORE validation, preserves on-disk; regression tests both directions. Breaking change footer on the commit.
- **C2** (`f225a429`): testserver `MockRoutines` + `McpState.routines`; compiled+clippy+tested locally on the Pi (the issue's explicit demand).
- **M2** (`9b08f25e`): `PortError::InvalidInput` → `invalid_request`; dry_run/save caller-input refusals no longer read as server bugs.
- **M1** (`06456860`): spec §14 "the validator warns" (matches §5 + parser).
- **Codex P3 — verified real** (`e7aa9aea`): WWV floor 3900→**2280 s** (nearest-of-:18/:45 worst wait 1910 s + 70 s dwell + 300 s margin); monolith drift-guard test pins the leaf constant to `next_capture`'s real constants (full-hour sweep).

CI green on both arches (first-ever CI run for the PR — see the trap below), merged by this agent with operator authorization, merge commit `ed69cc60`.

## Process traps discovered (transferable)

- **Stacked draft PRs get ZERO CI here:** `ci.yml` triggers only on PRs based at `main`. #1117 sat on a p2 base its whole draft life — that is the structural reason C2's `--workspace` break stayed invisible until review. Retargeting emits only an `edited` event (not a trigger); an empty commit (`f3b0b1da`) armed the first run. If stacking again: expect no CI until retarget + synchronize.
- **Two DIFFERENT flaky tests hit #1115's verify in consecutive runs:** (a) the branch's own quit-gate drain-timeout test — wall-clock margin race, fixed deterministically at `ef10bbee` (channel-park idiom, same as its sibling test); (b) `tuxlink-jt9` `signal_death_is_classified_with_stderr_tail` on arm64 — pre-existing (despite the 2026-07-13 jt9 deflake PR #1090), filed as a new bd bug this session (search `bd list --status=open` for "signal_death"). Rerun-on-flake worked; the jt9 test needs the channel-park treatment.
- **Main checkout is hook-locked while another session is live** — this session worked entirely from worktrees after the hook denied main-checkout ops; `git merge-base` also false-positives the hook (already in memory). This handoff was committed from a throwaway detached worktree pushed `HEAD:main`.

## State

- **Worktrees disposed this session per ADR 0009** (inventory → archive → rm → prune): `bd-tuxlink-03d39-routines-spec`, `bd-tuxlink-pgidw-routines-p3`, `bd-tuxlink-ofw4s-routines-p2`, `bd-tuxlink-oiigb-routines-p4`. Archives (with `.superpowers/sdd` ledgers + the p4 Codex transcript) in `.claude/worktree-archives/*-20260714T*.tar.gz`. Remote branches auto-deleted at merge. Local branch refs for p2/p4 still exist (harmless; `git branch -d` was skipped to avoid the main-checkout hook).
- **~100 other historical worktrees remain** under `worktrees/` — pre-existing backlog, untouched.
- **bd:** closed `tuxlink-efcc8`, `tuxlink-oiigb`, `tuxlink-ofw4s`, `tuxlink-pgidw`, `tuxlink-03d39` (all merged work). Created: plan-5 execution issue **`tuxlink-fdmg9`** (P1) and the jt9-flake bug (P2).
- **Main checkout** (`bd-tuxlink-ant8s/ardop-connect-fixes` + staged `.beads/issues.jsonl` + README edit + untracked scratch): operator state, untouched (one accidental commit was immediately soft-reset; final state verified identical).
- **Plan 6** (dockable pop-out surfaces) remains unwritten — sequenced after 5; pure convenience layer.

## What the next session should do, in order

1. Read this handoff + `dev/scratch/routines-p5-ui-plan-draft.md` in full (897 lines — it embeds the casing-contract table, mock-drift warnings, and per-task gates).
2. Claim **`tuxlink-fdmg9`**, create a worktree off **main**, and execute the plan via superpowers:subagent-driven-development (the plan's own header says so). Wire-walk stays waived; the 7 flows in `dev/scratch/routines-p5-flows.md` are the done-gate.
3. Codex adversarial round(s) per build-robust-features before the final PR.
