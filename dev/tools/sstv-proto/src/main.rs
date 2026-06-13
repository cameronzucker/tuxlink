//! Fast-iteration harness for the SSTV codec (bd tuxlink-st5n).
//!
//! Usage:
//!   sstv-proto encode <in.png> <out.pcm> [robot36|pd120|pd90]
//!   sstv-proto decode <in.pcm> <out.png>
//!   sstv-proto roundtrip <in.png> <out.png> [robot36|pd120|pd90]
//!
//! PCM files are raw 32 kHz mono s16le (the wire format SSTV produces/consumes;
//! the SBC codec sits between this PCM and the radio link).

use std::process::ExitCode;

use image::RgbImage;
use sstv_proto::encoder::{Encoder, PdMode};
use sstv_proto::{decode_full, f32_to_pcm16, pcm16_to_f32, DEFAULT_SAMPLE_RATE};

fn load_argb(path: &str) -> (Vec<u32>, usize, usize) {
    let img = image::open(path).expect("open image").to_rgb8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    let px = img
        .pixels()
        .map(|p| 0xff00_0000 | (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32)
        .collect();
    (px, w, h)
}

fn save_argb(px: &[u32], w: usize, h: usize, path: &str) {
    let mut img = RgbImage::new(w as u32, h as u32);
    for (i, p) in px.iter().enumerate() {
        let (x, y) = ((i % w) as u32, (i / w) as u32);
        img.put_pixel(
            x,
            y,
            image::Rgb([(p >> 16) as u8, (p >> 8) as u8, *p as u8]),
        );
    }
    img.save(path).expect("save image");
}

fn encode(px: &[u32], w: usize, h: usize, mode: &str) -> Vec<f32> {
    let mut enc = Encoder::new(DEFAULT_SAMPLE_RATE);
    match mode {
        "robot36" => enc.encode_robot36(px, w, h),
        "pd120" => enc.encode_paul_don(px, w, h, PdMode::PD120),
        "pd90" => enc.encode_paul_don(px, w, h, PdMode::PD90),
        other => panic!("unknown mode {other}"),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: sstv-proto <encode|decode|roundtrip> ...");
        return ExitCode::FAILURE;
    }
    match args[1].as_str() {
        "encode" => {
            let mode = args.get(4).map(|s| s.as_str()).unwrap_or("robot36");
            let (px, w, h) = load_argb(&args[2]);
            let samples = encode(&px, w, h, mode);
            std::fs::write(&args[3], f32_to_pcm16(&samples)).expect("write pcm");
            eprintln!("encoded {mode}: {} samples", samples.len());
        }
        "decode" => {
            let bytes = std::fs::read(&args[2]).expect("read pcm");
            let samples = pcm16_to_f32(&bytes);
            match decode_full(&samples, DEFAULT_SAMPLE_RATE, 1024) {
                Some((px, w, h, mode)) => {
                    save_argb(&px, w, h, &args[3]);
                    eprintln!("decoded {mode}: {w}x{h}");
                }
                None => {
                    eprintln!("no image decoded");
                    return ExitCode::FAILURE;
                }
            }
        }
        "roundtrip" => {
            let mode = args.get(4).map(|s| s.as_str()).unwrap_or("robot36");
            let (px, w, h) = load_argb(&args[2]);
            let samples = encode(&px, w, h, mode);
            match decode_full(&samples, DEFAULT_SAMPLE_RATE, 1024) {
                Some((dpx, dw, dh, dmode)) => {
                    save_argb(&dpx, dw, dh, &args[3]);
                    eprintln!("roundtrip {mode}->{dmode}: {dw}x{dh}");
                }
                None => {
                    eprintln!("roundtrip: no image decoded");
                    return ExitCode::FAILURE;
                }
            }
        }
        other => {
            eprintln!("unknown command {other}");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}
