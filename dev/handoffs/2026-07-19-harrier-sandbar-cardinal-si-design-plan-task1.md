# Handoff: Station Intelligence usability. Spec + plan approved, SDD Task 1 built; session ended for provenance

- **Agent:** harrier-sandbar-cardinal
- **Date:** 2026-07-19 (evening)
- **Scope this session:** operator-supervised brainstorm of the SI redesign (tuxlink-6i0ie cluster), approved spec, approved 13-task plan, subagent-driven execution started; Task 1 implemented and green. Session ended early: this session was launched in the main checkout and did worktree work via shell cd, which `block-main-checkout-race.sh` classifies as main-checkout ops (payload cwd pins to the launch dir), so its commits were denied once other sessions went live. Operator decision: the hook is untouched (tuxlink-0dp5l filed and CLOSED as documentation); the fix is this handoff to a session launched FROM the worktree.

## NOTE: this handoff is itself uncommitted

This session cannot commit. The next session's FIRST actions are commits (see Next steps). Everything below is sitting in the working tree of the worktree.

## Branch and remote state

- Branch: `bd-tuxlink-6i0ie/si-operational-usability`, worktree `worktrees/bd-tuxlink-6i0ie-si-operational-usability/`, claimed by bd `tuxlink-6i0ie` (in_progress).
- Pushed: `c546953f` (approved spec: `docs/superpowers/specs/2026-07-19-station-intelligence-operational-usability-design.md`) and `ec922c91` (approved plan: `docs/superpowers/plans/2026-07-19-station-intelligence-operational-usability.md`). Remote is up to date with local commits.
- No PR yet. The plan opens a DRAFT PR after Task 3 so CI compiles the Rust tasks.

## Worktree dirty state (Task 1, complete and green, awaiting commit)

- Modified: `src/ft8ui/Ft8SetupSurface.tsx` (imports the extracted hook; file is deleted entirely in Task 3)
- Untracked: `src/ft8ui/Ft8StripSetup.tsx`, `src/ft8ui/Ft8StripSetup.css`, `src/ft8ui/Ft8StripSetup.test.tsx`, `src/ft8ui/useDeviceMeterPoll.ts`
- Test evidence: Ft8StripSetup 5/5, Ft8SetupSurface 35/35, `pnpm typecheck` exit 0. Full report: `.superpowers/sdd/task-1-report.md` (gitignored, local).
- Gitignored-stateful content in this worktree: `.superpowers/sdd/{progress.md,task-1-brief.md,task-1-report.md}` (SDD ledger + briefs), `node_modules/` (installed). Do not lose the ledger.
- Implementer deviation worth knowing: the plan's Task 1 sketch imported `RigControlSection` from `../rig/RigControlSection`; the REAL path is `../radio/modes/RigControlSection` (already corrected in the code).

## What was decided this session (operator-approved, do not relitigate)

- Spec (canonical, read it): setup lives in the strip (takeover deleted), three map layers (heard / heat / evidence filter with R = 15% of path distance clamped 50 to 750 mi, 30 min recency, SNR floor default -24), channels JSON API for frequency + bandwidth, BW chips in filter row AND map layer box behind one shared state, frequency hero in the rail, containment invariant with measure-before-touching, MCP parity via `find_stations` extension.
- Approved visual mock (local only, main checkout): `/home/administrator/Code/tuxlink/.superpowers/brainstorm/1358490-1784495890/content/full-window-integration.html` plus `bandwidth-representation.html`. Companion server is STOPPED.
- Execution mode: superpowers:subagent-driven-development; subagents implement but never commit (parent commits with `Agent:` trailer); model tiering per plan.

## Next steps for the next session (launch it FROM the worktree directory)

1. Commit this handoff + the SI ledger note first or together with Task 1; then commit Task 1 exactly as the plan's Task 1 Step 6 lists the files, message: `feat(ft8ui): compact in-strip setup form with OS-convention device select` (+ body + your own `Agent:` trailer + `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`). Push.
2. Record BASE `ec922c91` and run the SDD review step for Task 1: `scripts/review-package ec922c91 HEAD` (from the subagent-driven-development skill dir), dispatch the task reviewer per the skill, fix loop if needed, then mark Task 1 complete in `.superpowers/sdd/progress.md`.
3. Continue Tasks 2 through 13 per the plan. Task 11 REQUIRES measurement before geometry changes; Task 13 runs the wire-walk skill (operator supplies flows greenfield) + a Codex adversarial round (GPT-5.5 pin, ADR 0023) before ship.
4. bd: `tuxlink-6i0ie` carries the resume note; sibling issues hcmfb / nkzng / 1w0d0 close at Task 13; 9obx2 stays blocked until after.

## Environment notes

- Spark serving `qwen3-coder-next` (as-found, untouched).
- Main-checkout lease NOT taken; no hook or main-checkout state was modified. Two other live sessions were active on the main checkout at session end.
- The main checkout's own dirty state (93 files, README, PNGs) predates this session and was not touched.
