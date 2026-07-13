# Routines Plan 3/6 — Validator (3 layers) + Dry-Run

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or superpowers:executing-plans. Contract-fidelity plan: exact interfaces + test contracts; implementer authors the code; per-task review is the gate.
> **Parallel-safe with plan 2:** every file here lives in the `tuxlink-routines` leaf crate (new `validate/` module + `dryrun.rs`) — zero overlap with plan 2's monolith files. Local TDD works (leaf crate compiles in seconds).

**Goal:** The validation doctrine from spec §10 as code: continuous static validation (one validator, no privileged path), the enable-time fleet check, and dry-run over the plan-1 fake-action path — plus the runtime gaps the plan-1 reviews assigned here.

**Architecture:** `tuxlink-routines::validate` — pure functions over `RoutineDef` + a `ValidationContext` port (entity existence, action descriptors, station profile, other enabled routines) so the crate stays Tauri-free; the monolith supplies a real context (thin adapter, wired in plan 4 alongside MCP or immediately by the executing controller as a follow-on task). Dry-run is a registry-swap: `build_dryrun_registry(real: &ActionRegistry, script: DryRunScript) -> ActionRegistry` producing fakes that mirror the real descriptors.

**Spec:** §10 (all three layers), §2 (validation classes), plan-1 ledger carry-ins.

## Global Constraints

- One validator, no privileged path: the same `validate()` runs for builder edits, imports, agent submissions, and enable-time (with the fleet context added).
- Errors block enable/run, never save. Severity enum: `Error | Warning`. Machine-readable: every finding carries a stable `code` (SCREAMING_SNAKE), the offending step/track/routine, and a human message with verbatim names.
- The word "workflow" never appears anywhere.
- Findings vocabulary is fixture-tested: the Laserfiche failure taxonomy IS the test corpus (each rot class = a fixture routine JSON expecting a specific code).

## File Structure (all in src-tauri/tuxlink-routines/)

```
src/validate/
├── mod.rs        # validate(def, ctx) -> Vec<Finding>; validate_fleet(defs, ctx) -> Vec<Finding>
├── context.rs    # ValidationContext trait + StaticContext test impl
├── findings.rs   # Finding { code, severity, routine, track, step, message }
├── refs.rs       # reference resolution checks (uses snapshot::collect_refs)
├── contracts.rs  # variable/type contracts: $var paths satisfiable on every path; branch-on-bool
├── structure.rs  # unreachable steps, no-terminal tracks, retry exit, branch cycles, call recursion
├── consent.rs    # transmit closure: TX anywhere in call graph → mode declared; auto → ack present; mixed-chain stall warning
├── capability.rs # needs_internet/needs_radio vs station profile; contention warnings (same-rig parallel lanes)
└── fleet.rs      # enable-time: schedule collisions on same rig, same-effect overlap warnings
src/dryrun.rs     # build_dryrun_registry + DryRunScript (per-action scripted outcomes; optimistic/pessimistic presets)
tests/fixtures/routines/*.json   # the failure-taxonomy corpus (integration test walks the dir)
tests/validator_corpus.rs        # table-driven: fixture file ↔ expected finding codes
```

---

### Task 1: Finding vocabulary + context port (`findings.rs`, `context.rs`, `mod.rs` skeleton)

**Interfaces produced:**
- `Finding { code: &'static str, severity: Severity, routine: String, track: Option<String>, step: Option<StepId>, message: String }`; `Severity::{Error, Warning}`; `Finding` is serde-Serialize (MCP + UI consume it).
- `trait ValidationContext: Send + Sync { fn entity_exists(&self, r: &EntityRef) -> bool; fn action_descriptor(&self, name: &str) -> Option<ActionDescriptor>; fn routine_def(&self, name: &str) -> Option<RoutineDef>; fn enabled_routines(&self) -> Vec<RoutineDef>; fn station_profile(&self) -> StationProfile; }` with `StationProfile { has_internet: bool, rigs: Vec<String> }`.
- `StaticContext` (test impl, builder-style seeding) — public like plan 1's fakes.
- `validate(def, ctx) -> Vec<Finding>` skeleton dispatching to per-module check fns (added task by task); deterministic ordering (sort by code, then step).

**Test contract:** skeleton returns empty on a trivially valid routine; ordering determinism.

### Task 2: Reference + capability checks (`refs.rs`, `capability.rs`)

Codes: `UNRESOLVED_REF` (Error; every `@`-token via `collect_refs` checked against `entity_exists`), `UNKNOWN_ACTION` (Error; ActionStep.action not in descriptors), `NEEDS_INTERNET_OFFGRID` (Warning; `needs_internet` action while `!profile.has_internet`), `NO_RIG_CONFIGURED` (Warning; `needs_radio` action with empty `profile.rigs`), `SAME_RIG_PARALLEL_LANES` (Warning; ≥2 tracks each containing needs_radio steps — they will serialize).
**Test contract:** one fixture per code (corpus files land in Task 6, unit tests here); `$var` strings are NOT flagged as unresolved refs (the plan-1 ledger's immunity test lands HERE, explicitly).

### Task 3: Contract + structure checks (`contracts.rs`, `structure.rs`)

Codes: `UNSATISFIABLE_VAR` (Error; a `$path` or branch `on` referencing a step id that does not exist earlier in the same track, or an input not declared — v1 rule is lexical/order-based, documented as such), `BRANCH_ON_UNKNOWN` (Error; branch `on` unparseable as var path or input), `UNREACHABLE_STEP` (Error; step not reachable from track start following sequence+branch edges, excluding retry targets), `NO_TERMINAL_PATH` (Warning; track can run off the end without an End — allowed but flagged), `RETRY_ZERO_ATTEMPTS` (Error — the plan-1 runtime backstop's static twin), `RETRY_TARGET_MISSING`/`RETRY_TARGET_NOT_ACTION` (Error), `BRANCH_CYCLE` (Error; DFS over sequence+branch edges finds a cycle — the plan-1 ledger carry-in; the 10k runtime budget stays as defense-in-depth), `CALL_RECURSION` (Error; A→B→A through `ctx.routine_def` closure), `CALL_TARGET_MISSING` (Error).
**Test contract:** each code positive + negative case; the branch-cycle fixture is the exact shape the runtime-budget test used (backward then-jump).

### Task 4: Consent closure (`consent.rs`)

Codes: `TX_MODE_UNDECLARED` (Error; transmit step in call-graph closure, no transmit_mode … n.b. transmit_mode is a required field in v1 schema, so this fires only via closure: caller doesn't re-declare a callee's transmit-ness — define: caller's OWN mode governs its run; the check is that a routine whose closure transmits cannot be `transmit_mode`-less — structurally impossible in v1 — so the REAL check is:), `AUTO_TX_UNACKED` (Error; automatic mode + transmit closure + missing/empty `transmit_ack`), `MIXED_MODE_STALL` (Warning; automatic-mode routine calls an attended-mode routine with TX steps: "the unattended 03:00 run will pause for a click nobody is present to give" — message must name the callee and step), `ATTENDED_UNDER_SCHEDULE` (Warning; attended routine with a Schedule trigger — same stall class, direct form).
**Test contract:** the four-cell matrix from plan 2's consent stub (attended/auto × TX/no-TX) plus the mixed-chain fixture; closure walks `ctx.routine_def` recursively with a visited-set.

### Task 5: Fleet check + dry-run (`fleet.rs`, `dryrun.rs`)

- `validate_fleet(defs: &[RoutineDef], ctx) -> Vec<Finding>`: `SCHEDULE_COLLISION` (Warning; two enabled routines with needs_radio closures whose `next_fire` sequences provably coincide within a tolerance window — use plan 1's `next_fire` math over a 7-day horizon, document the horizon), `SAME_EFFECT_OVERLAP` (Warning; two enabled routines both containing the same data.* action on overlapping schedules).
- `DryRunScript { outcomes: HashMap<String, Vec<Value-or-error>>, default: Optimistic|Pessimistic }`; `build_dryrun_registry(real_descriptors: &[ActionDescriptor], script) -> ActionRegistry` — every real action name gets a FakeAction with MIRRORED capability flags; radio/internet actions consume script outcomes; local actions may run scripted too (v1: everything mocked — a dry-run touches NOTHING real). Engine integration: a dry-run is `Engine::start_run` with the dryrun registry and a journal stamped via a `"dry_run": true` field on `RunStarted` (extend `RunEvent::RunStarted` with `#[serde(default)] dry_run: bool` — additive, wire-compatible).
**Test contract:** collision fixture (30m + 6h same-rig → collision at the 6h marks); dry-run of the spec-§14 example routine executes branches per script, journal stamped dry_run, no real action invoked (assert via a canary real-registry FakeAction that must record zero calls).

### Task 6: Corpus + integration test (`tests/fixtures/`, `tests/validator_corpus.rs`)

One fixture JSON per finding code (valid-but-for-one-defect routines), a manifest mapping fixture → expected codes, and the table-driven integration test. Plus three fully-valid fixtures (the spec §1 grounding scenarios authored as real routine JSON — these double as documentation and as plan-5 demo seeds). Grep gate: no "workflow"; every `Finding` code string appears in at least one fixture expectation.

### Task 7: Push + CI + ledger

Push, PR (base: main), CI both arches, fix-forward. Ledger carry-outs for plan 4/5: monolith `ValidationContext` adapter (plan 4 wires it beside the MCP port so `routines_validate` serves both UI and MCP); `routines_save` should ATTACH findings to its response (plan 4 extends the command), enable/run must REFUSE on Errors (plan 4 wires enforcement where the store lives).
