//! ARDOP (Amateur Radio Digital Open Protocol) modem client.
//!
//! Drives ardopcf (or any ARDOP-compatible TNC) over two TCP sockets:
//! - cmd socket (default 8515): `\r`-terminated ASCII command lines.
//! - data socket (default 8516): `[u16 BE length][3-byte type][payload]` inbound;
//!   raw bytes outbound (the TNC frames them for TX).
//!
//! This module is Phase 1 of ADR 0015 decision #2 (generic external-TCP-modem
//! client). Phase 1 = wire codec only (pure functions/structs, no I/O).
//! Phase 2 adds TCP sockets and `std::thread`-based concurrency.
//! Phase 5 adds managed-spawn integration (`ArdopConfig` + `with_managed_modem`
//! + `shutdown`) so tuxlink owns the ardopcf process lifecycle.

use std::path::PathBuf;

pub mod arq_state;
pub mod b2f;
pub mod command;
pub mod data;
pub mod frame;
pub mod session;
pub mod transport;
pub mod wire;

// ─── ArdopConfig ─────────────────────────────────────────────────────────────

/// Configuration for launching and connecting to an ardopcf (or compatible)
/// TNC process managed by tuxlink.
///
/// Used with [`transport::ArdopTransport::with_managed_modem`].
///
/// # RADIO-1
///
/// Building an `ArdopConfig` and calling `with_managed_modem` will spawn the
/// TNC binary. The caller must obtain per-invocation operator consent before
/// doing so (Part 97 requirement). Tests in this subtree use harmless stubs.
#[derive(Debug, Clone)]
pub struct ArdopConfig {
    /// Path to the ardopcf (or compatible) binary.
    pub binary: PathBuf,
    /// All arguments passed to the binary.
    ///
    /// For ardopcf the conventional call is `ardopcf <cmd_port> <capture> <playback>`,
    /// so a typical value is `vec!["8515".into(), "plughw:1,0".into(), "plughw:1,0".into()]`.
    /// PTT flags (`-p /dev/ttyUSB0`) can be prepended.
    ///
    /// `cmd_port` and `data_port` on this struct are used only for the bind-wait
    /// and transport socket addresses; they are not injected into `extra_args`
    /// automatically.
    pub extra_args: Vec<String>,
    /// TCP port for the ARDOP command socket (ardopcf default: 8515).
    pub cmd_port: u16,
    /// TCP port for the ARDOP data socket (ardopcf default: 8516 = cmd_port + 1).
    pub data_port: u16,
    /// Optional path to the audio device file (e.g. `/dev/snd/pcmC1D0c`) used
    /// for the ADR-0015 audio-device-release check inside `shutdown`.
    ///
    /// When `Some`, `shutdown` will call
    /// [`process::ManagedModem::confirm_audio_device_released`] before returning.
    pub audio_device_path: Option<PathBuf>,
}
