# 8. Worktrees mandatory for write work under concurrent sessions, governed by bd-issue ownership

Date: 2026-05-17
Status: Accepted (supersedes [ADR 0007](0007-lift-worktree-ban.md) §"permitted but not required" framing; the rest of ADR 0007 — the historical record of why the ban was lifted — stays accepted; complements [ADR 0006](0006-override-bd-claude-md-defaults.md) bd integration)
Deciders: cameronzucker, cedar

## Context

[ADR 0007](0007-lift-worktree-ban.md) (2026-05-05) lifted the original blanket worktree ban after the per-task-branch model + Beads + commit-discipline hooks made the structural defenses sufficient. ADR 0007's framing was "permitted but not required" — worktrees as an *option* for concurrent-agent scenarios, with `git checkout` in the main repo as the solo-agent default.

Two changes since 2026-05-05 invalidate the "optional" framing for the future tuxlink work shape:

### Change 1 — LFST proved the full 10-layer safety stack works in production

Cameron's two-week LFST sprint (May 2026) produced a battle-tested safety substrate documented in [`2026-05-17-parallel-agent-worktree-safety-stack.md`](https://github.com/cameronzucker/cz-agent-skills/blob/main/docs/2026-05-17-parallel-agent-worktree-safety-stack.md) — ten coordinated mechanisms covering destructive-op refusal, cross-checkout race protection, worktree-issue auditability, disposal data safety, and forensic traceability. The stack has held a clean record on LFST since 2026-05-08, including across high-parallelism multi-session work that would have been impossible without it.

When all ten layers are in place, worktrees become **the safer pattern** for concurrent write work, not the riskier one. The cross-checkout-confusion failure modes that motivated the original Geographica-era ban are blocked at hook layer (per the just-landed [D1 PR](https://github.com/cameronzucker/tuxlink/pull/13)) rather than relying on agent-discipline to avoid the mechanism.

### Change 2 — Tuxlink's concurrent-agent future is near, not hypothetical

The 2026-05-05 handoff explicitly defers auto-claude adoption ("the per-task-branch model and bd are auto-claude-ready when we want it"). The standing-conventions adoption sprint of which this ADR is part is the bridge: it brings tuxlink to LFST's safety-stack parity, which removes the last technical blocker to auto-claude / multi-session work. Tuxlink's UI tasks (Tasks 9-16, post-brainstorm) are natural candidates for parallel implementation overnight, exactly the dark-factory pattern the safety stack was built for.

A worktree-permissive-but-optional rule under those conditions invites the orphan-worktree proliferation that drives most teams to ban worktrees outright. Cameron's framing 2026-05-17 ("front-loading admin work to avoid spiraling grief") explicitly favors the structural defense over case-by-case operator discretion.

### Change 3 — The discipline gap ADR 0007 left open

ADR 0007 §"When using worktrees" mentioned that the per-task-branch model and the destructive-git / commit-discipline hooks "still apply." It did not specify:

- How a worktree's purpose is traceable to a work item.
- How orphan worktrees are prevented or detected.
- How concurrent worktrees coordinate.

[Standing-conventions §5](https://github.com/cameronzucker/cz-agent-skills/blob/main/docs/standing-conventions-cross-project.md) closes that gap: worktrees are tied to issue-tracker entries; a worktree exists iff a bd issue claims it and names its absolute path. This ADR ratifies that rule for tuxlink.

## Decision

### 1. Worktrees are MANDATORY for write work when concurrent sessions exist

When two or more Claude Code sessions are simultaneously live against the tuxlink repository (as detected by `.claude/session-leases/` per D1's race hook), any session not holding the main-checkout lease MUST perform its write work in a worktree, not in the main checkout. The main checkout is reserved for coordination / integration work and the lease-holder's own task work.

For solo-session work (the typical case today), worktrees remain **optional**. The solo agent may use `git checkout` in the main repo if no isolation benefit is gained from a worktree.

The lease-detection mechanism is automatic per D1's hook; agents do not need to manually check for concurrency. The hook denies risky operations from non-lease-holders in the main checkout, surfacing the situation when it arises.

### 2. Every worktree binds to a bd issue

A worktree (Pattern B from LFST's ADR 0011 — operator-spawned long-lived worktrees for multi-day work) is permitted IFF all of the following hold:

1. **A bd issue claims the worktree.** The issue is in `in_progress` status, claimed by the agent owning the worktree, with the worktree's absolute path recorded in the issue body or via `bd remember`. `bd show <id>` is the canonical answer to "what is `worktrees/X` for?"
2. **The branch follows the per-task convention** ([ADR 0004](0004-per-task-branch-model.md)): `bd-<id>/<slug>` preferred when the bd issue exists; `agent-<moniker>/<slug>` or `task-NN-<slug>` otherwise.
3. **The worktree path is `worktrees/<bd-id-or-slug>/`** at the repo root. The `worktrees/` directory is `.gitignore`d (already in place from the prior `.claude/worktrees/` entry; verify before first use).
4. **The worktree's session adheres to standard pre-flight + commit-discipline + handoff-doc requirements** in CLAUDE.md. Worktrees are not an exception to any other rule.

A worktree without a bd-issue claim is an anti-pattern. If a session encounters one (e.g., from a stale handoff), the agent either (a) creates a bd issue retroactively claiming it, or (b) inventories + archives + disposes per the disposal ritual (D3, forthcoming).

### 3. Pattern A (harness-spawned ephemeral worktrees) is uncontroversially permitted

The Claude Code `Agent` tool with `isolation: "worktree"` creates a worktree for the duration of a subagent invocation; the worktree auto-cleans if no changes are made. This pattern was never the failure-mode source and remains permitted without any per-worktree bd issue.

### 4. Coordination via bd dependency edges

When two or more worktrees are simultaneously `in_progress`, the orchestrator (Cameron, or any session) maintains the dependency graph via `bd dep add <consumer-id> <provider-id>` for cross-worktree dependencies. `bd ready` reflects which work is unblocked at any moment; worktree-owning sessions read `bd show <id>` at session start to surface any new dependencies their issue acquired.

This is a discipline expectation, not a hook-enforced rule. The orchestrator owns dependency graph maintenance.

### 5. CLAUDE.md replaces ADR 0007's section with a pointer to this ADR

The CLAUDE.md `## Git workflow — worktrees are permitted (ADR 0007)` section is replaced with `## Git workflow — worktrees mandatory under bd-issue ownership (ADR 0008)`, retaining the historical pointer to ADR 0007 (which still has the lift-the-ban rationale) but flipping the operative rule.

## Consequences

**Positive:**

- Tuxlink reaches LFST parity for safe concurrent-agent work. Auto-claude adoption (still deferred per the 2026-05-05 handoff) becomes unblocked at the source-control layer.
- bd's role expands from "task list" to "coordination primitive" — each worktree has a deterministic identity question (which bd issue claims it?), and orphan worktrees become detectable.
- The combined D1 (race hook) + D2 (this ADR) + D3 (disposal ritual) + D4 (worktree-creator + sessions scripts) closes Layers 2, 3, 4 of the standing-conventions safety stack, complementing the existing Layer 1 (destructive-git hook, expanded in C1) and Layer 5 (moniker discipline, to be generator-backed in B3).

**Negative:**

- Solo-session work gains a small ambient cost: when only one session exists, the rule is a no-op (worktrees optional), but the surrounding hooks (race-detection, lease writes) still execute per Bash tool call. Cost is small (jq + a few file reads).
- The bd-issue-per-worktree rule means worktree creation becomes a multi-step setup: claim or create the bd issue, record the worktree path in the issue body, then `git worktree add`. D4's `new_tuxlink_worktree.sh` script collapses these into one command; without it, agents do it manually.
- A worktree-discovered-without-an-issue situation requires a small triage (retroactive bd claim OR disposal). This is the desired behavior — orphans become explicit rather than ambient — but it does add friction the first time it happens.

**Watched failure modes** (signals that this ADR's conclusion needs revisiting):

1. **A bd-claimed worktree's path goes stale** (operator moves the worktree, deletes it without disposal ritual, etc.). → Detection: `bd show <id>` lists a path that doesn't exist on disk. Response: extend the next session's briefing or an upcoming `bd doctor` check to surface this.
2. **The lease mechanism mis-detects concurrent sessions.** False positives would block legitimate solo work; false negatives would allow main-checkout writes during a race. → Detection: `.claude/session-leases/denied-attempts.jsonl` accumulates entries that surprise the operator. Response: refine the lease TTL or the risky-op pattern set.
3. **Orphan worktrees proliferate despite the bd-ownership rule.** → Detection: `git worktree list` shows entries without corresponding `bd show <id>` matches. Response: schedule a periodic `bd preflight`-style sweep that joins worktree paths to bd issues; surface unclaimed worktrees for operator triage.
4. **The dependency-graph maintenance ask becomes a maintenance burden.** If operators don't keep `bd dep add` current, `bd ready` becomes a lie. → Response: this is a discipline issue; if it gets bad, consider a hook that requires `bd dep` evidence before allowing branch creation.

## Alternatives considered

- **Keep ADR 0007's "permitted but optional" framing.** Rejected. The standing-conventions adoption sprint's whole point is to front-load the safety stack so concurrent-agent work becomes safe by default. Leaving worktrees as a case-by-case choice keeps the project in an ambiguous middle state where the stack exists but isn't relied upon.

- **Mandate worktrees even for solo-session work.** Rejected. The setup overhead has no benefit when only one agent is touching the repo; the main checkout works fine for serial single-stream work. The "mandatory only under concurrency" rule from standing-conventions §5 is the right cut.

- **Use a different ownership mechanism** (e.g., a `worktrees/<name>/.owner` sentinel file, a simple Markdown index in `dev/worktrees.md`). Rejected. bd is already the canonical task tracker per ADR 0006; bundling worktree ownership into the same primitive avoids a second source of truth. The downside (bd-claim ceremony) is the same as for any other bd-tracked work.

- **Wait for auto-claude adoption to land before mandating worktrees.** Rejected. auto-claude is explicitly deferred; waiting indefinitely defers the safety-stack benefits. The mandate is decoupled from auto-claude — it applies whenever two sessions happen to overlap (e.g., Cameron starts a fresh CLI session while a long-running one is mid-task). Solo work is unaffected.
