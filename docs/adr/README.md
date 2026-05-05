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
- [0003 — Pat owns the mailbox; no SQLite in v0.0.1](0003-no-sqlite-pat-owns-mailbox.md)
- [0004 — Per-task branch model with squash-merge](0004-per-task-branch-model.md)
- [0005 — Rigorous SemVer via release-please](0005-rigorous-semver-via-release-please.md)

## References

- [ADR Tools (Nygard)](https://github.com/npryce/adr-tools) — `adr-tools` CLI; not used in Tuxlink, but the format inspires this directory's structure.
- [Cognitect blog post on ADRs](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions).
- [MADR — Markdown Architectural Decision Records](https://adr.github.io/madr/) — a more structured alternative if Tuxlink outgrows Nygard format.
