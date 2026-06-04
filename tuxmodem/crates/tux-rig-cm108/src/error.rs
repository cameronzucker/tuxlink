//! Error taxonomy for the CM108 PTT primitive.

use std::io;
use thiserror::Error;

/// All errors this crate can surface.
#[derive(Debug, Error)]
pub enum Cm108Error {
    /// Opening the hidraw device file failed (permission, not found,
    /// not a hidraw device, etc.).
    #[error("failed to open hidraw device {path}: {source}")]
    OpenDevice {
        /// The path that was attempted.
        path: String,
        /// The underlying I/O error.
        #[source]
        source: io::Error,
    },

    /// Writing the HID feature report failed.
    #[error("write to hidraw device failed: {0}")]
    WriteReport(#[source] io::Error),

    /// `write(2)` returned fewer bytes than the report length. The
    /// kernel-side hidraw driver does not split feature reports across
    /// multiple writes, so a short write signals a fundamentally broken
    /// transport — surface it as an error rather than retrying.
    #[error("short write to hidraw device: wrote {wrote} bytes, expected {expected}")]
    ShortWrite {
        /// Bytes the kernel acknowledged.
        wrote: usize,
        /// Bytes we tried to send.
        expected: usize,
    },

    /// The GPIO pin number is out of the 1..=8 range supported by
    /// CM108-family chips.
    #[error("invalid CM108 GPIO pin number {pin}: must be 1..=8")]
    InvalidPin {
        /// The out-of-range pin number the caller supplied.
        pin: u8,
    },
}

/// Result alias for crate operations.
pub type Cm108Result<T> = Result<T, Cm108Error>;
