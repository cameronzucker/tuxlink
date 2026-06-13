mod proto; mod encoder;
use mini_sbc::{frame_decoder::FrameDecoder, filter_state::FilterState, header::SBCHeader};

fn decode_skip_crc(sbc: &[u8]) -> Vec<i16> {
    let mut data: &[u8] = sbc;
    let mut filter = FilterState::<1, 8>::new();
    let mut pcm = Vec::new();
    while !data.is_empty() {
        let h = match SBCHeader::decode(&mut data) { Ok(h) => h, Err(_) => break };
        let frame = match FrameDecoder::new_skip_crc(&h, &mut filter, &mut data) { Ok(f) => f, Err(_) => break };
        for b in frame { for ch in b { for s in ch { pcm.push(s); } } }
    }
    pcm
}

fn main() {
    let pcm: Vec<i16> = include_bytes!("in.pcm").chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
    let scale = 4.0;
    let mut x = [0.0f64; 80];
    let mut sbc = Vec::new();
    for frame in pcm.chunks_exact(128) { sbc.extend(encoder::encode_frame(&mut x, frame, scale)); }
    let out = decode_skip_crc(&sbc);
    let d = 137usize;
    let n = pcm.len() - d;
    let ss_mae: f64 = (0..n).map(|i| (pcm[i] as f64 - out[i+d] as f64).abs()).sum::<f64>() / n as f64;
    let ss_peakerr = (0..n).map(|i| (pcm[i] as f64 - out[i+d] as f64).abs() as i64).max().unwrap_or(0);
    let peak = out.iter().map(|s| (*s as i32).abs()).max().unwrap_or(0);
    println!("scale {scale}: steady-state MAE {ss_mae:.1}, peak-err {ss_peakerr}, out-peak {peak} (in-peak 16383), sbc {} bytes", sbc.len());
}
