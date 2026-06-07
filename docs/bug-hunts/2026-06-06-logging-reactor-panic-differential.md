# Bug Hunt Report - Differential

## Scope

Analyzed the alpha diagnostic logging startup path and adjacent async-startup patterns in source files only. Primary files examined:

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

Adjacent source context used for relationship comparison:

- `src-tauri/src/bootstrap.rs`
- `src-tauri/src/position/gpsd.rs`
- `src-tauri/src/logging/commands.rs`
- `src-tauri/src/logging/event.rs`
- `src-tauri/src/logging/fanout.rs`

The strongest differential structure in this scope is not a data round-trip; it is a runtime-boundary invariant between Tauri setup/event producers and the async task spawners they call.

## Relationships Examined

- **Tauri setup -> logging init -> startup task spawners:** Code reached from `tauri::Builder::setup` must not require a currently-entered Tokio reactor unless the caller enters one first; app-start async work should be spawned onto Tauri's global async runtime or an equivalent explicit runtime. **Violated.**
- **Tauri first-paint event listener -> env probe task:** A synchronous Tauri event listener that starts async work must use an executor available from that event callback, not a bare `tokio::spawn` that requires the callback thread to be inside a Tokio runtime. **Violated.**
- **Fanout `LoggedEvent` producer -> disk JSONL writer -> export reader:** Events emitted by `FanoutLayer` and rendered by `LoggedEvent::to_jsonl()` must be deserializable by `export::build_archive()` as `LoggedEvent`. Evidence: `LoggedEvent` derives both `Serialize` and `Deserialize` at `src-tauri/src/logging/event.rs:13`; `to_jsonl()` is at `src-tauri/src/logging/event.rs:62`; disk writes that exact representation at `src-tauri/src/logging/disk_consumer.rs:120` and `src-tauri/src/logging/disk_consumer.rs:181`; export reads with `serde_json::from_str::<LoggedEvent>` at `src-tauri/src/logging/export.rs:136` and re-renders with `to_jsonl()` at `src-tauri/src/logging/export.rs:149`. **Held.**
- **Export flush barrier -> disk consumer ack:** `build_archive()` must not read log files until the disk consumer has drained and flushed events that are available to its receiver. Evidence: export calls `flush_and_wait()` at `src-tauri/src/logging/export.rs:114`; disk consumer receives the ack request at `src-tauri/src/logging/disk_consumer.rs:113`, drains `rx.try_recv()` at `src-tauri/src/logging/disk_consumer.rs:118`, flushes at `src-tauri/src/logging/disk_consumer.rs:124`, then acks at `src-tauri/src/logging/disk_consumer.rs:126`. No source-proven disagreement found in this hunt. **Held for the inspected producer/consumer pair.**
- **FreeDiskGuard pause producer -> disk consumer pause consumer:** The guard and disk consumer must agree that the shared `AtomicBool` means "disk writes are paused." Evidence: `logging::init()` passes `free_disk_guard.paused.clone()` into `disk_consumer::spawn()` at `src-tauri/src/logging/mod.rs:110`; free-space low sets the flag at `src-tauri/src/logging/free_disk_guard.rs:31`, recovery clears it at `src-tauri/src/logging/free_disk_guard.rs:37`, and disk writes check it at `src-tauri/src/logging/disk_consumer.rs:119` and `src-tauri/src/logging/disk_consumer.rs:132`. **Held.**

## Bugs

### Startup logging task spawners require a Tokio reactor that Tauri setup does not provide

**Location:** `src-tauri/src/lib.rs:160` / `src-tauri/src/lib.rs:170` (setup caller) and `src-tauri/src/logging/free_disk_guard.rs:21`, `src-tauri/src/logging/disk_consumer.rs:75`, `src-tauri/src/logging/disk_consumer.rs:102`, `src-tauri/src/logging/ui_consumer.rs:17`, `src-tauri/src/logging/bounded_timer.rs:48` (callee-side spawns)

**Severity:** critical

**Invariant violated:** Startup code reached from `tauri::Builder::setup` must use an executor available from that setup context. In this repo, adjacent app-start async work uses `tauri::async_runtime::spawn`, not bare `tokio::spawn`, when spawned from Tauri lifecycle code.

**Evidence:** `src-tauri/src/lib.rs:160` opens the synchronous `.setup(|app| { ... })` closure, and `src-tauri/src/lib.rs:170` calls `crate::logging::init(session_log)` from inside it. `logging::init()` immediately wires async logging helpers: `FreeDiskGuard::spawn` at `src-tauri/src/logging/mod.rs:95`, `disk_consumer::spawn` at `src-tauri/src/logging/mod.rs:106`, `ui_consumer::spawn` at `src-tauri/src/logging/mod.rs:127`, and `bounded_timer::schedule_revert` at `src-tauri/src/logging/mod.rs:167`.

The helpers disagree with that caller contract. `FreeDiskGuard::spawn()` calls bare `tokio::spawn` at `src-tauri/src/logging/free_disk_guard.rs:21`, which matches the reported panic location. If only that first line is fixed, the same startup path still reaches bare `tokio::spawn` in `disk_consumer::spawn()` at `src-tauri/src/logging/disk_consumer.rs:75` and `src-tauri/src/logging/disk_consumer.rs:102`, then in `ui_consumer::spawn()` at `src-tauri/src/logging/ui_consumer.rs:17`. The bounded timer is conditional, but persisted `DetailedMode::Bounded` makes `schedule_revert()` spawn at `src-tauri/src/logging/bounded_timer.rs:31` and `src-tauri/src/logging/bounded_timer.rs:48` from the same setup-initiated `logging::init()` path.

The adjacent startup code shows the expected side of the invariant: `src-tauri/src/position/gpsd.rs:120` documents spawning onto "Tauri's global async runtime (valid post-`.setup()`)" and `src-tauri/src/position/gpsd.rs:126` uses `tauri::async_runtime::spawn`; `src-tauri/src/bootstrap.rs:236` also uses `tauri::async_runtime::spawn` for app-start async work.

Git blame supports assigning the mismatch to the logging startup wiring: the setup call to `logging::init()` is from `93e77e92`, the first failing `FreeDiskGuard::spawn()` line is from `7effa301`, the disk-consumer bare spawns are from `ac67c505` / `aa75c746`, the UI consumer bare spawn is from `28b2383f`, and the bounded timer bare spawn is from `80fc8bb9`.

**Impact:** The app can compile successfully and then panic during startup before the UI is usable. The first observed failure is `FreeDiskGuard::spawn()` at line 21, but the invariant violation is pipeline-wide: several follow-on startup spawners would hit the same "there is no reactor running" failure after the first one is corrected.

### First-paint env probe runner repeats the same runtime-boundary mismatch

**Location:** `src-tauri/src/lib.rs:176` (setup registers the runner), `src-tauri/src/logging/env_probes/mod.rs:199` (Tauri event listener), and `src-tauri/src/logging/env_probes/mod.rs:201` (callee-side spawn)

**Severity:** significant

**Invariant violated:** A Tauri event listener that starts async work must spawn through a runtime that is valid from that listener callback. It must not assume the callback thread has a currently-entered Tokio reactor.

**Evidence:** After successful logging init, `src-tauri/src/lib.rs:176` calls `crate::logging::env_probes::spawn_runner(...)`. `spawn_runner()` registers a synchronous Tauri listener for `first_paint_complete` at `src-tauri/src/logging/env_probes/mod.rs:199`. Inside that listener callback, it calls bare `tokio::spawn` at `src-tauri/src/logging/env_probes/mod.rs:201`.

That disagrees with the same established app-start executor pattern cited above: `src-tauri/src/position/gpsd.rs:120` / `src-tauri/src/position/gpsd.rs:126` and `src-tauri/src/bootstrap.rs:236` use `tauri::async_runtime::spawn` for Tauri lifecycle-started async work. Unlike the immediate `FreeDiskGuard` panic, this path waits until the frontend emits `first_paint_complete`, but the runtime precondition gap is the same.

Git blame shows both sides arrived in the alpha logging series: the setup registration at `src-tauri/src/lib.rs:176` is from `93e77e92`, and the listener/spawn body at `src-tauri/src/logging/env_probes/mod.rs:199` / `src-tauri/src/logging/env_probes/mod.rs:201` is from `b835ac93` / `93e77e92`.

**Impact:** Even after the startup logging spawners are corrected, the first-paint diagnostic probe path can still panic when the frontend sends `first_paint_complete`, disrupting the alpha diagnostic logging pipeline just after launch.

## Design Concerns

The logging helper APIs hide their executor requirement. `FreeDiskGuard::spawn`, `disk_consumer::spawn`, `ui_consumer::spawn`, `bounded_timer::schedule_revert`, and `env_probes::spawn_runner` all look like ordinary synchronous setup helpers, but several require an async executor to be valid at the call site. That is the fragile relationship behind both findings: the caller cannot tell from the type signature whether it must already be inside Tokio, pass an explicit spawner, or rely on Tauri's global runtime.
