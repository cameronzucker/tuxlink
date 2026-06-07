# Bug Hunt Report

## Scope

Date: 2026-06-06

Scope slug: logging-reactor-panic

Analyzed production source only for the requested startup logging path:

- `src-tauri/src/main.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/src/logging/mod.rs`
- `src-tauri/src/logging/free_disk_guard.rs`
- `src-tauri/src/logging/disk_consumer.rs`
- `src-tauri/src/logging/ui_consumer.rs`
- `src-tauri/src/logging/bounded_timer.rs`
- `src-tauri/src/logging/env_probes/mod.rs`
- `src-tauri/src/logging/export.rs`
- `src-tauri/src/logging/logging_handle.rs`
- `src-tauri/src/logging/subscriber.rs`
- `src-tauri/src/logging/state_dir.rs`
- Adjacent source context: `src-tauri/src/bootstrap.rs`, `src-tauri/src/position/gpsd.rs`, selected production Tokio context from `src-tauri/src/modem_status.rs`.

Passes performed so far:

- Pass 1 — Contract Violations
- Pass 2 — Cross-Sibling Pattern Violations
- Pass 3 — Failure Mode Reasoning
- Pass 4 — Concurrency Reasoning
- Pass 5 — Error Propagation

## Bugs

### Logging startup promises fail-soft initialization but panics before returning

**Location:** `src-tauri/src/logging/mod.rs:57`

**Severity:** critical

**Evidence:** `logging::init()` is documented as the single startup owner that "Returns `InitOutcome::Full(handle)` on success or `InitOutcome::Degraded { reason }` if the state dir is unavailable" (`src-tauri/src/logging/mod.rs:57-59`), and the caller treats that as a fail-soft launch path in `.setup(...)` (`src-tauri/src/lib.rs:163-186`). But the same synchronous `init()` body calls `free_disk_guard::FreeDiskGuard::spawn(log_dir.clone())` at `src-tauri/src/logging/mod.rs:95`, whose implementation immediately calls `tokio::spawn(async move { ... })` at `src-tauri/src/logging/free_disk_guard.rs:21`. A direct `tokio::spawn` requires an active Tokio runtime on the current thread; when `init()` is reached from the synchronous Tauri setup path (`src-tauri/src/main.rs:4-5`, `src-tauri/src/lib.rs:160-170`), there is no proven Tokio runtime context, matching the reported launch panic at `free_disk_guard.rs:21`.

**Impact:** The app can compile successfully but abort during launch before logging returns `InitOutcome::Degraded`, so the alpha diagnostic logging pipeline prevents the whole Tauri app from starting.

**Found in:** Pass 1 — Contract Violations

### Logging worker spawners all bypass the app-start async-runtime pattern

**Location:** `src-tauri/src/logging/disk_consumer.rs:75`, `src-tauri/src/logging/disk_consumer.rs:102`, `src-tauri/src/logging/ui_consumer.rs:17`, `src-tauri/src/logging/bounded_timer.rs:48`, `src-tauri/src/logging/env_probes/mod.rs:201`

**Severity:** critical

**Evidence:** The adjacent app-start async workers use Tauri's runtime handle: `bootstrap::install_native` calls `tauri::async_runtime::spawn(async move { ... })` at `src-tauri/src/bootstrap.rs:236`, and `position::gpsd::spawn_gpsd_client` calls `tauri::async_runtime::spawn(run_gpsd_client(...))` at `src-tauri/src/position/gpsd.rs:126`. The logging worker siblings deviate from that app-start pattern and call direct `tokio::spawn` instead: the disk appender error poller at `disk_consumer.rs:75`, the disk consumer loop at `disk_consumer.rs:102`, the UI consumer at `ui_consumer.rs:17`, the bounded revert timer at `bounded_timer.rs:48`, and the first-paint probe task at `env_probes/mod.rs:201`. These spawners are wired from the same synchronous setup path: `logging::init()` calls the disk consumer at `src-tauri/src/logging/mod.rs:106-113`, UI consumer at `src-tauri/src/logging/mod.rs:126-127`, bounded timer at `src-tauri/src/logging/mod.rs:166-167`, and `lib.rs` calls `env_probes::spawn_runner(...)` at `src-tauri/src/lib.rs:176-179`.

**Impact:** Fixing only the first observed panic at `free_disk_guard.rs:21` is incomplete. The next direct `tokio::spawn` reached during startup can panic the same way, and the first-paint probe callback has the same runtime-context dependency when it later handles the frontend event.

**Found in:** Pass 2 — Cross-Sibling Pattern Violations

### Disk-consumer init failure leaves tracing installed with no consumers

**Location:** `src-tauri/src/logging/mod.rs:91`

**Severity:** significant

**Evidence:** `logging::init()` builds and installs the global subscriber before it knows whether disk logging can start: `subscriber::build(...)` at `src-tauri/src/logging/mod.rs:91` and `tracing::subscriber::set_global_default(subscriber)` at `src-tauri/src/logging/mod.rs:92`. Only after that does it call `disk_consumer::spawn(...)` at `src-tauri/src/logging/mod.rs:106-113`. If the appender build fails, `init()` returns `InitOutcome::Degraded` at `src-tauri/src/logging/mod.rs:115-118` before spawning the UI consumer (`src-tauri/src/logging/mod.rs:126-127`). In that failure path the already-installed global subscriber is the fanout subscriber, but the disk receiver has been dropped by the failed spawn and no UI consumer has been started; later fanout sends are best-effort and ignored at `src-tauri/src/logging/fanout.rs:149-150`. Unlike the earlier state-dir failure path, which installs a stderr subscriber at `src-tauri/src/logging/mod.rs:68-75`, this partial-init path leaves tracing events with no active consumer.

**Impact:** A writable-at-resolve but failing-at-appender-open log directory degrades the app, but subsequent `tracing::warn!` / `tracing::error!` diagnostics can disappear instead of surfacing to stderr or the session log. That makes the degraded diagnostic mode much less useful exactly when disk logging failed.

**Found in:** Pass 3 — Failure Mode Reasoning

### Export silently drops unreadable log files from successful archives

**Location:** `src-tauri/src/logging/export.rs:132`

**Severity:** significant

**Evidence:** The archive pipeline explicitly enumerates JSONL files at `src-tauri/src/logging/export.rs:117-130` and then reads each selected file. Directory read errors propagate through `std::fs::read_dir(inputs.log_dir)?` at `src-tauri/src/logging/export.rs:122`, but per-file read errors are converted to empty content by `std::fs::read_to_string(path).unwrap_or_default()` at `src-tauri/src/logging/export.rs:132-133`. The archive build then continues, counts only successfully parsed events at `src-tauri/src/logging/export.rs:153-165`, and returns `Ok(ExportResult { ... })` at `src-tauri/src/logging/export.rs:301-306`.

**Impact:** If one log file is temporarily unreadable, truncated by a permissions race, or removed between enumeration and read, the user receives a successful export archive that silently omits that file's events. The reported `events_in_archive` count reflects the reduced data but carries no warning that source log data was skipped.

**Found in:** Pass 3 — Failure Mode Reasoning

### Retention and clear-history can delete the active log when the tracker lock is contended

**Location:** `src-tauri/src/logging/commands.rs:232`

**Severity:** significant

**Evidence:** The active-file tracker is an async mutex updated by the disk consumer during rotation at `src-tauri/src/logging/disk_consumer.rs:151-155`. The retention command uses a non-blocking `try_lock()` at `src-tauri/src/logging/commands.rs:232` and converts lock contention into `None`, then calls `retention::sweep(...)` at `src-tauri/src/logging/commands.rs:233`. `retention::sweep()` only preserves the active file when `active_file_path` matches at `src-tauri/src/logging/retention.rs:80-83`; when the caller passed `None`, matching log files are eligible for deletion at `src-tauri/src/logging/retention.rs:105-107`. The clear-history command has the same pattern: `try_lock()` at `src-tauri/src/logging/commands.rs:312`, skips only if the optional active path matches at `src-tauri/src/logging/commands.rs:321-322`, and removes files at `src-tauri/src/logging/commands.rs:324`.

**Impact:** If the operator updates retention or clears history while the disk consumer briefly holds the active-file tracker lock, the command can behave as if there is no active file and unlink the file currently being written. On Unix the writer may keep writing to an unlinked inode, so subsequent exports that enumerate `tuxlink.*.jsonl` can miss current-session events until the next rotation.

**Found in:** Pass 4 — Concurrency Reasoning

### Logging init can report Full even when the subscriber was not installed

**Location:** `src-tauri/src/logging/mod.rs:92`

**Severity:** significant

**Evidence:** `logging::init()` ignores the result of `tracing::subscriber::set_global_default(subscriber)` at `src-tauri/src/logging/mod.rs:92` and continues building the full logging handle. The function later returns `InitOutcome::Full(handle_arc)` at `src-tauri/src/logging/mod.rs:166-169`. Because `set_global_default` is fallible, any prior global subscriber would make this call fail, but the function still spawns consumers and returns a handle whose broadcast channel is not connected to the actual global tracing dispatcher.

**Impact:** The app can display logging as fully initialized while tracing events never reach the alpha logging fanout, disk writer, or UI consumer. The failure is neither degraded nor surfaced to the operator.

**Found in:** Pass 5 — Error Propagation

### Retention reports failed deletions as successful deletions

**Location:** `src-tauri/src/logging/retention.rs:105`

**Severity:** minor

**Evidence:** When a file matches the retention policy, `retention::sweep()` calls `let _ = std::fs::remove_file(path);` at `src-tauri/src/logging/retention.rs:105-106`, discards the result, and then unconditionally increments `deleted_count` and `deleted_bytes` at `src-tauri/src/logging/retention.rs:107-108`. The caller logs those counts as completed work in `logging_set_retention` at `src-tauri/src/logging/commands.rs:234-238`, but there is no indication when the file was not actually removed.

**Impact:** Permission errors, races, or open-file behavior can leave retained files on disk while status/logging claims they were deleted. That can hide retention cap failures and mislead the diagnostic surface.

**Found in:** Pass 5 — Error Propagation

## Design Concerns

- The logging pipeline has several fire-and-forget async workers but no local abstraction for "spawn on Tauri runtime from setup/event callbacks." That makes it easy for individual worker modules to accidentally depend on a current Tokio runtime instead of the application runtime.
- Logging init performs irreversible global subscriber installation before all fallible startup steps have succeeded. That ordering makes fail-soft behavior harder to keep truthful because later failures cannot cleanly replace the subscriber with a degraded stderr path.
- Synchronous Tauri commands are peeking at `tokio::sync::Mutex` state with `try_lock()` and then making destructive filesystem decisions. For active-file protection, "could not inspect right now" is not equivalent to "no active file."
- Several diagnostics paths prefer best-effort behavior, which is appropriate for launch resilience, but the code sometimes reports success after losing the fact that a fallible operation failed. The operator-facing logging UI depends on those distinctions being accurate.
