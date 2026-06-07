# Bug Hunt Report

## Scope

Analyzed the runtime startup path for the alpha diagnostic logging pipeline in `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-xvqy-logging-reactor-panic`.

High-risk entry points chosen from the file listing before reading source:

- `src-tauri/src/lib.rs` because it owns the synchronous Tauri `.setup(...)` orchestration.
- `src-tauri/src/logging/mod.rs` because `logging::init()` is the single owner that wires state-dir resolution, subscriber install, disk logging, UI logging, free-disk guard, and bounded timers.
- `src-tauri/src/logging/free_disk_guard.rs`, `disk_consumer.rs`, `ui_consumer.rs`, and `bounded_timer.rs` because they spawn long-lived async work from init.

Threads followed for context:

- `src-tauri/src/main.rs` to confirm the app enters `tuxlink_lib::run()`.
- `src-tauri/src/bootstrap.rs` and `src-tauri/src/position/gpsd.rs` as adjacent startup async patterns; both avoid bare `tokio::spawn` for app-start Tauri tasks.
- `src-tauri/src/logging/env_probes/mod.rs`, `commands.rs`, `export.rs`, `logging_handle.rs`, `subscriber.rs`, `fanout.rs`, `settings.rs`, and `state_dir.rs` for adjacent logging runtime boundaries and init state.

## Bugs

### Logging startup spawns Tokio tasks without entering a Tokio runtime

**Location:** `src-tauri/src/logging/free_disk_guard.rs:21`

**Severity:** critical

**Evidence:** `src-tauri/src/main.rs:4-5` enters `tuxlink_lib::run()`. `src-tauri/src/lib.rs:160-170` runs a synchronous Tauri `.setup(|app| ...)` closure and calls `crate::logging::init(session_log)` directly. Inside that synchronous initializer, `src-tauri/src/logging/mod.rs:94-95` constructs logging state and immediately calls `free_disk_guard::FreeDiskGuard::spawn(log_dir.clone())`. `FreeDiskGuard::spawn` then calls bare `tokio::spawn(async move { ... })` at `src-tauri/src/logging/free_disk_guard.rs:21`.

Bare `tokio::spawn` requires a current Tokio runtime on the calling thread. The reported launch panic names exactly this line and message: "there is no reactor running, must be called from the context of a Tokio 1.x runtime". The source path shows no `async` boundary, `tokio::runtime::Handle`, or `tauri::async_runtime::spawn` before that call. The adjacent app-start GPS path uses the Tauri runtime explicitly at `src-tauri/src/position/gpsd.rs:120-126`, which is the contrasting pattern available in this codebase.

This is not isolated to the first failing line. If `free_disk_guard.rs:21` were bypassed or fixed alone, the same `logging::init()` call path would next reach additional bare Tokio spawns from the same synchronous setup flow:

- `src-tauri/src/logging/mod.rs:106-113` calls `disk_consumer::spawn(...)`, which calls bare `tokio::spawn` at `src-tauri/src/logging/disk_consumer.rs:75` and `src-tauri/src/logging/disk_consumer.rs:102`.
- `src-tauri/src/logging/mod.rs:126-127` calls `ui_consumer::spawn(...)`, which calls bare `tokio::spawn` at `src-tauri/src/logging/ui_consumer.rs:17`.
- `src-tauri/src/logging/mod.rs:166-167` calls `bounded_timer::schedule_revert(handle_arc.clone())`; when persisted settings are `DetailedMode::Bounded`, `src-tauri/src/logging/bounded_timer.rs:24-32` does not return early and calls bare `tokio::spawn` at `src-tauri/src/logging/bounded_timer.rs:48`.

**Impact:** The app can compile successfully and then panic during launch before setup completes, preventing the GUI from starting. Fixing only the observed free-disk guard line would leave later startup crashes in the disk consumer, UI consumer, and persisted-bounded timer paths.

## Design Concerns

The logging module mixes Tauri app-start runtime boundaries with direct Tokio APIs. `src-tauri/src/position/gpsd.rs:120-126` documents and uses `tauri::async_runtime::spawn` for a setup-started task, while logging startup uses bare Tokio spawns at `free_disk_guard.rs:21`, `disk_consumer.rs:75`, `disk_consumer.rs:102`, `ui_consumer.rs:17`, and conditionally `bounded_timer.rs:48`. That split makes future startup work fragile because code review has to infer whether a caller is currently inside a Tokio runtime.

The first-paint probe runner has the same shape after launch: `src-tauri/src/logging/env_probes/mod.rs:199-201` registers a synchronous `app.listen("first_paint_complete", ...)` callback and calls bare `tokio::spawn` inside it. The event is emitted by the sync Tauri command at `src-tauri/src/logging/commands.rs:397-400`, registered in the invoke handler at `src-tauri/src/lib.rs:478-488`, and called from the frontend after first render at `src/App.tsx:57-63`. I did not classify this as a separate proven bug because the reported panic proves the setup thread lacks a Tokio reactor, not the listener callback thread. It is still the same risky runtime-boundary pattern and should be audited with the startup fix.
