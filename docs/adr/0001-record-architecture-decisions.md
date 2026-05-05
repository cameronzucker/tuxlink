# 1. Record architecture decisions

Date: 2026-05-05
Status: Accepted
Deciders: cameronzucker, alder

## Context

Tuxlink is being built from the ground up to demonstrate disciplined AI-orchestrated development for a portfolio audience. A predictable failure mode in projects of this kind is the loss of architectural intent: decisions made in a chat session, an office-hours sketch, or a commit message body get diluted as the codebase grows, until "why is it this way" becomes archaeology.

The sister Geographica project surfaced this concretely: substantive choices (no SQLite in early phases, the dual-cost-number methodology, the worktree ban) were not captured in a structured way and had to be reconstructed from chat history, commit prose, and pitfalls docs months later. Reconstruction was error-prone and lossy.

A standard remediation, used by Kubernetes, CockroachDB, Tauri, and many other professional OSS projects, is the **Architecture Decision Record** (ADR): a short, dated Markdown file capturing one decision, its context, and its consequences, kept in version control alongside the code.

## Decision

Tuxlink adopts the [Nygard ADR format](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions) for recording architectural decisions, stored in `docs/adr/` and indexed in `docs/adr/README.md`.

ADRs are written:
- **At decision time** — when a choice between viable alternatives is being made, not retroactively after the code lands.
- **By the agent or contributor making the decision** — moniker / handle is recorded in the `Deciders` line.
- **As part of the same PR** that enacts the decision in code, so reviewers can verify the ADR matches what was built.

ADRs use the format documented in `docs/adr/README.md`: Title, Date, Status, Deciders, Context, Decision, Consequences, Alternatives considered.

## Consequences

**Positive:**
- Future contributors (human or AI) can answer "why is it this way" in seconds without reconstructing chat history.
- Reversing a decision is auditable — the original ADR stays, a superseding ADR explains the change.
- The ADR record itself is a portfolio artifact demonstrating engineering discipline.

**Negative:**
- ~10–30 minutes of overhead per significant decision to write the ADR. Acceptable; the cost of NOT writing one is reconstruction work later.
- ADR drift is possible (the code diverges from the ADR over time) but mitigated by the rule that ADR changes go through PRs alongside code changes.

## Alternatives considered

- **Decisions captured only in commit messages.** Rejected — commit prose is unstructured, hard to discover, and lost in long histories. Geographica demonstrated this failure mode.
- **Decisions captured in wiki / Notion / external doc.** Rejected — splits the source of truth from the code, requires separate access, and tends to go stale when the maintainer rotates.
- **MADR (Markdown Architectural Decision Records) format.** A more structured alternative with explicit "consequences" subsections. Defer; Nygard format is sufficient for a single-maintainer project, and migrating to MADR later is a mechanical exercise.
- **No ADRs (rely on README + pitfalls docs).** Rejected — pitfalls docs capture rules-of-thumb, README captures the user-facing surface, neither captures the *reasoning* behind architectural choices.
