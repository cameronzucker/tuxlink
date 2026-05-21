# PatBackend::spawn + app-start bootstrap — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make a CMS-configured tuxlink launch spawn the Pat sidecar, surface live mailbox + session-log data, and present a Pat-spawn *failure* as an explicit error — not a silent empty state.

**Architecture:** A background-thread app-start bootstrap (`lib.rs` `.setup()`) reads config, and for `wizard_completed && connect_to_cms` spawns Pat via `PatBackend::spawn`, installing it into a single `BackendState` managed-state (phase + optional backend). `PatBackend::spawn` wraps `PatProcess::spawn`, bridges Pat's stderr into a durable `SessionLogState` ring buffer (monotonic `seq`) and a live `broadcast` notification, and holds/supervises the `PatProcess`. The already-polling frontend (`backend_status` @2s, `mailbox_list` @10s, `session_log:line` listener) converges with no new event.

**Tech Stack:** Rust (Tauri 2, tokio, async-trait, thiserror), TypeScript/React frontend, `cargo test` + `vitest`.

**Canonical design:** [`docs/superpowers/specs/2026-05-20-pat-spawn-bootstrap-design.md`](../superpowers/specs/2026-05-20-pat-spawn-bootstrap-design.md). Read it first — §3 (architecture), §10 (Codex adrev dispositions), §11 (scope decomposition this plan implements). bd: `tuxlink-22l` (umbrella) depends on `tuxlink-xx3` (Task A) + `tuxlink-xyd` (Task B); `tuxlink-pqg` is a separate follow-up (out of scope).

---

## ⚠️ Part 97 (RADIO-1) — non-negotiable

This code can lead to a CMS session. **WRITE + COMMIT the code; the licensee RUNS any live-CMS path.** No agent/subagent/CI runs a live-CMS binary to "verify completion." All automated tests use Pat in `http` mode only (binds local HTTP; does **not** connect/send to CMS) or pure unit logic. The live CMS round-trip (spec acceptance point 4) is **operator-run** and is OUT OF SCOPE for this plan. If a task seems to require running a live-CMS binary to pass, it is misspecified — STOP and escalate.

## TDD preamble — applies to EVERY task

```
BEFORE starting work (fresh subagent = zero conversation history — do ALL of these):
0. READ the canonical spec docs/superpowers/specs/2026-05-20-pat-spawn-bootstrap-design.md
   — at minimum §3 (architecture), §10 (adrev dispositions), §11, AND every spec section
   this task cites. The plan's implementation steps reference the spec for full design;
   you cannot implement correctly without it.
1. Invoke superpowers:test-driven-development (or read .claude/skills/test-driven-development/).
2. Read docs/pitfalls/testing-pitfalls.md and docs/pitfalls/implementation-pitfalls.md.
3. You are agent <moniker>; put `Agent: <moniker>` + the Co-Authored-By trailer in commits.
Follow TDD: write the failing test → run it red → implement the minimal code → run green.
```

```
BEFORE marking ANY task complete:
1. Review your tests against docs/pitfalls/testing-pitfalls.md (esp. §1 pristine output,
   §7 infra hygiene incl. the tuxlink-cnd real-keyring guard, process-spawn cleanup).
2. Verify error paths + edge cases are tested, not just the happy path.
3. Run the relevant test subset and confirm green. Paste the result.
```

## Review loop — after EACH task group (A+B, then C+D, then E)

```
Review the batch from multiple perspectives (concurrency, error paths, resource cleanup,
Part 97, API drift vs the spec). Minimum THREE review rounds; if the 3rd still finds
substantive issues, keep going until clean. Then update notes and continue.
```

---

## File structure

| File | Responsibility | Tasks |
|---|---|---|
| `src-tauri/src/session_log.rs` (NEW) | `SessionLogState`: bounded ring buffer of `LogLine` with monotonic `seq`; `append`, `snapshot_since(seq)`, `snapshot()` | A |
| `src-tauri/src/winlink_backend.rs` (MOD) | `LogLine` gains `seq`; `PatBackend` gains `spawn`, `_process`, `_log_buffer`, log-bridge, `Drop` | A, C |
| `src-tauri/src/pat_process.rs` (MOD) | Enforce announce timeout (no hang); `Stdio::null()` stdout; expose pre-announce stderr lines | B |
| `src-tauri/src/app_backend.rs` → `backend_state.rs` (MOD/RENAME) | Replace `AppBackend` with `BackendState { RwLock<(BackendPhase, Option<Arc<dyn WinlinkBackend>>)> }` | D |
| `src-tauri/src/lib.rs` (MOD) | Background-thread bootstrap in `.setup()`; manage `BackendState` + `SessionLogState`; drain task via `tauri::async_runtime::spawn` | D |
| `src-tauri/src/ui_commands.rs` (MOD) | `backend_status` derives from `BackendState` snapshot; `session_log_snapshot` reads the ring buffer; `LogLineDto` gains `seq` | A, D |
| `src/session/SessionLog.tsx` + `logProjection.ts` (MOD) | Dedupe on `seq`; snapshot-then-tail from last `seq` | E |

**Sequencing (hard ordering — these share files):** A → B → C → D → E. A and C both touch `winlink_backend.rs`; A and D both touch `ui_commands.rs`; do NOT parallelize. Each task commits independently.

---

## Task A — SessionLogState ring buffer + `seq` (tuxlink-xx3)

Implements spec §11.1 (durability half) + adrev #1,2,3,4. Fixes the P0: broadcast-only loses Pat startup lines.

**Files:** Create `src-tauri/src/session_log.rs`, `src-tauri/tests/session_log_test.rs`; Modify `src-tauri/src/winlink_backend.rs` (add `seq` to `LogLine`), `src-tauri/src/lib.rs` (`pub mod session_log;`), `src-tauri/src/ui_commands.rs` (`LogLineDto` gains `seq`; `session_log_snapshot` reads the buffer).

- [ ] **A1: Failing test — ring buffer append + monotonic seq + bounded eviction.**

```rust
// src-tauri/tests/session_log_test.rs
use tuxlink_lib::session_log::SessionLogState;
use tuxlink_lib::winlink_backend::{LogLine, LogLevel, LogSource};

fn line(msg: &str) -> LogLine {
    LogLine { seq: 0, timestamp_iso: "2026-05-20T00:00:00Z".into(),
              level: LogLevel::Info, source: LogSource::Pat, message: msg.into() }
}

#[test]
fn append_assigns_monotonic_seq_starting_at_1() {
    let log = SessionLogState::new(8);
    let s1 = log.append(line("a"));
    let s2 = log.append(line("b"));
    assert_eq!((s1, s2), (1, 2), "append returns the assigned monotonic seq");
    let snap = log.snapshot();
    assert_eq!(snap.iter().map(|l| l.seq).collect::<Vec<_>>(), vec![1, 2]);
    assert_eq!(snap.iter().map(|l| l.message.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
}

#[test]
fn snapshot_since_returns_only_newer_lines() {
    let log = SessionLogState::new(8);
    for m in ["a", "b", "c"] { log.append(line(m)); }
    let since_1 = log.snapshot_since(1); // strictly-after seq 1
    assert_eq!(since_1.iter().map(|l| l.seq).collect::<Vec<_>>(), vec![2, 3]);
    assert!(log.snapshot_since(99).is_empty(), "no lines after a future cursor");
}

#[test]
fn bounded_capacity_evicts_oldest_but_seq_keeps_climbing() {
    let log = SessionLogState::new(2); // cap 2
    for m in ["a", "b", "c"] { log.append(line(m)); }
    let snap = log.snapshot();
    assert_eq!(snap.len(), 2, "ring buffer is bounded");
    assert_eq!(snap.iter().map(|l| l.seq).collect::<Vec<_>>(), vec![2, 3], "oldest evicted; seq never resets");
}
```

- [ ] **A2: Run red.** `cd src-tauri && cargo test --test session_log_test` → FAIL (module/type missing).

- [ ] **A3: Add `seq` to `LogLine`** in `winlink_backend.rs` (field `pub seq: u64`). Fix the test-only `push_log_line_for_test` callers + existing `winlink_backend_test.rs` / `ui_commands_test.rs` `LogLine { .. }` literals to include `seq` (grep `LogLine {`). The broadcast `LogLine` carries the seq assigned by the buffer (Task C sets it; for `from_url`/test pushes, `seq: 0` is fine).

- [ ] **A4: Implement `SessionLogState`.**

```rust
// src-tauri/src/session_log.rs
use std::collections::VecDeque;
use std::sync::RwLock;
use crate::winlink_backend::LogLine;

/// Durable, bounded, seq-stamped session-log history. The bridge appends here
/// (durable) AND broadcasts (live notify); `session_log_snapshot` reads here so
/// a late-mounting UI loses nothing (adrev #1,2,3). `seq` is process-monotonic
/// and never resets, so the frontend dedupes/cursors on it (adrev #4).
pub struct SessionLogState {
    inner: RwLock<Ring>,
    cap: usize,
}
struct Ring { buf: VecDeque<LogLine>, next_seq: u64 }

impl SessionLogState {
    pub fn new(cap: usize) -> Self {
        Self { inner: RwLock::new(Ring { buf: VecDeque::with_capacity(cap), next_seq: 1 }), cap }
    }
    /// Append a line, assigning + returning its monotonic seq. Poisoned lock → returns 0 (no-op).
    pub fn append(&self, mut line: LogLine) -> u64 {
        let Ok(mut g) = self.inner.write() else { return 0 };
        let seq = g.next_seq;
        g.next_seq += 1;
        line.seq = seq;
        if g.buf.len() == self.cap { g.buf.pop_front(); }
        g.buf.push_back(line);
        seq
    }
    pub fn snapshot(&self) -> Vec<LogLine> {
        self.inner.read().map(|g| g.buf.iter().cloned().collect()).unwrap_or_default()
    }
    /// Lines with seq strictly greater than `after`.
    pub fn snapshot_since(&self, after: u64) -> Vec<LogLine> {
        self.inner.read().map(|g| g.buf.iter().filter(|l| l.seq > after).cloned().collect()).unwrap_or_default()
    }
}
```

- [ ] **A5: Run green.** `cargo test --test session_log_test` → PASS. Also `pub mod session_log;` in `lib.rs` and fix any `LogLine {` literals so `cargo test` builds.

- [ ] **A6: Wire `session_log_snapshot` + `LogLineDto.seq`.** Add `seq: u64` to `LogLineDto` (`ui_commands.rs`); `From<LogLine>` copies it. Change `session_log_snapshot` to read `State<'_, SessionLogState>` and return `.snapshot()` mapped to `LogLineDto`. Add a unit test asserting the DTO carries `seq` and the snapshot reflects appended lines. (The `SessionLogState` is `.manage()`d in Task D; for the command test, construct one directly per the existing test pattern.)

- [ ] **A7: Completion check** (TDD preamble) + **Commit.**
```bash
git add src-tauri/src/session_log.rs src-tauri/tests/session_log_test.rs src-tauri/src/winlink_backend.rs src-tauri/src/ui_commands.rs src-tauri/src/lib.rs
git commit -m "feat(backend): durable SessionLogState ring buffer + seq (tuxlink-xx3)"  # + Agent/Co-Authored-By trailers
```

---

## Task B — PatProcess::spawn hardening (tuxlink-xyd)

Implements spec §11.1 (process half) + adrev #7,#18. **Pre-existing bug fixes** in `src-tauri/src/pat_process.rs`.

**Files:** Modify `src-tauri/src/pat_process.rs`; Test `src-tauri/tests/pat_process_test.rs` (exists).

- [ ] **B1: Failing test — announce timeout is actually enforced (no hang).** Spawn a fake "binary" that emits NO announce line and stays alive (e.g. a tiny shell script `sh -c 'sleep 30'` via a `PatSpawnOptions.binary` pointing at `/bin/sh` with crafted args, OR a purpose-built fixture). Assert `PatProcess::spawn` returns `Err(TimedOut)` within ~2s (use a short test-injected deadline if the 10s is hard-coded — refactor the deadline to a field/param to make it testable). Concrete approach: add `http_announce_timeout: Duration` to `PatSpawnOptions` (default 10s) so the test passes a 1s timeout.

```rust
// pat_process_test.rs (sketch — fill exact fixture per the existing test's helpers)
#[test]
fn spawn_times_out_when_no_announce_within_deadline() {
    let opts = no_announce_fixture_opts(std::time::Duration::from_secs(1)); // emits no port line, stays alive
    let start = std::time::Instant::now();
    let err = PatProcess::spawn(opts).expect_err("must time out");
    assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
    assert!(start.elapsed() < std::time::Duration::from_secs(3), "deadline must be REAL, not block forever");
}
```

- [ ] **B2: Run red.** Confirm it hangs/fails today (the blocking `read_line` ignores the deadline mid-line). Use `timeout 20 cargo test ...` so a true hang is observable as a timeout, proving the bug.

- [ ] **B3: Implement the real timeout.** Move the announce read off the calling thread: spawn a reader thread that sends each line over an `mpsc`, and use `recv_timeout(remaining_deadline)` in the spawn loop. On deadline: `child.kill()` + return `Err(TimedOut)`. Preserve the existing `log_sink` forwarding for post-announce lines AND capture pre-announce lines so Task C can seed them into the ring buffer (adrev #1). Set `cmd.stdout(Stdio::null())` (adrev #18 — was `piped()` and never read).

- [ ] **B4: Run green.** `cargo test --test pat_process_test` → PASS (timeout test + existing tests). Existing tests that spawn real Pat in `http` mode must still pass (they're Part-97-safe: `http` only).

- [ ] **B5: Completion check + Commit.** `feat(backend): enforce PatProcess announce timeout + null stdout (tuxlink-xyd)`.

> **REVIEW LOOP after A+B** (3+ rounds): concurrency (reader thread + recv_timeout races), resource cleanup (child killed on timeout, no zombie/thread leak), ring-buffer poisoning, `seq` API drift between A and downstream.

---

## Task C — PatBackend::spawn + bridge + supervised lifecycle

Implements spec §3.1/§3.2/§11.2 + adrev #5,6,8,16,17. **Files:** Modify `src-tauri/src/winlink_backend.rs`; Test `src-tauri/tests/winlink_backend_test.rs`.

- [ ] **C1: Failing test — spawn against the real Pat sidecar in `http` mode (Part-97-safe; skips if absent).** Resolve the sidecar (or honor `PAT_BINARY`); if not present/zero-byte, `eprintln!` skip + return. Assert: `PatBackend::spawn` returns `Ok`; `list_messages(Inbox).await` is `Ok` (empty mailbox); the `SessionLogState` passed in received ≥1 Pat startup line (proves the bridge + pre-announce capture); `status()` is `Disconnected` (NOT `Connected` — adrev #10).

- [ ] **C2: Run red** → FAIL (`spawn` undefined).

- [ ] **C3: Implement `PatBackend::spawn`** per spec §3.1: build `PatSpawnOptions` (ephemeral port, `log_sink: Some(tx)`), call `PatProcess::spawn`, map `io::Error → BackendError::BackendUnavailable`. Construct `PatClient` over the announced port. Add fields `_process: Option<PatProcess>`, `log_buffer: Arc<SessionLogState>`. Spawn the **bridge** `std::thread`: drain pre-announce lines first (from B3) then the `mpsc::Receiver<String>`; per line build a `LogLine` (`source: Pat`, `level: Info`, `timestamp_iso`: ingest UTC, `message` verbatim), `seq = log_buffer.append(line.clone())`, then `log_tx.send(line)` (live notify). Exit on `recv` error (Pat exited) or broadcast closed. Store the thread `JoinHandle` (adrev #17). Initial `status = Disconnected`. Implement `Drop` to `shutdown(timeout)` the process then join the bridge thread with a bounded wait (adrev #16,17).

- [ ] **C4: Run green** → `cargo test --test winlink_backend_test` PASS. Add a **non-transmit assertion** (adrev #11): a unit test (or instrument `PatProcess`) confirming the spawned argv contains `http` + `--addr 127.0.0.1:` and NEVER `connect`/`send` (Part 97 evidence).

- [ ] **C5: Completion check + Commit.** `feat(backend): PatBackend::spawn — bridge Pat stderr to durable log + broadcast, supervised lifecycle`.

---

## Task D — Bootstrap + single BackendState + three-state status + gates

Implements spec §3.3/§3.4/§3.6/§11.3 + adrev #9,12,14,15. **Files:** Modify `src-tauri/src/app_backend.rs` (→ `BackendState`), `src-tauri/src/lib.rs`, `src-tauri/src/ui_commands.rs`; Tests in the respective `*_test.rs`.

- [ ] **D1: Failing test — `backend_status` three-state mapping from one snapshot.** `BackendState` with phase `Failed { reason }` (no backend) → `backend_status` returns `Error { reason }`; `NotConfigured` → not-connected; `Ready` + backend → the backend's `status()`; `Spawning` → connecting. Assert no torn read (single lock).

- [ ] **D2: Run red** → FAIL.

- [ ] **D3: Implement `BackendState`** per spec §3.4 (`enum BackendPhase { NotConfigured, Spawning, Ready, Failed{reason}, ConfigError{reason} }`; `RwLock<(BackendPhase, Option<Arc<dyn WinlinkBackend>>)>`; methods `set_phase`, `install(backend)`, `snapshot()`). Migrate `AppBackend` consumers (`ui_commands.rs` `mailbox_list`/`message_*`/`backend_status` use `.snapshot().1` for the backend). Update `backend_status` to derive `StatusDto`/error from the snapshot. Keep the clone-Arc-drop-guard contract.

- [ ] **D4: Run green** (command + state unit tests).

- [ ] **D5: Failing test — bootstrap branch selection (no real process).** Factor the decision into a pure fn `fn bootstrap_decision(cfg: Result<Config, ConfigReadError>) -> BootstrapAction` where `BootstrapAction ∈ { NotConnected, ConfigError(String), Spawn }`. Tests: `Err(NotFound)`→NotConnected; `Err(Serde|Validation)`→ConfigError; `Ok(!wizard_completed)`→NotConnected; `Ok(wizard_completed, !connect_to_cms)`→NotConnected; `Ok(wizard_completed, connect_to_cms)`→Spawn (adrev #14,15).

- [ ] **D6: Implement `bootstrap_decision` + the `.setup()` wiring.** In `lib.rs`: `.manage(BackendState::new())` + `.manage(SessionLogState::new(500))`. In `.setup()`, clone `AppHandle`, then `tauri::async_runtime::spawn` (adrev #5) the bootstrap: compute `bootstrap_decision`; on `Spawn`, resolve the sidecar path (detect zero-byte/non-executable stub → `Failed` with a clear message, honor `PAT_BINARY`; adrev #12), compute Pat config/mbox/pid paths (spec §3.6), call `PatBackend::spawn`; `Ok` → `install` + phase `Ready` + start the drain task (`async_runtime::spawn`) that emits `session_log:line` per `stream_log()` line; `Err` → phase `Failed{reason}` + append a synthetic `LogSource::Backend` error line to the buffer + emit it. All non-fatal.

- [ ] **D7: Run green** + `cargo test` (full suite). **Do NOT** run the app against a live CMS. Verify build only.

- [ ] **D8: Completion check + Commit.** `feat(backend): app-start Pat bootstrap + single BackendState three-state (tuxlink-22l)`.

> **REVIEW LOOP after C+D** (3+ rounds): torn-read freedom on `BackendState`; bootstrap failure non-fatality (app always launches); sidecar-stub handling in dev; `async_runtime::spawn` runtime correctness; AppHandle `Send + 'static` into the task; Part 97 (no connect/send on the bootstrap path).

---

## Task E — Frontend: dedupe on `seq` + snapshot-then-tail

Implements spec §3.7/§11.4 + adrev #3,#4. **Files:** Modify `src/session/SessionLog.tsx`, `src/session/logProjection.ts`, `src/session/SessionLog.test.tsx`; the `LogLineDto` TS type gains `seq`.

- [ ] **E1: Failing test (vitest):** seeding via `session_log_snapshot` then a `session_log:line` event with an already-seen `seq` does NOT duplicate; a same-`timestampIso` line with a NEW `seq` IS appended (today's timestamp-only dedupe drops it — adrev #4). And: the tail listener filters events with `seq <=` the max seq seen at snapshot time (snapshot-then-tail, adrev #3).

- [ ] **E2: Run red.** `pnpm vitest run src/session/SessionLog.test.tsx` → FAIL.

- [ ] **E3: Implement.** Add `seq: number` to the `LogLineDto` TS type; dedupe/cursor on `seq` (not `timestampIso`); seed from snapshot, record `maxSeq`, append only events with `seq > maxSeq`.

- [ ] **E4: Run green** + `pnpm tsc --noEmit` + `pnpm vitest run`.

- [ ] **E5: Completion check + Commit.** `feat(ui): dedupe session log on seq + snapshot-then-tail (tuxlink-22l)`.

> **REVIEW LOOP after E** (3+ rounds): off-by-one on the `seq` cursor; snapshot/tail race; projection correctness.

---

## Out of scope (do NOT implement here)

- **Live CMS round-trip verification** (spec acceptance pt 4) — operator-run only.
- **`tuxlink-pqg`** — wizard test-send `:8080` vs ephemeral-port reconciliation + single-instance file lock. Separate issue; note the latent collision but don't fix here unless the bootstrap demonstrably breaks the wizard.
- **Structured Pat log-level parsing** — minimal `Info`/verbatim mapping ships first.
- **`backend:status` push event** to collapse the ≤10s mailbox-populate latency — polling suffices for v0.0.1.

## Final verification before the PR

- [ ] `cd src-tauri && cargo test` green (all non-`#[ignore]`d; real-keyring tests stay ignored).
- [ ] `pnpm vitest run && pnpm tsc --noEmit && pnpm build` green.
- [ ] Grep confirms no `connect`/`send` Pat call on the bootstrap path (Part 97).
- [ ] **A Codex adversarial round on the implementation diff** (`npx --yes @openai/codex review --base feat/v0.0.1 "..."`) per build-robust-features; disposition findings before merge.
- [ ] PR against `feat/v0.0.1`; body summarizes the design, the adrev dispositions, and the operator-run live-CMS verification still pending.
