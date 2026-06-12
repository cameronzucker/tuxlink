//! PHY error taxonomy.

use thiserror::Error;

/// Top-level PHY error.
#[derive(Debug, Error)]
pub enum PhyError {
    /// Frame detection failed (no preamble found within deadline).
    #[error("frame detection failed: {0}")]
    FrameDetect(String),
    /// Synchronization failed (CFO / symbol timing / frame sync).
    #[error("sync failed: {0}")]
    Sync(String),
    /// Mode selection invalid for current channel measurement.
    #[error("mode unavailable: {0}")]
    ModeUnavailable(String),
    /// Underlying FEC layer reported a decode failure.
    #[error("fec decode failed: {0}")]
    FecDecode(String),
    /// Audio I/O error.
    #[error("audio io: {0}")]
    AudioIo(String),
    /// Payload exceeds the selected mode's frame capacity.
    #[error("payload too large: {actual} bytes > {capacity}")]
    PayloadTooLarge {
        /// Actual payload size in bytes.
        actual: usize,
        /// Mode's per-frame capacity in bytes.
        capacity: usize,
    },
}
