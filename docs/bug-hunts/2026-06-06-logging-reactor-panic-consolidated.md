# Logging Reactor Panic Bug Hunt - Consolidated Findings

**Date:** 2026-06-06
**Scope:** Tuxlink alpha logging startup/runtime-boundary failure that panics at `src-tauri/src/logging/free_disk_guard.rs:21` on launch, plus adjacent logging code included in the hunter scope.
**Hunters:** Exploratory, Holistic, Multipass, Differential

---

## Completeness Accounting

Every source-level finding from the four reports is accounted for below.

| Finding | Hunters | Disposition |
|---|---|---|
| Logging startup calls bare `tokio::spawn` from synchronous Tauri setup and panics before fail-soft logging can return | Exploratory, Holistic, Multipass, Differential | Confirmed bug B1 |
| Disk consumer, UI consumer, and bounded timer repeat the same startup direct-`tokio::spawn` pattern | Exploratory, Holistic, Multipass, Differential | Confirmed bug B1 |
| First-paint env probe listener uses bare `tokio::spawn` from a synchronous Tauri listener callback | Exploratory concern, Holistic concern, Multipass, Differential | Confirmed bug B2 |
| Disk-consumer init failure leaves global tracing subscriber installed with no active consumer | Multipass | Confirmed bug B3 |
| Retention changes do not reach future disk-consumer rotation sweeps | Holistic | Confirmed bug B4 |
| Export silently treats unreadable log files as empty and returns success | Multipass | Confirmed bug B5 |
| Retention / clear-history can delete active log if tracker lock is contended | Multipass | Confirmed bug B6 |
| Logging init can report `Full` when `set_global_default` failed | Multipass | Confirmed bug B7 |
| Retention reports failed deletions as successful deletions | Multipass | Confirmed bug B8 |
| Export manifest can record pass-1 archive size instead of final archive size | Holistic | Confirmed bug B9 |
| `state_dir` docs/error text imply all path components reject symlinks, but implementation checks only the leaf plus canonical containment | Holistic concern | Design decision D1 |
| JSONL event producer/export reader relationship | Differential | Checked clean FP1 |
| Export flush barrier/disk consumer ack relationship | Differential | Checked clean FP2 |
| FreeDiskGuard pause flag/disk consumer pause check relationship | Differential | Checked clean FP3 |

Unique report findings: 14. Dispositions below: 9 confirmed bugs, 1 design decision, 3 checked-clean false positives, 1 folded into B1/B2 design concern accounting.

---

## Confirmed Bugs

### B1. Logging Startup Spawns Tokio Tasks Without Entering a Tokio Runtime

**Consensus:** Found by all four hunters.
**Location:** `src-tauri/src/lib.rs:160`, `src-tauri/src/lib.rs:170`, `src-tauri/src/logging/free_disk_guard.rs:21`, `src-tauri/src/logging/disk_consumer.rs:75`, `src-tauri/src/logging/disk_consumer.rs:102`, `src-tauri/src/logging/ui_consumer.rs:17`, `src-tauri/src/logging/bounded_timer.rs:48`
**Evidence:** Tauri calls a synchronous `.setup(|app| ...)` closure at `lib.rs:160`; that closure calls `logging::init(session_log)` at `lib.rs:170`. `logging::init()` immediately calls `FreeDiskGuard::spawn` at `logging/mod.rs:95`, and `FreeDiskGuard::spawn` calls bare `tokio::spawn` at `free_disk_guard.rs:21`. Bare `tokio::spawn` requires a current Tokio runtime, matching the operator-reported panic exactly. The same startup path also reaches direct `tokio::spawn` in `disk_consumer.rs:75`, `disk_consumer.rs:102`, `ui_consumer.rs:17`, and conditionally `bounded_timer.rs:48` when persisted settings are `DetailedMode::Bounded`. Adjacent startup code uses the Tauri runtime explicitly at `bootstrap.rs:236` and `position/gpsd.rs:126`.
**Impact:** The compiled app panics during launch before the GUI is usable. Fixing only `free_disk_guard.rs:21` leaves follow-on startup panics.
**Blast radius:** Logging startup worker APIs and any tests that construct logging handles or assert bounded timer behavior. Expected implementation surface is local to `src-tauri/src/logging/*` and possibly the `.setup` call site if a spawner/runtime handle is passed explicitly.
**Fix approach:** Route all logging startup tasks through `tauri::async_runtime::spawn` or a narrow logging-local spawn abstraction that is valid from Tauri setup. Add a regression that exercises the production launch/setup path rather than only `#[tokio::test]` contexts.

### B2. First-Paint Env Probe Runner Repeats the Runtime-Boundary Bug

**Consensus:** Reported as a bug by Multipass and Differential; flagged as the same risky pattern by Exploratory and Holistic.
**Location:** `src-tauri/src/lib.rs:176`, `src-tauri/src/logging/env_probes/mod.rs:199`, `src-tauri/src/logging/env_probes/mod.rs:201`
**Evidence:** After `logging::init()` succeeds, `.setup` registers the probe runner at `lib.rs:176`. `spawn_runner()` installs a synchronous Tauri listener with `app.listen("first_paint_complete", ...)` at `env_probes/mod.rs:199`; the listener body calls bare `tokio::spawn` at `env_probes/mod.rs:201`. The local code does not enter a Tokio runtime before that spawn and does not use the app-start pattern shown at `bootstrap.rs:236` / `position/gpsd.rs:126`.
**Impact:** After B1 is fixed, the app can still panic or lose probe execution when the frontend emits `first_paint_complete`.
**Blast radius:** Probe runner startup and first-paint diagnostics only. It shares the same remediation pattern as B1.
**Fix approach:** Include `env_probes::spawn_runner` in the same runtime-boundary remediation as B1, using Tauri's async runtime or the same logging-local spawn abstraction.

### B3. Disk-Consumer Init Failure Leaves Tracing Installed With No Consumers

**Consensus:** Found by Multipass.
**Location:** `src-tauri/src/logging/mod.rs:91`, `src-tauri/src/logging/mod.rs:92`, `src-tauri/src/logging/mod.rs:106`, `src-tauri/src/logging/mod.rs:115`
**Evidence:** `logging::init()` builds and installs the global subscriber at `logging/mod.rs:91-92` before `disk_consumer::spawn(...)` at `logging/mod.rs:106-113`. If appender setup fails, `init()` returns `InitOutcome::Degraded` at `logging/mod.rs:115-118` before starting the UI consumer at `logging/mod.rs:126-127`. The installed fanout subscriber then broadcasts best-effort at `fanout.rs:149-150` with no disk or UI consumer active.
**Impact:** The app degrades, but later warnings/errors can disappear instead of reaching stderr or the session log.
**Blast radius:** Logging initialization order and degraded logging behavior.
**Fix approach:** Install the global fanout subscriber only after fallible disk setup succeeds, or add an explicit degraded subscriber/consumer path for partial init failures.

### B4. Retention Updates Do Not Reach Future Rotation Sweeps

**Consensus:** Found by Holistic; follow-up issue already filed by the holistic subagent as `tuxlink-fuog`.
**Location:** `src-tauri/src/logging/mod.rs:97`, `src-tauri/src/logging/disk_consumer.rs:157`, `src-tauri/src/logging/commands.rs:222`
**Evidence:** Startup captures `RetentionConfig` once at `logging/mod.rs:97-100` and passes it by value into `disk_consumer::spawn(...)`. The disk consumer later uses that captured value for every rotation sweep at `disk_consumer.rs:157-162`. `logging_set_retention` saves updated settings at `commands.rs:222-230` and performs only one immediate sweep at `commands.rs:231-233`; it does not update the long-lived disk consumer config.
**Impact:** A changed retention policy appears to apply immediately but later rotations continue using stale startup values until restart.
**Blast radius:** Retention settings command and disk consumer rotation sweeps.
**Fix approach:** Store retention config behind shared state that the disk consumer reads at sweep time, or send config updates to the disk consumer task.

### B5. Export Silently Drops Unreadable Log Files From Successful Archives

**Consensus:** Found by Multipass.
**Location:** `src-tauri/src/logging/export.rs:132`
**Evidence:** Export enumerates JSONL files at `export.rs:122-130`, then reads each file with `std::fs::read_to_string(path).unwrap_or_default()` at `export.rs:132-133`. Per-file read errors become empty content, while `build_archive` still returns `Ok(ExportResult { ... })` at `export.rs:301-306`.
**Impact:** Support archives can omit log files while reporting success, with no warning to the operator.
**Blast radius:** Export/report-issue artifact generation.
**Fix approach:** Track skipped/unreadable files in the manifest/summary or fail the export, depending on product intent.

### B6. Retention and Clear-History Can Delete the Active Log When the Tracker Lock Is Contended

**Consensus:** Found by Multipass.
**Location:** `src-tauri/src/logging/commands.rs:232`, `src-tauri/src/logging/commands.rs:312`, `src-tauri/src/logging/retention.rs:80`, `src-tauri/src/logging/retention.rs:105`
**Evidence:** `logging_set_retention` uses `handle.active_file_path.try_lock().ok().and_then(|g| g.clone())` at `commands.rs:232`; contention becomes `None`, then `retention::sweep` can delete any eligible `tuxlink.*.jsonl` file except the optional active path it was not given. `logging_clear_history` repeats the pattern at `commands.rs:312` and removes files at `commands.rs:324`. The disk consumer updates the tracker under the async mutex at `disk_consumer.rs:151-155`.
**Impact:** During brief lock contention, an operator action can unlink the log currently being written. Exports may miss current-session events until rotation.
**Blast radius:** Retention command, clear-history command, active-file tracker synchronization.
**Fix approach:** Treat lock contention as an error/deferred operation, or use an awaitable command path/runtime bridge so the active path is known before destructive filesystem operations.

### B7. Logging Init Can Report Full When the Subscriber Was Not Installed

**Consensus:** Found by Multipass.
**Location:** `src-tauri/src/logging/mod.rs:92`, `src-tauri/src/logging/mod.rs:169`
**Evidence:** `logging::init()` ignores `tracing::subscriber::set_global_default(subscriber)` at `logging/mod.rs:92` and later returns `InitOutcome::Full(handle_arc)` at `logging/mod.rs:169`. `set_global_default` is fallible; if another global subscriber is already installed, logging can be reported as full while tracing events never reach the logging fanout.
**Impact:** The UI can think alpha logging is initialized while no tracing events reach disk or the live logging window.
**Blast radius:** Logging initialization and degraded-status reporting.
**Fix approach:** Handle `set_global_default` failure explicitly and return `Degraded`, or design an intentional fallback for repeated/dev-mode initialization.

### B8. Retention Reports Failed Deletions as Successful

**Consensus:** Found by Multipass.
**Location:** `src-tauri/src/logging/retention.rs:105`
**Evidence:** `retention::sweep()` discards `std::fs::remove_file(path)`'s result at `retention.rs:105-106`, then unconditionally increments `deleted_count` and `deleted_bytes` at `retention.rs:107-108`.
**Impact:** Retention status can claim files were deleted even when permissions/races prevented deletion.
**Blast radius:** Retention metrics, operator diagnostics, and any enforcement expectations around disk cap.
**Fix approach:** Count only successful removals and report failed removals separately.

### B9. Export Manifest Can Record Pass-1 Archive Size Instead of Final Archive Size

**Consensus:** Found by Holistic; follow-up issue already filed by the holistic subagent as `tuxlink-xkal`.
**Location:** `src-tauri/src/logging/export.rs:288`
**Evidence:** `build_archive()` builds a pass-1 archive to measure `outer_size` at `export.rs:279-281`, stores that value in `final_manifest.compression.outer_archive_bytes` at `export.rs:287-288`, then rebuilds the final archive at `export.rs:289`. The returned `ExportResult.archive_size_bytes` uses the final bytes at `export.rs:301-304`, but the embedded manifest can contain the pass-1 value.
**Impact:** Export manifests can disagree with the actual archive size and returned command result.
**Blast radius:** Export metadata integrity only.
**Fix approach:** Reconcile manifest size after the final build, or use a deterministic fixed-point build/measurement strategy with a test that extracts the manifest and compares it to the final archive size.

---

## Design Decisions Requiring User Input

### D1. Should `state_dir` Refuse Symlinks in Every Path Component?

**Location:** `src-tauri/src/logging/state_dir.rs:1`, `src-tauri/src/logging/state_dir.rs:11`, `src-tauri/src/logging/state_dir.rs:38`
**The concern:** The module docs say "symlink refusal" and the error says "path component is a symlink," but the implementation only checks the final `logs` path with `symlink_metadata`; it then enforces canonical containment under the resolved base.
**Why this needs a decision:** Canonical containment blocks obvious escapes, so this is not proven as a current exploit in this bug hunt. The question is whether the product/security contract requires rejecting every symlinked component or whether canonical containment plus leaf refusal is the intended policy.
**Options:** Option A: tighten implementation to reject all symlinked components. Option B: narrow docs/error text to the current leaf-refusal behavior. Option C: defer to a security-focused logging/storage pass.
**Recommendation:** Defer from the P0 launch fix unless you want this remediation plan to include storage hardening; the current operator-blocking bug is the runtime boundary.

---

## False Positives

### FP1. JSONL Event Producer/Export Reader Relationship

**Flagged by:** Differential as examined relationship, no bug.
**Why invalid:** `LoggedEvent` derives `Serialize` and `Deserialize` at `event.rs:12`; disk writes use `to_jsonl()` and export reads with `serde_json::from_str::<LoggedEvent>`. No source-proven mismatch was found.

### FP2. Export Flush Barrier/Disk Consumer Ack Relationship

**Flagged by:** Differential as examined relationship, no bug.
**Why invalid:** Export calls `flush_and_wait()` before reading files, and the disk consumer receives, drains, flushes, and acks in the inspected producer/consumer pair. No source-proven disagreement was found.

### FP3. FreeDiskGuard Pause Producer/Disk Consumer Pause Consumer

**Flagged by:** Differential as examined relationship, no bug.
**Why invalid:** `logging::init()` passes the same `Arc<AtomicBool>` into the disk consumer; the guard sets/clears it and the disk writer checks it before writing. No source-proven semantic mismatch was found.

---

## Bugs Outside Primary Scope

The primary incident is the P0 launch panic. B3 through B9 are real logging bugs found while auditing adjacent scoped files, but they are not required to make the app launch again. They can be included in this remediation cycle or left as follow-up work.

Already-filed follow-ups:

- `tuxlink-fuog`: B4, stale retention config in future rotation sweeps.
- `tuxlink-xkal`: B9, export manifest stale outer archive size.

Recommended scope split:

- Include B1 and B2 in the immediate P0 remediation because they share the same runtime-boundary root cause and can otherwise create serial crashes.
- Decide whether to include B3 and B7 with B1, because they are also `logging::init()` truthfulness/degraded-mode bugs.
- Defer B4-B6, B8-B9 unless you want this plan to be a broader alpha-logging integrity cleanup.

---

## Test Gap Analysis

### B1. Logging Startup Spawns Tokio Tasks Without Entering a Tokio Runtime

**Why missed:** Existing tests cover individual helpers, and bounded timer tests run under `#[tokio::test]`, which creates the runtime that production setup lacks. No test exercises the logging startup path from Tauri's real synchronous `.setup(...)` context.
**Pitfall coverage:** Covered by the newly added `docs/pitfalls/testing-pitfalls.md` §5 item: "production runtime-boundary spawns are tested from their real caller context."
**Catch test:** A launch/setup regression that initializes logging from the production Tauri setup path, or a focused sync-context test around the logging spawn abstraction proving it does not call bare `tokio::spawn` from a thread without a reactor.

### B2. First-Paint Env Probe Runner Repeats the Runtime-Boundary Bug

**Why missed:** Probe tests call each probe's read-only `run(...)` function directly and check RADIO-1 safety/serialization. They do not register the Tauri listener and emit `first_paint_complete`, so the listener callback's spawn boundary is never exercised.
**Pitfall coverage:** Covered by the same new §5 runtime-boundary pitfall.
**Catch test:** Register the probe runner through the app/runtime path, emit `first_paint_complete`, and assert the task is spawned through the app runtime without panic.

### B3. Disk-Consumer Init Failure Leaves Tracing Installed With No Consumers

**Why missed:** There is no test that forces `disk_consumer::spawn` to fail after `set_global_default` succeeds. Current degraded-path coverage appears focused on state-dir failure, pure helper behavior, and successful export/retention paths.
**Pitfall coverage:** Covered by existing §3 Error Path Coverage, especially "each error branch has a test" and "error-path side effects verified."
**Catch test:** Force appender initialization to fail after subscriber construction, then assert `logging::init()` reports degraded and later warning/error diagnostics still surface through the intended degraded channel.

### B4. Retention Updates Do Not Reach Future Rotation Sweeps

**Why missed:** Retention tests call `retention::sweep()` directly, and `logging_set_retention` performs one immediate sweep. No test mutates retention settings and then triggers a later disk-consumer rotation/sweep in the long-lived worker.
**Pitfall coverage:** Covered by the newly added `docs/pitfalls/testing-pitfalls.md` §6 item: "runtime setting changes reach long-lived workers after the immediate command path."
**Catch test:** Start the disk consumer with initial retention settings, call `logging_set_retention`, trigger a rotation sweep, and assert the later sweep uses the new policy.

### B5. Export Silently Drops Unreadable Log Files From Successful Archives

**Why missed:** Export tests create readable fixture files and assert that archives decode and contain parseable JSONL. They do not include an unreadable file or an enumerate-then-read race.
**Pitfall coverage:** Covered by existing §3 Error Path Coverage; no new general pitfall needed.
**Catch test:** Include a selected JSONL path that cannot be read, then assert export either fails or records the skipped file explicitly in the manifest/summary according to the chosen product behavior.

### B6. Retention and Clear-History Can Delete the Active Log When the Tracker Lock Is Contended

**Why missed:** Retention tests pass an active path directly to `sweep()` and verify that it is preserved. They do not exercise the Tauri commands while `active_file_path` is locked or unavailable.
**Pitfall coverage:** Covered by existing §5 Concurrency & TOCTOU, especially multi-step flows under concurrent access.
**Catch test:** Hold the active-file tracker lock while invoking `logging_set_retention` and `logging_clear_history`, then assert the active log is not removed and the command either waits or returns a safe error.

### B7. Logging Init Can Report Full When the Subscriber Was Not Installed

**Why missed:** No test forces `tracing::subscriber::set_global_default` to fail before calling `logging::init()`. The result is ignored, so successful downstream helper setup masks the failed subscriber install.
**Pitfall coverage:** Covered by existing §3 Error Path Coverage.
**Catch test:** Preinstall a global subscriber, call `logging::init()`, and assert it returns `Degraded` or another explicit failure state rather than `Full`.

### B8. Retention Reports Failed Deletions as Successful

**Why missed:** Retention tests use deletable temp files and assert happy-path deletion counts. They do not force `remove_file` failure.
**Pitfall coverage:** Covered by existing §3 Error Path Coverage.
**Catch test:** Force a deletion failure in a platform-appropriate fixture and assert `deleted_count` only counts successful removals while failures are reported separately.

### B9. Export Manifest Can Record Pass-1 Archive Size Instead of Final Archive Size

**Why missed:** Export tests verify archive structure and JSONL validity, but they do not extract `manifest.json` and compare its `compression.outer_archive_bytes` value against the final file length and returned `ExportResult.archive_size_bytes`.
**Pitfall coverage:** One-off export-artifact validation gap noted for the fix plan; no new general pitfall beyond existing "assert the correct value produced" guidance is needed.
**Catch test:** Build an archive, extract `manifest.json`, and assert `compression.outer_archive_bytes == std::fs::metadata(output_path).len() == ExportResult.archive_size_bytes`.

### Testing Pitfalls Updates

- Added before consolidation by the holistic hunter: §5 runtime-boundary spawn tests must exercise the production caller context.
- Added before consolidation by the holistic hunter: §6 runtime setting changes must reach long-lived workers after the immediate command path.
- No additional testing-pitfalls edits were made in Phase 4; the remaining gaps map to existing error-path, concurrency/TOCTOU, or artifact-specific test guidance.
