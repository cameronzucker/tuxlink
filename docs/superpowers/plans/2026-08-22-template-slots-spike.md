# Template+Slots Spike Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the template+slots routine-authoring surface (intake tool + compiler + real-port harness) and its evaluation instrument, per the ratified design, so the 27-run matrix can rule GO/NO-GO on the surface.

**Architecture:** A harness-only `RoutineAuthoringPort` in `tuxlink-mcp-core` serves a testserver-only tool `routine_template_compile`; the port is implemented in `tuxlink-mcp-testserver` by a deterministic template compiler on top of a `RoutineAuthoringService` extracted into `tuxlink-routines` (one implementation of store/validate/save shared with the product monolith). Typed `TuxlinkMcp::product`/`::harness` constructors with runtime `list_tools` proofs keep the product surface byte-identical. An independent Python grader compares runs against the freeze pack's gold fixtures.

**Tech Stack:** Rust (tuxlink-routines, tuxlink-mcp-core, tuxlink-mcp-testserver), Python 3 (grader), bash (runner), d3zwe driver. No new third-party packages (design §3: `tempfile`/`tracing`/`sha2` promoted within the locked graph only).

**Spec:** `docs/superpowers/specs/2026-08-19-template-slots-intake-design.md` (operator-approved 2026-08-22). Byte-authoritative fixtures: `dev/spikes/2026-08-13-ir-compiler-slice/freeze-v1/` (Task 0 ratifies; `MANIFEST.sha256` pins). The spec and freeze pack travel with this plan; executors read all three. Where this plan and the freeze pack both state content (copy strings, envelopes, lowerings), **the freeze pack wins byte-for-byte**.

## Global Constraints

- MSRV 1.95 (`incompatible_msrv` denied); pre-1.95 idioms only.
- No new third-party packages; lockfile diff audited and limited to local-package entries (design §3).
- This Pi does not finish cold monolith builds: leaf crates (`tuxlink-routines`, `tuxlink-mcp-core`, `tuxlink-mcp-testserver`) test locally with `cargo test -p <crate> --manifest-path src-tauri/Cargo.toml --locked`; anything touching `src-tauri/src` verifies in PR CI only.
- Product surface untouched: parity manifest and tool budget UNCHANGED; every PR cites `No row` per the campaign ledger; the product binary never links the intake tool (runtime proofs, Task 3).
- Completion copy is ASCII-only (2026-07-29 operator ruling); all copy strings come from `freeze-v1/completion-copy.md` verbatim.
- One git write per shell call; cd first; moniker trailer on every commit; branch `bd-tuxlink-3gaz7/template-slots-spike` (this worktree).
- Merges by steward subagent on green CI only; one Codex adversarial round on the compiler code BEFORE its PR merges (Task 9).
- Session 3 (evaluation) requires operator-freed serving (Inkling or Qwen, his policy — ASK first); build tasks need no inference. No sampling params, ever.
- Mid-build discovery that the spec or freeze pack is wrong = STOP and surface (falsified-premise rule); never quietly adapt.

**Session mapping:** Session 1 = Tasks 1-4 (harness architecture, one PR). Session 2 = Tasks 5-8 + Codex round (instrument, one PR). Session 3 = Task 10 (evaluation, results PR). Task 0 gates everything.

---

### Task 0: Freeze-gate ratification (NO CODE)

**Files:** none (review-only).

**Interfaces:**
- Consumes: `dev/spikes/2026-08-13-ir-compiler-slice/freeze-v1/` (7 artifacts + manifest).
- Produces: the operator's written ratification recorded in bd tuxlink-3gaz7; the pack becomes byte-frozen.

- [ ] **Step 1: Operator reviews the pack** — README.md (the D1-D10 decision points), tool-description.md, input-schema.json, result-envelopes.md, completion-copy.md, lowerings/, matrix-v1.json.
- [ ] **Step 2: Record ratification**

```bash
bd update tuxlink-3gaz7 --notes "Task-0 freeze ratified by operator <date>, quote: '<his words>'. Pack pinned at freeze-v1/MANIFEST.sha256 as committed in <sha>. D1-D10 accepted [or list overrides]."
```

- [ ] **Step 3: If the operator overrides any D-point** — amend the pack files, re-run `python3 hash-fill.py`, regenerate `MANIFEST.sha256`, commit as pack v1.1, and re-record. Implementation consumes only the ratified version.

**HARD GATE: no Task 1-10 work before this ratification exists in bd.**

---

### Task 1: `RoutineAuthoringService` extraction into `tuxlink-routines`

**Files:**
- Create: `src-tauri/tuxlink-routines/src/store.rs` (moved `DefinitionStore` + `revision_of` + `atomic_write`)
- Create: `src-tauri/tuxlink-routines/src/authoring.rs` (`RoutineAuthoringService`, `SavePrecondition`, `SaveOutcome`)
- Modify: `src-tauri/tuxlink-routines/src/lib.rs` (export new modules)
- Modify: `src-tauri/tuxlink-routines/Cargo.toml` (promote `tempfile`, `tracing`, `sha2` from the locked graph; `rust-version.workspace = true` already inherited)
- Modify: `src-tauri/src/routines/store.rs` → re-export shim (`pub use tuxlink_routines::store::*;`) so monolith call sites compile unchanged
- Modify: `src-tauri/src/routines/mod.rs` (atomic_write moves; keep a `pub(crate) use` shim)
- Modify: `src-tauri/src/mcp_ports.rs` save/validate call sites route through the service (LibraryChanged emission stays monolith-side via the post-save callback)
- Test: `src-tauri/tuxlink-routines/src/authoring.rs` `#[cfg(test)]` module

**Interfaces:**
- Consumes: `DefinitionStore` (`open/list/get/get_with_revision/save/delete/set_enabled`, `revision_of` = sha256 of stored bytes), `validate(def, ctx) -> Vec<Finding>` from `tuxlink_routines::validate`, `ValidationContext` (`validate/context.rs`).
- Produces (later tasks rely on these exact names):

```rust
// tuxlink-routines/src/authoring.rs
pub enum SavePrecondition {
    CreateOnly,
    MatchRevision(String), // product-graduation extension; implemented but only CreateOnly is used by the spike
}

pub struct SaveOutcome {
    pub revision: String,
    pub findings: Vec<Finding>, // real validator findings from the save-path validation
}

#[derive(Debug, PartialEq)]
pub enum SaveRefusal {
    NameExistsCreateOnly { name: String },
    RevisionMismatch { expected: String, actual: String },
    StoreIo(String),
    LockUnavailable,
}

pub struct RoutineAuthoringService { /* store: DefinitionStore, lock: Mutex<()>, post_save: Option<Box<dyn Fn(&str) + Send + Sync>> */ }

impl RoutineAuthoringService {
    pub fn open(dir: std::path::PathBuf) -> Self;
    pub fn with_post_save(self, cb: Box<dyn Fn(&str) + Send + Sync>) -> Self;
    pub fn store(&self) -> &DefinitionStore;
    pub fn validate_draft(&self, def: &RoutineDef, ctx: &dyn ValidationContext) -> Vec<Finding>;
    pub fn save(&self, def: &RoutineDef, pre: SavePrecondition, ctx: &dyn ValidationContext)
        -> Result<SaveOutcome, SaveRefusal>;
}
```

The lock is process-local and the docs say so (design §1). `save` under `CreateOnly` checks existence INSIDE the lock, refuses without touching bytes, saves otherwise, runs the post-save callback after a successful write.

- [ ] **Step 1: Write the failing regression test (CreateOnly bytes+revision)**

```rust
#[test]
fn create_only_collision_changes_no_bytes_and_no_revision() {
    let dir = tempfile::tempdir().unwrap();
    let svc = RoutineAuthoringService::open(dir.path().to_path_buf());
    let def = minimal_manual_def("wa-gateway-check"); // helper: schema_version 1, manual trigger, one local.log step, end
    let first = svc.save(&def, SavePrecondition::CreateOnly, &NullCtx).unwrap();
    let bytes_before = std::fs::read(dir.path().join("wa-gateway-check.json")).unwrap();
    let mut def2 = def.clone();
    def2.tracks[0].steps = vec![]; // would-be different bytes
    let refusal = svc.save(&def2, SavePrecondition::CreateOnly, &NullCtx).unwrap_err();
    assert_eq!(refusal, SaveRefusal::NameExistsCreateOnly { name: "wa-gateway-check".into() });
    let bytes_after = std::fs::read(dir.path().join("wa-gateway-check.json")).unwrap();
    assert_eq!(bytes_before, bytes_after);
    assert_eq!(svc.store().get_with_revision("wa-gateway-check").unwrap().1, first.revision);
}
```

`NullCtx` is a test `ValidationContext` impl returning empty capability/entity answers — model it on the fakes in `tuxlink-routines/src/fakes.rs`.

- [ ] **Step 2: Run it — expect FAIL** (`authoring` module does not exist):
`cargo test -p tuxlink-routines --manifest-path src-tauri/Cargo.toml --locked create_only_collision` → compile error.
- [ ] **Step 3: Move the store.** Copy `DefinitionStore` + `revision_of` from `src-tauri/src/routines/store.rs` and `atomic_write` from `src-tauri/src/routines/mod.rs:181` into `tuxlink-routines/src/store.rs` VERBATIM (same doc comments); leave re-export shims behind (`pub use tuxlink_routines::store::{DefinitionStore, RoutineSummary, StoreError, revision_of};`). Enumerate call sites first and confirm the shim covers them:

```bash
grep -rn "DefinitionStore\|revision_of\|atomic_write" src-tauri/src --include="*.rs" | grep -v "^src-tauri/src/routines/store.rs"
```

Acceptance: zero call-site edits outside the two shim files (if a call site names the module path explicitly, fix that call site to the re-export and record it in the PR body).
- [ ] **Step 4: Implement `authoring.rs`** per the Produces block. Name/revision validation moves with it only if Step 3's grep shows it living in store.rs; otherwise it stays put this task (record which). Check the store's own name rule here against freeze D8 (`^[a-z0-9]+(-[a-z0-9]+)*$`, 1..48): if the store is STRICTER, STOP and surface for a pack v1.1 (freeze D8's documented path).
- [ ] **Step 5: Run the test — expect PASS**, plus the whole crate: `cargo test -p tuxlink-routines --manifest-path src-tauri/Cargo.toml --locked`.
- [ ] **Step 6: Rewire the monolith save path** (`src-tauri/src/mcp_ports.rs` around the `AuthoringDispositionDto::classify` call sites at ~5380-5427): construct the service where the store is constructed today; pass the LibraryChanged emission as the post-save callback. `MonolithValidationContext` stays in the monolith (design §3.1). No behavior change intended: existing monolith tests are the gate.
- [ ] **Step 7: Audit the lockfile diff** — `git diff src-tauri/Cargo.lock` must touch only local-package entries. If any external version moves, STOP and surface.
- [ ] **Step 8: Commit**

```bash
git add src-tauri/tuxlink-routines/src/store.rs src-tauri/tuxlink-routines/src/authoring.rs src-tauri/tuxlink-routines/src/lib.rs src-tauri/tuxlink-routines/Cargo.toml src-tauri/src/routines/store.rs src-tauri/src/routines/mod.rs src-tauri/src/mcp_ports.rs src-tauri/Cargo.lock
```

```bash
git commit -m "refactor(routines): extract RoutineAuthoringService + DefinitionStore into tuxlink-routines (tuxlink-3gaz7)

Freeze: consumed freeze-v1 (D8 name-rule check done: <result>).
No row.

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 2: One catalog truth — shared descriptor + role metadata

**Files:**
- Create: `src-tauri/tuxlink-routines/src/catalog_meta.rs` (Tauri-free descriptor specifications + validator role tables + the MCP catalog projection: controls, triggers, definition template, sorting, DTO shape)
- Modify: `src-tauri/src/routines/actions/*.rs` (each real `Action::descriptor()` delegates to the shared metadata)
- Modify: `src-tauri/src/mcp_ports.rs` (`routines_actions_list` projection delegates to the shared projection)
- Test: `src-tauri/src/routines/validation.rs` (or nearest existing test module) — the two equality gates

**Interfaces:**
- Produces:

```rust
// tuxlink-routines/src/catalog_meta.rs
pub struct ActionSpec { pub name: &'static str, pub params: /* existing ParamSpec type */, pub outputs: /* existing */, /* role table entry */ }
pub fn shared_action_specs() -> &'static [ActionSpec];
pub fn catalog_projection(specs: &[ActionSpec]) -> serde_json::Value; // the routines_actions_list DTO shape
```

(Exact field types: reuse the existing descriptor/ParamSpec types — Step 1 identifies them from `src-tauri/src/routines/actions/radio.rs` `RADIO_APRS_SEND` and `local.rs` `LOCAL_LOG` descriptor sites; if those types are monolith-bound (Tauri-linked), the task moves the TYPES to tuxlink-routines first, re-export shim as in Task 1.)

- [ ] **Step 1: Write the failing equality-gate tests**

```rust
#[test]
fn product_registry_descriptor_set_equals_shared_metadata() {
    let shared: BTreeSet<&str> = tuxlink_routines::catalog_meta::shared_action_specs().iter().map(|s| s.name).collect();
    let product: BTreeSet<&str> = crate::routines::registry_action_names(); // existing registry enumeration; find it via grep "descriptor()" callers
    assert_eq!(shared, product);
}

#[test]
fn actions_list_payload_serializes_identically_via_shared_projection() {
    let via_shared = tuxlink_routines::catalog_meta::catalog_projection(tuxlink_routines::catalog_meta::shared_action_specs());
    let via_product = /* current routines_actions_list building path, called directly */;
    assert_eq!(via_shared, via_product);
}
```

- [ ] **Step 2: Run — expect FAIL** (module missing). CI-only if the test must live monolith-side.
- [ ] **Step 3: Move descriptor specs + role tables** into `catalog_meta.rs`; each `Action::descriptor()` becomes a delegation; `routines_actions_list` uses `catalog_projection`.
- [ ] **Step 4: Tests green** (local for tuxlink-routines; monolith gates in CI — push a WIP commit and let PR CI verify before proceeding to Task 3 if any monolith test was touched).
- [ ] **Step 5: Commit** (same trailer form as Task 1; message `refactor(routines): single catalog truth - shared descriptor+role metadata (tuxlink-3gaz7) ... No row.`).

---

### Task 3: Typed router constructors + runtime list_tools proofs

**Files:**
- Modify: `src-tauri/tuxlink-mcp-core/src/router.rs` (constructors; harness-only registration of `routine_template_compile`)
- Modify: `src-tauri/tuxlink-mcp-core/src/ports.rs` (the `RoutineAuthoringPort` trait + envelope DTOs)
- Modify: `src-tauri/src/mcp_server.rs` or wherever `TuxlinkMcp::new` is called in the product (grep `TuxlinkMcp::new`) → `TuxlinkMcp::product`
- Modify: `src-tauri/tuxlink-mcp-testserver/src/main.rs` → constructor choice by launch flag
- Test: `src-tauri/tuxlink-mcp-core/src/router.rs` `#[cfg(test)]` — the three runtime proofs

**Interfaces:**
- Produces:

```rust
// ports.rs
pub trait RoutineAuthoringPort: Send + Sync {
    /// The WHOLE intake operation behind one seam: compile, validate, maybe save.
    /// Envelope-level violations are rejected by the router before this is called.
    fn template_compile(&self, template: &str, slots: &serde_json::Value, save: bool)
        -> Result<TemplateCompileResult, TemplateEnvelopeError>;
}

pub struct TemplateCompileResult(pub serde_json::Value); // serialized per freeze-v1/result-envelopes.md; the port owns the exact shape
pub struct TemplateEnvelopeError { pub code: String, pub message: String } // frozen error-text form

// router.rs
impl TuxlinkMcp {
    pub fn product(state: Arc<McpState>) -> Self;                     // exactly today's tool set
    pub fn harness(state: Arc<McpState>, authoring: Arc<dyn RoutineAuthoringPort>) -> Self; // product + routine_template_compile
}
// TuxlinkMcp::new remains as a deprecated alias for product() for one PR, then is removed in the same PR after call sites move (grep proves zero remaining).
```

The router-side handler for `routine_template_compile`: validates the envelope against the frozen schema law (required keys, undeclared keys, slots-is-object AFTER the existing one-parse absorption boundary, scalar/null leaves, bands array-of-strings) producing `TemplateEnvelopeError` with the frozen text form; then delegates to the port. Tool description = `freeze-v1/tool-description.md` between the BEGIN/END markers, via `include_str!` + marker extraction, so the frozen bytes ARE the shipped bytes.

- [ ] **Step 1: Write the three failing runtime proofs**

```rust
#[tokio::test]
async fn product_list_tools_equals_parity_manifest_set() {
    let router = TuxlinkMcp::product(test_state());
    let tools: BTreeSet<String> = list_tool_names(&router).await; // helper over the MCP list_tools handler
    let manifest: BTreeSet<String> = parity_manifest_tool_names(); // parse docs/parity/parity-manifest.json (include_str! relative path)
    assert_eq!(tools, manifest); // 95 today; the manifest is the source, not the number
}

#[tokio::test]
async fn harness_minus_product_is_exactly_the_intake_tool() {
    let p = list_tool_names(&TuxlinkMcp::product(test_state())).await;
    let h = list_tool_names(&TuxlinkMcp::harness(test_state(), Arc::new(NullAuthoring))).await;
    let diff: Vec<_> = h.difference(&p).collect();
    assert_eq!(diff, vec!["routine_template_compile"]);
    assert!(p.difference(&h).next().is_none());
}

#[tokio::test]
async fn shared_tool_schemas_are_byte_identical_across_constructors() {
    let p = list_tools_full(&TuxlinkMcp::product(test_state())).await;   // name -> serialized schema bytes
    let h = list_tools_full(&TuxlinkMcp::harness(test_state(), Arc::new(NullAuthoring))).await;
    for (name, schema) in &p { assert_eq!(Some(schema), h.get(name), "schema drift on {name}"); }
}
```

`NullAuthoring` implements the port with `unimplemented!()` bodies — these tests exercise registration only.
- [ ] **Step 2: Run — expect FAIL** (constructors missing): `cargo test -p tuxlink-mcp-core --manifest-path src-tauri/Cargo.toml --locked`.
- [ ] **Step 3: Implement** constructors + trait + envelope validation + frozen-description include. The intake tool's input schema in the tool listing = `include_str!` of `freeze-v1/input-schema.json` (path constant from `CARGO_MANIFEST_DIR`; strip the `$comment` when serving if the MCP layer objects, and if stripped, the runtime proof for the intake tool asserts against the stripped form — record which).
- [ ] **Step 4: Move product call sites** to `TuxlinkMcp::product`; delete `new`. `grep -rn "TuxlinkMcp::new" src-tauri | wc -l` must print 0.
- [ ] **Step 5: Tests green locally** for mcp-core; parity CI (`src-tauri/src/parity_check.rs` + `src/parityManifest.test.ts`) untouched and green in PR CI — the textual scan sees the same product registrations (design round-4 F4: the runtime proofs are the seam's guarantee, the manifest stays as-is).
- [ ] **Step 6: Commit** (`feat(mcp-core): typed product/harness constructors + runtime list_tools proofs (tuxlink-3gaz7) ... No row.`).

---

### Task 4: Testserver real authoring harness

**Files:**
- Create: `src-tauri/tuxlink-mcp-testserver/src/harness_context.rs` (the harness `ValidationContext`: temp store, shared action metadata from Task 2, seeded station sets, declared station profile)
- Modify: `src-tauri/tuxlink-mcp-testserver/src/main.rs` (launch flags; real routines port replaces `mocks::MockRoutines`; constructor choice)
- Modify: `src-tauri/tuxlink-mcp-testserver/src/mocks.rs` (MockRoutines retired from the routines slot; keep the file compiling for other mocks)
- Modify: `src-tauri/tuxlink-mcp-testserver/Cargo.toml` (add `tuxlink-routines` path dep)
- Test: `src-tauri/tuxlink-mcp-testserver/src/harness_context.rs` `#[cfg(test)]`

**Interfaces:**
- Consumes: `RoutineAuthoringService` (Task 1), `catalog_meta` (Task 2), `TuxlinkMcp::harness`/`product` (Task 3).
- Produces:

```rust
pub struct HarnessWorld { pub service: Arc<RoutineAuthoringService>, pub ctx: Arc<HarnessValidationContext> }
pub fn build_harness_world(store_dir: &Path, seed_sets: &[&str], seed_routines: &[&str]) -> HarnessWorld;
```

Launch environment contract (frozen here; the runner consumes it):
- `TUXLINK_TSLOTS_INTAKE=1` → `TuxlinkMcp::harness` (intervention arm); unset → `TuxlinkMcp::product` (CTRL arm). The ONLY difference between arms (design §4 CTRL).
- `TUXLINK_TSLOTS_STORE_DIR=<dir>` → the temp definition store.
- `TUXLINK_TSLOTS_SEED_ROUTINES=name1,name2` → pre-save those defs from `freeze-v1/lowerings/<name>.json` before serving (RS-collision cells).
- Seeded station sets are ALWAYS `wa-gateways,nv-gateways,or-coast-gateways` (freeze matrix `harness_seeds`) — hardcoded in `build_harness_world`, not env-variable (fewer degrees of freedom in the instrument).

`routines_run` through the real port returns the pinned error string: `"execution is out of scope in this harness (not your call's fault); authoring, compiling, and saving remain available"` (design §3.4) — an invalid-request ERROR through the existing stack.

- [ ] **Step 1: Failing tests** — (a) `build_harness_world` seeds resolve: `validate` of the wa-gateway-check lowering (loaded from `freeze-v1/lowerings/wa-gateway-check.json` via `include_str!`) against the harness ctx returns NO unresolved-station-set finding; (b) the same def with `stations: "emcomm-critical"` DOES return the blocking finding; (c) `routines_run` handler yields the pinned string byte-for-byte (`assert_eq!` against the literal).
- [ ] **Step 2: Run — expect FAIL**; `cargo test -p tuxlink-mcp-testserver --manifest-path src-tauri/Cargo.toml --locked`.
- [ ] **Step 3: Implement** context + wiring + flags. What "a station set resolves" means mechanically: read the real validator's station-set lookup on `ValidationContext` (Task 1's Step 1 `NullCtx` work will have identified the method) and back it with a fixed map for the three seeded names.
- [ ] **Step 4: Tests green.**
- [ ] **Step 5: Commit** (`feat(testserver): real authoring harness - seeded context, launch flags, pinned routines_run refusal (tuxlink-3gaz7) ... No row.`). Open the Session-1 PR (draft), hand to a steward on green; Session 2 may proceed in the worktree while the steward watches.

---

### Task 5: The compiler

**Files:**
- Create: `src-tauri/tuxlink-mcp-testserver/src/template_compiler/mod.rs` (registry, lowerings, findings)
- Create: `src-tauri/tuxlink-mcp-testserver/src/template_compiler/normalize.rs` (the frozen alias grammar)
- Create: `src-tauri/tuxlink-mcp-testserver/src/template_compiler/goldens.rs` (`#[cfg(test)]` — golden + adversarial + execution tests)
- Test: `goldens.rs`

**Interfaces:**
- Consumes: freeze pack via `include_str!` (`matrix-v1.json`, `lowerings/*.json`, `completion-copy.md` strings transcribed into a `copy.rs` const table with a test asserting each const appears verbatim in the frozen file — the copy table is markdown, so the consts are checked-against, not parsed-from).
- Produces:

```rust
pub enum Lowered {
    Ok { def: RoutineDef, normalized: BTreeMap<String, serde_json::Value>, behavior_summary: String },
    Refused { findings: Vec<CompileFinding> },
}
pub struct CompileFinding { pub code: &'static str, pub slot: Option<String>, pub value: Option<String>, pub rule: String, pub remedy: Option<String>, pub fault: Option<String> }
pub fn compile(template: &str, slots: &serde_json::Map<String, serde_json::Value>) -> Lowered;
```

Behavior is 100% pinned by the freeze pack: registry = the three template ids; slot tables per `tool-description.md`; refusal codes per `result-envelopes.md`; normalization per the alias grammar (full-input consumption, ASCII-folded case-insensitive, checked arithmetic, 1s..30d, s/m/h only, band table = the 10 labels, `$station`/`$band` rules D9); lowerings must serde-value-equal the three frozen exemplar files for the worked-example inputs.

- [ ] **Step 1: Write the failing golden tests FIRST — all of them:**

```rust
#[test]
fn wa_gateway_check_lowering_matches_frozen_exemplar() {
    let slots = worked_example_slots_primary(); // transcribed from tool-description.md's first worked call
    let Lowered::Ok { def, .. } = compile("scheduled-connect-with-fallback", &slots) else { panic!("refused") };
    let frozen: serde_json::Value = serde_json::from_str(include_str!(FREEZE!("lowerings/wa-gateway-check.json"))).unwrap();
    assert_eq!(serde_json::to_value(&def).unwrap(), frozen);
}
// + noon_status_lowering_matches_frozen_exemplar (window variant)
// + hourly_heartbeat_lowering_matches_frozen_exemplar (save example's normalized "1 hour" -> "1h")
```

Adversarial goldens (one test each, refusal code asserted): trailing text (`"15 minutes then retry"` → SLOT_NOT_A_DURATION); day unit (`"2 days"` → SLOT_NOT_A_DURATION); zero/negative/overflow (`"0m"`, `"-5m"`, `"999999999h"` → DURATION_OUT_OF_RANGE); Unicode lookalike (`"15\u{2009}minutes"` thin-space and `"４0m"` fullwidth → refusal, never silent acceptance); band unknown (`"2 meters"`, `"44m"` → BAND_UNKNOWN); duplicate (`["40m","40 meters"]` → BAND_DUPLICATE — alias-collision case); empty bands → BANDS_EMPTY; equal window endpoints (`"08:00-08:00"` → WINDOW_ENDPOINTS_EQUAL); overnight window (`"22:00-06:00"` → ACCEPTED); malformed window (`"8am-6pm"` → SLOT_NOT_A_WINDOW); window without schedule → WINDOW_WITHOUT_SCHEDULE; `$band` in failure_log → SLOT_TOKEN_UNAVAILABLE; `$stations` in success_log → SLOT_TOKEN_UNKNOWN; unknown template → TEMPLATE_UNKNOWN with all three ids in the rule; unknown slot → SLOT_UNKNOWN; missing required slot → SLOT_MISSING; nullable omission compiles (D1); name rule violations (`"WA_check"`, 49 chars) → NAME_INVALID.

Execution goldens (two-terminal-ends invariant, round-5 F1): run the wa-gateway-check def through the `tuxlink-routines` executor with fakes (`fakes.rs`) forcing `radio.connect` → `connected:true` and assert the run terminates at `e1` not-failed with the s2 log emitted; force `connected:false` and assert termination at `e2` failed with reason `"no gateway reachable"` and s3 emitted, s2 never.

- [ ] **Step 2: Run — expect FAIL across the board.**
- [ ] **Step 3: Implement `normalize.rs`** (pure functions: `duration(&str) -> Result<String, CompileFinding>`, `band(&str) -> Result<&'static str, CompileFinding>`, `window(&str) -> Result<String, CompileFinding>`, `name(&str) -> Result<String, CompileFinding>`, `tokens_check(slot_name, text) -> Result<(), CompileFinding>`).
- [ ] **Step 4: Implement `mod.rs`** registry + lowering + behavior_summary rendering (frozen sentence forms from `result-envelopes.md`).
- [ ] **Step 5: All goldens green:** `cargo test -p tuxlink-mcp-testserver --manifest-path src-tauri/Cargo.toml --locked template_compiler`.
- [ ] **Step 6: Commit** (`feat(testserver): template compiler with frozen goldens (tuxlink-3gaz7) ... No row.`).

---

### Task 6: The intake tool end-to-end in the testserver

**Files:**
- Create: `src-tauri/tuxlink-mcp-testserver/src/authoring_port.rs` (implements `RoutineAuthoringPort` over compiler + service + harness ctx)
- Modify: `src-tauri/tuxlink-mcp-testserver/src/main.rs` (wire the port into `TuxlinkMcp::harness`)
- Test: `src-tauri/tuxlink-mcp-testserver/tests/intake_tool.rs` (integration: through the MCP layer)

**Interfaces:**
- Consumes: `compile` (Task 5), `RoutineAuthoringService.validate_draft/save` (Task 1), `HarnessWorld` (Task 4), envelope DTOs (Task 3).
- Produces: the full frozen result envelope. Assembly rules (all from `freeze-v1/result-envelopes.md`, byte-authoritative):
  - `Lowered::Refused` → `lowering:"failed"`, `persistence:"not_saved"`, `draft_validation:"n/a"`, findings, refused copy.
  - `Lowered::Ok` + `save:false` → `validate_draft`; `draft_validation` from findings (none → valid; advisory-only → advisory; any blocking → blocked); copy row accordingly.
  - `Lowered::Ok` + `save:true` → `service.save(CreateOnly)`; success → `persistence:{"saved":{...}}` + the verbatim `AuthoringDispositionDto` (reuse `AuthoringDispositionDto::classify` — it lives in `tuxlink-mcp-core/src/ports.rs:1806`); `NameExistsCreateOnly` → `save_refused` envelope; `StoreIo`/`LockUnavailable` → the pinned error states with `fault` attribution.
  - `submitted_slots` verbatim always; `normalized_slots`/`behavior_summary` only on lowering ok; absent-not-null discipline.

- [ ] **Step 1: Failing integration tests** — drive the REAL MCP handler (the same path d3zwe hits): (a) worked-example call → envelope equals the byte-fixed expected JSON (build the expected value in the test from the frozen exemplar + copy consts); (b) save:true happy path → revision present, store contains the def, disposition state `"valid"`; (c) CreateOnly collision (pre-seed via `TUXLINK_TSLOTS_SEED_ROUTINES` path helper) → `save_refused` envelope + bytes/revision unchanged; (d) nested-object slot → `TemplateEnvelopeError` with `SLOT_VALUE_NOT_SCALAR` naming `slots.window`, frozen text form; (e) stringified WELL-FORMED slots → absorbed, compiles, raw retained (assert the telemetry capture contains the original string form — the capture seam from the in-situ runner's verbatim log); (f) malformed stringified slots → `SLOTS_NOT_OBJECT`.
- [ ] **Step 2: Run — expect FAIL.**
- [ ] **Step 3: Implement `authoring_port.rs`.**
- [ ] **Step 4: Green:** `cargo test -p tuxlink-mcp-testserver --manifest-path src-tauri/Cargo.toml --locked`.
- [ ] **Step 5: Commit** (`feat(testserver): routine_template_compile end-to-end - frozen envelopes, save path, pinned error states (tuxlink-3gaz7) ... No row.`).

---

### Task 7: The independent grader + mutation tests

**Files:**
- Create: `dev/spikes/2026-08-13-ir-compiler-slice/grade-tslots.py`
- Create: `dev/spikes/2026-08-13-ir-compiler-slice/grader-mutations/` (mutation fixtures: crafted fake run captures)

**Interfaces:**
- Consumes: `freeze-v1/matrix-v1.json` (gold), `freeze-v1/lowerings/skeleton-*.json` + `INSTANTIATION.md` rules, run captures from the Task-8 runner (same verbatim `.txt`/JSONL format as `runs-insitu/`).
- Produces: per-run verdict JSON `{run, verdict: PASS|PASS_WITH_NOTE|FAIL|INSTRUMENT_INVALID, checks: {...}}` + a matrix summary implementing the mechanical GO/NO-GO formula (design §4) verbatim.

Independence rules (design §4, round-3 F1): pure Python; imports NOTHING from the compiler; its only inputs are freeze files and captures; it re-implements skeleton instantiation from `INSTANTIATION.md` (hole substitution + `$station`→`$s1.station` rewrite + trigger forms) and compares serde-value-style (parsed JSON equality) against the def the tool result carried. Any grader-vs-compiler disagreement on a run → `INSTRUMENT_INVALID` (quarantine semantics, never a model FAIL).

- [ ] **Step 1: Write the mutation fixtures FIRST** — six crafted captures the grader MUST flag: (1) false acceptance (compiler said valid, values diverge from gold `expected_slots_exact`); (2) false refusal (gold expects compile_ok, capture shows refusal); (3) wrong-template lowering (log-entry def where primary expected); (4) changed value (`every` 3h vs gold 2h); (5) dropped field (failure_log missing from def); (6) instrument split (tool-result def ≠ grader-instantiated skeleton → INSTRUMENT_INVALID). Plus one clean capture that must PASS.
- [ ] **Step 2: `python3 grade-tslots.py --self-test grader-mutations/` — expect FAIL** (script doesn't exist), then implement until the self-test reports 6/6 caught + 1/1 clean-pass. The self-test is the grader's own gate and runs in CI? NO — dev/ tooling stays out of CI; it runs at Session-3 start as a pre-flight (runner refuses to grade if self-test fails).
- [ ] **Step 3: Implement full per-cell checks:** trace matching against `passing_traces`/`pass_with_note_traces`; exact-slot gates; predicate gates (kebab, contains, nonempty); unchanged-field verbatim gates; prohibited-content + smuggling keyword sweep over decoded string leaves (candidate flags for the human pass, never auto-FAIL on keywords alone); final-message truth checks (claims vs actual last result state — regex candidates + human confirm); call budget (≤4, excess flags); identical-repeat thrash detection; harness-integrity assertions reported separately (echo vs captured input, decoded leaves).
- [ ] **Step 4: Commit** (`feat(spike): independent tslots grader with mutation self-test (tuxlink-3gaz7) ... No row.`).

---

### Task 8: Matrix runner v2

**Files:**
- Create: `dev/spikes/2026-08-13-ir-compiler-slice/run-tslots.sh` (adapt `run-insitu.sh` — same resumable skip-if-output-exists per-run guard, same verbatim capture)
- Create: `dev/spikes/2026-08-13-ir-compiler-slice/runs-tslots/` (output dir; `.txt` captures like `runs-insitu/`, gitignore-safe naming — plain `.txt`, the `*.log` trap from the in-situ round)

**Interfaces:**
- Consumes: `matrix-v1.json` (asks + hashes + seeds + arm), launch env contract (Task 4), d3zwe driver + serving pre-flight from `run-insitu.sh`.
- Produces: per-run capture files consumed by `grade-tslots.py`.

Runner obligations (all design §4): recompute and verify each `ask_sha256` before sending (mismatch = abort INSTRUMENT_INVALID); fresh testserver per run with the run's seeds; arm via `TUXLINK_TSLOTS_INTAKE`; launch assertions ONCE per arm pair before the matrix: list_tools set-difference is exactly `{routine_template_compile}`, shared schemas byte-identical, catalog hash + token count recorded (a `--launch-check` mode that boots both arms and diffs); serving provenance per run (model fingerprint, serving config hash, effective sampling as reported by the server, cache state); NO sampling params sent, ever; UDS socket dir mode 700 (`/run/user/<uid>/tuxlink-tslots/`); detached long runs per the ops runbook (setsid + bracketed pgrep + loud stall alarms).

- [ ] **Step 1:** Copy `run-insitu.sh` → `run-tslots.sh`; strip the old matrix; read cells from `matrix-v1.json` via `python3 -c` helpers (no jq dependency drift).
- [ ] **Step 2:** Implement hash verification + seeds + arm launch + `--launch-check`.
- [ ] **Step 3:** Dry-run against a LOCAL stub (no serving): `TUXLINK_TSLOTS_DRYRUN=1` mode replays a canned model that emits the worked-example call — proves capture format + hash gate + seed plumbing end-to-end without inference.
- [ ] **Step 4: Commit** (`feat(spike): tslots matrix runner v2 (tuxlink-3gaz7) ... No row.`).

---

### Task 9: Codex adversarial round on the compiler (pre-merge gate, Session 2)

- [ ] **Step 1:** Stdin-prompt Codex round (per CLAUDE.md recipe; detached; ROUND-DONE marker) targeting the Session-2 diff: attack angles = lowering fidelity vs the frozen exemplars, refusal-coverage gaps vs the frozen code table, id/determinism, normalization bypasses (Unicode, aliasing, bounds), envelope-law bypasses, grader-independence violations. Output tee'd to `dev/adversarial/2026-08-<dd>-tslots-compiler-codex.md` (gitignored).
- [ ] **Step 2:** Ground every finding against source; fix real ones in-branch; record findings + dispositions in the PR body.
- [ ] **Step 3:** Session-2 PR ready → steward merges on green. **The PR does not merge before this round completes** (SPEC constraint).

---

### Task 10: Session 3 — the evaluation (operator-gated serving)

- [ ] **Step 1:** ASK the operator to free a Spark model (Inkling or Qwen — his call); record which + full serving provenance. Do not touch serving otherwise (control-plane-only policy).
- [ ] **Step 2:** Grader self-test pre-flight; runner `--launch-check`; then the 27-run matrix (24 intervention + 3 CTRL), resumable, detached, monitored per the ops runbook.
- [ ] **Step 3:** `grade-tslots.py` full pass → mandatory eyeball pass over every capture (smuggling/relay human grading per the codebook; blind to cell id where feasible) → results doc `RESULTS-<date>-tslots.md` in the spike dir: per-cell table, the mechanical GO/NO-GO formula output, INSTRUMENT_INVALID quarantines named, raw appendix pointers, serving provenance, "failed under this serving configuration" framing.
- [ ] **Step 4:** The ruling brief for the operator (plain narrative, no code-soup): PASS/FAIL per pre-registered bar, findings, and if FAIL the failure-shape analysis. **bd tuxlink-3gaz7 closes on the operator's ruling, not on the merge.** The spike's disposition triggers the campaign ledger's GO/NO-GO consequence for rows 11-15 (design §6).
- [ ] **Step 5:** Results PR → steward; thin handoff per ADR 0031.

---

## Self-review (performed at authoring, 2026-08-22)

- **Spec coverage:** §1 intake tool → Tasks 3/5/6; §2 compiler/registry → Task 5; §3.1 service → Task 1; §3.2 catalog → Task 2; §3.3 constructors/proofs → Task 3; §3.4 routines_run → Task 4; §4 instrument → Tasks 7/8/10 (cells/traces/bars live in the frozen matrix, consumed not restated); §5 edge semantics → repeat-notice is runner-layer provenance: Task 8 records it, and the runner-level pinning test named by the design already exists in tuxlink-agent-runner — Session-2 verifies it covers byte-identical `refused` results and errors-reset, extends it if not (gap-check step folded into Task 8 Step 3); §6 process → Tasks 0/9/10 + session mapping.
- **Placeholder scan:** the `ActionSpec`/registry-enumeration types in Task 2 and the exact `ValidationContext` station-set method in Task 4 are deliberately named-by-location rather than fully typed — they are EXISTING types the mover must read at the named sites, not new design; every NEW type is fully specified. No TBDs.
- **Type consistency:** `SavePrecondition`/`SaveRefusal`/`SaveOutcome` (Task 1) are the exact names Tasks 5/6 consume; `RoutineAuthoringPort::template_compile` (Task 3) is what Task 6 implements; `Lowered`/`CompileFinding` (Task 5) are what Task 6 assembles from.
- **Known risk, stated:** Task 1's monolith rewire is the widest blast radius; its gate is the untouched monolith test suite in CI plus zero-call-site-edit acceptance. If the store move turns out to drag Tauri-bound types, STOP and surface (falsified-premise rule) — the design committed to a clean extraction and the operator should see the contradiction, not a workaround.
