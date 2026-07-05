# 21. Tuxlink as its own simulation harness (port-boundary result injection)

Date: 2026-07-05

Status: Proposed (full build gated on the PoC fidelity result)

Deciders: Cameron Zucker (operator), delta-basil-fen (agent)

## Context

Elmer distillation trains and evaluates an agent against a Python simulator
(`dev/elmer-distill/src/elmer_distill/simulator.py`) that reimplements the
Tuxlink MCP router's read-path logic in a second language: deterministic gateway
synthesis, position and rig state, catalog and search returns, session state. A
second, legacy driver (`dev/elmer-distill/reference/harness.py`) returns a bare
success marker (`{"ok": true, "note": "... stub"}`) for any tool it does not
model.

Maintaining a parallel simulator carries two costs:

1. **Drift.** Any behavior the reimplementation gets subtly wrong, or fails to
   track as the Rust router evolves, teaches the student a distribution the real
   application never produces. The student then confabulates against the real
   tool surface it meets at inference time. Parity is a manual, unenforced
   obligation.
2. **Fabrication at the tool boundary.** Where the simulator lacks a real model
   for a tool, it returns a success marker or a memorizable constant instead of
   a structured domain return. The agent reasons over fake station lists and
   fake radio state and learns to invent, because the environment rewards
   invention. This class of artifact (fabrication, stall, false-sent) is the
   suspected dominant driver of the distillation failures observed through
   iter-3.

The router already exposes a clean dependency-injection seam. Each MCP tool in
`src-tauri/tuxlink-mcp-core/src/router.rs` dispatches to a port trait method
(`src-tauri/tuxlink-mcp-core/src/ports.rs`, roughly twelve `Arc<dyn Port>`
traits held in `McpState`) and JSON-encodes the returned DTO. A separate
`tuxlink-mcp-testserver` binary already wires `McpState` with hard-coded `Mock*`
ports while constructing the real `EgressGuard`. The production monolith never
links the testserver or its mocks.

Refusal and restraint training are out of scope for this and every Elmer work
item. Injection resistance is owned by engineering and administrative controls
(the egress guard, operator arm), per the operator decision that closed
tuxlink-grg1i and rescoped tuxlink-0mudm. This ADR does not train, reward, or
grade refusal behavior.

## Decision

**1. The real Rust MCP router is the training and evaluation environment.** The
router's own curation, distance, staging, and state logic runs for real against
synthetic inputs. Parity with the shipped application becomes tautological
because there is no second implementation of that logic to drift.

**2. Synthetic state is injected at the tool-result port boundary, not at the
transport or protocol layer.** Agentic behavior is a function of tool results,
not modem internals. The hard-coded `Mock*` ports in `tuxlink-mcp-testserver`
evolve into scenario-driven fixtures: a mock port returns values loaded from a
scenario fixture instead of constants. No code is added to `router.rs`, and no
Cargo feature is toggled in the production monolith.

**3. The mechanism is additive and test-mode-gated by construction.** The
testserver is a distinct binary. A single new environment variable,
`TUXLINK_TEST_SCENARIO`, points the testserver at a fixture file. Absence of the
variable preserves the current hard-coded mock behavior. Production transmit
paths are never touched.

**4. One scenario artifact serves four uses** — train the student, gate
regressions per build, reproduce field agentic bugs end to end, observe a live
agent. The fixture that seeds a port also supplies the judge's ground-truth, so
environment and grader cannot disagree on the same run. This unification is the
long-term aim; it is not delivered by the proof-of-concept.

**5. The full build is gated on a proof-of-concept fidelity measurement.** The
PoC (design: `docs/superpowers/specs/2026-07-05-tuxlink-as-sim-harness-poc-design.md`)
drives the same OpenRouter model and the same two scenarios through both the
active Python simulator and the real-MCP testserver, grades both with the same
judge, and reports where the two environments diverge. A material,
fabrication-class divergence eliminated by the real-MCP arm validates the full
build. A negligible divergence indicates the Python simulator was already
faithful on the measured axes and lowers the priority of the full build. The
experiment succeeds as a measurement regardless of which verdict it produces.

## Alternatives considered

### A. Keep maintaining the Python simulator (status quo)

Rejected as the standing cost this ADR exists to remove. The simulator must be
hand-kept in parity with an evolving Rust router, and it fabricates where it
lacks a model. The status quo is the source of the drift and fabrication
described in Context.

### B. Deep transport or protocol simulation

Rejected. Simulating the modem, TNC, or CMS wire protocol would be a large build
and would not change what determines agentic behavior. The agent reacts to tool
results, not to modem internals. Injecting at the result boundary reproduces the
training-relevant behavior at a fraction of the cost.

### C. Tool-level injection inside the router (`#[cfg(test)]` in `router.rs`)

Rejected. Adding a per-tool test-mode branch inside each tool method would
pollute the production tool implementations, require rebuilding the monolith
with test configuration to activate, and scale poorly (one environment variable
per tool). The port-boundary seam already isolates the substitution to the
testserver crate, which the production binary never links.

## Watched failure modes

- **Confounded comparison.** If the two arms do not share the same scenario
  `world`, a divergence conflates a difference in underlying data with a
  difference in backend fidelity. The same fixture must seed both arms; this
  requires a small change so the Python simulator reads fixture data rather than
  its native generator for a scenario carrying a `world`.
- **Cross-language schema drift.** The Rust fixture types and the Python fixture
  types describe the same JSON. A round-trip test asserts agreement so the two
  sides cannot silently diverge on field names or shapes — the very drift this
  ADR removes for router logic must not reappear in the fixture schema.
- **Testserver environment sprawl.** Adding one control variable per concept
  recreates the per-tool-flag problem of alternative C. Scenario state travels in
  the fixture file, not in a growing set of environment variables.
- **Overreading the PoC.** Two scenarios establish a method and a first data
  point. They do not settle what fraction of scenario space is reproducible at
  the port boundary. That question remains open and is answered by breadth added
  after the PoC, not by the PoC.

## Consequences

- Parity is tautological for any port covered by a scenario fixture: the real
  router logic runs, so the simulated result matches the shipped result by
  construction.
- The Python simulator retires incrementally. Each port converted to a
  scenario-driven fixture removes a slice of reimplemented logic that could
  drift.
- The fixture schema becomes the shared contract across training, regression,
  bug-repro, and observation. A bug report, a training scenario, and a
  regression fixture converge on one artifact type.
- The testserver is Rust and the Raspberry Pi does not compile Rust comfortably.
  Builds and verification run on R2. The plan states the build target explicitly.
- The egress guard, taint machinery, and transmit paths are unchanged. The
  testserver constructs the real guard; the fidelity comparison excludes the
  guard path by operator direction and targets data-return fabrication only.

## Propagation

Per the documentation propagation contract, the canonical sources are this ADR
(the decision) and the PoC design spec
(`docs/superpowers/specs/2026-07-05-tuxlink-as-sim-harness-poc-design.md`, the
detailed slice design). The work is tracked on bd issue tuxlink-cnz5o. No
parallel restatement in CLAUDE.md is warranted until the approach is accepted
past the PoC gate.
