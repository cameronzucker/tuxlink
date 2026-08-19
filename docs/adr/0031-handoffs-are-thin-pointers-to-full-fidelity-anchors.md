# ADR 0031 — Handoffs are thin scaffolding pointers to full-fidelity anchors

- **Status:** accepted
- **Date:** 2026-08-18
- **Decider:** Cameron Zucker (operator), issued verbatim in-session
- **Recorded by:** agent basil-redwood-cove (transcription of a settled
  operator decision; see the transcription rule in
  [CLAUDE.md §Documentation propagation contract](../../CLAUDE.md))

## Decision

Session-end handoff documents are **thin scaffolding: pointers only**, to
full-fidelity artifacts — plans, specs, ADRs, epics in bd, specific
fully-fledged and fully-described bd issues, and the like. A handoff never
attempts to BE the memory of a work line. For any in-flight work line, a
full-fidelity artifact must exist as its anchor and designated source of
truth; the handoff points at it. Cross-session state is established by the
anchors working in concert with bd issues, PRs, and git history, plus a
quick grep of the local repo — never by reconstruction from a handoff's
own prose.

## Context and rationale (operator's, near-verbatim)

The project has run into **effective cross-session data corruption** when
handoffs attempted to be lossy memory themselves without full-fidelity
artifacts backing them. This ADR is **anticipatory, not reactive to a
local incident**: the operator hit major blow-ups at work involving model
degradation, where state reconstruction from lossy summaries failed. The
structural insight: a lossy handoff makes cross-session state quality
dependent on the *reading model's* reconstruction ability at read time — a
dependency that fails exactly when the reader is weakest. Full-fidelity
anchors make the reader's condition irrelevant: the state is written down,
not re-derived.

The fix is easy so long as it is enforced at write time: ensure the
full-fidelity artifacts exist FIRST, designate them sources of truth, and
reduce the handoff to the map.

This formalizes what the 2026-08-16/17 classifier-program episode taught
(see `dev/incidents/2026-08-17-classifier-program-overlatitude-and-regrounding.md`
and the no-unspecced-self-iteration rule): session notes are a lossy
compression of program state; the spec/plan pair is the artifact designed
to survive compaction and handoff. ADR 0031 extends that from build lines
to ALL cross-session state.

## What a compliant handoff contains

1. One short framing paragraph (what happened, at what altitude to read).
2. Pointers: the anchor artifact for each in-flight line (spec/plan path,
   ADR number, bd issue/epic id), open PRs by number, and the moniker.
3. The critical first action or gate the next session must not skip.
4. Worktree enumeration per [ADR 0009](0009-worktree-disposal-ritual.md)
   (this remains — it is inventory the anchors don't carry).

## What a handoff must NOT contain

- Ledgers duplicating PR/CI/bd state that gh and bd answer live (they go
  stale within hours and then actively corrupt).
- Findings, evidence, decisions, or specs stated only in the handoff — if
  it matters across sessions, it gets a full-fidelity home first, then a
  pointer.
- Narrative reconstruction a future session is expected to re-expand.

## Consequences

- Writing a handoff gets cheaper; the cost moves to where it belongs —
  keeping anchors (specs, plans, ADRs, bd issues) fully described at the
  moment the state exists.
- A degraded or context-poor reader recovers state by following pointers
  and running live queries (`bd show`, `gh pr list`, `git log`, grep), not
  by trusting prose.
- CLAUDE.md §Session Completion step 6 is amended in the same PR as this
  ADR; AGENTS.md carries the parity line. Per the propagation contract,
  this ADR is canonical and those are pointers.
