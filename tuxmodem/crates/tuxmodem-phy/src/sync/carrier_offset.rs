//! Carrier-frequency-offset estimation via Schmidl-Cox-style
//! repeat-pair correlation. Given a known-repeating preamble segment,
//! the phase of the per-sample cross-correlation between the first and
//! second halves is proportional to the residual frequency offset.

use num_complex::Complex;

/// Carrier-frequency-offset estimator. Stateless; constructed once per
/// sample-rate, reused across detections.
pub struct CfoEstimator {
    sample_rate_hz: f32,
}

impl CfoEstimator {
    /// Construct a CFO estimator for the given sample rate in Hz.
    pub fn new(sample_rate_hz: f32) -> Self {
        Self { sample_rate_hz }
    }

    /// Estimate the residual carrier-frequency offset (Hz) of a signal
    /// known to contain a repeated `half_len`-sample preamble segment.
    pub fn estimate_repeat(&self, signal: &[Complex<f32>], half_len: usize) -> f32 {
        let mut acc = Complex::new(0.0, 0.0);
        for i in 0..half_len {
            acc += signal[i].conj() * signal[i + half_len];
        }
        let phase = acc.arg();
        phase * self.sample_rate_hz / (2.0 * std::f32::consts::PI * half_len as f32)
    }
}
