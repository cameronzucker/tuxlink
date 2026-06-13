//! Spec-level behavior: an image survives encode -> PCM -> decode.
//! Robot 36 and PD use chroma subsampling + low-pass filtering, so the test
//! compares interior patches of flat color blocks within a per-channel
//! mean-absolute-error tolerance rather than exact pixels.

use crate::encoder::{Encoder, PdMode};
use crate::sbc_vendored::{SbcCodec, UvproSbcCodec};
use crate::{decode_full, f32_to_pcm16, pcm16_to_f32, DEFAULT_SAMPLE_RATE};

/// Four flat quadrants: red, green, blue, mid-gray.
fn quadrant_image(w: usize, h: usize) -> Vec<u32> {
    let mut px = vec![0u32; w * h];
    for y in 0..h {
        for x in 0..w {
            let left = x < w / 2;
            let top = y < h / 2;
            let argb = match (top, left) {
                (true, true) => 0xff_ff_00_00,   // red
                (true, false) => 0xff_00_ff_00,  // green
                (false, true) => 0xff_00_00_ff,  // blue
                (false, false) => 0xff_80_80_80, // gray
            };
            px[y * w + x] = argb;
        }
    }
    px
}

fn unpack(argb: u32) -> (i32, i32, i32) {
    (
        ((argb >> 16) & 0xff) as i32,
        ((argb >> 8) & 0xff) as i32,
        (argb & 0xff) as i32,
    )
}

/// Mean per-channel absolute error over an interior patch of a quadrant.
fn patch_mae(px: &[u32], w: usize, h: usize, top: bool, left: bool, expect: u32) -> f64 {
    let x0 = if left { w / 4 } else { 3 * w / 4 };
    let y0 = if top { h / 4 } else { 3 * h / 4 };
    let half = (w.min(h) / 16).max(2);
    let (er, eg, eb) = unpack(expect);
    let mut sum = 0.0;
    let mut n = 0.0;
    for y in y0.saturating_sub(half)..(y0 + half).min(h) {
        for x in x0.saturating_sub(half)..(x0 + half).min(w) {
            let (r, g, b) = unpack(px[y * w + x]);
            sum += ((r - er).abs() + (g - eg).abs() + (b - eb).abs()) as f64 / 3.0;
            n += 1.0;
        }
    }
    sum / n
}

#[test]
fn robot36_round_trip_recovers_quadrants() {
    let (w, h) = (320, 240);
    let src = quadrant_image(w, h);
    let mut enc = Encoder::new(DEFAULT_SAMPLE_RATE);
    let samples = enc.encode_robot36(&src, w, h);

    let (px, dw, dh, mode) =
        decode_full(&samples, DEFAULT_SAMPLE_RATE, 1024).expect("decode produced an image");
    assert_eq!(mode, "Robot 36 Color");
    assert_eq!((dw, dh), (320, 240));

    for (top, left, color) in [
        (true, true, 0xff_ff_00_00u32),
        (true, false, 0xff_00_ff_00),
        (false, true, 0xff_00_00_ff),
        (false, false, 0xff_80_80_80),
    ] {
        let mae = patch_mae(&px, dw, dh, top, left, color);
        assert!(mae < 45.0, "quadrant top={top} left={left} MAE {mae:.1} too high");
    }
}

#[test]
fn pd120_round_trip_recovers_quadrants() {
    let (w, h) = (640, 496);
    let src = quadrant_image(w, h);
    let mut enc = Encoder::new(DEFAULT_SAMPLE_RATE);
    let samples = enc.encode_paul_don(&src, w, h, PdMode::PD120);

    let (px, dw, dh, mode) =
        decode_full(&samples, DEFAULT_SAMPLE_RATE, 1024).expect("decode produced an image");
    assert_eq!(mode, "PD 120");
    assert_eq!((dw, dh), (640, 496));

    for (top, left, color) in [
        (true, true, 0xff_ff_00_00u32),
        (true, false, 0xff_00_ff_00),
        (false, true, 0xff_00_00_ff),
        (false, false, 0xff_80_80_80),
    ] {
        let mae = patch_mae(&px, dw, dh, top, left, color);
        assert!(mae < 45.0, "PD120 quadrant top={top} left={left} MAE {mae:.1} too high");
    }
}

// ---------------------------------------------------------------------------
// DECISIVE GATE (tuxlink-st5n): full image round-trip through the ACTUAL shipped
// UvproSbcCodec. If the image survives, the encoder's ~MAE-156 amplitude error
// does NOT need refinement for SSTV; if it fails, refinement is required.
// ---------------------------------------------------------------------------

/// image -> SSTV-encode -> s16 PCM -> SBC encode -> SBC decode -> s16 PCM ->
/// SSTV-decode -> image. Returns per-quadrant MAE.
fn sbc_gate_roundtrip(
    w: usize,
    h: usize,
    encode_sstv: impl Fn(&mut Encoder, &[u32]) -> Vec<f32>,
    expect_mode: &str,
) -> [f64; 4] {
    let src = quadrant_image(w, h);
    let mut enc = Encoder::new(DEFAULT_SAMPLE_RATE);
    let sstv = encode_sstv(&mut enc, &src);

    let codec = UvproSbcCodec::new();
    let sbc = codec.encode(&f32_to_pcm16(&sstv));
    let recovered_pcm = codec.decode(&sbc);
    let recovered = pcm16_to_f32(&recovered_pcm);
    eprintln!(
        "SBC gate {expect_mode}: sstv {} samples -> sbc {} bytes -> recovered {} samples",
        sstv.len(),
        sbc.len(),
        recovered.len()
    );

    let (px, dw, dh, mode) =
        decode_full(&recovered, DEFAULT_SAMPLE_RATE, 1024).expect("decode after SBC round-trip");
    assert_eq!(mode, expect_mode);
    assert_eq!((dw, dh), (w, h));
    [
        patch_mae(&px, dw, dh, true, true, 0xff_ff_00_00),
        patch_mae(&px, dw, dh, true, false, 0xff_00_ff_00),
        patch_mae(&px, dw, dh, false, true, 0xff_00_00_ff),
        patch_mae(&px, dw, dh, false, false, 0xff_80_80_80),
    ]
}

#[test]
fn robot36_survives_sbc_round_trip() {
    let mae = sbc_gate_roundtrip(320, 240, |e, s| e.encode_robot36(s, 320, 240), "Robot 36 Color");
    eprintln!("Robot36 post-SBC quadrant MAE: {mae:?}");
    for (i, m) in mae.iter().enumerate() {
        assert!(*m < 60.0, "Robot36 quadrant {i} post-SBC MAE {m:.1} too high");
    }
}

#[test]
fn pd120_survives_sbc_round_trip() {
    let mae = sbc_gate_roundtrip(640, 496, |e, s| e.encode_paul_don(s, 640, 496, PdMode::PD120), "PD 120");
    eprintln!("PD120 post-SBC quadrant MAE: {mae:?}");
    for (i, m) in mae.iter().enumerate() {
        assert!(*m < 60.0, "PD120 quadrant {i} post-SBC MAE {m:.1} too high");
    }
}
