//! Soundcard modem integration.
//!
//! This module hosts the managed-spawn / external-TCP-modem client layer
//! (ADR 0015 decisions #1 and #2). Each supported modem is a submodule that
//! implements [`ModemTransport`], driving the modem's TCP host protocol while
//! tuxlink owns the modem process lifecycle (spawn / supervise / SIGINT-clean-stop
//! / audio-device-release gate before swap).
//!
//! The concurrency model is synchronous `std::io` + `std::thread` — no Tokio
//! anywhere in this subtree (see plan concurrency-architecture note and
//! ADR 0015). Phase 1 (wire codec) is pure functions/structs; threads and
//! TCP connections arrive in Phase 2.

use std::time::Duration;

// Re-export the types that appear in the ModemTransport signature so callers
// can use the trait without importing ardop internals directly.
pub use ardop::session::{ConnectInfo, InitConfig, SessionError};

pub mod ardop;
pub mod process;
pub mod vara;

// ─── ReadWrite ──────────────────────────────────────────────────────────────

/// Marker for a duplex byte stream (the connected-mode data path).
///
/// Blanket-implemented for every type that is `Read + Write + Send`, so
/// callers only need to name `dyn ReadWrite` — they never implement this trait
/// themselves.
pub trait ReadWrite: std::io::Read + std::io::Write + Send {}
impl<T: std::io::Read + std::io::Write + Send> ReadWrite for T {}

// ─── ModemTransport ─────────────────────────────────────────────────────────

/// Drive an external soundcard modem over its TCP host protocol.
///
/// Implemented by [`ardop::transport::ArdopTransport`] (ardopcf) today; the
/// same abstraction that future Dire Wolf / VARA / first-party sonde
/// transports will instantiate (ADR 0015 decision #3). Synchronous — no
/// async_trait, no Tokio.
///
/// # Object safety
///
/// The trait is object-safe and works behind `Box<dyn ModemTransport>`.
/// `data_stream` returns `&mut dyn ReadWrite` (a trait-object reference)
/// rather than an associated type, preserving object safety.
pub trait ModemTransport: Send {
    /// Connect to the modem's control socket and run the init sequence.
    ///
    /// Must be called before [`connect_arq`] or [`data_stream`].
    fn init(&mut self, cfg: &InitConfig) -> Result<(), SessionError>;

    /// Initiate an ARQ connection to `target` with `repeat` retries.
    ///
    /// `deadline` is `Option<Duration>`:
    /// - `Some(d)` — bound the entire ARQCALL handshake by wall-clock `d`;
    ///   if no CONNECTED / FAULT / DISCONNECTED arrives in time, return
    ///   `Err(SessionError::Timeout)`. Used by the legacy
    ///   `modem_ardop_connect` (Start-button) path to preserve its
    ///   historical 120s cap.
    /// - `None` — block until the modem emits CONNECTED / FAULT /
    ///   DISCONNECTED (per operator decision bd tuxlink-qtgg + Codex
    ///   Round 1 P1 #3: no tuxlink-added wall-clock cap on the new
    ///   `b2f_exchange` dial path; the bound is the modem's own retry
    ///   logic plus the operator's ABORT side channel). Implementations
    ///   MUST NOT fake `None` via `Duration::MAX` — Codex Round 2 P1 #2
    ///   flagged that `Duration::MAX` overflows
    ///   `mpsc::Receiver::recv_timeout`'s internal `Instant::checked_add`.
    fn connect_arq(
        &mut self,
        target: &str,
        repeat: u32,
        deadline: Option<Duration>,
    ) -> Result<ConnectInfo, SessionError>;

    /// Tear down the ARQ link, bounded by `deadline`.
    fn disconnect(&mut self, deadline: Duration) -> Result<(), SessionError>;

    /// Borrow the connected-mode data byte stream.
    ///
    /// Valid after a successful [`connect_arq`]. The returned `Read + Write`
    /// is what the synchronous B2F `run_exchange` consumes.
    ///
    /// Returns `Err` (not a panic) if [`init`] was never called or if the
    /// data socket is not yet open.
    fn data_stream(&mut self) -> std::io::Result<&mut dyn ReadWrite>;

    /// Drain any pending non-blocking events from the modem and apply them
    /// to the provided [`ModemStatus`]. Called by
    /// [`crate::modem_status::ModemStatusBroadcaster`] on every tick
    /// (250 ms by default).
    ///
    /// Default impl is a no-op for backends that don't emit live events. The
    /// ARDOP transport overrides this to drain its cmd-socket and update
    /// `state` / `arq_flags` / `peer` / `width_hz` / `last_error` in place.
    ///
    /// Implementations MUST NOT block — drain with `Duration::ZERO` (or
    /// equivalent non-blocking receive) and bound the per-call event count
    /// so a runaway emitter cannot starve the broadcaster tick.
    fn drain_status_events(&mut self, _status: &mut crate::modem_status::ModemStatus) {
        // Default: no-op. Backends that emit live status events override.
    }

    /// Return a side-channel writer + hard-close stream pair that another
    /// thread can use to inject an `ABORT`-style command (cooperative path)
    /// or RST the socket (fallback path) while the main transport thread is
    /// blocked inside [`connect_arq`]'s recv loop (tuxlink-o3f2 — P1
    /// abort-during-connect fix; tuxlink-0ye6 Task 4.1 — bounded write +
    /// hard-close fallback per Codex Round 4 P1 #3).
    ///
    /// Implementations MUST set the writer's `write_timeout` to
    /// [`crate::modem_status::ABORT_WRITE_TIMEOUT`] before returning so the
    /// cooperative phase of `abort_in_flight` is bounded at the socket layer.
    /// The session layer relies on the timeout to fit the spec §2 "~2s"
    /// abort contract regardless of how unresponsive the peer is.
    ///
    /// The default impl returns `None` — backends that don't support
    /// side-channel abort fall back to whatever in-line cancellation they
    /// can offer. The ARDOP transport overrides this to expose a
    /// [`std::net::TcpStream`] clone of its cmd-socket write half so
    /// `ModemSession::abort_in_flight` can write `ABORT\r` and unblock the
    /// `arq_connect` recv loop (ardopcf responds with `FAULT`/`NEWSTATE
    /// DISC`, which the cmd reader thread delivers via the channel). The
    /// VARA transport (Task 4.1) does the same with the cmd port and the
    /// VARA-specific `ABORT` host command (distinct from VARA's
    /// `DISCONNECT`, which is graceful).
    fn try_clone_abort_writer(
        &self,
    ) -> Option<(
        Box<dyn std::io::Write + Send>,
        Box<dyn crate::modem_status::ShutdownableStream>,
    )> {
        None
    }

    /// Wait for an inbound CONNECTED event during listener mode.
    ///
    /// Backends that support listener mode (ARDOP, VARA) override this to
    /// poll the cmd-socket / modem-control channel for a `Connected { peer,
    /// bandwidth }` event with a bounded wait. Returns `Ok(Some(info))` when
    /// a peer connects, `Ok(None)` when the wait times out (caller loops),
    /// or `Err` on transport failure (caller exits the listener).
    ///
    /// The default impl returns `Ok(None)` immediately (no listener support).
    /// The ARDOP impl drains the cmd socket, skipping status events and
    /// returning on the first Connected.
    ///
    /// bd: tuxlink-61yg
    fn wait_for_listener_connect(
        &mut self,
        _timeout: std::time::Duration,
    ) -> Result<
        Option<crate::winlink::modem::ardop::session::ConnectInfo>,
        crate::winlink::modem::ardop::session::SessionError,
    > {
        Ok(None)
    }
}
