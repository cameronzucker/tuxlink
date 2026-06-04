//! Error taxonomy for the serial-RTS PTT primitive.

use std::io;
use thiserror::Error;

/// All errors this crate can surface.
#[derive(Debug, Error)]
pub enum RtsError {
    /// Opening the tty device file failed (permission, not found,
    /// not a tty, etc.).
    #[error("failed to open tty device {path}: {source}")]
    OpenDevice {
        /// The path that was attempted.
        path: String,
        /// The underlying I/O error.
        #[source]
        source: io::Error,
    },

    /// The `TIOCMGET` / `TIOCMBIS` / `TIOCMBIC` ioctl returned -1.
    /// Underlying `io::Error` carries the errno.
    #[error("modem-line ioctl failed: {0}")]
    ModemLineIoctl(#[source] io::Error),

    /// Configuring termios after open failed (`tcgetattr` or
    /// `tcsetattr`). On a real serial device this is essentially
    /// "the kernel rejected our line-discipline request" — surface
    /// it rather than proceed with a possibly-noisy line.
    #[error("termios configuration failed: {0}")]
    TermiosConfig(#[source] io::Error),
}

/// Result alias for crate operations.
pub type RtsResult<T> = Result<T, RtsError>;
