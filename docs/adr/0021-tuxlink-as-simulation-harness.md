# 21. Tuxlink as its own simulation harness (port-boundary result injection)

Date: 2026-07-05

Status: Accepted (operator-directed full build, 2026-07-05). The fidelity
contract-diff and A/B measurement are validation outputs of the harness and
inform whether to later invest in below-seam logic parity; they do not gate
building the harness.

Deciders: Cameron Zucker (operator), delta-basil-fen (agent)

Reviewers: five-round Codex adversarial review, 2026-07-05 (agents
granite-ivy-sequoia, arroyo-poplar-sandbar, glade-opossum-basalt, and two
unnamed rounds). Findings are folded into the decision and the watched failure
modes below.

## Context

Elmer distillation trains and evaluates an agent against a Python simulator
(`dev/elmer-distill/src/elmer_distill/simulator.py`) and a legacy reference
driver (`dev/elmer-distill/reference/harness.py`). The active training/eval path
(`teacher.py` → `StatefulSimulator.apply`) returns `{"ok": true, "tool": name}`
for every unmodeled read tool — `find_stations`, `rig_status`, `modem_get_status`,
`position_status`, `solar_conditions`, `predict_path`. The model reasons over
these empty success markers and learns to invent structured domain data
(gateway lists, radio state, solar indices, path reliability) because the
environment rewards invention. Grounded station data exists only in the *legacy*
reference harness, not the active path.

Two costs follow from a separate simulator:

1. **Fabrication at the tool boundary.** Every `{ok:true}` return is a data void
   the model fills by confabulating. This is the suspected dominant driver of the
   distillation failures observed through iter-3.
2. **Drift.** Any behavior the simulator models is a second implementation that
   must be hand-kept in parity with the evolving Rust router.

The router already exposes a dependency-injection seam. Each MCP tool in
`src-tauri/tuxlink-mcp-core/src/router.rs` dispatches to a port trait method
(`src-tauri/tuxlink-mcp-core/src/ports.rs`) held as `Arc<dyn Port>` in
`McpState`. A separate `tuxlink-mcp-testserver` binary wires `McpState` with
hard-coded `Mock*` ports (`mocks.rs`) while constructing the real `EgressGuard`.
The production monolith wires the same slots with `Monolith*Port` adapters
(`src-tauri/src/mcp_ports.rs`, `src-tauri/src/lib.rs:1406`) and never links the
testserver.

A separate headless agent frontend already exists: `src-tauri/d3zwe` drives the
real bounded agent loop (`tuxlink-agent-runner`, shared with the Elmer pane)
against the MCP server over an rmcp Unix-domain-socket client
(`UdsToolInvoker`), using an OpenAI-compatible provider (`OpenAiProvider`, key
from `D3ZWE_API_KEY`), and prints the transcript and outcome.

Refusal and restraint training are out of scope for this and every Elmer work
item; injection resistance is owned by the egress guard and the operator (the
closed tuxlink-grg1i, the rescoped tuxlink-0mudm). This ADR does not train,
reward, or grade refusal.

## What runs for real, and what does not (parity, precisely)

The adversarial review established that the business logic for data-return tools
lives in `Monolith*Port` (`src-tauri/src/mcp_ports.rs`), *below* the port seam:
`MonolithStationPort` runs `curate_gateway`, distance/bearing, and nearest-first
sorting; `MonolithStatusPort` derives backend/modem state and applies rig-safety
policy; `MonolithComposePort` performs real staging. A fixture-backed mock port
returns synthetic data and does **not** execute that logic.

Therefore the harness delivers, by construction:

- **Router and tool-schema parity** — the real MCP router dispatch, argument
  validation, tool schema, DTO serialization, and error mapping.
- **Real guard and taint** — the testserver constructs the real `EgressGuard`;
  arm/taint/egress decisions and post-read taint are genuine.
- **Data-shape and serialization fidelity** — the agent receives results in the
  exact real DTO wire shape (`GatewayDto`, `RigStatusDto`, …) instead of
  `{ok:true}`. This is the whole anti-fabrication property: what the agent sees
  and learns from is the shipped result shape, populated with scenario data.

It does **not** deliver, at the mock-port seam:

- **Business-logic parity** — `curate_gateway`, distance computation, status
  derivation, and staging do not execute; the fixture supplies their outputs
  directly. Whether the real function computed a distance or the fixture stated
  it makes no difference to what the agent observes, but it means the mock is
  still a (thin) second implementation of each port's *output*, not its *logic*.

Full logic parity would require injecting synthetic raw data *below* the real
`Monolith*Port` implementations (a synthetic station cache, a synthetic rig data
source), which requires linking the Tauri monolith or refactoring every
`Monolith*Port` to accept an injectable source. That is a separate, larger
effort recorded under Future work. It changes nothing the agent observes, so it
is not required for the anti-fabrication goal.

## Decision

**1. The real Rust MCP router is the training and evaluation environment.** The
harness drives the real MCP router and the real guard; only the data a port
returns is synthetic.

**2. Synthetic state is injected at the tool-result port boundary, not at the
transport or protocol layer.** The hard-coded `Mock*` ports in
`tuxlink-mcp-testserver` become scenario-driven fixtures loaded from a file named
by a new `TUXLINK_TEST_SCENARIO` environment variable. Coverage spans every
read-path data port the simulator stubs: `StatusPort`, `StationPort`,
`PredictionPort`, `SearchPort`, `MailboxPort`, `ConfigPort`, `DevicePort`,
`LogPort`. Fixtures carry data in the **exact real DTO wire shapes**; a JSON
Schema generated from the Rust fixture types is the single source of truth and
validates the Python-side fixtures.

**3. The mechanism is additive and test-mode-gated, enforced mechanically.** The
testserver is a distinct binary; absence of `TUXLINK_TEST_SCENARIO` preserves
current behavior. A CI grep-gate asserts that `TUXLINK_TEST_SCENARIO` and the
fixture loader/types never appear under `src-tauri/src` or
`src-tauri/tuxlink-mcp-core` (the production-linked crates, one of which carries
a `test-support` mock feature that is an attractive wrong home for this code).

**4. The agent is driven by the existing `d3zwe` frontend, not a new client.**
`d3zwe` runs the real agent loop against the testserver socket with an
OpenRouter-configured `OpenAiProvider`. No Python MCP client is built. The
`harness_oai.py` OpenRouter-readiness gap is moot.

**5. Two complementary measurements.** The real MCP router always serializes a
typed DTO, so a port cannot emit the simulator's literal `{ok:true}` non-answer
through the router. The literal sim-vs-real comparison therefore lives in a
model-free contract diff, and the confound-free A/B compares two real-DTO worlds.

- **(A) Tool-return contract diff (no model).** For each covered tool, call the
  active Python sim (returns `{ok:true}`) and the fixture-seeded testserver
  (returns a populated real DTO) and diff the results. This is the complete,
  direct map of the fabrication void and the primary evidence.
- **(B) Behavioral A/B through one transport (with model).** Both arms run the
  same model, the same agent loop (`d3zwe`), and the same MCP transport; only the
  scenario `world` differs. **Arm GROUNDED** seeds a populated `world` (real
  DTOs); **Arm VOID** seeds a data-void `world` in the real wire shape — empty
  collections (`gateways`, search hits) and absent optional fields
  (`RigStatusDto` all-`None`). Arm VOID is the router-faithful, in-distribution
  analog of the sim's non-answer (the shipped app can return empty). The test:
  does the agent fabricate data the tool did not provide. For DTOs with no void
  representation (non-optional `ModemStatusDto`/`PositionStatusDto`), Arm VOID
  supplies a minimal concrete state and Arm GROUNDED a specific one, testing
  whether the model reports the provided state or invents a different one.
- **Optional reference arm.** The active Python sim (`{ok:true}`) run on the
  existing Python harness as a third data point, to confirm Arm VOID's
  fabrication rate tracks the sim's. It does not use `d3zwe` (the sim is not an
  MCP server), so its loop/transport difference is acknowledged, not confounding
  the primary A/B.

**6. Answer grounding is graded, and the decision rule is pre-committed.** The
judge gains a content-grounding capability: a final answer that cites a
callsign, frequency, grid, distance, or solar index absent from the scenario
`world` is fabrication; declining when `world` lacks a datum is correct. The
harness emits a divergence report and applies a pre-registered rule: **GO** when
Arm VOID fabricates in at least two of three samples and Arm GROUNDED eliminates
at least two of those with no regression; **AMBIGUOUS** when the delta is a single
sample or cause tags are stochastic or shape-mismatch; **NO-GO** when Arm GROUNDED
shows no fabrication reduction.

**7. One scenario artifact serves four uses** — train the student, gate
regressions per build, reproduce field agentic bugs end to end, observe a live
agent. The fixture that seeds a port also supplies the judge's ground truth, so
environment and grader cannot disagree on a run.

## Alternatives considered

### A. Keep maintaining the Python simulator (status quo)

Rejected. It fabricates via `{ok:true}` and is a drift-prone second
implementation.

### B. Deep transport or protocol simulation

Rejected. Agent behavior is a function of tool results, not modem internals.
Injecting at the result boundary reproduces the training-relevant behavior at a
fraction of the cost.

### C. Tool-level injection inside the router (`#[cfg(test)]` in `router.rs`)

Rejected. It would pollute production tool methods, require a test-configured
monolith rebuild, and scale poorly. The port seam already isolates the
substitution to the testserver crate.

### D. Below-seam injection for full logic parity (in this build)

Deferred, not rejected. Running `curate_gateway`/distance/status logic on
synthetic inputs is the only way to make parity truly tautological, but it
requires linking the monolith or refactoring every `Monolith*Port`. It changes
nothing the agent observes, so it is Future work, not part of the harness.

### E. Build a Python MCP-over-UDS client and OpenRouter loop

Rejected. `d3zwe` already drives the real agent loop over rmcp/UDS with an
OpenAI-compatible provider. Reimplementing it in Python would be lower fidelity
(a second agent loop) and net-new transport work.

## Watched failure modes

- **False-green judge (highest risk).** Both arms run and the grader cannot tell
  fabricated final content from grounded content. The current judge scores
  tool-call choreography only; without the content-grounding capability of
  Decision 6, the measurement is meaningless. This capability is load-bearing,
  not optional.
- **Confounded comparison.** Avoided by construction: both arms share model,
  loop, and transport (Decision 5). The only variable is the port return.
- **Cross-language schema drift.** Fixtures use real DTO shapes; a JSON Schema
  generated from the Rust types validates the Python fixtures. A single-sample
  round-trip is insufficient and is not the mechanism.
- **Production leakage.** The fixture loader must stay in the testserver crate; a
  CI grep-gate enforces it against the tempting `tuxlink-mcp-core` `test-support`
  path.
- **Non-optional DTO fields.** `ModemStatusDto` and `PositionStatusDto` fields
  are non-optional; only `RigStatusDto` has nullable live fields. "Genuine null"
  scenarios are constrained to genuinely optional fields; forcing nulls elsewhere
  would require a production DTO change and is out of scope.
- **Statistical noise.** With small N a one-sample swing is large; the
  pre-registered decision rule (Decision 6) governs interpretation, and runs are
  fixed-temperature with a recorded sample count.

## Consequences

- Parity is delivered at the shape/schema/guard level for every covered port; the
  agent never again sees `{ok:true}` where the shipped app returns structured
  data.
- The Python simulator's read-path stubs are superseded by the real router plus
  fixtures; the drift surface shrinks to the fixture schema, which is
  contract-checked against the Rust types.
- The fixture schema becomes the shared artifact across training, regression,
  bug-repro, and observation.
- The testserver is Rust; builds and verification run on R2 (`r2-poe`,
  x86_64, currently rustc 1.75.0 — the workspace MSRV is 1.75, so the toolchain
  is verified at build time and updated via `rustup` if a dependency demands it).
- Guard, taint, and transmit paths are unchanged.

## Future work

- **Below-seam logic parity** (Alternative D): inject synthetic raw data beneath
  the real `Monolith*Port` implementations so `curate_gateway`, distance, status
  derivation, and staging execute against scenario inputs. Makes parity
  tautological for logic, not only shape. Large; scoped separately.

## Propagation

Per the documentation propagation contract, the canonical sources are this ADR
(the decision) and the PoC/build design spec
(`docs/superpowers/specs/2026-07-05-tuxlink-as-sim-harness-poc-design.md`). The
work is tracked on bd issue tuxlink-cnz5o. No parallel restatement in CLAUDE.md.
