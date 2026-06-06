# Bug Hunt Report

## Scope

Analyzed production source for the requested `logging-reactor-panic` scope in `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-xvqy-logging-reactor-panic`.

Primary source read before reasoning:

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

Adjacent source/context read for runtime-boundary patterns and cross-flow evidence:

- `src-tauri/src/bootstrap.rs`
- `src-tauri/src/position/gpsd.rs`
- selected production/test-context Tokio usage in `src-tauri/src/modem_status.rs`
- `src-tauri/src/logging/commands.rs`
- `src-tauri/src/logging/retention.rs`
- `src-tauri/src/logging/settings.rs`

Approach: read the scoped implementation first, then followed source-proven control flow from app entry to logging initialization, async worker spawning, retention changes, and export archive metadata. I did not run or modify tests and did not change application code.

## Bugs

### Logging startup spawns Tokio tasks without entering a Tokio runtime

**Location:** `src-tauri/src/logging/free_disk_guard.rs:21`

**Severity:** critical

**Evidence:** The app enters `tuxlink_lib::run()` from `src-tauri/src/main.rs:4-5`. `src-tauri/src/lib.rs:160-170` runs a synchronous Tauri `.setup(|app| ...)` closure and calls `crate::logging::init(session_log)` directly. Inside that initializer, `src-tauri/src/logging/mod.rs:94-95` constructs the async mutex state and immediately calls `free_disk_guard::FreeDiskGuard::spawn(log_dir.clone())`. `FreeDiskGuard::spawn` calls bare `tokio::spawn(async move { ... })` at `src-tauri/src/logging/free_disk_guard.rs:21`.

Bare `tokio::spawn` requires a current Tokio runtime on the calling thread. The reported launch panic names this exact call site and error: "there is no reactor running, must be called from the context of a Tokio 1.x runtime". The source path contains no async boundary, `tokio::runtime::Handle`, or `tauri::async_runtime::spawn` before that call.

This is also a sibling-pattern violation in the same startup phase. Adjacent app-start code uses Tauri's runtime explicitly: `src-tauri/src/bootstrap.rs:236` calls `tauri::async_runtime::spawn(async move { ... })`, and `src-tauri/src/position/gpsd.rs:120-126` documents and uses `tauri::async_runtime::spawn(run_gpsd_client(...))`. Logging's startup workers instead use bare Tokio spawns:

- `src-tauri/src/logging/disk_consumer.rs:75` and `src-tauri/src/logging/disk_consumer.rs:102`, reached from `src-tauri/src/logging/mod.rs:106-113`
- `src-tauri/src/logging/ui_consumer.rs:17`, reached from `src-tauri/src/logging/mod.rs:126-127`
- `src-tauri/src/logging/bounded_timer.rs:48` when persisted settings are `DetailedMode::Bounded`, reached from `src-tauri/src/logging/mod.rs:166-167`

**Impact:** The compiled app panics during launch before `.setup(...)` completes, so the GUI never starts. Fixing only the first free-disk guard spawn would leave later startup spawns in the disk consumer, UI consumer, and persisted-bounded timer paths able to fail the same way.

### Retention changes are only applied to the immediate sweep; future rotation sweeps use stale settings

**Location:** `src-tauri/src/logging/mod.rs:97`

**Severity:** significant

**Evidence:** During startup, `logging::init()` reads the current settings into a one-time `retention_cfg` at `src-tauri/src/logging/mod.rs:97-100` and passes that value into `disk_consumer::spawn(...)` at `src-tauri/src/logging/mod.rs:106-113`. `disk_consumer::spawn` receives the config by value at `src-tauri/src/logging/disk_consumer.rs:44-50`, then the long-lived consumer task uses that captured `retention_config` for every hour-rotation sweep at `src-tauri/src/logging/disk_consumer.rs:157-162`.

The operator command for changing retention saves new settings at `src-tauri/src/logging/commands.rs:222-230` and runs one immediate sweep with a freshly constructed config at `src-tauri/src/logging/commands.rs:231-233`, but it never updates the config captured by the already-running disk consumer. `settings::save` persists the new values to disk at `src-tauri/src/logging/settings.rs:46-53`; the disk consumer does not reload that file and has no shared config handle to observe the new values.

**Impact:** A retention change appears to work immediately, but future hourly rotation sweeps continue enforcing the old startup limits until the app restarts. Depending on the direction of the change, the logging pipeline can keep too much diagnostic history or delete logs according to a stricter cap the operator already replaced.

### Export manifest records the first-pass archive size instead of the final archive size

**Location:** `src-tauri/src/logging/export.rs:288`

**Severity:** minor

**Evidence:** `build_archive` creates an initial manifest with `compression.outer_archive_bytes: 0` at `src-tauri/src/logging/export.rs:240-249`, builds a first archive to measure it at `src-tauri/src/logging/export.rs:279-281`, then writes that first-pass size into `final_manifest.compression.outer_archive_bytes` at `src-tauri/src/logging/export.rs:287-288`. The final archive is then rebuilt from the modified manifest at `src-tauri/src/logging/export.rs:289`.

Changing the manifest content from `outer_archive_bytes: 0` to the measured value changes the tar payload and can change the compressed archive length. The function returns the actual final size from `outer_compressed.len()` at `src-tauri/src/logging/export.rs:301-304`, but the embedded manifest keeps the earlier `outer_size` value from the pass-1 archive. There is no second measurement/update check after `outer_compressed` is produced.

**Impact:** Diagnostic exports can contain a manifest whose `compression.outer_archive_bytes` disagrees with the archive's real byte size and with the `ExportResult.archive_size_bytes` returned to the UI/report-issue flow. That weakens the integrity of support artifacts generated by the logging pipeline.

## Design Concerns

The logging subsystem lacks one local, shared spawn abstraction for "run async work from Tauri setup/event/command code." App-start siblings already use `tauri::async_runtime::spawn`, while logging modules call `tokio::spawn` directly across startup workers and bounded timers. The first-paint probe runner shows the same risky shape after launch: `src-tauri/src/logging/env_probes/mod.rs:197-199` registers a synchronous Tauri event listener, and that callback calls bare `tokio::spawn` at `src-tauri/src/logging/env_probes/mod.rs:201`. The source read for this hunt proves the setup path lacks a reactor; it does not prove the listener callback thread does, so I treated the probe runner as a design concern rather than a separate bug.

`src-tauri/src/logging/state_dir.rs:1-2` describes "symlink refusal" and `ResolveError::SymlinkComponent` says "path component" at `src-tauri/src/logging/state_dir.rs:11`, but the implementation only checks the final `logs` path with `symlink_metadata` at `src-tauri/src/logging/state_dir.rs:38-43`. The canonical escape check at `src-tauri/src/logging/state_dir.rs:45-52` prevents obvious writes outside the resolved state root, so I did not classify this as a proven bug in this hunt. The contract and implementation are still narrower/different enough that a future security-focused pass should decide whether every path component really must reject symlinks.
