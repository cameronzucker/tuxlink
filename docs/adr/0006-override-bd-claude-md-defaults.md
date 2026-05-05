# 6. Override bd's CLAUDE.md defaults via an external "Tool referee" section

Date: 2026-05-05
Status: Accepted
Deciders: cameronzucker, alder

## Context

[Beads](https://github.com/gastownhall/beads) is the project's chosen issue-tracking primitive (see [ADR 0004 §Alternatives considered](0004-per-task-branch-model.md) and the Beads adoption decision recorded 2026-05-05). When `bd setup claude` runs in tuxlink, it appends a `<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:ca08a54f -->` block to `CLAUDE.md` containing operational directives for AI agents.

Three of those directives conflict with existing tuxlink-wide commitments:

1. **"do NOT use TodoWrite"** — Claude Code's TodoWrite is a first-class harness primitive for in-turn working memory. Issue spam results if every micro-todo ("read file X, edit file Y") becomes a Beads issue. The Claude Code harness even prompts for TodoWrite usage via system reminders.

2. **"do NOT use MEMORY.md files"** — Claude Code's auto-memory system at `~/.claude/projects/<slug>/memory/` is a documented harness feature that auto-loads `MEMORY.md` into every session's context. The system prompt has an entire `# auto memory` section describing how to use it. The tuxlink memory dir is already seeded with 9 entries (user profile + cross-cutting feedback adapted from geographica) which would be lost or fragmented under bd's directive.

3. **"PUSH TO REMOTE — This is MANDATORY … YOU must push"** — directly conflicts with the operator's stated preference to "directly supervise" push timing, with [ADR 0004's per-task-branch model](0004-per-task-branch-model.md) where pushes happen post-PR-review, and with the Claude Code system prompt's risk-action default ("pushing code … warrants confirmation … unless authorized in advance in durable instructions").

bd's directives are well-meaning — bd assumes a greenfield where bd is the sole player. tuxlink isn't greenfield: TodoWrite, MEMORY.md auto-memory, the per-task-branch ADR, and the operator-supervises-push convention all predate bd installation.

## Decision

Override bd's three conflicting directives via an external `## Tool referee` section in `CLAUDE.md`, placed *outside* bd's `<!-- BEGIN BEADS INTEGRATION -->` markers and *before* them in document order. The override section:

1. Lists which tool owns which job, in a concern-vs-tool table.
2. States explicitly that "when bd's auto-managed section conflicts with the table, the table wins."
3. Calls out each of the three specific overrides in a labeled list, with a one-line rationale for each.
4. Cross-references this ADR for full context.

Critically: **do NOT edit inside bd's BEADS INTEGRATION markers.** bd's `hash:ca08a54f` field is its drift-detection mechanism; future `bd setup claude` invocations (bd version bumps, fresh clones re-running install, agents reflexively running bd setup) may regenerate the section if the hash mismatches, silently overwriting in-marker edits.

## Consequences

**Positive:**
- Override is durable across bd version bumps and re-installs (bd doesn't touch outside its markers).
- A reader landing on bd's directives can find the override by reading further down in CLAUDE.md or by searching for "Tool referee."
- The override mechanism is explicit and grep-discoverable, not implicit ("bd's directives apply except where they contradict X" — which would be ambiguous in practice).
- The `## Tool referee` section becomes the single load-bearing referee any future tool conflict can be added to (a fourth tool with conflicting opinions extends the table; doesn't require a fresh ADR per conflict).

**Negative:**
- A naïve reader of CLAUDE.md who skims and lands inside the BEADS INTEGRATION block may follow bd's directives literally without noticing the override. Mitigated by: (a) the `## Tool referee` section appearing earlier in the document, (b) the section's first sentence explicitly telling the reader to override bd's section, (c) the operational drift signature being captured in [docs/pitfalls/implementation-pitfalls.md §BD-1](../pitfalls/implementation-pitfalls.md#bd-1-bd-opinionated-tooling-overrides).
- The override section becomes a maintenance surface. If bd 1.x → 2.x adds new directives that conflict with project commitments, the override section must be extended AND this ADR's "Specific overrides" list updated. (See "Watched failure modes" below.)
- Two sources of truth in CLAUDE.md (override section + bd's auto-managed section) is structurally messier than a single coherent document. Acceptable cost for the durability gain.

**Watched failure modes (signals that this ADR's conclusion needs revisiting):**

1. **bd version bump introduces a new directive** that conflicts with existing project commitments. → Extend ADR 0006's override list AND the `## Tool referee` table. Do NOT silently soften the override; record the new conflict explicitly.
2. **An agent files spurious `bd create` micro-issues** for in-turn work that should have been TodoWrite. → bd's TodoWrite ban won the agent's attention. Refresh the override or move the `## Tool referee` section higher in CLAUDE.md.
3. **The auto-memory dir at `~/.claude/projects/<slug>/memory/` stops growing** while bd-stored knowledge does. → bd's MEMORY.md ban won. Same response.
4. **A session auto-pushes** to origin without operator confirmation. → bd's mandatory-push directive won. Same response.
5. **`bd setup claude` reports a hash mismatch** or silently regenerates the BEADS INTEGRATION block. → Expected; the override section survives. If bd's regeneration also touches outside its markers (a bd 2.x change), file an issue and consider Option A migration (edit inside the markers as a fallback).

## Alternatives considered

- **Option A: Edit inside bd's BEADS INTEGRATION markers.** Rejected. bd's `hash:` field is drift-detection. Future `bd setup claude` runs may regenerate the block, silently overwriting in-marker edits. The failure mode is invisible until an agent acts on a regenerated directive.

- **Option C: Leave bd's section untouched; trust agents to reconcile conflicts case-by-case.** Rejected. Agents under task pressure tend to grab the closest explicit rule and follow it. The "closest rule" near a `bd ready` decision will be bd's directives, not the system prompt's general guidance about TodoWrite or auto-memory. Implicit conflict-resolution doesn't hold under load.

- **Migrate user/feedback memory into bd's `bd remember` store.** Rejected. The Claude Code harness has a documented auto-memory feature that auto-loads `MEMORY.md` from the project's auto-memory directory. Migrating to bd would lose that automatic context injection at session start AND couple cross-project memory (Cameron's user profile is the same across geographica/tuxlink/pandora-hardware) to a single tool's storage format.

- **Stop using bd entirely; remove the BEADS INTEGRATION block.** Rejected — bd's *core function* (dependency-aware issue tracking with `bd ready` for orchestration) is genuinely valuable and has no equivalent in TodoWrite or auto-memory. The conflict is with bd's *opinionated framing*, not its core feature set.

- **Fork bd or submit a PR upstream** to make the BEADS INTEGRATION text configurable / less opinionated. Deferred. Worth doing if upstream is receptive, but the override mechanism is sufficient for tuxlink in the meantime.
