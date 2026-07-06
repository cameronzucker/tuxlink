# Tuxlink-as-simulation-harness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the real Tuxlink MCP router the training/eval environment by seeding scenario-driven fixtures at the tool-result port boundary, so the agent receives real structured DTOs instead of the simulator's `{ok:true}` stubs — then measure the fabrication reduction.

**Architecture:** Additive, test-mode-gated. A new `TUXLINK_TEST_SCENARIO` fixture (real DTO wire shapes) seeds new scenario ports in the `tuxlink-mcp-testserver` binary; the real router + real `EgressGuard` are reused. The existing `d3zwe` frontend drives the real agent loop against it over OpenRouter. A Python content-grounding judge scores final-answer fabrication; a contract diff (no model) and a d3zwe A/B (Arm GROUNDED vs Arm VOID) produce the fidelity measurement under a pre-committed decision rule.

**Tech Stack:** Rust (testserver + d3zwe crates, serde/schemars, async-trait, tokio), Python 3 (elmer_distill package, pytest, jsonschema), the real MCP router (`tuxlink-mcp-core`, read-only here).

**Canonical design:** [ADR 0021](../adr/0021-tuxlink-as-simulation-harness.md) + [spec](../superpowers/specs/2026-07-05-tuxlink-as-sim-harness-poc-design.md). This plan implements them; it does not restate the decision.

## Global Constraints

Every task's requirements implicitly include this section.

- **Additive + test-mode-gated ONLY.** All Rust changes are confined to `src-tauri/tuxlink-mcp-testserver/` and `src-tauri/d3zwe/`. `src-tauri/src` (the Tauri monolith) and `src-tauri/tuxlink-mcp-core` are **read-only** for this plan. No production DTO changes, no production-path behavior change. Task 5's grep-gate mechanically enforces this.
- **Refusal / restraint / egress-guard fidelity are out of scope.** The guard runs for real (it is already wired) but is not the measured axis. No refusal training.
- **The Pi cannot compile Rust.** All Rust build/test/clippy runs execute on **R2** (`ssh r2-poe`, x86_64, rustc 1.75.0 = MSRV; `rustup update` only if a dependency demands it) or in CI. Python runs locally on the Pi.
- **DTO field names are verified at compile time.** The Rust code blocks use the drafter's read of `src-tauri/tuxlink-mcp-core/src/ports.rs`. Before implementing each Rust task, the worker reads the exact DTO definition in `ports.rs`; the R2 compile (Task 2 of each TDD cycle failing to build) is the enforcement — fix field names/variants against the real DTO, do not invent.
- **Non-optional DTOs cannot be null.** `ModemStatusDto` and `PositionStatusDto` fields are non-optional; only `RigStatusDto` has nullable live fields. A void world supplies **minimal concrete state** for the non-optional DTOs and `None`/empty for the rest.
- **Commit discipline:** conventional-commit type matching intent; every commit carries `Agent: delta-basil-fen` above the `Co-Authored-By:` trailer. TDD every task (failing test → confirm red → minimal impl → confirm green → commit).
- **No `git reset --hard`.** For R2 branch sync use `git fetch origin <branch> && git checkout <branch> && git merge --ff-only origin/<branch>` (the destructive-git posture holds even on the R2 build clone).

## Cross-half interface contract (Rust ↔ Python)

The two halves interlock on ONE artifact: the scenario fixture JSON.

- **Canonical shape = the Rust `World` struct (Task 1).** A fixture file is one JSON object. The Rust `Fixture` reads `{id, world}` and **ignores** any other top-level keys (`family`, `depth`, `taint_state`, `prompt`, `spec`) — `Fixture`/`World` must NOT use `#[serde(deny_unknown_fields)]`. The Python `Scenario` reads the whole object. So a single scenario file drives both sides.
- **`world` MUST include `modem` and `position`** (non-optional DTOs) in every fixture the testserver loads, even when they are not the scenario's focus (minimal concrete state). The Python-authored fixtures (`grounded-gateways-01.json`, `void-gateways-01.json`, `sample-grounded.json`) must be brought into conformance with `World` — add `modem` + `position`, and place `operator_grid` inside `stations` (not at the `world` top level; the Python grounding flattener still finds it there).
- **Schema handoff (reconciliation):** Task 2 (Rust) generates the JSON Schema. Task 2 MUST also write it to a committed file `src-tauri/tuxlink-mcp-testserver/tests/fixtures/world.schema.json` (via a test that serializes `fixture_json_schema()` to that path). Python Task 9 validates fixtures against that file (pointed via `TUXLINK_WORLD_SCHEMA` or an explicit path), replacing its placeholder schema once the Rust artifact lands.
- **Transcript shape:** d3zwe `--json` (Task 6) emits `{"kind","text"}`. The Python grounding judge (Task 10) grades the final-answer `text` against the scenario `world`. If richer tool-sequence grading is later needed, that is a filed follow-up (the runner does not expose the full transcript today).

## Task sequence & dependencies

```
RUST                                   PYTHON
1 Fixture/World types ─┬─► 2 Schema ───────────────► 9 schema validation
                       ├─► 3 Loader ─┐
                       │             ├─► 4 scenario_ports ─┐
                       │             │                     ├─► 6 d3zwe --json ─► 7 R2 verify
                       └─────────────┴─► 5 grep-gate       │
8 Scenario.world/spec ─┬─► 10 grounding judge (LOAD-BEARING) ─┬─► 12 A/B harness (needs 6,7 on R2)
                       ├─► 11 contract diff                   └─► 13 reference arm
                       └─► 9 schema validation
```

- Rust 3 + 4 commit close together (the `main.rs` wiring in 3 references ports defined in 4).
- Rust 7 (d3zwe) is independent of 1–5 and may proceed in parallel.
- Python 8 is the foundation; 9/10/11 depend only on 8; 12/13 depend on 8+10 and, at run time on R2, on the Rust 6/7 artifacts (fake d3zwe locally, real on R2).
- Contention files (`main.rs`, `scenario.py`, `judge.py`) are each touched by a single owning task in sequence — never two workers at once.

---

# RUST TASKS

*(Verbatim from the Rust plan draft; DTO field names verified at R2 compile per Global Constraints. `World` requires `modem`+`position`; do not add `deny_unknown_fields`.)*

## Task 1: Fixture types + deserialization (testserver crate)

**Files:**
- Create `src-tauri/tuxlink-mcp-testserver/src/fixture.rs`
- Modify `src-tauri/tuxlink-mcp-testserver/src/main.rs` (add `mod fixture;` after `mod mocks;`)
- Modify `src-tauri/tuxlink-mcp-testserver/Cargo.toml` (`[dependencies]`: `serde`, `serde_json`, `thiserror`; `[dev-dependencies]`: `tempfile`)
- Test: inline `#[cfg(test)] mod tests` in `fixture.rs`

**Interfaces:**
- Consumes: real DTOs from `tuxlink_mcp_core::ports` (all derive `Serialize + Deserialize`).
- Produces: `pub struct Fixture { pub id: String, pub world: World }`, `pub struct World {…}`, `pub fn load_fixture(path: &Path) -> Result<Fixture, FixtureError>`, `pub enum FixtureError`. Consumed by Tasks 2, 3, 4.

- [ ] **Step 1: Write the failing test** — in `fixture.rs`, the four tests: `grounded_fixture_deserializes_into_real_dtos`, `void_fixture_uses_minimal_concrete_for_non_optional`, `malformed_fixture_fails_loudly`, `missing_file_fails_loudly`. (Full test bodies as in the Rust draft; `GROUNDED_JSON` const includes `modem`+`position` and real `GatewayDto`/`StationListDto`/`RigStatusDto`/`SolarSnapshotDto` fields. Verify each field name against `ports.rs` first.)
- [ ] **Step 2: Run it, expect FAIL** `ssh r2-poe 'cd ~/tuxlink-cnz5o && cargo test --manifest-path src-tauri/Cargo.toml --locked -p tuxlink-mcp-testserver fixture::'` → `undeclared type Fixture` / `cannot find function load_fixture`.
- [ ] **Step 3: Implement** — `fixture.rs` with `World` (fields: `stations: Option<StationListDto>`, `rig: Option<RigStatusDto>`, `modem: ModemStatusDto` [required], `position: PositionStatusDto` [required], `backend/solar: Option<…>`, `predictions/docs/catalog/mailbox/log: Vec<…>` with `#[serde(default)]`, `config: Option<ConfigWorld>`, `devices: Option<DeviceWorld>`), `MailboxEntry`, `ConfigWorld`, `DeviceWorld`, `Fixture`, `FixtureError` (Io/Parse via `thiserror`), `load_fixture` (read_to_string → serde_json::from_str, both errors loud). NO `deny_unknown_fields`. Add the Cargo.toml deps + `mod fixture;`.
- [ ] **Step 4: Run it, expect PASS** — same command → `4 passed`.
- [ ] **Step 5: Commit** — `feat(testserver): scenario Fixture/World types over real DTOs` (+ Agent trailer).

## Task 2: JSON Schema generation + committed schema file (testserver-only)

**Files:** Modify `fixture.rs` (add `schema` submodule + shadow `WorldSchema`/`FixtureSchema` deriving `schemars::JsonSchema`); Modify `Cargo.toml` (`schemars = "1.0"`); Create the committed schema via a test that writes `tests/fixtures/world.schema.json`.

**Interfaces:** Produces `pub fn fixture_json_schema() -> serde_json::Value` and the committed `tests/fixtures/world.schema.json` (consumed by Python Task 9).

**Why a shadow struct:** prod DTOs deliberately do NOT derive `JsonSchema` (adding it is a forbidden prod-crate change). `WorldSchema` mirrors `World`'s field NAMES with `serde_json::Value` sub-shapes; a drift test ties the shadow field set to `World`'s.

- [ ] **Step 1: Write the failing test** — `schema_has_world_and_id_at_top_level`, `schema_world_field_set_matches_struct`, and **`schema_written_to_committed_file`** (calls `fixture_json_schema()`, writes it to `CARGO_MANIFEST_DIR/tests/fixtures/world.schema.json`, asserts the file parses and has `properties.world`).
- [ ] **Step 2: Run it, expect FAIL** `ssh r2-poe 'cd ~/tuxlink-cnz5o && cargo test … fixture::schema'` → `cannot find function fixture_json_schema`.
- [ ] **Step 3: Implement** — `fixture_json_schema()` via `schemars::generate::SchemaSettings::draft2020_12()` on `FixtureSchema`; shadow structs. **schemars 1.0 API note:** the exact generator call + the `$defs`/`WorldSchema` JSON pointer are pinned at first R2 build; if the resolved 1.0.x differs, adjust the call and the test's schema-path accessor — the contract (top-level `id`+`world`, world field set == `World`'s) is stable. Add the schema-writing test that emits the committed file.
- [ ] **Step 4: Run it, expect PASS** → `3 passed`; `tests/fixtures/world.schema.json` now exists.
- [ ] **Step 5: Commit** — `feat(testserver): generate + commit fixture JSON Schema via shadow struct` (+ trailer; `git add` the schema file too).

## Task 3: Scenario loader (`TUXLINK_TEST_SCENARIO` → `Arc<World>`)

**Files:** Modify `fixture.rs` (add `SCENARIO_ENV`, `resolve_scenario`, `load_scenario_from_env`); Modify `main.rs` (load branch — lands with Task 4). Tests inline in `fixture.rs`.

**Interfaces:** Produces `const SCENARIO_ENV`, `pub fn resolve_scenario(Option<String>) -> Result<Option<Arc<World>>, FixtureError>` (None/empty → `Ok(None)` = current mock behavior), `load_scenario_from_env`.

- [ ] **Step 1: Write the failing test** — `scenario_env_absent_yields_none`, `scenario_env_present_loads_world`, `scenario_env_bad_path_fails_loudly`.
- [ ] **Step 2: Run it, expect FAIL** → `cannot find function resolve_scenario`.
- [ ] **Step 3: Implement** — `resolve_scenario` (pure, testable) + `load_scenario_from_env` (env IO). In `main.rs` add the load call before `McpState` init (the `McpState` port select is in Task 4).
- [ ] **Step 4: Run it, expect PASS** → `3 passed`.
- [ ] **Step 5: Commit** (land alongside Task 4) — `feat(testserver): TUXLINK_TEST_SCENARIO loader holding Arc<World>`.

## Task 4: `scenario_ports.rs` — port impls from `Arc<World>`

**Files:** Create `src-tauri/tuxlink-mcp-testserver/src/scenario_ports.rs`; Modify `main.rs` (`mod scenario_ports;` + branch the `McpState` read-port fields to scenario vs mock; egress/abort/write/compose stay on `mocks::*` with the real `EgressGuard`). Tests inline.

**Interfaces:** Produces `ScenarioStatus/Station/Prediction/Search/Mailbox/Config/Device/Log(pub Arc<World>)`, each `impl`ing its port trait. `main.rs` wires them when `scenario.is_some()`.

**Void semantics:** empty collections + `None` optionals from `World`; `rig_status` → all-`None`/`configured:false` when `world.rig` is `None`; `modem`/`position` pass through (required); ports whose `World` datum is `None` and whose trait return is non-optional return `PortError::Unavailable`. `StatusPort` also implements `vara_status`/`platform_info`/`wizard_completed`/`p2p_peer_password_status` with deterministic minimal values (not on the fabrication axis).

- [ ] **Step 1: Write the failing test** — `void_find_stations_returns_empty_list`, `void_rig_status_is_all_none_unconfigured`, `modem_status_passes_through_required_dto` (tokio tests; build `void_world()` helper). Verify every trait method signature against `ports.rs`.
- [ ] **Step 2: Run it, expect FAIL** → `cannot find type ScenarioStation`.
- [ ] **Step 3: Implement** — the eight `#[async_trait]` impls (full bodies as in the Rust draft); the `main.rs` `match &scenario { Some(world) => (Scenario…) , None => (Mock…) }` port-tuple select, leaving egress/abort/write/compose/guard/name/version untouched.
- [ ] **Step 4: Run it, expect PASS** → `3 passed`; whole-crate `cargo test -p tuxlink-mcp-testserver` green (main.rs compiles).
- [ ] **Step 5: Commit** — `feat(testserver): scenario ports return real DTOs from Arc<World>`.

## Task 5: CI grep-gate (scenario code out of prod crates)

**Files:** Create `src-tauri/tuxlink-mcp-testserver/tests/no_scenario_leak.rs` (integration test; runs under the existing workspace `cargo test`, no CI YAML change).

- [ ] **Step 1: Write the failing test** — `scenario_code_absent_from_prod_crates` walking `src-tauri/src` + `src-tauri/tuxlink-mcp-core/src` for FORBIDDEN tokens (`TUXLINK_TEST_SCENARIO`, `load_fixture`, `resolve_scenario`, `ScenarioStatus`, `ScenarioStation`, `fixture_json_schema`).
- [ ] **Step 2: Run it, expect FAIL** — plant `// TUXLINK_TEST_SCENARIO` in `tuxlink-mcp-core/src/lib.rs`, run `ssh r2-poe '… --test no_scenario_leak'` → reports the leak. Remove the planted comment.
- [ ] **Step 3: Implement** — the test is the gate; "implement" = confirm the true clean state (planted leak removed).
- [ ] **Step 4: Run it, expect PASS** → `1 passed`.
- [ ] **Step 5: Commit** — `test(testserver): CI grep-gate keeps scenario code out of prod crates`.

## Task 6: d3zwe `--json` (machine-readable outcome) + OpenRouter surfacing

**Files:** Modify `src-tauri/d3zwe/src/cli.rs` (`Args.json` + parse + USAGE + tests), `src-tauri/d3zwe/src/print.rs` (`render_outcome_json` + tests), `src-tauri/d3zwe/src/main.rs` (emit JSON when `args.json`; thread `json` through `run_one`/`repl`).

**Interfaces:** Produces `pub fn render_outcome_json(&RunOutcome) -> String` (single-line `{"kind","text"}`) and `Args.json: bool`. **OpenRouter needs no new plumbing** — `--allow-remote --endpoint https://openrouter.ai/api/v1/chat/completions --model <name>` + `D3ZWE_API_KEY` already exist; the only work is the gradeable transcript.

- [ ] **Step 1: Write the failing test** — `print.rs`: `json_outcome_is_parseable_with_kind_and_text`, `json_outcome_tags_denied`; `cli.rs`: `json_flag_parses`, `json_defaults_false`. (Match the real `RunOutcome` variants in `tuxlink_agent_runner` — verify the variant set before writing `render_outcome_json`.)
- [ ] **Step 2: Run it, expect FAIL** `ssh r2-poe '… -p d3zwe json'` → `cannot find function render_outcome_json`.
- [ ] **Step 3: Implement** — `render_outcome_json` mapping every `RunOutcome` variant to `{kind,text}`; `Args.json`; parse `--json`; USAGE line; `main.rs` branch on `json`.
- [ ] **Step 4: Run it, expect PASS** `ssh r2-poe '… -p d3zwe'` → all d3zwe tests pass.
- [ ] **Step 5: Commit** — `feat(d3zwe): --json emits a gradeable machine-readable outcome`.

## Task 7: R2 build + verify (sync, cargo test/clippy, one smoke)

**Files:** Create `src-tauri/tuxlink-mcp-testserver/tests/fixtures/sample-grounded.json` (committed; grounded world WITH `modem`+`position`); add `committed_sample_fixture_loads` unit test in `fixture.rs`. No source changes otherwise.

- [ ] **Step 1: Write the failing test** — `committed_sample_fixture_loads` (loads the committed fixture, asserts id + first gateway callsign).
- [ ] **Step 2: Run it, expect FAIL** (fixture file absent) → `Io(...)` panic.
- [ ] **Step 3: Implement** — commit the `sample-grounded.json` (real DTO shapes, includes `modem`+`position`). Sync branch to R2: `git push origin bd-tuxlink-cnz5o/sim-harness-poc`; on R2 `git fetch origin bd-tuxlink-cnz5o/sim-harness-poc && git checkout bd-tuxlink-cnz5o/sim-harness-poc && git merge --ff-only origin/bd-tuxlink-cnz5o/sim-harness-poc`; `ssh r2-poe 'rustc --version'` (expect 1.75.x; `rustup update` only if a dep demands).
- [ ] **Step 4: Run it, expect PASS** — on R2: `cargo test … -p tuxlink-mcp-testserver -p d3zwe`; `cargo clippy … -p tuxlink-mcp-testserver -p d3zwe --all-targets -- -D warnings`; `cargo build … -p tuxlink-mcp-testserver -p d3zwe`; then the loopback smoke (testserver seeded with `sample-grounded.json` over `TUXLINK_MCP_SOCK`, `d3zwe --socket … --json --prompt "find a station" --endpoint <loopback> --model local`) — testserver logs `scenario loaded (1 gateways)`, d3zwe prints one `{"kind","text"}` line. No transmit path (read tools only; egress disarmed). The live OpenRouter A/B is the operator's wire-walk.
- [ ] **Step 5: Commit** — `test(testserver): committed sample-grounded fixture + R2 smoke evidence`.

---

# PYTHON TASKS

*(Verbatim from the Python plan draft. All run locally on the Pi via pytest. Rust artifacts — the `world.schema.json` from Task 2, the `d3zwe` binary from Task 6 — are injected by path/subprocess with local fakes; the real artifacts swap in on R2 for Task 7 / the wire-walk. Fixtures created here MUST include `modem`+`position` per the cross-half contract.)*

## Task 8: `Scenario.world` + grounding-predicate support

**Files:** Modify `dev/elmer-distill/src/elmer_distill/scenario.py` (SuccessSpec + from_json/to_json); Create `dev/elmer-distill/tests/test_scenario_world.py`; Create `dev/elmer-distill/tests/fixtures/scenarios/grounded-gateways-01.json` (**include `modem`+`position`; put `operator_grid` inside `stations`**).

**Interfaces:** Produces `Scenario.world: dict` (default `{}`), `SuccessSpec.grounded_claims: list`, `SuccessSpec.must_decline_when_absent: list`, round-tripped through from_json/to_json (world + new spec fields emitted only when non-empty so world-less fixtures keep exact-equality round-trip).

- [ ] Steps 1–5 as in the Python draft P1 (tests `test_world_survives_roundtrip`, `test_grounding_predicates_parsed`, `test_world_defaults_empty_when_absent`; implement the dataclass fields + conditional to_json emission). Command: `PYTHONPATH=src python3 -m pytest tests/test_scenario_world.py tests/test_scenario.py -v`. Commit: `feat(elmer): carry opaque world + grounding predicates on Scenario`.

## Task 9: JSON-Schema validation of `world`

**Files:** Create `dev/elmer-distill/src/elmer_distill/fixture_schema.py`; Modify `requirements.txt` (`jsonschema`); Create `tests/test_fixture_schema.py`, `tests/fixtures/scenarios/invalid-world-01.json`. **Point validation at the Rust-generated `src-tauri/tuxlink-mcp-testserver/tests/fixtures/world.schema.json` (Task 2) via `TUXLINK_WORLD_SCHEMA` or path arg;** keep a committed placeholder only as a fallback for pre-Task-2 local runs.

**Interfaces:** Produces `validate_world(world, schema_path=None) -> None` (raises `jsonschema.ValidationError`), `load_schema(schema_path=None) -> dict`.

- [ ] Steps 1–5 as in the Python draft P2 (valid passes, invalid raises, load_schema reads file). Commit: `feat(elmer): validate fixture world against a JSON Schema by path`.

## Task 10: Content-grounding judge capability — LOAD-BEARING

**Files:** Create `dev/elmer-distill/src/elmer_distill/grounding.py`; Modify `dev/elmer-distill/src/elmer_distill/judge.py` (predicate (f), applied only when the scenario carries grounding fields); Create `tests/test_grounding.py`.

**Interfaces:** Produces `flatten_world_values(world) -> set`, `extract_claims(answer) -> dict`, `check_grounding(world, answer) -> {"grounded","fabricated"}`, `world_lacks_category(world, category) -> bool`; Judge appends `fabricated claim: <tok>` / `stated-absent-datum: <cat>` reasons.

- [ ] Steps 1–5 as in the Python draft P3 (unit: flatten/extract/check; integration: grounded passes, fabricated fails, honest-decline passes; existing `test_judge.py`/`test_judge_negatives.py` stay green because the block is gated on grounding intent). Commit: `feat(elmer): content-grounding judge capability`. **This closes ADR 0021's highest-risk "false-green judge" mode — without it the whole A/B is meaningless.**

## Task 11: Tool-return contract diff (no model)

**Files:** Create `dev/elmer-distill/src/elmer_distill/contract_diff.py`; Create `tests/test_contract_diff.py`.

**Interfaces:** Produces `sim_return(tool)`, `testserver_return(tool, world, testserver_cmd=None)`, `diff_tool(tool, world)`, `build_table(tools, world)`, `main(argv)`. Testserver return is projected from the fixture `world` by default (the single source the real testserver also reads); `--testserver-cmd` optionally captures live from the Rust harness on R2.

- [ ] Steps 1–5 as in the Python draft P4 (sim returns `{ok:true}`; testserver_return populated from world; diff reports void_fields; build_table covers all requested tools). Commit: `feat(elmer): model-free tool-return contract diff`.

## Task 12: Behavioral A/B divergence harness (drives d3zwe)

**Files:** Create `dev/elmer-distill/src/elmer_distill/ab_harness.py`; Create `tests/test_ab_harness.py`; Create `tests/fixtures/scenarios/void-gateways-01.json` (**void twin; include `modem`+`position`**).

**Interfaces:** Produces `run_arm(scenario_path, d3zwe_cmd, env)`, `grade_arm(scenario, transcript)`, `divergence_report(scenario, grounded_runs, void_runs)`, `decision(report) -> "GO"|"AMBIGUOUS"|"NO-GO"`. Drives the real `d3zwe` binary as a subprocess (env: `D3ZWE_API_KEY`, endpoint/model, `TUXLINK_TEST_SCENARIO`); tests use a fake-d3zwe Python script so the plumbing is fully unit-tested on the Pi.

- [ ] Steps 1–5 as in the Python draft P5 (grounded arm passes, void arm fabricates, divergence + decision GO, env wiring probe). **Note:** `run_arm`/`grade_arm` transcript shape must match d3zwe `--json` `{"kind","text"}` (Task 6) — grade the `text` as the final answer; adapt the fake-d3zwe to emit that shape. Commit: `feat(elmer): behavioral A/B divergence harness driving d3zwe`.

## Task 13: Optional reference arm (active sim `{ok:true}`)

**Files:** Create `dev/elmer-distill/src/elmer_distill/reference_arm.py`; Create `tests/test_reference_arm.py`.

**Interfaces:** Produces `grade_reference(scenario, transcript)`, `reference_report(scenario, sim_runs, void_fabrication_rate)` (carries a `caveat` noting the loop/transport difference — a third data point, not part of the confound-free primary A/B).

- [ ] Steps 1–5 as in the Python draft P6 (sim fabrication flagged; report carries rate + caveat). Full-suite regression: `PYTHONPATH=src python3 -m pytest -v`. Commit: `feat(elmer): optional reference arm for sim fabrication rate`.

---

# FINAL GATE — wire-walk (hard gate before any "done")

Per CLAUDE.md's wire-walk gate and ADR 0021: before the harness is called shipped/done, run the **`wire-walk`** skill. The operator supplies the key user/agent flows **greenfield** (do NOT draft them). Trace each verbatim to `file:line`. Any broken motivating flow means NOT shipped.

The motivating flows to have the operator confirm (they supply them; this is a reminder of the seams that must be walked):
- Operator runs the R2 A/B: `d3zwe` (OpenRouter) → testserver (`TUXLINK_TEST_SCENARIO` grounded fixture) → real router → `ScenarioStation` returns real gateways → agent answer cites real gateways → grounding judge PASSES; same with the void fixture → agent fabricates or honestly declines → judge classifies correctly.
- The contract diff over all covered tools produces the fabrication-void map.
- The divergence report + decision rule emit GO/AMBIGUOUS/NO-GO on real runs.

The operator conducts the live OpenRouter runs (model + real transcript are operator-run; R2 Task 7 proves the plumbing without a live model). Only after the operator's greenfield flows trace clean is cnz5o done.

## Self-review notes (assembler)

- **Spec coverage:** all 8 spec scope items map to tasks — fixtures/loader (1,3), all read ports (4), schema (2/9), grep-gate (5), d3zwe (6), Scenario.world (8), grounding judge (10), contract diff (11), A/B + decision rule (12), reference arm (13), R2 verify (7), wire-walk (final).
- **Cross-half consistency fixed:** `modem`+`position` required in all testserver-loaded fixtures; schema written to a committed file (Task 2) and consumed by Task 9; single JSON serves both readers (no `deny_unknown_fields`); transcript shape reconciled (Task 12 grades d3zwe `--json` `text`).
- **Contention:** `main.rs` (Tasks 1,3,4 — sequenced), `scenario.py` (Task 8 only), `judge.py` (Task 10 only) — no parallel edits.
- **Field-name risk** is deferred to R2 compile-time enforcement, stated in Global Constraints.
