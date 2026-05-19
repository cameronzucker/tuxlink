# WinlinkBackend Trait — Design Spec

**Spec ID:** tuxlink-z5f
**Date:** 2026-05-18 (v2 — post-Codex-R1 revision)
**Author:** agent `badger-oak-dahlia`
**Status:** revised — Codex R1 (cross-provider) findings applied (3 P0, 4 P1, 4 P2, 1 P3)
**Branch:** `bd-tuxlink-z5f/winlink-backend-trait` (worktree off `feat/v0.0.1`)
**Closes via deliverable:** the PR that merges this spec's implementation into `feat/v0.0.1`
**Discipline:** tightly-scoped `superpowers:build-robust-features` per [memory `feedback_discipline_triage_rule`](../../../.claude/projects/-home-administrator-Code-tuxlink/memory/feedback_discipline_triage_rule.md) — architectural (trait shape constrains every future backend) but bounded (1hr brainstorm, ≥1 Codex round, trivial plan, one PR).

### v2 revision changes from v1

This v2 supersedes v1 (committed `eb85377`) after one Codex cross-provider adrev round (raw transcript at `dev/adversarial/2026-05-18-tuxlink-z5f-winlink-backend-trait-codex-r1.md`, gitignored).

| Severity | Finding | Applied where |
|---|---|---|
| P0 | Session contract internally inconsistent (Drop docstring says best-effort disconnect; §3.5 says Drop does nothing). Wrong-backend-session can pass through `disconnect`. | §3.1 + §3.2 + §3.5 — Session carries a backend-instance-id; `BackendError::InvalidSession` added; Drop docstring rewritten as "local cleanup only; explicit `disconnect` is the only guaranteed release path" |
| P0 | `MessageBody.mime_text: String` bakes UTF-8 into the long-lived boundary; native backend needs byte fidelity for MIME attachments. | §3.2 — changed to `raw_rfc5322: Vec<u8>`; added doc comment about display-decode at Tauri boundary |
| P0 | `BroadcastStream` named in §3.6 lives in `tokio-stream` crate (not `tokio` or `futures`); §2.1 deps list would not compile. | §2.1 — added `tokio-stream = { version = "0.1", features = ["sync"] }` |
| P1 | `PatBackend` wraps APIs that don't exist on `PatClient` yet (no `read`, no `Clone`, `send` returns `()` not MID). | §2.1 + new §3.8.0 — pat_client extension prerequisites enumerated; `send_message` trait return changed to `Result<Option<MessageId>, BackendError>` per the [feedback_ai_amateur_radio_reliability memory](../../../.claude/projects/-home-administrator-Code-tuxlink/memory/feedback_ai_amateur_radio_reliability.md) (don't take Codex as authority on "does Pat return MID") |
| P1 | Only `TransportConfig` is `#[non_exhaustive]`; other public enums harden as soon as multiple backends depend on them. | §3.2 + §3.3 — `#[non_exhaustive]` added to `BackendError`, `BackendStatus`, `LogSource`, `LogLevel`, `MessageMeta`, `MessageBody`, `MailboxFolder` (kept off `OutboundMessage` for caller-construction ergonomics) |
| P1 | `PatProcess::spawn` consumes stderr inside startup loop; after the break, no reader is left for `stream_log` to broadcast from. | §2.1 + new §3.8.1 — pat_process spawn-signature refactor enumerated (accepts optional log sink) |
| P1 | `BackendError` collapses too much into `String`; loses source chains. | §3.3 — `BackendUnavailable`, `TransportFailed`, `Internal` made structured with `source: Option<Box<dyn Error + Send + Sync>>` |
| P2 | Send+Sync correct; add MutexGuard-not-across-await note. | §3.4 — note added |
| P2 | Date type question mis-framed: it's "where is timestamp validation enforced," not String-vs-chrono. | §3.2 — `date: String` doc updated: "RFC 3339 UTC; backend MUST validate or pass through validated form" |
| P2 | Note about `async fn in trait` in §3.4 conflated with object-safety blocker. | §3.4 — correction: 1.75 has async-fn-in-trait but `dyn` object-safety is the blocker (not MSRV); `async-trait` stays |
| P2 | `status()` non-async correct; document that implementations cache + update during ops. | §3.1 docstring updated |
| P3 | NativeBackend stub returning typed `NotImplemented` correct. | Ratified |

Test count grew from 8 to 10 to cover: backend-instance Session affinity (test #9) + MessageBody byte fidelity (test #10).

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
2. Define supporting types: `MessageId`, `MessageMeta`, `MessageBody`, `OutboundMessage`, `TransportConfig`, `Session` (carries backend-instance id per v2 P0 #1), `BackendStatus`, `LogLine`, `BackendError` (structured-source variants per v2 P1 #7).
3. Re-export or relocate `MailboxFolder` (currently in `pat_client`) so the trait surface doesn't reach into a Pat-specific module. Mark `#[non_exhaustive]` per v2 P1 #5.
4. **Extend `PatClient` (per v2 P1 #4):** add `#[derive(Clone)]`, add `read(folder, mid) -> Result<MessageBody, PatClientError>` method, investigate Pat's `/api/mailbox/out` response for MID (if present, return it; else trait returns `None`). See §3.8.0 for full prerequisite list.
5. **Refactor `PatProcess::spawn` (per v2 P1 #6):** change signature to accept an optional log sink (`Option<std::sync::mpsc::Sender<String>>` in `PatSpawnOptions`); after startup-port-detection, spawn a thread that forwards remaining stderr lines into the sink. Existing callers (test code) pass `None` and behavior is unchanged. See §3.8.1.
6. Implement `PatBackend: WinlinkBackend` — wraps the extended `PatClient` + the refactored `PatProcess`. Translates `PatClientError` → `BackendError` (table in §3.3).
7. Implement `NativeBackend: WinlinkBackend` as a **stub** — every method returns `BackendError::NotImplemented`. No real native logic in this PR; that's v0.5 Steps 3–10.
8. Migrate the **existing internal call site(s)** that consume `PatClient` directly to consume `WinlinkBackend` (if any exist in `src-tauri/src/lib.rs` or the Tauri command handlers). Tauri command surface stays the same; injection point shifts to a `Box<dyn WinlinkBackend>` (or generic `B: WinlinkBackend`) in command setup.
9. Test surface: 8 trait-contract tests + 2 type-level tests covering Session-backend-affinity + MessageBody byte fidelity. **Total: 10 tests** — at the upper end of the bd-issue's "5–10" cap.
10. Add `async-trait = "0.1"` dependency for the trait definition (see §3.4 for runtime choice rationale).
11. Add `futures = "0.3"` for `Stream` definitions.
12. Add `tokio-stream = { version = "0.1", features = ["sync"] }` for `BroadcastStream` (per v2 P0 #3 — `BroadcastStream` is NOT provided by `tokio` or `futures`).

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

    /// Queue an outbound message for the next session. The trait returns
    /// `Option<MessageId>` to honestly handle backends that DO assign and
    /// return a MID at queue time (future native backend) vs backends that
    /// do NOT (current Pat 1.0.0 HTTP API does not — confirmed by code
    /// inspection of `src-tauri/src/pat_client.rs::PatClient::send`). Does
    /// NOT open a transport session — that's `connect`.
    async fn send_message(&self, msg: OutboundMessage)
        -> Result<Option<MessageId>, BackendError>;

    /// Open a transport session. Returns a `Session` handle that carries
    /// backend-instance identity (so `disconnect` can reject a session
    /// passed to the wrong backend with `BackendError::InvalidSession`).
    /// The connection remains open until the handle is dropped OR
    /// `disconnect` is called.
    ///
    /// **Drop semantics:** dropping a `Session` performs LOCAL cleanup
    /// only — releasing in-process state, NOT a remote-disconnect call.
    /// Explicit `disconnect` is the only guaranteed remote-release path.
    /// The reason: backends with async-only remote disconnect (PatBackend
    /// HTTP) cannot block-on cleanup inside Drop without risking executor
    /// deadlock. Pat-side orphaned sessions are server-time-out-bounded;
    /// native-side orphans will close their TCP socket fd via Drop.
    async fn connect(&self, transport: TransportConfig)
        -> Result<Session, BackendError>;

    /// Explicit disconnect with error propagation. Consumes the session.
    /// Returns `BackendError::InvalidSession` if the session was minted
    /// by a different backend instance.
    async fn disconnect(&self, session: Session)
        -> Result<(), BackendError>;

    /// Snapshot the current backend status. Cheap — MUST NOT do I/O.
    /// Implementations cache the status internally and update it during
    /// connect/disconnect/operation flows.
    fn status(&self) -> BackendStatus;

    /// Subscribe to the backend's log stream. The stream emits one
    /// `LogLine` per backend log event. **Cancellation:** drop the stream
    /// to unsubscribe. The backend handles lagged subscribers internally
    /// (oldest log lines are dropped silently if the consumer falls
    /// behind; this is acceptable because nothing relies on log lines for
    /// correctness).
    fn stream_log(&self) -> BoxStream<'static, LogLine>;
}
```

**`Send + Sync` discipline (per v2 P2 #8):** implementors MUST NOT hold a
`std::sync::MutexGuard` across an `.await` point (Rust's mutex guards are
`!Send`). Long-running blocking work (Pat process lifecycle, sync HTTP)
belongs in `tokio::task::spawn_blocking`. Where async state must be
shared across tasks, prefer `tokio::sync::Mutex` (whose guard IS `Send`)
or design around `Arc<RwLock<...>>` with short-held read/write locks.

### 3.2 Supporting types

```rust
/// Folder selector — re-exported from `pat_client` for trait-level naming
/// stability. The Pat module retains it for internal use.
/// `#[non_exhaustive]` per v2 P1 #5 — native backend may add Spam/Drafts.
pub use crate::pat_client::MailboxFolder;
// Note: the `#[non_exhaustive]` attribute is added to MailboxFolder
// in-place at its definition in pat_client.rs.

/// Newtype around the Winlink Message ID (MID) string.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MessageId(pub String);

/// Light header-only view returned by `list_messages`.
/// `#[non_exhaustive]` per v2 P1 #5 — future fields (html_subject hint,
/// has_attachments flag, etc.) added without breaking exhaustive matches.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct MessageMeta {
    pub id: MessageId,
    pub subject: String,
    pub from: String,
    /// RFC 3339 UTC timestamp. Backend MUST emit canonical form (e.g.,
    /// `2026-05-18T15:37:27Z`); callers MAY parse with `chrono::DateTime`
    /// at the Tauri/frontend boundary. Validation lives at the backend
    /// emit point, not in this type.
    pub date: String,
    pub unread: bool,
    pub body_size: u64,
}

/// Full body returned by `read_message`. Headers come from the matching
/// `MessageMeta` if the caller wants them — `MessageBody` carries the
/// bytes.
///
/// **Byte fidelity (per v2 P0 #2):** the body is `Vec<u8>` not `String`
/// because Winlink B2F messages can contain MIME parts with binary
/// payloads (attachments). Pat's HTTP response is currently text/RFC 5322
/// but byte-decoded — the conversion to display text (via
/// `String::from_utf8_lossy` or a MIME parser) happens at the Tauri
/// command boundary, NOT in the backend trait. Native backend in v0.5+
/// preserves raw bytes from the B2F wire format.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct MessageBody {
    pub id: MessageId,
    pub raw_rfc5322: Vec<u8>,
}

/// Outbound message — what `send_message` consumes.
///
/// Intentionally NOT `#[non_exhaustive]` (per v2 P1 #5 carve-out):
/// callers construct this type, and `#[non_exhaustive]` would force
/// `OutboundMessage { ..Default::default() }` syntax that hurts
/// ergonomics. Adding fields later IS a breaking change for callers,
/// accepted in exchange for clean construction. Revisit at v1.0.
#[derive(Debug, Clone)]
pub struct OutboundMessage {
    pub to: Vec<String>,     // Winlink callsigns or RFC 5322 addresses
    pub cc: Vec<String>,
    pub subject: String,
    pub body: String,
    /// RFC 3339 UTC timestamp. Caller-provided so backend impls are
    /// deterministic in tests. Backend MUST validate format and reject
    /// with `BackendError::MessageRejected("invalid date format")` on
    /// non-RFC-3339 input.
    pub date: String,
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

/// Opaque session handle carrying backend-instance identity (per v2 P0
/// #1) so that `disconnect` rejects sessions passed to the wrong backend.
///
/// **Not** `Clone` — sessions are unique resources. **`Send`** because
/// the session may live across an `await` point in a Tauri command
/// handler.
///
/// **Drop semantics:** LOCAL cleanup only — releases in-process state
/// (e.g., a `Mutex` guard count). It does NOT perform a remote-disconnect
/// call. Callers wanting error-propagating release MUST call
/// `WinlinkBackend::disconnect`. Rationale: backends with async-only
/// remote disconnect (PatBackend's HTTP) cannot safely block-on cleanup
/// inside Drop without executor deadlock risk. Pat-side orphans
/// auto-time-out server-side; native-side orphans close their socket fd
/// via the normal Drop chain on the inner TCP stream.
#[derive(Debug)]
pub struct Session {
    pub(crate) backend_id: BackendInstanceId,
    pub(crate) inner: SessionInner,
}

/// Backend-instance identifier minted at backend construction time
/// (e.g., via `uuid::Uuid` or a monotonic atomic counter). Embedded in
/// every `Session` so `disconnect` can validate the session came from
/// this backend instance. v0.0.1 uses a process-local `AtomicU64`
/// counter — no UUID dependency needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BackendInstanceId(pub(crate) u64);

#[derive(Debug)]
pub(crate) enum SessionInner {
    Pat { pat_session_id: String },  // Pat-minted session id if/when Pat exposes one
    Native(()),                       // NativeBackend stub never produces one
}

impl Drop for Session {
    fn drop(&mut self) {
        // Local cleanup only. See type-level docstring for rationale.
        // No-op for v0.0.1 PatBackend (Pat sessions are server-tracked).
        // Future native backend may add `inner.local_cleanup()` calls
        // here when the SessionInner variant owns OS resources directly.
    }
}

/// Backend connection status. `#[non_exhaustive]` per v2 P1 #5 — future
/// states (e.g., `Authenticating`, `RetryBackoff { until_iso }`) added
/// without breaking exhaustive matches.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum BackendStatus {
    Disconnected,
    Connecting { transport: String },
    Connected { transport: String, peer: String, since_iso: String },
    Disconnecting,
    Error { reason: String },
}

/// Backend log line. `#[non_exhaustive]` is NOT applied because callers
/// construct test fixtures of this type frequently. Adding fields is a
/// breaking change; accepted.
#[derive(Debug, Clone)]
pub struct LogLine {
    pub timestamp_iso: String,
    pub level: LogLevel,
    pub source: LogSource,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LogLevel { Trace, Debug, Info, Warn, Error }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LogSource { Backend, Pat, Transport, Wire }
```

### 3.3 Error model

Single `BackendError` enum for all trait methods, **`#[non_exhaustive]`** per v2 P1 #5 (future variants land without breaking matches) with **structured source-preserving variants** per v2 P1 #7 for the cases where retry/debug policy benefits from a source chain. Mirrors the `thiserror`-based pattern from `config.rs`'s `ConfigValidationError` / `ConfigReadError` / `ConfigWriteError`.

```rust
use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum BackendError {
    #[error("backend not configured: {0}")]
    NotConfigured(String),

    #[error("message not found: {0:?}")]
    NotFound(MessageId),

    #[error("authentication failed: {reason}")]
    AuthFailed { reason: String },

    /// Wire-level transport failure (CMS connection dropped, modem
    /// returned BUSY, etc.). The `source` chain preserves the underlying
    /// I/O or protocol error for debug + retry policy.
    #[error("transport failed: {reason}")]
    TransportFailed {
        reason: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    },

    #[error("backend rejected message: {0}")]
    MessageRejected(String),

    /// Backend not reachable (sidecar not running, native daemon down,
    /// network unreachable to CMS). `source` carries the underlying
    /// connect-class error when available.
    #[error("backend unavailable: {reason}")]
    BackendUnavailable {
        reason: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    },

    /// Session passed to disconnect was minted by a different backend
    /// instance (per v2 P0 #1).
    #[error("session does not belong to this backend instance")]
    InvalidSession,

    #[error("operation cancelled")]
    Cancelled,

    #[error("not implemented (this backend does not support this operation)")]
    NotImplemented,

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Catch-all. `source` carries the underlying error when available;
    /// the `msg` describes the context in which it occurred.
    #[error("internal error: {msg}")]
    Internal {
        msg: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    },
}
```

**PatBackend translates `PatClientError` → `BackendError`** (no string-only loss; `source` chain preserved):

| `PatClientError` | → | `BackendError` |
|---|---|---|
| `Http(e) where e.is_connect()` | → | `BackendUnavailable { reason: "could not reach Pat HTTP sidecar", source: Some(Box::new(e)) }` |
| `Http(e) where e.is_timeout()` | → | `TransportFailed { reason: "Pat HTTP request timed out", source: Some(Box::new(e)) }` |
| `Http(e)` other | → | `Internal { msg: "Pat HTTP client error", source: Some(Box::new(e)) }` |
| `Status(401)` | → | `AuthFailed { reason: "Pat returned 401" }` |
| `Status(404)` | → | `NotFound(...)` when context is `read_message`; otherwise `Internal { msg: format!("Pat returned 404 for {context}"), source: None }` |
| `Status(other)` | → | `Internal { msg: format!("Pat returned status {n}"), source: None }` |

### 3.4 Async / runtime choice

**Choice: `async-trait`-based async-first.** Rationale (atomic decision; converging here for Codex round):

1. `stream_log() -> BoxStream<...>` requires async — `Stream` is built on `Future`.
2. Tauri 2 commands are `async fn` by convention. Backend methods feeding a Tauri command want to be async to avoid `tokio::task::spawn_blocking` shims at every call site.
3. The future `NativeBackend` is inherently async (network I/O, B2F state machine over async TCP). A sync trait would force `block_on` in native callers, deadlocking under the tokio runtime.
4. `tokio = "1"` is already a dependency.
5. `PatBackend` wraps the existing `reqwest::blocking::Client` via `tokio::task::spawn_blocking` — minor PatBackend-only complexity, contained.

**Trait-object compatibility:** `async-trait = "0.1"` macro generates `Box<dyn Future>`-returning trait methods so `Box<dyn WinlinkBackend>` works for runtime dispatch. **Correction per v2 P2 #10:** Rust 1.75 DOES ship native `async fn in trait` (RFC 3185), but `dyn Trait` object-safety with native async-fn-in-trait is the actual blocker (the compiler emits "dyn-incompatible" errors); `async-trait` remains the right call for tuxlink regardless of MSRV because the trait must be object-safe to flow through `Box<dyn WinlinkBackend>` in the Tauri command surface.

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

### 3.8.0 PatClient extension prerequisites (per v2 P1 #4)

Codex's R1 review correctly flagged that `PatBackend` cannot wrap `PatClient` as-is — the current API surface is too thin. Three extensions land as the first commits of the impl, BEFORE the trait code:

1. **`#[derive(Clone)]` on `PatClient`.** The `reqwest::blocking::Client` field is already `Arc`-backed and `Clone`. The derive is one line. Required because `PatBackend::list_messages` clones the client into a `spawn_blocking` closure.

2. **New `PatClient::read(folder, mid) -> Result<MessageBody, PatClientError>` method.** Wraps Pat's `GET /api/mailbox/{folder}/{mid}` endpoint. Returns the response body as `Vec<u8>` (the trait wraps this into `MessageBody { id, raw_rfc5322 }`). One method, ~15 lines.

3. **`PatClient::send` return-type investigation.** Current signature: `send(...) -> Result<(), PatClientError>` — discards Pat's response body. The Pat 1.0.0 OpenAPI docs SHOULD indicate whether the POST to `/api/mailbox/out` returns the assigned MID. Implementation phase reads Pat's actual response (via integration test against a real Pat 1.0.0 binary). Outcomes:
   - **If Pat returns MID:** change `PatClient::send` to return `Result<String, PatClientError>` (the MID); `PatBackend::send_message` returns `Ok(Some(MessageId(mid)))`.
   - **If Pat does NOT return MID:** keep `PatClient::send` returning `Result<(), PatClientError>`; `PatBackend::send_message` returns `Ok(None)`.
   - Either way, the trait's `Result<Option<MessageId>, BackendError>` signature accommodates the truth as found in code.

These are NOT Codex's authoritative call — per [`feedback_ai_amateur_radio_reliability`](../../../.claude/projects/-home-administrator-Code-tuxlink/memory/feedback_ai_amateur_radio_reliability.md), we don't take AI as authority on Pat HTTP API specifics. The impl-phase investigation IS authoritative.

### 3.8.1 PatProcess::spawn refactor (per v2 P1 #6)

Codex's R1 review flagged that the current `PatProcess::spawn` consumes the Child's stderr in its startup-port-detection loop, drops the `BufReader` when the announce is seen, and leaves no path for `PatBackend` to forward subsequent log lines to subscribers.

**Refactor:** add an optional log sink to `PatSpawnOptions`. After startup-port-detection succeeds, spawn a dedicated thread that drains the remaining stderr into the sink.

```rust
// In src-tauri/src/pat_process.rs:
pub struct PatSpawnOptions {
    pub binary: PathBuf,
    pub config_path: PathBuf,
    pub mbox_dir: PathBuf,
    pub http_listen_port: u16,
    pub pid_file: PathBuf,
    /// NEW (per tuxlink-z5f v2 P1 #6): optional log sink. When `Some`,
    /// stderr lines AFTER the startup-port-detection are forwarded into
    /// the sender. When `None`, behavior is unchanged from pre-z5f
    /// (stderr is silently drained by the OS after spawn returns).
    pub log_sink: Option<std::sync::mpsc::Sender<String>>,
}
```

Inside `spawn`, after the startup detection breaks the loop:

```rust
if let Some(tx) = opts.log_sink {
    std::thread::spawn(move || {
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    if tx.send(l).is_err() {
                        // Receiver dropped; nothing more to do.
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
}
```

The existing `pat_process_test.rs` tests pass `log_sink: None` (or `..Default::default()` once `PatSpawnOptions` gets a `Default` impl); their behavior is unchanged.

`PatBackend::spawn` provides the sink. A `tokio::task::spawn_blocking` task on the PatBackend side drains the sync `mpsc::Receiver<String>` into the `tokio::sync::broadcast::Sender<LogLine>` (after parsing each string into a structured `LogLine`).

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

## 4. Test plan (10 tests)

Tests live in `src-tauri/tests/winlink_backend_test.rs`. Uses `mockito` for HTTP mocking (same pattern as the existing `PatClient` tests would use if they existed; today the `pat_client` module is untested directly — this spec adds trait-level tests that exercise it transitively).

| # | Test name | What it verifies |
|---|---|---|
| 1 | `test_pat_backend_list_messages_returns_mapped_metas` | `PatBackend::list_messages` against a `mockito` server returning the Pat DTO JSON, asserts the returned `Vec<MessageMeta>` is correctly mapped (id, subject, from, date, unread, body_size). |
| 2 | `test_pat_backend_send_message_posts_multipart` | `PatBackend::send_message` POSTs the expected multipart form to `/api/mailbox/out` and returns `Ok(Some(MessageId))` or `Ok(None)` per §3.8.0's send-return-investigation outcome. Mockito matches the request body. |
| 3 | `test_pat_backend_translates_404_to_not_found_for_read` | `PatBackend::read_message` against a mock returning 404 → returns `BackendError::NotFound(_)`. Validates the error-translation table in §3.3. |
| 4 | `test_pat_backend_translates_401_to_auth_failed` | Any method against a mock returning 401 → returns `BackendError::AuthFailed { .. }`. |
| 5 | `test_pat_backend_translates_connect_error_to_backend_unavailable` | `PatBackend` pointed at a closed port → returns `BackendError::BackendUnavailable { source: Some(_), .. }` on any method. Asserts `source.is_some()` to validate v2 P1 #7 source-preservation. |
| 6 | `test_native_backend_returns_not_implemented_for_every_method` | `NativeBackend::new()`, then call each of `list_messages` / `read_message` / `send_message` / `connect` / `disconnect` → all return `BackendError::NotImplemented`. `status()` returns `Disconnected`. `stream_log()` is an empty stream. Asserts no panics. |
| 7 | `test_session_drop_does_not_panic` | Construct a `Session` (via `PatBackend::connect` against a mocked Pat connect endpoint), drop it, assert no panic and that the test completes. Validates the §3.5 Drop contract. |
| 8 | `test_log_stream_emits_lines_and_handles_drop` | Construct a `PatBackend`, subscribe to `stream_log()`, push 3 fake log events via the internal broadcast sender, assert the stream emits them in order. Drop the stream, assert the backend continues running (no panic on dropped receiver). |
| 9 | `test_session_from_other_backend_instance_rejected` (new in v2 per P0 #1) | Create two `PatBackend` instances (`a` and `b`). Mint a `Session` from `a.connect(...)`. Pass it to `b.disconnect(session)` → returns `BackendError::InvalidSession`. Validates the backend-instance-id check. |
| 10 | `test_message_body_preserves_bytes` (new in v2 per P0 #2) | Configure `PatClient::read` mock to return a body with non-UTF-8 bytes (e.g., `[0x48, 0x69, 0xff, 0xfe]`). Assert `PatBackend::read_message` returns `MessageBody { raw_rfc5322: <exact bytes>, .. }`. Validates byte-fidelity at the trait boundary. |

**Why only 10 tests (not 24+):** per [`feedback_discipline_triage_rule`](../../../.claude/projects/-home-administrator-Code-tuxlink/memory/feedback_discipline_triage_rule.md), this is tightly-scoped architectural work. The trait's behavior contract is small (each method has one happy path + one error-class mapping); deeper tests belong to the future native backend (B2F parser tests, state-machine tests, parity-vs-Pat tests). At v0.5 Step 2, the `NativeBackend` stub gets replaced with real logic, and that PR carries the deeper test suite.

---

## 5. Wave-2+ extensibility notes (informational; NOT in this PR)

These notes document the trait's forward-compat surface so future steps don't redesign it:

1. **Adding a new transport (Packet, VARA HF, etc.):** add a variant to `TransportConfig`. Existing callers compile unchanged (the `#[non_exhaustive]` attribute warns them their `match` is non-exhaustive but doesn't break it).
2. **Adding a backend with restricted capabilities:** split the trait into `MessageStore: Send + Sync` + `SessionControl` + `LogStream`, then `trait WinlinkBackend: MessageStore + SessionControl + LogStream {}` is a blanket marker. Existing impls satisfy it automatically.
3. **Backwards-incompat surface change:** if v0.5 needs to add an argument to `send_message`, deprecate `send_message` with a `#[deprecated]` attribute and add `send_message_v2`. Existing callers continue compiling; new callers use the new API. Remove the old method in a major version (v0.6+).
4. **Adding a backend-config selector:** introduce `enum BackendKind { Pat, Native }` in `config.rs`, gate startup-time selection on it. Out of scope for this PR but cleared by the trait existing.
5. **Mocking for end-to-end tests:** a `MockBackend` is trivial to add later — implement the trait with `Vec`-backed in-memory storage. Out of scope for this PR per §2.2.

---

## 6. Open questions — RESOLVED in v2

The 8 open questions enumerated in v1 §6 were the Codex R1 converge targets. All 8 are now resolved; see the v2-revision change table at the top of this document for the disposition of each. The raw Codex R1 transcript is at `dev/adversarial/2026-05-18-tuxlink-z5f-winlink-backend-trait-codex-r1.md` (gitignored).

Quick summary of dispositions:

| v1 §6 question | Disposition in v2 |
|---|---|
| 1. Send + Sync bounds | Ratified; added MutexGuard-not-across-await discipline note (§3.1) |
| 2. async-trait overhead | Ratified; no hot path expected |
| 3. Session backend affinity | **Tightened** — Session now carries `BackendInstanceId`; `disconnect` returns `InvalidSession` for cross-backend sessions (§3.2, §3.3, test #9) |
| 4. MessageBody String vs Vec<u8> | **Changed to `Vec<u8>`** — native backend needs byte fidelity for MIME (§3.2, test #10) |
| 5. status() non-async | Ratified; added cache-and-update discipline note (§3.1) |
| 6. date type | Mis-framed per Codex; kept as `String` with RFC 3339 UTC spec + backend-side validation requirement (§3.2) |
| 7. stream_log() non-async | Ratified; standard `fn -> Stream` pattern |
| 8. NativeBackend stub typed errors | Ratified |

Plus 3 additional concerns raised by Codex and applied:
- `BroadcastStream` dependency missing → added `tokio-stream` (§2.1)
- PatClient gaps → enumerated extension prerequisites (§3.8.0)
- PatProcess stderr lifecycle → refactor outlined (§3.8.1)
- Public-type forward-compat → `#[non_exhaustive]` added to BackendError, BackendStatus, LogSource, LogLevel, MessageMeta, MessageBody, MailboxFolder (§3.2, §3.3)
- BackendError source preservation → structured `source: Option<Box<dyn Error>>` variants (§3.3)

No additional Codex rounds planned — bd-issue scope is "≥1 round, NOT 5"; one round delivered high-signal P0+P1 fixes that justify moving to impl.

---

## 7. Revision log

| Version | Date | Author | Change summary |
|---|---|---|---|
| v1 | 2026-05-18 | badger-oak-dahlia | Initial spec — pre-adrev |
| v2 | 2026-05-18 | badger-oak-dahlia | Codex R1 cross-provider review applied: 3 P0 + 4 P1 + 4 P2 + 1 P3 fixes. Session carries backend-instance id; MessageBody is Vec<u8>; tokio-stream dep added; PatClient + PatProcess prerequisite refactors enumerated; BackendError variants made source-preserving; public enums `#[non_exhaustive]`. Test count grew 8 → 10. |
| v3 | 2026-05-18 | badger-oak-dahlia | Impl-phase discovery applied: PatClient converted from `reqwest::blocking::Client` to `reqwest::Client` (async). Reason: `reqwest::blocking::Client` constructs an internal tokio runtime; when used from a `#[tokio::test]` async context, dropping the inner runtime panics with "Cannot drop a runtime in a context where blocking is not allowed." All 9 async tests failed with this panic on v2's blocking impl. The async conversion drops the `spawn_blocking` wrappers in PatBackend (simpler impl), and is the natural fit for Tauri command handlers (which are async-by-default). The "blocking" feature is removed from the reqwest dep; `reqwest::multipart::Form` replaces `reqwest::blocking::multipart::Form`. All `PatClient::*` methods become `async fn`. Existing pat_client tests convert to `#[tokio::test]` + `Server::new_async()` + `create_async()`. No spec-level shape change — just a runtime-class swap that v2's §3.4 ratified as the right call already (the §3.4 note about `spawn_blocking` for blocking Pat work was specifically about the lifecycle ops, not the HTTP client). |

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
