# 7. Lift the worktree ban — superseded by per-task-branch model + Beads + commit-discipline hooks

Date: 2026-05-05
Status: Accepted
Deciders: cameronzucker, kestrel

## Context

The original `CLAUDE.md` (committed 2026-04-22 by agent `lichen` at project init) included a `## Git workflow — worktrees are BANNED` section. The rationale was carried forward from the sister Geographica project, where two near-misses involved subagents `cd`'ing out of a worktree and performing destructive operations on the main repo's branch — one `git reset --hard` wiped 6+ commits from `dev`'s tip pointer, recovered via reflog. The ban prevented the topology that enabled the confusion.

In a 2026-04-23 design conversation, Cameron explicitly framed the ban as "a bandaid while I figured out a better way to address the root cause," noting that "the real issue was subagents and parallel agents either drifting from the intended target branch or making unexpected changes." He proposed a three-layer structural defense: Beads for "what's the work and what blocks what," `auto-claude` for "who has the lease right now and which branch they're on," and `safe-git` for "this commit lands on the right branch or it doesn't land."

Two of those three layers are now in place. [ADR 0004](0004-per-task-branch-model.md) documents the per-task-branch model — each task is a discardable branch off `feat/v0.0.1`, squash-merged after PR, never sharing in-flight state with another task. Beads (`bd` v1.0.3) is installed with the v0.0.1 task graph ported and `bd ready` wired. The third layer is substituted by two `PreToolUse` hooks installed in the 2026-05-05 prep wave: `block-destructive-git.sh` (rejects 13 destructive patterns including `reset --hard`, `push --force`, `clean -f`, `branch -D`, `--amend`, `--no-verify`) and `check-commit-discipline.sh` (rejects direct commits to `main` / `feat/v0.0.1` and missing `Agent:` trailers). The fourth layer Cameron envisioned — auto-claude's lease model for "one agent per branch / per worktree" — is **not** yet in place; the 2026-05-05 handoff explicitly defers auto-claude adoption.

[ADR 0004 §Context](0004-per-task-branch-model.md) noted that the geographica response (banning worktrees) "addressed the symptom but not the root cause," and ADR 0004 itself addresses the root cause. The implication — that the ban was now redundant — was never written up as a follow-on ADR. ADR 0004 §Alternatives considered still cited the ban as in-force at the time of writing, leaving CLAUDE.md, AGENTS.md, and the v0.0.1 plan with the ban intact. This ADR closes that gap.

## Decision

Lift the worktree ban. `git worktree` is permitted as an isolation tool when it adds value — true filesystem isolation between concurrent agents, alignment with eventual auto-claude leases, parallel build artifacts that would otherwise collide. It is not required. The default solo-agent workflow remains `git checkout` in the main repo at `/home/administrator/Code/tuxlink`, because for single-stream work the topology cost of two checkouts buys nothing.

Specifically:

1. Replace `CLAUDE.md`'s `## Git workflow — worktrees are BANNED` section with a tombstone-style note pointing to this ADR. Future agents grepping for "worktree" should land on the lift, not on a stale ban.
2. Update the `AGENTS.md` summary line to match.
3. Update the v0.0.1 plan's two prerequisite/guardrail callouts (lines 19 and 49 in [the 2026-04-22 plan](../plans/2026-04-22-tuxlink-v0.0.1-plan.md)) to drop the worktree-ban language.
4. Do **not** edit prior handoffs or older ADRs that referenced the ban as in-force at the time of writing — those are historical records of state-as-of-then, and this ADR is the authoritative reconciliation.

Worktrees, when used, must still respect the existing infrastructure:

- The per-task-branch model: branches are still `task-NN-<slug>` or `bd-<id>/<slug>` off `feat/v0.0.1`, regardless of worktree topology.
- The destructive-git ban: hooks reject the dangerous operations whether the agent is in the main checkout or a worktree.
- The commit-discipline hook: direct commits to `main` and `feat/v0.0.1` are rejected regardless of worktree, and the `Agent: <moniker>` trailer is still mandatory.

## Consequences

**Positive:**

- Removes a behavioral rule that had become orthogonal to its enforcement layer. Future agents reading `CLAUDE.md` and the plan no longer face a policy-vs-infrastructure mismatch.
- Enables true filesystem isolation between concurrent agents when adopted (a worktree per agent means no cross-pollution of working-tree state, no `cargo build` artifacts colliding, no half-applied edits visible to a parallel reader).
- Aligns with the eventual auto-claude adoption path: auto-claude's lease model assumes one agent per worktree, and removing the ban now keeps the door open without further governance work.
- Reduces the surface area of "rules that must be remembered" by replacing one with structural defenses already in place.
- Establishes a precedent for retiring policies once their structural replacements land — exactly the discipline ADR 0001 was created to enforce.

**Negative:**

- The geographica failure mode (subagent `cd`'s out of a worktree, runs destructive op on main repo's branch) is no longer prevented topologically. It is prevented by the destructive-git hook *for the specific operations* the hook rejects, but a non-destructive but unintended operation (e.g., a non-rejected `git merge` on the wrong branch, a `git tag` in the wrong checkout) is still possible if a subagent loses track of which working tree it is in. The commit-discipline hook covers commits-to-protected-branches; other operations rely on agent discipline.
- Two sources of truth for "where am I working": the working directory + the worktree's `HEAD` pointer. Operators reviewing a session must consider both.
- This ADR creates a maintenance expectation: every future structural mitigation should trigger a review of any older behavioral rule it might have obviated. Acceptable cost; failing to do that review is what produced the ban-vs-infrastructure mismatch this ADR resolves.

**Watched failure modes** (signals that this ADR's conclusion needs revisiting):

1. **A subagent `cd`'s out of a worktree and performs an unintended (non-destructive) operation on the wrong branch.** → The hook layer didn't catch it because the operation wasn't in the destructive-ops list. Response: extend `block-destructive-git.sh` to also reject the offending op, or document a mandatory `git rev-parse --show-toplevel` check at agent dispatch time.
2. **An auto-claude session adopts the lease model but two leases collide on the same worktree.** → That is the lease layer's job; document in auto-claude's adoption ADR if and when it lands.
3. **A `bd ready` race produces two agents simultaneously claiming the same task and creating colliding branch names.** → bd's hash IDs make the collision space tiny but not zero; the coordination layer is bd's claim semantics, not worktrees. If observed, escalate to bd upstream.
4. **A worktree is created and forgotten, leaving a stale `HEAD` pointer that a future session reads.** → Hygiene problem. Response: add a `git worktree list` check to the SessionStart briefing hook output so stale worktrees surface immediately.
5. **An agent reflexively uses `git worktree` for routine solo-stream work.** → Topology cost without benefit. The decision permits worktrees; it does not encourage them. If a session pattern emerges where worktrees are used by default for tasks that don't need isolation, refresh the framing in `CLAUDE.md` toward "worktrees on demand, not by default."

## Alternatives considered

- **Keep the ban as belt-and-suspenders.** Rejected. Behavioral rules that don't add to a structural defense erode in attention. ADR 0001 §Context names the specific failure mode: "substantive choices … were not captured in a structured way and had to be reconstructed from chat history." A ban that is redundant *and* contradicts the implication of a sibling ADR is exactly that reconstruction burden — and is what produced the inconsistency this ADR resolves.

- **Soften the ban to "worktrees discouraged unless X".** Rejected. Half-measures invite case-by-case argument from agents under task pressure ("X applies here, right?"); the result is unpredictable adoption. Either ban or permit; permit + structural defense is the cleaner resolution.

- **Wait until auto-claude is adopted before lifting.** Rejected. Auto-claude is explicitly deferred (per the 2026-05-05 handoff); waiting indefinitely keeps the contradiction alive. The lift is decoupled from auto-claude adoption — the ban is redundant given the *currently-in-place* mitigations (per-task branches + Beads + hooks), not given the *eventually-in-place* mitigations.

- **Convert the ban to a runtime check (e.g., a SessionStart hook that refuses to launch in a worktree).** Rejected. The rationale for lifting is that the failure modes are blocked at a layer below worktrees; a hook that rejects worktrees would just re-implement the ban as code, with no operational benefit beyond "the agent can't ignore it." The agents can't ignore the destructive-git hook either, and that's the layer that actually matters.

- **Remove the ban silently (delete the section from `CLAUDE.md`, write nothing else).** Rejected. ADR 0001 exists specifically to prevent this — substantive decisions must leave a record. A future contributor or agent grepping git history for "worktree" needs to land on the lift, not on a deletion they have to reconstruct from blame.
