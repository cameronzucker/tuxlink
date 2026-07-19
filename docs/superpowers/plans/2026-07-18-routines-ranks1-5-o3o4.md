# Routines Ranks 1-5 + O3/O4 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the compat-tree ranks 1-5 routines actions (status reads, find_stations, config reads, docs_search, first config-write family with the writes_config consent class) plus observability O3/O4 (child run ids, end events), per the adversarially-hardened design spec.

**Spec (read FIRST, canonical for every contract):**
`docs/superpowers/specs/2026-07-18-routines-round2-ranks1-5-o3o4-design.md`

**Architecture:** The engine leaf crate (`src-tauri/tuxlink-routines`) gains journal variants, the invoker split, the shared closure walk + digest, and the `writes_config` flag; the monolith (`src-tauri/src`) gains the action/seam implementations mirroring the MCP DTOs and owns the child-start registry bridge; the frontend gains History rows, consent-surface branching, and ack validity/enumeration UI. Six PR groups, merged serially and in order; the tolerant journal reader merges FIRST.

**Tech Stack:** Rust (tokio, serde, sha2 for the digest), React 18 + TS, vitest.

## Global Constraints

- **Rust compiles/tests run on R2 ONLY** (`ssh r2-poe`; rustup toolchain, NOT distro 1.75; MSRV 1.75 — no post-1.75 APIs; clippy denies `incompatible_msrv`). This Pi never runs cargo. Sync first: `rsync -a --delete --exclude node_modules --exclude target <pi-worktree>/ r2-poe:~/build/ranks1-5/`, then run cargo over ssh in `~/build/ranks1-5`. Leaf tests: `cargo test --manifest-path src-tauri/Cargo.toml --locked -p tuxlink-routines`; workspace tests/clippy where a task says so; workspace clippy `--all-targets --locked -- -D warnings` before every PR.
- **Subagents write code and report; the PARENT commits.** Subagents must NOT run git commit in the worktree.
- **Branch/PR mechanics (ADR 0017 branch-death):** groups merge SERIALLY. The arc branch `bd-tuxlink-iizmk/round2-ranks-o3o4` carries only the spec + this plan; it gets its own docs PR, merged before group A. Each group's branch is cut fresh from origin/main AFTER the previous group's PR merges (never stack on a merged branch — its hooks deny commits; never open a group PR before the predecessor merges — the base-filtered ci.yml fires nothing on retarget and needs an empty commit to arm).
- **Tasks within a group execute STRICTLY SEQUENTIALLY in numbered order.** Do not parallel-dispatch tasks; several share files by design.
- Frontend tests on the Pi: `pnpm vitest run src/routines` + `pnpm typecheck` (CI runs FULL vitest + clippy `--all-targets`; scoped local green is not CI green).
- `schema_version` stays **1**. All new `RoutineDef` fields are `#[serde(default)]` optional. No `deny_unknown_fields` anywhere.
- New journal fields/variants: additive serde, snake_case tags, `#[serde(default, skip_serializing_if = "Option::is_none")]` on optional fields.
- **ParkKind naming, all three surfaces (do not "fix" the difference):** Rust journal field `park_kind` (snake_case, passes to TS unrecased); the Tauri APP EVENT payload field is camelCase `parkKind` (the event union's discriminant is already `kind`); TS journal type uses `park_kind`.
- **No branch-built binary runs against the real journal/config dirs mid-arc.** Only the converge build (origin/main) touches the operator's live dirs; the acceptance task rebuilds converge AFTER all merges.
- Journals are ONE trust domain (spec §6): `call_child` is navigability, not authorization. Do not add per-routine ACLs to `routines_journal_get` as an "improvement."
- Kill test fixtures with SIGKILL, never SIGSEGV. vitest invoke-mocks are called with NO args at teardown; follow existing `src/routines` test patterns.
- **Oversized tasks:** B2 and C3 are each dispatched as TWO sequential subagent chunks along their built-in seam (B2: steps 1-4 leaf, then 5-6 monolith; C3: step 1 leaf, then steps 2+ monolith) so no single context carries the whole task.
- **ORCH-1:** any parallel analysis/review dispatch persists its findings to a file (dev/adversarial/ or the PR body) before returning; findings that exist only in a subagent's final message are lost context.
- Every task: BEFORE starting, read `docs/pitfalls/testing-pitfalls.md` and follow TDD (write failing test -> run it, verify FAIL -> implement -> run green). BEFORE marking complete: re-check against testing-pitfalls, confirm error paths covered, run the task's named test commands and paste output. After every PR group: minimum three review rounds; continue past three while substantive findings remain.
- Conventional commits with scope, `Agent: alder-oriole-cedar` trailer + `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.
- Do NOT touch `.local/converge-build-worktree`. Do NOT run any transmit-capable binary. Do NOT add safeguards beyond spec.

---

## PR group A — tolerant journal reader (merges FIRST, alone)

### Task A1: per-line tolerant `read_journal`

**Files:**
- Modify: `src-tauri/tuxlink-routines/src/journal.rs` (read_journal ~177-193, scan_interrupted ~196-224, RunEvent enum)
- Test: same file `#[cfg(test)]`

**Contract (final, complete):**
1. Add a normal variant to `RunEvent`: `Opaque { raw: serde_json::Value }` with tag `opaque`. The engine NEVER writes it; only the tolerant reader constructs it. Serialize round-trip of Opaque is not required.
2. `read_journal` keeps its current signature (`std::io::Result<Vec<JournalEntry>>` — do NOT introduce a new error type). Per line: first deserialize the envelope
   ```rust
   #[derive(Deserialize)]
   struct RawEntry { ts_unix: i64, run_id: String, seq: u64, event: serde_json::Value }
   ```
   (a line failing THIS — torn tail, non-JSON — errors the whole file, current behavior, deliberately in-spec: §0.4 scopes the fix to unknown event TYPES; do not "fix" torn tails). Then try `RunEvent::deserialize(entry.event.clone())`; on Err substitute `RunEvent::Opaque { raw: entry.event }`.
3. `scan_interrupted`: the last PARSEABLE entry decides terminal state — `entries.iter().rev().find(|e| !matches!(e.event, RunEvent::Opaque{..}))`. A journal containing ONLY Opaque entries classifies interrupted (matches the existing unreadable-file arm).

- [ ] **Step 1: failing tests**: (a) valid `run_started` + line `{"ts_unix":1,"run_id":"r","seq":1,"event":{"type":"from_the_future","x":1}}` + valid `run_finished`: `read_journal` returns 3 entries, entry[1] is Opaque with raw preserving `"type":"from_the_future"`; (b) that file via `scan_interrupted`: NOT interrupted; (c) unknown line AFTER run_finished: still NOT interrupted; (d) non-JSON envelope line: whole-file error (current behavior pinned); (e) only-Opaque journal: interrupted.
- [ ] **Step 2: run on R2, verify FAIL** (rsync first): `-p tuxlink-routines journal`.
- [ ] **Step 3: implement.** **Step 4: green on R2 (full `-p tuxlink-routines`).**
- [ ] **Step 5: PARENT commits** `fix(routines): tolerant per-line journal decode — unknown future event types become opaque entries instead of corrupting History`

### Task A2: PR A — open, CI, merge

- [ ] Cut `bd-tuxlink-iizmk/tolerant-journal-reader` from origin/main; cherry-pick A1's commit onto it; push.
- [ ] `gh pr create` (title `[alder-oriole-cedar] fix(routines): tolerant journal reader`); verify CI green BY COMMIT SHA (`gh run list --commit <sha>`, match headSha + conclusion); merge with the bare command `gh pr merge <n> --merge` (never chained).

---

## PR group B — O3/O4 engine + History (branch cut from origin/main after A merges)

### Task B1: journal variants `call_child`, `end_reached`, `park_kind`

**Files:** Modify `src-tauri/tuxlink-routines/src/journal.rs`; test same file.

**Interfaces (produced, exact):**
```rust
CallChild { step: StepId, child_run_id: String },
EndReached { step: StepId, failed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")] reason: Option<String> },
// StateChanged gains:
#[serde(default, skip_serializing_if = "Option::is_none")] park_kind: Option<ParkKind>,
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParkKind { Transmit, Write }
```
Wire tags `call_child` / `end_reached`; field `park_kind` (see Global Constraints for the three-surface naming rule).

- [ ] **Step 1: failing serde round-trip tests** (both variants; state_changed with/without park_kind).
- [ ] **Steps 2-4: FAIL on R2 -> implement -> green on R2.**
- [ ] **Step 5: PARENT commits** `feat(routines): call_child + end_reached journal events, park_kind on state_changed (O3/O4 decree)`

### Task B2: invoker split, cancellation, registry bridge, CallChild emission, orphan-journal fix

**Files:**
- Modify: `src-tauri/tuxlink-routines/src/compose.rs` (RoutineInvoker trait + Provenance + MAX_CALL_DEPTH + its F&F tests), `src-tauri/tuxlink-routines/src/engine.rs` (impls: NoInvoker ~310, EngineChildInvoker ~333, DryRunChildInvoker; run_internal ~204-283; RunHandle), `src-tauri/tuxlink-routines/src/executor.rs` (Call arm ~607-694), `src-tauri/tuxlink-routines/src/fakes.rs` (FakeInvoker ~199)
- Modify: `src-tauri/src/routines/session.rs` (registry insertion ~437-464; new `SessionChildInvoker`)
- Test: engine.rs + executor.rs tests; monolith session tests

**Interfaces (produced, exact — this is the single canonical shape).** The
trait + `Provenance` + `MAX_CALL_DEPTH` live in **`compose.rs`** (NOT
engine.rs — add `src-tauri/tuxlink-routines/src/compose.rs` to the files;
its tests ~104-119 pin the OLD F&F dispatched-marker behavior and are
updated to the inline-start contract). Errors are the EXISTING `StepError`
(no new error type; current impls already produce
`StepError::Action { action: "call:<routine>", cause }` and the executor
journals it unmapped).
```rust
pub struct ChildHandle { pub run_id: String, pub cancel: CancellationToken,
    /* private: outcome oneshot receiver */ }
impl ChildHandle {
    /// Public constructor + consuming extractor: cross-crate impls (the
    /// monolith SessionChildInvoker) construct via from_parts and extract
    /// the receiver in their await_outcome. CALLERS (the executor) await
    /// only through the trait fn.
    pub fn from_parts(run_id: String, cancel: CancellationToken,
        done: tokio::sync::oneshot::Receiver<RunOutcome>) -> Self;
    pub fn into_parts(self) -> (String, CancellationToken,
        tokio::sync::oneshot::Receiver<RunOutcome>);
}
/// Call-site context the executor already holds (ExecCtx); carried so the
/// impl can gate + register without global state.
pub struct CallCtx { pub provenance: Provenance, pub child_depth: u32,
    pub parent_attended: bool, pub root: Option<RootConsent> }
pub struct RootConsent { pub routine: String,
    pub transmit_digest: Option<String>, pub write_digest: Option<String> }
#[async_trait]
pub trait RoutineInvoker: Send + Sync {
    /// run_id known on return. Impls derive the child token from
    /// `parent_cancel.child_token()`. Registry-registering impls register
    /// the child (id + the CHILD's own token — cancellability must not
    /// depend on the ChildHandle, which F&F drops) BEFORE returning.
    async fn start(&self, routine: &str, args: serde_json::Value,
        call: CallCtx, parent_cancel: &CancellationToken)
        -> Result<ChildHandle, StepError>;
    /// Await terminal outcome; consumes the handle.
    async fn await_outcome(&self, handle: ChildHandle)
        -> Result<serde_json::Value, StepError>;
    // The old single-shot invoke() is DELETED; call sites move to
    // start + await_outcome (grep for invoke( in executor/engine/tests).
}
```
**Engine mounting (the mechanism, resolved here so no subagent re-derives
it):** `EngineConfig` gains nothing; `Engine` gains
`child_invoker: OnceLock<Arc<dyn RoutineInvoker>>`, preferred over the
internally-constructed `EngineChildInvoker` in `run_internal`'s non-dry
arm when set. The monolith installs `SessionChildInvoker` at the END of
`build_routines_state` (session.rs ~820-828) — the engine is built before
`RoutinesState` exists (construction cycle), so post-construction install
via the OnceLock is the resolution; `SessionChildInvoker` holds the store
Arc, runs-map Arc, and event-sink Arc directly (not RoutinesState).
Dry runs keep `DryRunChildInvoker` (never the session invoker).
**Oneshot relay (F&F registry wedge fix):** the engine's `RunHandle.done`
oneshot has ONE consumer — `SessionChildInvoker.start` spawns the SAME
registry watcher body `start_routine_def` uses (~476-500) on it (flips the
registry entry terminal, emits `RoutinesEvent::RunFinished` on the sink —
children ARE sink-visible like session runs, and the relay watcher also
emits `RunStarted`), and the watcher RELAYS the outcome into a fresh
oneshot that becomes the ChildHandle's private receiver. Sync callers
await the relay; F&F drops the relay receiver harmlessly; the registry
entry goes terminal in every case (a wedged `Running` entry would block
`is_routine_running` + the scheduler forever).
Executor Call arm, both modes: `StepIntent` -> `invoker.start(...)` INLINE
-> on Ok(handle): journal `CallChild { step, child_run_id }`. Sync: clone
`handle.cancel` BEFORE the select (the select's other branch owns the
handle); race `await_outcome` against `ctx.cancel`; on the cancel branch
cancel the clone, then `StepErr(Cancelled)`. F&F: journal
`StepOk {"dispatched": true}` and DROP the handle unawaited; pass
`&CancellationToken::new()` as `parent_cancel` (F&F children deliberately
detached from parent cancellation). Start Err in EITHER mode: `StepErr`
verbatim (the silent `dispatched:true` lie dies) and NO `call_child`.
`FakeInvoker` (fakes.rs:199) goes two-phase: `start` mints a deterministic
fake run id; `Hang` behavior moves to `await_outcome`.
**`start_run_ext` final signature, pinned ONCE for both groups** (B2 adds
`cancel`, C3 adds `root` — same options struct, no second churn):
```rust
pub struct StartOpts { pub depth: u32, pub parent_attended: bool,
    pub dry_run: bool, pub cancel: Option<CancellationToken>,
    pub root: Option<RootConsent> }  // B2 passes root: None; C3 fills it
pub fn start_run_ext(&self, def: &RoutineDef, args: Value, opts: StartOpts) -> RunHandle
```
(existing call sites + test constructors at compose.rs ~61-74 and
executor.rs ~846-857 update mechanically).
Orphan-journal fix in `run_internal`: parse the snapshot BEFORE
`JournalWriter::create` + `RunStarted`; SnapshotShape failure -> Err with
no journal file.
`SessionChildInvoker` (monolith): `start` loads the def (ONE read), runs
the start gate on THAT def (C3 extends the gate; leave a `// C3 extends:`
seam comment), registers id + child token, spawns the relay watcher, then
`start_run_ext` with the loaded def + `StartOpts { cancel:
Some(parent_cancel.child_token()), .. }`. `cancel_run(child_id)` == true
for sync AND F&F children.

- [ ] **Step 1: failing engine tests**: (a) sync success: intent -> call_child -> step_ok, output carries `{"completed":true,"run_id":...}`; (b) sync failure: call_child BEFORE step_err, id matches the child journal; (c) F&F: call_child + step_ok{dispatched:true} strictly before parent run_finished (assert seq); (d) F&F start failure: step_err, NOT dispatched:true, AND no call_child entry; (e) parent cancel mid-sync-child: child journal terminal cancelled + parent step_err Cancelled; (f) F&F child survives parent cancel; (g) snapshot-shape failure leaves no child journal file.
- [ ] **Step 2: FAIL on R2.** **Step 3: implement** (single-shot `invoke` reimplemented as start + await_outcome; DryRunChildInvoker/NoInvoker/FakeInvoker updated). **Step 4: green on R2 (`-p tuxlink-routines`).**
- [ ] **Step 5: failing monolith test**: engine-invoked child; read call_child; `cancel_run(&child_id)` true; child journal terminates cancelled. **Step 6: implement SessionChildInvoker; workspace green on R2.**
- [ ] **Step 7: PARENT commits** `feat(routines): two-phase invoker — child run ids journaled in all call paths, parent cancellation propagates, children registry-cancellable (O3)`

### Task B3: End threading + EndReached + precedence

**Files:** Modify `src-tauri/tuxlink-routines/src/executor.rs` (End site ~565-570, TrackEnd ~144-153, run_track_shared ~391-424, run_tracks ~733-815, RunOutcome ~155-158), `src-tauri/tuxlink-routines/src/engine.rs` (~259-283); test executor.rs.

**Interfaces (produced, exact):**
```rust
pub enum TrackEnd { Completed, Ended { step: StepId, failed: bool, reason: Option<String> } }
pub struct RunOutcome { pub state: RunState, pub reason: Option<String>, pub end_step: Option<StepId> } // loses Copy
```
End site journals `EndReached` BEFORE returning (ordering: end_reached -> skip sweep -> run_finished). Precedence for `run_finished.reason`: propagated StepErr > failed-End reason > success-End reason; same-class ties: first arrival. Sibling tracks' unvisited steps keep the EXISTING cancellation skip reason (no rewriting).

- [ ] **Step 1: failing tests**: (a) End{failed:true,"why"} -> end_reached carries step id AND run_finished.reason=="why"; (b) seq ordering end_reached < step_skipped < run_finished; (c) parallel success-End("a")+failed-End("b"), both arrival orders -> "b"; (d) StepErr + End -> error string wins; (e) cross-track: sibling skips retain the existing cancellation reason verbatim; (f) callers (session.rs ~479-489) compile: state-only access.
- [ ] **Steps 2-4: FAIL -> implement -> green on R2.**
- [ ] **Step 5: PARENT commits** `feat(routines): end_reached events + End reason threads into run_finished with deterministic precedence (O4)`

### Task B4: History UI — call/end/park rows, navigation, TS types

**Files:** Modify `src/routines/routinesApi.ts` (~159-170), `src/routines/designer/RunsTab.tsx` (stepListModel ~305-380, kind union ~286, ROW_ICON ~384-393, JSX ~829-895, header ~650-657); test `src/routines/designer/RunsTab.test.tsx`.

TS additions (snake_case; journal is unrecased):
```ts
| { type: 'call_child'; step: string; child_run_id: string }
| { type: 'end_reached'; step: string; failed: boolean; reason?: string }
| { type: 'opaque'; raw: unknown }   // typing hygiene for A1's reader; stepListModel skips unknown types already
// state_changed gains: park_kind?: 'transmit' | 'write'
```
Rows: `'call'` renders `call:<routine-from-intent>` + short child id, click -> `setSelectedRunId(child_run_id)` capturing `{parentRunId, parentShortId}` in one-deep `navContext`; `'end'` renders `ended at <step>: complete|failed[, <reason>]`; park rows append `(config write)` when `park_kind==='write'`; finished row suppresses its reason when string-equal to the winning end row's; foreign `selectedRunId` (not in `runsSorted`) renders the context strip `Viewing a run of <status.routine> (called by this routine) — back to run <parentShortId>` with a working back link.

- [ ] **Step 1: failing vitest** for each mapping + navigation + strip + suppression + park kind.
- [ ] **Step 2: run, verify FAIL.** **Step 3: implement.** **Step 4: green + `pnpm typecheck` + full `pnpm vitest run src/routines`.**
- [ ] **Step 5: PARENT commits** `feat(routines): History renders call/end/park-kind rows with child-run navigation (O3/O4 UI)`

### Task B5: PR B — cut branch per Global Constraints, open PR, CI by SHA, merge (bare). Harness render of RunsTab with the existing `&real=1` fixture (visual check only).

---

## PR group C — consent: closure walk, digest, acks, writes_config (branch cut after B merges)

### Task C1: parameterized closure walk + canonical digest

**Files:**
- Create: `src-tauri/tuxlink-routines/src/consent_closure.rs` (+ lib.rs declaration)
- Modify: `src-tauri/src/routines/consent.rs` (`closure_transmits` ~83-133 REIMPLEMENTED on the new walk) and `src-tauri/tuxlink-routines/src/validate/consent.rs` (its mirror walk REIMPLEMENTED likewise) — public signatures unchanged, duplicated traversals deleted
- Modify: `src-tauri/tuxlink-routines/Cargo.toml` (+`sha2`) AND regenerate `Cargo.lock` (a stale lock under `--locked` masks everything)
- Test: consent_closure.rs + both consumers' existing suites

**Interfaces (produced, exact):**
```rust
pub struct ClosureStep { pub routine: String, pub track: String, pub step: StepId,
    pub action: String, pub params: serde_json::Value }
pub struct CallEdge { pub routine: String, pub step: StepId, pub callee: String,
    pub args: serde_json::Value }
pub struct ConsentClosure { pub steps: Vec<ClosureStep>, pub call_edges: Vec<CallEdge> }
pub fn consent_closure(root: &RoutineDef, lookup: &dyn Fn(&str) -> Option<RoutineDef>,
    is_relevant: &dyn Fn(&str) -> bool) -> ConsentClosure;
pub fn closure_digest(c: &ConsentClosure) -> String;
```
- `track` is carried for validator findings (`.with_track`) and EXCLUDED
  from the hash; the digest hashes exactly the spec'd tuple
  `(routine, step, action, params)` + call edges `(routine, step, callee, args)`.
- `closure_digest` canonicalizes each `params`/`args` `Value` by recursive
  key-sort and canonical re-serialization before hashing (never relies on
  serde_json map ordering); tuples sort by `(routine, step)`; sha256 hex.
- Call edges included only on paths reaching a relevant step.
- **Traversal decree (the two old walks disagree; one shared walk cannot
  match both):** global visited-set (needed for deterministic enumeration)
  + `MAX_CALL_DEPTH` cap. The monolith gate keeps boolean-equivalent
  behavior; the leaf VALIDATOR GAINS a depth cap it lacks today (aligning
  it with the runtime gate — an intended small behavior change, noted in
  the commit body).
- **Scope:** `closure_transmits` (monolith) and `scan_routine_for_transmit`
  (leaf validator) sit on the new walk. `find_attended_transmitting_in_closure`
  (MIXED_MODE_STALL) is mode-aware over ROUTINES and does NOT fit the
  step/edge shape — it stays a separate walk, untouched by this task.

- [ ] **Step 1: failing tests**: key-order independence; param/Call-args/callee mutations each flip the digest; unrelated edit does not; cycle + depth-cap parity; both existing transmit-walk suites green post-reimplementation.
- [ ] **Steps 2-4: FAIL -> implement -> WORKSPACE green on R2** (the monolith consumer changed).
- [ ] **Step 5: PARENT commits** `refactor(routines): one parameterized consent-closure walk + canonical closure digest (replaces duplicated transmit walks)`

### Task C2: `writes_config` flag + executor park + park kinds

**Files:** Modify `src-tauri/tuxlink-routines/src/action.rs` (ActionDescriptor gains `writes_config: bool`), the descriptor LITERAL sites (they are NOT in action.rs): `src-tauri/src/routines/actions/{cat,local,data,radio}.rs` + leaf `dryrun.rs`, `fakes.rs`, `executor.rs` test literals, `validate/fleet.rs` (every literal gains `writes_config: false`), `executor.rs` (~283-317), `dryrun.rs` (~93-97 mirror; forced-attended-false unchanged), leaf `consent.rs` (ConsentPort::park gains `kind: ParkKind`), `src-tauri/tuxlink-routines/src/fakes.rs` (`FakeConsent::park` gains the `kind` param or the leaf won't compile), `src-tauri/src/routines/consent.rs` (ConsentRegistry), `src-tauri/src/routines/events.rs` (AwaitingConsent event — **the app-event payload field is camelCase `parkKind`**; the JOURNALED state_changed field is snake_case `park_kind`; see Global Constraints), the park-site journal emission; test executor.rs.

Park predicate `ctx.attended && (d.transmits || d.writes_config)`; kind = Write iff `writes_config && !transmits`, else Transmit. Retry-wrapped writes park per attempt.

- [ ] **Step 1: failing tests**: attended park for a writes_config fake with journaled `park_kind:"write"`; per-attempt retry parks; dry-run never parks; automatic never parks; app event carries `parkKind`.
- [ ] **Steps 2-4: FAIL -> implement -> workspace green on R2.**
- [ ] **Step 5: PARENT commits** `feat(routines): writes_config consent class — attended park with park kind end to end`

### Task C3: write_ack + digests, validator codes, gates, child re-verification

**Files:** Modify `src-tauri/tuxlink-routines/src/types.rs`, `validate/consent.rs`, `src-tauri/tuxlink-mcp-core/src/router.rs` (the CLOSED routines tool-list pin ~2482: clarify its coverage comment; the sorted-equality pin auto-asserts the new commands' absence when the expected list is NOT extended, plus add the explicit acknowledge_write absence assertion), `src-tauri/src/routines/commands.rs` (~382-402 strip, ~442-450 validate_draft, ~670-681 run path, ack commands), `src-tauri/src/routines/session.rs` (start gate + ExecCtx threading + SessionChildInvoker verification), `executor.rs`/`engine.rs` (ExecCtx `root: Option<RootConsent>`); tests in each.

**Ack type (exact, load-bearing for E2 and the §14 amendment):** REUSE the existing `TransmitAck` struct for both acks — it gains `#[serde(default)] pub closure_digest: Option<String>`; `RoutineDef` gains `#[serde(default)] pub write_ack: Option<TransmitAck>`. Serialized fields: `by`, `at`, `closure_digest`.
Validator codes: `AUTO_WRITE_UNACKED` (Error; missing/empty/digest-mismatched), `AUTO_TX_UNACKED` gains the digest-mismatch clause (digest-less legacy == stale == fires), `MIXED_MODE_STALL_WRITE` (Warning), `ATTENDED_WRITE_UNDER_SCHEDULE` (Warning), `WRITE_VALUE_RUNTIME` (Warning; message exactly `step "<id>" write param "<key>" is "<$ref>" - the value is chosen at run time by whoever starts the run`).
Behavior sentences a faithful implementation must include:
- **Leaving automatic mode revokes `write_ack`** exactly as it revokes transmit_ack (the `_ => None` arm of the strip match, commands.rs ~394-401); test mirrors the existing flip test at ~1451.
- Start gate: single read — the validated def IS the snapshot started.
  Named plumbing (round-2 verified): expose
  `pub async fn start_routine_with_def(&self, def: &RoutineDef, args) -> ...`
  on `RoutinesState` (the current private `start_routine_def` body);
  `run_routine` (commands.rs ~670-681) passes its validated def instead of
  calling `start_routine` (which reloads by name). Scheduler / MCP / recovery
  paths keep flowing through the gate inside the same body. Both digest
  checks run there.
- ExecCtx threads `root: Option<RootConsent>` (the B2-pinned struct — it
  CARRIES the root routine's name, which the child-start recompute needs to
  look the root up) via `StartOpts.root`; `SessionChildInvoker.start`
  recomputes the ROOT's digests against the live store, failing the Call
  verbatim `callee changed after acknowledgment` on mismatch; attended
  root -> None -> no-op.
- New UI-ONLY commands: `acknowledge_write` (sibling of `acknowledge_automatic`; BOTH record the digest at ack time) and `routines_consent_closure(name)` returning `{transmit_steps, write_steps, call_edges}`. **Neither appears on the MCP router** — add a router-surface test asserting their absence (the routines tool list is pinned CLOSED in router.rs ~2482; extend that pin).

- [ ] **Step 1: failing leaf tests** (all codes; all three UNACKED clauses on both classes).
- [ ] **Step 2: failing monolith tests**: write_ack stripped on save + validate_draft; mode-flip revokes write_ack; BOTH ack commands record digests; routine edit / callee edit / digest-less legacy each invalidate (finding + gate refusal); TOCTOU (started snapshot == validated def under concurrent save); child-start mismatch verbatim failure; consent-closure command output; MCP router excludes both new commands.
- [ ] **Steps 3-5: FAIL -> implement -> workspace green on R2.**
- [ ] **Step 6: PARENT commits** `feat(routines): write_ack + closure-digest binding on both ack classes — save/enable/start/child-start verification, one validator`

### Task C4: PR C — docs amendments + follow-up filing + merge

- [ ] Same-PR `docs:` commit: 2026-07-13 routines design spec §4 (write-consent class + digest binding on both acks), §14 (definition format: `write_ack`, `closure_digest`), §7 divergence note (Call targets resolve live; bd follow-up). ALSO record the acceptance divergence: the attended-write park is validated by vitest + harness render, not a live-capture click (the click is an operator act; named in the handoff).
- [ ] `bd create` the callee-pinning follow-up; note its id in the PR body.
- [ ] Cut branch per Global Constraints; PR; CI by SHA; merge (bare).

---

## PR group D — actions (branch cut after C merges)

### Task D1: rank 1 status read sources

**Files:** Modify `src-tauri/src/routines/actions/mod.rs` (DataService + 3 methods), `src-tauri/src/routines/actions/data.rs` (ReadSource variants `ModemStatus`, `BackendStatus`, `AppStatus` + match arms + MonolithDataService impls calling the same gatherers the MCP ports call); tests in data.rs + curation pins.

Seam signatures:
```rust
async fn read_modem_status(&self) -> Result<tuxlink_mcp_core::ports::ModemStatusDto, String>;
async fn read_backend_status(&self) -> Result<tuxlink_mcp_core::ports::BackendStatusDto, String>;
async fn read_app_status(&self) -> Result<serde_json::Value, String>; // ServerInfoDto shape; use the typed mcp-core ServerInfoDto if importable from lib.rs, else Value pinned by test
```
- [ ] **Step 1: failing tests** incl. curation-equality pins for ALL THREE: `modem_status` == `modem_get_status` tool output; `app_status` == `server_info` output; `backend_status` == tool output for an Error state containing `;PQ...` (redaction pinned).
- [ ] **Steps 2-4: FAIL -> implement -> workspace green on R2.**
- [ ] **Step 5: PARENT commits** `feat(routines): data.read sources modem_status/backend_status/app_status (rank 1)`

### Task D2: rank 3 config read sources

**Files:** same seam/action files; ReadSource `Config`, `ArdopConfig`, `VaraConfig`, `PacketConfig`, `RigConfig`; impls reuse the MCP curation (5-field projection + 4-char clamp via `redact_config_view`; per-modem DTOs verbatim).
- [ ] **Step 1: failing tests**: per-source pins (each == its MCP tool output; `config` pinned with a 6-char-grid fixture proving the clamp). **Steps 2-4: FAIL -> implement -> workspace green on R2.**
- [ ] **Step 5: PARENT commits** `feat(routines): data.read config sources with MCP-identical curation (rank 3)`

### Task D3: `data.find_stations`

**Files:** Modify `src-tauri/src/routines/actions/mod.rs` (new seam), Create `src-tauri/src/routines/actions/find_stations.rs` (+ build_registry registration), Modify `src-tauri/tuxlink-routines/src/composability_proof.rs`; tests in both.

Descriptor: `data.find_stations`, "Find gateway stations", `needs_internet:true` only. Params:
```rust
#[derive(Deserialize)] struct FindStationsParams {
  #[serde(default)] modes: Vec<crate::catalog::stations::ListingMode>, // kebab-case enum, same as data.stationlist_update (data.rs ~418) — Vec<String> would accept garbage
  #[serde(default)] bands: Vec<String>,
  #[serde(default)] history_hours: Option<u32>, // validate <= 720 via the same helper the MCP port uses (validate_history_hours, importable) — reject verbatim
  #[serde(default)] limit: Option<usize> }     // Some(0) -> invalid params
```
Impl: same path as the MCP tool (`catalog_fetch_stations` + `curate_gateway` + band filter + distance sort with 4-char own grid); dedup callsigns preserving order; truncate the DEDUPED callsign list to `limit`; `gateways` = rows whose callsign survived. Output `{"gateways":[...],"fetched_at_ms":u64|null,"operator_grid":str|null,"callsigns":[...]}`.
- [ ] **Step 1: failing tests**: limit-over-distinct-callsigns (nearest station occupying 3 rows + limit 3 -> 3 distinct); null-grid directory-order truncation; empty result not-an-error; history_hours 721 rejected verbatim; PII omission pin vs MCP output; **`callsigns` derived only from post-curation gateways** (a gateway dropped by curation contributes no callsign). Composability proof: faked directory -> `$s1.callsigns` -> `radio.connect.stations` resolves to the sorted array.
- [ ] **Steps 2-4: FAIL -> implement -> workspace green on R2.**
- [ ] **Step 5: PARENT commits** `feat(routines): data.find_stations — distance-sorted gateway query feeding radio.connect (rank 2)`

### Task D4: `data.docs_search`

**Files:** Create `src-tauri/src/routines/actions/docs_search.rs` (+ seam + registration). Descriptor all-flags-false; params `{query: non-empty}`; same `search_docs` (raw-then-OR, 30-cap); output `{"hits":[{title,slug,snippet}...]}`; zero hits -> `{"hits":[]}`.
- [ ] **Step 1: failing tests** incl. pin vs MCP docs_search output + empty-query invalid params. **Steps 2-4: FAIL -> implement -> workspace green on R2.**
- [ ] **Step 5: PARENT commits** `feat(routines): data.docs_search (rank 4)`

### Task D5: `config.set_ardop` + locked RMW (both front-ends)

**Files:** Create `src-tauri/src/routines/actions/config_write.rs` (descriptor `writes_config:true`, others false); Modify `src-tauri/src/modem_commands.rs` or `config.rs` consumers: new `set_ardop_drive_level(level: u8) -> Result<(Option<u8>, u8), String>` computing (old, new) INSIDE `config::update_config`'s writer lock; Modify `src-tauri/src/mcp_ports.rs` (~1616-1640): MCP `set_ardop` calls the SAME locked setter.
Params `{drive_level: u8}`; `>100` invalid params BEFORE any read. Output `{"field":"drive_level","old":<u8|null>,"new":<u8>}`.
- [ ] **Step 1: failing tests**: validation-before-effect; old/new under the lock (two writers synchronized with a barrier — not bare join!); **absent-field-erases guard**: seed non-default ardop host/port/bandwidth, write drive_level only, assert the others survive on disk (testing-pitfalls §7 class); MCP path uses the locked setter with its existing tests green; output shape pin.
- [ ] **Steps 2-4: FAIL -> implement -> workspace green on R2.**
- [ ] **Step 5: PARENT commits** `feat(routines): config.set_ardop write action + locked drive-level RMW shared with the MCP write path (rank 5)`

### Task D6: dry-run canned shapes + authoring affordances

**Files:** Modify `src-tauri/tuxlink-routines/src/action.rs` (descriptor gains `example_params: Option<&'static str>`, `allowed_values: Option<(&'static str, &'static [&'static str])>`, AND `dry_run_shape: Option<fn(&serde_json::Value) -> serde_json::Value>` — fn pointer keeps the descriptor `Copy` + `'static`, MSRV-safe; ALL descriptor literals gain the fields — literal sites: `src-tauri/src/routines/actions/{cat,local,data,radio,find_stations,docs_search,config_write}.rs` + leaf `dryrun.rs`/`fakes.rs`/`executor.rs`-tests/`validate/fleet.rs`; this task runs LAST in group D for that reason), `src-tauri/tuxlink-routines/src/dryrun.rs` + `fakes.rs` (**the dry-run mechanism lives HERE, not in a monolith merge** — round-2 P1-5: `DryRunScript.outcomes` is a per-action-NAME queue replayed order-blind by a params-blind fake, so 13 `data.read` sources cannot ride it; instead `build_dryrun_registry`/`apply_default` consult the descriptor's `dry_run_shape` with the RESOLVED params when nothing was scripted for that action — `data.read` matches on `source`, unknown/`$ref` source falls through to the optimistic default; FakeAction gains the params-aware outcome mode), `validate/contracts.rs` (`UNKNOWN_READ_SOURCE` Error: literal non-`$` `source` outside allowed_values; **`contracts::check` gains a `ctx: &dyn ValidationContext` param** — it is deliberately pure today, so the `validate/mod.rs` call site and the module-doc "pure over def" claim both update; `ValidationContext::action_descriptor` already returns the full descriptor), ActionInfo DTO (commands.rs) + `src/routines/routinesApi.ts` ActionInfo (gain `writes_config`, `example_params`). Scripted outcomes (explicit `DryRunScriptDto`) still take precedence over `dry_run_shape`.

Canned outputs (exact, all of them): `data.find_stations` -> `{"gateways":[],"callsigns":["DRYRUN-1"],"fetched_at_ms":null,"operator_grid":null,"dry_run":true}`; `data.read` per source: `grid` -> `{"grid":"AA00aa"}`, `modem_status` -> `{"kind":"idle","connected":false,"state":"idle","running":[],"selected":null,"conflict":false}`, `backend_status` -> `{"connected":false,"transport":"","state":"not_configured"}`, `app_status` -> `{"name":"tuxlink","version":"0.0.0-dryrun","armed":false,"armed_remaining_secs":0,"tainted":false,"taint_reason":null}`, `config` -> `{"connect_to_cms":false,"transport":"CmsSsl","host":"","callsign":"N0CALL","grid":"AA00"}`, `ardop_config` -> `{"host":"127.0.0.1","port":8515,"drive_level":80,"bandwidth":500}`, `vara_config` -> `{"host":"127.0.0.1","port":8300,"bandwidth":2300,"drive_level":0}`, `packet_config` -> `{"kiss_host":"127.0.0.1","kiss_port":8001,"baud":9600,"tx_delay":300}`, `rig_config` -> `{"rig_hamlib_model":null,"rigctld_host":"127.0.0.1","rigctld_port":4532,"rigctld_binary":"rigctld","close_serial_sequencing":false,"live_vfo_poll":false,"qsy_on_fail":false,"cat_serial_path":null,"cat_baud":19200}`, existing sources (`inbox_summary` -> `{"total":0,"unread":0}`, `space_weather` -> `null`, `last_connected_gateway` -> honest-gap error unchanged, `heard_stations` unchanged); `config.set_ardop` -> `{"field":"drive_level","old":0,"new":0,"dry_run":true}`; `data.docs_search` -> `{"hits":[],"dry_run":true}`. All carry `"dry_run":true` where the object shape permits an extra field (objects yes; bare `null` for space_weather stays bare).
`example_params`: `data.read` -> `{"source":"modem_status"}`, `data.find_stations` -> `{"modes":["vara-hf"],"limit":3}`, `data.docs_search` -> `{"query":"find stations"}`, `config.set_ardop` -> `{"drive_level":80}`; existing actions -> None.
- [ ] **Step 1: failing tests**: per-action/source shape pins as the spec splits them (find_stations pins `callsigns`; `grid` pins `grid`; statuses pin `state`; `ardop_config` pins `drive_level`; `set_ardop` pins `field/old/new`); marquee dry-run e2e (find_stations -> branch on `$s1.callsigns` -> connect) completes; `UNKNOWN_READ_SOURCE` fires on `"sorce"`/`"modem-status"` literals, silent on `$ref` values and on known sources.
- [ ] **Steps 2-4: FAIL -> implement -> workspace green on R2 + `pnpm typecheck` (ActionInfo TS).**
- [ ] **Step 5: PARENT commits** `feat(routines): shape-true dry-run outputs + example params + read-source vocabulary lint`

### Task D7: PR D — cut branch, PR, CI by SHA, merge (bare).

---

## PR group E — frontend consent + authoring surfaces (branch cut after D merges)

### Task E1: ConsentGate parkKind branching

**Files:** Modify `src/routines/routinesEvents.ts` (payload field `parkKind` — the union discriminant `kind` is taken), `src/routines/ConsentGate.tsx` (ParkedRun carries parkKind; copy branches header, Part 97 sub-line, body, AND button: transmit copy unchanged; write -> header "Confirm config write", sub-line "You are changing station configuration", button "Confirm config write"; `recoverParkedStepId` additionally reads `park_kind` from the last `state_changed{awaiting_consent}` journal entry); tests in ConsentGate.test.
- [ ] **Step 1: failing vitest** (both copy sets incl. button + sub-line; mixed queue; launch recovery from journaled park_kind). **Steps 2-4: FAIL -> implement -> green + typecheck.**
- [ ] **Step 5: PARENT commits** `feat(routines): consent dialog branches on park kind — write parks never render transmit language`

### Task E2: SettingsTab ack validity + closure enumeration + write ack row

**Files:** Modify `src/routines/designer/SettingsTab.tsx` + test, `src/routines/routinesApi.ts` (`consentClosure(name)` -> cmd `routines_consent_closure`; `acknowledgeWrite`).
Visibility (closure-based, NOT direct-step scan): transmit section iff `closure.transmit_steps` non-empty; write ack row iff `closure.write_steps` non-empty **AND mode is automatic** (mirror the transmit section's mode gating). Ack panels branch on VALIDITY (present AND no AUTO_*_UNACKED finding): valid green / absent pending / present-but-invalid third state with copy `Acknowledgment no longer valid: the routine, or a routine it calls, changed after <by> acknowledged on <at>. Re-acknowledge to run automatically.` Both panels enumerate covered steps (`<routine> · <step> · <action> · <params>`) with WRITE_VALUE_RUNTIME warnings inline; write-only closures relabel the mode toggle "Unattended (automatic)".
- [ ] **Step 1: failing vitest**: call-only-closure-with-valid-ack still shows the row (R5 pin); all three panel states; enumeration; both-classes routine renders both rows; mode gating. **Steps 2-4: FAIL -> implement -> green + typecheck.**
- [ ] **Step 5: PARENT commits** `feat(routines): ack panels render validity + the enumerated closure being signed`

### Task E3: StepInspector + palette

**Files:** Modify `src/routines/designer/StepInspector.tsx` (description line; WRITES badge in its hardcoded flags row), `src/routines/designer/PaletteRail.tsx` (WRITES badge via flagsFor; `insertAction` seeds parsed `example_params` when present, else `{}`), `src/routines/designer/canvasModel.ts` if flagsFor/category lives there.
- [ ] **Step 1: failing vitest**: seeded grid on insert; WRITES badge in BOTH palette and inspector; description renders; `config.*` stays in the LOCAL palette group. **Steps 2-4: FAIL -> implement -> green + typecheck.**
- [ ] **Step 5: PARENT commits** `feat(routines): palette + inspector surface writes badge, descriptions, seeded example params`

### Task E4: PR E — cut branch, PR, CI by SHA, merge (bare). Harness renders: palette, settings ack rows (all three states), ConsentGate write park (PNGs to scratch, visual inspection).

---

## PR group F — docs + acceptance (branch cut after E merges)

### Task F1: user-guide routines-actions reference page

**Files:** Create `docs/user-guide/<next-number>-routines-actions.md` (follow existing chapter numbering + front-matter): action catalog (names, labels, params with examples, all 13 `data.read` sources, consent classes incl. writes/acks in plain words). ALSO Modify `src-tauri/src/search/docs_bundle.rs`: `include_str!` the new chapter + extend `BUNDLED_TOPICS` (docs_registry_test FAILS the workspace if a .md exists unregistered — this makes PR F a Rust-compiling PR, not docs-only). The FTS index re-derives via content fingerprint; no SCHEMA_VERSION action.
- [ ] **Step 1: failing integration test**: docs_search("find stations") returns the new page. **Steps 2-4: FAIL -> implement -> green on R2.** **Step 5: PARENT commits** `docs(user-guide): routines actions reference (searchable by data.docs_search)`. PR; CI by SHA; merge (bare).

### Task F2: acceptance (SESSION-LEVEL — the parent runs it, never a subagent)

- [ ] All groups merged. Converge rebuild on R2 (restarts the operator's app — run when the operator is not mid-session, or name it in the handoff).
- [ ] Fresh wire-walk via the MCP shim against the rebuilt app: one real run exercising branch + skip + a sync call (child) + End-with-reason (NO write step — the attended write park needs the operator's click, which is an operator act; the park is validated by vitest + the E4 harness render, and the divergence is recorded in C4's docs commit). Capture journal -> `dev/render-harness/real-run-<date>.json` + `&real=2` additive overlay in harness.tsx; WebKitGTK render of History incl. child navigation + end row; commit fixture.
- [ ] **Wire-walk skill (hard gate):** trace the operator flows (author each new action in the designer -> dry-run -> run -> History; ack a write routine; consent a write park) to file:line.
- [ ] Coverage recount vs compat-tree §3 (expect 24/24): update the compat-tree spec §3 note + bd iizmk.

---

## Self-review notes (fixed inline, round-1 findings folded)

- A1 states only the final contract; error type pinned to the existing `std::io::Result`.
- Branch mechanics + serial merges + stacked-PR CI gotcha live in Global Constraints; every PR task says "cut branch per Global Constraints."
- ParkKind tri-surface naming is a Global Constraint (C2/E1 both restate their side).
- write_ack revocation-on-mode-flip named + tested in C3; router-exclusion pinned via the CLOSED tool-list test.
- C1's Files list names both reimplemented walk sites; workspace tests.
- D6 runs last in group D (new descriptors from D3/D4/D5 need the new fields); groups are serial; tasks are serial.
- F2's write-park acceptance divergence is recorded in C4's docs commit, not silent.
