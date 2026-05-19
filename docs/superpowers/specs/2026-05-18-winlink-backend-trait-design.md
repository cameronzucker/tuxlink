# WinlinkBackend Trait — Design Spec

**Spec ID:** tuxlink-z5f
**Date:** 2026-05-18 (pre-adrev v1)
**Author:** agent `badger-oak-dahlia`
**Status:** pre-adrev — awaiting Codex cross-provider review (per bd-issue scope: ≥1 round, NOT 5)
**Branch:** `bd-tuxlink-z5f/winlink-backend-trait` (worktree off `feat/v0.0.1`)
**Closes via deliverable:** the PR that merges this spec's implementation into `feat/v0.0.1`
**Discipline:** tightly-scoped `superpowers:build-robust-features` per [memory `feedback_discipline_triage_rule`](../../../.claude/projects/-home-administrator-Code-tuxlink/memory/feedback_discipline_triage_rule.md) — architectural (trait shape constrains every future backend) but bounded (1hr brainstorm, ≥1 Codex round, trivial plan, one PR).

---

## 1. Why this spec exists

Per the 2026-05-18 operator + Codex convergence captured in [memory `project_v05_modem_design_posture`](../../../.claude/projects/-home-administrator-Code-tuxlink/memory/project_v05_modem_design_posture.md), tuxlink moves off Pat to a native Rust Winlink stack at v0.5 via staged replacement (not flag-day). **Step 1** is the architectural boundary: a `WinlinkBackend` trait that decouples tuxlink's UI/config layer from the Pat HTTP sidecar so that:

1. The Tauri command layer + frontend code in v0.0.1+ depends on `WinlinkBackend`, not on `PatClient`/`PatProcess` directly.
2. A future `NativeBackend` can co-exist behind the same interface during Steps 2–10 of the v0.5 plan (Pat-vs-native parity tests run two implementations against the same fixture set).
3. The Pat sidecar can be deleted after one fallback release without touching call sites.

This spec defines the trait surface, behavior contract, error model, runtime choice, and supporting types. It also defines two thin implementations: `PatBackend` (wrapper around the existing `pat_client` + `pat_process` modules) and `NativeBackend` (stub returning `NotImplemented` for every method; fleshed out in Steps 3–10).

**This is purely v0.5 prep.** v0.0.1 continues to ship the Pat sidecar via the cred-handling fork (PR #59 + PR #66 merged); the trait wraps that existing surface without disturbing the shipped wizard or `PatProcess` flow.

---

## 2. Scope

### 2.1 In scope

1. Define `WinlinkBackend` trait in a new module `src-tauri/src/winlink_backend.rs`.
2. Define supporting types: `MessageId`, `MessageMeta`, `MessageBody`, `OutboundMessage`, `TransportConfig`, `Session`, `BackendStatus`, `LogLine`, `BackendError`.
3. Re-export or relocate `MailboxFolder` (currently in `pat_client`) so the trait surface doesn't reach into a Pat-specific module.
4. Implement `PatBackend: WinlinkBackend` — wraps existing `PatClient` + `PatProcess`. Translates `PatClientError` → `BackendError`.
5. Implement `NativeBackend: WinlinkBackend` as a **stub** — every method returns `BackendError::NotImplemented`. No real native logic in this PR; that's Steps 3–10.
6. Migrate the **existing internal call site(s)** that consume `PatClient` directly to consume `WinlinkBackend` (if any exist in `src-tauri/src/lib.rs` or the Tauri command handlers). Tauri command surface stays the same; injection point shifts to a `Box<dyn WinlinkBackend>` (or generic `B: WinlinkBackend`) in command setup.
7. Test surface: 6 trait-contract tests (run against both `PatBackend` and `NativeBackend` where applicable) + 2 type-level tests. **Total: 8 tests** — meets the bd-issue's "5–10" cap.
8. Add `async-trait = "0.1"` dependency for the trait definition (see §3.4 for runtime choice rationale).
9. Add `futures = "0.3"` for `Stream` definitions (already pulled transitively by `tokio` but declared explicitly for clarity).

### 2.2 Out of scope

- **Real native Winlink protocol logic.** That's Steps 3–10 of the v0.5 plan (B2F mailbox/parser/writer, B2F session state machine, CMS telnet client, AX.25/KISS, native VARA, etc.). `NativeBackend` is a stub in this PR.
- **Splitting `WinlinkBackend` into multiple narrow traits** (`MessageStore`, `SessionControl`, `LogStream`). Single fat trait per §3.7 rationale. Splitting deferred until pain manifests.
- **Hot-swap of backends at runtime.** v0.0.1 chooses one backend at startup (Pat) and lives with it. v0.5 ships only the native backend after parity is proven; Pat sidecar is removed. No need to switch backends mid-process.
- **Mock backend for tests.** PatBackend already has a `mockito`-based test harness for `PatClient`. The trait-contract tests in §6 use `mockito` similarly. A dedicated `MockBackend` is YAGNI in v0.5 prep.
- **Frontend-facing changes.** The frontend's Tauri command invocations don't change. Command implementations may switch from `PatClient::new(...)` to a `Box<dyn WinlinkBackend>` lookup, but the wire shape is unchanged.
- **Pat sidecar removal.** That's Step 10 of the v0.5 plan, after one fallback release.
- **Config schema changes** to select a backend. v0.0.1 hardcodes `PatBackend`; a `backend_kind: BackendKind` config field arrives in Step 2 of v0.5 (when both `PatBackend` and `NativeBackend` are simultaneously functional). Forward-compat thought: this spec does NOT codify the config field, but the trait surface doesn't preclude it.

### 2.3 Dependency map

This spec is **not on the critical path for v0.0.1.** It develops in parallel with the wizard cluster (`tuxlink-ln3`). No bd issue currently blocks on this one. Downstream of this spec:

- **Step 2** (Freeze Pat backend as reference) — trivial follow-up once `PatBackend` exists.
- **Step 3** (Native mailbox + B2F) — has its own bd issue cluster (not yet filed).
- **Steps 4–10** — each gets a bd issue when this lands.

bd dep edges: none filed at spec time; will add `tuxlink-z5f` as a transitive ancestor when the v0.5 child issues are created.

---

## 3. Design

### 3.1 Trait surface

```rust
use async_trait::async_trait;
use futures::stream::BoxStream;

#[async_trait]
pub trait WinlinkBackend: Send + Sync {
    /// List message metadata in a folder. Returns `Vec<MessageMeta>` ordered
    /// most-recent-first.
    async fn list_messages(&self, folder: MailboxFolder)
        -> Result<Vec<MessageMeta>, BackendError>;

    /// Read the full body of one message by its MID.
    async fn read_message(&self, id: &MessageId)
        -> Result<MessageBody, BackendError>;

    /// Queue an outbound message for the next session. Returns the assigned
    /// MID. Does NOT open a transport session — that's `connect`.
    async fn send_message(&self, msg: OutboundMessage)
        -> Result<MessageId, BackendError>;

    /// Open a transport session. Returns a `Session` handle; the connection
    /// remains open until the handle is dropped OR `disconnect` is called.
    /// Dropping the handle disconnects best-effort (errors silenced).
    async fn connect(&self, transport: TransportConfig)
        -> Result<Session, BackendError>;

    /// Explicit disconnect with error propagation. Consumes the session.
    async fn disconnect(&self, session: Session)
        -> Result<(), BackendError>;

    /// Snapshot the current backend status. Cheap — does NOT do I/O.
    fn status(&self) -> BackendStatus;

    /// Subscribe to the backend's log stream. The stream emits one `LogLine`
    /// per backend log event. **Cancellation:** drop the stream to
    /// unsubscribe. The backend handles lagged subscribers internally
    /// (oldest log lines are dropped if the consumer falls behind).
    fn stream_log(&self) -> BoxStream<'static, LogLine>;
}
```

### 3.2 Supporting types

```rust
/// Folder selector — re-exported from `pat_client` for trait-level naming
/// stability. The Pat module retains it for internal use.
pub use crate::pat_client::MailboxFolder;

/// Newtype around the Winlink Message ID (MID) string.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MessageId(pub String);

/// Light header-only view returned by `list_messages`.
#[derive(Debug, Clone)]
pub struct MessageMeta {
    pub id: MessageId,
    pub subject: String,
    pub from: String,
    pub date: String,        // ISO-8601 string (matches Pat's wire format)
    pub unread: bool,
    pub body_size: u64,
}

/// Full body returned by `read_message`. Headers come from the matching
/// `MessageMeta` if the caller wants them — `MessageBody` is the bytes only.
#[derive(Debug, Clone)]
pub struct MessageBody {
    pub id: MessageId,
    pub mime_text: String,   // RFC 5322 representation; native backend may add MIME parts in v0.5+
}

/// Outbound message — what `send_message` consumes.
#[derive(Debug, Clone)]
pub struct OutboundMessage {
    pub to: Vec<String>,     // Winlink callsigns or RFC 5322 addresses
    pub cc: Vec<String>,
    pub subject: String,
    pub body: String,
    pub date: String,        // ISO-8601; caller-provided so backend impls are deterministic in tests
}

/// Transport selector for `connect`. Wraps the v0.0.1 enum with
/// `#[non_exhaustive]` for forward compat — v0.5+ adds Packet/Pactor/VARA
/// HF/VARA FM/AX.25/KISS variants without breaking callers.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum TransportConfig {
    /// CMS Telnet (plain or TLS), per existing `config::CmsTransport`.
    Cms { mode: crate::config::CmsTransport },
    // Future: Packet { freq, ... }, Pactor { ... }, VaraHf { ... }, etc.
}

/// Opaque session handle. `Drop` impl disconnects best-effort; callers
/// wanting error propagation use `WinlinkBackend::disconnect`.
///
/// **Not** `Clone` — sessions are unique resources. `Send` because the
/// session may live across an `await` point in a Tauri command handler.
#[derive(Debug)]
pub struct Session {
    pub(crate) inner: SessionInner,
}

#[derive(Debug)]
pub(crate) enum SessionInner {
    Pat { pat_session_id: String },  // Pat returns a session ID via HTTP
    Native(()),                       // NativeBackend stub never produces one
}

impl Drop for Session {
    fn drop(&mut self) {
        // Best-effort: if the backend exposes a synchronous cleanup, call it.
        // For PatBackend, the HTTP disconnect is async — we cannot block-on
        // here without risking deadlock in async contexts. The contract:
        // explicit `disconnect` is the right way to release; Drop is the
        // safety net for forgotten sessions.
    }
}

#[derive(Debug, Clone)]
pub enum BackendStatus {
    Disconnected,
    Connecting { transport: String },
    Connected { transport: String, peer: String, since_iso: String },
    Disconnecting,
    Error { reason: String },
}

#[derive(Debug, Clone)]
pub struct LogLine {
    pub timestamp_iso: String,
    pub level: LogLevel,
    pub source: LogSource,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel { Trace, Debug, Info, Warn, Error }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogSource { Backend, Pat, Transport, Wire }
```

### 3.3 Error model

Single `BackendError` enum for all trait methods. Mirrors the `thiserror`-based pattern from `config.rs`'s `ConfigValidationError` / `ConfigReadError` / `ConfigWriteError`.

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("backend not configured: {0}")]
    NotConfigured(String),

    #[error("message not found: {0:?}")]
    NotFound(MessageId),

    #[error("authentication failed: {0}")]
    AuthFailed(String),

    #[error("transport failed: {0}")]
    TransportFailed(String),

    #[error("backend rejected message: {0}")]
    MessageRejected(String),

    #[error("backend unavailable: {0}")]
    BackendUnavailable(String),

    #[error("operation cancelled")]
    Cancelled,

    #[error("not implemented (this backend does not support this operation)")]
    NotImplemented,

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("internal error: {0}")]
    Internal(String),
}
```

**PatBackend translates `PatClientError` → `BackendError`:**

| `PatClientError` | → | `BackendError` |
|---|---|---|
| `Http(e) where e.is_connect()` | → | `BackendUnavailable("could not reach Pat HTTP sidecar")` |
| `Http(e) where e.is_timeout()` | → | `TransportFailed("Pat HTTP request timed out")` |
| `Http(e) other` | → | `Internal("Pat HTTP client error: {e}")` |
| `Status(401)` | → | `AuthFailed("Pat returned 401")` |
| `Status(404)` | → | `NotFound(...)` if context is read_message; otherwise `Internal` |
| `Status(other)` | → | `Internal("Pat returned status {n}")` |

### 3.4 Async / runtime choice

**Choice: `async-trait`-based async-first.** Rationale (atomic decision; converging here for Codex round):

1. `stream_log() -> BoxStream<...>` requires async — `Stream` is built on `Future`.
2. Tauri 2 commands are `async fn` by convention. Backend methods feeding a Tauri command want to be async to avoid `tokio::task::spawn_blocking` shims at every call site.
3. The future `NativeBackend` is inherently async (network I/O, B2F state machine over async TCP). A sync trait would force `block_on` in native callers, deadlocking under the tokio runtime.
4. `tokio = "1"` is already a dependency.
5. `PatBackend` wraps the existing `reqwest::blocking::Client` via `tokio::task::spawn_blocking` — minor PatBackend-only complexity, contained.

**Trait-object compatibility:** `async-trait = "0.1"` macro generates `Box<dyn Future>`-returning trait methods so `Box<dyn WinlinkBackend>` works for runtime dispatch. v0.5+ may move to native `async fn in trait` once MSRV permits, but `async-trait` is the right call for tuxlink's current Rust 1.75 floor (per `Cargo.toml`).

**Alternative considered: sync trait + `block_on` at call sites.** Rejected — bad ergonomics for Tauri command handlers, deadlock risk under tokio runtime, awkward for `Stream`-returning methods.

**Alternative considered: split sync ops (list/read/send) from async ops (connect/stream_log).** Rejected — unnecessary complexity; the sync ops will be async-IO-backed in NativeBackend anyway.

### 3.5 Session handle ownership

**Choice: RAII handle, Drop = best-effort disconnect, explicit `disconnect()` for error propagation.** Rationale:

1. Mirrors `PatProcess`'s ownership model (Drop = best-effort SIGKILL).
2. Sessions are unique resources — `!Clone` prevents double-disconnect bugs.
3. `Send` (no `Sync`) — sessions can move across async tasks but cannot be shared by reference. Matches typical TCP-session ownership.
4. The Drop impl deliberately does NOTHING for `PatBackend` (HTTP disconnect requires async — see Drop body comment in §3.2). The reason this is safe: Pat's sessions are server-side and auto-time-out; an orphaned session leaks an idle session-id but no resources beyond that. For `NativeBackend`, Drop will eventually do a blocking TCP close (no async runtime needed for `close()` on a socket fd).

**Alternative considered: synchronous `disconnect_blocking()` callable from Drop.** Rejected — `PatBackend` cannot do a blocking HTTP call inside async-context Drop without `block_on`, which can deadlock the executor.

**Alternative considered: typed `OwnedSession<B: WinlinkBackend>` with a back-reference to the backend, enabling RAII async-disconnect via spawned cleanup task.** Rejected — adds significant complexity (lifetimes, Send bounds on the backend, runtime presence) for a defensive cleanup that's not load-bearing in v0.0.1. Revisit at Step 3 if Pat session leaks become measurable.

### 3.6 Log stream cancellation

**Choice: drop-to-cancel via `BroadcastStream`.** Rationale:

1. Internally, the backend produces log events to a `tokio::sync::broadcast::Sender`. Each `stream_log()` call creates a fresh `Receiver` wrapped in `tokio_stream::wrappers::BroadcastStream`.
2. Dropping the stream drops the receiver — the sender's broadcast queue drops the slot when all receivers go away (or when a receiver lags past the buffer, `RecvError::Lagged` is emitted, and the stream is filtered to skip lagged values).
3. No explicit cancellation token needed — `Drop` is the cancellation surface.
4. Multiple concurrent subscribers supported (e.g., session-log pane + diagnostics export both running).

**Alternative considered: `mpsc::UnboundedReceiver` with cancellation token.** Rejected — single-subscriber model, doesn't fit "session-log pane + future diagnostics export."

**Alternative considered: poll-based `next_log_line() -> Option<LogLine>`.** Rejected — busy-wait semantics, doesn't compose with `select!` / `StreamExt` ergonomics in the Tauri command surface.

**Lag behavior:** `BroadcastStream` emits `Result<T, BroadcastStreamRecvError>`; we filter and drop `Err(Lagged(n))` cases inside `stream_log()`'s mapping closure, so the public stream is `Stream<Item = LogLine>` (no Result wrapper, no error variants surfaced to consumers). Lagged log lines are silently dropped — operators viewing the log live see "fresh" events only; nothing relies on log lines for correctness.

### 3.7 Single trait vs split traits

**Choice: single fat trait `WinlinkBackend`.** Rationale:

1. Both Pat and the future native backend naturally provide all operations — there's no real partitioning where one backend implements half the surface.
2. Callers use one trait bound, not three.
3. Mockability: trait-contract tests in §6 use `mockito` at the HTTP level for `PatBackend`. A `MockBackend` is YAGNI.
4. The trait is medium-sized (7 methods, 2 fn-returning-Stream) — not so large that the cognitive overhead justifies splitting.
5. **Future split is non-breaking** — if v0.5+ surfaces a backend that implements only message ops (no transport), we can extract `MessageStore: WinlinkBackend` as a supertrait split. No code change to current callers.

**Alternative considered: `MessageStore` + `SessionControl` + `LogStream` traits with `WinlinkBackend = MessageStore + SessionControl + LogStream` blanket impl.** Rejected — adds boilerplate for no current consumer benefit; revisit if v0.5+ surfaces a backend asymmetry.

### 3.8 PatBackend implementation outline

```rust
pub struct PatBackend {
    process: Arc<Mutex<PatProcess>>,         // owns the spawned Pat process
    client: PatClient,                        // HTTP client to Pat
    log_tx: broadcast::Sender<LogLine>,       // multiplexes Pat stderr → subscribers
    status: Arc<RwLock<BackendStatus>>,       // updated by connect/disconnect; read by status()
}

impl PatBackend {
    pub fn spawn(spawn_opts: PatSpawnOptions) -> Result<Self, BackendError> {
        // 1. Pre-spawn: create broadcast channel for logs
        // 2. PatProcess::spawn(spawn_opts) — blocks until Pat HTTP is ready
        // 3. Replace stderr tail loop with a tokio task that parses lines into LogLine + broadcast::send
        // 4. Build PatClient pointed at process.http_port()
        // 5. status = BackendStatus::Disconnected
    }
}

#[async_trait]
impl WinlinkBackend for PatBackend {
    async fn list_messages(&self, folder: MailboxFolder)
        -> Result<Vec<MessageMeta>, BackendError>
    {
        let client = self.client.clone();
        let folder_copy = folder;
        tokio::task::spawn_blocking(move || client.list(folder_copy))
            .await
            .map_err(|e| BackendError::Internal(format!("spawn_blocking join: {e}")))?
            .map_err(translate_pat_err)
            .map(|msgs| msgs.into_iter().map(meta_from_pat_message).collect())
    }
    // ... etc
}
```

Open detail (Codex-converge target): does Pat 1.0.0 HTTP expose connect/disconnect endpoints? If yes, `PatBackend::connect` POSTs there. If no, `PatBackend::connect` is a no-op that returns a synthetic `Session` and Pat does on-demand connections per `send_message`. The trait shape is independent of this resolution.

### 3.9 NativeBackend stub

```rust
pub struct NativeBackend;

impl NativeBackend {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl WinlinkBackend for NativeBackend {
    async fn list_messages(&self, _folder: MailboxFolder)
        -> Result<Vec<MessageMeta>, BackendError>
    {
        Err(BackendError::NotImplemented)
    }
    // ... all other methods return Err(BackendError::NotImplemented)
    // status() returns BackendStatus::Disconnected
    // stream_log() returns futures::stream::empty().boxed()
}
```

The stub exists so v0.5 Step 2+ can add real native logic incrementally without breaking the trait bound. No `unimplemented!()` panics — every method returns a typed error so callers see `NotImplemented` at runtime rather than crashing the process.

---

## 4. Test plan (8 tests)

Tests live in `src-tauri/tests/winlink_backend_test.rs`. Uses `mockito` for HTTP mocking (same pattern as the existing `PatClient` tests would use if they existed; today the `pat_client` module is untested directly — this spec adds trait-level tests that exercise it transitively).

| # | Test name | What it verifies |
|---|---|---|
| 1 | `test_pat_backend_list_messages_returns_mapped_metas` | `PatBackend::list_messages` against a `mockito` server returning the Pat DTO JSON, asserts the returned `Vec<MessageMeta>` is correctly mapped (id, subject, from, date, unread, body_size). |
| 2 | `test_pat_backend_send_message_posts_multipart` | `PatBackend::send_message` POSTs the expected multipart form to `/api/mailbox/out` and returns the assigned MID. Mockito matches the request body. |
| 3 | `test_pat_backend_translates_404_to_not_found_for_read` | `PatBackend::read_message` against a mock returning 404 → returns `BackendError::NotFound(_)`. Validates the error-translation table in §3.3. |
| 4 | `test_pat_backend_translates_401_to_auth_failed` | Any method against a mock returning 401 → returns `BackendError::AuthFailed(_)`. |
| 5 | `test_pat_backend_translates_connect_error_to_backend_unavailable` | `PatBackend` pointed at a closed port → returns `BackendError::BackendUnavailable(_)` on any method. |
| 6 | `test_native_backend_returns_not_implemented_for_every_method` | `NativeBackend::new()`, then call each of `list_messages` / `read_message` / `send_message` / `connect` / `disconnect` → all return `BackendError::NotImplemented`. `status()` returns `Disconnected`. `stream_log()` is an empty stream. |
| 7 | `test_session_drop_does_not_panic` | Construct a `Session` (via `PatBackend::connect` against a mocked Pat connect endpoint), drop it, assert no panic and that the test completes. Validates the §3.5 Drop contract. |
| 8 | `test_log_stream_emits_lines_and_handles_drop` | Construct a `PatBackend`, subscribe to `stream_log()`, push 3 fake log events via the internal broadcast sender, assert the stream emits them in order. Drop the stream, assert the backend continues running (no panic on dropped receiver). |

**Why only 8 tests (not 24+):** per [`feedback_discipline_triage_rule`](../../../.claude/projects/-home-administrator-Code-tuxlink/memory/feedback_discipline_triage_rule.md), this is tightly-scoped architectural work. The trait's behavior contract is small (each method has one happy path + one error-class mapping); deeper tests belong to the future native backend (B2F parser tests, state-machine tests, parity-vs-Pat tests). At v0.5 Step 2, the `NativeBackend` stub gets replaced with real logic, and that PR carries the deeper test suite.

---

## 5. Wave-2+ extensibility notes (informational; NOT in this PR)

These notes document the trait's forward-compat surface so future steps don't redesign it:

1. **Adding a new transport (Packet, VARA HF, etc.):** add a variant to `TransportConfig`. Existing callers compile unchanged (the `#[non_exhaustive]` attribute warns them their `match` is non-exhaustive but doesn't break it).
2. **Adding a backend with restricted capabilities:** split the trait into `MessageStore: Send + Sync` + `SessionControl` + `LogStream`, then `trait WinlinkBackend: MessageStore + SessionControl + LogStream {}` is a blanket marker. Existing impls satisfy it automatically.
3. **Backwards-incompat surface change:** if v0.5 needs to add an argument to `send_message`, deprecate `send_message` with a `#[deprecated]` attribute and add `send_message_v2`. Existing callers continue compiling; new callers use the new API. Remove the old method in a major version (v0.6+).
4. **Adding a backend-config selector:** introduce `enum BackendKind { Pat, Native }` in `config.rs`, gate startup-time selection on it. Out of scope for this PR but cleared by the trait existing.
5. **Mocking for end-to-end tests:** a `MockBackend` is trivial to add later — implement the trait with `Vec`-backed in-memory storage. Out of scope for this PR per §2.2.

---

## 6. Open questions (Codex-converge targets)

The Codex cross-provider round (1 round per bd-issue scope) should focus on:

1. **Send + Sync bounds on the trait.** Are `Send + Sync` correct? `PatBackend` holds `Arc<Mutex<PatProcess>>` which is `Send + Sync` if `PatProcess` is `Send` (it is — owns `Option<Child>` which is `Send`). NativeBackend stub is trivially both. Any future backend with `Rc<RefCell<...>>` internals would break this — flag if forward-compat concern.
2. **`async-trait` macro overhead.** Generates a `Pin<Box<dyn Future>>` per method call. Acceptable for backend operations (network I/O dwarfs the box allocation) but worth noting if a hot path emerges. Codex: verify no hot path expected.
3. **`Session` not exposing the backend type.** The session is `pub struct Session { inner: SessionInner }` with `pub(crate) enum SessionInner`. This means callers can `disconnect(session)` against the correct backend but can also (accidentally) try to pass a `PatBackend` session to a `NativeBackend::disconnect`. Codex: should we tighten this with a phantom type parameter `Session<B>` or accept the v0.0.1 looseness (one backend in process)?
4. **`MessageBody.mime_text: String` vs `Vec<u8>`.** Pat returns RFC 5322 as text. Native backend may need byte-level fidelity (binary attachments). For v0.0.1+v0.5-Step-3, text suffices. Codex: flag if changing to `Vec<u8>` should happen now to avoid a churn later.
5. **`status() -> BackendStatus` being non-async.** Is the snapshot-cheap contract realistic for both Pat (reads an `RwLock`) and native (also reads an in-memory state)? Codex: verify or surface a counter-example.
6. **`OutboundMessage.date: String` being caller-provided.** Pat-style — caller passes ISO-8601. Native backend can re-validate. Codex: should this be `chrono::DateTime<Utc>` instead to push validation to the type system? Cost: adds `chrono` dependency.
7. **`stream_log()` not being `async fn`.** It returns a stream synchronously; the stream itself yields values asynchronously. Standard pattern but worth double-checking with Codex that this matches what callers expect under tokio.
8. **NativeBackend stub returning `NotImplemented` vs `unimplemented!()`.** The spec chooses typed error. Codex: any reason to prefer panic-on-stub-call instead? (Position: typed error is safer; stubs shouldn't crash the process.)

Findings from the Codex round land in §7 as "v2 revision" + applied inline.

---

## 7. Revision log

| Version | Date | Author | Change summary |
|---|---|---|---|
| v1 | 2026-05-18 | badger-oak-dahlia | Initial spec — pre-adrev |

(v2 entry to be added after Codex round.)

---

## 8. References

- **bd issue:** `tuxlink-z5f`
- **Memory: `feedback_discipline_triage_rule`** — tightly-scoped pipeline framing
- **Memory: `feedback_no_atomic_decisions_to_operator`** — atomic decisions converge with Codex, not operator
- **Memory: `project_v05_modem_design_posture`** — full-replacement (no VARA interop), no adoption constraint
- **Memory: `feedback_ai_amateur_radio_reliability`** — Codex training-data bias against current amateur-radio reality; cross-check claims against operator
- **Existing code:**
  - [`src-tauri/src/pat_client.rs`](../../../src-tauri/src/pat_client.rs) — current sync HTTP client
  - [`src-tauri/src/pat_process.rs`](../../../src-tauri/src/pat_process.rs) — current process lifecycle
  - [`src-tauri/src/config.rs`](../../../src-tauri/src/config.rs) — `thiserror` patterns used here
- **CLAUDE.md sections:** Tool referee, Git workflow, Documentation propagation contract
