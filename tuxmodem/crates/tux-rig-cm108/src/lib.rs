//! tux-rig-cm108 — CM108-family USB-HID PTT primitive.
//!
//! Asserts/releases the PTT GPIO line on a C-Media CM108 / CM119 /
//! CM119A USB sound-card chip via the standard 5-byte HID feature
//! report. This is the bench-bring-up safety primitive that gates all
//! subsequent autonomous TX work in the tuxmodem program (Phase 1 of
//! tuxlink-u1js / tuxlink-9ggl).
//!
//! ## What this controls
//!
//! C-Media CM108-family USB audio codec chips (CM108, CM108AH, CM108B,
//! CM109, CM119, CM119A, CM119B, plus the SSS1621/1623 work-alikes and
//! the AIOC emulator) expose an HID interface alongside their USB
//! audio interfaces. The HID interface exposes a handful of GPIO pins
//! addressable via a 5-byte HID feature report — the same mechanism
//! Direwolf, Hamlib's `cm108` rig type, and fldigi's "C-Media GPIO PTT"
//! all use. On the Masters Communications DRA-100-DIN6 adapter
//! (the project's reference bench rig per
//! [`docs/hardware/modem-test-rig.md`]), the chip's GPIO3 is wired
//! through a 2N2222 buffer to the radio's PTT pin.
//!
//! ## Report format
//!
//! Per Direwolf's `cm108.c` (GPL-3, AGPL-3-compatible; this layout was
//! NOT hand-recited from memory because per the bench-rig spec
//! "CM108/CM119/CM119A report layouts differ subtly"):
//!
//! ```text
//! byte 0:  0x00      (reserved)
//! byte 1:  0x00      (reserved)
//! byte 2:  iodata    (GPIO output state — bit N-1 = pin-N high)
//! byte 3:  iomask    (GPIO direction mask — bit N-1 = pin-N is output)
//! byte 4:  0x00      (reserved)
//! ```
//!
//! For GPIO pin 3 (the DRA-100-DIN6 PTT pin):
//! - assert → `iodata=0x04, iomask=0x04` (bit 2 = high, output)
//! - release → `iodata=0x00, iomask=0x04` (bit 2 = low, still output)
//!
//! Sent via plain `write(2)` to `/dev/hidraw*` (NOT `HIDIOCSFEATURE`),
//! per Direwolf's choice — kernels going back ~10 years accept both
//! but `write` is the maximally compatible path.
//!
//! ## State latching — why we need Drop release
//!
//! The CM119A holds its last commanded GPIO state if the controlling
//! process dies. A modem panic with PTT asserted leaves the radio
//! transmitting until manually intervened. Two countermeasures here:
//!
//! 1. [`Cm108Ptt`]'s [`Drop`] impl explicitly writes a release report.
//!    This handles process exit, panics that unwind, and any path
//!    where Rust runs destructors.
//! 2. The CLI binary installs a SIGTERM/SIGINT handler that releases
//!    before exiting. This handles signal-induced exits where
//!    destructors might not run.
//!
//! SIGKILL still leaves PTT stuck — that's why the full bring-up plan
//! (tuxlink-9ggl) escalates to a separate watchdog process that owns
//! the hidraw fd. This crate is the primitive that watchdog will use;
//! the watchdog itself is a follow-up child issue.
//!
//! ## Cross-platform
//!
//! Linux only for now (`/dev/hidraw*`). macOS + Windows use `hidapi`
//! which Direwolf also supports; deferred to a future slice.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod ptt;
pub mod report;
pub mod writer;

#[cfg(target_os = "linux")]
pub mod hidraw;

pub use error::{Cm108Error, Cm108Result};
pub use ptt::{GpioPin, Ptt, PttState};
pub use report::{Cm108Report, REPORT_SIZE};
pub use writer::{Cm108Ptt, HidWriter, MockHidWriter};

#[cfg(target_os = "linux")]
pub use hidraw::HidrawWriter;
