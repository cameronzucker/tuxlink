# Architecture Decision Records

This directory holds **Architecture Decision Records (ADRs)** — short, dated documents capturing significant architectural decisions made on Tuxlink, why they were made, and what the consequences are.

ADRs are not a replacement for design documents or specs. They are the **record** of what was decided, written when the decision is fresh, so future contributors (and future AI agents) can reconstruct the reasoning without spelunking through commit messages or chat logs.

## When to write an ADR

- A choice between two or more viable architectures or technologies.
- A constraint accepted that limits future options (e.g., "Pat owns the mailbox; no SQLite").
- A workflow / process commitment that the project will be held to (e.g., "per-task branches with squash-merge").
- A reversal of a prior decision — supersede the old ADR, write a new one explaining the change.

Routine implementation choices and minor refactors do NOT need ADRs. The bar is "would a contributor six months from now reasonably ask 'why is it this way' and benefit from a paragraph of context?"

## Format

Tuxlink uses the [Nygard ADR format](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions) — short, structured Markdown. Each ADR has these sections:

```markdown
# NNNN. Title (decision in present tense — "Adopt X" / "Use Y" / "Ban Z")

Date: YYYY-MM-DD
Status: Accepted | Superseded by NNNN | Deprecated
Deciders: <names or session monikers of people involved>

## Context

<The problem or situation that prompted the decision. ~3 paragraphs.>

## Decision

<What was decided, in present tense. Be concrete.>

## Consequences

<What follows — both the positive consequences (this is now possible) and the negative ones (we now have to live with this constraint). Include reversal cost if non-trivial.>

## Alternatives considered

<Brief list of options NOT chosen, and why. Don't bury this — it's the most useful section for future readers.>
```

## File naming

`NNNN-<short-slug>.md`, zero-padded to 4 digits. Numbers are assigned in chronological order; once an ADR has a number, it never changes.

## Lifecycle

- An ADR is `Accepted` when merged.
- If a later ADR overrides it, the original's status changes to `Superseded by NNNN` (and the superseding ADR's `Context` references the original). The original's content stays — it's the historical record.
- An ADR is never deleted; superseded ADRs remain for the audit trail.

## Index

- [0001 — Record architecture decisions](0001-record-architecture-decisions.md)
- [0002 — Tauri 2 + React + single-crate architecture](0002-tauri-react-single-crate.md)
- [0003 — Pat owns the mailbox; no SQLite in v0.0.1](0003-no-sqlite-pat-owns-mailbox.md) — *dependency target shifted from upstream `la5nta/pat` to `tuxlink-pat` fork per 0011; ownership-of-mailbox rule itself remains operative*
- [0004 — Per-task branch model with squash-merge](0004-per-task-branch-model.md) — *squash-merge clause superseded by 0010; per-task-branch model itself remains operative*
- [0005 — Rigorous SemVer via release-please](0005-rigorous-semver-via-release-please.md)
- [0006 — Override bd's CLAUDE.md defaults via Tool referee section](0006-override-bd-claude-md-defaults.md)
- [0007 — Lift the worktree ban (superseded by per-task-branch model + Beads + hooks)](0007-lift-worktree-ban.md) — *operative rule superseded by 0008; historical record retained*
- [0008 — Worktrees mandatory under bd-issue ownership](0008-worktrees-mandatory-under-bd-issue-ownership.md)
- [0009 — Worktree disposal ritual (inventory → archive → physical remove → prune)](0009-worktree-disposal-ritual.md)
- [0010 — No-squash merge for integration branches; merge-commit (no-ff) replaces squash (supersedes 0004's squash clause)](0010-no-squash-merge.md)
- [0011 — Fork Pat as `tuxlink-pat`; refactor cred-handling first; refactor other limits as discovered (amends 0003's dependency target)](0011-fork-pat-for-tuxlink.md)
- [0012 — v0.0.1 main UI adopts Mock D (Mail.app-minimal)](0012-v001-main-ui-adopts-mock-d.md) — *SUPERSEDED by 0013; its premise (operator approved Mock D) was a misidentification — retained as the historical record*
- [0013 — v0.0.1 main UI is Mock B (principles-faithful), not Mock D (supersedes 0012)](0013-v001-main-ui-is-mock-b-not-mock-d.md) — *the approved design is Mock B; agent-authored records are not the spec — the operator's approved artifact is*
- [0014 — Design the v0.5+ modem clean-sheet; do not examine VARA's internals (preserve the independent-creation defense)](0014-clean-sheet-modem-no-prior-art-examination.md) — *bright line: no decompilation, no RE write-ups, no black-box on-air of VARA, from any source*

## References

- [ADR Tools (Nygard)](https://github.com/npryce/adr-tools) — `adr-tools` CLI; not used in Tuxlink, but the format inspires this directory's structure.
- [Cognitect blog post on ADRs](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions).
- [MADR — Markdown Architectural Decision Records](https://adr.github.io/madr/) — a more structured alternative if Tuxlink outgrows Nygard format.
