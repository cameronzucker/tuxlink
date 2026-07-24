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

## Amendment 2026-07-24: the quality dimension (structural safety over model-parsed advisories)

Amended by tanager-owl-cardinal per operator direction during the Build-Carefully
lift session, and CORRECTED the same day: a two-front adversarial convergence check (a
blind Codex re-derivation from the raw run data, plus an independent source-grounded
critique) overturned this amendment's original precipitating example. The correction is
preserved below as the honest record, per the project's evidence discipline.

The original ADR establishes parity of *functionality* (the accessibility dimension:
the agent can reach a capability's complete behavior). This amendment adds the second
facet of the same bar: parity of *feedback quality*. When the agent uses a capability
and something is wrong, the diagnostic it receives is held to the same design standard
as the human-facing one. These are two facets of one invariant, which is why they live
in one ADR.

### A corrected precipitating finding

The first draft of this amendment cited lift cell base/S1 as a missing-typed-remedy
defect: an `ARM_FALLTHROUGH_LEAK` warning named its fix only in prose,
`disposition.remedies` was `[]`, and the model looped to the turn cap, so (the draft
argued) authoring findings should carry typed remedies the way the consent findings do.
**That diagnosis was wrong**, and three sources agree:

- `ARM_FALLTHROUGH_LEAK` is a **warning**, and base/S1's routine was already **valid**
  (`validates_green`).
- `AuthoringDispositionDto::classify` returns `remedies: []` for any warning-only
  routine **by deliberate design**: a tested anti-ping-pong invariant
  (`tuxlink-mcp-core/src/ports.rs:1644`, plus a test at `ports.rs:2214` asserting
  "no remedy for an acceptable warning (kills the ping-pong)"). Emitting a typed remedy
  for a non-blocking warning would **reintroduce** the ping-pong the invariant prevents.
- base/S1's real mechanism is a **model non-termination** (tuxlink-m5oia): it re-sent a
  byte-identical no-op patch ~35 times, each correctly reported `applied: false` (the
  m5oia idempotency guard, `src/routines/commands.rs:911`), while ignoring both that
  signal and the `state: valid` disposition telling it to stop. skill/S1 hit the same
  warning on the same prose and resolved it. So base/S1 is a model/teaching failure, not
  a remedy-typing gap.

The quality dimension survives the correction; only the product lever moves. Where a
diagnostic names a fix the agent *should* make, the parity-respecting move is
**structural**, not "narrate a remedy and hope the model parses it and does not loop."

### Decision (the quality dimension, corrected)

Add to the Decision above:

5. **Prefer making the unsafe shape unrepresentable, or blocking, over surfacing a
   non-actionable advisory the weak model must judge.** For the recurring branch
   fall-through footgun (`ARM_FALLTHROUGH_LEAK` and the `BRANCH_*` family): either make
   the safe shape the default (auto-terminate a `then` arm unless the author opts into a
   shared tail), or, where fall-through genuinely corrupts intent (a transmit-bearing
   arm leaking into another), **escalate it from a warning to a blocking error** so it
   is not an "acceptable terminal state" the floor model has to recognize and leave
   alone. The warning-with-no-remedy design is *correct* for true advisories; it is
   *wrong* only when the fall-through actually breaks the routine.

6. **Typed remedies belong on BLOCKING findings whose fix is a concrete op, and only
   there.** The consent findings already do this correctly: `AUTO_TX_UNACKED` ships
   `remedies: [operator_acknowledge, set_attended]` with `agent_terminal: true`
   (`ports.rs:1658`). That is the LSP-`CodeAction` pattern applied where it belongs: a
   *blocking* diagnostic carries its fix as structured data; the operator-only branch
   stays consistent with ADR 0024's `operator-authority` class. Warnings stay
   remedy-free under the anti-ping-pong invariant. The original draft's "populate
   `remedies` for all authoring findings" instruction is **withdrawn** as it conflicted
   with that invariant.

7. **A surface the model reliably mis-navigates is a product signal even when the model
   is at fault, but the lever is footgun-reduction, not narration.** base/S1 (ignored
   `applied: false` + `state: valid`) and skill/S3 (declared a buildable task
   infeasible) are model errors; the base-vs-skill delta (skill passed what base failed
   on identical tooling) shows the surface is navigable. The product lever is reducing
   the number of judgment calls the surface demands (safer defaults, fewer footguns,
   aligned cross-surface vocabularies) plus the teaching/skill layer, not adding prose.

### Consequences (corrected)

- **First enforcement targets (tuxlink-90vcc re-scoped).** The branch fall-through
  affordance (safe-default or blocking escalation), and the cross-surface vocab/semantics
  drifts the lift surfaced: `vara-fm` vocabulary mismatch between the MCP and routine
  `find_stations` surfaces; `params` patch replace-vs-merge semantics; `rig.tune_atu`
  advertised but runtime-unimplemented with no validation flag; `data.read source:grid`
  exposing no per-source output to reference. These are structural affordance/error
  fixes, not narration.
- **No new per-turn context tax** (unchanged). Typed remedies ride in tool *results* on
  *blocking* findings only, never in tool *schemas*, so ADR 0027's context-budget
  objection does not apply.
- **`predict_path` Routines-action parity** (ADR 0024) remains a genuine capability gap,
  confirmed by both fronts of the convergence check.
- **Reachability is still tested with the shipped model** (Principle 4): the floor model,
  handed the surface, must navigate it; where it cannot, prefer a safer default over more
  words.
- **Method note.** This amendment's own correction is the evidence for a process rule: an
  agent-authored parity finding is grounded against source and cross-checked before it
  drives a build. The original draft would have added a typed remedy that violated a
  tested invariant; the two-front convergence check caught it pre-build.
