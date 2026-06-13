//! Free-running SSTV decoder (`Decoder.cs`), minus the display/scope buffer
//! and waterfall `DrawLines` calls, which are presentation-only and never
//! influence decode control flow. Only the `image_buffer` write path is kept.

use crate::color::PixelBuffer;
use crate::demodulator::{Demodulator, SyncPulseWidth};
use crate::dsp::SimpleMovingAverage;
use crate::modes::{Mode, PaulDon, RawDecoder, Robot36};

/// Width handed to `RawDecoder` (the C# scope buffer is 800 wide). Only the
/// raw fallback consumes it; Robot 36 / PD ignore it.
const SCOPE_WIDTH: usize = 800;

const SCAN_LINE_COUNT: usize = 4;
const SYNC_PULSE_COUNT: usize = SCAN_LINE_COUNT + 1; // 5

pub struct Decoder {
    pulse_filter: SimpleMovingAverage,
    demodulator: Demodulator,
    pixel_buffer: PixelBuffer,
    image_buffer: PixelBuffer,
    scan_line_buffer: Vec<f32>,
    scratch_buffer: Vec<f32>,

    modes: Vec<Box<dyn Mode>>,
    raw_index: usize,
    sync5: Vec<usize>,
    sync9: Vec<usize>,
    sync20: Vec<usize>,
    current_mode: usize,

    last5_sync: [i64; SYNC_PULSE_COUNT],
    last9_sync: [i64; SYNC_PULSE_COUNT],
    last20_sync: [i64; SYNC_PULSE_COUNT],
    last5_lines: [i64; SCAN_LINE_COUNT],
    last9_lines: [i64; SCAN_LINE_COUNT],
    last20_lines: [i64; SCAN_LINE_COUNT],
    last5_foff: [f32; SYNC_PULSE_COUNT],
    last9_foff: [f32; SYNC_PULSE_COUNT],
    last20_foff: [f32; SYNC_PULSE_COUNT],
    vis_bit_freqs: [f32; 10],

    pulse_filter_delay: i64,
    scan_line_min_samples: i64,
    sync_pulse_tolerance_samples: i64,
    scan_line_tolerance_samples: i64,
    leader_tone_samples: i64,
    leader_tone_tolerance_samples: i64,
    transition_samples: i64,
    vis_code_bit_samples: i64,
    vis_code_samples: i64,

    current_sample: i64,
    leader_break_index: i64,
    last_sync_pulse_index: i64,
    current_scan_line_samples: i64,
    last_frequency_offset: f32,
}

impl Decoder {
    pub fn new(sample_rate: u32) -> Self {
        let rate = sample_rate as f64;
        let round = |s: f64| (s * rate).round() as i64;

        let pulse_filter_samples = (round(0.0025) | 1) as usize;
        let pulse_filter_delay = (pulse_filter_samples as i64 - 1) / 2;
        let pulse_filter = SimpleMovingAverage::new(pulse_filter_samples);

        let scan_line_max_samples = round(7.0) as usize;
        let scratch_buffer_samples = round(1.1) as usize;

        // Build the mode table. 5 ms-sync modes (Wraase/Martin) are out of
        // scope for bd-st5n; the list is left empty.
        let mut modes: Vec<Box<dyn Mode>> = Vec::new();
        let robot36 = Robot36::new(sample_rate);
        let current_scan_line_samples = robot36.scan_line_samples();
        modes.push(Box::new(robot36));
        let robot36_index = 0usize;
        let sync9 = vec![robot36_index];

        let pd_specs = [
            ("50", 93, 320, 256, 0.09152),
            ("90", 99, 320, 256, 0.17024),
            ("120", 95, 640, 496, 0.1216),
            ("160", 98, 512, 400, 0.195584),
            ("180", 96, 640, 496, 0.18304),
            ("240", 97, 640, 496, 0.24448),
            ("290", 94, 800, 616, 0.2288),
        ];
        let mut sync20 = Vec::new();
        for (name, code, w, h, ch) in pd_specs {
            modes.push(Box::new(PaulDon::new(name, code, w, h, ch, sample_rate)));
            sync20.push(modes.len() - 1);
        }

        modes.push(Box::new(RawDecoder::new("Raw", sample_rate)));
        let raw_index = modes.len() - 1;

        // PD 290 is the largest mode (800x616); size both buffers for it.
        let pixel_buffer = PixelBuffer::new(800, 2);
        let mut image_buffer = PixelBuffer::new(800, 616);
        image_buffer.line = -1;

        Self {
            pulse_filter,
            demodulator: Demodulator::new(sample_rate),
            pixel_buffer,
            image_buffer,
            scan_line_buffer: vec![0.0; scan_line_max_samples],
            scratch_buffer: vec![0.0; scratch_buffer_samples],
            modes,
            raw_index,
            sync5: Vec::new(),
            sync9,
            sync20,
            current_mode: robot36_index,
            last5_sync: [0; SYNC_PULSE_COUNT],
            last9_sync: [0; SYNC_PULSE_COUNT],
            last20_sync: [0; SYNC_PULSE_COUNT],
            last5_lines: [0; SCAN_LINE_COUNT],
            last9_lines: [0; SCAN_LINE_COUNT],
            last20_lines: [0; SCAN_LINE_COUNT],
            last5_foff: [0.0; SYNC_PULSE_COUNT],
            last9_foff: [0.0; SYNC_PULSE_COUNT],
            last20_foff: [0.0; SYNC_PULSE_COUNT],
            vis_bit_freqs: [0.0; 10],
            pulse_filter_delay,
            scan_line_min_samples: round(0.05),
            sync_pulse_tolerance_samples: round(0.03),
            scan_line_tolerance_samples: round(0.001),
            leader_tone_samples: round(0.3),
            leader_tone_tolerance_samples: round(0.3 * 0.2),
            transition_samples: round(0.0005),
            vis_code_bit_samples: round(0.03),
            vis_code_samples: round(0.3),
            current_sample: 0,
            leader_break_index: 0,
            last_sync_pulse_index: 0,
            current_scan_line_samples,
            last_frequency_offset: 0.0,
        }
    }

    fn scan_line_mean(lines: &[i64]) -> f64 {
        lines.iter().map(|&d| d as f64).sum::<f64>() / lines.len() as f64
    }

    fn scan_line_std_dev(lines: &[i64], mean: f64) -> f64 {
        let var = lines
            .iter()
            .map(|&d| (d as f64 - mean) * (d as f64 - mean))
            .sum::<f64>()
            / lines.len() as f64;
        var.sqrt()
    }

    fn frequency_offset_mean(offsets: &[f32]) -> f64 {
        offsets.iter().map(|&d| d as f64).sum::<f64>() / offsets.len() as f64
    }

    /// Pick the mode in `list` whose scan-line length best matches `line`.
    fn detect_mode(&self, list: &[usize], line: i64) -> usize {
        let mut best = self.raw_index;
        let mut best_dist = i64::MAX;
        for &mi in list {
            let dist = (line - self.modes[mi].scan_line_samples()).abs();
            if dist <= self.scan_line_tolerance_samples && dist < best_dist {
                best_dist = dist;
                best = mi;
            }
        }
        best
    }

    fn find_mode_by_code(&self, list: &[usize], code: i32) -> Option<usize> {
        list.iter().copied().find(|&mi| self.modes[mi].vis_code() == code)
    }

    /// Copy the freshly decoded `pixel_buffer` rows into `image_buffer`
    /// (display/scope copy from the C# original is intentionally dropped).
    fn copy_lines(&mut self, okay: bool) {
        if !okay {
            return;
        }
        let line = self.image_buffer.line;
        if line >= 0
            && line < self.image_buffer.height as i64
            && self.image_buffer.width == self.pixel_buffer.width
        {
            let width = self.image_buffer.width;
            let mut row = 0;
            while row < self.pixel_buffer.height && self.image_buffer.line < self.image_buffer.height as i64 {
                let dst = self.image_buffer.line as usize * width;
                let src = row * width;
                self.image_buffer.pixels[dst..dst + width]
                    .copy_from_slice(&self.pixel_buffer.pixels[src..src + width]);
                row += 1;
                self.image_buffer.line += 1;
            }
        }
    }

    fn adjust_sync_pulses(pulses: &mut [i64], shift: i64) {
        for p in pulses.iter_mut() {
            *p -= shift;
        }
    }

    fn shift_samples(&mut self, shift: i64) {
        if shift <= 0 || shift > self.current_sample {
            return;
        }
        self.current_sample -= shift;
        self.leader_break_index -= shift;
        self.last_sync_pulse_index -= shift;
        Self::adjust_sync_pulses(&mut self.last5_sync, shift);
        Self::adjust_sync_pulses(&mut self.last9_sync, shift);
        Self::adjust_sync_pulses(&mut self.last20_sync, shift);
        let n = self.current_sample as usize;
        self.scan_line_buffer.copy_within(shift as usize..shift as usize + n, 0);
    }

    /// Decode `pixel_buffer` for the given mode at `sync_index`, then copy.
    fn decode_and_copy(&mut self, mode_index: usize, sync_index: i64, scan_line_samples: i64, foff: f32) {
        let okay = {
            let mode = &mut self.modes[mode_index];
            mode.decode_scan_line(
                &mut self.pixel_buffer,
                &mut self.scratch_buffer,
                &self.scan_line_buffer,
                SCOPE_WIDTH,
                sync_index,
                scan_line_samples,
                foff,
            )
        };
        self.copy_lines(okay);
    }

    fn handle_header(&mut self) -> bool {
        let break_pulse_index = self.leader_break_index;
        if self.leader_break_index < self.vis_code_bit_samples + self.leader_tone_tolerance_samples
            || self.current_sample
                < self.leader_break_index
                    + self.leader_tone_samples
                    + self.leader_tone_tolerance_samples
                    + self.vis_code_samples
                    + self.vis_code_bit_samples
        {
            return false;
        }
        self.leader_break_index = 0;

        let center_frequency = 1900.0f32;
        let leader_tone_frequency = 1900.0f32;
        let tolerance_frequency = 50.0f32;
        let half_band_width = 400.0f32;
        let buf = &self.scan_line_buffer;

        let mut pre_break_freq = 0.0f32;
        for i in 0..self.leader_tone_tolerance_samples {
            let idx = break_pulse_index - self.vis_code_bit_samples - self.leader_tone_tolerance_samples + i;
            pre_break_freq += buf[idx as usize];
        }
        pre_break_freq =
            pre_break_freq * half_band_width / self.leader_tone_tolerance_samples as f32 + center_frequency;
        if (pre_break_freq - leader_tone_frequency).abs() > tolerance_frequency {
            return false;
        }

        let mut leader_freq = 0.0f32;
        for i in self.transition_samples..self.leader_tone_samples - self.leader_tone_tolerance_samples {
            leader_freq += buf[(break_pulse_index + i) as usize];
        }
        let leader_freq_offset = leader_freq
            / (self.leader_tone_samples - self.transition_samples - self.leader_tone_tolerance_samples) as f32;
        leader_freq = leader_freq_offset * half_band_width + center_frequency;
        if (leader_freq - leader_tone_frequency).abs() > tolerance_frequency {
            return false;
        }

        let stop_bit_frequency = 1200.0f32;
        let pulse_threshold_frequency = (stop_bit_frequency + leader_tone_frequency) / 2.0;
        let pulse_threshold_value = (pulse_threshold_frequency - center_frequency) / half_band_width;
        let mut vis_begin_index = break_pulse_index + self.leader_tone_samples - self.leader_tone_tolerance_samples;
        let vis_end_index = break_pulse_index
            + self.leader_tone_samples
            + self.leader_tone_tolerance_samples
            + self.vis_code_bit_samples;
        let pf_len = self.pulse_filter.len() as i64;
        for _ in 0..pf_len {
            let v = buf[vis_begin_index as usize] - leader_freq_offset;
            self.pulse_filter.avg(v);
            vis_begin_index += 1;
        }
        loop {
            vis_begin_index += 1;
            if vis_begin_index >= vis_end_index {
                break;
            }
            let v = buf[vis_begin_index as usize] - leader_freq_offset;
            if self.pulse_filter.avg(v) < pulse_threshold_value {
                break;
            }
        }
        if vis_begin_index >= vis_end_index {
            return false;
        }
        vis_begin_index -= self.pulse_filter_delay;
        let mut vis_end_index = vis_begin_index + self.vis_code_samples;

        self.vis_bit_freqs = [0.0; 10];
        for j in 0..10i64 {
            for i in self.transition_samples..self.vis_code_bit_samples - self.transition_samples {
                let idx = vis_begin_index + self.vis_code_bit_samples * j + i;
                self.vis_bit_freqs[j as usize] += buf[idx as usize] - leader_freq_offset;
            }
        }
        for i in 0..10 {
            self.vis_bit_freqs[i] = self.vis_bit_freqs[i] * half_band_width
                / (self.vis_code_bit_samples - 2 * self.transition_samples) as f32
                + center_frequency;
        }
        if (self.vis_bit_freqs[0] - stop_bit_frequency).abs() > tolerance_frequency
            || (self.vis_bit_freqs[9] - stop_bit_frequency).abs() > tolerance_frequency
        {
            return false;
        }
        let one_bit_frequency = 1100.0f32;
        let zero_bit_frequency = 1300.0f32;
        for i in 1..9 {
            if (self.vis_bit_freqs[i] - one_bit_frequency).abs() > tolerance_frequency
                && (self.vis_bit_freqs[i] - zero_bit_frequency).abs() > tolerance_frequency
            {
                return false;
            }
        }
        let mut vis_code = 0i32;
        for i in 0..8 {
            vis_code |= ((self.vis_bit_freqs[i + 1] < stop_bit_frequency) as i32) << i;
        }
        let mut check = true;
        for i in 0..8 {
            check ^= (vis_code & (1 << i)) != 0;
        }
        vis_code &= 127;
        if !check {
            return false;
        }

        let sync_pulse_frequency = 1200.0f32;
        let sync_porch_frequency = 1500.0f32;
        let sync_threshold_frequency = (sync_pulse_frequency + sync_porch_frequency) / 2.0;
        let sync_threshold_value = (sync_threshold_frequency - center_frequency) / half_band_width;
        let mut sync_pulse_index = vis_end_index - self.vis_code_bit_samples;
        let sync_pulse_max_index = vis_end_index + self.vis_code_bit_samples;
        for _ in 0..pf_len {
            let v = buf[sync_pulse_index as usize] - leader_freq_offset;
            self.pulse_filter.avg(v);
            sync_pulse_index += 1;
        }
        loop {
            sync_pulse_index += 1;
            if sync_pulse_index >= sync_pulse_max_index {
                break;
            }
            let v = buf[sync_pulse_index as usize] - leader_freq_offset;
            if self.pulse_filter.avg(v) > sync_threshold_value {
                break;
            }
        }
        if sync_pulse_index >= sync_pulse_max_index {
            return false;
        }
        sync_pulse_index -= self.pulse_filter_delay;
        let _ = &mut vis_end_index;

        // Resolve mode by VIS code; pick the matching sync-width tracking set.
        let (mode_index, which): (usize, u8) =
            if let Some(mi) = self.find_mode_by_code(&self.sync5.clone(), vis_code) {
                (mi, 5)
            } else if let Some(mi) = self.find_mode_by_code(&self.sync9.clone(), vis_code) {
                (mi, 9)
            } else if let Some(mi) = self.find_mode_by_code(&self.sync20.clone(), vis_code) {
                (mi, 20)
            } else {
                return false;
            };

        self.modes[mode_index].reset_state();
        let mode_width = self.modes[mode_index].width();
        let mode_height = self.modes[mode_index].height();
        let first_sync = self.modes[mode_index].first_sync_pulse_index();
        let first_pixel = self.modes[mode_index].first_pixel_sample_index();
        let mode_scan_line = self.modes[mode_index].scan_line_samples();

        self.image_buffer.width = mode_width as usize;
        self.image_buffer.height = mode_height as usize;
        self.image_buffer.line = 0;
        self.current_mode = mode_index;
        self.last_sync_pulse_index = sync_pulse_index + first_sync;
        self.current_scan_line_samples = mode_scan_line;
        self.last_frequency_offset = leader_freq_offset;

        let mut oldest = self.last_sync_pulse_index - (SYNC_PULSE_COUNT as i64 - 1) * mode_scan_line;
        if first_sync > 0 {
            oldest -= mode_scan_line;
        }
        let pulses: &mut [i64] = match which {
            5 => &mut self.last5_sync,
            9 => &mut self.last9_sync,
            _ => &mut self.last20_sync,
        };
        for (i, p) in pulses.iter_mut().enumerate() {
            *p = oldest + i as i64 * mode_scan_line;
        }
        let lines: &mut [i64] = match which {
            5 => &mut self.last5_lines,
            9 => &mut self.last9_lines,
            _ => &mut self.last20_lines,
        };
        lines.fill(mode_scan_line);

        self.shift_samples(self.last_sync_pulse_index + first_pixel);
        true
    }

    fn process_sync_pulse(&mut self, which: u8, latest_sync_index: i64) -> bool {
        // Shift tracking arrays and append the latest pulse.
        let foff_now = self.demodulator.frequency_offset;
        {
            let sync = match which {
                5 => &mut self.last5_sync,
                9 => &mut self.last9_sync,
                _ => &mut self.last20_sync,
            };
            for i in 1..sync.len() {
                sync[i - 1] = sync[i];
            }
            sync[SYNC_PULSE_COUNT - 1] = latest_sync_index;
        }
        {
            let sync_last = self.sync_at(which, SYNC_PULSE_COUNT - 1);
            let sync_prev = self.sync_at(which, SYNC_PULSE_COUNT - 2);
            let lines = match which {
                5 => &mut self.last5_lines,
                9 => &mut self.last9_lines,
                _ => &mut self.last20_lines,
            };
            for i in 1..lines.len() {
                lines[i - 1] = lines[i];
            }
            lines[SCAN_LINE_COUNT - 1] = sync_last - sync_prev;
        }
        {
            let foff = match which {
                5 => &mut self.last5_foff,
                9 => &mut self.last9_foff,
                _ => &mut self.last20_foff,
            };
            for i in 1..foff.len() {
                foff[i - 1] = foff[i];
            }
            foff[SYNC_PULSE_COUNT - 1] = foff_now;
        }

        let lines_copy: [i64; SCAN_LINE_COUNT] = *match which {
            5 => &self.last5_lines,
            9 => &self.last9_lines,
            _ => &self.last20_lines,
        };
        if lines_copy[0] == 0 {
            return false;
        }
        let mean = Self::scan_line_mean(&lines_copy);
        let scan_line_samples = mean.round() as i64;
        if scan_line_samples < self.scan_line_min_samples
            || scan_line_samples > self.scratch_buffer.len() as i64
        {
            return false;
        }
        if Self::scan_line_std_dev(&lines_copy, mean) > self.scan_line_tolerance_samples as f64 {
            return false;
        }

        let sync_newest = self.sync_at(which, SYNC_PULSE_COUNT - 1);
        let sync_oldest = self.sync_at(which, 0);

        let mut picture_changed = false;
        if self.image_buffer.line >= 0 && self.image_buffer.line < self.image_buffer.height as i64 {
            if self.current_mode != self.raw_index
                && (scan_line_samples - self.modes[self.current_mode].scan_line_samples()).abs()
                    > self.scan_line_tolerance_samples
            {
                return false;
            }
        } else {
            let list: Vec<usize> = match which {
                5 => self.sync5.clone(),
                9 => self.sync9.clone(),
                _ => self.sync20.clone(),
            };
            let prev_mode = self.current_mode;
            self.current_mode = self.detect_mode(&list, scan_line_samples);
            picture_changed = self.current_mode != prev_mode
                || (self.current_scan_line_samples - scan_line_samples).abs()
                    > self.scan_line_tolerance_samples
                || (self.last_sync_pulse_index + scan_line_samples - sync_newest).abs()
                    > self.sync_pulse_tolerance_samples;
        }

        let foff_copy: [f32; SYNC_PULSE_COUNT] = *match which {
            5 => &self.last5_foff,
            9 => &self.last9_foff,
            _ => &self.last20_foff,
        };
        let frequency_offset = Self::frequency_offset_mean(&foff_copy) as f32;

        // Back-fill extrapolated lines on a fresh picture.
        if sync_oldest >= scan_line_samples && picture_changed {
            let end_pulse = sync_oldest;
            let extrapolate = end_pulse / scan_line_samples;
            let first_pulse = end_pulse - extrapolate * scan_line_samples;
            let mut pulse_index = first_pulse;
            let ci = self.current_mode;
            while pulse_index < end_pulse {
                self.decode_and_copy(ci, pulse_index, scan_line_samples, frequency_offset);
                pulse_index += scan_line_samples;
            }
        }

        let start = if picture_changed { 0 } else { SCAN_LINE_COUNT - 1 };
        let ci = self.current_mode;
        #[allow(clippy::needless_range_loop)] // i indexes both sync_at(which, i) and lines_copy
        for i in start..SCAN_LINE_COUNT {
            let sync_i = self.sync_at(which, i);
            let len_i = lines_copy[i];
            self.decode_and_copy(ci, sync_i, len_i, frequency_offset);
        }

        self.last_sync_pulse_index = sync_newest;
        self.current_scan_line_samples = scan_line_samples;
        self.last_frequency_offset = frequency_offset;
        let first_pixel = self.modes[self.current_mode].first_pixel_sample_index();
        self.shift_samples(self.last_sync_pulse_index + first_pixel);
        true
    }

    fn sync_at(&self, which: u8, i: usize) -> i64 {
        match which {
            5 => self.last5_sync[i],
            9 => self.last9_sync[i],
            _ => self.last20_sync[i],
        }
    }

    /// Feed mono float samples (-1..1). Returns true if new image lines were produced.
    pub fn process(&mut self, record_buffer: &mut [f32]) -> bool {
        let mut new_lines = false;
        let sync_pulse_detected = self.demodulator.process(record_buffer);
        let mut sync_pulse_index = self.current_sample + self.demodulator.sync_pulse_offset as i64;
        for &sample in record_buffer.iter() {
            self.scan_line_buffer[self.current_sample as usize] = sample;
            self.current_sample += 1;
            if self.current_sample >= self.scan_line_buffer.len() as i64 {
                let s = self.current_scan_line_samples;
                self.shift_samples(s);
                sync_pulse_index -= s;
            }
        }

        if sync_pulse_detected {
            match self.demodulator.sync_pulse_width {
                SyncPulseWidth::Five => {
                    new_lines = self.process_sync_pulse(5, sync_pulse_index);
                }
                SyncPulseWidth::Nine => {
                    self.leader_break_index = sync_pulse_index;
                    new_lines = self.process_sync_pulse(9, sync_pulse_index);
                }
                SyncPulseWidth::Twenty => {
                    self.leader_break_index = sync_pulse_index;
                    new_lines = self.process_sync_pulse(20, sync_pulse_index);
                }
            }
        } else if self.handle_header() {
            new_lines = true;
        } else if self.current_sample
            > self.last_sync_pulse_index + (self.current_scan_line_samples * 5) / 4
        {
            let ci = self.current_mode;
            let lspi = self.last_sync_pulse_index;
            let csls = self.current_scan_line_samples;
            let foff = self.last_frequency_offset;
            self.decode_and_copy(ci, lspi, csls, foff);
            self.last_sync_pulse_index += self.current_scan_line_samples;
            new_lines = true;
        }

        new_lines
    }

    /// True once a full image has been decoded (cursor reached image height).
    pub fn is_complete(&self) -> bool {
        self.image_buffer.line >= 0 && self.image_buffer.line >= self.image_buffer.height as i64
    }

    pub fn current_mode_name(&self) -> &str {
        self.modes[self.current_mode].name()
    }

    /// The decoded image (valid once `is_complete`): (pixels, width, height).
    pub fn image(&self) -> (Vec<u32>, usize, usize) {
        let w = self.image_buffer.width;
        let h = self.image_buffer.height;
        (self.image_buffer.pixels[..w * h].to_vec(), w, h)
    }
}
