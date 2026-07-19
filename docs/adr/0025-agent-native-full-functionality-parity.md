# 25. Agent-native: a feature's complete functionality is reachable by the agent, by design

Date: 2026-07-19
Status: Proposed (drafted per operator direction; awaiting operator review)
Deciders: cameronzucker (N7CPZ), isthmus-sage-owl (authoring session)

## Context

Tuxlink's thesis is one radio backend that a human AND an autonomous agent
(Elmer, over MCP) drive as co-equal operators. During the Overwatch design
(2026-07-19) that thesis was stress-tested and found to be, in places, aspiration
rather than fact — and the gap was instructive enough to name as a principle.

**The precipitating finding.** A frontier model, with full source access and a
night of grounding, repeatedly concluded that agent-driven radio tasks (tune an
AM broadcast station, rank candidates by signal strength) were infeasible —
because the MCP tool surface exposes `rig_tune` (frequency-only, carrying a
baked-in 1500 Hz Winlink-gateway sideband offset) and `rig_status` (no S-meter).
The underlying CAT layer is rich: `tux-rig` has `set_mode`, rigctld reports signal
strength via `l STRENGTH`, all exercised directly the same night. But **the agent
cannot read source at runtime.** The MCP tool surface is the agent's entire
reality; unexposed capability does not exist for it, and use-case-shaped tools
actively mislead. If a frontier model needed to grep to find the path, the
shipped local model (Qwen 3.5 122b-class) has no chance: it will call
`rig_tune(1070000)`, mis-tune by 1.5 kHz in the wrong mode, and hallucinate
success. "The model will figure it out" is not a shippable strategy.

**The architectural diagnosis.** Legacy enterprise software terminates at the
*database*; a UI is bolted on as forms-over-data, an API is added later (shaped
by whichever integration a deal required), and an agent is finally grafted onto
that third-class API. The agent receives leftovers of leftovers of leftovers —
four layers of afterthought, each lossy, none designed with the next in mind.
That is the dominant architecture of enterprise software, and it is why bolt-on
agents are both anemic (arbitrary subset of capability) and unreliable (each
sediment layer carries its own partial validation, tuned for the layer above,
not for an autonomous caller).

**Relationship to [ADR 0024](0024-dual-actionability-one-capability-tree.md).**
ADR 0024 (Dual actionability) requires every operator-meaningful capability to
have a *counterpart* on both front-ends: an agent MCP tool AND a human Routines
action. That is parity-of-**existence**. The Overwatch finding exposes the layer
beneath it: `rig_tune` *exists* — it passes 0024's counterpart check — yet it
exposes only the Winlink slice of the radio. Parity-of-existence is not
parity-of-**completeness**. This ADR is the general principle that 0024 is an
instance of: the counterpart must project the capability's *whole* functionality,
from a core designed to be projected, or the feature is not agent-native. (0024
arose from the Routines arc; this ADR from the Overwatch arc — two workstreams
converging on the same truth.)

## Decision

**Tuxlink is agent-native, not agent-bolt-on. A feature's definition-of-done
includes the agent reaching that feature's COMPLETE functionality, by design, at
conception.**

1. **Capability lives in a headless core; the human UI and the agent tool
   surface are co-equal PROJECTIONS of it.** Neither front-end is privileged.
   Capability that lives in the presentation layer (UI event handlers,
   screen-shaped Tauri commands) is structurally un-projectable to the agent and
   is the sediment anti-pattern in miniature. Tuxlink's ports/adapters structure
   (`tuxlink-mcp-core` traits + `Monolith*Port` adapters) is already the right
   shape; the discipline is to keep capability in the core, not to leak it into a
   front-end.

2. **Completeness, not just existence.** A tool that exposes a UI-era slice of a
   capability (the `rig_tune` Winlink slice) does not satisfy this ADR even
   though it satisfies 0024's counterpart check. The agent-facing surface must
   express what the agent is *trying to do* (operate the radio: tune + set mode
   incl. AM/FM, read signal, scan), described in agent-task terms, not as a thin
   wrapper of whatever a past screen needed.

3. **Same-severity rule.** "The agent cannot do X" is a defect of the same
   severity as "the human cannot do X." There is no tier in which agent
   reachability is a nice-to-have or a later audit. A feature the human UI
   reaches but the agent cannot is NOT shipped in an agent-native product.

4. **Test reachability with the SHIPPED model.** Agent-facing completeness is
   verified by having the shipped local model (not a frontier model) accomplish
   the task through the tools it can see. If it cannot, the tools are wrong,
   regardless of what the core or a frontier model can do.

## Consequences

- **`wire-walk` gains an agent lane.** It currently traces the human's flows
  greenfield; it must ALSO trace the agent's flows to full functionality. A
  feature is not wire-walked until both the human path and the agent path reach
  the whole capability. (Propagation site: the `wire-walk` skill.)
- **`features-shipped-end-to-end` includes the agent path.** "Shipped" = both
  the human and the agent reach full functionality.
- **Feature design asks the agent-surface question at conception**, co-equal
  with UI design (office-hours / spec / plan): "what is the agent's
  full-functionality surface for this feature?" — so NEW features are born
  native and never need a retrofit audit.
- **The legacy 82-tool surface predates this discipline.** `tuxlink-to358` (the
  agent-shaped MCP surface audit) is reclassified from an enhancement to
  **remediation of incomplete features** — the radio, and any capability exposed
  only as a UI-era slice, was half-built. Same severity as any other
  incomplete-feature defect.
- **This is the moat, stated as an invariant.** A sediment-architected
  competitor can bolt on an agent but never grant it parity, because the
  capability core was designed to terminate at a database/UI. Native parity is a
  foundation, not a feature to copy — and it is the precondition for anything
  like the unattended-operator autopilot.

## Alternatives considered

### A. Fold this into ADR 0024
Rejected: 0024 is scoped to counterpart-existence across the two front-ends,
driven by the scenario corpus, and is already Proposed/awaiting review. The
completeness/architecture/severity principle is broader and 0024 is an instance
of it; conflating them buries the general rule inside a specific parity check.
0024 stands as the surface-parity application of this ADR.

### B. Treat it as a one-time surface audit (`to358` only), no principle
Rejected: an audit pays down today's debt but institutionalizes the bolt-on
posture (build for UI, expose to agent later). Without the definition-of-done
invariant, every new feature re-accrues the debt.

### C. Rely on the shipped model being capable enough to reason around thin tools
Rejected: the precipitating finding is precisely that it will not. Designing the
surface to depend on the caller's cleverness is the failure mode, not the fix.

## Numbering note

ADR 0024 is the highest existing number; this is 0025. It generalizes 0024;
0024 is not superseded — it becomes the parity-of-existence application of this
principle.
