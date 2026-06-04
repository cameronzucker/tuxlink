//! tux-rig-rts — serial-RTS PTT primitive.
//!
//! Toggles the RTS modem-control line on a USB-serial device as a
//! PTT signal. This is the PTT mechanism the **Digirig Mobile** uses
//! (and the **Digirig Lite**, plus any custom CP2102-class adapter
//! wired the same way), and is the path the operator's Xiegu G90 +
//! Digirig bench setup uses.
//!
//! ## What this is and isn't
//!
//! The crate does NOT do serial communication. It opens
//! `/dev/ttyUSB*`, configures the line discipline to "ignore the
//! modem inputs / no flow control / raw" so opening + closing don't
//! cause spurious assertions, and then exposes assert/release via
//! the `TIOCMBIS` / `TIOCMBIC` ioctls which set or clear the RTS
//! bit of the modem-control register. No `read` or `write` happens
//! on the fd; baud rate doesn't matter.
//!
//! The crate does NOT cover CAT-PTT (sending a `TX` command over the
//! serial port to a radio that exposes CAT-PTT). That's a separate
//! backend per radio family (G90 CI-V, Yaesu CAT, Icom CI-V, etc.)
//! filed as future tux-rig backends — see ADR 0015 and the
//! `tuxlink-5jb` rig-control plane research.
//!
//! ## Why RTS, not the higher-level `serialport` crate
//!
//! The popular `serialport` crate wraps termios for baud-rate-aware
//! communication. Our use case is the opposite: we never communicate.
//! Asking `serialport` to open without a baud is awkward, and
//! pulling in its dependencies for a single ioctl is unnecessary.
//! Direct `libc::ioctl` on the fd is fewer lines and cleaner.
//!
//! ## Watched failure mode: spurious-key-on-open
//!
//! Opening `/dev/ttyUSB*` on Linux historically asserts DTR (and on
//! some configurations, RTS too) as a vestige of modem-era serial
//! semantics. If the radio interprets either line as PTT, the
//! radio keys at the moment our process opens the device. We
//! defensively clear both DTR and RTS in [`LinuxTty::open`] via
//! [`libc::TIOCMBIC`] **before any other operation** so the caller's
//! first observable state is "PTT released."
//!
//! A regression test pins this: the very first ioctl issued must
//! clear `TIOCM_RTS | TIOCM_DTR`, observed via [`MockTtyWriter`].
//!
//! ## Cross-platform
//!
//! Linux only for v1 (the operator's target). macOS exposes the same
//! ioctls (BSD-style); Windows uses `EscapeCommFunction(SETRTS/CLRRTS)`.
//! Both are future slices.

// `deny`, not `forbid`, because [`linux`] needs `unsafe` for the
// `libc::ioctl` calls — there's no safe Rust wrapper for
// `TIOCMBIS`/`TIOCMBIC` that doesn't pull in heavy deps. Every
// other module is `forbid(unsafe_code)` via local attribute or by
// construction (no `unsafe` blocks present).
#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod ptt;
pub mod watchdog;
pub mod writer;

#[cfg(target_os = "linux")]
pub mod linux;

pub use error::{RtsError, RtsResult};
pub use ptt::{Ptt, PttState};
pub use watchdog::{run_watchdog, WatchdogOutcome, DEFAULT_MAX_DURATION, HARD_CAP_DURATION};
pub use writer::{MockTtyWriter, RtsPtt, TtyOp, TtyWriter};

#[cfg(target_os = "linux")]
pub use linux::LinuxTty;
