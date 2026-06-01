//! PHY-public interface to upper layers (subsystem #5 link/MAC,
//! subsystem #7 link adaptation).
//!
//! Stability contract: this file's public types are the inter-subsystem
//! boundary. Breaking changes here ripple to #5 and #7; treat with care.

use crate::error::PhyError;
use crate::modes::{ModeHint, ResolvedMode};

/// Acknowledgement that a TX request was accepted and queued for
/// transmission. Carries a per-frame correlation tag for upper layers
/// that want to associate a TX request with downstream observations
/// (sound-card emit completion, per-frame energy estimate, etc.).
#[derive(Debug, Clone, Copy)]
pub struct TxToken(
    /// Monotonically-increasing per-frame correlation tag.
    pub u64,
);

/// A received frame, post-demodulation + post-FEC.
#[derive(Debug, Clone)]
pub struct RxFrame {
    payload: Vec<u8>,
    mode: ResolvedMode,
    per_subcarrier_snr_db: Option<Vec<f32>>,
    frame_snr_db: f32,
    decode_ok: bool,
}

impl RxFrame {
    /// Construct a new `RxFrame` from its component parts. Typically only
    /// called by the demod side of the PHY (or by `NullPhy` for loopback).
    pub fn new(
        payload: Vec<u8>,
        mode: ResolvedMode,
        per_subcarrier_snr_db: Option<Vec<f32>>,
        frame_snr_db: f32,
        decode_ok: bool,
    ) -> Self {
        Self {
            payload,
            mode,
            per_subcarrier_snr_db,
            frame_snr_db,
            decode_ok,
        }
    }
    /// Decoded payload bytes (FEC-corrected).
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
    /// Mode the frame was demodulated as.
    pub fn mode(&self) -> &ResolvedMode {
        &self.mode
    }
    /// Per-sub-carrier SNR measurement, when the mode produced one
    /// (OFDM modes do; narrow-FSK does not).
    pub fn per_subcarrier_snr_db(&self) -> Option<&[f32]> {
        self.per_subcarrier_snr_db.as_deref()
    }
    /// Aggregate frame SNR in dB.
    pub fn frame_snr_db(&self) -> f32 {
        self.frame_snr_db
    }
    /// `true` if FEC decoded cleanly; `false` if residual errors remain.
    pub fn decode_ok(&self) -> bool {
        self.decode_ok
    }
}

/// Read-only snapshot for subsystem #7 (link adaptation).
///
/// Per R3 disposition in PR #183: `doppler_spread_hz` lands here in a
/// later phase once Phase 4's CFO estimator can populate it. For v0.1
/// the field set covers per-sub-carrier SNR + aggregate SNR + frame
/// error rate + current bit-loading.
#[derive(Debug, Clone)]
pub struct ChannelQualityReport {
    per_subcarrier_snr_db: Vec<f32>,
    aggregate_snr_db: f32,
    recent_frames_total: u32,
    recent_frames_failed: u32,
    current_bit_loading: Option<Vec<u8>>,
}

impl ChannelQualityReport {
    /// Empty report (no frames yet observed). `aggregate_snr_db` is NaN.
    pub fn empty() -> Self {
        Self {
            per_subcarrier_snr_db: Vec::new(),
            aggregate_snr_db: f32::NAN,
            recent_frames_total: 0,
            recent_frames_failed: 0,
            current_bit_loading: None,
        }
    }
    /// Aggregate SNR averaged across recent frames, in dB.
    pub fn aggregate_snr_db(&self) -> f32 {
        self.aggregate_snr_db
    }
    /// Per-sub-carrier SNR in dB, most recent OFDM frame.
    pub fn per_subcarrier_snr_db(&self) -> &[f32] {
        &self.per_subcarrier_snr_db
    }
    /// Frame-error rate computed from the most recent window.
    pub fn frame_error_rate(&self) -> f32 {
        if self.recent_frames_total == 0 {
            0.0
        } else {
            self.recent_frames_failed as f32 / self.recent_frames_total as f32
        }
    }
    /// Current per-sub-carrier bit-loading bitmap, if a bit-loader is active.
    pub fn current_bit_loading(&self) -> Option<&[u8]> {
        self.current_bit_loading.as_deref()
    }
    /// Construct a `ChannelQualityReport` from explicit parts. Used by the
    /// PHY when synthesizing the snapshot from internal state.
    pub fn from_parts(
        per_subcarrier_snr_db: Vec<f32>,
        aggregate_snr_db: f32,
        recent_frames_total: u32,
        recent_frames_failed: u32,
        current_bit_loading: Option<Vec<u8>>,
    ) -> Self {
        Self {
            per_subcarrier_snr_db,
            aggregate_snr_db,
            recent_frames_total,
            recent_frames_failed,
            current_bit_loading,
        }
    }
}

/// PHY service exposed to subsystem #5 link/MAC.
pub trait PhyTransport {
    /// Queue a payload for transmission under the given mode hint.
    /// Returns a `TxToken` correlating this request to later observations.
    fn send_frame(&mut self, payload: &[u8], hint: ModeHint) -> Result<TxToken, PhyError>;
    /// Pop the next available decoded `RxFrame`, if any.
    fn poll_rx(&mut self) -> Option<RxFrame>;
    /// Snapshot of channel-quality observables for subsystem #7.
    fn channel_quality(&self) -> ChannelQualityReport;
}

/// In-process loopback PHY for contract tests. Frames sent are echoed
/// back via `poll_rx`. Does NOT exercise modulation/demodulation —
/// that's what later phases' integration tests cover.
pub struct NullPhy {
    pending_rx: std::collections::VecDeque<RxFrame>,
    next_token: u64,
    quality: ChannelQualityReport,
}

impl NullPhy {
    /// Construct an empty `NullPhy` with no frames pending.
    pub fn new() -> Self {
        Self {
            pending_rx: std::collections::VecDeque::new(),
            next_token: 0,
            quality: ChannelQualityReport::empty(),
        }
    }
}

impl Default for NullPhy {
    fn default() -> Self {
        Self::new()
    }
}

impl PhyTransport for NullPhy {
    fn send_frame(&mut self, payload: &[u8], hint: ModeHint) -> Result<TxToken, PhyError> {
        let mode = crate::modes::ModeTable::default().resolve(hint, None);
        let token = TxToken(self.next_token);
        self.next_token += 1;
        self.pending_rx.push_back(RxFrame::new(
            payload.to_vec(),
            mode,
            None,
            f32::INFINITY, // loopback = perfect
            true,
        ));
        Ok(token)
    }
    fn poll_rx(&mut self) -> Option<RxFrame> {
        self.pending_rx.pop_front()
    }
    fn channel_quality(&self) -> ChannelQualityReport {
        self.quality.clone()
    }
}
