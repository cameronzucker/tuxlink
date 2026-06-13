//! FM demodulator + sync-pulse classifier (`Demodulator.cs`).
//!
//! Shifts the SSTV passband to baseband with a complex NCO, low-pass filters
//! it, runs an FM discriminator, and classifies sync pulses by width
//! (5/9/20 ms). `process` overwrites `buffer` in place with the demodulated
//! frequency values and reports the *last* sync pulse seen in the buffer.

use crate::dsp::{
    filter, ComplexConvolution, Complex, Delay, FrequencyModulation, Kaiser, Phasor, SchmittTrigger,
    SimpleMovingAverage,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncPulseWidth {
    Five,
    Nine,
    Twenty,
}

const SYNC_PULSE_FREQUENCY: f64 = 1200.0;
const BLACK_FREQUENCY: f64 = 1500.0;
const WHITE_FREQUENCY: f64 = 2300.0;

pub struct Demodulator {
    sync_pulse_filter: SimpleMovingAverage,
    base_band_low_pass: ComplexConvolution,
    frequency_modulation: FrequencyModulation,
    sync_pulse_trigger: SchmittTrigger,
    base_band_oscillator: Phasor,
    sync_pulse_value_delay: Delay,
    scan_line_bandwidth: f64,
    center_frequency: f64,
    sync_pulse_frequency_value: f32,
    sync_pulse_frequency_tolerance: f32,
    sync_pulse_5ms_min: i32,
    sync_pulse_5ms_max: i32,
    sync_pulse_9ms_max: i32,
    sync_pulse_20ms_max: i32,
    sync_pulse_filter_delay: i32,
    sync_pulse_counter: i32,

    // Outputs of the last `process` call.
    pub sync_pulse_width: SyncPulseWidth,
    pub sync_pulse_offset: i32,
    pub frequency_offset: f32,
}

impl Demodulator {
    pub fn new(sample_rate: u32) -> Self {
        let rate = sample_rate as f64;
        let scan_line_bandwidth = WHITE_FREQUENCY - BLACK_FREQUENCY;
        let frequency_modulation = FrequencyModulation::new(scan_line_bandwidth, rate);

        let sync_pulse_5ms = 0.005;
        let sync_pulse_9ms = 0.009;
        let sync_pulse_20ms = 0.020;
        let sync_pulse_5ms_min_seconds = sync_pulse_5ms / 2.0;
        let sync_pulse_5ms_max_seconds = (sync_pulse_5ms + sync_pulse_9ms) / 2.0;
        let sync_pulse_9ms_max_seconds = (sync_pulse_9ms + sync_pulse_20ms) / 2.0;
        let sync_pulse_20ms_max_seconds = sync_pulse_20ms + sync_pulse_5ms;
        let sync_pulse_5ms_min = (sync_pulse_5ms_min_seconds * rate).round() as i32;
        let sync_pulse_5ms_max = (sync_pulse_5ms_max_seconds * rate).round() as i32;
        let sync_pulse_9ms_max = (sync_pulse_9ms_max_seconds * rate).round() as i32;
        let sync_pulse_20ms_max = (sync_pulse_20ms_max_seconds * rate).round() as i32;

        let sync_pulse_filter_seconds = sync_pulse_5ms / 2.0;
        let sync_pulse_filter_samples = ((sync_pulse_filter_seconds * rate).round() as i32) | 1;
        let sync_pulse_filter_delay = (sync_pulse_filter_samples - 1) / 2;
        let sync_pulse_filter = SimpleMovingAverage::new(sync_pulse_filter_samples as usize);
        let sync_pulse_value_delay = Delay::new(sync_pulse_filter_samples as usize);

        let lowest_frequency = 1000.0;
        let highest_frequency = 2800.0;
        let cutoff_frequency = (highest_frequency - lowest_frequency) / 2.0;
        let base_band_low_pass_seconds = 0.002;
        let base_band_low_pass_samples = ((base_band_low_pass_seconds * rate).round() as i32) | 1;
        let mut base_band_low_pass = ComplexConvolution::new(base_band_low_pass_samples as usize);
        let kaiser = Kaiser;
        let n = base_band_low_pass.len();
        for i in 0..n {
            base_band_low_pass.taps[i] = (Kaiser::window(2.0, i, n)
                * filter::low_pass(cutoff_frequency, rate, i, n))
                as f32;
        }
        let _ = kaiser;
        let center_frequency = (lowest_frequency + highest_frequency) / 2.0;
        let base_band_oscillator = Phasor::new(-center_frequency, rate);

        let normalize = |frequency: f64| (frequency - center_frequency) * 2.0 / scan_line_bandwidth;
        let sync_pulse_frequency_value = normalize(SYNC_PULSE_FREQUENCY) as f32;
        let sync_pulse_frequency_tolerance = (50.0 * 2.0 / scan_line_bandwidth) as f32;
        let sync_porch_frequency = 1500.0;
        let sync_high_frequency = (SYNC_PULSE_FREQUENCY + sync_porch_frequency) / 2.0;
        let sync_low_frequency = (SYNC_PULSE_FREQUENCY + sync_high_frequency) / 2.0;
        let sync_low_value = normalize(sync_low_frequency);
        let sync_high_value = normalize(sync_high_frequency);
        let sync_pulse_trigger = SchmittTrigger::new(sync_low_value as f32, sync_high_value as f32);

        Self {
            sync_pulse_filter,
            base_band_low_pass,
            frequency_modulation,
            sync_pulse_trigger,
            base_band_oscillator,
            sync_pulse_value_delay,
            scan_line_bandwidth,
            center_frequency,
            sync_pulse_frequency_value,
            sync_pulse_frequency_tolerance,
            sync_pulse_5ms_min,
            sync_pulse_5ms_max,
            sync_pulse_9ms_max,
            sync_pulse_20ms_max,
            sync_pulse_filter_delay,
            sync_pulse_counter: 0,
            sync_pulse_width: SyncPulseWidth::Nine,
            sync_pulse_offset: 0,
            frequency_offset: 0.0,
        }
    }

    /// Demodulate `buffer` in place (mono). Returns true if a sync pulse was
    /// detected; the pulse's width/offset/frequency-offset are then readable.
    pub fn process(&mut self, buffer: &mut [f32]) -> bool {
        let mut sync_pulse_detected = false;
        for (i, slot) in buffer.iter_mut().enumerate() {
            let base_band_in = Complex::new(*slot, 0.0);
            let rotated = self.base_band_oscillator.rotate();
            let base_band = self.base_band_low_pass.push(base_band_in * rotated);
            let frequency_value = self.frequency_modulation.demod(base_band);
            let sync_pulse_value = self.sync_pulse_filter.avg(frequency_value);
            let sync_pulse_delayed_value = self.sync_pulse_value_delay.push(sync_pulse_value);
            *slot = frequency_value;
            if !self.sync_pulse_trigger.latch(sync_pulse_value) {
                self.sync_pulse_counter += 1;
            } else if self.sync_pulse_counter < self.sync_pulse_5ms_min
                || self.sync_pulse_counter > self.sync_pulse_20ms_max
                || (sync_pulse_delayed_value - self.sync_pulse_frequency_value).abs()
                    > self.sync_pulse_frequency_tolerance
            {
                self.sync_pulse_counter = 0;
            } else {
                self.sync_pulse_width = if self.sync_pulse_counter < self.sync_pulse_5ms_max {
                    SyncPulseWidth::Five
                } else if self.sync_pulse_counter < self.sync_pulse_9ms_max {
                    SyncPulseWidth::Nine
                } else {
                    SyncPulseWidth::Twenty
                };
                self.sync_pulse_offset = i as i32 - self.sync_pulse_filter_delay;
                self.frequency_offset = sync_pulse_delayed_value - self.sync_pulse_frequency_value;
                sync_pulse_detected = true;
                self.sync_pulse_counter = 0;
            }
        }
        // Silence unused-field warnings for values kept only to mirror the C# layout.
        let _ = (self.scan_line_bandwidth, self.center_frequency);
        sync_pulse_detected
    }
}
