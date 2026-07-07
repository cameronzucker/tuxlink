//! FT-8 channelization: turn 12 kHz real audio into the time/frequency
//! representation the Costas sync search consumes.
//!
//! # Clean-room provenance (see `PROVENANCE.md`)
//!
//! The spectrogram construction (symbol-length windowed FFTs stepped by a
//! quarter-symbol, zero-padded for a finer frequency grid) follows the **WB2FKO
//! "Synchronization in FT8"** description of `sync8.f90`: "The audio energy
//! spectrum is calculated at sequential, partially overlapping time windows. The
//! time increment is 1/4 of the duration of a single FT8 symbol, i.e. 40 ms …
//! Each 160 ms sample interval is zero-padded to produce a 320 ms input signal
//! for the FFT." It is cross-checked against the MIT `ft8_lib` waterfall
//! oversampling model (`time_osr` / `freq_osr` in `decode.c` `get_cand_mag`).
//! The FFT itself is the permissively-licensed `realfft`/`rustfft`; the Hann
//! window and the single-bin (Goertzel-style) DFT are standard public-domain
//! DSP, cited to their own literature per the two-tier rule in `PROVENANCE.md`.
//!
//! # What this module does
//!
//! [`compute_spectrogram`] produces a [`Spectrogram`]: a dense
//! `num_time_steps × num_bins` grid of per-bin *power* (magnitude², i.e. received
//! energy). A "time step" is a quarter-symbol (40 ms / 480 samples); a symbol
//! therefore spans [`TIME_OSR`] time steps. A "bin" is [`BIN_HZ`] (3.125 Hz)
//! wide, so an FT-8 tone (6.25 Hz spacing) is exactly [`FREQ_OSR`] bins away from
//! its neighbour. [`tone_power`] is the single-frequency DFT primitive the sync
//! fine-refinement and per-symbol tone extraction use to read energy at an
//! arbitrary (sub-bin) frequency and sample offset.

use crate::consts::SAMPLE_RATE_HZ;
use realfft::RealFftPlanner;
use std::f64::consts::PI;

/// Samples per FT-8 symbol at 12 kHz: `0.160 s × 12000 = 1920`. The detection FFT
/// spans a full symbol so the 6.25 Hz tones resolve (a shorter FFT cannot).
/// provenance: QEX 2020 §4 / Table 4 (symbol time 0.160 s); WB2FKO "160 ms time
/// windows". Value = `SYMBOL_SECS × SAMPLE_RATE_HZ`.
pub const SYMBOL_SAMPLES: usize = 1920;

/// Frequency oversampling: the symbol window is zero-padded by this factor before
/// the FFT, giving a bin spacing of `TONE_SPACING / FREQ_OSR = 3.125 Hz` so a tone
/// lands on every `FREQ_OSR`-th bin.
/// provenance: WB2FKO "Each 160 ms sample interval is zero-padded to produce a
/// 320 ms input signal" (2× pad); MIT `ft8_lib` `decode.c` `freq_osr = 2`.
pub const FREQ_OSR: usize = 2;

/// Time oversampling: FFT windows are stepped by `SYMBOL_SAMPLES / TIME_OSR`
/// (a quarter symbol = 40 ms = 480 samples), so a symbol spans `TIME_OSR` steps.
/// provenance: WB2FKO "The time increment is 1/4 of the duration of a single FT8
/// symbol, i.e. 40 ms" (quarter-symbol hop).
pub const TIME_OSR: usize = 4;

/// Zero-padded FFT length: `SYMBOL_SAMPLES × FREQ_OSR = 3840`.
/// provenance: derived from [`SYMBOL_SAMPLES`] × [`FREQ_OSR`] (WB2FKO 320 ms pad).
pub const FFT_LEN: usize = SYMBOL_SAMPLES * FREQ_OSR;

/// Sample hop between consecutive time steps: `SYMBOL_SAMPLES / TIME_OSR = 480`.
/// provenance: derived from [`SYMBOL_SAMPLES`] / [`TIME_OSR`] (WB2FKO 40 ms hop).
pub const HOP_SAMPLES: usize = SYMBOL_SAMPLES / TIME_OSR;

/// Frequency width of one spectrogram bin in Hz: `12000 / FFT_LEN = 3.125`.
/// provenance: derived from [`SAMPLE_RATE_HZ`] / [`FFT_LEN`]; equals
/// `TONE_SPACING_HZ / FREQ_OSR` (QEX 2020 §4 tone spacing 6.25 Hz).
pub const BIN_HZ: f64 = SAMPLE_RATE_HZ as f64 / FFT_LEN as f64;

/// A dense short-time power spectrogram of a decode slot.
///
/// `power[t * num_bins + f]` is the energy (magnitude²) in frequency bin `f` at
/// time step `t`. Time step `t` begins at sample `t * HOP_SAMPLES`; bin `f` is
/// centred at `f * BIN_HZ` Hz.
#[derive(Clone, Debug)]
pub struct Spectrogram {
    /// Number of quarter-symbol time steps.
    pub num_time_steps: usize,
    /// Number of frequency bins (`FFT_LEN / 2 + 1`, one-sided real FFT).
    pub num_bins: usize,
    /// Row-major power grid, length `num_time_steps × num_bins`.
    pub power: Vec<f32>,
}

impl Spectrogram {
    /// Power at time step `t`, bin `f` (`0.0` if out of range).
    #[inline]
    pub fn at(&self, t: usize, f: usize) -> f32 {
        if t < self.num_time_steps && f < self.num_bins {
            self.power[t * self.num_bins + f]
        } else {
            0.0
        }
    }
}

/// Periodic Hann window of length `n`: `0.5 - 0.5·cos(2πk/n)`.
/// provenance: standard public-domain DSP window (Harris, "On the Use of Windows
/// for Harmonic Analysis with the DFT", Proc. IEEE 1978).
fn hann(n: usize) -> Vec<f32> {
    (0..n)
        .map(|k| 0.5 - 0.5 * (2.0 * PI * k as f64 / n as f64).cos())
        .map(|w| w as f32)
        .collect()
}

/// Compute the [`Spectrogram`] of a real audio slot sampled at 12 kHz.
///
/// Each time step applies a Hann window to a [`SYMBOL_SAMPLES`]-sample slice,
/// zero-pads it to [`FFT_LEN`], forward-FFTs it, and stores per-bin power. The
/// number of time steps is `(len - SYMBOL_SAMPLES) / HOP_SAMPLES + 1` (or `0` if
/// the input is shorter than one symbol).
///
/// # Panics
///
/// Panics if `sample_rate != 12000`. The FT-8 decode grid (6.25 Hz tones,
/// 160 ms symbols) is defined at the canonical 12 kHz rate; a caller with a
/// different rate must resample first. The rate is asserted rather than silently
/// ignored.
/// provenance: WB2FKO spectrogram construction; MIT `ft8_lib` waterfall model.
pub fn compute_spectrogram(samples: &[f32], sample_rate: u32) -> Spectrogram {
    assert_eq!(
        sample_rate, SAMPLE_RATE_HZ,
        "FT-8 channelization requires {SAMPLE_RATE_HZ} Hz audio; resample first"
    );

    let num_bins = FFT_LEN / 2 + 1;
    let num_time_steps = if samples.len() >= SYMBOL_SAMPLES {
        (samples.len() - SYMBOL_SAMPLES) / HOP_SAMPLES + 1
    } else {
        0
    };

    let window = hann(SYMBOL_SAMPLES);
    let mut planner = RealFftPlanner::<f32>::new();
    let r2c = planner.plan_fft_forward(FFT_LEN);
    let mut input = r2c.make_input_vec(); // length FFT_LEN
    let mut spectrum = r2c.make_output_vec(); // length num_bins

    let mut power = vec![0.0f32; num_time_steps * num_bins];
    for t in 0..num_time_steps {
        let base = t * HOP_SAMPLES;
        // Windowed symbol slice into the first SYMBOL_SAMPLES; zero-pad the rest.
        for (i, slot) in input.iter_mut().enumerate() {
            *slot = if i < SYMBOL_SAMPLES {
                samples[base + i] * window[i]
            } else {
                0.0
            };
        }
        // realfft mutates the input scratch; that is fine, it is refilled above.
        r2c.process(&mut input, &mut spectrum)
            .expect("realfft: input/output vecs are correctly sized");
        let row = &mut power[t * num_bins..(t + 1) * num_bins];
        for (dst, c) in row.iter_mut().zip(spectrum.iter()) {
            *dst = c.norm_sqr();
        }
    }

    Spectrogram { num_time_steps, num_bins, power }
}

/// Energy (magnitude²) at an arbitrary frequency `freq_hz`, computed as a single
/// DFT bin over `samples[start .. start + len]` via a rotating-phasor
/// (Goertzel-style) accumulation. Returns `0.0` if the window falls outside the
/// sample buffer.
///
/// Unlike the [`Spectrogram`] (fixed 3.125 Hz grid), this resolves *any* centre
/// frequency and sample offset, so the sync fine-refinement and per-symbol tone
/// extraction can read exactly `fc + tone·6.25 Hz` at a sub-step time alignment.
/// provenance: single-bin DFT / Goertzel algorithm — standard public-domain DSP
/// (Goertzel, "An Algorithm for the Evaluation of Finite Trigonometric Series",
/// Amer. Math. Monthly 1958).
pub fn tone_power(samples: &[f32], start: isize, len: usize, freq_hz: f64, sample_rate: u32) -> f32 {
    if start < 0 || (start as usize).saturating_add(len) > samples.len() {
        return 0.0;
    }
    let start = start as usize;
    let w = 2.0 * PI * freq_hz / sample_rate as f64;
    let (cw, sw) = (w.cos(), w.sin());
    // Rotating phasor e^{-jwn}: (pc, ps) = (cos wn, sin wn).
    let (mut pc, mut ps) = (1.0f64, 0.0f64);
    let (mut re, mut im) = (0.0f64, 0.0f64);
    for n in 0..len {
        let x = samples[start + n] as f64;
        re += x * pc;
        im -= x * ps;
        let npc = pc * cw - ps * sw;
        let nps = ps * cw + pc * sw;
        pc = npc;
        ps = nps;
    }
    (re * re + im * im) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pure real cosine of `freq_hz` and amplitude `amp` over `n` samples.
    fn cosine(freq_hz: f64, amp: f32, n: usize, sample_rate: u32) -> Vec<f32> {
        (0..n)
            .map(|i| amp * (2.0 * PI * freq_hz * i as f64 / sample_rate as f64).cos() as f32)
            .collect()
    }

    /// Geometry: a full 15 s slot (180000 samples) yields 372 quarter-symbol
    /// windows (WB2FKO's stated count), 1921 one-sided bins, 3.125 Hz each.
    #[test]
    fn window_geometry_matches_wb2fko() {
        let samples = vec![0.0f32; 180_000];
        let spec = compute_spectrogram(&samples, 12_000);
        assert_eq!(spec.num_time_steps, 372, "quarter-symbol window count");
        assert_eq!(spec.num_bins, FFT_LEN / 2 + 1);
        assert_eq!(spec.num_bins, 1921);
        assert!((BIN_HZ - 3.125).abs() < 1e-9, "bin width 3.125 Hz");
        assert_eq!(HOP_SAMPLES, 480);
        assert_eq!(SYMBOL_SAMPLES, 1920);
    }

    /// A pure sinusoid produces a spectrogram peak in the correct bin (frequency
    /// accuracy within one bin). 1000 Hz → bin 1000/3.125 = 320.
    #[test]
    fn pure_sinusoid_peaks_in_correct_bin() {
        let samples = cosine(1000.0, 1.0, 180_000, 12_000);
        let spec = compute_spectrogram(&samples, 12_000);
        // Inspect a mid-slot time step.
        let t = 100;
        let row = &spec.power[t * spec.num_bins..(t + 1) * spec.num_bins];
        let (peak_bin, _) = row
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap();
        let expected = (1000.0 / BIN_HZ).round() as usize; // 320
        assert_eq!(expected, 320);
        assert!(
            (peak_bin as isize - expected as isize).abs() <= 1,
            "peak bin {peak_bin} not within 1 of expected {expected}"
        );
    }

    /// `tone_power` peaks at the true tone and is small at a neighbouring
    /// (non-present) FT-8 tone, and its on-bin value matches the closed-form
    /// `(A·N/2)²` energy of a cosine at an exact DFT bin (Parseval sanity).
    #[test]
    fn tone_power_matches_closed_form_and_is_selective() {
        let n = SYMBOL_SAMPLES;
        let amp = 1.0f32;
        // 1500 Hz is an exact bin of a 1920-sample window (1500 = 240·6.25).
        let samples = cosine(1500.0, amp, n, 12_000);
        let on = tone_power(&samples, 0, n, 1500.0, 12_000);
        let off = tone_power(&samples, 0, n, 1500.0 + 6.25, 12_000);

        // Closed form for a real cosine at an exact bin: |DFT| = A·N/2.
        let expected = (amp as f64 * n as f64 / 2.0).powi(2) as f32;
        let rel = (on - expected).abs() / expected;
        assert!(rel < 0.02, "on-bin power {on} vs closed form {expected} (rel {rel})");
        assert!(on > off * 50.0, "tone not selective: on {on} vs off {off}");
    }

    /// `tone_power` returns 0 for a window that runs past the buffer end or
    /// starts before it, rather than panicking.
    #[test]
    fn tone_power_out_of_bounds_is_zero() {
        let samples = vec![1.0f32; 100];
        assert_eq!(tone_power(&samples, -5, 50, 1000.0, 12_000), 0.0);
        assert_eq!(tone_power(&samples, 80, 50, 1000.0, 12_000), 0.0);
    }

    /// The 12 kHz rate is enforced, not silently ignored.
    #[test]
    #[should_panic(expected = "requires 12000 Hz")]
    fn wrong_sample_rate_panics() {
        let samples = vec![0.0f32; 4000];
        let _ = compute_spectrogram(&samples, 8_000);
    }

    /// An input shorter than one symbol yields zero time steps (no panic).
    #[test]
    fn short_input_yields_no_time_steps() {
        let samples = vec![0.0f32; SYMBOL_SAMPLES - 1];
        let spec = compute_spectrogram(&samples, 12_000);
        assert_eq!(spec.num_time_steps, 0);
        assert!(spec.power.is_empty());
    }
}
