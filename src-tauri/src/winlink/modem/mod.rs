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
/// same abstraction that future Dire Wolf / VARA / first-party tuxmodem
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

    /// Initiate an ARQ connection to `target` with `repeat` retries, bounded
    /// by `deadline`.
    fn connect_arq(
        &mut self,
        target: &str,
        repeat: u32,
        deadline: Duration,
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
}
