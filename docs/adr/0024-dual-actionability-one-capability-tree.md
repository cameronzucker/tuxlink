# 24. Dual actionability: one capability tree, two front-ends

Date: 2026-07-18
Status: Proposed (drafted per operator direction in bd tuxlink-iizmk; awaiting operator review)
Deciders: cameronzucker (N7CPZ), chasm-marsh-heron (authoring session)

## Context

Tuxlink has grown two ways of *doing things*: the agent tool surface (50 MCP
tools, `dev/elmer-distill/reference/tools.json`) and the Routines action
surface (17 actions, `src-tauri/src/routines/actions/`). They were built in
different arcs, by different criteria, and they have drifted into two
overlapping but incompatible capability sets.

The drift is now measured, not felt. The distillation scenario corpus
(`dev/elmer-distill/src/elmer_distill/scenariogen.py`) is a vetted bank of
operator-realistic multi-step jobs (radio debugging, EmComm traffic handling,
helpdesk triage, and blends), each with a machine-checkable SuccessSpec. The
2026-07-18 compatibility tree
([spec](../superpowers/specs/2026-07-18-routines-round2-compat-tree.md))
mapped every scenario step to a Routines action or MISSING:

- **0 of 24 scenario cells are human-actionable through Routines today.** Every
  family is blocked at depth 2, mostly by absent *read* actions (modem status,
  gateway-directory query, config reads, docs search).
- The reverse direction also gaps: ten Routines actions (APRS send, listen
  dwell, WWV space-weather decode, tactical identity, and others) have no
  agent-tool counterpart, so the agent cannot do things a routine can.

This matters because of who the requirements come from. The operator's work
context is deliberately firewalled from this project; requirements for what a
station operator needs to automate come from the vetted scenario corpus, and
taste comes from the operator's critique loop on rendered artifacts. If a
scenario is worth teaching the agent to do, it is by the same evidence worth
letting a human do without the agent: on a desert ridge with no LLM reachable,
the human IS the fallback executor. A capability that exists only behind the
agent is a single point of failure; a capability that exists only as a routine
action is invisible to the agent that is supposed to help run the station.

## Decision

**Operator-meaningful capabilities live in ONE capability tree, and every
capability in it is actionable by BOTH front-ends: the agent (MCP tool) and
the human (Routines action and/or direct UI).**

1. **Requirements flow from the scenario corpus.** A scenario cell that the
   agent is scored on must be expressible as a routine a human can build and
   run. The compat-tree spec's ranked missing-action list is the requirements
   backlog for the Routines side, and its reverse diff is the backlog for the
   next agent tool-surface revision.
2. **New capabilities ship dual-actionable.** Adding an agent tool without the
   corresponding Routines action (or a named, recorded exception) is the same
   defect as adding a Routines action the agent cannot invoke. The exception
   list lives in this ADR and is expected to stay short; the current entries
   are the six not-routine-shaped tools from the spec's surface diff
   (first-run wizard state, design-time device enumeration, and stop verbs
   already owned by run cancel + arbiter release).
3. **Authority semantics are part of the capability, not the front-end.** The
   agent surface's classification (taint reads, egress, tier-2 writes,
   staging, stop) and the Routines capability flags (`needs_radio`,
   `transmits`, `needs_internet`) are two projections of one authority model.
   A capability's gating must agree across front-ends: a tier-2 write is
   consent-relevant whether the agent calls it or a routine step runs it, and
   staging is always allowed on both. Divergence here is a security defect,
   not a style issue.
4. **One implementation, two registrations.** Both front-ends delegate to the
   same service seam (the pattern the Routines action registry already uses).
   Front-ends may differ in shape (the agent's `cms_connect` +
   `ardop_connect` fold into the routine `radio.connect` station x band walk)
   as long as the underlying capability and its authority class are the same;
   what is banned is a capability implemented twice or reachable once.

## Consequences

- Round 2 of Routines has an objective requirements list: the five ranked
  read/write action families in the compat-tree spec, sized by how many
  scenario cells each unblocks (14, 10, 10, 6, 3). "Done" for round 2's
  functional half is measurable: the 24-cell coverage number moves.
- The scenario corpus becomes a standing compatibility test between the two
  front-ends. Re-running the mapping after each surface change is cheap (the
  corpus is 11 distinct tools today) and catches drift the way the holdout
  split catches memorization.
- The agent tool surface gets a deliberate next revision from the reverse
  diff (listen, APRS send, WWV decode, tactical identity) instead of growing
  by accretion.
- The first config-write action family (rank 5) forces the consent-parity
  design work (point 3) early, on the smallest honest slice: the ARDOP
  drive-level write the corpus already requires.
- Validator and consent closure gain a cross-surface invariant worth a CI
  check later: every registered capability's authority class must be declared
  identically in both registries, and every non-excepted capability must be
  registered in both.
