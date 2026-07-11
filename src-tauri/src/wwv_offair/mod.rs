//! Off-air WWV decode pipeline: rig orchestration, audio capture, and
//! frequency/normalize/schedule helpers.

pub mod capture;
pub mod commands;
pub mod freq;
pub mod model;
pub mod normalize;
pub mod schedule;

use std::cell::RefCell;
use std::path::PathBuf;
use std::time::Duration;

use tux_rig::{ManagedRig, Mode, RigConfig, RigError, RigStatus};

use crate::wwv_offair::capture::CaptureSource;

/// Errors from the WWV off-air capture cycle: rig control or audio capture.
#[derive(Debug, thiserror::Error)]
pub enum WwvError {
    #[error("rig: {0}")]
    Rig(String),
    #[error("capture: {0}")]
    Capture(String),
}

/// Minimal rig surface `run_cycle` needs — lets tests substitute a mock and lets
/// the internal-codec path re-spawn after `release_serial`.
pub(crate) trait TuneRig {
    fn status(&self) -> Result<RigStatus, RigError>;
    fn tune(&self, hz: u64, mode: Mode) -> Result<(), RigError>;
    fn release_serial(&self);
    fn respawn(&self) -> Result<(), RigError>;
}

/// Save the current VFO, tune to WWV/USB, capture, then restore the saved
/// VFO+mode. On internal-codec radios (`close_serial`), release the CAT
/// serial before capture (so the audio device isn't fighting the serial
/// port) and re-spawn rigctld afterward so the restore tune succeeds.
pub(crate) fn run_cycle<R: TuneRig, C: CaptureSource>(
    rig: &R,
    close_serial: bool,
    freq_hz: u64,
    dwell: Duration,
    capture: &C,
) -> Result<PathBuf, WwvError> {
    let saved = rig.status().map_err(|e| WwvError::Rig(e.to_string()))?;
    rig.tune(freq_hz, Mode::Usb)
        .map_err(|e| WwvError::Rig(e.to_string()))?;
    if close_serial {
        rig.release_serial();
    }
    let out = capture
        .capture(freq_hz, dwell)
        .map_err(|e| WwvError::Capture(e.to_string()))?;
    if close_serial {
        rig.respawn().map_err(|e| WwvError::Rig(e.to_string()))?;
    }
    // Restore original VFO+mode (mode may be unknown → leave as-is).
    if let Some(m) = saved.mode {
        rig.tune(saved.freq_hz, m)
            .map_err(|e| WwvError::Rig(e.to_string()))?;
    }
    Ok(out)
}

/// Adapts a live `ManagedRig` to `TuneRig`, holding the `RigConfig` so
/// `respawn` can re-spawn rigctld after `release_serial` stopped it.
struct ManagedTuneRig {
    cfg: RigConfig,
    inner: RefCell<Option<ManagedRig>>,
}

impl TuneRig for ManagedTuneRig {
    fn status(&self) -> Result<RigStatus, RigError> {
        self.inner
            .borrow_mut()
            .as_mut()
            .ok_or_else(|| RigError::Spawn("no rig".into()))?
            .status()
    }

    fn tune(&self, hz: u64, mode: Mode) -> Result<(), RigError> {
        self.inner
            .borrow_mut()
            .as_mut()
            .ok_or_else(|| RigError::Spawn("no rig".into()))?
            .tune(hz, mode)
    }

    fn release_serial(&self) {
        if let Some(r) = self.inner.borrow_mut().as_mut() {
            r.release_serial();
        }
        *self.inner.borrow_mut() = None;
    }

    fn respawn(&self) -> Result<(), RigError> {
        let r = ManagedRig::spawn(self.cfg.clone())?;
        *self.inner.borrow_mut() = Some(r);
        Ok(())
    }
}

/// Run one WWV off-air capture cycle against a real (managed) rig: spawn
/// rigctld, save the current VFO, tune to WWV, capture, then restore.
pub fn capture_cycle<C: CaptureSource>(
    rig_cfg: RigConfig,
    close_serial: bool,
    freq_hz: u64,
    dwell: Duration,
    capture: &C,
) -> Result<PathBuf, WwvError> {
    let rig = ManagedRig::spawn(rig_cfg.clone()).map_err(|e| WwvError::Rig(e.to_string()))?;
    let adapter = ManagedTuneRig {
        cfg: rig_cfg,
        inner: RefCell::new(Some(rig)),
    };
    run_cycle(&adapter, close_serial, freq_hz, dwell, capture)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wwv_offair::capture::CaptureError;
    use std::rc::Rc;

    /// Ordered call log shared between `MockRig` and `MockCapture` so tests
    /// can assert the *interleaving* between rig calls and the capture call,
    /// not just each mock's calls in isolation. `run_cycle` takes the rig and
    /// capture source as separate generic parameters with no shared state in
    /// production, but the test double needs the shared log to prove the
    /// serial-sequencing invariant: capture() happens strictly after
    /// release_serial() and strictly before respawn()/restore.
    type CallLog = Rc<RefCell<Vec<String>>>;

    /// Records ordered rig calls via interior mutability — `TuneRig`'s
    /// methods take `&self` (to fit behind the `ManagedTuneRig`/`RefCell`
    /// adapter), so a plain `Vec` field can't be mutated directly.
    struct MockRig {
        status: RigStatus,
        log: CallLog,
    }

    impl MockRig {
        fn new(status: RigStatus, log: CallLog) -> Self {
            Self { status, log }
        }
    }

    impl TuneRig for MockRig {
        fn status(&self) -> Result<RigStatus, RigError> {
            self.log.borrow_mut().push("status".into());
            Ok(self.status)
        }

        fn tune(&self, hz: u64, mode: Mode) -> Result<(), RigError> {
            self.log.borrow_mut().push(format!("tune {hz} {mode:?}"));
            Ok(())
        }

        fn release_serial(&self) {
            self.log.borrow_mut().push("release_serial".into());
        }

        fn respawn(&self) -> Result<(), RigError> {
            self.log.borrow_mut().push("respawn".into());
            Ok(())
        }
    }

    /// Records its `capture()` invocation into the same shared log as
    /// `MockRig` — see `CallLog` doc comment above.
    struct MockCapture {
        log: CallLog,
    }

    impl MockCapture {
        fn new(log: CallLog) -> Self {
            Self { log }
        }
    }

    impl CaptureSource for MockCapture {
        fn capture(&self, _freq_hz: u64, _dwell: Duration) -> Result<PathBuf, CaptureError> {
            self.log.borrow_mut().push("capture".into());
            Ok(PathBuf::from("/mock/wwv.wav"))
        }
    }

    #[test]
    fn cycle_saves_tunes_captures_restores_no_release() {
        let log: CallLog = Rc::new(RefCell::new(Vec::new()));
        let mock = MockRig::new(
            RigStatus {
                freq_hz: 14_074_000,
                mode: Some(Mode::PktUsb),
                ptt: false,
            },
            Rc::clone(&log),
        );
        let cap = MockCapture::new(Rc::clone(&log));
        let out = run_cycle(&mock, false, 10_000_000, Duration::from_secs(70), &cap).unwrap();
        assert_eq!(out, PathBuf::from("/mock/wwv.wav"));
        assert_eq!(
            log.borrow().clone(),
            vec![
                "status".to_string(),
                "tune 10000000 Usb".to_string(),
                "capture".to_string(),
                "tune 14074000 PktUsb".to_string(), // restore, no release/re-spawn
            ]
        );
    }

    #[test]
    fn cycle_releases_serial_and_respawns_for_internal_codec() {
        let log: CallLog = Rc::new(RefCell::new(Vec::new()));
        let mock = MockRig::new(
            RigStatus {
                freq_hz: 14_074_000,
                mode: Some(Mode::PktUsb),
                ptt: false,
            },
            Rc::clone(&log),
        );
        let cap = MockCapture::new(Rc::clone(&log));
        run_cycle(&mock, true, 10_000_000, Duration::from_secs(70), &cap).unwrap();
        assert_eq!(
            log.borrow().clone(),
            vec![
                "status".to_string(),
                "tune 10000000 Usb".to_string(),
                "release_serial".to_string(),
                "capture".to_string(),
                "respawn".to_string(),
                "tune 14074000 PktUsb".to_string(),
            ]
        );
    }
}
