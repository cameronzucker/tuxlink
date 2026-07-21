# 27. Agent/human parity is CI-enforced through a classified command manifest

Date: 2026-07-21
Status: accepted

## Context

ADR 0025 makes agent reachability of complete feature functionality a
definition-of-done invariant, and ADR 0024 requires operator-meaningful
capabilities to live in one capability tree actionable by both surfaces.
Both were prose. On 2026-07-21 an operator question exposed two live parity
gaps neither review nor the wire-walk had caught: the routine designer's new
radio surface gave humans authoring-time transport warnings with no agent
equivalent anywhere, and the designer promoted favorites — a store with zero
MCP exposure — to a first-class human authoring source. This repeats the
project's founding enforcement lesson (ADR 0008's grounding incident,
standing-conventions §1): prose alone does not prevent the failure class;
the mechanical layer does.

The naive escalation — "every command must have a tool" — was rejected for a
concrete cost the operator named: tool schemas are a permanent context tax
paid by every agent turn, heaviest for the self-hosted models this project
targets, and tool proliferation demonstrably *hurts* small-model capability
(the 2026-07-19 exam evidence: a model failing to find real control-flow
capability among nineteen routines tools). A parity gate that pressures
toward minting tools would damage the very parity it enforces.

## Decision

A machine-readable manifest, `docs/parity/parity-manifest.json`, classifies
**every** Tauri command registered in `generate_handler!` into exactly one
class, and CI (the existing `verify` job — a Rust test module and a vitest)
enforces the classification's invariants.

**The unit of parity is the capability, not the command.** Three classes are
terminal by design and never map to an agent path:

- `chrome` — app/window/session mechanics (quit, tray, window opens, print
  dialogs). No parity obligation exists or ever will.
- `presentation` — UI-state persistence and observability chrome (pane
  state, saved-search bookkeeping, logging panel controls).
- `operator-authority` — consent grants, acknowledgments, egress arming,
  identity and credential custody. These must **never** gain an agent path:
  an `mcp` mapping on this class is a CI failure, making the manifest a
  mechanical defense of ADR 0024's authority parity (generalizing the
  routines router's existing pinned-closed guard).

The fourth class, `capability`, requires an agent path in one of four
forms, cheapest-first by design so compliance pressure points toward
consolidation, not tool minting:

1. `{"mcp": "<tool>"}` — a live tool (existence CI-verified against the
   router source).
2. `{"mcp-field": "<tool>.<field>"}` — a field on an existing tool's result.
3. `{"finding": "<CODE>"}` — a validation finding both surfaces read.
4. `{"pending": "<bd-id>"}` — a tracked gap. The initial backfill cites
   tuxlink-to358 (the standing MCP-surface audit); a **new** pending entry
   is legal only when it carries its own bd id, so gaps are tracked at
   birth rather than discovered at audit.

**The counter-ratchet:** the manifest records `tool_budget`, asserted equal
to the router's actual tool count by CI. Growing the tool surface requires
editing the budget in the same PR — the schema tax becomes a visible,
operator-owned line item instead of silent accumulation. The budget freezes
at 92 (the 2026-07-21 count).

## What this deliberately does not enforce

Semantic completeness of an exposed tool (use-case-crippled surfaces),
derived-display asymmetries where both surfaces reach the data but in
unequal forms, and discoverability quality. Those remain wire-walk and
audit territory (tuxlink-to358). The gate makes the enumerated failure
classes impossible and the remainder visible; it does not replace judgment.

## Consequences

- Every new command lands classified or CI fails; every new tool debits a
  visible budget; operator-authority boundaries are machine-defended.
- The 197 initial `pending` entries convert to358's unknown-unknowns into
  an enumerated worklist; the ratchet direction is shrink-or-hold.
- The manifest is ADR 0025's machine-readable propagation. Per the
  propagation contract, CLAUDE.md carries one pointer; this ADR is
  canonical.

## Alternatives considered

Per-command tool parity (rejected: token tax + small-model discoverability
damage, above). A standalone CI workflow (rejected: the verify job already
runs both test suites; a new workflow adds surface without adding
enforcement). Schema-bytes budget instead of tool count (rejected: it would
pressure the teaching descriptions the exam evidence shows are
load-bearing).
