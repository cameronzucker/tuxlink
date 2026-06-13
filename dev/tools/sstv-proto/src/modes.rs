//! Per-mode scan-line decoders (`BaseMode.cs`, `Robot_36_Color.cs`,
//! `PaulDon.cs`, `RawDecoder.cs`). Each mode knows its timing layout and
//! demodulates one (or two) image rows from the FM-demodulated scan-line
//! buffer into the working `PixelBuffer`.

use crate::color::{self, PixelBuffer};
use crate::dsp::ExponentialMovingAverage;

fn round_samples(seconds: f64, rate: f64) -> usize {
    (seconds * rate).round() as usize
}

fn freq_to_level(frequency: f32, offset: f32) -> f32 {
    0.5 * (frequency - offset + 1.0)
}

/// `IMode` — the decode interface. Indices are signed because the decoder
/// extrapolates sync-pulse positions that can be negative before clamping.
pub trait Mode {
    fn name(&self) -> &str;
    fn vis_code(&self) -> i32;
    fn width(&self) -> i64;
    fn height(&self) -> i64;
    fn first_pixel_sample_index(&self) -> i64;
    fn first_sync_pulse_index(&self) -> i64;
    fn scan_line_samples(&self) -> i64;
    fn reset_state(&mut self);

    /// Returns true if a scan line (one or more image rows) was produced into
    /// `pixel_buffer`.
    #[allow(clippy::too_many_arguments)]
    fn decode_scan_line(
        &mut self,
        pixel_buffer: &mut PixelBuffer,
        scratch: &mut [f32],
        scan_line: &[f32],
        scope_width: usize,
        sync_pulse_index: i64,
        scan_line_samples: i64,
        frequency_offset: f32,
    ) -> bool;
}

// ---------------------------------------------------------------------------
// Robot 36 Color
// ---------------------------------------------------------------------------

pub struct Robot36 {
    lp: ExponentialMovingAverage,
    horizontal_pixels: usize,
    scan_line_samples: usize,
    luminance_samples: usize,
    separator_samples: usize,
    begin_samples: usize,
    luminance_begin: usize,
    separator_begin: usize,
    chrominance_begin: usize,
    chrominance_samples: usize,
    end_samples: usize,
    last_even: bool,
}

impl Robot36 {
    pub fn new(sample_rate: u32) -> Self {
        let rate = sample_rate as f64;
        let sync_pulse = 0.009;
        let sync_porch = 0.003;
        let luminance = 0.088;
        let separator = 0.0045;
        let porch = 0.0015;
        let chrominance = 0.044;
        let scan_line = sync_pulse + sync_porch + luminance + separator + porch + chrominance;
        let luminance_begin_seconds = sync_porch;
        let separator_begin_seconds = luminance_begin_seconds + luminance;
        let separator_end_seconds = separator_begin_seconds + separator;
        let chrominance_begin_seconds = separator_end_seconds + porch;
        let chrominance_end_seconds = chrominance_begin_seconds + chrominance;
        Self {
            lp: ExponentialMovingAverage::new(),
            horizontal_pixels: 320,
            scan_line_samples: round_samples(scan_line, rate),
            luminance_samples: round_samples(luminance, rate),
            separator_samples: round_samples(separator, rate),
            begin_samples: round_samples(luminance_begin_seconds, rate),
            luminance_begin: round_samples(luminance_begin_seconds, rate),
            separator_begin: round_samples(separator_begin_seconds, rate),
            chrominance_begin: round_samples(chrominance_begin_seconds, rate),
            chrominance_samples: round_samples(chrominance, rate),
            end_samples: round_samples(chrominance_end_seconds, rate),
            last_even: false,
        }
    }
}

impl Mode for Robot36 {
    fn name(&self) -> &str {
        "Robot 36 Color"
    }
    fn vis_code(&self) -> i32 {
        8
    }
    fn width(&self) -> i64 {
        self.horizontal_pixels as i64
    }
    fn height(&self) -> i64 {
        240
    }
    fn first_pixel_sample_index(&self) -> i64 {
        self.begin_samples as i64
    }
    fn first_sync_pulse_index(&self) -> i64 {
        0
    }
    fn scan_line_samples(&self) -> i64 {
        self.scan_line_samples as i64
    }
    fn reset_state(&mut self) {
        self.last_even = false;
    }

    fn decode_scan_line(
        &mut self,
        pixel_buffer: &mut PixelBuffer,
        scratch: &mut [f32],
        scan_line: &[f32],
        _scope_width: usize,
        sync_pulse_index: i64,
        _scan_line_samples: i64,
        frequency_offset: f32,
    ) -> bool {
        let begin = self.begin_samples as i64;
        let end = self.end_samples as i64;
        if sync_pulse_index + begin < 0 || sync_pulse_index + end > scan_line.len() as i64 {
            return false;
        }
        let base = sync_pulse_index;

        let mut separator = 0.0f32;
        for i in 0..self.separator_samples {
            separator += scan_line[(base + self.separator_begin as i64 + i as i64) as usize];
        }
        separator /= self.separator_samples as f32;
        separator -= frequency_offset;
        let mut even = separator < 0.0;
        if separator < -1.1
            || (separator > -0.9 && separator < 0.9)
            || separator > 1.1
        {
            even = !self.last_even;
        }
        self.last_even = even;

        self.lp
            .cutoff_order(self.horizontal_pixels as f64, 2.0 * self.luminance_samples as f64, 2);
        self.lp.reset();
        for i in self.begin_samples..self.end_samples {
            scratch[i] = self.lp.avg(scan_line[(base + i as i64) as usize]);
        }
        self.lp.reset();
        for i in (self.begin_samples..self.end_samples).rev() {
            scratch[i] = freq_to_level(self.lp.avg(scratch[i]), frequency_offset);
        }

        let hp = self.horizontal_pixels;
        for i in 0..hp {
            let luminance_pos = self.luminance_begin + (i * self.luminance_samples) / hp;
            let chrominance_pos = self.chrominance_begin + (i * self.chrominance_samples) / hp;
            if even {
                pixel_buffer.pixels[i] = color::rgb(scratch[luminance_pos], 0.0, scratch[chrominance_pos]);
            } else {
                let even_yuv = pixel_buffer.pixels[i];
                let odd_yuv = color::rgb(scratch[luminance_pos], scratch[chrominance_pos], 0.0);
                pixel_buffer.pixels[i] =
                    color::yuv_to_rgb_packed((even_yuv & 0x00ff_00ff) | (odd_yuv & 0x0000_ff00));
                pixel_buffer.pixels[i + hp] =
                    color::yuv_to_rgb_packed((odd_yuv & 0x00ff_ff00) | (even_yuv & 0x0000_00ff));
            }
        }
        pixel_buffer.width = hp;
        pixel_buffer.height = 2;
        !even
    }
}

// ---------------------------------------------------------------------------
// PaulDon (PD modes)
// ---------------------------------------------------------------------------

pub struct PaulDon {
    lp: ExponentialMovingAverage,
    name: String,
    code: i32,
    horizontal_pixels: usize,
    vertical_pixels: usize,
    scan_line_samples: usize,
    channel_samples: usize,
    begin_samples: usize,
    y_even_begin: usize,
    v_avg_begin: usize,
    u_avg_begin: usize,
    y_odd_begin: usize,
    end_samples: usize,
}

impl PaulDon {
    pub fn new(
        name: &str,
        code: i32,
        horizontal_pixels: usize,
        vertical_pixels: usize,
        channel_seconds: f64,
        sample_rate: u32,
    ) -> Self {
        let rate = sample_rate as f64;
        let sync_pulse = 0.02;
        let sync_porch = 0.00208;
        let scan_line = sync_pulse + sync_porch + 4.0 * channel_seconds;
        let y_even_begin_seconds = sync_porch;
        let v_avg_begin_seconds = y_even_begin_seconds + channel_seconds;
        let u_avg_begin_seconds = v_avg_begin_seconds + channel_seconds;
        let y_odd_begin_seconds = u_avg_begin_seconds + channel_seconds;
        let y_odd_end_seconds = y_odd_begin_seconds + channel_seconds;
        Self {
            lp: ExponentialMovingAverage::new(),
            name: format!("PD {name}"),
            code,
            horizontal_pixels,
            vertical_pixels,
            scan_line_samples: round_samples(scan_line, rate),
            channel_samples: round_samples(channel_seconds, rate),
            begin_samples: round_samples(y_even_begin_seconds, rate),
            y_even_begin: round_samples(y_even_begin_seconds, rate),
            v_avg_begin: round_samples(v_avg_begin_seconds, rate),
            u_avg_begin: round_samples(u_avg_begin_seconds, rate),
            y_odd_begin: round_samples(y_odd_begin_seconds, rate),
            end_samples: round_samples(y_odd_end_seconds, rate),
        }
    }
}

impl Mode for PaulDon {
    fn name(&self) -> &str {
        &self.name
    }
    fn vis_code(&self) -> i32 {
        self.code
    }
    fn width(&self) -> i64 {
        self.horizontal_pixels as i64
    }
    fn height(&self) -> i64 {
        self.vertical_pixels as i64
    }
    fn first_pixel_sample_index(&self) -> i64 {
        self.begin_samples as i64
    }
    fn first_sync_pulse_index(&self) -> i64 {
        0
    }
    fn scan_line_samples(&self) -> i64 {
        self.scan_line_samples as i64
    }
    fn reset_state(&mut self) {}

    fn decode_scan_line(
        &mut self,
        pixel_buffer: &mut PixelBuffer,
        scratch: &mut [f32],
        scan_line: &[f32],
        _scope_width: usize,
        sync_pulse_index: i64,
        _scan_line_samples: i64,
        frequency_offset: f32,
    ) -> bool {
        let begin = self.begin_samples as i64;
        let end = self.end_samples as i64;
        if sync_pulse_index + begin < 0 || sync_pulse_index + end > scan_line.len() as i64 {
            return false;
        }
        let base = sync_pulse_index;

        self.lp
            .cutoff_order(self.horizontal_pixels as f64, 2.0 * self.channel_samples as f64, 2);
        self.lp.reset();
        for i in self.begin_samples..self.end_samples {
            scratch[i] = self.lp.avg(scan_line[(base + i as i64) as usize]);
        }
        self.lp.reset();
        for i in (self.begin_samples..self.end_samples).rev() {
            scratch[i] = freq_to_level(self.lp.avg(scratch[i]), frequency_offset);
        }

        let hp = self.horizontal_pixels;
        for i in 0..hp {
            let position = (i * self.channel_samples) / hp;
            let y_even_pos = position + self.y_even_begin;
            let v_avg_pos = position + self.v_avg_begin;
            let u_avg_pos = position + self.u_avg_begin;
            let y_odd_pos = position + self.y_odd_begin;
            pixel_buffer.pixels[i] =
                color::yuv_to_rgb(scratch[y_even_pos], scratch[u_avg_pos], scratch[v_avg_pos]);
            pixel_buffer.pixels[i + hp] =
                color::yuv_to_rgb(scratch[y_odd_pos], scratch[u_avg_pos], scratch[v_avg_pos]);
        }
        pixel_buffer.width = hp;
        pixel_buffer.height = 2;
        true
    }
}

// ---------------------------------------------------------------------------
// Raw fallback decoder (grayscale, no VIS, used when no mode is locked)
// ---------------------------------------------------------------------------

pub struct RawDecoder {
    lp: ExponentialMovingAverage,
    small_picture_max: usize,
    medium_picture_max: usize,
    name: String,
}

impl RawDecoder {
    pub fn new(name: &str, sample_rate: u32) -> Self {
        let rate = sample_rate as f64;
        Self {
            lp: ExponentialMovingAverage::new(),
            small_picture_max: round_samples(0.125, rate),
            medium_picture_max: round_samples(0.175, rate),
            name: name.to_string(),
        }
    }
}

impl Mode for RawDecoder {
    fn name(&self) -> &str {
        &self.name
    }
    fn vis_code(&self) -> i32 {
        -1
    }
    fn width(&self) -> i64 {
        -1
    }
    fn height(&self) -> i64 {
        -1
    }
    fn first_pixel_sample_index(&self) -> i64 {
        0
    }
    fn first_sync_pulse_index(&self) -> i64 {
        -1
    }
    fn scan_line_samples(&self) -> i64 {
        -1
    }
    fn reset_state(&mut self) {}

    fn decode_scan_line(
        &mut self,
        pixel_buffer: &mut PixelBuffer,
        scratch: &mut [f32],
        scan_line: &[f32],
        scope_width: usize,
        sync_pulse_index: i64,
        scan_line_samples: i64,
        frequency_offset: f32,
    ) -> bool {
        if sync_pulse_index < 0 || sync_pulse_index + scan_line_samples > scan_line.len() as i64 {
            return false;
        }
        let sls = scan_line_samples as usize;
        let mut horizontal_pixels = scope_width;
        if sls < self.small_picture_max {
            horizontal_pixels /= 2;
        }
        if sls < self.medium_picture_max {
            horizontal_pixels /= 2;
        }
        self.lp
            .cutoff_order(horizontal_pixels as f64, 2.0 * sls as f64, 2);
        self.lp.reset();
        let base = sync_pulse_index;
        for i in 0..sls {
            scratch[i] = self.lp.avg(scan_line[(base + i as i64) as usize]);
        }
        self.lp.reset();
        for i in (0..sls).rev() {
            scratch[i] = freq_to_level(self.lp.avg(scratch[i]), frequency_offset);
        }
        for i in 0..horizontal_pixels {
            let position = (i * sls) / horizontal_pixels;
            pixel_buffer.pixels[i] = color::gray(scratch[position]);
        }
        pixel_buffer.width = horizontal_pixels;
        pixel_buffer.height = 1;
        true
    }
}
