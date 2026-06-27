# Handoff — MCP Plan 3 hands-off harness designed + bootstrapped; phase 3.1 plan ready to execute

Date: 2026-06-26 · Agent: maple-sumac-larch · Epic: tuxlink-cvx84

## What this session did (design + bootstrap, no phase code yet)

1. **Designed + got operator approval for the hands-off execution harness** driving MCP Plan 3
   phases 3.1→3.6. Spec: `docs/superpowers/specs/2026-06-26-tuxlink-mcp-plan3-execution-harness-design.md`
   (committed on this branch). Locked decisions:
   - **Spine:** self-paced loop + CI-parking (`ScheduleWakeup`), subagent-driven-development per
     phase, `Workflow` fan-out inside 3.2 + the adrev rounds.
   - **Merge authority:** agent SELF-MERGES each phase on green CI (`gh pr merge --no-ff`); tier-2
     Pi `claude mcp` round-trip gates MCP-surface phases; Codex adrev gates 3.1 + 3.3 pre-merge.
   - **Operator touches (entire budget):** wire-walk flows (captured below), final tier-3 smoke,
     relicense (`tuxlink-tm0cp`, gates only network-exposable packaging — NOT local UDS/stdio),
     and any escalation.
2. **Locked five wire-walk flows** (user + agent flow) as the definition-of-done / tier-3 smoke
   script — in the spec, §"Wire-walk flows". Grounded in the recorded north stars: A(i) parking-lot
   diagnose→remediate, A(ii) uv-pro onboarding, A(iii) CMS-Z password lag, B/D armed ICS-213 send,
   threat-model injection containment. Surfaced **four MUST-VERIFY backend findings** (verify_cms_connection
   dual-listing; modem_ardop_connect gateway param; VARA-emit-unimplemented graceful degrade; the
   exact taint-source set) — embedded in the 3.2/3.3 bd issues.
3. **Seeded all conductor state in bd** (survives any context reset): six phase issues
   `tuxlink-cvx84.1`…`.6` with dep graph `3.1 → {3.2 ∥ 3.3} → 3.4 → {3.5 ∥ 3.6}`. State is
   re-derived each wake from `bd ready` + `gh pr list` — nothing lives in chat context.
4. **Resolved phase 3.1's critical technical unknown + corrected the blueprint** (research recorded
   in `bd show tuxlink-cvx84.1` notes):
   - rmcp UDS transport = feature **`transport-async-rw`** (NOT `transport-io`); `(R,W) IntoTransport`
     over `UnixStream::into_split()`. **Shim needs NO rmcp** (dumb byte-pump).
   - rmcp latest 0.8.x = **0.8.5**; **MSRV-vs-1.75 risk is CI-verified** (push draft, let the cold
     compile answer) — NEVER a Pi cold build. MSRV bump = operator decision → escalate.
   - Plan 2 `EgressGuard` IS on origin/main and `.manage()`-wired (`lib.rs:655`); commands at
     `lib.rs:1526-1528`. No dep on bd-7dwqa. (A subagent wrongly read stale worktree state; verified
     authoritative vs `origin/main`.)
   - AppState = ~9 separate `.manage()` handles; router bundles clones.
5. **Wrote the phase 3.1 implementation plan** (subagent-proof, TDD, 6 tasks):
   `docs/superpowers/plans/2026-06-26-mcp-3.1-transport-spine-plan.md` (this branch).

## Branch / working-tree state

- **Live branch (this worktree):** `bd-tuxlink-cvx84.1/mcp-transport-spine`, pushed to origin, off
  `origin/main`. Contains: harness design doc, 3.1 plan, this handoff. **No phase code yet. No PR
  yet** (opens at plan Task 2, after the workspace + rmcp + server_info stub compile-trigger).
- **Worktree:** `worktrees/bd-tuxlink-cvx84.1-mcp-transport-spine` (claimed by `tuxlink-cvx84.1`,
  `in_progress`). `pnpm install` already run (needed for the pre-push docs linter + frontend gates).
  No untracked/gitignored at-risk content beyond standard `node_modules`/`target`.
- **Stranded (harmless):** the original main-checkout branch `bd-tuxlink-ant8s/ardop-connect-fixes`
  is `[gone]` on remote; my first design-doc commit (`1e274958`, local-only) landed there and swept
  in a pre-existing staged `.beads/issues.jsonl`. A CLEAN copy of the design doc is on this phase
  branch, so nothing is lost; the gone-branch commit can be ignored/pruned. The main checkout still
  has ~80 pre-existing uncommitted files from before this session (not mine; left untouched).

## NEXT SESSION — execute phase 3.1 (do NOT re-design; the plan is ready)

Critical first actions, in order:
1. `cd worktrees/bd-tuxlink-cvx84.1-mcp-transport-spine` and READ
   `docs/superpowers/plans/2026-06-26-mcp-3.1-transport-spine-plan.md` (full TDD task list) +
   `bd show tuxlink-cvx84.1` (research facts — esp. `transport-async-rw`, MSRV-via-CI).
2. Execute via subagent-driven-development. Subagents code+gate+**STOP uncommitted**; the PARENT
   commits via `git -C <worktree>`.
3. **At plan Task 2, push the draft PR** — this starts the CI cold-compile that authoritatively
   answers the rmcp-0.8.5-on-MSRV-1.75 question. If CI demands an MSRV bump → STOP + escalate.
4. Gate before self-merge: CI green → tier-2 Pi `claude mcp` round-trip vs `tuxlink-mcp-testserver`
   (agent-runnable, no operator) → **Codex adrev** (UDS/RCE surface) → `gh pr merge --no-ff`.
5. Merging 3.1 unblocks 3.2 ∥ 3.3 — continue the harness loop.

Pitfall: do NOT build anything beyond the single inert `server_info` tool in 3.1 (no redaction,
no taint, no egress gate — those are 3.2/3.3).
