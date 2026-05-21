# PatBackend::spawn + app-start bootstrap — design

**Date:** 2026-05-20
**bd issue:** tuxlink-22l (P2, feature)
**Author:** fen-sycamore-falcon (design), Cameron Zucker (design authority)
**Status:** Draft — pending adversarial review + operator spec review
**Supersedes the stub:** the `STUBBED in v0.0.1` block in `src-tauri/src/lib.rs` `.setup()` (lines ~77-101) and the deferred-bootstrap note in `app_backend.rs`.

## 1. Context & goal

Task 12 (tuxlink-zsm) shipped the main-UI cluster with `AppBackend` permanently
`None`: every UI command degrades to `NotConfigured`, and the populated app the
operator sees in dev is the **dev fixture** (`src/mailbox/devFixture.ts`, gated on
`import.meta.env.MODE === 'development'`), not real Winlink mail.

`PatProcess::spawn` (renders Pat config, binds an ephemeral port, launches Pat in
`http` mode, waits for the port announce, forwards Pat stderr to an optional
`mpsc::Sender<String>`) and `PatBackend` (HTTP client + `broadcast::Sender<LogLine>`
+ `stream_log()`, with `from_url` constructor and a **documented-but-unimplemented
`spawn`**) already exist. The gap is exactly two pieces:

1. **`PatBackend::spawn`** — the full-lifecycle constructor that ties `PatProcess`
   to the HTTP client and bridges Pat's stderr into the `LogLine` broadcast.
2. **App-start bootstrap** — in `lib.rs` `.setup()`: conditionally spawn Pat,
   install the backend in `AppBackend`, and let the live mailbox + session log
   flow to the already-built UI.

**Goal:** when a CMS-configured install launches, the app spawns Pat, shows live
mailbox/session data, and surfaces a spawn *failure* as an explicit error — not a
silent empty state.

## 2. The three-state model (the design correction)

The prior framing conflated two different situations under "graceful `None`". They
are distinct and the UI must distinguish them:

| State | When | Surface |
|---|---|---|
| **Not connected** | pre-wizard (no config) OR offline mode (`connect_to_cms = false`; no Pat spawned) | today's empty "not connected" state |
| **Backend error** | CMS configured, but Pat spawn/health failed | **explicit error + reason** in the ribbon status + session-log pane |
| **Connected** | CMS configured, Pat healthy | live mail |

Pat is a **core runtime dependency in CMS mode** — its failure is an error to be
shown loudly, not a benign absence. Offline mode genuinely needs no Pat, so `None`
remains correct there.

## 3. Architecture

### 3.1 `PatBackend::spawn`

```rust
pub struct PatBackendSpawnOptions {
    pub binary: PathBuf,              // resolved Pat sidecar path
    pub config_path: PathBuf,         // Pat config.json render destination
    pub mbox_dir: PathBuf,            // Pat mailbox dir
    pub pid_file: PathBuf,            // Pat pid file
    pub tuxlink_config: crate::config::Config,
}

impl PatBackend {
    pub fn spawn(opts: PatBackendSpawnOptions) -> Result<Self, BackendError> { ... }
}
```

Responsibilities, in order:

1. Create a `std::sync::mpsc::channel::<String>()` for Pat stderr lines.
2. Build `PatSpawnOptions { http_listen_port: 0 (ephemeral), log_sink: Some(tx), .. }`
   from `opts` and call `PatProcess::spawn(...)`. Map `io::Error` →
   `BackendError::BackendUnavailable { reason, source }` (binary-missing, announce
   timeout, config render failure all surface here).
3. Read `process.http_port()`; construct `PatClient::new(format!("http://127.0.0.1:{port}"))`.
4. **Spawn the log-bridge thread** (§3.2) consuming the `mpsc::Receiver<String>` and
   re-broadcasting `LogLine`s on the existing `broadcast::Sender<LogLine>`.
5. **Hold the `PatProcess`** in a new field so the process's lifetime is the
   backend's. NOTE: `PatProcess::Drop` today does an *immediate* `child.kill()`
   (SIGKILL) — graceful SIGTERM lives only in `shutdown(timeout)` (adrev #16).
   `PatBackend` should implement `Drop` to call `process.shutdown(timeout)` for a
   graceful stop, falling back to kill. `from_url` keeps this field `None`.
6. Mint `backend_id`; set initial `status` to **`Disconnected`** (a "backend ready"
   shape — Pat's HTTP server is up, but NO CMS connection exists yet; `Connected`
   would be a false claim of a link, see adrev #10 in §10). A real `Connected`
   status is only minted by an operator-triggered `connect()`.

New field on `PatBackend`: `_process: Option<crate::pat_process::PatProcess>`
(`None` for `from_url`, `Some` for `spawn`). `PatProcess` is `Send`; `PatBackend`
stays `Send + Sync` (held as `Arc<dyn WinlinkBackend>`).

### 3.2 The stderr → `LogLine` bridge

`PatProcess::spawn` already forwards Pat stderr lines into the `mpsc::Sender<String>`
on a dedicated OS thread (one `send` per line). `PatBackend::spawn` adds a second
dedicated `std::thread` (consistent with `PatProcess`'s blocking-`mpsc` pattern,
not a tokio task — `mpsc::Receiver::recv` is blocking) that:

- `recv()`s each `String`, maps it to a `LogLine`, and `broadcast::Sender::send`s it.
- Exits cleanly when the `mpsc` sender side closes (Pat exited) **or** the broadcast
  channel is dropped.

**Line → `LogLine` mapping (v0.0.1, deliberately minimal):**
- `timestamp_iso`: ingestion time (UTC ISO-8601). We do not parse Pat's own
  timestamps in v0.0.1.
- `level`: `LogLevel::Info` for all Pat lines. (Pat's stderr has no machine-readable
  level prefix we rely on; structured parsing is a follow-up.)
- `source`: `LogSource::Pat`.
- `message`: the line verbatim (already newline-trimmed by `PatProcess`).

Synthetic backend lines (e.g. "Pat HTTP server ready on port N", "Pat failed to
start: …") use `source: LogSource::Backend`.

### 3.3 App-start bootstrap (`lib.rs` `.setup()`)

Runs **off the main thread** so the window paints immediately (the webview must
not block on Pat's up-to-10s announce). Use `tauri::async_runtime::spawn` (or a
`std::thread` that owns work and only re-enters Tauri via a cloned `AppHandle`) —
NOT a raw `tokio::spawn` from a bare `std::thread` (adrev #5: no runtime in scope).
Move a cloned `AppHandle` into the task (adrev #6). Sequence:

1. `crate::config::read_config()` — classify the result (adrev #15):
   - `Err(NotFound)` (pre-wizard) → leave `None` (**Not connected**). Done.
   - `Err(Serde | Validation | Io)` → **Backend error** (`Failed { reason }`); the
     config exists but is unusable — surface it, don't masquerade as not-connected.
   - `Ok(cfg)` with `!cfg.wizard_completed` → leave `None` (**Not connected**); the
     wizard is still rendering (adrev #14). Done.
   - `Ok(cfg)` with `wizard_completed && !connect.connect_to_cms` → **Not connected**
     (offline; no Pat). Done.
   - `Ok(cfg)` with `wizard_completed && connect_to_cms` → continue.
2. Set bootstrap state → `Spawning` (§3.4); resolve the Pat sidecar path (§3.5) and
   the Pat config/mbox/pid paths (§3.6).
3. `PatBackend::spawn(...)`:
   - `Ok(backend)` → `AppBackend::set(Arc::new(backend))`; bootstrap state → `Ready`;
     spawn the **session-log drain task** (§3.7).
   - `Err(e)` → bootstrap state → `Failed { reason: e.to_string() }`; emit a
     `session_log:line` error line; leave `AppBackend` `None`. **Non-fatal** — the
     app stays up; the operator sees the reason.

No new "backend ready" event is required: the frontend already polls
`backend_status` (2s) and `mailbox_list` (10s), so the UI converges once the
backend installs. (Optional later enhancement: emit a `backend:status` event to
collapse the ≤10s mailbox-populate latency. Out of scope for v0.0.1.)

### 3.4 Representing the "backend error" state

`AppBackend` answers "is there a live backend?" (`Some`/`None`). It cannot express
"configured but failed". Add a sibling Tauri-managed state:

```rust
pub enum BootstrapState { NotConfigured, Spawning, Ready, Failed { reason: String } }
pub struct BackendBootstrap(pub RwLock<BootstrapState>);
```

**RESOLVED by adrev #9 — single managed state, not two.** Reading `AppBackend` and a
separate `BackendBootstrap` independently risks torn reads (e.g. `Some` while phase
still `Spawning`). Instead, ONE managed state holds both behind one lock:

```rust
pub enum BackendPhase { NotConfigured, Spawning, Ready, Failed { reason: String }, ConfigError { reason: String } }
pub struct BackendState { inner: RwLock<(BackendPhase, Option<Arc<dyn WinlinkBackend>>)> }
```

`backend_status` derives its `StatusDto` from one atomic snapshot of `(phase, backend)`:
`Failed`/`ConfigError` → `BackendStatus::Error { reason }`; `Ready` → the live
backend's `status()`; `Spawning` → connecting; `NotConfigured` → not-connected. This
replaces the §3.1/`AppBackend::set` calls with `BackendState` phase transitions, and
supersedes the prior `AppBackend` shape (or wraps it). Pin the exact migration of the
existing `AppBackend` managed state in the plan.

### 3.5 Pat sidecar path resolution

Pat ships as a Tauri sidecar (`tauri.conf.json` `externalBin: ["sidecars/pat"]`).
Resolve the per-target binary via Tauri's sidecar/path API (`tauri_plugin_shell`'s
sidecar resolution or `app.path()` against the resource dir + target triple suffix),
not a hardcoded path. The bootstrap has the `AppHandle`, so resolution happens there
and the resolved `PathBuf` is passed into `PatBackend::spawn`. **(Exact API flagged
for the plan — §8.3.)**

### 3.6 Pat config / mbox / pid paths

- Pat config: `XDG_CONFIG_HOME/pat/config.json` (mirrors `pat_config`'s first-run
  case and Pat's own default), or the Tauri app-config dir.
- mbox dir + pid file: the Tauri app-data dir (`app.path().app_data_dir()`),
  e.g. `…/tuxlink/pat-mbox/` and `…/tuxlink/pat.pid`.

These are derived once in the bootstrap. Reuse `XDG_*` honoring helpers where they
exist (`config::config_path()` is the pattern).

### 3.7 Session-log drain task

After install, start the drain via **`tauri::async_runtime::spawn`** (adrev #5),
holding `backend.stream_log()`; per `LogLine` it (a) appends to the durable
`SessionLogState` ring buffer (§11.1) with a monotonic `seq`, and (b) emits a
`session_log:line` Tauri event carrying `LogLineDto` (now including `seq`, adrev #4).
`SessionLog.tsx` seeds from `session_log_snapshot` (which reads the ring buffer) and
tails the event from the snapshot's last `seq` (snapshot-then-tail, adrev #3) — so no
line is lost in the window between subscribe and first listen, and same-timestamp
lines are de-duped on `seq`, not timestamp.

## 4. Data flow

```
config.json ──read_config──▶ bootstrap (bg thread)
                                  │ connect_to_cms?
                                  ├─ no ──▶ AppBackend=None (Not connected)
                                  └─ yes ─▶ PatBackend::spawn
                                              │  ok                err
                                              ▼                     ▼
                                    AppBackend::set(Arc)     BootstrapState::Failed
                                    BootstrapState::Ready    + error session_log:line
                                              │
        Pat stderr ─mpsc(String)─▶ bridge thread ─broadcast(LogLine)─▶ stream_log()
                                              │                              │
                                    mailbox_list / backend_status     drain task
                                    (polled by UI: 10s / 2s)          emit session_log:line
                                                                              │
                                                                      SessionLog.tsx (listen)
```

## 5. Error handling

All bootstrap failures are **non-fatal** (the app must launch):
- Pat binary not found → `BackendUnavailable`; `Failed { reason }`.
- Pat config render fails (missing callsign etc.) → surfaced from `PatProcess::spawn`'s
  `io::Error`; `Failed { reason }`. (Config::validate should have caught this; defense.)
- Port announce timeout (Pat hung/crashed at start) → `Failed { reason }`.
- Each failure emits one synthetic `LogSource::Backend` error `session_log:line` so
  the reason is visible in the pane, and sets `BootstrapState::Failed` so the ribbon
  shows an error rather than "not connected".

## 6. Part 97 boundary (RADIO-1)

- **What tuxlink controls (provable here):** the bootstrap invokes Pat **only** as
  `pat --config … --mbox … http --addr 127.0.0.1:<port>` and never calls Pat's
  connect/send APIs. Serving the local HTTP API is not a CMS session and not a
  transmission. A non-transmit instrumentation test asserts the spawned argv is
  `http`-only with a loopback addr (adrev #11). We do **not** claim anything about
  Pat's *internals* — `external/tuxlink-pat` is not in this worktree; the honest
  scope is "tuxlink does not initiate a connect/send on the bootstrap path."
- The live CMS round-trip (acceptance point 4: a real CMS:8773 session visible in the
  log) is **operator-triggered** (via the UI / wizard test-send) and **operator-run**.
- **Implementation rule:** this code is WRITTEN + COMMITTED by the agent; the licensee
  RUNS any path that can connect/send. No agent/CI/subagent executes a live-CMS binary
  to "verify completion" (CLAUDE.md live-radio rule). Spawn lifecycle tests use Pat in
  `http` mode only (no CMS target).

## 7. Testing strategy

- **`PatBackend::spawn` lifecycle (integration, real Pat `http` mode — safe, no
  transmission):** spawn against the bundled sidecar; assert the HTTP port is live,
  `list_messages(Inbox)` returns (empty) OK, and `stream_log()` delivers Pat's startup
  lines. Gated behind the sidecar's presence; skips cleanly if absent.
- **Bridge mapping (unit):** feed synthetic `String` lines through the bridge logic;
  assert `LogLine` field mapping (source=Pat, level=Info, message verbatim, timestamp
  parseable ISO).
- **Bootstrap conditional (unit, with fakes/seams):** `read_config` → `None`/offline/CMS
  branch selection without spawning a real process. Assert offline + pre-wizard leave
  `None`; CMS attempts spawn.
- **`backend_status` three-state mapping (unit):** `NotConfigured`/`Spawning`/`Ready`/
  `Failed` → the right `StatusDto`/error shape.
- **Live CMS round-trip:** OUT — operator-run only (acceptance point 4).
- Review tests against `docs/pitfalls/testing-pitfalls.md` (esp. process-spawn cleanup,
  no-real-keyring [tuxlink-cnd guard], pristine output).

## 8. Open decisions (for adversarial review)

### 8.1 Backend-error representation
§3.4 proposes a sibling `BackendBootstrap` managed state read by `backend_status`.
Alternative: an "error backend" installed in `AppBackend` whose `status()` returns
`Error`. Trade-off: the sibling-state approach keeps `AppBackend` semantics clean
(`Some` always means a usable backend) at the cost of a second managed state +
`backend_status` reading both. Adrev to confirm.

### 8.2 Wizard test-send vs ephemeral port
`wizard.rs::resolve_pat_base_url()` hardcodes `http://127.0.0.1:8080` (or `PAT_URL`),
but the bootstrap spawns Pat on an **ephemeral** port held in `AppBackend`. The wizard
test-send runs at first-run (pre-config, before the bootstrap spawns anything), so it
is a separate short-lived concern — but the inconsistency (two notions of "the Pat
URL") is a latent bug once both exist. **Proposed:** keep tuxlink-22l scoped to the
bootstrap; file a follow-up to reconcile the wizard test-send (likely: the wizard
spawns its own ephemeral Pat, or routes through a shared spawn helper). Adrev to
confirm scope.

### 8.3 Sidecar resolution API
Exact Tauri call for resolving the per-target sidecar binary path at runtime — pin in
the plan against the installed Tauri/`tauri_plugin_shell` version.

### 8.4 Double-spawn / single-instance
If the bootstrap spawns Pat and the operator also has a Pat running (or the wizard
spawns one), two Pat instances could contend for the mailbox dir / pid file. The
`pid_file` + `mbox_dir` are bootstrap-owned; adrev to confirm no contention with the
wizard path and whether a single-instance guard is needed for v0.0.1.

## 9. Out of scope / follow-ups

- Live CMS round-trip verification (operator-run; acceptance point 4).
- Session-log history ring buffer (`session_log_snapshot` real data) — tuxlink-xx3.
- Wizard test-send / ephemeral-port reconciliation (§8.2) — file a follow-up.
- Structured parsing of Pat log levels/timestamps (minimal mapping ships first).
- `backend:status` push event to collapse mailbox-populate latency (polling suffices).
- Backend teardown/disconnect + respawn-on-config-change (v0.0.1 spawns once at start).

## 10. Adversarial review — Codex round 1 (2026-05-20)

Raw transcript: `dev/adversarial/2026-05-20-pat-spawn-bootstrap-codex.md` (gitignored).
18 findings; dispositions below. Several reshape scope — see §11.

| # | Sev | Finding | Disposition |
|---|---|---|---|
| 1 | P0 | Startup logs lost: `PatProcess::spawn` discards stderr through the announce line; bridge starts before drain subscribes; snapshot empty | **ACCEPT.** Add a durable session-log ring buffer (see §11.1); `PatProcess` must expose the pre-announce lines (not discard them). |
| 2 | P0 | `tokio::broadcast` is the wrong persistence layer (only current subscribers) | **ACCEPT.** Broadcast = live notification only; durable last-N history owned by a managed `SessionLogState` the bridge writes to. |
| 3 | P1 | Tauri `session_log:line` events also lose lines before the frontend `listen`s | **ACCEPT.** Snapshot-then-tail with a monotonic `seq` cursor; `session_log_snapshot` returns the buffer. |
| 4 | P1 | `SessionLog.tsx` dedupes by `timestampIso` only — collisions drop lines | **ACCEPT.** Add `seq` to `LogLineDto`; dedupe on `seq`. (Frontend change — coordinate in plan.) |
| 5 | P1 | `tokio::spawn` from a raw `std::thread` may panic (no runtime in scope) | **ACCEPT.** Use `tauri::async_runtime::spawn` (or capture a `Handle`) for the drain task. Corrected in §3.7. |
| 6 | P1 | AppHandle use from the bg thread underspecified | **ACCEPT.** Clone `AppHandle`, move into the thread, handle `emit` errors; never move borrowed `app`/`State`. |
| 7 | P1 | `PatProcess::spawn` 10s timeout not actually enforced (blocking `read_line` can hang forever) | **ACCEPT — pre-existing bug.** File a separate bd issue to fix the announce read (timeout thread / `recv_timeout` / non-blocking). 22l depends on it. |
| 8 | P1 | Quitting mid-spawn can orphan Pat (child created before owner returns; detached setup thread) | **ACCEPT.** Spawn must be cancellable / the child owned by a supervisor that kills on app-exit; covered by §11.2. |
| 9 | P1 | Three-state torn read: `backend_status` reads `AppBackend` + `BackendBootstrap` independently | **ACCEPT.** Single managed state holding `{ phase, Option<Arc<backend>> }` under one lock; `backend_status` derives from one atomic snapshot. Replaces §3.4's two-state proposal. |
| 10 | P1 | `Connected { peer: cms.winlink.org }` is semantically false (no CMS link) | **ACCEPT.** Spawn sets `Disconnected` ("ready"); `Connected` only via operator `connect()`. Corrected in §3.1. |
| 11 | P1 | Part 97 "Pat never auto-connects" not provable here (`external/tuxlink-pat` absent) | **ACCEPT.** Reword: claim only that *tuxlink's* spawn runs `pat http --addr` and never calls connect/send (§6 rewritten below). Add a non-transmit instrumentation test asserting the spawned argv is `http`-only. |
| 12 | P1 | Debug sidecar is a 0-byte stub (`build.rs`); bootstrap would try to invoke it | **ACCEPT.** Detect zero-byte/non-executable sidecar → `Failed { reason }` with a clear message; honor a `PAT_BINARY` override for dev/tests. |
| 13 | P1 | Double-spawn / single-instance not optional (bootstrap ephemeral vs wizard `:8080` vs user Pat vs multi-instance; mbox/pid contention) | **ACCEPT.** Advisory file lock on `mbox_dir`; reconcile wizard test-send (§8.2 → file follow-up). |
| 14 | P1 | `wizard_completed=false` not handled — could spawn Pat while the wizard renders | **ACCEPT.** Gate = `wizard_completed && connect_to_cms`. Corrected in §3.3. |
| 15 | P2 | Invalid CMS config (`Serde`/`Validation`) misclassified as "Not connected" | **ACCEPT.** Distinguish `read_config` `NotFound` (pre-wizard → Not connected) from `Serde`/`Validation` (→ explicit config error state). |
| 16 | P2 | Drop semantics misstated (immediate kill, not SIGTERM→SIGKILL) | **ACCEPT.** Corrected in §3.1; `PatBackend::Drop` calls `shutdown()`. |
| 17 | P2 | Detached bridge/reader threads unsupervised (no join handles) | **ACCEPT.** Store join handles / a supervisor; on teardown: shutdown Pat → close channels → bounded join. |
| 18 | P2 | Pat stdout piped but never drained — child can block if it writes enough stdout | **ACCEPT — pre-existing.** Set Pat stdout to `Stdio::null()` (or drain it). Fold into the §11.1 PatProcess work. |

## 11. Scope revision driven by the adrev

The review shows 22l is bigger than "wire two functions." It decomposes into:

1. **§11.1 — PatProcess hardening + session-log durability (NEW dependency).**
   `PatProcess` must (a) not discard pre-announce stderr, (b) enforce the announce
   timeout without a hang risk (#7), (c) drain/null stdout (#18). Plus a durable
   `SessionLogState` ring buffer (last N lines, monotonic `seq`) that `PatBackend`'s
   bridge writes to and `session_log_snapshot` reads (#1,2,3). This subsumes
   tuxlink-xx3 (previously "out of scope") — it is now a prerequisite, not a follow-up.
2. **§11.2 — `PatBackend::spawn` + supervised lifecycle** (#5,6,8,16,17): the bridge,
   the drain task via `tauri::async_runtime::spawn`, the held+supervised `PatProcess`,
   graceful Drop.
3. **§11.3 — bootstrap + single managed state** (#9,12,14,15): `wizard_completed`
   gate, sidecar-stub detection + `PAT_BINARY` override, config-error classification,
   the single `{phase, backend}` managed state behind `backend_status`.
4. **§11.4 — frontend `seq` dedupe** (#4) + snapshot-then-tail cursor (#3).
5. **Follow-ups (separate bd issues):** wizard test-send/ephemeral-port reconciliation
   (#13/§8.2); the pre-existing `PatProcess` announce-timeout bug (#7) likely warrants
   its own issue even though 22l depends on it.

This decomposition is the basis for the implementation plan (writing-plans step).
