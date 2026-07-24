# 25. Agent-native parity: a feature's complete functionality AND its diagnostics are reachable and actionable by the agent, by design

Date: 2026-07-19
Status: Proposed (drafted per operator direction; awaiting operator review)
Deciders: cameronzucker (N7CPZ), isthmus-sage-owl (authoring session)
Amended: 2026-07-24 (tanager-owl-cardinal) to add the second parity dimension, quality/diagnostic actionability, per operator direction. The original 2026-07-19 record is preserved intact below; the extension is the dated "Amendment 2026-07-24" section near the end. Parity now has two dimensions under one invariant: **accessibility** (can the agent reach the whole capability) and **quality** (is the feedback the agent gets held to the human-facing bar).

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

## Amendment 2026-07-24: the quality dimension (agent diagnostics carry structured remedies)

Amended by tanager-owl-cardinal per operator direction, during the Build-Carefully
lift session. The original ADR establishes parity of *functionality*: the agent can
reach a capability's complete behavior (the accessibility dimension). This amendment
adds the second dimension of the same bar: parity of *feedback quality*. When the
agent uses a capability and something is wrong, the diagnostic it receives is held to
the same design standard as the human-facing diagnostic. These are two facets of one
invariant (the agent front-end meets the human front-end's bar), which is why they
live in one ADR; a separate ADR for the quality dimension was drafted and rejected as
fragmentation of a single principle.

### Precipitating finding (Build-Carefully lift, 2026-07-24)

In the lift's S1 cell (build a scheduled dial-with-fallback routine) the shipped-class
local model (qwen-3.5-122b) built a branch whose `then` arm fell through into the
`else` arm. `routines_step_update` returned a complete, correct, plain-English
diagnosis, `ARM_FALLTHROUGH_LEAK`, whose message names the fix outright ("insert an end
control after the then arm's steps"). But the fix lived only in that sentence. The
response's `disposition.remedies` was `[]`, and no finding carried any structured
remedy field. To act, the model had to compile the prose into a `routines_step_add
{control: "end"}` call, a different tool than the `routines_step_update` it was holding.
It could not, and re-sent the same no-op patch 38 times (every response `applied:
false`, revision unchanged) until the turn cap cancelled the cell.

### The natural experiment that isolates the cause

Two same-run comparisons pin the defect to the diagnostic contract, not to model
weakness alone:

1. *Within S1, across arms.* The skill arm hit the identical finding, translated the
   prose into the correct `end`-control insertion, and completed in 11 calls with zero
   no-ops. The remedy was recoverable from prose by a sufficiently capable caller, and
   not by the floor model this project ships.
2. *Across finding types, within the base arm.* In the P3 cell the same base model hit
   `AUTO_TX_UNACKED`, a finding that (via #1254, tuxlink-kbh4t) carries a typed
   `disposition.remedies` entry. The base model applied it mechanically and reached an
   honest terminal. Same model, same run: structured remedy succeeded where prose-only
   remedy looped.

So the product already speaks two dialects: consent findings hand the agent a
code-action it applies; authoring findings hand it a paragraph and require it to
compile that paragraph into a call. Translate the S1 response into the human routine
designer and it is unshippable: a warning banner with a paragraph, the Save button
green, and no "Fix it" button. The mature standard for machine-surfaced diagnostics,
the Language Server Protocol, pairs a `Diagnostic` with `CodeAction` quick-fixes for
exactly this reason. The agent is not a second-class consumer that tolerates the legacy
bare-code-plus-lookup pattern. The reason this class went unseen is that an agent's
confusion is invisible where a human squinting at an actionless warning is not; the
lift is the instrument that made the squint visible.

### Decision (the quality dimension)

Add a fifth principle to the Decision above:

5. **A diagnostic surfaced to the agent is an interface, held to the same design bar as
   the human-facing diagnostic.** Whenever a finding has a mechanical fix, the finding
   carries that fix as a structured, machine-applyable remedy, the agent-facing
   equivalent of an LSP `CodeAction`, never as prose the caller must compile into a
   call.
   - **Populate `disposition.remedies`** with the typed shape #1254 established
     (`{actor, tool, patch, expected_revision, consequence, changes_behavior}`) for
     every finding whose remedy is a concrete edit. Prose in `message` stays for
     explanation but is never the sole carrier of an actionable fix.
   - **Operator-only remedies are typed, not omitted.** A fix that is the operator's to
     make (a design-time acknowledgment, a consent grant, a credential) is a remedy with
     `actor: "operator"` and `changes_behavior: false`, as the P3 `AUTO_TX_UNACKED`
     remedy already is. This keeps the surface consistent with ADR 0024's
     `operator-authority` class: the agent is told, in structured form, that the path
     forward is a human act it cannot perform.
   - **The verdict must be consistent with what action is required.** A response that
     reports `state: "valid", agent_terminal: false` while dangling warnings with no
     typed remedy is self-contradictory. Genuinely advisory findings must be explicitly
     ignorable so the correct behavior is "note it and terminate," not "try to clear
     it."
   - **Where the footgun can be made unrepresentable, prefer that over a better
     remedy.** If the engine can make the safe shape the default (auto-terminating a
     `then` arm unless the author opts into a shared tail), the finding never fires. The
     best code-action is the one made unnecessary.

### Consequences (the quality dimension)

- **No new per-turn context tax.** ADR 0027 rejected minting tools because tool schemas
  are a standing context cost that hurts small models. That objection does not apply
  here: remedies ride in tool *results*, not tool *schemas*. They cost tokens only on
  the turn a finding fires, and they reduce total tokens by preventing the multi-turn
  no-op loops this dimension exists to kill. This pushes in the same direction as 0027's
  cost argument, not against it.
- **First enforcement target: tuxlink-90vcc (P1).** Populate `disposition.remedies` for
  `ARM_FALLTHROUGH_LEAK` and the other prose-only authoring findings
  (`ATTENDED_UNDER_SCHEDULE`, `NEEDS_INTERNET_OFFGRID`, `NO_RIG_CONFIGURED`), and add a
  `reason` to `applied: false` no-ops.
- **CI enforcement is possible but not mandated here.** A check parallel to ADR 0027's
  parity manifest could assert that every finding `code` with a known mechanical remedy
  emits a structured remedy. This amendment sets the bar and leaves mechanization to a
  follow-up if drift recurs, matching 0027's "prose first, mechanize when prose proves
  insufficient" arc.
- **Reachability is still tested with the shipped model.** Principle 4 already requires
  verifying with the shipped local model; the quality dimension is verified the same
  way: the floor model, handed the diagnostic, must be able to act on it.
