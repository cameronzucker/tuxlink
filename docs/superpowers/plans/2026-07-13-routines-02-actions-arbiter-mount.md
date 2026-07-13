# Routines Plan 2/6 — Radio Arbiter, Real Action Catalog, Tauri Mount

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.
> **Fidelity note:** unlike plan 1, tasks here specify exact interfaces + test contracts and leave code authorship to the implementer (sonnet-class). The per-task review remains the quality gate. Monolith-touching tasks CANNOT be compile-verified on this Pi — implementers must be clippy-trap-armed (see Global Constraints) and verification lands via CI.

**Goal:** Mount the tuxlink-routines engine in the Tauri backend with a real action catalog, the radio arbiter, the definition store, and Radio Presets — after this plan, a JSON routine can genuinely drive the station.

**Architecture:** Mirrors the Elmer pattern: monolith module `src-tauri/src/routines/` (store, arbiter, actions, resolver, session/state, commands, events) over the `tuxlink-routines` crate. The arbiter is its own small module patterned on `PositionArbiter` (Mutex-wrapped single-owner state machine + proptest). Actions are thin `Action`-trait impls delegating to existing services; they hold `AppHandle`/service handles, never reimplement transport logic.

**Tech Stack:** Rust in the main `tuxlink` crate (tauri, tokio, serde, thiserror), `tuxlink-routines` crate from plan 1.

**Spec:** spec §6 (catalog), §9 (arbiter), §14 (storage). Consumes plan 1's locked interfaces: `Action`/`ActionDescriptor`/`ActionRegistry`, `Engine`/`EngineConfig{journal_dir, registry, resolver, now, default_timeout_s, lookup}`, `EntityResolver`, `RoutineDef`.

## Global Constraints

- The word "workflow" NEVER appears in code symbols, JSON keys, docs, or UI copy.
- Actions surface underlying errors VERBATIM in `StepError::Action.cause` — the actual VARA/CAT/HTTP error text, never a paraphrase.
- Any action that keys the transmitter has `transmits: true`; any action seizing the rig has `needs_radio: true`; internet actions declare `needs_internet: true` (spec §6 table is authoritative).
- A routine step NEVER preempts the human operator's radio use; the operator is a first-class lease holder (spec §9).
- Transmit-capable execution paths route through the existing consent posture: the engine refuses to start a run whose snapshot contains `transmits: true` steps unless the definition's `transmit_mode` is declared, and automatic mode requires a recorded `transmit_ack` (spec §4). Attended-mode pauses are stubbed in this plan (block on a consent channel; the UI wires it in plan 5) — stub = the run enters `AwaitingConsent` and a Tauri event is emitted; a `routines_consent_grant` command releases it.
- **Clippy/CI arming for monolith tasks (no local full compile):** adding a field to a no-`Default` struct breaks every full struct literal repo-wide; `Cargo.lock` regen for any new dep; CI runs `clippy --all-targets -D warnings` + full test suite on BOTH arches; `cargo check -p tuxlink` locally is permitted if warm but never block on it; write code grep-verified against existing call sites.
- Commit trailers as elsewhere on this branch; parent session commits.

## File Structure

```
src-tauri/src/routines/
├── mod.rs          # module mount + doc
├── store.rs        # DefinitionStore: routines/ dir CRUD, atomic writes, lookup fn
├── presets.rs      # RadioPreset entity: presets.json beside config.json
├── resolver.rs     # MonolithEntityResolver: @station-set/@preset/@identity/@template
├── arbiter.rs      # RadioArbiter: lease per rig, holders (Interactive|Run), wait/fail policy
├── actions/
│   ├── mod.rs      # build_registry(deps) -> ActionRegistry (all actions registered)
│   ├── radio.rs    # radio.connect, radio.listen, radio.aprs_send
│   ├── cat.rs      # rig.read_state, rig.validate_preset, rig.apply_preset, rig.switch_vfo, rig.tune_atu
│   ├── data.rs     # data.spacewx_wwv, data.spacewx_swpc, data.stationlist_update, data.read
│   └── local.rs    # local.compose, local.compose_catalog_request, local.set_identity, local.log, local.notify
├── session.rs      # RoutinesState: Arc<Engine>, consent channels, run registry (managed state)
├── commands.rs     # Tauri commands: routines_list/get/save/delete/enable/disable/run/cancel/run_status/journal/consent_grant
└── events.rs       # RoutinesEvent enum → app.emit("routines:event", …)
```

Modify: `src-tauri/src/lib.rs` (mod routines; .manage(RoutinesState); generate_handler! additions), `src-tauri/Cargo.toml` (dep on tuxlink-routines path crate) + `Cargo.lock`.

---

### Task 1: DefinitionStore + RadioPreset entity (`store.rs`, `presets.rs`)

**Files:** create `src-tauri/src/routines/mod.rs`, `store.rs`, `presets.rs`; modify `src-tauri/src/lib.rs` (mod routines only), `src-tauri/Cargo.toml` (+ `tuxlink-routines = { path = "tuxlink-routines" }`) + `Cargo.lock` regen.

**Interfaces produced:**
- `DefinitionStore::open(dir: PathBuf) -> Self` — dir = `config_path().parent().join("routines")`, created on open.
- `list(&self) -> Vec<RoutineSummary>` where `RoutineSummary { routine: String, transmit_mode: TransmitMode, enabled: bool, triggers: Vec<Trigger> }` (enabled flag lives in a sidecar `enabled.json` set, NOT in the definition — definitions stay portable).
- `get(&self, name: &str) -> Option<RoutineDef>`; `save(&self, def: &RoutineDef) -> Result<(), StoreError>` (atomic: tempfile + persist, same discipline as `write_config_atomic`); `delete`, `set_enabled(name, bool)`, `is_enabled(name) -> bool`.
- `lookup_fn(&self) -> Arc<dyn Fn(&str) -> Option<RoutineDef> + Send + Sync>` for `EngineConfig.lookup`.
- `RadioPresetStore` (presets.rs): `RadioPreset { name, frequency_hz: u64, mode: String, power_w: Option<u32>, atu: Option<bool> }`, CRUD over `radio-presets.json`, atomic writes.
- File format = one `<name>.json` per routine, content exactly the spec §14 shape (parse via `RoutineDef::parse`).

**Test contract (inline `#[cfg(test)]`, tempdir-based — these are monolith-module tests but filesystem-only, no tauri runtime needed):** save→get round-trip; save rejects invalid JSON shape (parse error surfaces); enabled flag survives store reopen and is absent from the definition file on disk; delete removes file + enabled entry; lookup_fn resolves saved routines; preset CRUD round-trip.

Steps: write failing tests → implement → `cargo check -p tuxlink 2>/dev/null || true` (warm-cache best-effort; do NOT block) → self-review → hand to parent (commit + CI verifies).

### Task 2: RadioArbiter (`arbiter.rs`)

**Interfaces produced:**
- `RadioArbiter::new(now: fn() -> i64)`; `Holder::{Interactive, Run { run_id: String, step: String } }`.
- `acquire(&self, rig: &str, holder: Holder, policy: BusyPolicy, timeout: Duration, cancel: &CancellationToken) -> Result<RadioLease, ArbiterError>` — async; `Wait` queues FIFO with timeout; `Fail` errors immediately naming the current holder verbatim (`ArbiterError::Busy { held_by: String, held_for_s: u64 }`).
- `RadioLease` releases on Drop; `interactive_acquire/interactive_release(&self, rig)` for the existing UI session paths to call (wired in Task 5); `operator_take(&self, rig)` — cancels the head run-holder's lease token (run pauses `AwaitingRadio` — engine side wired in Task 4); `status(&self, rig) -> Option<HolderInfo>`.
- Every acquire/release/timeout emits a `tracing::info!(target: "tuxlink::routines::arbiter", …)` structured event (feeds run journals context and the diagnostic jsonl).

**Test contract:** two runs contend → FIFO order; Fail policy errors immediately with holder named; Wait policy times out with holder named; interactive holder blocks runs but `operator_take` never affects interactive; drop releases; proptest invariant (patterned on `position/arbiter.rs`): at most one holder per rig at any time across arbitrary op sequences.

### Task 3: MonolithEntityResolver (`resolver.rs`)

**Interfaces produced:** `MonolithEntityResolver { presets: Arc<RadioPresetStore>, stations: <existing station-list service handle>, identities: <existing identity store handle>, templates: <existing template service handle> }` implementing `EntityResolver`. Resolution table: `@preset:<name>` → RadioPreset as JSON; `@station-set:<name>` → array of station callsigns (reuse the existing station-list/finder service — Explore recon: identity store at `identity_store_path()`, station data via the stations service used by Find-a-Station); `@identity:<name>` → identity record; `@template:<name>` → message template body. Unknown kind or name → `SnapshotError::UnresolvedRef` (verbatim token comes from plan 1's walker).

**Test contract:** each kind resolves from a seeded store; unknown name errors; unknown KIND errors (not silently passed through). Implementer must grep the real service seams first (`src-tauri/src/identity/`, station list modules) and adapt constructor params to what exists — the review verifies against the actual codebase.

### Task 4: Action catalog (`actions/*.rs`)

The big one. Every action from spec §6, each a struct implementing `Action` with the exact descriptor flags from the spec table. Delegation targets (from recon + implementer verification): `radio.connect` → the transport session layer used by `cms_connect`/`packet_connect` (station×band iteration loop lives in the action; forwards staged outbox traffic exactly like the existing connect paths); `radio.listen` → capture path (tuxlink-capture) + busy detector (audio RMS energy over N seconds; decoder-activity integration is a stretch goal, RMS is the v1 detector); CAT verbs → `tux-rig` `ManagedRig`/`Rig` trait via `ModemSession::rig` handles; `data.spacewx_wwv` → the shipped `wwv_offair` orchestration; `data.spacewx_swpc` → existing SWPC fetch; `data.stationlist_update` → Winlink gateway status API refresh path; `local.compose` / `local.compose_catalog_request` → the B2F composer + outbox used by Compose/Catalog-Request menu paths; `local.set_identity` → RUN-SCOPED only: the action writes to run vars (`{"identity": …}` consumed by later compose/connect actions via a run-context field), NEVER mutates global identity config; `local.log`/`local.notify` → station log write + Tauri notification.

**Radio-action lease discipline:** every `needs_radio` action acquires from the arbiter (rig id from params or default rig), holds for the operation, releases on completion/error/cancel (RAII lease). `on_radio_busy` policy + step timeout come in via params injected by the engine glue (Task 5 wires `BusyPolicy` from the ActionStep through a params envelope — document the envelope key `"_radio_busy_policy"` explicitly).

**Test contract:** actions are constructed with trait-object service seams (define narrow traits per dependency — e.g. `trait ConnectService`, `trait RigService` — with test fakes), so every action's logic (param validation, lease acquire/release ordering, verbatim error passthrough, output shape) unit-tests WITHOUT hardware or tauri. Each action: happy-path output shape test + verbatim-error test + (radio actions) lease-held-during/released-after test. The real service impls are thin adapter structs whose correctness CI's compile + existing integration tests cover.

**This task may be split by the executing controller into 4a (radio.rs), 4b (cat.rs), 4c (data.rs), 4d (local.rs) with parallel-safe file boundaries if wall-clock demands.**

### Task 5: Engine mount + consent stub + events (`session.rs`, `events.rs`, glue)

**Interfaces produced:**
- `RoutinesState { engine: Arc<Engine>, store: Arc<DefinitionStore>, arbiter: Arc<RadioArbiter>, runs: Mutex<HashMap<String, RunHandle-registry-entry>>, consent: ConsentRegistry }` built in `lib.rs` `.setup()` (needs resolved config dir), `.manage()`d.
- Engine wiring: `EngineConfig { journal_dir: <config>/routines-runs/, registry: build_registry(deps), resolver: MonolithEntityResolver, now: unix now fn, default_timeout_s: 300, lookup: store.lookup_fn() }`.
- **Consent enforcement at start:** `start_routine(state, name, args)` refuses (typed error) if snapshot contains transmit steps and mode undeclared/unacked (spec §4) — test with a transmit-flagged fake in the registry.
- **Attended-mode pause:** engine-side, a transmitting action's params envelope carries `"_transmit_mode": "attended"`; the action glue (a wrapper around transmit actions installed by build_registry when mode=attended) parks on `ConsentRegistry` (oneshot per run+step), emits `RoutinesEvent::AwaitingConsent { run_id, step }`, journals `StateChanged(AwaitingConsent)`, resumes on grant. `operator_take` + cancel paths release parked steps with `StepError::Cancelled`.
- `RoutinesEvent` enum (serde, camelCase): `RunStarted/RunFinished/StateChanged/AwaitingConsent/StepCompleted` → `app.emit("routines:event", &e)` via an event-sink task reading a broadcast channel the engine glue feeds (pattern: elmer `events.rs`).
- Launch recovery: `.setup()` calls `engine.recover()`, emits events for interrupted runs, and applies `on_interrupted: resume` policy (re-invoke from journal snapshot) vs `stay`.

**Test contract:** consent refusal matrix (attended/automatic/unacked-auto × has-TX/no-TX); attended pause parks and resumes on grant; recovery honors stay-vs-resume. Use plan-1 fakes + a stub AppHandle-free event sink (trait the sink).

### Task 6: Tauri commands (`commands.rs`) + registration

**Interfaces produced (all `#[tauri::command] pub async fn`, `Result<Dto, UiError>` per house style):** `routines_list`, `routines_get(name)`, `routines_save(def_json: String)` (parse → store.save; returns parse errors verbatim — the validator layer is plan 3, but shape errors surface now), `routines_delete(name)`, `routines_set_enabled(name, enabled)`, `routines_run(name, args) -> run_id`, `routines_cancel(run_id)`, `routines_run_status(run_id)`, `routines_journal(run_id) -> Vec<JournalEntry>`, `routines_consent_grant(run_id, step_id)`, `routines_presets_list/save/delete`, `routines_station_sets_list/save/delete`. Register in `generate_handler!` (lib.rs:1972 list) fully-qualified. Emit events where state changes.

**Plan amendment (2026-07-13, post-Task-3 review):** Task 3's recon found no pre-existing station-set concept and built `StationSetStore`; without CRUD commands `@station-set:` would be a hand-edit-JSON-only entity — unauthorable entities violate the shipped-end-to-end posture (ADR 0022). The command layer must also apply `valid_name`-style validation to preset AND station-set names before accepting them: `EntityRef::parse` cannot express names containing `:` or empty strings, so junk names would create unreferenceable entities.

**Test contract:** commands.rs follows the `search/commands.rs` service pattern — logic lives in testable service fns taking `&RoutinesState`; thin command shims. Unit tests on the service fns with tempdir stores + fake registry.

### Task 7: CI gate + branch verification

`git push` → PR → CI both arches: workspace clippy `--all-targets -D warnings`, full test suite. Fix-forward any CI finding. Grep gates: no "workflow" anywhere in `src-tauri/src/routines/`; every spec §6 action name present in `build_registry`; `generate_handler!` contains every new command.

## Deliberately deferred to later plans (recorded, not dropped)

- Validator layers 1–3 + dry-run → plan 3 (its work items from plan-1 reviews are in the SDD ledger).
- MCP tools → plan 4. UI surfaces + consent UI → plan 5 (the consent stub's event/command contract here is what plan 5 binds to). Dockable shell → plan 6.
