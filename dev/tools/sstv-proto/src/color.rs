//! Color conversion (`ColorConverter.cs`) and the pixel buffer (`PixelBuffer.cs`).
//!
//! Pixels are packed ARGB stored as `u32` (C# uses `int`; the bit masks are
//! identical and the YUV→RGB integer math is done in `i32` to preserve the
//! signed arithmetic right-shifts).

pub const OPAQUE: u32 = 0xff00_0000;

fn clamp_i(v: i32) -> i32 {
    v.clamp(0, 255)
}

fn clamp_f(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

fn float_to_int(level: f32) -> i32 {
    clamp_i((255.0 * level).round() as i32)
}

fn compress(level: f32) -> i32 {
    float_to_int(clamp_f(level).sqrt())
}

fn yuv_to_rgb_int(mut y: i32, mut u: i32, mut v: i32) -> u32 {
    y -= 16;
    u -= 128;
    v -= 128;
    let r = clamp_i((298 * y + 409 * v + 128) >> 8);
    let g = clamp_i((298 * y - 100 * u - 208 * v + 128) >> 8);
    let b = clamp_i((298 * y + 516 * u + 128) >> 8);
    OPAQUE | (r as u32) << 16 | (g as u32) << 8 | b as u32
}

pub fn gray(level: f32) -> u32 {
    OPAQUE | 0x0001_0101u32.wrapping_mul(compress(level) as u32)
}

pub fn rgb(red: f32, green: f32, blue: f32) -> u32 {
    OPAQUE
        | (float_to_int(red) as u32) << 16
        | (float_to_int(green) as u32) << 8
        | float_to_int(blue) as u32
}

pub fn yuv_to_rgb(y: f32, u: f32, v: f32) -> u32 {
    yuv_to_rgb_int(float_to_int(y), float_to_int(u), float_to_int(v))
}

/// Decode a packed YUV value (Y in bits 16-23, U in 8-15, V in 0-7) to RGB.
pub fn yuv_to_rgb_packed(yuv: u32) -> u32 {
    yuv_to_rgb_int(
        ((yuv & 0x00ff_0000) >> 16) as i32,
        ((yuv & 0x0000_ff00) >> 8) as i32,
        (yuv & 0x0000_00ff) as i32,
    )
}

/// `PixelBuffer.cs`: a flat ARGB raster with a write cursor (`line`).
pub struct PixelBuffer {
    pub pixels: Vec<u32>,
    pub width: usize,
    pub height: usize,
    /// Signed cursor: -1 means "no image being decoded yet".
    pub line: i64,
}

impl PixelBuffer {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            pixels: vec![0; width * height],
            width,
            height,
            line: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_round_trips_primaries() {
        assert_eq!(rgb(1.0, 0.0, 0.0), OPAQUE | 0x00ff_0000);
        assert_eq!(rgb(0.0, 1.0, 0.0), OPAQUE | 0x0000_ff00);
        assert_eq!(rgb(0.0, 0.0, 1.0), OPAQUE | 0x0000_00ff);
    }

    #[test]
    fn gray_endpoints() {
        assert_eq!(gray(0.0), OPAQUE);
        assert_eq!(gray(1.0), OPAQUE | 0x00ff_ffff);
    }
}
