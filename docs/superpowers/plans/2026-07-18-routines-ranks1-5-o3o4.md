# Routines Ranks 1-5 + O3/O4 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the compat-tree ranks 1-5 routines actions (status reads, find_stations, config reads, docs_search, first config-write family with the writes_config consent class) plus observability O3/O4 (child run ids, end events), per the adversarially-hardened design spec.

**Spec (read FIRST, canonical for every contract):**
`docs/superpowers/specs/2026-07-18-routines-round2-ranks1-5-o3o4-design.md`

**Architecture:** The engine leaf crate (`src-tauri/tuxlink-routines`) gains journal variants, the invoker split, the shared closure walk + digest, and the `writes_config` flag; the monolith (`src-tauri/src`) gains the action/seam implementations mirroring the MCP DTOs and owns the child-start registry bridge; the frontend gains History rows, consent-surface branching, and ack validity/enumeration UI. Six PR groups, merged in order; the tolerant journal reader merges FIRST.

**Tech Stack:** Rust (tokio, serde, sha2 for the digest), React 18 + TS, vitest.

## Global Constraints

- **Rust compiles/tests run on R2 ONLY** (`ssh r2-poe`; rustup toolchain, NOT distro 1.75; MSRV 1.75 — no post-1.75 APIs, clippy denies `incompatible_msrv`). This Pi never runs cargo. Use `cargo test --manifest-path src-tauri/Cargo.toml --locked -p tuxlink-routines` (leaf) or full-workspace as each task states, and workspace clippy `--all-targets --locked -- -D warnings` before any PR.
- The R2 build tree for this branch: sync the worktree to R2 via
  `rsync -a --delete --exclude node_modules --exclude target /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-iizmk-round2-ranks-o3o4/ r2-poe:~/build/ranks1-5/` then run cargo over ssh in `~/build/ranks1-5`. Commits happen on the Pi worktree only (subagents write code + report; the PARENT commits — subagents must NOT commit in worktrees).
- Frontend tests on the Pi: `pnpm vitest run src/routines` + `pnpm typecheck` (CI runs the FULL vitest + clippy `--all-targets`; scoped local green is not CI green).
- `schema_version` stays **1**. All new `RoutineDef` fields are `#[serde(default)]` optional. No `deny_unknown_fields` anywhere.
- New journal fields/variants: additive serde, snake_case tags, `#[serde(default, skip_serializing_if = "Option::is_none")]` on optional fields.
- Kill test fixtures with SIGKILL, never SIGSEGV (no core-dump signals — apport popups).
- vitest invoke-mocks: mocks are called with NO args at teardown; follow the existing `mockClear` patterns in `src/routines` tests.
- Every task: BEFORE starting, read `docs/pitfalls/testing-pitfalls.md` and follow TDD (failing test -> implement -> green). BEFORE marking complete: re-check tests against testing-pitfalls, confirm error paths covered, run the task's named test commands and paste output. After every PR group: minimum three review rounds; keep going past three if the third still finds substantive issues.
- Conventional commits with scope, `Agent: alder-oriole-cedar` trailer + `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.
- Do NOT touch `.local/converge-build-worktree`. Do NOT run any transmit-capable binary. Do NOT add safeguards beyond spec (no extra caps/timers).

---

## PR group A — tolerant journal reader (merges FIRST, alone)

### Task A1: per-line tolerant `read_journal`

**Files:**
- Modify: `src-tauri/tuxlink-routines/src/journal.rs` (read_journal ~181-193, scan_interrupted ~196-224; add `RawJournalEntry`)
- Test: same file `#[cfg(test)]`

**Interfaces:**
- Produces: `read_journal` returns entries where an unparseable `event` becomes `RunEvent::Unknown` (new unit-like catch variant is NOT possible with internal tagging — use the envelope approach below). Exact contract: add
  ```rust
  /// Envelope-level decode: parse the line as
  /// `{ts_unix, run_id, seq, event: serde_json::Value}` first; then try
  /// `RunEvent::deserialize(event)`. On failure the entry is kept with
  /// `event: RunEvent::Opaque { raw: Value }`.
  ```
  Add variant to `RunEvent`:
  ```rust
  /// Forward-compat catch-all (design §0.4): a journal line written by a
  /// NEWER build whose event type this build does not know. Never written
  /// by this build; constructed only by the tolerant reader.
  #[serde(skip)]
  Opaque { raw: serde_json::Value },
  ```
  Note `#[serde(skip)]` on a variant is not valid for deserialize-into; implement instead via a two-step reader (do NOT put Opaque in serde at all):
  ```rust
  #[derive(Deserialize)]
  struct RawEntry { ts_unix: i64, run_id: String, seq: u64, event: serde_json::Value }

  pub fn read_journal(path: &Path) -> Result<Vec<JournalEntry>, JournalError> {
      // per line: parse RawEntry (a line failing THIS is still an error);
      // then RunEvent::deserialize(entry.event.clone()) — on Err, substitute
      // RunEvent::Opaque { raw: entry.event }.
  }
  ```
  and add `Opaque { raw: serde_json::Value }` as a plain variant with `#[serde(other)]`? — `serde(other)` only works on unit variants for internally-tagged enums. FINAL contract (the one to implement): `Opaque` variant declared normally with tag `opaque` for Serialize (it will simply never be written by the engine), constructed manually by the reader on decode failure. Serialization round-trip of Opaque is NOT required; a test pins that the reader yields it.
- `scan_interrupted`: an `Opaque` entry counts as non-terminal (only a parsed `RunFinished` is terminal), and a file whose LAST entry is Opaque classifies interrupted=false ONLY if a parsed RunFinished exists anywhere later — keep the existing "last entry is RunFinished" rule but skip trailing Opaque entries when finding the last meaningful entry? NO — simpler per spec: treat Opaque as a non-terminal entry exactly like a step event; the existing last-entry rule then classifies a journal ending in Opaque as interrupted, which is wrong for a new-build-written completed run… The spec's requirement is only that the FILE SURVIVES and the run stays listed. Implement: `scan_interrupted` checks `entries.iter().rev().find(|e| !matches!(e.event, RunEvent::Opaque{..}))` — the last PARSEABLE entry decides terminal state. `list_runs` (monolith, commands.rs ~785-822) needs no change once read_journal stops erroring.

- [ ] **Step 1: failing tests** (in `journal.rs` tests): (a) write a journal file with a valid `run_started` line + a line `{"ts_unix":1,"run_id":"r","seq":1,"event":{"type":"from_the_future","x":1}}` + a valid `run_finished` line; assert `read_journal` returns 3 entries with entry[1] matching `RunEvent::Opaque` and raw preserving `"type":"from_the_future"`. (b) same file through `scan_interrupted`: NOT interrupted (last parseable = RunFinished). (c) a file ending in the unknown line after run_finished: still NOT interrupted. (d) truly corrupted envelope line (not JSON): current behavior (error) preserved.
- [ ] **Step 2: run on R2, verify FAIL** — `ssh r2-poe 'cd ~/build/ranks1-5 && cargo test --manifest-path src-tauri/Cargo.toml --locked -p tuxlink-routines journal'` (rsync first).
- [ ] **Step 3: implement** per the FINAL contract above.
- [ ] **Step 4: run tests green on R2**; also full `-p tuxlink-routines` suite.
- [ ] **Step 5: PARENT commits** `fix(routines): tolerant per-line journal decode — unknown future event types become opaque entries instead of corrupting History`

### Task A2: PR A — open, CI, merge

- [ ] Push branch `bd-tuxlink-iizmk/tolerant-journal-reader` (branch off current arc branch is NOT needed; this task's commit goes on its own branch cut from origin/main so it merges first — cherry-pick A1's commit onto `bd-tuxlink-iizmk/tolerant-journal-reader` cut from origin/main).
- [ ] `gh pr create` (title `[alder-oriole-cedar] fix(routines): tolerant journal reader`), verify CI green BY COMMIT SHA (`gh run list --commit <sha>`; match headSha + conclusion), then merge with the bare command `gh pr merge <n> --merge` (never chained). Then rebase the arc branch onto the new origin/main (non-interactive `git rebase origin/main` — local unpushed commits only) — if the arc branch was already pushed, MERGE origin/main into it instead of rebasing.

---

## PR group B — O3/O4 engine + History

### Task B1: journal variants `call_child`, `end_reached`, `parkKind` on state_changed

**Files:**
- Modify: `src-tauri/tuxlink-routines/src/journal.rs` (RunEvent enum ~31-114)
- Test: same file

**Interfaces (produced, exact):**
```rust
CallChild { step: StepId, child_run_id: String },
EndReached { step: StepId, failed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")] reason: Option<String> },
// StateChanged gains:
#[serde(default, skip_serializing_if = "Option::is_none")] park_kind: Option<ParkKind>,
// with:
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParkKind { Transmit, Write }
```
Wire tags: `call_child`, `end_reached`; field `park_kind` (JSON snake_case; the FRONTEND TS name is `park_kind` too — journal passes through unrecased; the `parkKind` name in the spec applies to the camelCase APP EVENT payload only).

- [ ] **Step 1: failing serde round-trip tests** for both variants + `state_changed` with and without `park_kind` (old journals: absence tolerated; new: value survives).
- [ ] **Step 2: FAIL on R2.** **Step 3: implement.** **Step 4: green on R2.**
- [ ] **Step 5: PARENT commits** `feat(routines): call_child + end_reached journal events, park_kind on state_changed (O3/O4 decree)`

### Task B2: invoker split, cancellation, registry bridge, CallChild emission, orphan-journal fix

**Files:**
- Modify: `src-tauri/tuxlink-routines/src/engine.rs` (RoutineInvoker trait, EngineChildInvoker ~354-431, run_internal ~204-283, DryRunChildInvoker ~379-411, NoInvoker ~310-326), `src-tauri/tuxlink-routines/src/executor.rs` (Call arm ~607-694), `src-tauri/tuxlink-routines/src/fakes.rs` (FakeInvoker ~199)
- Modify: `src-tauri/src/routines/session.rs` (registry insertion ~437-464; new `SessionChildInvoker` owning start)
- Test: engine.rs + executor.rs tests; monolith session tests

**Interfaces (produced, exact):**
```rust
// tuxlink-routines/src/engine.rs
pub struct ChildHandle {
    pub run_id: String,
    pub cancel: CancellationToken,
    outcome: /* oneshot receiver or JoinHandle — awaited via ChildHandle::outcome() */,
}
#[async_trait]
pub trait RoutineInvoker: Send + Sync {
    /// Start a child run: run_id known on return. `parent_cancel` is the
    /// caller's token; impls derive the child token from it
    /// (parent_cancel.child_token()). Registry-registering impls (the
    /// monolith session) register the child BEFORE returning.
    async fn start(&self, routine: &str, args: serde_json::Value,
        provenance: Provenance, parent_cancel: &CancellationToken)
        -> Result<ChildHandle, InvokeError>;
    /// Await terminal outcome (existing invoke() semantics minus startup).
    async fn await_outcome(&self, handle: ChildHandle)
        -> Result<serde_json::Value, InvokeError>;
}
```
Executor Call arm (both sync + F&F): journal `StepIntent` -> `invoker.start(...)` INLINE -> on Ok(handle): journal `CallChild { step, child_run_id: handle.run_id.clone() }`; sync: race `await_outcome` against `ctx.cancel` (on cancel: `handle.cancel.cancel()` then `StepErr(Cancelled)`); F&F: journal `StepOk {"dispatched": true}` and DROP the handle (do NOT await; do NOT link parent token — child token derives from a FRESH root token the invoker creates when the F&F flag is set... NO: per spec F&F children stay detached, so for F&F pass a detached token: the executor passes `&CancellationToken::new()` as parent_cancel for F&F calls). On start Err: `StepErr` (the honest behavior change; the old silent-dispatched lie dies).
Root-digest re-verification hook: `start()` on the monolith impl re-verifies root ack digests (Task C3 wires it; B2 leaves a `// C3:` seam comment).
Orphan-journal fix in `run_internal`: parse the snapshot BEFORE `JournalWriter::create` + `RunStarted` append; a SnapshotShape failure returns Err with NO journal file created.
Monolith `SessionChildInvoker` (in session.rs): implements `start` by loading the def (ONE read), running the start gate on THAT def (Task C3 extends it), registering the run in the same registry map + watcher used by `start_routine_def` (~450), then calling `start_run_ext` with the loaded def and derived token; child runs therefore `cancel_run(child_id)` == true.

- [ ] **Step 1: failing engine tests**: (a) sync call success journals intent -> call_child -> step_ok, and step_ok output still carries `{"completed":true,"run_id":...}`; (b) sync call FAILURE journals call_child BEFORE step_err and the child run id in call_child matches the child journal file; (c) F&F journals call_child + step_ok{dispatched:true} strictly before parent run_finished (assert seq ordering in the parent journal); (d) F&F start failure (unknown routine) journals step_err, NOT dispatched:true; (e) cancelling the parent mid-sync-child cancels the child (child journal terminal = cancelled) and parent step_err = Cancelled; (f) F&F child SURVIVES parent cancel; (g) snapshot-shape failure leaves no child journal file on disk.
- [ ] **Step 2: FAIL on R2.** **Step 3: implement** (engine + executor + fakes + DryRunChildInvoker/NoInvoker get the new trait shape; single-shot `invoke` reimplemented as start+await_outcome).
- [ ] **Step 4: green on R2** (`-p tuxlink-routines` full).
- [ ] **Step 5: failing MONOLITH test** (session.rs tests): start a parent whose child is engine-invoked; read `call_child` from the parent journal; `cancel_run(&child_id)` returns true and the child journal terminates cancelled.
- [ ] **Step 6: implement SessionChildInvoker; green on R2 (workspace test).**
- [ ] **Step 7: PARENT commits** `feat(routines): two-phase invoker — child run ids journaled in all call paths, parent cancellation propagates, children registry-cancellable (O3)`

### Task B3: End threading + EndReached + precedence

**Files:**
- Modify: `src-tauri/tuxlink-routines/src/executor.rs` (End site ~565-570, TrackEnd ~144-153, run_track_shared ~391-424, run_tracks ~733-815, RunOutcome ~155-158), `src-tauri/tuxlink-routines/src/engine.rs` (run_finished emission ~259-283)
- Test: executor.rs tests

**Interfaces (produced, exact):**
```rust
pub enum TrackEnd { Completed, Ended { step: StepId, failed: bool, reason: Option<String> }, /* existing Cancelled if present stays */ }
pub struct RunOutcome { pub state: RunState, pub reason: Option<String>, pub end_step: Option<StepId> } // loses Copy
```
End site journals `EndReached { step: c.id.clone(), failed: *failed, reason: reason.clone() }` BEFORE returning `TrackEnd::Ended` (ordering: end_reached -> skip sweep -> run_finished). `run_tracks` precedence for `run_finished.reason`: propagated StepErr wins over any End reason; failed-End reason wins over success-End reason regardless of arrival order; same-class: first arrival. Engine emits `RunFinished { state, reason: outcome.reason }`.

- [ ] **Step 1: failing tests**: (a) single-track End{failed:true, reason:"why"} -> journal has end_reached with step id + run_finished.reason == "why" (pins the dropped-reason fix); (b) end_reached seq < each step_skipped seq < run_finished seq; (c) parallel tracks success-End("a") + failed-End("b") in both arrival orders -> reason "b"; (d) StepErr + End -> StepErr string wins; (e) callers compile (session.rs ~479-489 touches .state only).
- [ ] **Steps 2-4: FAIL -> implement -> green on R2.**
- [ ] **Step 5: PARENT commits** `feat(routines): end_reached events + End reason threads into run_finished with deterministic precedence (O4)`

### Task B4: History UI — call/end/park rows, navigation, TS types

**Files:**
- Modify: `src/routines/routinesApi.ts` (RunEvent union ~159-170), `src/routines/designer/RunsTab.tsx` (stepListModel ~305-380, kind union ~286, ROW_ICON ~384-393, JSX ~829-895, header ~650-657)
- Test: `src/routines/designer/RunsTab.test.tsx` (existing patterns)

**Interfaces:** TS additions (snake_case, journal passes through unrecased):
```ts
| { type: 'call_child'; step: string; child_run_id: string }
| { type: 'end_reached'; step: string; failed: boolean; reason?: string }
// state_changed gains: park_kind?: 'transmit' | 'write'
```
Rows: `kind:'call'` renders `call:<routine-from-intent>` + short child id, clickable -> `setSelectedRunId(child_run_id)` capturing `{parentRunId, parentShortId}` in a one-deep `navContext` state; `kind:'end'` renders `ended at <step>: complete|failed[, <reason>]`; park rows append `(config write)` when `park_kind==='write'`; finished row suppresses its reason when string-equal to the winning end row's reason; when `selectedRunId` is not in `runsSorted`, render the context strip `Viewing a run of <status.routine> (called by this routine) — back to run <parentShortId>` whose back link restores the parent selection.

- [ ] **Step 1: failing vitest** for each row mapping + click navigation + context strip + reason suppression + park kind text (feed synthetic journals through stepListModel; follow the existing RunsTab test fixtures).
- [ ] **Step 2: FAIL (`pnpm vitest run src/routines/designer/RunsTab.test.tsx`).** **Step 3: implement.** **Step 4: green + `pnpm typecheck` + full `pnpm vitest run src/routines`.**
- [ ] **Step 5: PARENT commits** `feat(routines): History renders call/end/park-kind rows with child-run navigation (O3/O4 UI)`

### Task B5: PR B — open, CI by SHA, merge (bare command). WebKitGTK render check of RunsTab via the harness (`&real=1` fixture still renders; visual-only, no fixture update yet).

---

## PR group C — consent: closure walk, digest, acks, writes_config, park kind

### Task C1: parameterized closure walk + canonical digest (leaf crate)

**Files:**
- Create: `src-tauri/tuxlink-routines/src/consent_closure.rs` (declare in lib.rs)
- Test: same file

**Interfaces (produced, exact):**
```rust
pub struct ClosureStep { pub routine: String, pub step: StepId, pub action: String, pub params_json: String }
pub struct CallEdge { pub routine: String, pub step: StepId, pub callee: String, pub args_json: String }
pub struct ConsentClosure { pub steps: Vec<ClosureStep>, pub call_edges: Vec<CallEdge> }
/// ONE walk, parameterized by a descriptor predicate; cycle-guarded and
/// depth-capped identically to the existing walks it replaces.
pub fn consent_closure(
    root: &RoutineDef,
    lookup: &dyn Fn(&str) -> Option<RoutineDef>,
    is_relevant: &dyn Fn(&str) -> bool,   // action-name -> descriptor flag
) -> ConsentClosure;
/// Canonical digest: recursive JSON key-sort of every params/args value,
/// tuples sorted by (routine, step), sha256 over the canonical byte string,
/// hex-encoded. Call edges included only on paths reaching a relevant step.
pub fn closure_digest(c: &ConsentClosure) -> String;
```
The existing transmit walks (monolith `src/routines/consent.rs` `closure_transmits` and the validator's mirror in `validate/consent.rs`) are REIMPLEMENTED on top of `consent_closure` (non-empty steps == transmits) — delete the duplicated traversals, keep their public signatures.

- [ ] **Step 1: failing tests**: key-order independence (`{"a":1,"b":2}` vs reversed -> same digest); param mutation, Call-args mutation, callee-step mutation each flip the digest; unrelated-step edit does NOT flip it; cycle + depth-cap parity with the old walks (port their existing tests); transmit walks still pass their existing test suites after reimplementation.
- [ ] **Steps 2-4: FAIL -> implement -> green on R2** (add `sha2` to tuxlink-routines Cargo.toml + REGENERATE Cargo.lock — `--locked` masks a stale lock).
- [ ] **Step 5: PARENT commits** `refactor(routines): one parameterized consent-closure walk + canonical closure digest (replaces duplicated transmit walks)`

### Task C2: `writes_config` flag + executor park + parkKind + dry-run mirror

**Files:**
- Modify: `src-tauri/tuxlink-routines/src/action.rs` (ActionDescriptor + all existing descriptor literals gain `writes_config: false`), `executor.rs` (~283-317 park predicate), `dryrun.rs` (flag mirror ~93-97, forced attended false unchanged), `consent.rs` leaf (ConsentPort park signature gains `kind: ParkKind`), `src-tauri/src/routines/consent.rs` (ConsentRegistry emits kind), `src-tauri/src/routines/events.rs` (AwaitingConsent event + serde), journal StateChanged park_kind emission at the park site
- Test: executor tests

Park predicate: `ctx.attended && (d.transmits || d.writes_config)`; kind = Write when `writes_config && !transmits`, else Transmit. Retry-wrapped writes park per attempt (existing structure; pin with a test).

- [ ] **Step 1: failing tests**: attended park fires for a writes_config fake (kind Write in the journaled state_changed), per-attempt retry parks, dry-run never parks, automatic never parks.
- [ ] **Steps 2-4: FAIL -> implement -> green on R2 (workspace: monolith consent registry compiles).**
- [ ] **Step 5: PARENT commits** `feat(routines): writes_config consent class — attended park with park kind end to end`

### Task C3: write_ack + digests on RoutineDef, validator codes, gates, child re-verification

**Files:**
- Modify: `src-tauri/tuxlink-routines/src/types.rs` (RoutineDef: `write_ack: Option<Ack>` serde-default; `Ack`/TransmitAck gains `#[serde(default)] closure_digest: Option<String>`), `validate/consent.rs` (codes), `src-tauri/src/routines/commands.rs` (ack stripping ~382-402 + validate_draft ~442-450 mirror for write_ack; run_routine ~670-681 passes the VALIDATED def through), `src-tauri/src/routines/session.rs` (start gate on the one read; digest recompute; threads root digests into ExecCtx; SessionChildInvoker.start re-verifies), `executor.rs`/`engine.rs` (ExecCtx carries `root_digests: Option<RootDigests{transmit: Option<String>, write: Option<String>}>`)
- Modify: UI ack command site (the existing `acknowledge_automatic` command in commands.rs): records digest at ack time; new sibling `acknowledge_write` (UI-only, NOT on the MCP router); new UI-only `routines_consent_closure(name)` command returning `{transmit_steps, write_steps, call_edges}` from Task C1's walk.
- Test: leaf validator tests + monolith commands/session tests

Validator codes (Finding messages name the offending entity verbatim, matching existing style):
- `AUTO_WRITE_UNACKED` (Error): automatic + non-empty write closure + write_ack missing/empty/digest-mismatched.
- `AUTO_TX_UNACKED` gains the digest-mismatch clause (digest-less legacy ack == stale == fires).
- `MIXED_MODE_STALL_WRITE` (Warning), `ATTENDED_WRITE_UNDER_SCHEDULE` (Warning).
- `WRITE_VALUE_RUNTIME` (Warning): message exactly `step "<id>" write param "<key>" is "<$ref>" - the value is chosen at run time by whoever starts the run`.
Start gate: single-read (validate the def you snapshot), both digest checks; child-start (SessionChildInvoker) recomputes the ROOT digests against the live store and fails the call verbatim `callee changed after acknowledgment` on mismatch; attended root -> no digests threaded -> no-op.

- [ ] **Step 1: failing leaf tests** (validator codes incl. all three UNACKED clauses on both classes; the stall/schedule/runtime warnings).
- [ ] **Step 2: failing monolith tests**: body-supplied write_ack stripped on save + validate_draft; ack command records digest; routine edit, callee edit, and digest-less legacy ack each invalidate (finding fires + start gate refuses); TOCTOU (concurrent save between validate and start cannot swap the def — the started snapshot IS the validated def); child-start mismatch fails the Call step with the verbatim message; `routines_consent_closure` returns the enumeration.
- [ ] **Steps 3-5: FAIL -> implement -> green on R2 (workspace).**
- [ ] **Step 6: PARENT commits** `feat(routines): write_ack + closure-digest binding on both ack classes — save/enable/start/child-start verification, one validator`

### Task C4: PR C — open, CI by SHA, merge (bare). Includes the AGENTS-of-record docs touch: design-spec §4/§14 amendment + §7 divergence note (small `docs:` commit in the same PR), and bd follow-up issue for general callee pinning (`bd create` + note id in PR body).

---

## PR group D — actions

### Task D1: rank 1 status read sources

**Files:**
- Modify: `src-tauri/src/routines/actions/mod.rs` (DataService trait + 3 methods), `src-tauri/src/routines/actions/data.rs` (ReadSource variants `ModemStatus`, `BackendStatus`, `AppStatus`; match arms; MonolithDataService impls calling the same gatherers the MCP ports call: `gather_modem_status`/`derive_modem_status`, `derive_status_dto` + curation map + `redact_freeform` on the error arm, guard `armed_remaining`/`is_tainted`/`taint_reason` + name/version)
- Test: data.rs fakes + monolith curation-equality pins

Seam signatures:
```rust
async fn read_modem_status(&self) -> Result<tuxlink_mcp_core::ports::ModemStatusDto, String>;
async fn read_backend_status(&self) -> Result<tuxlink_mcp_core::ports::BackendStatusDto, String>;
async fn read_app_status(&self) -> Result<serde_json::Value, String>; // ServerInfoDto shape
```
- [ ] **Step 1: failing tests** incl. the PIN: routines `backend_status` output for an Error{reason with ";PQ..."} backend state serializes byte-identical to the MCP `backend_status` tool output for the same state (redaction included).
- [ ] **Steps 2-4 on R2. Step 5: PARENT commits** `feat(routines): data.read sources modem_status/backend_status/app_status (rank 1)`

### Task D2: rank 3 config read sources

**Files:** same seam/action files; ReadSource variants `Config`, `ArdopConfig`, `VaraConfig`, `PacketConfig`, `RigConfig`; impls call the SAME curation the MCP ports use (`config_read`'s 5-field projection + 4-char grid clamp via the same `redact_config_view`; per-modem DTOs verbatim).
- [ ] TDD steps on R2; pins: routines `config` == MCP `config_read` byte-identical for same underlying config (grid clamp pinned with a 6-char grid fixture); each per-modem source == its MCP tool output.
- [ ] **PARENT commits** `feat(routines): data.read config sources with MCP-identical curation (rank 3)`

### Task D3: `data.find_stations`

**Files:**
- Modify: `src-tauri/src/routines/actions/mod.rs` (new seam `StationQueryService`), Create: `src-tauri/src/routines/actions/find_stations.rs` (registered in build_registry), Test: composability proof extension in `src-tauri/tuxlink-routines/src/composability_proof.rs`

Descriptor: `data.find_stations`, "Find gateway stations", needs_internet only. Params struct:
```rust
#[derive(Deserialize)] struct FindStationsParams {
  #[serde(default)] modes: Vec<String>, #[serde(default)] bands: Vec<String>,
  #[serde(default)] history_hours: Option<u32>, #[serde(default)] limit: Option<usize> }
```
`limit == Some(0)` -> invalid params. Impl: same path as the MCP tool (`catalog_fetch_stations` + `curate_gateway` + band filter + distance sort with 4-char own grid); THEN dedup callsigns preserving order; THEN truncate the deduped callsign list to `limit`; `gateways` = rows whose callsign survived. Output: `{"gateways":[...], "fetched_at_ms":u64|null, "operator_grid":str|null, "callsigns":[...]}`.
- [ ] Tests: limit-over-distinct-callsigns (nearest gateway occupying 3 rows + limit 3 -> 3 DISTINCT callsigns), null-grid directory-order truncation, empty result `{"gateways":[],"callsigns":[]}` (not an error), PII omission pin vs MCP output. Composability proof: faked directory -> `$s1.callsigns` -> `radio.connect` `stations` resolves to the sorted callsign array.
- [ ] **PARENT commits** `feat(routines): data.find_stations — distance-sorted gateway query feeding radio.connect (rank 2)`

### Task D4: `data.docs_search`

**Files:** Create `src-tauri/src/routines/actions/docs_search.rs` (+ seam method on a `DocsService` or reuse SearchService lock as the MCP port does). Descriptor: all flags false. Params `{query: non-empty}`; output `{"hits":[{title,slug,snippet}...]}` via the same `search_docs` (raw-then-OR fallback, 30-cap). Zero hits -> `{"hits":[]}`.
- [ ] TDD on R2 + pin vs MCP docs_search output. **PARENT commits** `feat(routines): data.docs_search (rank 4)`

### Task D5: `config.set_ardop` + locked RMW (both front-ends)

**Files:** Create `src-tauri/src/routines/actions/config_write.rs` (namespace `config.`, descriptor `writes_config: true`, others false); Modify `src-tauri/src/config.rs` consumers: implement `set_ardop_drive_level(level: u8) -> Result<(old: Option<u8>, new: u8), String>` INSIDE `config::update_config`'s writer lock; Modify `src-tauri/src/mcp_ports.rs` (~1616-1640): the MCP `set_ardop` closure calls the SAME locked setter (upgrading the racy get-then-set).
Params `{drive_level: u8}`; `>100` -> invalid params BEFORE any read. Output `{"field":"drive_level","old":<u8|null>,"new":<u8>}`.
- [ ] Tests: validation-before-effect, old/new computed under the lock (concurrent-update test via two tasks), MCP path uses the locked setter (its existing tests stay green), output shape pin.
- [ ] **PARENT commits** `feat(routines): config.set_ardop write action + locked drive-level RMW shared with the MCP write path (rank 5)`

### Task D6: dry-run canned shapes + authoring affordances

**Files:** Modify `src-tauri/src/routines/commands.rs` (~169-207 dry-run script assembly: merge shape-true canned outputs keyed by action name before the optimistic default), `src-tauri/tuxlink-routines/src/action.rs` (descriptor gains `example_params: Option<&'static str>`, `allowed_values: Option<(&'static str, &'static [&'static str])>`; all existing literals gain `None`), `validate/contracts.rs` (UNKNOWN_READ_SOURCE Error for literal source outside allowed_values), `commands.rs` ActionInfo DTO + `src/routines/routinesApi.ts` ActionInfo (gains `writes_config`, `example_params`).
Canned outputs (exact): `data.find_stations` -> `{"gateways":[],"callsigns":["DRYRUN-1"],"fetched_at_ms":null,"operator_grid":null,"dry_run":true}`; `data.read` per source -> its DTO shape with fake values (`grid` -> `{"grid":"AA00aa"}`, `modem_status` -> full shape kind "idle", `ardop_config` -> `{"host":"127.0.0.1","port":8515,"drive_level":80,"bandwidth":500}`, etc.); `config.set_ardop` -> `{"field":"drive_level","old":0,"new":0,"dry_run":true}`; `data.docs_search` -> `{"hits":[],"dry_run":true}`.
- [ ] Tests: per-action/source shape pins as split by the spec (find_stations pins `callsigns`; grid pins `grid`; statuses pin `state`; ardop_config pins `drive_level`; set_ardop pins `field/old/new`); marquee dry-run e2e (find_stations -> branch on callsigns -> connect) completes; UNKNOWN_READ_SOURCE fires on `sorce`/`modem-status` literals, not on `$ref` values.
- [ ] **PARENT commits** `feat(routines): shape-true dry-run outputs + example params + read-source vocabulary lint`

### Task D7: PR D — open, CI by SHA, merge (bare).

---

## PR group E — frontend consent + authoring surfaces

### Task E1: ConsentGate parkKind branching

**Files:** `src/routines/routinesEvents.ts` (payload field `parkKind`), `src/routines/ConsentGate.tsx` (ParkedRun carries parkKind; copy branches header/sub-line/body/button: transmit copy unchanged; write -> header "Confirm config write", sub-line drops Part 97 wording for "You are changing station configuration", button "Confirm config write"; `recoverParkedStepId` also reads `park_kind` off the last `state_changed{awaiting_consent}`), backend `events.rs` emits parkKind (done in C2 — consume here).
- [ ] vitest: both copy sets, mixed queue, launch-recovery from journaled park_kind. **PARENT commits** `feat(routines): consent dialog branches on park kind — write parks never render transmit language`

### Task E2: SettingsTab ack validity + closure enumeration + write ack row

**Files:** `src/routines/designer/SettingsTab.tsx` (+ its test), `src/routines/routinesApi.ts` (`consentClosure(name)` wrapper for `routines_consent_closure`, `acknowledgeWrite`)
Visibility: transmit section renders iff closure.transmit_steps non-empty; write ack row iff closure.write_steps non-empty (closure-based, NOT direct-step scan). Ack panels branch on VALIDITY (present AND no AUTO_*_UNACKED finding): valid -> green; absent -> pending; present-but-invalid -> the third state with copy `Acknowledgment no longer valid: the routine, or a routine it calls, changed after <by> acknowledged on <at>. Re-acknowledge to run automatically.` Both panels enumerate covered steps (`<routine> · <step> · <action> · <params>` rows) with WRITE_VALUE_RUNTIME warnings inline; write-only closures relabel the mode toggle "Unattended (automatic)".
- [ ] vitest: call-only-closure-with-valid-ack still shows the row (the R5 pin), all three panel states, enumeration render, both-classes routine shows both rows. **PARENT commits** `feat(routines): ack panels render validity + the enumerated closure being signed`

### Task E3: StepInspector + palette

**Files:** `src/routines/designer/StepInspector.tsx` (description line; WRITES badge in flags row), `src/routines/designer/PaletteRail.tsx` (WRITES badge via flagsFor; insertAction seeds `example_params` when present instead of `{}`), `src/routines/designer/canvasModel.ts` if flagsFor lives there.
- [ ] vitest: seeded params grid on insert, badges, description render. **PARENT commits** `feat(routines): palette + inspector surface writes badge, descriptions, seeded example params`

### Task E4: PR E — open, CI by SHA, merge (bare). WebKitGTK harness renders: designer palette + settings ack rows + ConsentGate write park (visual inspection, PNGs to scratch).

---

## PR group F — docs + acceptance

### Task F1: user-guide routines-actions reference page

**Files:** Create `docs/user-guide/<next-number>-routines-actions.md` following the existing chapter format (check `docs/user-guide/` numbering + front-matter conventions): the action catalog (names, labels, params with examples, all `data.read` sources, consent classes incl. writes/acks in plain words). Verify it lands in the docs FTS index (the search index build path in `search/docs_index.rs`) so `data.docs_search` finds it.
- [ ] Test: docs_search integration test finds "find stations". **PARENT commits** `docs(user-guide): routines actions reference (searchable by data.docs_search)`

### Task F2: acceptance (SESSION-LEVEL, parent runs it, not a subagent)

- [ ] All PR groups merged; converge rebuild on R2 (operator's `pnpm dev:converged` path — coordinate: this restarts the app, so run it when the operator is not mid-session, or note it in the handoff).
- [ ] Fresh wire-walk against the rebuilt app via the MCP shim: author a probe exercising branch + skip + a sync call (child) + End-with-reason + an ATTENDED config.set_ardop park (grant via UI is operator-only — for the capture, use a dry-run for the write step OR leave the park step out and validate the park via a dry... NO: dry-runs never park. Capture two runs: (1) real run with branch/skip/call/end — no write; (2) the write park validated in vitest + harness render only, with the operator's live click left as an explicitly-named operator step in the handoff).
- [ ] Capture journal -> `dev/render-harness/real-run-<date>.json` + `&real=2` overlay in harness.tsx; WebKitGTK render of History incl. call-child navigation + end row; commit fixture.
- [ ] Wire-walk skill (hard gate): trace the operator flows (author each new action in the designer -> dry-run -> run -> History) to file:line.
- [ ] Coverage recount vs compat-tree §3 (expect 24/24) — update the compat-tree spec's §3 table + bd iizmk notes.

---

## Self-review notes (run after drafting, fixed inline)

- Spec §5.2 root-digest threading appears in B2 (seam comment) + C3 (implementation) — intentional split, C3 states it completes B2's seam.
- ParkKind naming: Rust journal field `park_kind`, app-event payload `parkKind` (camelCase Tauri event), TS journal type `park_kind` — three names stated explicitly here so no task "fixes" the mismatch.
- Tolerant reader merges first (A) and is not depended on by B-E except operationally (converge skew); no code dependency.
- Every new finding code has a test in C3; UNKNOWN_READ_SOURCE's test lives in D6 with its carrier.
