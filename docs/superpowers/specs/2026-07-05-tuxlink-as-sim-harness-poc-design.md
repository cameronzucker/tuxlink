# Tuxlink-as-simulation-harness — build design

- **Date:** 2026-07-05 (revised same day after the five-round Codex adrev)
- **bd issue:** tuxlink-cnz5o (architecture / feature, P1, in_progress)
- **Branch / worktree:** `bd-tuxlink-cnz5o/sim-harness-poc`
- **ADR:** [0021](../../adr/0021-tuxlink-as-simulation-harness.md) (canonical decision)
- **Status:** design approved; full build directed by operator 2026-07-05
- **Agent:** delta-basil-fen

This spec is the detailed design. The decision, the precise parity claim, the
alternatives, and the watched failure modes are canonical in ADR 0021 and are
not restated here. This document covers component design, the scenario/fixture
shapes, the coverage set, and the measurement.

## Goal

Drive the real Tuxlink MCP router as the training and evaluation environment so
the agent receives real structured tool results instead of the active
simulator's `{"ok": true}` stubs. Deliver the complete harness (not a slice):
scenario-driven fixtures for every read-path data tool, the real agent loop
driven against it, a content-grounding judge, and a fidelity measurement with a
pre-committed decision rule.

## Scope

Covered read-path data ports and their tools:

| Port | Tools | DTO(s) returned |
|---|---|---|
| `StatusPort` | `backend_status`, `modem_get_status`, `rig_status`, `position_status` | `BackendStatusDto`, `ModemStatusDto`, `RigStatusDto`, `PositionStatusDto` |
| `StationPort` | `find_stations` | `StationListDto { gateways: [GatewayDto], fetched_at_ms, operator_grid }` |
| `PredictionPort` | `predict_path`, `solar_conditions` | `PathPredictionDto`, `SolarSnapshotDto` |
| `SearchPort` | `docs_search`, `catalog_list`, `tauri_search_run` | `DocsHitDto`, `CatalogEntryDto`, `SearchResultsDto` |
| `MailboxPort` | `mailbox_list`, `message_read` | `MessageMetaDto`, `ParsedMessageDto` |
| `ConfigPort` | `config_read`, `config_get_*` | curated config DTOs |
| `DevicePort` | `packet_list_serial_devices`, … | device DTOs |
| `LogPort` | `session_log_snapshot` | `LogLineDto` |

Out of scope (per ADR 0021): refusal/restraint training; egress-guard/taint
*fidelity* comparison (the guard runs for real but is not the measured axis);
deep transport simulation; below-seam logic parity (Future work); any production
transmit-path or production-crate change.

## Architecture

### Injection seam (Rust, testserver only)

The `Mock*` ports in `src-tauri/tuxlink-mcp-testserver/src/mocks.rs` become
scenario-driven (implemented in a new `scenario_ports.rs` module to avoid
contention on `mocks.rs`). On startup the testserver reads `TUXLINK_TEST_SCENARIO`
(a fixture path); absent, current hard-coded behavior is preserved. A covered
scenario port **always returns a real, typed DTO** built from the loaded `world`
— the router cannot emit the simulator's `{ok:true}`, so there is no "stub mode."
The A/B is two *fixtures*, not two code paths:

- **grounded `world`** — populated data, yielding fully-populated DTOs;
- **void `world`** — the data-void in the real wire shape: empty collections
  (`gateways`, search hits) and absent optional fields (`RigStatusDto` all-`None`).
  For DTOs with non-optional fields (`ModemStatusDto`, `PositionStatusDto`) the
  void fixture supplies a minimal concrete state, since those cannot be null.

The real router and real `EgressGuard` are untouched. A CI grep-gate asserts
`TUXLINK_TEST_SCENARIO` and the fixture loader/types never appear under
`src-tauri/src` or `src-tauri/tuxlink-mcp-core`.

### Fixture schema (cross-language, real DTO shapes)

`world` carries data in the exact Rust DTO wire shapes — `GatewayDto`
(`callsign`, `mode`, `channel`, `frequencies_khz`, optional `distance_km` /
`distance_mi` / `bearing_deg`, optional `grid`), `RigStatusDto` (optional live
fields), `ModemStatusDto` / `PositionStatusDto` (non-optional fields),
`SolarSnapshotDto`, `PathPredictionDto`, and the search/mailbox/config/device/log
DTOs. The Rust fixture types are the source of truth; a JSON Schema is generated
from them and validates the Python-side fixtures. No invented fields: the
pre-adrev sketch's `world.stations` shape, `modem.backend`, and
`position.gps_fix` do not exist in the DTOs and are not used.

`Scenario` / `SuccessSpec` (`dev/elmer-distill/src/elmer_distill/scenario.py`)
gain a `world` block and content-grounding predicate fields; `from_json` /
`to_json` must round-trip `world` (today they drop unknown fields).

### Agent driver (Rust, reuse d3zwe)

`src-tauri/d3zwe` drives the real bounded agent loop (`tuxlink-agent-runner`)
against the testserver socket via its rmcp/UDS `UdsToolInvoker`, using
`OpenAiProvider` pointed at OpenRouter (endpoint + `D3ZWE_API_KEY`). Work here is
limited to parameterizing endpoint/model and confirming a machine-readable
transcript is emitted for grading. No Python MCP client is built;
`harness_oai.py` is not used as the driver.

### Content-grounding judge (Python)

The current judge scores tool-call choreography only (`judge.py:52-100`) and
cannot detect data fabrication. Add a grounding capability: extract the
final-answer factual claims (callsigns, frequencies, grids, distances, solar
indices, gateway counts) and check each against the scenario `world`. A claim
absent from `world` is fabrication; an honest decline when `world` lacks the
datum is correct. This is net-new judgment, not wiring, and is load-bearing (ADR
0021 "false-green judge").

### Measurement

**Contract diff (per tool, no model):** for every covered tool, call the active
Python sim (`{ok:true}`) and the fixture-seeded testserver (populated real DTO)
and diff the returns. The direct, complete map of the fabrication void; the
primary evidence. Produced without a model.

**Behavioral A/B (per scenario, with model):** same model, same `d3zwe` loop,
same UDS transport; only the seeded `world` differs — **Arm GROUNDED** = populated
world, **Arm VOID** = data-void world (empty collections / `None` optionals /
minimal concrete state for non-optional DTOs). Grade both with the grounding
judge. Emit a divergence report: verdict delta, per-claim fabrication delta,
tool-sequence delta, cause tag (fabricated-data, honest-decline, shape, noise).
Runs are fixed-temperature with a recorded sample count N. An optional reference
arm runs the active Python sim (`{ok:true}`) on the existing Python harness to
confirm Arm VOID tracks the sim's fabrication rate (its loop/transport difference
is acknowledged, not part of the primary A/B).

**Decision rule (pre-committed):** GO when Arm VOID fabricates in ≥2/3 samples
and Arm GROUNDED eliminates ≥2 with no regression; AMBIGUOUS when the delta is a
single sample or cause tags are stochastic/shape; NO-GO when Arm GROUNDED shows
no fabrication reduction.

## Scenarios

Scenarios exercise the covered tools on the data-fabrication axis. Each carries a
`world` (real DTO shapes) and grounding predicates. The set covers, at minimum:
gateway identification (`find_stations`), radio/modem/position state
(`rig_status` / `modem_get_status` / `position_status`), propagation and space
weather (`predict_path` / `solar_conditions`), and product/docs answers
(`docs_search` / `catalog_list`). Against the active simulator baseline all of
these return `{ok:true}`, so all are fabrication demonstrators; there is no
"grounded control" in the active path (the pre-adrev station-as-control framing
was wrong — grounded stations exist only in the legacy reference harness).

## Testing

TDD throughout. Read `.claude/skills/test-driven-development` and
`docs/pitfalls/testing-pitfalls.md` first.

- Rust: fixture deserialization fails loudly on malformed input; each covered
  mock port returns the fixture value in the real DTO shape (fixture mode) and a
  stub (stub mode); absent env var preserves current behavior; the JSON Schema is
  generated from the Rust types and a Python fixture validates against it; a CI
  grep-gate test asserts no fixture code under the production crates.
- Python: `Scenario` round-trips `world`; the grounding judge flags a fabricated
  claim and passes a grounded one and an honest decline; the divergence harness
  produces the four delta sections and applies the decision rule.
- d3zwe: one-shot run against a fixture-seeded testserver emits a gradeable
  transcript.

## Build / verify

The testserver and d3zwe are Rust; the Pi does not compile the workspace
locally. Build and run on R2 (`r2-poe`, x86_64, rustc 1.75.0 = MSRV; `rustup
update` if a dependency demands it). CI compiles both arches. The contract diff
and the A/B divergence reports are the verification artifacts. The operator
conducts the wire-walk of the agent flows before the work is marked ready.
