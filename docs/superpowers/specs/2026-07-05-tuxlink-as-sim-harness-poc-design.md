# Tuxlink-as-simulation-harness — PoC slice design

- **Date:** 2026-07-05
- **bd issue:** tuxlink-cnz5o (architecture / feature, P1, in_progress)
- **Branch / worktree:** `bd-tuxlink-cnz5o/tuxlink-as-sim-harness`
- **Status:** design — pending operator review, then ADR 0021 seed + adversarial review + plan
- **Agent:** delta-basil-fen

## Problem

Elmer distillation trains and evaluates an agent against a **Python simulator**
(`dev/elmer-distill/src/elmer_distill/simulator.py`) that reimplements the Tuxlink
MCP router's read-path logic in a second language: deterministic gateway synthesis
(`build_gateways`), position and rig state, catalog and search returns, session
state. This reimplementation must be held in parity with the real Rust router by
hand. Two costs follow:

1. **Drift.** Any behavior the Python reimplementation gets subtly wrong, or fails
   to track as the Rust router evolves, teaches the student a distribution the real
   application never produces. The student then confabulates against the real tool
   surface it meets at inference time.
2. **Fabrication at the tool boundary.** Where the simulator lacks a real model for a
   tool, it returns a success marker (`{"ok": true, "note": "... stub"}`) or a
   memorizable constant instead of a structured domain return. The agent reasons over
   fake station lists and fake radio state and learns to invent, because the training
   environment rewards invention.

A legacy driver (`dev/elmer-distill/reference/harness.py`) exhibits the `{ok:true}`
fall-through in its starkest form (`harness.py:146`). The active grading path uses the
stateful `simulator.py`, which is grounded for stations and position but remains a
parallel reimplementation subject to both costs above.

## Thesis

Make the real Rust MCP router the training and evaluation environment. Inject
scenario-driven synthetic state at the **tool-result port boundary** of the real
router, not at the transport or protocol layer. The router's own curation, distance,
staging, and state logic then runs for real against synthetic inputs, so parity with
the shipped application is tautological: there is no second implementation to drift.

Agentic behavior is a function of tool **results**, not modem internals. Injecting at
the port boundary is sufficient to reproduce the training-relevant behavior while
leaving transport untouched.

### Unification (full vision, not this slice)

One scenario artifact serves four uses: train the student, gate regressions per build,
reproduce field agentic bugs end to end, observe a live agent. This design covers only
the proof-of-concept slice that establishes whether the port-boundary approach is
faithful enough to be worth the full build.

## Scope of this PoC slice

In scope:

1. A **scenario fixture schema** that extends the existing `Scenario` / `SuccessSpec`
   dataclasses with a `world` block describing what each mock port returns.
2. **Scenario-driven mock ports** in `tuxlink-mcp-testserver` that load a fixture and
   return its synthetic state instead of hard-coded values.
3. An **MCP agent driver** that drives an OpenRouter model against the real testserver
   over its Unix-domain socket, using the real tool schema.
4. A **fidelity harness** that runs the same model and the same two scenarios through
   both the Python simulator and the real-MCP testserver, scores both with the same
   judge, and reports where the two environments diverge.
5. A small **`simulator.py` world-seeding change** so that, for a scenario carrying a
   `world`, the Python arm reads its station, rig, modem, and position state from the
   fixture rather than from its native generator. This makes the paired comparison
   controlled (both arms see identical ground truth).
6. **Two data-fabrication scenarios** on the easy tier: station-data and radio-state.

Out of scope for this slice (deferred to the full build or excluded entirely):

- The full train / regress / bug-repro / observe unification.
- Deep transport or protocol simulation.
- Any change to production transmit paths. The testserver is a separate binary; the
  production monolith never links mock code.
- **Refusal or restraint training of any kind.** Injection resistance is handled by
  engineering and administrative controls (the egress guard, operator arm), per the
  operator decision recorded on tuxlink-grg1i (closed) and tuxlink-0mudm. The harness
  does not train, reward, or grade refusal behavior.
- **Egress-guard and taint fidelity.** Per operator direction (2026-07-05), the
  fidelity comparison targets raw data-return fabrication only. The guard path is
  where the Python simulator and the real router already agree; it is not measured
  here.

## Architecture

### Injection seam

The real router (`src-tauri/tuxlink-mcp-core/src/router.rs`) dispatches each MCP tool
to a port trait method and JSON-encodes the returned DTO. The port traits
(`src-tauri/tuxlink-mcp-core/src/ports.rs`) are held as `Arc<dyn Port>` in `McpState`.
The `tuxlink-mcp-testserver` binary already wires `McpState` with hard-coded `Mock*`
ports (`src-tauri/tuxlink-mcp-testserver/src/mocks.rs`) while constructing the real
`EgressGuard`.

The seam is therefore the mock port implementations. The PoC evolves the relevant
`Mock*` ports so their return values come from a loaded scenario fixture rather than
from constants. The real router and the real guard are reused verbatim. No code is
added to `router.rs`, and no cargo feature is toggled in the production monolith. The
change is confined to the testserver crate.

Ports touched by this slice (data-return, read-path only):

- `StationPort` — synthetic gateway list for the station-data scenario.
- `StatusPort` — synthetic `RigStatusDto`, `ModemStatusDto`, `PositionStatusDto` for
  the radio-state scenario.

Ports not touched: `EgressPort`, `AbortPort`, `WritePort`, `ComposePort`,
`MailboxPort`, `SearchPort`, `ConfigPort`, `DevicePort`, `PredictionPort`.

### Fixture loading

The testserver gains a single new environment variable, `TUXLINK_TEST_SCENARIO`,
holding a path to a fixture JSON file. On startup, the testserver deserializes the
fixture and seeds the scenario-driven mock ports before serving. Absence of the
variable preserves the current hard-coded mock behavior, so existing testserver users
are unaffected.

### Scenario fixture schema

The fixture extends the existing `Scenario` / `SuccessSpec` shape (`scenario.py`) with a
`world` block. The scenario's grading spec, prompt, and provenance are unchanged. The
`world` block carries the synthetic port state:

```
world:
  stations: [ { call, grid, freq_khz, distance_km, bearing_deg, last_heard_h, reachable }, ... ]
  rig:      { vfo_hz, mode, ptt, configured }
  modem:    { backend, state, ... }
  position: { grid, gps_fix, source, precision }
```

The `world.stations` list is the single source of truth for both the mock
`StationPort` return and the judge predicate ground-truth (`references_real_gateway`).
One artifact drives both the environment and the grader, which is the unification
principle in miniature.

The Rust fixture types and the Python fixture types describe the same JSON. A shared
schema test asserts round-trip agreement so the two sides cannot silently diverge on
field names or shapes.

### MCP agent driver

The existing OpenRouter loop (`dev/elmer-distill/reference/harness_oai.py`) already
speaks the OpenAI-compatible tool-call protocol against a chat endpoint whose tools it
supplies from a static list. The driver is adapted to instead:

1. Connect to the testserver over its Unix-domain socket (`TUXLINK_MCP_SOCK`) as an MCP
   client.
2. Enumerate the real tool schema from the server rather than a static file.
3. Run the model → tool-call → tool-result loop, forwarding each tool call to the
   server and each real result back to the model.
4. Capture the full transcript and the resulting server-side state for grading.

The model-facing loop (prompt, turn cap, transcript capture) is preserved so the two
environments differ only in where tool results originate.

### Fidelity harness

The same scenario `world` seeds **both** arms. This is the controlled-experiment
requirement: if Arm A generated its own data (for example via `build_gateways`) while
Arm B returned fixture data, any divergence would conflate a difference in underlying
data with a difference in backend fidelity. Seeding both arms from one `world` isolates
the variable under test — where a tool result originates — so a divergence is
attributable to fabrication or shape, not to different ground truth. This requires a
small `simulator.py` change: for a scenario carrying a `world`, the simulator reads its
station, rig, modem, and position state from the fixture rather than from its native
generator.

For each of the two scenarios, the harness performs a paired run:

- **Arm A (Python):** the model runs against `simulator.py`, seeded from the scenario
  `world`, graded by `judge.py`.
- **Arm B (real MCP):** the same model, same prompt, same seed runs against the
  testserver seeded from the same `world`, graded by the same `judge.py`.

The harness emits a divergence report per scenario:

- verdict delta (pass/fail in A vs B),
- reason delta (which rubric points fired differently),
- tool-sequence delta,
- a cause tag for each divergence (fabricated-data, missing-field, shape-mismatch,
  stochastic).

To control model stochasticity, each arm runs N samples at fixed temperature and the
report aggregates over samples. N is a harness parameter; the PoC default is small
(for example 3) and stated in the report.

## The two PoC scenarios

Both are easy tier, both target data-return fabrication, neither involves egress.

1. **Station-data (`sim-station-fabrication-1`) — the control.** Prompt asks the
   operator's assistant to identify usable gateways for a message. Success spec requires
   the answer to cite real gateways from `world.stations` (the `references_real_gateway`
   predicate). The active `simulator.py` already grounds station data, so with both arms
   seeded from the same `world` this scenario is expected to show low divergence. Its
   role is the control: it demonstrates that the method does not manufacture false
   divergence where the Python sim was already faithful. A large divergence here would
   itself be a finding (a shape or field mismatch between the sim and the real
   `StationPort`).

2. **Radio-state (`sim-radiostate-fabrication-1`) — the fabrication demonstrator.**
   Prompt asks a question answerable only from live radio and modem state (for example
   the current band or mode, or whether the modem is connected). The active
   `simulator.py` does not model rig or modem state and falls through to an `{ok:true}`
   style return, so the Python arm gives the model nothing real to ground on and rewards
   confabulation. Arm B returns a structured `StatusPort` DTO, or a genuine null for an
   absent field. Success spec requires the answer to reflect `world.rig` / `world.modem`
   and to decline when a field is absent rather than invent a value. This scenario is
   where the fabrication divergence is expected to appear.

## Fidelity metric and success criteria

The PoC answers one question: **does the real-MCP arm eliminate data-fabrication
divergences that the Python arm exhibits, and is the residual divergence between the
two arms small enough to trust the port-boundary approach?**

Interpretation:

- **Material fabrication divergence found and eliminated by Arm B** validates the full
  cnz5o build: the Python reimplementation was teaching invention that the real router
  does not.
- **Negligible divergence** indicates the active Python simulator was already faithful
  on these axes, and the full build is lower priority than assumed.
- **Divergence caused by the real router behaving differently than expected** is itself
  a finding: it locates a real-router behavior the Python sim mismodeled.

The PoC succeeds as an experiment regardless of which outcome it produces. Success is a
defensible measurement, not a particular verdict.

## Testing approach

Test-driven throughout. Before implementation, read `.claude/skills/test-driven-development`
and `docs/pitfalls/testing-pitfalls.md`.

Rust (testserver):

- Fixture deserialization: a malformed fixture fails loudly at startup, not silently at
  first tool call.
- Scenario-driven `StationPort` returns exactly the `world.stations` entries.
- Scenario-driven `StatusPort` returns the `world.rig` / `world.modem` / `world.position`
  values, and returns a genuine null (not a fabricated default) for absent fields.
- Absent `TUXLINK_TEST_SCENARIO` preserves the current hard-coded mock behavior.

Cross-language:

- A schema round-trip test asserts the Rust fixture types and the Python fixture types
  agree on the same JSON sample.

Python (harness):

- The MCP driver forwards a tool call and returns the server's real result unmodified.
- The fidelity harness produces a divergence report with the four delta sections for a
  fixture pair.

## Risks and open questions

- **Rust build cost.** The testserver is Rust and the Pi does not compile Rust
  comfortably. Build and verification run on R2. The plan states the build/verify
  target explicitly and does not assume local compilation on the Pi.
- **rmcp client ergonomics.** The driver must speak the same MCP wire protocol the
  testserver serves. If a suitable Python MCP client is not already available in the
  harness environment, the plan accounts for adding one.
- **Judge portability.** `judge.py` grades a transcript plus final state. Arm B must
  surface the equivalent final state (staged items are not exercised here, so the
  read-path state is small). The plan verifies the judge consumes Arm B output
  unmodified.
- **Scenario realism.** Two scenarios cannot settle the bd issue's open scope question
  (what fraction of scenario space is reproducible at the port boundary). They
  establish the method and a first data point. Generalization is future work.

## R2 verification

The testserver is built on R2. The two fixtures are loaded, the testserver is served
over a Unix-domain socket, and the fidelity harness runs both arms against an OpenRouter
model. The divergence reports for both scenarios are the verification artifact. The
operator conducts the wire-walk of the agent flows before the work is marked ready.
