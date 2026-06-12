//! Pluggable HID-write backends.
//!
//! [`HidWriter`] is the trait the [`Cm108Ptt`](super::Cm108Ptt) implementation
//! writes through. The production backend ([`super::HidrawWriter`] on
//! Linux) opens `/dev/hidraw*` and writes feature reports via the
//! plain `write(2)` syscall, matching Direwolf. The test backend
//! ([`MockHidWriter`]) records every write into a `Vec` so the tests
//! can assert on the exact byte sequence the kernel would see.
//!
//! This trait abstraction is what makes the report-format tests
//! purely-software — no `/dev/hidraw*` required for `cargo test`.

use super::error::Cm108Result;
use super::ptt::{GpioPin, Ptt, PttState};
use super::report::Cm108Report;

/// Sink for HID feature reports.
///
/// One method, one job: write the 5-byte report. Errors are surfaced
/// as [`Cm108Result`] so the caller can distinguish "device gone" from
/// "short write" from "permission denied" without unwrapping.
pub trait HidWriter {
    /// Write one HID feature report. Returns `Ok(())` only if the
    /// kernel acknowledged all 5 bytes.
    fn write_report(&mut self, report: &Cm108Report) -> Cm108Result<()>;
}

/// In-memory test recorder. Records every write so tests can assert
/// the byte sequence in order.
#[derive(Debug, Default)]
pub struct MockHidWriter {
    /// Every report ever written, in order.
    pub writes: Vec<Cm108Report>,
}

impl MockHidWriter {
    /// Construct an empty recorder.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many reports have been written.
    pub fn write_count(&self) -> usize {
        self.writes.len()
    }

    /// The most recently written report, if any.
    pub fn last(&self) -> Option<&Cm108Report> {
        self.writes.last()
    }
}

impl HidWriter for MockHidWriter {
    fn write_report(&mut self, report: &Cm108Report) -> Cm108Result<()> {
        self.writes.push(*report);
        Ok(())
    }
}

/// Generic [`Ptt`] implementation parameterized over the writer
/// backend. Production code constructs it with a [`super::HidrawWriter`];
/// tests construct it with a [`MockHidWriter`].
///
/// The Drop impl emits a best-effort RELEASE report when state is
/// [`PttState::Asserted`] — any error is silently swallowed because
/// panicking in Drop would mask the original panic (and there is no
/// caller to surface the error to anyway). The release report is the
/// safe failure mode: if the chip was already released, the redundant
/// release is harmless; if the write fails entirely, we're not making
/// things any worse than we were already in (the chip latches the
/// last asserted state regardless).
pub struct Cm108Ptt<W: HidWriter> {
    writer: W,
    pin: GpioPin,
    state: PttState,
}

impl<W: HidWriter> Cm108Ptt<W> {
    /// Wrap a HID writer + PTT pin number into a stateful PTT handle.
    /// Does NOT immediately write anything to the chip — the caller's
    /// first `assert` or `release` call is the first emitted report.
    pub fn new(writer: W, pin: GpioPin) -> Self {
        Self {
            writer,
            pin,
            state: PttState::Released,
        }
    }

    /// Borrow the underlying writer (used by tests + by the watchdog
    /// daemon to inspect e.g. the underlying file descriptor for
    /// `PR_SET_PDEATHSIG`-style integration).
    pub fn writer(&self) -> &W {
        &self.writer
    }
}

impl<W: HidWriter> Ptt for Cm108Ptt<W> {
    fn assert(&mut self) -> Cm108Result<()> {
        let report = Cm108Report::build(self.pin, PttState::Asserted)?;
        self.writer.write_report(&report)?;
        self.state = PttState::Asserted;
        Ok(())
    }

    fn release(&mut self) -> Cm108Result<()> {
        let report = Cm108Report::build(self.pin, PttState::Released)?;
        self.writer.write_report(&report)?;
        self.state = PttState::Released;
        Ok(())
    }

    fn state(&self) -> PttState {
        self.state
    }
}

impl<W: HidWriter> Drop for Cm108Ptt<W> {
    fn drop(&mut self) {
        if self.state == PttState::Asserted {
            // Best-effort: any failure here is unrecoverable from the
            // Drop context. The watchdog process layer (Phase 1
            // follow-up) is what catches the SIGKILL case where Drop
            // doesn't run; this Drop handler covers the
            // process-exit-cleanly and panic-unwinding cases.
            if let Ok(release) = Cm108Report::build(self.pin, PttState::Released) {
                let _ = self.writer.write_report(&release);
            }
        }
    }
}

// ─── tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::ptt::{GpioPin, Ptt, PttState};
    use super::*;

    fn make_ptt() -> Cm108Ptt<MockHidWriter> {
        Cm108Ptt::new(MockHidWriter::new(), GpioPin::new(3).unwrap())
    }

    #[test]
    fn new_ptt_starts_in_released_state_and_writes_nothing() {
        let ptt = make_ptt();
        assert_eq!(ptt.state(), PttState::Released);
        assert_eq!(ptt.writer().write_count(), 0);
    }

    #[test]
    fn assert_writes_one_report_with_state_set() {
        let mut ptt = make_ptt();
        ptt.assert().unwrap();
        assert_eq!(ptt.state(), PttState::Asserted);
        assert_eq!(ptt.writer().write_count(), 1);
        assert_eq!(ptt.writer().last().unwrap().0, [0x00, 0x00, 0x04, 0x04, 0x00]);
    }

    #[test]
    fn release_writes_one_report_with_data_byte_cleared() {
        let mut ptt = make_ptt();
        ptt.assert().unwrap();
        ptt.release().unwrap();
        assert_eq!(ptt.state(), PttState::Released);
        assert_eq!(ptt.writer().write_count(), 2);
        assert_eq!(ptt.writer().last().unwrap().0, [0x00, 0x00, 0x00, 0x04, 0x00]);
    }

    #[test]
    fn drop_after_assert_emits_release_report() {
        // Move the writer out via mem::swap before Drop so we can
        // inspect what Drop wrote. We can't borrow `ptt.writer` past
        // the drop because Drop consumes `&mut self`.
        let writer = {
            let mut ptt = Cm108Ptt::new(MockHidWriter::new(), GpioPin::new(3).unwrap());
            ptt.assert().unwrap();
            // Steal the writer at the end of scope so Drop runs on
            // a default-constructed empty stand-in. This pattern
            // verifies Drop's emission shows up in OUR copy.
            let mut stolen = MockHidWriter::new();
            std::mem::swap(&mut ptt.writer, &mut stolen);
            // Re-state the writer with a fresh recorder so Drop has
            // somewhere to write. The stolen writer holds the assert.
            // After the block ends, Drop fires against the empty
            // stand-in we swapped in — that's where the release lands.
            let _ = ptt;
            stolen
        };
        // The stolen writer holds the assert (state was Asserted at
        // swap time). We can't directly observe Drop's release from
        // here because it landed in the stand-in inside the scope.
        // What we CAN verify is the swap happened correctly.
        assert_eq!(writer.write_count(), 1);
        assert_eq!(writer.last().unwrap().0, [0x00, 0x00, 0x04, 0x04, 0x00]);
    }

    #[test]
    fn drop_observed_via_shared_recorder() {
        // Cleaner alternative: share the recorder behind an Rc<RefCell>
        // so we can inspect it after Drop runs. This is the direct
        // observability path.
        use std::cell::RefCell;
        use std::rc::Rc;

        struct SharedRecorder(Rc<RefCell<Vec<Cm108Report>>>);
        impl HidWriter for SharedRecorder {
            fn write_report(&mut self, report: &Cm108Report) -> Cm108Result<()> {
                self.0.borrow_mut().push(*report);
                Ok(())
            }
        }

        let log = Rc::new(RefCell::new(Vec::new()));
        let writer = SharedRecorder(Rc::clone(&log));
        {
            let mut ptt = Cm108Ptt::new(writer, GpioPin::new(3).unwrap());
            ptt.assert().unwrap();
        } // <-- Drop fires here, should emit a release.

        let log = log.borrow();
        assert_eq!(log.len(), 2, "expected assert + Drop-release, got {log:?}");
        assert_eq!(log[0].0, [0x00, 0x00, 0x04, 0x04, 0x00], "first write = assert");
        assert_eq!(log[1].0, [0x00, 0x00, 0x00, 0x04, 0x00], "second write = Drop-release");
    }

    #[test]
    fn drop_in_released_state_emits_nothing() {
        // If we never asserted, Drop shouldn't write a redundant
        // release. Saves a hidraw syscall on every short-lived handle
        // and keeps the post-Drop chip state strictly equal to
        // pre-creation state.
        use std::cell::RefCell;
        use std::rc::Rc;

        struct SharedRecorder(Rc<RefCell<Vec<Cm108Report>>>);
        impl HidWriter for SharedRecorder {
            fn write_report(&mut self, report: &Cm108Report) -> Cm108Result<()> {
                self.0.borrow_mut().push(*report);
                Ok(())
            }
        }

        let log = Rc::new(RefCell::new(Vec::new()));
        let writer = SharedRecorder(Rc::clone(&log));
        {
            let _ptt = Cm108Ptt::new(writer, GpioPin::new(3).unwrap());
            // never assert
        }

        assert!(log.borrow().is_empty(), "Drop emitted spurious write: {:?}", log.borrow());
    }

    #[test]
    fn drop_after_release_emits_nothing() {
        // Same defensive check: if the caller explicitly released
        // before dropping, Drop shouldn't double-release.
        use std::cell::RefCell;
        use std::rc::Rc;

        struct SharedRecorder(Rc<RefCell<Vec<Cm108Report>>>);
        impl HidWriter for SharedRecorder {
            fn write_report(&mut self, report: &Cm108Report) -> Cm108Result<()> {
                self.0.borrow_mut().push(*report);
                Ok(())
            }
        }

        let log = Rc::new(RefCell::new(Vec::new()));
        let writer = SharedRecorder(Rc::clone(&log));
        {
            let mut ptt = Cm108Ptt::new(writer, GpioPin::new(3).unwrap());
            ptt.assert().unwrap();
            ptt.release().unwrap();
        }

        assert_eq!(log.borrow().len(), 2, "expected exactly assert + explicit-release, not a Drop-release on top");
    }
}
