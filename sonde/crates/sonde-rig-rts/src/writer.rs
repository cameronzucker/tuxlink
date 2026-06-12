//! Pluggable TTY-modem-line backends.
//!
//! [`TtyWriter`] is the trait the [`RtsPtt`] implementation operates
//! through. The production backend ([`super::LinuxTty`] on Linux)
//! holds a raw fd to `/dev/ttyUSB*` and issues `TIOCMBIS` / `TIOCMBIC`
//! ioctls. The test backend ([`MockTtyWriter`]) records every op
//! into a `Vec` so the tests assert on the exact ioctl sequence the
//! kernel would see.
//!
//! This is what makes the open-spurious-key regression test
//! purely-software — no `/dev/ttyUSB*` required for `cargo test`.

use super::error::RtsResult;
use super::ptt::{Ptt, PttState};

/// What modem-line operation the writer is performing.
///
/// `OpenClearBoth` is the always-issued-first op — it explicitly
/// clears BOTH `TIOCM_RTS` and `TIOCM_DTR` to defuse the
/// spurious-key-on-open failure mode (Linux historically asserts
/// DTR on tty open).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtyOp {
    /// Issued exactly once at writer construction. Clears RTS + DTR
    /// before any other writer state.
    OpenClearBoth,
    /// `TIOCMBIS` with `TIOCM_RTS` — set the RTS bit.
    AssertRts,
    /// `TIOCMBIC` with `TIOCM_RTS` — clear the RTS bit. DTR is
    /// untouched (we set it low at open and don't touch it after).
    ReleaseRts,
}

/// Sink for modem-line ops on a TTY device.
///
/// Implementations OWN the underlying file descriptor for their
/// lifetime. Drop closes the fd; the kernel-side serial driver
/// returns its idle state (lines all low on most CP210x/CH340/FTDI
/// adapters, but the spec doesn't pin this — that's why we drop RTS
/// explicitly in [`RtsPtt`]'s `Drop` impl before the writer's fd
/// closes).
pub trait TtyWriter {
    /// Issue one modem-line op against the underlying fd.
    fn modem_op(&mut self, op: TtyOp) -> RtsResult<()>;
}

/// In-memory test recorder. Records every op so tests can assert
/// the order + identity of the ioctls that would be issued.
#[derive(Debug, Default)]
pub struct MockTtyWriter {
    /// Every op ever issued, in order.
    pub ops: Vec<TtyOp>,
}

impl MockTtyWriter {
    /// Construct an empty recorder.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many ops have been issued.
    pub fn op_count(&self) -> usize {
        self.ops.len()
    }

    /// The most recently issued op, if any.
    pub fn last(&self) -> Option<TtyOp> {
        self.ops.last().copied()
    }
}

impl TtyWriter for MockTtyWriter {
    fn modem_op(&mut self, op: TtyOp) -> RtsResult<()> {
        self.ops.push(op);
        Ok(())
    }
}

/// Generic [`Ptt`] implementation parameterized over the writer
/// backend. Production code constructs it with a [`super::LinuxTty`];
/// tests construct it with a [`MockTtyWriter`].
///
/// The Drop impl emits a best-effort `ReleaseRts` op when state is
/// [`PttState::Asserted`] — any error is silently swallowed because
/// panicking in Drop would mask the original panic.
pub struct RtsPtt<W: TtyWriter> {
    writer: W,
    state: PttState,
}

impl<W: TtyWriter> RtsPtt<W> {
    /// Wrap a TTY writer into a stateful PTT handle. Issues the
    /// always-required `OpenClearBoth` op as its first action — this
    /// ensures the writer's tracked state matches the wire state
    /// regardless of what the kernel did on open.
    ///
    /// Returns `Err` if the initial clear op fails. The writer is
    /// dropped (and its fd closed) when this constructor errors.
    pub fn new(mut writer: W) -> RtsResult<Self> {
        writer.modem_op(TtyOp::OpenClearBoth)?;
        Ok(Self {
            writer,
            state: PttState::Released,
        })
    }

    /// Borrow the underlying writer (used by tests + by the future
    /// watchdog daemon to inspect e.g. `PR_SET_PDEATHSIG`-style
    /// integration knobs).
    pub fn writer(&self) -> &W {
        &self.writer
    }
}

impl<W: TtyWriter> Ptt for RtsPtt<W> {
    type Error = super::error::RtsError;

    fn assert(&mut self) -> RtsResult<()> {
        self.writer.modem_op(TtyOp::AssertRts)?;
        self.state = PttState::Asserted;
        Ok(())
    }

    fn release(&mut self) -> RtsResult<()> {
        self.writer.modem_op(TtyOp::ReleaseRts)?;
        self.state = PttState::Released;
        Ok(())
    }

    fn state(&self) -> PttState {
        self.state
    }
}

impl<W: TtyWriter> Drop for RtsPtt<W> {
    fn drop(&mut self) {
        if self.state == PttState::Asserted {
            // Best-effort: failure here is unrecoverable from the
            // Drop context. The watchdog daemon (Phase 1.5) is what
            // catches the SIGKILL case where Drop doesn't run; this
            // Drop covers process-exit-cleanly and panic-unwind.
            let _ = self.writer.modem_op(TtyOp::ReleaseRts);
        }
    }
}

// ─── tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Shared-recorder wrapper so we can inspect `Drop` semantics
    /// after the `RtsPtt` falls out of scope.
    struct SharedRecorder(Rc<RefCell<Vec<TtyOp>>>);
    impl TtyWriter for SharedRecorder {
        fn modem_op(&mut self, op: TtyOp) -> RtsResult<()> {
            self.0.borrow_mut().push(op);
            Ok(())
        }
    }

    #[test]
    fn construction_clears_both_lines_first() {
        // The single most important regression test: ensure the
        // OpenClearBoth op is the FIRST thing emitted, before any
        // assert. Defuses the spurious-key-on-tty-open failure mode.
        let ptt = RtsPtt::new(MockTtyWriter::new()).unwrap();
        assert_eq!(ptt.writer().op_count(), 1);
        assert_eq!(ptt.writer().last(), Some(TtyOp::OpenClearBoth));
        assert_eq!(ptt.state(), PttState::Released);
    }

    #[test]
    fn assert_emits_assert_rts() {
        let mut ptt = RtsPtt::new(MockTtyWriter::new()).unwrap();
        ptt.assert().unwrap();
        assert_eq!(ptt.writer().op_count(), 2);
        assert_eq!(ptt.writer().last(), Some(TtyOp::AssertRts));
        assert_eq!(ptt.state(), PttState::Asserted);
    }

    #[test]
    fn release_emits_release_rts() {
        let mut ptt = RtsPtt::new(MockTtyWriter::new()).unwrap();
        ptt.assert().unwrap();
        ptt.release().unwrap();
        assert_eq!(ptt.writer().op_count(), 3);
        assert_eq!(ptt.writer().last(), Some(TtyOp::ReleaseRts));
        assert_eq!(ptt.state(), PttState::Released);
    }

    #[test]
    fn drop_after_assert_emits_release_observed_via_shared_recorder() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let writer = SharedRecorder(Rc::clone(&log));
        {
            let mut ptt = RtsPtt::new(writer).unwrap();
            ptt.assert().unwrap();
        }
        let log = log.borrow();
        assert_eq!(log.len(), 3, "expected OpenClearBoth + AssertRts + Drop-ReleaseRts, got {log:?}");
        assert_eq!(log[0], TtyOp::OpenClearBoth);
        assert_eq!(log[1], TtyOp::AssertRts);
        assert_eq!(log[2], TtyOp::ReleaseRts);
    }

    #[test]
    fn drop_in_released_state_emits_nothing_extra() {
        // No spurious release on Drop if we never asserted. Saves
        // a kernel round-trip on short-lived handles.
        let log = Rc::new(RefCell::new(Vec::new()));
        let writer = SharedRecorder(Rc::clone(&log));
        {
            let _ptt = RtsPtt::new(writer).unwrap();
            // never assert
        }
        assert_eq!(*log.borrow(), vec![TtyOp::OpenClearBoth]);
    }

    #[test]
    fn drop_after_explicit_release_does_not_double_release() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let writer = SharedRecorder(Rc::clone(&log));
        {
            let mut ptt = RtsPtt::new(writer).unwrap();
            ptt.assert().unwrap();
            ptt.release().unwrap();
        }
        assert_eq!(
            *log.borrow(),
            vec![TtyOp::OpenClearBoth, TtyOp::AssertRts, TtyOp::ReleaseRts],
            "expected exactly OpenClearBoth + Assert + explicit-Release; Drop must not double-release",
        );
    }

    #[test]
    fn assert_release_cycle_can_repeat() {
        let mut ptt = RtsPtt::new(MockTtyWriter::new()).unwrap();
        for _ in 0..3 {
            ptt.assert().unwrap();
            assert_eq!(ptt.state(), PttState::Asserted);
            ptt.release().unwrap();
            assert_eq!(ptt.state(), PttState::Released);
        }
        // OpenClearBoth + 3 × (Assert + Release) = 7 ops.
        assert_eq!(ptt.writer().op_count(), 7);
    }
}
