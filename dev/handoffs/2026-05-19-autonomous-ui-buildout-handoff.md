# Handoff — 2026-05-19 — Autonomous UI Build-Out (orchestrator session)

**From agent:** `willow-cypress-heron`
**Session arc:** Shipped Task 9 wizard (PR #72) + webkit TV-static fix (PR #73), then the operator commissioned an **autonomous build-out of all remaining v0.0.1 UI**. This handoff starts the fresh session that runs it as orchestrator.
**Status:** pushed; `feat/v0.0.1` at `773e792`. Execution plan + this handoff committed. No work in flight; no worktrees open.

**You are the orchestrator.** You will dispatch subagents via `superpowers:subagent-driven-development`, review their output with Codex, auto-merge clean low-risk tasks, hold keyring/Part-97/milestone work for the operator, and surface only at two smoke milestones. The operator wants **minimal interaction** — they have a settled design (mocks + design doc) and want execution, not more of the per-task hand-holding that made the prior session inefficient.

---

## Next session's starting prompt

> I'm resuming tuxlink as the **orchestrator** for the autonomous UI build-out. `willow-cypress-heron` handed off 2026-05-19. Read these IN ORDER before doing anything:
>
> 1. `dev/plans/2026-05-19-autonomous-ui-buildout.md` — **THE MASTER PLAYBOOK.** Operator decisions, dependency graph, phase sequence, review model, merge-authority rules, milestone gates. Everything below is a pointer into it.
> 2. `dev/handoffs/2026-05-19-autonomous-ui-buildout-handoff.md` — this handoff.
> 3. `CLAUDE.md` — project rules (worktree discipline ADR 0008/0009, destructive-git ban, Part 97, Session Completion).
> 4. Memory entries (load before acting): `[[main-checkout-is-operator-state]]`, `[[no-ceremony-spiral-on-small-fixes]]`, `[[no-carveout-on-cross-provider-adrev]]`, `[[no-atomic-decisions-to-operator]]`, `[[codex-post-subagent-review]]`, `[[codex-quota-gotcha]]`, `[[browser-smoke-before-ship]]`, `[[subagent-ldc-scoping]]`.
> 5. The wizard plan (already executable): `docs/superpowers/plans/2026-05-18-wizard-cluster-plan.md` Phases 3/4/5.
>
> Then:
> - Generate a fresh moniker (`python3 .claude/scripts/get_agent_moniker.py`).
> - **FIRST ACTION = Phase 0 of the playbook**: author the main-UI-cluster spec (`docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md`) from the design doc + the WinlinkBackend trait, run ONE Codex cross-provider round, revise, write the main-UI plan. Do NOT dispatch any main-UI subagent before this lands. (The wizard cluster already has its plan — Phase 1 can start in parallel with Phase 0 authoring if you want.)
> - Execute per the playbook's phase sequence + waves. Auto-merge clean render tasks after you+Codex review + green gates; HOLD Task 10 (keyring) + Task 11 (Part 97) + milestones for the operator.
> - Surface to the operator ONLY at M1 (wizard smoke), M2 (main-UI smoke), or a genuine blocker.

---

## Why this shape (so you don't second-guess it)

The operator was explicit: *"This is requiring too much operator interaction to be efficient right now when you have a good design spec and well-defined final shape with the mocks we did."* The prior session smoke-tested every PR and surfaced many small decisions. The new model: trust the design, parallelize, let orchestrator+Codex be the review loop, and pull the operator in only for the two milestone smokes (the one thing a headless agent genuinely cannot do — verify rendered UI).

The four locked decisions (playbook §"Operator decisions") were chosen by the operator from recommended options 2026-05-19. The most load-bearing: **main UI gets ONE consolidated spec + ONE Codex round + plan** — not zero adrev (that's what got Wave-1's Tasks 12-16 REVOKED as PRs #47-52), not the full 5-round ceremony. Phase 0 is non-skippable for exactly this reason.

---

## What landed this session

| Item | What | PR # | Status |
|---|---|---|---|
| Quit menu fix | PredefinedMenuItem::quit is Linux-unsupported → reverted to canonical custom-item + on_menu_event + app.exit(0) | #71 | merged |
| Task 9 wizard | Infra (types/reducer/context/Rust skeleton/App routing) + Step 1 Welcome | #72 | merged |
| webkit TV-static | WEBKIT_DISABLE_DMABUF_RENDERER=1 in run() | #73 | merged |
| Bug-hunt records | 4-way Quit-menu analysis | — | committed (dev/bug-hunts/) |
| Execution plan | This autonomous build-out playbook | — | committed (dev/plans/) |

bd: closed r21, 6vi, 4mt, 756, 9pb, ln3, 4p2, cvs, ttp, ko0, wfw this session. Filed wfw (closed). 14 stale worktrees disposed.

---

## State at pause

### Pushed to origin
```
feat/v0.0.1       773e792  (PR #73 merge — the active integration branch; ALL work targets this)
task-amd-main-ui  48656e0  (main checkout's branch; carries handoffs + bug-hunt records + this plan)
main              86ddd3d  (ancient, frozen; not used)
```

### Working tree (main checkout)
- `git status --short`: `.beads/issues.jsonl` (bd auto-export — commit with this handoff) + untracked `dev/scratch/`, `src-tauri/gstshark_*/`, `src-tauri/sidecars/` (build byproducts; harmless; candidates for .gitignore someday)
- `git stash list`: empty
- `git worktree list`: only the main checkout. No worktrees in flight.

### bd state
- 0 in_progress. Ready/open for the build-out: `tuxlink-1r5` (T10), `tuxlink-d76` (T11.5), `tuxlink-e4x` (T11), `tuxlink-zsm` (T12), `tuxlink-y5c` (T13), `tuxlink-dm8` (T14), `tuxlink-69z` (T15), `tuxlink-hvv` (T16), `tuxlink-rit` (T8).
- Excluded from this run: `tuxlink-nk7` (T6 live-CMS, operator-only), `tuxlink-cs7` (T17 AppImage — has a Pat-binary-source decision pending; note already on the issue that PR #73's webkit fix is inherited), `tuxlink-gkn` (T18), `tuxlink-n65` (T19).

---

## Open decisions for the operator

None blocking the run. The four shape decisions are locked in the playbook. The only operator touchpoints are M1 + M2 smokes (and any genuine blocker).

Deferred (NOT this run): `tuxlink-cs7` Task 17's "fetch fork's Pat binary vs la5nta upstream" decision — surface when Task 17 is eventually picked up, not now.

---

## Reminders for the orchestrator

- **Worktree discipline:** per-task worktree (`new_tuxlink_worktree.py --issue <id> --slug <slug>`), `EnterWorktree path=...` before committing, `gh pr merge --merge --delete-branch` (NO squash, ADR 0010), dispose via ADR 0009 ritual after merge. The subagent-LDC-scoping memory: if a subagent must update a plan's LDC banner, its prompt MUST explicitly authorize plan-file edits (default "don't touch the plan" blocks the LDC discipline).
- **Subagent dispatch:** subagent_type must be one of the real types (`general-purpose`, etc.) — the `code-bug-hunter-*` names are SKILLS, not agent types; route them through `general-purpose` with "read + follow the skill at <path>." Pass the moniker into each subagent prompt for commit-trailer forensics.
- **Codex invocation:** `npx --yes @openai/codex exec "<prompt>" 2>&1 | tee dev/adversarial/<date>-<topic>-codex.md` (the `review --commit <SHA>` form rejects a positional prompt — use `exec`). `dev/adversarial/` is gitignored; summarize findings in PR bodies, don't commit transcripts.
- **Parallel dispatch:** send multiple `Agent` tool calls in ONE message for true parallelism. Respect the dep graph (playbook §"Task inventory") — never dispatch two subagents that edit the same file (e.g., wizard.rs Tasks 10/11.5, or anything depending on Task 12's unmerged message model).
- **Don't smoke yourself:** you're headless. Automated gates (vitest/tsc/cargo) + orchestrator + Codex review are your ceiling. Visual verification is the operator's M1/M2 job. Never claim a UI renders correctly — claim "gates green, smoke pending."
- **bd dolt push** fails (Dolt remote not configured) — non-blocking; `.beads/issues.jsonl` commits via git carry the state. Don't chase it.

---

**If this handoff looks wrong tomorrow:** `willow-cypress-heron` made multiple wrong early-session assumptions before ground-truthing via bug-hunt. Verify any surprising claim before acting. Canonical source for any restated rule: the ADRs + CLAUDE.md + the playbook.
