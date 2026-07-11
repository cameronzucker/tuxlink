//! The ONE ALSA touchpoint (spec §ALSA open + §ALSA read loop). Opens
//! `hw:<card_index>,0` — numeric live index, NOT `plughw:`, NOT `CARD=<id>`
//! (plug silently resamples, masking `blocked(unsupported-sample-rate)`;
//! id-based names collide on same-model codecs — see
//! `devices::ResolvedManagedDevice::alsa_hw`).
//!
//! CI-COMPILE-CHECKED ONLY: this file has no unit tests by design (it needs
//! real ALSA hardware); logic is kept minimal and every decision above the
//! errno mapping lives in the testable capture loop (service.rs).

use alsa::pcm::{Access, Format, HwParams, PCM};
use alsa::{Direction, ValueOr};

use super::traits::{process_mono_us, ReadBatch, SampleSource, SourceError};
use tuxlink_capture::slot::{GapKind, GapReport};

/// Open parameters (spec §ALSA open): S16_LE, exactly 48 000 Hz, mono
/// preferred / stereo-ch0 fallback, period 4 800 frames (100 ms), buffer 4
/// periods.
const RATE_HZ: u32 = 48_000;
const PERIOD_FRAMES: i64 = 4_800;
const BUFFER_FRAMES: i64 = PERIOD_FRAMES * 4;
/// 10 consecutive wait-timeouts (2 s of silent, non-erroring stream) — the
/// C-Media wedge class (spec §ALSA read loop).
const WEDGE_TIMEOUTS: u32 = 10;
/// `alsa::pcm::PCM::wait` takes `Option<u32>` in the 0.9 crate API.
const WAIT_MS: u32 = 200;

// Monotonic stamps come from `traits::process_mono_us` — the ONE process
// epoch (its doc pins why; no second OnceLock epoch may exist here).

pub struct AlsaSource {
    pcm: PCM,
    channels: u32,
    /// Interleaved scratch for the stereo-ch0 fallback path.
    stereo_buf: Vec<i16>,
    consecutive_wait_timeouts: u32,
    /// Set when an EPIPE recovery happened and the NEXT successful read must
    /// report the gap.
    pending_gap: Option<GapReport>,
}

/// errno → SourceError for the OPEN path.
fn map_open_err(e: &alsa::Error) -> SourceError {
    match e.errno() {
        libc::EBUSY => SourceError::Busy,
        libc::ENOENT | libc::ENODEV | libc::ENXIO => SourceError::Absent,
        _ => SourceError::Io(e.to_string()),
    }
}

impl AlsaSource {
    /// Open + negotiate on the hw device. Any parameter rejection →
    /// `UnsupportedFormat` carrying the ALSA diagnostic (the axis name is
    /// delta-pinned; the diagnostic distinguishes rate vs channel vs format).
    pub fn open(alsa_hw: &str) -> Result<Self, SourceError> {
        let pcm = PCM::new(alsa_hw, Direction::Capture, true /* nonblock */)
            .map_err(|e| map_open_err(&e))?;
        let channels = {
            let hwp = HwParams::any(&pcm).map_err(|e| SourceError::Io(e.to_string()))?;
            hwp.set_access(Access::RWInterleaved)
                .map_err(|e| SourceError::UnsupportedFormat(format!("access: {e}")))?;
            hwp.set_format(Format::s16())
                .map_err(|e| SourceError::UnsupportedFormat(format!("format S16_LE: {e}")))?;
            hwp.set_rate(RATE_HZ, ValueOr::Nearest)
                .map_err(|e| SourceError::UnsupportedFormat(format!("rate 48000: {e}")))?;
            // hw (no plug) may still land a neighbor rate via Nearest —
            // verify EXACT 48 000 (native only; no resampler path).
            let got = hwp.get_rate().map_err(|e| SourceError::Io(e.to_string()))?;
            if got != RATE_HZ {
                return Err(SourceError::UnsupportedFormat(format!(
                    "device native rate {got} != required 48000"
                )));
            }
            // Channels: 1 preferred; 2 with channel-0 extraction as fallback.
            let channels = if hwp.set_channels(1).is_ok() {
                1
            } else {
                hwp.set_channels(2)
                    .map_err(|e| SourceError::UnsupportedFormat(format!("channels 1|2: {e}")))?;
                2
            };
            hwp.set_period_size_near(PERIOD_FRAMES, ValueOr::Nearest)
                .map_err(|e| SourceError::UnsupportedFormat(format!("period: {e}")))?;
            hwp.set_buffer_size_near(BUFFER_FRAMES)
                .map_err(|e| SourceError::UnsupportedFormat(format!("buffer: {e}")))?;
            pcm.hw_params(&hwp).map_err(|e| SourceError::UnsupportedFormat(e.to_string()))?;
            channels
        };
        pcm.prepare().map_err(|e| SourceError::Io(e.to_string()))?;
        pcm.start().map_err(|e| SourceError::Io(e.to_string()))?;
        Ok(Self {
            pcm,
            channels,
            stereo_buf: Vec::new(),
            consecutive_wait_timeouts: 0,
            pending_gap: None,
        })
    }

    /// errno → SourceError for the READ path; EPIPE handled by the caller.
    /// `&self`, not `&mut self`: `PCM::prepare`/`PCM::start` take `&self` in
    /// alsa 0.9 (state lives behind the FFI handle) — a `&mut self` receiver
    /// here would conflict with the `IO<'_, i16>` borrow (`io_i16()`) still
    /// live across the `readi` match in [`SampleSource::read`] (`IO`
    /// implements `Drop`, so NLL cannot shorten that borrow early).
    fn map_read_err(&self, e: &alsa::Error) -> Option<SourceError> {
        match e.errno() {
            libc::EAGAIN => None, // nonblocking no-data: not an error
            libc::ESTRPIPE => {
                // Suspend: recover the PCM so the next read works, surface
                // Suspended ONCE (capture loop abandons the slot).
                let _ = self.pcm.prepare();
                Some(SourceError::Suspended)
            }
            libc::ENODEV | libc::EBADFD | libc::ENOENT => Some(SourceError::Absent),
            _ => Some(SourceError::Io(e.to_string())),
        }
    }
}

impl SampleSource for AlsaSource {
    fn read(&mut self, buf: &mut [i16]) -> Result<ReadBatch, SourceError> {
        // PCM::wait bounds the park (abort latency ≈ one timeout).
        match self.pcm.wait(Some(WAIT_MS)) {
            Ok(true) => self.consecutive_wait_timeouts = 0,
            Ok(false) => {
                self.consecutive_wait_timeouts += 1;
                if self.consecutive_wait_timeouts >= WEDGE_TIMEOUTS {
                    return Err(SourceError::Wedged);
                }
                return Ok(ReadBatch { frames: 0, mono_ts_us: process_mono_us(), gap: self.pending_gap.take() });
            }
            Err(e) => {
                if e.errno() == libc::EPIPE {
                    // Overrun signaled via wait: recover; gap size comes from
                    // the assembler's monotonic counter, never from ALSA.
                    let _ = self.pcm.prepare();
                    let _ = self.pcm.start();
                    self.pending_gap = Some(GapReport { kind: GapKind::Overrun });
                    return Ok(ReadBatch { frames: 0, mono_ts_us: process_mono_us(), gap: None });
                }
                if let Some(err) = self.map_read_err(&e) {
                    return Err(err);
                }
                return Ok(ReadBatch { frames: 0, mono_ts_us: process_mono_us(), gap: self.pending_gap.take() });
            }
        }

        let io = self.pcm.io_i16().map_err(|e| SourceError::Io(e.to_string()))?;
        if self.channels == 1 {
            match io.readi(buf) {
                Ok(frames) => Ok(ReadBatch { frames, mono_ts_us: process_mono_us(), gap: self.pending_gap.take() }),
                Err(e) if e.errno() == libc::EPIPE => {
                    drop(io);
                    let _ = self.pcm.prepare();
                    let _ = self.pcm.start();
                    self.pending_gap = Some(GapReport { kind: GapKind::Overrun });
                    Ok(ReadBatch { frames: 0, mono_ts_us: process_mono_us(), gap: None })
                }
                Err(e) => match self.map_read_err(&e) {
                    Some(err) => Err(err),
                    None => Ok(ReadBatch { frames: 0, mono_ts_us: process_mono_us(), gap: self.pending_gap.take() }),
                },
            }
        } else {
            // Stereo: read interleaved, keep channel 0 (left).
            self.stereo_buf.resize(buf.len() * 2, 0);
            match io.readi(&mut self.stereo_buf) {
                Ok(frames) => {
                    for i in 0..frames.min(buf.len()) {
                        buf[i] = self.stereo_buf[i * 2];
                    }
                    Ok(ReadBatch { frames: frames.min(buf.len()), mono_ts_us: process_mono_us(), gap: self.pending_gap.take() })
                }
                Err(e) if e.errno() == libc::EPIPE => {
                    drop(io);
                    let _ = self.pcm.prepare();
                    let _ = self.pcm.start();
                    self.pending_gap = Some(GapReport { kind: GapKind::Overrun });
                    Ok(ReadBatch { frames: 0, mono_ts_us: process_mono_us(), gap: None })
                }
                Err(e) => match self.map_read_err(&e) {
                    Some(err) => Err(err),
                    None => Ok(ReadBatch { frames: 0, mono_ts_us: process_mono_us(), gap: self.pending_gap.take() }),
                },
            }
        }
    }
}
