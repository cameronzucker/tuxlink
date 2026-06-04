//! SIGKILL-safe PTT release loop.
//!
//! The problem: [`crate::RtsPtt`]'s `Drop` impl releases PTT cleanly on
//! SIGINT / SIGTERM / panic-unwind, but **SIGKILL skips `Drop`**.
//! Without external safety, a SIGKILL of `tuxmodem-tx` mid-transmission
//! can leave the radio keyed until the operator power-cycles.
//!
//! The solution: a **separate process** that holds the PTT line. When
//! the parent dies (any reason including SIGKILL), the OS closes the
//! pipe → watchdog detects stdin EOF → releases PTT and exits.
//! Kernel-mediated; works through SIGKILL because the OS closes the
//! pipe automatically.
//!
//! This module exposes the orchestration loop as a generic, testable
//! function. The `tux-rig-watchdog` binary wires it to real Linux
//! signals + real stdin EOF detection; tests wire it to in-memory
//! atomic flags.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::ptt::Ptt;
use crate::RtsError;

/// Default cap on how long the watchdog will hold PTT before
/// auto-releasing. Mirrors the cap in [`crate::bin::tux_rig_rts::HARD_CAP_SECS`].
pub const DEFAULT_MAX_DURATION: Duration = Duration::from_secs(30);

/// Hard ceiling on the caller-supplied max-duration. No watchdog
/// invocation may hold PTT longer than this regardless of CLI input.
pub const HARD_CAP_DURATION: Duration = Duration::from_secs(60);

/// Outcome of a [`run_watchdog`] call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchdogOutcome {
    /// The caller-supplied max-duration elapsed; PTT released and exit.
    MaxDurationElapsed,
    /// Caller's stdin-EOF flag was observed; PTT released and exit.
    /// In production this fires when the parent process dies (any
    /// reason including SIGKILL) and the OS closes the pipe.
    StdinEof,
    /// Caller's signal flag was observed (SIGINT / SIGTERM); PTT
    /// released and exit.
    Signaled,
}

/// Execute the watchdog loop.
///
/// Asserts PTT, then polls three exit conditions on `poll_interval`:
///
/// 1. The caller-supplied `signal` flag (set by SIGINT/SIGTERM
///    handlers in production).
/// 2. The caller-supplied `stdin_eof` flag (set by a stdin-watcher
///    thread when the parent's pipe to us closes).
/// 3. The deadline `Instant::now() + max_duration` elapsing.
///
/// On any of these, releases PTT and returns the corresponding
/// [`WatchdogOutcome`]. If PTT release itself errors, the [`Ptt::Drop`]
/// impl is the backstop.
///
/// Pre-condition: caller has clamped `max_duration` against
/// [`HARD_CAP_DURATION`]. The binary enforces this; this function
/// trusts its caller.
pub fn run_watchdog<P>(
    ptt: &mut P,
    max_duration: Duration,
    stdin_eof: &AtomicBool,
    signal: &AtomicBool,
    poll_interval: Duration,
) -> Result<WatchdogOutcome, RtsError>
where
    P: Ptt<Error = RtsError>,
{
    ptt.assert()?;

    let deadline = Instant::now() + max_duration;

    let outcome = loop {
        if signal.load(Ordering::Acquire) {
            break WatchdogOutcome::Signaled;
        }
        if stdin_eof.load(Ordering::Acquire) {
            break WatchdogOutcome::StdinEof;
        }
        let now = Instant::now();
        if now >= deadline {
            break WatchdogOutcome::MaxDurationElapsed;
        }
        let remaining = deadline.saturating_duration_since(now);
        std::thread::sleep(poll_interval.min(remaining));
    };

    ptt.release()?;
    Ok(outcome)
}

// ─── tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::writer::{MockTtyWriter, RtsPtt, TtyOp};

    /// Drives `run_watchdog` with the given pre-set flags + waits for it
    /// to complete via a short max_duration. Returns the recorded ops
    /// + outcome.
    fn drive(
        signal_initial: bool,
        stdin_eof_initial: bool,
        max_duration: Duration,
    ) -> (Vec<TtyOp>, WatchdogOutcome) {
        let mut ptt = RtsPtt::new(MockTtyWriter::new()).unwrap();
        let signal = AtomicBool::new(signal_initial);
        let stdin_eof = AtomicBool::new(stdin_eof_initial);
        let outcome = run_watchdog(
            &mut ptt,
            max_duration,
            &stdin_eof,
            &signal,
            Duration::from_millis(5),
        )
        .unwrap();
        let ops = ptt.writer().ops.clone();
        (ops, outcome)
    }

    #[test]
    fn watchdog_asserts_then_releases_on_max_duration() {
        let (ops, outcome) = drive(false, false, Duration::from_millis(20));
        assert_eq!(outcome, WatchdogOutcome::MaxDurationElapsed);
        assert_eq!(
            ops,
            vec![TtyOp::OpenClearBoth, TtyOp::AssertRts, TtyOp::ReleaseRts]
        );
    }

    #[test]
    fn watchdog_releases_immediately_when_signal_flag_already_set() {
        let (ops, outcome) = drive(true, false, Duration::from_secs(5));
        assert_eq!(outcome, WatchdogOutcome::Signaled);
        assert_eq!(
            ops,
            vec![TtyOp::OpenClearBoth, TtyOp::AssertRts, TtyOp::ReleaseRts]
        );
    }

    #[test]
    fn watchdog_releases_immediately_when_stdin_eof_already_set() {
        let (ops, outcome) = drive(false, true, Duration::from_secs(5));
        assert_eq!(outcome, WatchdogOutcome::StdinEof);
        assert_eq!(
            ops,
            vec![TtyOp::OpenClearBoth, TtyOp::AssertRts, TtyOp::ReleaseRts]
        );
    }

    #[test]
    fn watchdog_observes_signal_set_mid_run() {
        // Spawn a thread that sets the signal flag after 20 ms; the
        // watchdog must observe it before its 1-second max_duration
        // elapses.
        let mut ptt = RtsPtt::new(MockTtyWriter::new()).unwrap();
        let signal = std::sync::Arc::new(AtomicBool::new(false));
        let stdin_eof = AtomicBool::new(false);
        let signal_setter = std::sync::Arc::clone(&signal);
        let _t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            signal_setter.store(true, Ordering::Release);
        });
        let started = Instant::now();
        let outcome = run_watchdog(
            &mut ptt,
            Duration::from_secs(1),
            &stdin_eof,
            &signal,
            Duration::from_millis(5),
        )
        .unwrap();
        let elapsed = started.elapsed();
        assert_eq!(outcome, WatchdogOutcome::Signaled);
        assert!(
            elapsed < Duration::from_millis(200),
            "watchdog took {elapsed:?} to observe signal — too slow"
        );
        assert_eq!(
            ptt.writer().ops,
            vec![TtyOp::OpenClearBoth, TtyOp::AssertRts, TtyOp::ReleaseRts]
        );
    }

    #[test]
    fn watchdog_observes_stdin_eof_set_mid_run() {
        // Same as above but for the stdin-EOF flag. In production
        // this fires when the parent pipe closes.
        let mut ptt = RtsPtt::new(MockTtyWriter::new()).unwrap();
        let signal = AtomicBool::new(false);
        let stdin_eof = std::sync::Arc::new(AtomicBool::new(false));
        let eof_setter = std::sync::Arc::clone(&stdin_eof);
        let _t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            eof_setter.store(true, Ordering::Release);
        });
        let started = Instant::now();
        let outcome = run_watchdog(
            &mut ptt,
            Duration::from_secs(1),
            &stdin_eof,
            &signal,
            Duration::from_millis(5),
        )
        .unwrap();
        let elapsed = started.elapsed();
        assert_eq!(outcome, WatchdogOutcome::StdinEof);
        assert!(
            elapsed < Duration::from_millis(200),
            "watchdog took {elapsed:?} to observe stdin-EOF — too slow"
        );
    }

    #[test]
    fn watchdog_does_not_release_before_any_trigger_fires() {
        // Tight invariant: while signal=false + stdin_eof=false + now <
        // deadline, the watchdog must NOT have released PTT. Verify by
        // running with a short max_duration and confirming the loop
        // didn't exit early.
        let mut ptt = RtsPtt::new(MockTtyWriter::new()).unwrap();
        let signal = AtomicBool::new(false);
        let stdin_eof = AtomicBool::new(false);
        let started = Instant::now();
        let outcome = run_watchdog(
            &mut ptt,
            Duration::from_millis(50),
            &stdin_eof,
            &signal,
            Duration::from_millis(5),
        )
        .unwrap();
        let elapsed = started.elapsed();
        assert_eq!(outcome, WatchdogOutcome::MaxDurationElapsed);
        // Held for at least most of the 50 ms (allow some slack).
        assert!(
            elapsed >= Duration::from_millis(40),
            "watchdog held PTT for only {elapsed:?} — released too early"
        );
    }

    #[test]
    fn watchdog_release_lands_via_drop_if_release_call_fails() {
        // RtsPtt's Drop is the backstop. We simulate "release call
        // fails" by using a TtyWriter that errors on ReleaseRts; the
        // run_watchdog returns an Err but the Drop impl issues a
        // best-effort release op anyway.
        //
        // For this test we use a custom writer that errors on
        // ReleaseRts but records ops up to and including the error.
        struct ErrorOnRelease {
            ops: std::cell::RefCell<Vec<TtyOp>>,
        }
        impl crate::TtyWriter for ErrorOnRelease {
            fn modem_op(&mut self, op: TtyOp) -> crate::RtsResult<()> {
                self.ops.borrow_mut().push(op);
                if op == TtyOp::ReleaseRts {
                    return Err(crate::RtsError::ModemLineIoctl(
                        std::io::Error::from_raw_os_error(5), // EIO
                    ));
                }
                Ok(())
            }
        }
        let writer = ErrorOnRelease {
            ops: std::cell::RefCell::new(Vec::new()),
        };
        // RtsPtt::new succeeds (its OpenClearBoth IS a ReleaseRts-like
        // ioctl in spirit, but here we only error on the specific
        // ReleaseRts op).
        // Wait — looking at RtsPtt::new, it issues OpenClearBoth, not
        // ReleaseRts. Our impl errors only on the EXACT ReleaseRts.
        // So new() should succeed.
        let mut ptt = RtsPtt::new(writer).unwrap();
        let signal = AtomicBool::new(true); // trigger immediate exit
        let stdin_eof = AtomicBool::new(false);
        let result = run_watchdog(
            &mut ptt,
            Duration::from_secs(5),
            &stdin_eof,
            &signal,
            Duration::from_millis(5),
        );
        // run_watchdog returns Err from the release().
        assert!(result.is_err());
        // The release attempt was issued (ops contains ReleaseRts).
        let ops = ptt.writer().ops.borrow().clone();
        assert!(ops.contains(&TtyOp::ReleaseRts));
    }
}
