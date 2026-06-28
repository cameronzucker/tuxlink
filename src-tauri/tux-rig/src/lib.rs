//! tux-rig — CAT rig control over a managed `rigctld` subprocess.
//!
//! Owns frequency/mode tuning and manual/Tune-only PTT. The live ARDOP ARQ
//! session keys PTT via ardopcf's own path, not this crate.

use std::fmt;

mod mode;
pub use mode::Mode;

mod protocol;

mod client;
pub use client::RigctldClient;

mod managed;
pub use managed::{ManagedRig, RigConfig};

mod list;
pub use list::{list_models, RigModel};

/// A snapshot of rig state from `read_status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RigStatus {
    pub freq_hz: u64,
    pub mode: Option<Mode>,
    pub ptt: bool,
}

/// Frequency/mode/PTT control of a radio.
pub trait Rig {
    fn set_freq(&mut self, hz: u64) -> Result<(), RigError>;
    fn set_mode(&mut self, mode: Mode) -> Result<(), RigError>;
    fn ptt(&mut self, on: bool) -> Result<(), RigError>;
    fn read_status(&mut self) -> Result<RigStatus, RigError>;
}

/// Errors from rig control.
#[derive(Debug)]
pub enum RigError {
    /// Underlying I/O (socket connect, read, write).
    Io(std::io::Error),
    /// rigctld returned a non-zero `RPRT` code.
    Rprt(i32),
    /// A response could not be parsed into the expected shape.
    Protocol(String),
    /// Spawning / supervising the rigctld subprocess failed.
    Spawn(String),
}

impl fmt::Display for RigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RigError::Io(e) => write!(f, "rig I/O error: {e}"),
            RigError::Rprt(code) => write!(f, "rigctld returned RPRT {code}"),
            RigError::Protocol(s) => write!(f, "rig protocol error: {s}"),
            RigError::Spawn(s) => write!(f, "rigctld spawn error: {s}"),
        }
    }
}

impl std::error::Error for RigError {}

impl From<std::io::Error> for RigError {
    fn from(e: std::io::Error) -> Self {
        RigError::Io(e)
    }
}
