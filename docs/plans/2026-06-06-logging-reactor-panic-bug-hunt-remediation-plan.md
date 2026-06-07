# Logging Reactor Panic Remediation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore app launch by removing bare `tokio::spawn` calls from logging code reached by Tauri setup/listener contexts, and prevent logging from reporting `Full` when the global subscriber failed to install.

**Architecture:** Keep the fix local to alpha logging. Startup/listener helpers will spawn async work through `tauri::async_runtime::spawn`, matching existing app-start patterns in `src-tauri/src/bootstrap.rs:236` and `src-tauri/src/position/gpsd.rs:126`. Add regression coverage that fails on the current runtime-boundary pattern and does not rely only on `#[tokio::test]`.

**Tech Stack:** Rust, Tauri 2, Tokio 1, tracing/tracing-subscriber, cargo integration tests.

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

---

## Execution Status

**Overall:** 3/3 phases shipped in `1014bad` on 2026-06-07.

| Phase | Status | Ship SHA(s) | Notes |
|---|---|---|---|
| 1 — Regression Tests | ✅ Shipped | `1014bad` | Added sync-context regression tests plus source guards for logging startup/listener spawns. |
| 2 — Runtime-Boundary Fix | ✅ Shipped | `1014bad` | Replaced logging startup/listener bare `tokio::spawn` calls with `tauri::async_runtime::spawn`. |
| 3 — Subscriber Install Truthfulness | ✅ Shipped | `1014bad` | `logging::init()` now returns degraded when the global subscriber cannot be installed. |

### Verification Evidence

- `cargo test --manifest-path src-tauri/Cargo.toml --test logging_runtime_boundary_test --locked -- --nocapture` passed; before the fix, this same test failed with the operator-reported no-reactor panic and source-guard failures.
- `grep -R "tokio::spawn" -n src-tauri/src/logging` returned no matches.
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings` passed.
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --verbose` passed outside the sandbox after the sandboxed run failed on local socket permissions.
- `pnpm tauri dev` on branch `bd-tuxlink-xvqy/logging-reactor-panic` built and launched `/target/debug/tuxlink`; the app stayed up through the critical startup dwell with no `there is no reactor running` panic, then exited via the `timeout` wrapper.
- `pnpm typecheck`, `pnpm vitest run` (133 files / 1455 tests), `pnpm build`, and `pnpm lint:docs` passed. The docs lint required an unsandboxed rerun because `tsx` could not create its `/tmp` IPC pipe in the sandbox.
- `git diff --check` passed.

### Deviations

- The final launch smoke used direct branch launch (`pnpm tauri dev`) instead of `pnpm dev:converged` because `scripts/converge-build.sh` validates the detached `origin/main` convergence worktree, which would not contain this unmerged branch fix. The launch-smoke intent was preserved: build and run the fixed branch through Tauri startup and verify the original startup panic is gone.

### Deferred Findings

- B3 disk-consumer partial-init no-consumer path, B4 retention stale config, B5 unreadable export files, B6 active log deletion under lock contention, B8 deletion count accuracy, B9 export manifest size drift, and D1 state-dir symlink policy are documented in `docs/bug-hunts/2026-06-06-logging-reactor-panic-consolidated.md`. They are not part of this P/S0 launch fix unless explicitly promoted later.
- Existing follow-up issues: `tuxlink-fuog` for B4 and `tuxlink-xkal` for B9.

### Plan Review Log

- Round 1: 6 findings fixed.
  - Clarified that the first-paint probe regression is enforced by source scan plus final launch smoke, not a full Tauri `AppHandle` fixture.
  - Removed confusing `set_default` setup from the subscriber-install source guard.
  - Added an explicit bounded run/termination instruction for `pnpm dev:converged`.
  - Added a branch-claim requirement before implementation begins.
  - Clarified that lower-priority export/retention bugs stay deferred unless promoted in a later plan.
  - Added explicit no-live-radio boundary to test and implementation tasks.
- Round 2: 3 findings fixed.
  - Added explicit instruction to update phase banners when implementation starts.
  - Clarified that launch-smoke verification is required before user-facing "fixed" claims even if broader gates are still running or fail for unrelated reasons.
  - Clarified that frontend gates are secondary for this Rust startup regression and must not replace the launch smoke.
- Round 3: 1 finding fixed.
  - Removed duplicate launch-smoke checklist item introduced during Round 2.
- Round 4: 0 findings. Review cycle complete.

---

## Phase 1 — Regression Tests

**Execution Status:** ✅ SHIPPED — `1014bad` on 2026-06-07
(branch `bd-tuxlink-xvqy/logging-reactor-panic`)

Implementation executor must update this banner to 🚧 IN PROGRESS with branch `bd-tuxlink-xvqy/logging-reactor-panic` before editing files in this phase.

### Task 1.1: Add Runtime-Boundary Regression Tests

**Files:**
- Create: `src-tauri/tests/logging_runtime_boundary_test.rs`
- Read: `docs/pitfalls/testing-pitfalls.md`

BEFORE starting work:
1. Invoke `/superpowers:test-driven-development`.
2. Read `docs/pitfalls/testing-pitfalls.md`.
3. Follow TDD: write failing test -> implement -> verify green.

Pitfalls to apply:
- `docs/pitfalls/testing-pitfalls.md` §5: production runtime-boundary spawns must be tested from their real caller context or through an app-runtime abstraction.
- Do not weaken assertions if any runtime-boundary test is awkward. Use deterministic source assertions plus sync-context calls.
- RADIO-1 from `docs/pitfalls/implementation-pitfalls.md`: this test must not invoke Winlink/CMS/modem/RF code or any path that can transmit.

- [x] **Step 1: Create failing source-scan regression**

Add this file:

```rust
//! Regression coverage for tuxlink-xvqy:
//! logging startup/listener helpers must not call bare `tokio::spawn`.
//!
//! The original release panic happened because `FreeDiskGuard::spawn` was
//! called from synchronous Tauri setup and used bare `tokio::spawn`, which
//! requires a currently-entered Tokio reactor. Unit tests under
//! `#[tokio::test]` hide that bug, so this test scans the known startup
//! helper files for the forbidden spawn form. The first-paint probe runner is
//! covered by this source scan because constructing a full Tauri `AppHandle`
//! fixture is not needed for the P/S0 patch; final launch smoke covers the
//! production app path.

const STARTUP_LOGGING_FILES: &[&str] = &[
    "src/logging/free_disk_guard.rs",
    "src/logging/disk_consumer.rs",
    "src/logging/ui_consumer.rs",
    "src/logging/bounded_timer.rs",
    "src/logging/env_probes/mod.rs",
];

#[test]
fn logging_startup_helpers_do_not_call_bare_tokio_spawn() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for relative in STARTUP_LOGGING_FILES {
        let path = manifest_dir.join(relative);
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        assert!(
            !src.contains("tokio::spawn"),
            "{} must spawn through tauri::async_runtime::spawn or a logging-local wrapper, not bare tokio::spawn",
            relative
        );
    }
}
```

- [x] **Step 2: Add sync-context smoke tests for the startup helpers that can be constructed without a full Tauri app**

Append to the same file:

```rust
use std::sync::Arc;
use tuxlink_lib::logging::{
    bounded_timer,
    disk_consumer,
    filter_layer,
    free_disk_guard::FreeDiskGuard,
    logging_handle::LoggingHandle,
    retention::RetentionConfig,
    settings::{DetailedMode, Settings},
    ui_consumer,
};
use tuxlink_lib::session_log::SessionLogState;

fn make_logging_handle_for_revert(initial: DetailedMode) -> Arc<LoggingHandle> {
    use std::sync::atomic::AtomicBool;
    use tokio::sync::broadcast;

    let (_, filter_reload) = filter_layer::build();
    let (writer, guard) = tracing_appender::non_blocking(std::io::sink());
    drop(writer);

    Arc::new(LoggingHandle {
        _appender_guard: guard,
        session_log: Arc::new(SessionLogState::new(16)),
        broadcast_tx: {
            let (tx, _) = broadcast::channel(16);
            tx
        },
        log_dir: std::env::temp_dir(),
        active_file_path: Arc::new(tokio::sync::Mutex::new(None)),
        boot_id: "test-boot".to_string(),
        boot_at: "2026-06-06T00:00:00.000Z".to_string(),
        settings: Arc::new(std::sync::Mutex::new(Settings {
            detailed_mode: initial,
            retention_days: 14,
            retention_mb_cap: 500,
        })),
        filter_reload,
        free_disk_paused: Arc::new(AtomicBool::new(false)),
        revert_cancel: Arc::new(std::sync::Mutex::new(None)),
        probe_listener_id: std::sync::Mutex::new(None),
        flush_barrier: {
            let (barrier, _rx) = tuxlink_lib::logging::export::FlushBarrier::new();
            barrier
        },
    })
}

#[test]
fn free_disk_guard_spawn_does_not_require_current_tokio_reactor() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let _guard = FreeDiskGuard::spawn(tmp.path().to_path_buf());
}

#[test]
fn disk_consumer_spawn_does_not_require_current_tokio_reactor() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let log_dir = tmp.path().join("logs");
    std::fs::create_dir_all(&log_dir).expect("create log dir");
    let (_tx, rx) = tokio::sync::broadcast::channel(16);
    let active_file_path = Arc::new(tokio::sync::Mutex::new(None));
    let paused = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (_barrier, flush_rx) = tuxlink_lib::logging::export::FlushBarrier::new();

    let _guard = disk_consumer::spawn(
        rx,
        log_dir,
        active_file_path,
        paused,
        RetentionConfig { days: 14, mb_cap: 500 },
        flush_rx,
    )
    .expect("disk consumer appender should initialize");
}

#[test]
fn ui_consumer_spawn_does_not_require_current_tokio_reactor() {
    let (_tx, rx) = tokio::sync::broadcast::channel(16);
    ui_consumer::spawn(rx, Arc::new(SessionLogState::new(16)));
}

#[test]
fn bounded_timer_spawn_does_not_require_current_tokio_reactor() {
    let expires_at = chrono::Utc::now() + chrono::Duration::hours(1);
    let handle = make_logging_handle_for_revert(DetailedMode::Bounded { expires_at });
    bounded_timer::schedule_revert(handle);
}
```

- [x] **Step 3: Run the new test and verify it fails before implementation**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test logging_runtime_boundary_test --locked -- --nocapture
```

Expected before Phase 2: failure showing at least `free_disk_guard.rs` contains `tokio::spawn`, or a sync-context panic with "there is no reactor running".

BEFORE marking this task complete:
1. Review tests against `docs/pitfalls/testing-pitfalls.md`.
2. Verify the tests cover setup/listener runtime-boundary risk rather than only happy-path Tokio contexts.
3. Do not remove the source-scan assertion to make the runtime tests pass.

After completing this group:
Review the batch from at least three perspectives: release-launch regression, masked follow-on spawns, and test rigor under pressure. If round 3 still finds issues, keep going until clean.

---

## Phase 2 — Runtime-Boundary Fix

**Execution Status:** ✅ SHIPPED — `1014bad` on 2026-06-07
(branch `bd-tuxlink-xvqy/logging-reactor-panic`)

Implementation executor must update this banner to 🚧 IN PROGRESS with branch `bd-tuxlink-xvqy/logging-reactor-panic` before editing files in this phase.

### Task 2.1: Spawn Logging Startup/Listener Workers Through the Tauri Runtime

**Files:**
- Modify: `src-tauri/src/logging/free_disk_guard.rs:21`
- Modify: `src-tauri/src/logging/disk_consumer.rs:75`
- Modify: `src-tauri/src/logging/disk_consumer.rs:102`
- Modify: `src-tauri/src/logging/ui_consumer.rs:17`
- Modify: `src-tauri/src/logging/bounded_timer.rs:48`
- Modify: `src-tauri/src/logging/env_probes/mod.rs:201`
- Test: `src-tauri/tests/logging_runtime_boundary_test.rs`

BEFORE starting work:
1. Invoke `/superpowers:test-driven-development`.
2. Read `docs/pitfalls/testing-pitfalls.md`.
3. Follow TDD: write failing test -> implement -> verify green.

Current behavior:
- Logging helpers called from Tauri setup/listener contexts call bare `tokio::spawn`, which requires a current Tokio reactor and panics before app launch completes.

Desired behavior:
- The same helpers use `tauri::async_runtime::spawn`, matching `src-tauri/src/bootstrap.rs:236` and `src-tauri/src/position/gpsd.rs:126`.
- Do NOT introduce a new runtime.
- Do NOT block Tauri setup with `block_on`.
- Do NOT move logging init into a background thread for this P/S0 fix.
- Do NOT touch live radio / Winlink transmission code.
- Do NOT include B4-B6/B8-B9 export or retention cleanup in this P/S0 patch; those are deferred unless a later plan promotes them.

- [x] **Step 1: Replace the six forbidden spawn calls**

Make these exact mechanical replacements:

```rust
// before
tokio::spawn(async move {
    // existing body unchanged
});

// after
tauri::async_runtime::spawn(async move {
    // existing body unchanged
});
```

Apply in:
- `src-tauri/src/logging/free_disk_guard.rs`
- `src-tauri/src/logging/disk_consumer.rs` both spawn sites
- `src-tauri/src/logging/ui_consumer.rs`
- `src-tauri/src/logging/bounded_timer.rs`
- `src-tauri/src/logging/env_probes/mod.rs`

- [x] **Step 2: Run the focused regression**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test logging_runtime_boundary_test --locked -- --nocapture
```

Expected after implementation: all tests pass; source scan finds no `tokio::spawn` in the listed startup helper files.

- [x] **Step 3: Search for remaining forbidden startup spawns**

Run:

```bash
grep -R "tokio::spawn" -n src-tauri/src/logging
```

Expected: no matches. If a match remains in logging, either fix it or document why it is not reachable from Tauri setup/listener/command contexts before proceeding.

BEFORE marking this task complete:
1. Review tests against `docs/pitfalls/testing-pitfalls.md`.
2. Verify no assertion was weakened.
3. Run the focused test and confirm green.

---

## Phase 3 — Subscriber Install Truthfulness

**Execution Status:** ✅ SHIPPED — `1014bad` on 2026-06-07
(branch `bd-tuxlink-xvqy/logging-reactor-panic`)

Implementation executor must update this banner to 🚧 IN PROGRESS with branch `bd-tuxlink-xvqy/logging-reactor-panic` before editing files in this phase.

### Task 3.1: Make `logging::init()` Degrade When the Global Subscriber Cannot Be Installed

**Files:**
- Modify: `src-tauri/src/logging/mod.rs:91-93`
- Test: `src-tauri/tests/logging_runtime_boundary_test.rs`

BEFORE starting work:
1. Invoke `/superpowers:test-driven-development`.
2. Read `docs/pitfalls/testing-pitfalls.md`.
3. Follow TDD: write failing test -> implement -> verify green.

Current behavior:
- `logging::init()` ignores `tracing::subscriber::set_global_default(subscriber)` failure at `src-tauri/src/logging/mod.rs:92`.
- It can later return `InitOutcome::Full`, even though tracing events will not reach the logging fanout.

Desired behavior:
- If `set_global_default` fails, return `InitOutcome::Degraded { reason }` with a clear reason string.
- Do not spawn disk/UI/free-disk/bounded logging workers after subscriber install failure.
- Do not panic.

- [x] **Step 1: Add a regression test for subscriber install failure**

Append to `src-tauri/tests/logging_runtime_boundary_test.rs`:

```rust
#[test]
fn logging_init_degrades_when_global_subscriber_is_already_set() {
    // Do not mutate the process-global subscriber in a shared Rust test binary;
    // it is one-way and can contaminate sibling tests. This source-level guard
    // catches the regression shape without installing a global subscriber.
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src = std::fs::read_to_string(manifest_dir.join("src/logging/mod.rs"))
        .expect("read logging/mod.rs");
    assert!(
        src.contains("tracing::subscriber::set_global_default(subscriber)") &&
            src.contains("InitOutcome::Degraded") &&
            !src.contains("let _ = tracing::subscriber::set_global_default(subscriber);"),
        "logging::init must handle set_global_default failure explicitly"
    );
}
```

Note: do not use process-global subscriber mutation in a shared Rust test binary; it is one-way and can contaminate sibling tests. The source guard is intentional here.

- [x] **Step 2: Implement explicit error handling**

Change `src-tauri/src/logging/mod.rs` from:

```rust
let (subscriber, handles) = subscriber::build(session_log.clone());
let _ = tracing::subscriber::set_global_default(subscriber);
```

to:

```rust
let (subscriber, handles) = subscriber::build(session_log.clone());
if let Err(e) = tracing::subscriber::set_global_default(subscriber) {
    return InitOutcome::Degraded {
        reason: format!("logging subscriber install failed: {e}"),
    };
}
```

- [x] **Step 3: Run focused tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test logging_runtime_boundary_test --locked -- --nocapture
```

Expected: all tests pass.

BEFORE marking this task complete:
1. Review error-path behavior against `docs/pitfalls/testing-pitfalls.md` §3.
2. Confirm `logging::init()` no longer reports `Full` after subscriber install failure.
3. Confirm no logging worker is spawned before the subscriber install result is checked.

After completing this group:
Review the batch from at least three perspectives: fail-soft startup behavior, process-global subscriber side effects in tests, and launch regression risk.

---

## Final Verification

- [x] Run focused Rust test:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test logging_runtime_boundary_test --locked -- --nocapture
```

- [x] Run broader Rust test gate:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --locked --verbose
```

- [x] Run clippy:

```bash
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
```

- [x] Run frontend gates if Rust gates pass:

```bash
pnpm typecheck
pnpm vitest run
pnpm build
pnpm lint:docs
```

- [x] Reproduce the operator launch failure path before making any user-facing "fixed" claim. This launch smoke is required for this P/S0 incident even if frontend gates are still pending or fail for unrelated reasons:

```bash
timeout 600s pnpm tauri dev
```

Expected: no panic with "there is no reactor running"; app reaches GUI startup. The launched branch app stayed up through the critical dwell and was stopped by the `timeout` wrapper so no dev process remained running.

## Execution Strategy Recommendation

Recommended execution: inline in this session after plan review. The P/S0 launch break is tightly coupled, touches six small logging spawn sites plus one logging init error branch, and needs one coherent verification run. Parallelizing this would add merge/review overhead without reducing risk.
