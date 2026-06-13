//! SSTV encoder ported from `Encoder.cs`. Generates FM audio samples
//! (`f32` in -1..1) from a packed-ARGB image. Robot 36 and the PD family
//! are ported; the bd-st5n scope is Robot 36 + one PD mode.

use std::f64::consts::PI;

const SYNC_PULSE_FREQUENCY: f64 = 1200.0;
const SYNC_PORCH_FREQUENCY: f64 = 1500.0;
const BLACK_FREQUENCY: f64 = 1500.0;
const WHITE_FREQUENCY: f64 = 2300.0;
const LEADER_TONE_FREQUENCY: f64 = 1900.0;
const VIS_BIT_ONE_FREQUENCY: f64 = 1100.0;
const VIS_BIT_ZERO_FREQUENCY: f64 = 1300.0;

/// PD sub-mode parameters: (vis_code, horizontal, vertical, channel_seconds).
#[derive(Clone, Copy, Debug)]
pub struct PdMode {
    pub vis_code: i32,
    pub width: usize,
    pub height: usize,
    pub channel_seconds: f64,
}

impl PdMode {
    /// PD 120 — 640x496, the highest-resolution PD mode that still fits a
    /// reasonable airtime; a good interop default alongside Robot 36.
    pub const PD120: PdMode = PdMode {
        vis_code: 95,
        width: 640,
        height: 496,
        channel_seconds: 0.1216,
    };

    /// PD 90 — 320x256, compact color mode.
    pub const PD90: PdMode = PdMode {
        vis_code: 99,
        width: 320,
        height: 256,
        channel_seconds: 0.17024,
    };
}

pub struct Encoder {
    sample_rate: f64,
    phase: f64,
}

impl Encoder {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate: sample_rate as f64,
            phase: 0.0,
        }
    }

    fn add_sample(&mut self, samples: &mut Vec<f32>, frequency: f64) {
        let delta = 2.0 * PI * frequency / self.sample_rate;
        samples.push(self.phase.sin() as f32);
        self.phase += delta;
        if self.phase > 2.0 * PI {
            self.phase -= 2.0 * PI;
        }
    }

    fn add_tone(&mut self, samples: &mut Vec<f32>, frequency: f64, duration_seconds: f64) {
        let count = (duration_seconds * self.sample_rate).round() as i64;
        for _ in 0..count {
            self.add_sample(samples, frequency);
        }
    }

    fn level_to_frequency(level: f32) -> f64 {
        let level = level.clamp(0.0, 1.0) as f64;
        BLACK_FREQUENCY + level * (WHITE_FREQUENCY - BLACK_FREQUENCY)
    }

    fn add_pixel_line(&mut self, samples: &mut Vec<f32>, levels: &[f32], duration_seconds: f64) {
        let count = (duration_seconds * self.sample_rate).round() as i64;
        for i in 0..count {
            let mut pixel_index = (i as usize * levels.len()) / count as usize;
            pixel_index = pixel_index.min(levels.len() - 1);
            let freq = Self::level_to_frequency(levels[pixel_index]);
            self.add_sample(samples, freq);
        }
    }

    fn add_vis_header(&mut self, samples: &mut Vec<f32>, vis_code: i32) {
        self.add_tone(samples, LEADER_TONE_FREQUENCY, 0.3);
        self.add_tone(samples, SYNC_PULSE_FREQUENCY, 0.01);
        self.add_tone(samples, LEADER_TONE_FREQUENCY, 0.3);
        self.add_tone(samples, SYNC_PULSE_FREQUENCY, 0.03);

        let mut parity = 0;
        for i in 0..7 {
            parity ^= (vis_code >> i) & 1;
        }
        let vis_code_with_parity = (vis_code & 0x7F) | (parity << 7);

        for i in 0..8 {
            let bit = (vis_code_with_parity & (1 << i)) != 0;
            self.add_tone(
                samples,
                if bit {
                    VIS_BIT_ONE_FREQUENCY
                } else {
                    VIS_BIT_ZERO_FREQUENCY
                },
                0.03,
            );
        }
        self.add_tone(samples, SYNC_PULSE_FREQUENCY, 0.03);
    }

    pub fn reset(&mut self) {
        self.phase = 0.0;
    }

    /// Encode Robot 36 Color (VIS 8, 320x240, ~36s).
    pub fn encode_robot36(&mut self, pixels: &[u32], width: usize, height: usize) -> Vec<f32> {
        let sync_pulse = 0.009;
        let sync_porch = 0.003;
        let luminance = 0.088;
        let separator = 0.0045;
        let porch = 0.0015;
        let chrominance = 0.044;
        let hp = 320usize;
        let vp = 240usize;

        let mut samples = Vec::new();
        self.add_vis_header(&mut samples, 8);

        let mut line = 0;
        while line < vp {
            let mut y_even = vec![0.0f32; hp];
            let mut y_odd = vec![0.0f32; hp];
            let mut u_avg = vec![0.0f32; hp];
            let mut v_avg = vec![0.0f32; hp];

            for x in 0..hp {
                let src_x = (x * width) / hp;
                let src_y_even = (line * height) / vp;
                let src_y_odd = (((line + 1) * height) / vp).min(height - 1);

                let (re, ge, be) = unpack_rgb(pixels[src_y_even * width + src_x]);
                let (ye, ue, ve) = rgb_to_yuv(re, ge, be);
                let (ro, go, bo) = unpack_rgb(pixels[src_y_odd * width + src_x]);
                let (yo, uo, vo) = rgb_to_yuv(ro, go, bo);

                y_even[x] = ye;
                y_odd[x] = yo;
                u_avg[x] = (ue + uo) / 2.0;
                v_avg[x] = (ve + vo) / 2.0;
            }

            // Even line: sync + porch + Y + separator(1500) + porch + V
            self.add_tone(&mut samples, SYNC_PULSE_FREQUENCY, sync_pulse);
            self.add_tone(&mut samples, SYNC_PORCH_FREQUENCY, sync_porch);
            self.add_pixel_line(&mut samples, &y_even, luminance);
            self.add_tone(&mut samples, SYNC_PORCH_FREQUENCY, separator);
            self.add_tone(&mut samples, SYNC_PORCH_FREQUENCY, porch);
            self.add_pixel_line(&mut samples, &v_avg, chrominance);

            // Odd line: sync + porch + Y + separator(2300) + porch + U
            self.add_tone(&mut samples, SYNC_PULSE_FREQUENCY, sync_pulse);
            self.add_tone(&mut samples, SYNC_PORCH_FREQUENCY, sync_porch);
            self.add_pixel_line(&mut samples, &y_odd, luminance);
            self.add_tone(&mut samples, WHITE_FREQUENCY, separator);
            self.add_tone(&mut samples, SYNC_PORCH_FREQUENCY, porch);
            self.add_pixel_line(&mut samples, &u_avg, chrominance);

            line += 2;
        }
        samples
    }

    /// Encode a PD (PaulDon) mode.
    pub fn encode_paul_don(
        &mut self,
        pixels: &[u32],
        width: usize,
        height: usize,
        mode: PdMode,
    ) -> Vec<f32> {
        let sync_pulse = 0.02;
        let sync_porch = 0.00208;
        let hp = mode.width;
        let vp = mode.height;
        let ch = mode.channel_seconds;

        let mut samples = Vec::new();
        self.add_vis_header(&mut samples, mode.vis_code);

        let mut line = 0;
        while line < vp {
            let mut y_even = vec![0.0f32; hp];
            let mut y_odd = vec![0.0f32; hp];
            let mut u_avg = vec![0.0f32; hp];
            let mut v_avg = vec![0.0f32; hp];

            for x in 0..hp {
                let src_x = (x * width) / hp;
                let src_y_even = (line * height) / vp;
                let src_y_odd = (((line + 1) * height) / vp).min(height - 1);

                let (re, ge, be) = unpack_rgb(pixels[src_y_even * width + src_x]);
                let (ye, ue, ve) = rgb_to_yuv(re, ge, be);
                let (ro, go, bo) = unpack_rgb(pixels[src_y_odd * width + src_x]);
                let (yo, uo, vo) = rgb_to_yuv(ro, go, bo);

                y_even[x] = ye;
                y_odd[x] = yo;
                u_avg[x] = (ue + uo) / 2.0;
                v_avg[x] = (ve + vo) / 2.0;
            }

            self.add_tone(&mut samples, SYNC_PULSE_FREQUENCY, sync_pulse);
            self.add_tone(&mut samples, SYNC_PORCH_FREQUENCY, sync_porch);
            self.add_pixel_line(&mut samples, &y_even, ch);
            self.add_pixel_line(&mut samples, &v_avg, ch);
            self.add_pixel_line(&mut samples, &u_avg, ch);
            self.add_pixel_line(&mut samples, &y_odd, ch);

            line += 2;
        }
        samples
    }
}

fn unpack_rgb(argb: u32) -> (f32, f32, f32) {
    let r = ((argb >> 16) & 0xFF) as f32 / 255.0;
    let g = ((argb >> 8) & 0xFF) as f32 / 255.0;
    let b = (argb & 0xFF) as f32 / 255.0;
    (r, g, b)
}

fn rgb_to_yuv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let y = 0.299 * r + 0.587 * g + 0.114 * b;
    let u = -0.169 * r - 0.331 * g + 0.500 * b + 0.5;
    let v = 0.500 * r - 0.419 * g - 0.081 * b + 0.5;
    (y, u, v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_image(w: usize, h: usize, argb: u32) -> Vec<u32> {
        vec![argb; w * h]
    }

    #[test]
    fn robot36_duration_is_about_36s() {
        let mut enc = Encoder::new(32000);
        let img = flat_image(320, 240, 0xff_80_80_80);
        let s = enc.encode_robot36(&img, 320, 240);
        let secs = s.len() as f64 / 32000.0;
        // VIS header (~0.91s) + 120 line-pairs * 0.30s ≈ 36.9s.
        assert!(
            (secs - 36.9).abs() < 0.2,
            "robot36 duration {secs}s out of expected range"
        );
    }

    #[test]
    fn samples_stay_in_range() {
        let mut enc = Encoder::new(32000);
        let img = flat_image(320, 240, 0xff_12_34_56);
        let s = enc.encode_robot36(&img, 320, 240);
        assert!(s.iter().all(|&v| (-1.0..=1.0).contains(&v)));
    }

    #[test]
    fn pd120_is_larger_and_longer() {
        let mut enc = Encoder::new(32000);
        let img = flat_image(640, 496, 0xff_20_40_60);
        let s = enc.encode_paul_don(&img, 640, 496, PdMode::PD120);
        let secs = s.len() as f64 / 32000.0;
        // PD120: 248 line-pairs * (0.02+0.00208 + 4*0.1216) ≈ 126s + header.
        assert!(secs > 100.0, "pd120 duration {secs}s unexpectedly short");
    }
}
