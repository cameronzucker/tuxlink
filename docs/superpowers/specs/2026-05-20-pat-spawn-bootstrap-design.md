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
5. **Hold the `PatProcess`** in a new field so its `Drop` (SIGTERM→SIGKILL) is the
   process's lifetime. `from_url` keeps this `None`.
6. Mint `backend_id`; set initial `status` to `Connected { transport, peer, since }`
   reflecting the configured transport (Pat's HTTP server is up; CMS connect is
   lazy/operator-triggered — see §6).

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

Runs **on a background thread** so the window paints immediately (the webview must
not block on Pat's up-to-10s announce). Sequence:

1. `crate::config::read_config()`.
   - `Err(_)` (pre-wizard / malformed) → leave `None` (**Not connected**). Done.
   - `Ok(cfg)` with `cfg.connect.connect_to_cms == false` → **Not connected** (offline;
     no Pat). Done.
   - `Ok(cfg)` with `connect_to_cms == true` → continue.
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

`backend_status` reads BOTH `AppBackend` and `BackendBootstrap`: when `AppBackend`
is `None` it returns a `StatusDto` derived from `BootstrapState` — `Failed` →
`BackendStatus::Error { reason }` (ribbon shows the error), `NotConfigured`/`Spawning`
→ the existing not-connected/connecting shapes. This is the smallest change that
makes the three states distinguishable through the existing polled command.
**(Flagged for adrev — see §8.1.)**

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

After `AppBackend::set`, spawn a tokio task that holds `backend.stream_log()` and,
per `LogLine`, emits a `session_log:line` Tauri event carrying the existing
`LogLineDto` (`ui_commands.rs`) shape. `SessionLog.tsx` already `listen`s for this
event and seeds from `session_log_snapshot` (which stays empty until the ring-buffer
follow-up, tuxlink-xx3 — out of scope here).

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

- **Spawning Pat in `http` mode does NOT transmit.** Pat keys up only on an explicit
  `/api/connect` or an outbound send to CMS. The bootstrap spawns Pat and serves the
  local HTTP API; it does **not** auto-connect to CMS.
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
