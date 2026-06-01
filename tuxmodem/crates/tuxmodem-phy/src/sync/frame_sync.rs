//! Frame-sync state machine. Coordinates preamble detection +
//! frame-boundary tracking; consumed by both mode families.

/// Frame-sync FSM state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameSyncState {
    /// No preamble currently locked; scanning for one.
    Searching,
    /// Preamble acquired; frame-boundary tracking active.
    Acquired,
}

/// Frame-sync state machine.
pub struct FrameSync {
    state: FrameSyncState,
    last_start_sample: Option<usize>,
    last_snr_db: f32,
}

impl FrameSync {
    /// Construct a fresh FSM in [`FrameSyncState::Searching`].
    pub fn new() -> Self {
        Self {
            state: FrameSyncState::Searching,
            last_start_sample: None,
            last_snr_db: f32::NEG_INFINITY,
        }
    }
    /// Current FSM state.
    pub fn state(&self) -> FrameSyncState {
        self.state
    }
    /// Sample index of the most-recently acquired preamble, if any.
    pub fn last_start_sample(&self) -> Option<usize> {
        self.last_start_sample
    }
    /// Estimated SNR of the most-recently acquired preamble.
    pub fn last_snr_db(&self) -> f32 {
        self.last_snr_db
    }
    /// Notify the FSM that a preamble was detected at `start_sample` with
    /// estimated `snr_db`; transitions to `Acquired`.
    pub fn notify_preamble_found(&mut self, start_sample: usize, snr_db: f32) {
        self.state = FrameSyncState::Acquired;
        self.last_start_sample = Some(start_sample);
        self.last_snr_db = snr_db;
    }
    /// Notify the FSM that the current frame completed; transitions back
    /// to `Searching`.
    pub fn notify_frame_complete(&mut self) {
        self.state = FrameSyncState::Searching;
    }
    /// Notify the FSM that decoding the current frame failed;
    /// transitions back to `Searching` to scan for the next preamble.
    pub fn notify_decode_failed(&mut self) {
        self.state = FrameSyncState::Searching;
    }
}

impl Default for FrameSync {
    fn default() -> Self {
        Self::new()
    }
}
