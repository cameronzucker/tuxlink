//! The setup-surface live input meter (spec §NewCommands `ft8_device_meter`).
//!
//! Opens the SAME ALSA capture device the listener uses — via
//! [`crate::ft8::alsa_source::AlsaSource`], NOT a second ALSA implementation —
//! reads a short window, and reports an RMS level + a coarse state the setup UI
//! renders as a live bar. The device-open race against the listener is arbitrated
//! by [`crate::ft8::service::DeviceReservation`]; this module is only the read +
//! RMS math and the errno→state mapping.
//!
//! Testability: the read/RMS loop [`meter_read`] is generic over
//! [`SampleSource`] so unit tests drive it with the scripted fake (no hardware).
//! Only the thin [`open_and_meter`] wrapper touches real ALSA and is therefore
//! CI-compile-checked only, exactly like `alsa_source.rs`.

use serde::Serialize;

use crate::ft8::traits::{SampleSource, SourceError};

/// One live-meter sample on the wire (spec §NewCommands). `rmsDbfs` is the
/// windowed RMS in dBFS (always finite — floored, never `-inf`/`NaN`); `state`
/// is one of `live | silent | in-use | error`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeterDto {
    pub rms_dbfs: f64,
    pub state: String,
    /// The underlying ALSA diagnostic for the `error` state (open/negotiate/IO
    /// failure text) — the setup surface surfaces it so "meter unavailable" is
    /// actionable ("rate 48000: ..." vs EBUSY vs vanished device). `None` for
    /// every non-error state. Additive; skipped on the wire when absent so
    /// pre-existing consumers/tests see the exact old shape.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// dBFS floor: a zero/near-silent buffer reports this instead of `-inf`, so the
/// wire value is always finite (the L3 bar needs a number, not `NaN`).
pub(crate) const FLOOR_DBFS: f64 = -120.0;
/// At or below this level the input reads as silence (no signal present).
pub(crate) const SILENCE_DBFS: f64 = -50.0;
/// Discard at least this many frames after open — one full 100 ms ALSA period
/// (spec §ALSA open: `PERIOD_FRAMES = 4800`). The first data after `PCM::start`
/// is unreliable; discarding a period also lets the capture ring fill.
const DISCARD_FRAMES: usize = 4_800;
/// Measure RMS over ~100 ms of real frames (4800 @ 48 kHz mono) — one period.
/// Kept short (was 150 ms) to trim the meter's nominal device hold; 100 ms is
/// ample for a level-meter RMS. The discard period is NOT shortened (it must
/// clear the full ALSA capture-start transient). The listener-priority
/// preemption poll, not this length, is what guarantees the clean win.
const MEASURE_FRAMES: usize = 4_800;
/// Read buffer = one period; also the total-read bound (belt-and-suspenders:
/// each `SampleSource::read` is itself ~200 ms-bounded, so the meter cannot
/// spin forever on a silent-but-open device).
const BUF_FRAMES: usize = 4_800;
const MAX_READS: usize = 16;

impl MeterDto {
    /// Device held by the listener or another process (EBUSY at open/read).
    pub(crate) fn in_use() -> Self {
        Self { rms_dbfs: FLOOR_DBFS, state: "in-use".into(), detail: None }
    }
    /// `error`, carrying the underlying ALSA diagnostic (operator live-test
    /// 2026-07-12: a bare "meter unavailable" is undiagnosable in the field).
    pub(crate) fn error_with(e: &crate::ft8::traits::SourceError) -> Self {
        use crate::ft8::traits::SourceError as E;
        // SourceError has no Display (diagnostics travel as Strings per the
        // result_large_err discipline) — render the operator-readable form here.
        let detail = match e {
            E::Busy => "device busy".to_string(),
            E::Absent => "device disappeared (unplugged?)".to_string(),
            E::UnsupportedFormat(d) => format!("unsupported format: {d}"),
            E::Suspended => "stream suspended (system sleep)".to_string(),
            E::Wedged => "device wedged (silent, not erroring) — replug or restart".to_string(),
            E::Io(d) => format!("audio I/O error: {d}"),
        };
        Self { rms_dbfs: FLOOR_DBFS, state: "error".into(), detail: Some(detail) }
    }
}

/// Outcome of [`meter_read`] on the source side (a source error is the other
/// arm, in the `Result`). `Preempted` means the listener claimed priority
/// mid-read and we bailed — the caller maps it to `in-use`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MeterRead {
    Level(MeterDto),
    Preempted,
}

/// Convert a summed-squares accumulation to a finite dBFS value + coarse state.
fn dbfs_state(sum_sq: f64, n: usize) -> (f64, &'static str) {
    if n == 0 {
        return (FLOOR_DBFS, "silent");
    }
    let rms = (sum_sq / n as f64).sqrt();
    let dbfs = if rms <= 0.0 { FLOOR_DBFS } else { (20.0 * (rms / 32_768.0).log10()).max(FLOOR_DBFS) };
    let state = if dbfs <= SILENCE_DBFS { "silent" } else { "live" };
    (dbfs, state)
}

/// The read + RMS loop (spec §NewCommands: "wait ≥1 period → RMS over ~100 ms").
/// Discards the first full period, then accumulates ~100 ms of real frames and
/// returns the level. Between every read it calls `preempted()`: when the
/// listener claims priority mid-measurement it returns `Preempted` immediately
/// (→ `in-use`) rather than finishing the full window, so the listener's
/// `acquire_priority` wins within ~one read iteration (this is what makes the
/// clean win real — see `DeviceReservation`'s module comment). Returns `Err`
/// only if the source itself errors — the caller maps that to `in-use`/`error`.
/// A source that opens but delivers only empty batches yields a finite floored
/// value, never `NaN` (the "single nonblocking read gives NaN" hazard).
pub(crate) fn meter_read<S, F>(src: &mut S, mut preempted: F) -> Result<MeterRead, SourceError>
where
    S: SampleSource,
    F: FnMut() -> bool,
{
    let mut buf = vec![0i16; BUF_FRAMES];

    // Phase 1: discard ≥1 full period (empty/timeout batches don't count, so a
    // slow first period is waited through rather than measured on garbage).
    let mut discarded = 0usize;
    let mut reads = 0usize;
    while discarded < DISCARD_FRAMES && reads < MAX_READS {
        if preempted() {
            return Ok(MeterRead::Preempted);
        }
        let batch = src.read(&mut buf)?;
        discarded += batch.frames;
        reads += 1;
    }

    // Phase 2: accumulate sum-of-squares over ~100 ms of delivered frames.
    let mut sum_sq = 0f64;
    let mut n = 0usize;
    while n < MEASURE_FRAMES && reads < MAX_READS {
        if preempted() {
            return Ok(MeterRead::Preempted);
        }
        let batch = src.read(&mut buf)?;
        for &s in &buf[..batch.frames.min(buf.len())] {
            sum_sq += f64::from(s) * f64::from(s);
        }
        n += batch.frames;
        reads += 1;
    }

    let (rms_dbfs, state) = dbfs_state(sum_sq, n);
    Ok(MeterRead::Level(MeterDto { rms_dbfs, state: state.into(), detail: None }))
}

/// Open the device, run [`meter_read`] preemptible on `resv`/`id`, close.
/// CI-compile-checked only (needs real ALSA). Maps a busy device (or a
/// listener-priority preemption) to `in-use`, any other open/read failure to
/// `error`; a clean read returns `live`/`silent`. The caller MUST already hold
/// the meter reservation (`DeviceReservation::try_meter`) across this call — the
/// preemption poll reads the SAME reservation to detect a concurrent listener.
pub(crate) fn open_and_meter(
    resv: &crate::ft8::service::DeviceReservation,
    id: &crate::winlink::ax25::devices::StableAudioId,
    alsa_hw: &str,
) -> MeterDto {
    match crate::ft8::alsa_source::AlsaSource::open(alsa_hw) {
        Ok(mut src) => match meter_read(&mut src, || resv.listener_wants(id)) {
            Ok(MeterRead::Level(dto)) => dto,
            Ok(MeterRead::Preempted) => MeterDto::in_use(),
            Err(SourceError::Busy) => MeterDto::in_use(),
            Err(e) => MeterDto::error_with(&e),
        },
        Err(SourceError::Busy) => MeterDto::in_use(),
        Err(e) => MeterDto::error_with(&e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ft8::testutil::{ScriptedSource, SourceStep, SyntheticClock};
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    fn scripted(steps: Vec<SourceStep>) -> ScriptedSource {
        ScriptedSource {
            steps: Arc::new(Mutex::new(VecDeque::from(steps))),
            clock: SyntheticClock::new(0),
        }
    }

    /// Extract the level DTO from a non-preempted read (test helper).
    fn level(r: Result<MeterRead, SourceError>) -> MeterDto {
        match r.unwrap() {
            MeterRead::Level(dto) => dto,
            MeterRead::Preempted => panic!("unexpected preemption"),
        }
    }

    /// (a) A silent source → `state:"silent"`, a FINITE `rms_dbfs`, and the
    /// serialized key is camelCase (`rmsDbfs`).
    #[test]
    fn meter_read_on_silent_source_is_silent_and_finite() {
        // 1 discard period + 1 measure period of pure-zero frames.
        let mut src = scripted(vec![
            SourceStep::Frames { frames: 4_800, value: 0, gap: None },
            SourceStep::Frames { frames: 4_800, value: 0, gap: None },
            SourceStep::Frames { frames: 4_800, value: 0, gap: None },
        ]);
        let dto = level(meter_read(&mut src, || false));
        assert_eq!(dto.state, "silent");
        assert!(dto.rms_dbfs.is_finite(), "rms_dbfs must be finite, got {}", dto.rms_dbfs);

        let v = serde_json::to_value(&dto).unwrap();
        assert!(v["rmsDbfs"].is_number(), "wire key is camelCase rmsDbfs");
        assert_eq!(v["state"], "silent");
        assert!(v.get("rms_dbfs").is_none(), "no snake_case leakage");
    }

    /// (b) A single post-start nonblocking read would give NaN; the impl must
    /// wait past the empty batch for a real period and return a FINITE value.
    #[test]
    fn meter_read_waits_a_period_and_returns_finite_not_nan() {
        // First read is an empty batch (models a not-yet-ready period); the
        // loop must keep reading until it has a full period of real frames.
        let mut src = scripted(vec![
            SourceStep::Idle, // empty batch: frames = 0
            SourceStep::Frames { frames: 4_800, value: 2_000, gap: None },
            SourceStep::Frames { frames: 4_800, value: 2_000, gap: None },
        ]);
        let dto = level(meter_read(&mut src, || false));
        assert!(dto.rms_dbfs.is_finite() && !dto.rms_dbfs.is_nan(), "finite, not NaN");
        // value 2000 → 20*log10(2000/32768) ≈ -24.3 dBFS → above the silence
        // threshold → "live".
        assert_eq!(dto.state, "live");
        assert!(dto.rms_dbfs > SILENCE_DBFS, "a real signal reads as live");
    }

    /// (c) Listener priority mid-measurement → the read ABORTS (Preempted)
    /// rather than finishing the window. Here the preempt flag is false for the
    /// first poll (a read happens) then true — proving it bails MID-loop, not
    /// only at entry.
    #[test]
    fn meter_read_aborts_when_listener_preempts_mid_read() {
        let mut src = scripted(vec![
            SourceStep::Frames { frames: 4_800, value: 3_000, gap: None },
            SourceStep::Frames { frames: 4_800, value: 3_000, gap: None },
            SourceStep::Frames { frames: 4_800, value: 3_000, gap: None },
        ]);
        let polls = std::cell::Cell::new(0u32);
        let r = meter_read(&mut src, || {
            let n = polls.get();
            polls.set(n + 1);
            n >= 1 // false on the first poll, true thereafter
        });
        assert_eq!(r.unwrap(), MeterRead::Preempted, "bails once the listener claims priority");
    }

    /// A read error propagates so `open_and_meter` can map it to in-use/error.
    #[test]
    fn meter_read_propagates_source_error() {
        let mut src = scripted(vec![SourceStep::Fail(SourceError::Busy)]);
        assert_eq!(meter_read(&mut src, || false), Err(SourceError::Busy));
    }

    /// dBFS is floored, never `-inf`, on an all-zero measurement.
    #[test]
    fn dbfs_state_floors_zero_to_finite_silent() {
        let (dbfs, state) = dbfs_state(0.0, 4_800);
        assert_eq!(dbfs, FLOOR_DBFS);
        assert_eq!(state, "silent");
        assert!(dbfs.is_finite());
    }

    /// in-use / error constructors carry the floor level and their state tag.
    #[test]
    fn in_use_and_error_dtos_are_tagged() {
        assert_eq!(MeterDto::in_use().state, "in-use");
        assert_eq!(MeterDto::error_with(&SourceError::Absent).state, "error");
        assert!(MeterDto::in_use().rms_dbfs.is_finite());
    }
}
