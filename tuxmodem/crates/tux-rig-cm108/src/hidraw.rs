//! Real `/dev/hidraw*` backend.
//!
//! Linux-only. The kernel exposes each USB-HID device as a
//! `/dev/hidraw{N}` character device; CM108-family chips show up
//! alongside their ALSA card. The operator pins a stable symlink
//! (e.g. `/dev/dra100-ptt`) via a udev rule keyed on the CM119A's
//! USB VID:PID — see `docs/hardware/modem-test-rig.md` for the
//! one-time setup.
//!
//! We use plain `write(2)` rather than `HIDIOCSFEATURE`. The kernel
//! hidraw driver accepts both for feature reports, but Direwolf has
//! used the write-syscall path since ~2014 across millions of
//! deployments with every CM108-family revision — that's the path of
//! least surprise.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;

use super::error::{Cm108Error, Cm108Result};
use super::report::{Cm108Report, REPORT_SIZE};
use super::writer::HidWriter;

/// Linux hidraw backend.
///
/// Owns the file descriptor for the lifetime of the writer. Dropping
/// the writer closes the fd, which on the kernel side reverts hidraw
/// to a no-current-owner state but does NOT change the chip's GPIO
/// latches — that's the [`crate::Cm108Ptt`] Drop's job. Don't rely on
/// fd-close to release PTT.
pub struct HidrawWriter {
    file: File,
    path: String,
}

impl HidrawWriter {
    /// Open the hidraw device at `path`. Operator typically points
    /// this at a udev symlink like `/dev/dra100-ptt` so the udev rule
    /// can survive USB re-enumeration.
    ///
    /// Opens `O_WRONLY` per Direwolf (`cm108.c:open(name, O_WRONLY)`).
    /// We never read from the hidraw fd — the only kernel-side
    /// behavior we depend on is `write` of a 5-byte feature report.
    pub fn open(path: impl AsRef<Path>) -> Cm108Result<Self> {
        let path_ref = path.as_ref();
        let path_str = path_ref.display().to_string();
        let file = OpenOptions::new()
            .write(true)
            .read(false)
            .open(path_ref)
            .map_err(|source| Cm108Error::OpenDevice {
                path: path_str.clone(),
                source,
            })?;
        Ok(Self { file, path: path_str })
    }

    /// The path this writer was opened with (for diagnostics).
    pub fn path(&self) -> &str {
        &self.path
    }
}

impl HidWriter for HidrawWriter {
    fn write_report(&mut self, report: &Cm108Report) -> Cm108Result<()> {
        let bytes = report.as_bytes();
        let wrote = self
            .file
            .write(bytes)
            .map_err(Cm108Error::WriteReport)?;
        if wrote != REPORT_SIZE {
            return Err(Cm108Error::ShortWrite {
                wrote,
                expected: REPORT_SIZE,
            });
        }
        // Don't flush — hidraw doesn't buffer feature reports. A
        // `write` that returns the full byte count means the kernel
        // has already dispatched the URB.
        Ok(())
    }
}
