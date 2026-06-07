# Alpha Logging Session Transcript Boundary Fix

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore the alpha logging boundary after the 2026-06-07 smoke finding: diagnostic tracing must not pollute the operator-visible Session log, and GitHub issue/export archives must include the retained operator Session log transcript alongside diagnostic events.

**Architecture:** Keep diagnostic logging and operator Session log as separate surfaces. Diagnostic `tracing` events go to `events.jsonl.zst`; explicit Session log APIs append to `SessionLogState` and live-emit `session_log:line`; export adds a new plain JSONL member, `operator_session_log.jsonl`, generated from the retained `SessionLogState` snapshot. Diagnostic fanout no longer mutates `SessionLogState`.

**Tech Stack:** Rust 2021 (Tauri 2), existing `tracing` / `tracing-subscriber` / `tracing-appender` logging stack, existing zstd/tar archive builder, existing `winlink::redaction` credential scrubber, existing Rust integration tests.

**bd issue:** `tuxlink-1ky8`

Historical note: implementation started on `tuxlink-h1gh`, but that branch was
merged-dead when commit was attempted. ADR 0017 correctly blocked the commit.
The work moved to fresh bd issue/worktree `tuxlink-1ky8` off current `main`.

## Living Document Contract

This plan is a living document. Every executing agent MUST update it as
execution progresses, not only at completion.

- **On phase claim:** the executor MUST flip the banner to 🚧 IN PROGRESS
  with a claim timestamp (ISO 8601 UTC) and the active branch name. The
  banner MUST NOT include an expected-completion estimate — agents cannot
  reliably estimate their own wall-clock, and a fabricated duration
  becomes a stale anchor that misleads future readers. Followers
  encountering a 🚧 banner determine liveness by observable signals (PR
  existence, recent branch commits), not by arithmetic on expected times.
  See Step 5's stale-claim reclaim protocol.
- **On phase ship:** the executor MUST update that phase's **Execution
  Status** banner with the shipped commit SHA(s) and date. If a PR is
  open, the PR number and URL MUST appear in the top-of-plan Execution
  Status table.
- **On phase defer:** the executor MUST update the banner with ⏸ status
  AND a prose description of the unblock condition + a link to the
  likely-unblocker artifact (plan page, task, or PR whose own Execution
  Status banner will signal completion). Prose + link is durable across
  paraphrases and scope edits; exact-string coordination between agents
  is not.
- **On PR merge:** the executor MUST record the merge SHA in the banner
  + the top-of-plan Execution Status table.
- **On deviation from the written plan** (scope edits, structural
  refactors, dropped tasks, reordered phases): the executor MUST
  inline-document the deviation in the affected task AND summarize it
  in the top-of-plan Execution Status as a "Deviations" subsection.
  Deviation state MUST NOT live only in PR notes or status reports.
- **On discovery** (pre-existing drift surfaced during execution, new
  bugs found, architectural issues noted): the executor MUST add a
  "Discoveries" subsection at the top of the plan with pointers to the
  files/lines affected. Follow-up dispatches read this subsection to
  avoid duplicate discovery work.

The plan SHOULD reflect reality at the end of every session that touches
it. Anything worth putting in a status report to the user is worth
putting in the plan.

Rationale: `/writing-plans-enhanced` Step 5. Writing at ship time is
cheap; reconstruction by downstream readers is expensive, compounds
across dispatches, and fails silently when state is split across PR
notes and commit messages.

## Execution Status

**Overall:** 6/6 phases implemented; quality gates green; final commit/push pending.

| Phase | Status | Ship SHA(s) | Notes |
|---|---|---|---|
| 1 - Tests and bridge removal | ✅ Implemented | pending final commit | `ui_consumer` removed; boundary tests green |
| 2 - Diagnostic seq split | ✅ Implemented | pending final commit | Fanout owns diagnostic `AtomicU64`; operator cursor isolated |
| 3 - Session-log helper and source correction | ✅ Implemented | pending final commit | Shared redacting helper; P2P wire source guard green |
| 4 - Export retained operator transcript | ✅ Implemented | pending final commit | `operator_session_log.jsonl` added; export tests green |
| 5 - Narrow remote-error redaction | ✅ Implemented | pending final commit | Red/green test confirmed CRED-1 leak fixed |
| 6 - Spec/docs and gates | ✅ Implemented | pending final commit | Specs/plans updated; gates green |

### Deviations

- Phase 3 minimized `ui_commands.rs` churn: instead of adding `LogSource::Transport`
  to every existing progress callsite, the private `emit_session_line(...)`
  wrapper remains the normal Transport path and delegates to a new
  source-parameterized helper. The P2P raw wire callback uses the
  source-parameterized helper with `LogSource::Wire`. This preserves the
  intended behavior and keeps the exceptional source selection explicit.

### Discoveries

- Diagnostic-to-UI bridge exists today through `logging::ui_consumer`, spawned in `src-tauri/src/logging/mod.rs`, and current tests bless the `session_log=true` bridge.
- `FanoutLayer` allocates diagnostic event sequence numbers by mutating `SessionLogState`; this violates the desired boundary even when it no longer appends visible lines.
- Export archives currently contain only `summary.txt`, `events.jsonl.zst`, optional `dict.zdict`, and `manifest.json`; they do not include the operator Session log.
- P2P wire callback currently uses the generic session helper that hardcodes `LogSource::Transport`, so raw protocol lines can look human-visible to the frontend projection.
- `winlink::session::remote_error()` returns later `***` payload text without `redact_freeform`; first post-handshake `***` errors are already scrubbed.

## Evidence Checked

Code paths inspected on current `origin/main` before this plan:

- `src-tauri/src/session_log.rs`: `SessionLogState::append`, `snapshot`, `allocate_seq`, `append_with_seq`.
- `src-tauri/src/logging/fanout.rs`: `FanoutLayer` creates `LoggedEvent` and currently calls `SessionLogState::allocate_seq`.
- `src-tauri/src/logging/mod.rs`: `init()` spawns `disk_consumer` and `ui_consumer`.
- `src-tauri/src/logging/ui_consumer.rs`: `session_log=true` diagnostic events append into `SessionLogState`.
- `src-tauri/src/logging/export.rs`: `build_archive()` reads only diagnostic JSONL files and writes the current archive members.
- `src-tauri/src/logging/commands.rs`: `logging_export()` and `report_issue_flow()` both call `build_archive()`.
- `src-tauri/src/ui_commands.rs`: `emit_session_line()` appends to `SessionLogState` and emits `session_log:line`; P2P progress/wire closures call it.
- `src-tauri/src/bootstrap.rs`: native backend progress/wire sinks append to `SessionLogState` and emit `session_log:line`.
- `src-tauri/src/winlink/modem/vara/commands.rs`: `emit_vara_log()` duplicates append-and-emit behavior.
- `src/session/logProjection.ts` and `src/radio/sections/useSessionLog.ts`: frontend Human view hides `source === "wire"`; Raw view passes all retained lines.
- `docs/pitfalls/implementation-pitfalls.md`: CRED-1 requires every B2F wire sink to redact `;PQ` and `;PR`.

## Build-Robust-Features Review Record

**Pipeline status:** BRF degraded mode. Claude/cross-provider review was unavailable during this session, so five same-provider Codex review rounds were used at highest available effort. This is explicitly weaker than the normal Claude/Codex cross-provider requirement.

Round summaries:

- Round 1, reliability/load/ordering: found that a tracing-only mirror is not a reliable transcript guarantee because broadcast lag can drop events; also found diagnostic fanout currently mutates `SessionLogState` by allocating seq.
- Round 2, redaction/security/privacy: found the later `remote_error()` redaction gap and P2P wire-source misclassification.
- Round 3, minimality/API/refactor risk: recommended the minimal shape in this plan; warned not to grow a new logging API or import UI DTOs into export.
- Round 4, export/archive contract: recommended always writing plain `operator_session_log.jsonl`, naming it as retained/raw transcript, and extending manifest/summary counts.
- Round 5, test/spec/UX drift: confirmed old tests still bless the bridge, confirmed sequence isolation needs explicit regression coverage, confirmed backend-only P2P source classification is enough, and found that the old alpha implementation plan must be patched or future agents may resurrect `ui_consumer.rs`.

## Plan Review Record

- Round 1: found 7 substantive issues. Fixed phase-ordering conflict for `append_with_seq` source scans, chose exact `append_redacted` helper shape, chose exact `AtomicU64` diagnostic seq allocator, fixed exact export projection/count names, removed Phase 5 deferral, and made comment-update files explicit.
- Round 2: found 4 substantive issues. Fixed source-scan roots, listed every current `FanoutLayer::create(...)` callsite, placed helper/P2P tests in exact files, and made the `ui_commands::emit_session_line` signature change explicit.
- Round 3: found 2 substantive issues. Fixed a vague helper-test file-map line and added this review record. Because round 3 was not clean, a round 4 review is required before implementation.
- Round 4: found 0 substantive issues. Implementation may proceed.

## Execution Strategy

Recommended execution: focused sequential implementation in this session, not parallel workers. The phases are tightly coupled around shared Rust logging/session files and tests; parallel edits would create avoidable conflicts. The only safe parallelism was the read-only adversarial design review already completed.

## Global Task Discipline

Every code-bearing phase below MUST start with:

```text
BEFORE starting work:
1. Invoke /superpowers:test-driven-development if available; otherwise read docs/pitfalls/testing-pitfalls.md.
2. Read docs/pitfalls/implementation-pitfalls.md, especially CRED-1.
3. Follow TDD: write failing test -> implement -> verify green.
```

Every code-bearing phase below MUST finish with:

```text
BEFORE marking this phase complete:
1. Review tests against docs/pitfalls/testing-pitfalls.md.
2. Verify test coverage of the fix, including negative/redaction/error paths.
3. Run the listed tests and confirm green.
4. Do not weaken assertions for timing or CI convenience.
```

Global boundaries:

- Do NOT touch `CLAUDE.md`.
- Do NOT change frontend session projection/hook behavior unless a failing test proves backend source classification is insufficient.
- Do NOT change the `LoggedEvent` schema for this fix.
- Do NOT rework `disk_consumer.rs`, `filter_layer.rs`, or the zstd event compression contract for this fix.
- Do NOT introduce a repo-root Cargo workspace.
- Do NOT run live radio or live CMS transmission paths.

## Phase 1 - Tests and Diagnostic-to-UI Bridge Removal

**Execution Status:** ✅ IMPLEMENTED, pending final commit/push

**Goal:** Make the desired boundary executable: diagnostic tracing can produce diagnostic events, but it cannot append or opt into the operator Session log.

**Files:**

- Modify: `src-tauri/tests/session_log_boundary_test.rs`
- Modify: `src-tauri/tests/logging_runtime_boundary_test.rs`
- Modify: `src-tauri/src/logging/mod.rs`
- Delete: `src-tauri/src/logging/ui_consumer.rs`

### Task 1.1 - Write failing boundary tests first

- [ ] Replace the old `session_log=true` positive test in `session_log_boundary_test.rs`.
- [ ] New test: create `SessionLogState`, build a `FanoutLayer`, emit diagnostic `tracing::info!(target: "tuxlink::position::gpsd", "gpsd connected")`, drain the fanout receiver to prove the diagnostic event was emitted, and assert `session_log.snapshot().is_empty()`.
- [ ] New source-scan test in `session_log_boundary_test.rs`: scan production Rust files under `src-tauri/src/**/*.rs` only. Exclude `src-tauri/tests/**` and docs. Assert they do not contain `session_log=true`, `fields.insert("session_log"`, `fields.get("session_log"`, `pub mod ui_consumer`, or `ui_consumer::spawn`.
- [ ] Do not scan for `append_with_seq(` in Phase 1; that API is removed in Phase 2 after diagnostic seq allocation is split.
- [ ] Expected failing state before implementation: the `session_log=true` bridge test or source scan fails because `ui_consumer` still exists and tests still import it.

### Task 1.2 - Remove the bridge

- [ ] Remove `pub mod ui_consumer;` from `src-tauri/src/logging/mod.rs`.
- [ ] Remove the `ui_rx` subscription and `ui_consumer::spawn(...)` block from `logging::init()`.
- [ ] Delete `src-tauri/src/logging/ui_consumer.rs`.
- [ ] Update `logging_runtime_boundary_test.rs`: remove `ui_consumer` import, remove `src/logging/ui_consumer.rs` from `STARTUP_LOGGING_FILES`, and remove `ui_consumer_spawn_does_not_require_current_tokio_reactor`.
- [ ] Do NOT add any replacement diagnostic-to-UI consumer.

**Verification:**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test session_log_boundary_test -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test logging_runtime_boundary_test -- --nocapture
```

## Phase 2 - Split Diagnostic Seq Allocation From SessionLogState

**Execution Status:** ✅ IMPLEMENTED, pending final commit/push

**Goal:** Diagnostic fanout must not mutate `SessionLogState`, including its internal `next_seq` counter.

**Files:**

- Modify: `src-tauri/src/logging/fanout.rs`
- Modify: `src-tauri/src/logging/subscriber.rs`
- Modify: `src-tauri/src/logging/mod.rs`
- Modify: `src-tauri/src/session_log.rs`
- Modify tests importing `FanoutLayer::create(...)`: `src-tauri/tests/emission_coverage_test.rs`, `src-tauri/tests/redaction_integration.rs`, `src-tauri/tests/export_during_writes_test.rs`, `src-tauri/src/logging/fanout.rs` unit tests, `src-tauri/src/logging/visit.rs` unit tests, and `src-tauri/src/logging/subscriber.rs` unit tests.

### Task 2.1 - Write failing seq-isolation test

- [ ] Add/extend a test proving that emitting diagnostic tracing through `FanoutLayer` does not advance the next operator Session log seq.
- [ ] Concrete shape: create `SessionLogState`, emit one diagnostic event through fanout, then append one explicit `LogLine` to `SessionLogState`; expected line seq is `1`, not `2`.
- [ ] Expected failing state before implementation: current fanout calls `SessionLogState::allocate_seq()`, so the appended line receives seq `2`.

### Task 2.2 - Move diagnostic seq into FanoutLayer

- [ ] Add an internal `AtomicU64` diagnostic counter to `FanoutLayer`, initialized to `1`.
- [ ] Change `FanoutLayer::create()` so it no longer accepts or stores `SessionLogState`.
- [ ] Change `on_event()` to allocate diagnostic seq from the new counter.
- [ ] Change `subscriber::build()` and callsites to stop passing `SessionLogState` into fanout.
- [ ] Remove `SessionLogState::allocate_seq()` and `append_with_seq()` once no production code uses them.
- [ ] Update comments in `session_log.rs` so it no longer claims Fanout/UI share the same seq.

**Verification:**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test session_log_boundary_test -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test emission_coverage_test -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib logging::fanout -- --nocapture
```

## Phase 3 - Redacting Explicit Session-Log Helper and Source Correction

**Execution Status:** ✅ IMPLEMENTED, pending final commit/push

**Goal:** All explicit Session log producers share one sanitizing append path, and callers can preserve `LogSource::Wire` for raw protocol lines.

**Files:**

- Modify: `src-tauri/src/session_log.rs`
- Create: `src-tauri/src/session_log_emit.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/ui_commands.rs`
- Modify: `src-tauri/src/bootstrap.rs`
- Modify: `src-tauri/src/winlink/modem/vara/commands.rs`
- Modify: `src-tauri/tests/session_log_boundary_test.rs` for bounded source guards around P2P wire source classification.
- Modify: `src-tauri/src/session_log.rs` unit tests for helper redaction and source preservation.

### Task 3.1 - Write failing helper/redaction tests

- [ ] Add a pure Rust test for the helper core: appending `"bad ;PQ: 23753528 ;PR: 72768415"` stores a line whose message does not contain either numeric token and does contain redaction markers.
- [ ] Add `session_log.rs` unit tests that `append_redacted` redacts credential tokens and preserves caller-provided `LogSource::Wire`.
- [ ] Add a bounded source-level guard in `session_log_boundary_test.rs` for the P2P wire callback: inspect `src-tauri/src/ui_commands.rs` and assert the `wire_log` callback in the P2P block passes `LogSource::Wire`.

### Task 3.2 - Add the helper without dragging AppHandle into core state

- [ ] Add a method named `append_redacted(&self, level: LogLevel, source: LogSource, message: impl AsRef<str>) -> LogLine` on `SessionLogState`. It applies `winlink::redaction::redact_freeform`, appends to `SessionLogState`, and returns the stored `LogLine`.
- [ ] Add `src-tauri/src/session_log_emit.rs` with a thin Tauri wrapper that calls the core helper and emits `session_log:line` using `crate::ui_commands::LogLineDto`.
- [ ] Register `pub mod session_log_emit;` in `src-tauri/src/lib.rs`.
- [ ] Do NOT move `LogLineDto` into logging/export, and do NOT put this helper under `logging`.

### Task 3.3 - Replace duplicated emitters

- [ ] Change `ui_commands::emit_session_line(...)` to accept a `LogSource` parameter and delegate to `session_log_emit::emit(...)`; update all existing non-wire callsites in `ui_commands.rs` to pass `LogSource::Transport`.
- [ ] Update the P2P wire callback around `ui_commands.rs` lines 5230-5240 to pass `LogSource::Wire`.
- [ ] Replace bootstrap progress/wire sink append code with the helper; progress uses `Transport`, wire uses `Wire`, backend problem lines use `Backend`.
- [ ] Replace VARA `emit_vara_log()` duplicate append code with the helper; VARA user-facing transport lines remain `Transport`.
- [ ] Do NOT modify frontend projection logic unless backend source tests cannot prove the behavior.

**Verification:**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib session_log -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test session_log_boundary_test -- --nocapture
```

## Phase 4 - Export Retained Operator Session Transcript

**Execution Status:** ✅ IMPLEMENTED, pending final commit/push

**Goal:** Every log export/archive contains the retained raw operator Session log snapshot as plain JSONL, independent of diagnostic tracing broadcast loss.

**Files:**

- Modify: `src-tauri/src/logging/export.rs`
- Modify: `src-tauri/src/logging/commands.rs`
- Modify: `src-tauri/src/logging/manifest.rs`
- Modify: `src-tauri/src/logging/summary.rs`
- Modify: `src-tauri/tests/export_during_writes_test.rs`
- Modify any export unit tests in `src-tauri/src/logging/export.rs`

### Task 4.1 - Write failing archive tests first

- [ ] Add a test that builds an archive with an empty `SessionLogState` and verifies `operator_session_log.jsonl` exists and is valid empty UTF-8.
- [ ] Add a test that appends retained session lines, exports, extracts `operator_session_log.jsonl`, parses each JSONL line structurally, and verifies `seq`, `timestampIso`, `level`, `source`, and `message`.
- [ ] Add a redaction test: retained session text containing `;PQ: 23753528` and `;PR: 72768415` must not contain either token in `operator_session_log.jsonl`.
- [ ] Add a manifest/summary assertion that operator transcript counts are separate from diagnostic `events` counts.

### Task 4.2 - Extend export inputs and archive contract

- [ ] Add a required `session_log: &'a SessionLogState` to `ExportInputs`.
- [ ] In `build_archive()`, call `session_log.snapshot()` immediately after the flush barrier to keep the transcript capture close to the diagnostic export window.
- [ ] Render `operator_session_log.jsonl` as plain UTF-8 JSONL under the outer tar.zst, not inner zstd-with-dict.
- [ ] Always append `operator_session_log.jsonl`, even when the retained snapshot is empty.
- [ ] Add a private `OperatorSessionLine` archive projection type in `logging/export.rs`; do NOT import `ui_commands::LogLineDto` into export.
- [ ] Projection fields are exactly: `v`, `seq`, `timestampIso`, `level`, `source`, `message`.
- [ ] Sanitize at archive projection as defense in depth: strip ANSI, replace control characters except tab with spaces, apply `redact_freeform`, cap each message to 4096 Unicode scalar values, and increment a truncation counter when capping occurs.
- [ ] Extend `manifest::Counts` and `summary::SummaryInputs` with exactly these fields: `operator_session_log_lines`, `operator_session_log_bytes`, and `operator_session_log_truncated`.
- [ ] Update `logging_export()` and `report_issue_flow()` to pass `handle.session_log`.

**Verification:**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test export_during_writes_test -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib logging::export -- --nocapture
```

## Phase 5 - Narrow Remote-Error Redaction

**Execution Status:** ✅ IMPLEMENTED, pending final commit/push

**Goal:** Close the adjacent confirmed CRED-1 leak where later `***` remote errors bypass freeform redaction before reaching UI/session/export surfaces.

**Files:**

- Modify: `src-tauri/src/winlink/session/mod.rs`

### Task 5.1 - Write failing redaction test

- [ ] Add a unit test for `remote_error("*** saw ;PQ: 23753528 ;PR: 72768415")`.
- [ ] Expected output must not contain `23753528` or `72768415`.
- [ ] Keep the test local to `session/mod.rs`; do not broaden session API surface.

### Task 5.2 - Redact inside `remote_error()`

- [ ] Change only `remote_error()` to call `super::redaction::redact_freeform(rest.trim()).into_owned()`.
- [ ] Do NOT refactor handshake, telnet, or B2F event plumbing in this phase.

**Verification:**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib winlink::session -- --nocapture
```

## Phase 6 - Spec, Plan, and Quality Gates

**Execution Status:** ✅ IMPLEMENTED, pending final commit/push

**Goal:** Prevent future agents from resurrecting the bridge or assuming the diagnostic archive and Session log share one stream.

**Files:**

- Modify: `docs/superpowers/specs/2026-06-04-alpha-logging-design.md`
- Modify: `docs/superpowers/plans/2026-06-04-alpha-logging-implementation.md`
- Modify: this plan's Execution Status sections as work ships
- Modify comments in `src-tauri/src/logging/mod.rs` that describe a UI consumer of diagnostic events.
- Modify comments in `src-tauri/src/session_log.rs` that describe Fanout/UI sharing the same sequence allocation.

### Task 6.1 - Update alpha logging spec

- [x] Remove `session_log=true` as a sanctioned diagnostic-to-UI bridge.
- [x] Replace the pipeline diagram text that shows a UI consumer of diagnostic events.
- [x] Document the new invariant: diagnostic fanout never writes `SessionLogState`; explicit Session log APIs own the operator transcript.
- [x] Document archive member `operator_session_log.jsonl` as the retained raw operator Session log, bounded by the ring size.
- [x] Document that Human/Raw projection is a frontend view over the retained transcript; the export intentionally includes the retained raw transcript after redaction.
- [x] Add a short amendment note to `docs/superpowers/plans/2026-06-04-alpha-logging-implementation.md` stating that this plan supersedes its `ui_consumer.rs`, `session_log=true`, and shared diagnostic/session seq claims. Do not rewrite the historical plan wholesale; add a clear forward pointer.

### Task 6.2 - Run gates

- [x] Run all targeted Rust tests listed in phases 1-5.
- [x] Run docs link lint.
- [x] Run a broader Rust test subset if targeted tests touch shared logging types.
- [ ] Update PR description with `Evidence Checked`, BRF degraded-mode note, and exact gates run.

Suggested gates:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test session_log_boundary_test -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test logging_runtime_boundary_test -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test export_during_writes_test -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test emission_coverage_test -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib logging::export -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib winlink::session -- --nocapture
PATH=/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-h1gh-evidence-logging/node_modules/.bin:$PATH pnpm lint:docs
```

Actual gates run by `yew-sorrel-condor`:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test session_log_boundary_test -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test logging_runtime_boundary_test -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test export_during_writes_test -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test emission_coverage_test -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test redaction_integration -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib
pnpm lint:docs
```

Result: all green. Broad lib suite: 1315 passed, 0 failed.
