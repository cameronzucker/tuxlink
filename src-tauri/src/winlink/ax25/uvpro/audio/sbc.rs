//! SBC codec for the UV-Pro audio transport (tuxlink-vgvn).
//!
//! The `AudioData` wire payload is SBC (Bluetooth sub-band codec), not raw PCM.
//! DECODE uses `mini_sbc` (pure-Rust, MIT/Apache). ENCODE is a from-scratch pure-Rust
//! port (no pure-Rust SBC encoder exists on crates.io): analysis filterbank + Loudness
//! bit-allocation + quantization (the inverse of `mini_sbc`'s dequant) + bitstream
//! packing + CRC-8. Validated off-air by round-trip through the decoder (the operator's
//! loopback strategy) — see `docs/superpowers/plans/2026-06-13-sbc-codec.md` and the
//! `dev/tools/sbc-proto/` iteration harness.
//!
//! Params are the UV-Pro's fixed format (= benlink, frame header `9c 71 10`):
//! 32 kHz, MONO, 8 subbands, 16 blocks, bitpool 16, Loudness allocation.
//!
//! QUALITY NOTE (2026-06-13): the encoder round-trips faithfully (decodable SBC) but a
//! 1 kHz calibration tone reconstructs at ~MAE 156 / peak-err ~9850 vs ffmpeg's 6.7 —
//! fine-grained quantization/scale refinement remains (the analysis convention itself
//! is confirmed correct). SSTV decode is STFT (frequency-domain), expected to be robust
//! to this amplitude error; the decisive check is a full image round-trip (tuxlink-st5n).
//! Refinement tracked in the plan.

use std::sync::Mutex;

use mini_sbc::filter_state::FilterState;
use mini_sbc::frame_decoder::FrameDecoder;
use mini_sbc::header::SBCHeader;

use super::codec::SbcCodec;

// ---- SBC 8-subband fixed params (the UV-Pro format) ----
const SUBBANDS: usize = 8;
const BLOCKS: usize = 16;
const SAMPLES_PER_FRAME: usize = SUBBANDS * BLOCKS; // 128
const BITPOOL: i32 = 16;
/// Loudness allocation offsets for 8 subbands @ 32 kHz (SBC_OFFSET8[freq=32k]).
const OFFSET8_32K: [i8; 8] = [-3, 0, 0, 0, 0, 0, 1, 2];
/// Analysis→dequant domain factor = 2^SBCDEC_FIXED_EXTRA_BITS (mini_sbc's extra fixed bits).
const SCALE: f64 = 4.0;
/// SBC 8-subband prototype window proto_8_80 ([FILTER_ORDER=10][SUBBANDS=8]).
const PROTO: [[f64; 8]; 10] = [
    [0.00000000e00, 1.56575398e-04, 3.43256425e-04, 5.54620202e-04, 8.23919506e-04, 1.13992507e-03, 1.47640169e-03, 1.78371725e-03],
    [2.01182542e-03, 2.10371989e-03, 1.99454554e-03, 1.61656283e-03, 9.02154502e-04, -1.78805361e-04, -1.64973098e-03, -3.49717454e-03],
    [5.65949473e-03, 8.02941163e-03, 1.04584443e-02, 1.27472335e-02, 1.46525263e-02, 1.59045603e-02, 1.62208471e-02, 1.53184106e-02],
    [1.29371806e-02, 8.85757540e-03, 2.92408442e-03, -4.91578024e-03, -1.46404076e-02, -2.61098752e-02, -3.90751381e-02, -5.31873032e-02],
    [6.79989431e-02, 8.29847578e-02, 9.75753918e-02, 1.11196689e-01, 1.23264548e-01, 1.33264415e-01, 1.40753505e-01, 1.45389847e-01],
    [1.46955068e-01, 1.45389847e-01, 1.40753505e-01, 1.33264415e-01, 1.23264548e-01, 1.11196689e-01, 9.75753918e-02, 8.29847578e-02],
    [-6.79989431e-02, -5.31873032e-02, -3.90751381e-02, -2.61098752e-02, -1.46404076e-02, -4.91578024e-03, 2.92408442e-03, 8.85757540e-03],
    [1.29371806e-02, 1.53184106e-02, 1.62208471e-02, 1.59045603e-02, 1.46525263e-02, 1.27472335e-02, 1.04584443e-02, 8.02941163e-03],
    [-5.65949473e-03, -3.49717454e-03, -1.64973098e-03, -1.78805361e-04, 9.02154502e-04, 1.61656283e-03, 1.99454554e-03, 2.10371989e-03],
    [2.01182542e-03, 1.78371725e-03, 1.47640169e-03, 1.13992507e-03, 8.23919506e-04, 5.54620202e-04, 3.43256425e-04, 1.56575398e-04],
];

fn proto_flat(n: usize) -> f64 {
    PROTO[n / 8][n % 8]
}

// ---------------------------------------------------------------------------
// Decode (mini_sbc)
// ---------------------------------------------------------------------------

/// Decode a stream of UV-Pro SBC frames (mono, 8-subband) to PCM samples, carrying
/// synthesis-filter state in `filter` across calls for streaming. A truncated trailing
/// frame is dropped.
fn decode_into(filter: &mut FilterState<1, 8>, sbc: &[u8]) -> Vec<i16> {
    let mut data: &[u8] = sbc;
    let mut pcm: Vec<i16> = Vec::new();
    while !data.is_empty() {
        let header = match SBCHeader::decode(&mut data) {
            Ok(h) => h,
            Err(_) => break,
        };
        // Tolerate a wrong/absent CRC on RX (we cannot guarantee the peer's CRC under
        // a lossy RF path); structural decode is what matters for SSTV.
        let mut frame = match FrameDecoder::new_skip_crc(&header, filter, &mut data) {
            Ok(f) => f,
            Err(_) => break,
        };
        // Drive the frame via the FALLIBLE inherent `next()` — NOT `for block in frame`,
        // whose `Iterator` impl `.unwrap()`s a mid-frame read error and PANICS. The frame
        // header can validate while the sample body is truncated/garbled (the normal case
        // on a lossy RF link), so a read error mid-frame must stop the stream gracefully,
        // never panic the RX loop. (Adversarial review P0, 2026-06-13.) `Err` covers both
        // `NoBlock` (frame fully decoded) and a truncated-body read error; both stop here.
        loop {
            match frame.next() {
                Ok(block) => {
                    for ch in block {
                        for sample in ch {
                            pcm.push(sample);
                        }
                    }
                }
                Err(_) => break,
            }
        }
    }
    pcm
}

/// Stateless decode of a complete SBC byte stream to PCM (fresh filter state).
pub fn decode_sbc(sbc: &[u8]) -> Vec<i16> {
    let mut filter = FilterState::<1, 8>::new();
    decode_into(&mut filter, sbc)
}

// ---------------------------------------------------------------------------
// Encode (pure-Rust port)
// ---------------------------------------------------------------------------

/// Analysis filterbank: maintain `x` (80-sample history, newest at x[0]); push one
/// block of 8 PCM samples → 8 subband samples (float). Standard SBC 8-subband
/// cosine-modulated analysis (convention confirmed by round-trip vs mini_sbc).
fn analyze(x: &mut [f64; 80], block: &[i16; 8]) -> [f64; 8] {
    x.copy_within(0..72, 8); // shift history back by one block (X[8..80] = X[0..72])
    for (i, slot) in x[..8].iter_mut().enumerate() {
        *slot = block[7 - i] as f64;
    }
    let mut z = [0.0f64; 16];
    for (i, zi) in z.iter_mut().enumerate() {
        let mut s = 0.0;
        for j in 0..5 {
            s += proto_flat(i + 16 * j) * x[i + 16 * j];
        }
        *zi = s;
    }
    let mut out = [0.0f64; 8];
    for (k, ok) in out.iter_mut().enumerate() {
        let mut s = 0.0;
        for (i, &zi) in z.iter().enumerate() {
            s += ((2.0 * k as f64 + 1.0) * (i as f64 - 4.0) * std::f64::consts::PI / 16.0).cos() * zi;
        }
        *ok = s;
    }
    out
}

fn scale_factor(max_abs: u32) -> u8 {
    if max_abs == 0 {
        0
    } else {
        (32 - max_abs.leading_zeros()).min(15) as u8
    }
}

/// SBC Loudness bit-allocation for mono/8-subband (port of `mini_sbc::calculate_bits`).
/// The decoder runs the identical algorithm from the scale factors, so they agree.
fn allocate(sf: &[u8; 8]) -> [u8; 8] {
    let mut bitneed = [0i8; 8];
    for sb in 0..8 {
        let loud = sf[sb] as i8 - OFFSET8_32K[sb];
        bitneed[sb] = if loud > 0 { loud / 2 } else { loud };
    }
    let max_bitneed = *bitneed.iter().max().unwrap();
    let mut bitcount = 0i32;
    let mut slicecount = 0i32;
    let mut bitslice = max_bitneed + 1;
    loop {
        bitslice -= 1;
        bitcount += slicecount;
        slicecount = bitneed
            .iter()
            .map(|&n| {
                if n > bitslice + 1 && n < bitslice + 16 {
                    1
                } else if n == bitslice + 1 {
                    2
                } else {
                    0
                }
            })
            .sum();
        if bitcount + slicecount >= BITPOOL {
            break;
        }
    }
    if bitcount + slicecount < BITPOOL {
        bitslice -= 1;
        bitcount += slicecount;
    }
    let mut bits = [0u8; 8];
    for sb in 0..8 {
        if bitneed[sb] < bitslice + 2 {
            bits[sb] = 0;
        } else {
            bits[sb] = (bitneed[sb] - bitslice).min(16) as u8;
        }
    }
    for sb in 0..8 {
        if bitcount >= BITPOOL {
            break;
        }
        if bits[sb] >= 2 && bits[sb] < 16 {
            bits[sb] += 1;
            bitcount += 1;
        } else if bitneed[sb] == bitslice + 1 && BITPOOL > bitcount + 1 {
            bits[sb] = 2;
            bitcount += 2;
        }
    }
    for b in bits.iter_mut() {
        if bitcount >= BITPOOL {
            break;
        }
        if *b < 16 {
            *b += 1;
            bitcount += 1;
        }
    }
    bits
}

/// Quantize a subband sample to `bits` (inverse of mini_sbc dequant, shift = sf + 3).
fn quantize(sample: i32, sf: u8, bits: u8) -> u32 {
    if bits == 0 {
        return 0;
    }
    let shift = sf as i64 + 3;
    let levels = (1i64 << bits) - 1;
    let one = 1i64 << shift;
    let s = (((sample as i64 + one) * levels) / one - 1) / 2;
    s.clamp(0, levels) as u32
}

const CRC_POLY: u8 = 0x1D;

fn crc8(init: u8, data: &[u8]) -> u8 {
    let mut crc = init;
    for &byte in data {
        crc ^= byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ CRC_POLY
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// Encode exactly one frame (128 PCM samples) to a 40-byte SBC frame with CRC.
fn encode_frame(x: &mut [f64; 80], pcm: &[i16; SAMPLES_PER_FRAME]) -> Vec<u8> {
    let mut sub = [[0i32; 8]; BLOCKS];
    for (b, blk) in pcm.chunks_exact(8).enumerate() {
        let arr: [i16; 8] = std::array::from_fn(|i| blk[i]);
        let s = analyze(x, &arr);
        for sb in 0..8 {
            sub[b][sb] = (s[sb] * SCALE).round() as i32;
        }
    }
    let mut sf = [0u8; 8];
    for sb in 0..8 {
        let m = (0..BLOCKS).map(|b| sub[b][sb].unsigned_abs()).max().unwrap();
        sf[sb] = scale_factor(m);
    }
    let bits = allocate(&sf);

    // Scale-factor bytes (4 nibbles-per-byte packing) — used for both the bitstream
    // and the CRC region.
    let mut sf_bytes = [0u8; 4];
    for sb in 0..8 {
        sf_bytes[sb >> 1] |= (sf[sb] & 0x0F) << (((!sb) & 1) * 4);
    }
    // CRC-8 over [header_byte, bitpool_byte] then the scale-factor bytes (init 0x0F).
    let mut crc = crc8(0x0F, &[0x71, 0x10]);
    crc = crc8(crc, &sf_bytes);

    let mut out = vec![0x9C, 0x71, 0x10, crc];
    let mut bw = BitWriter::new();
    for &v in &sf {
        bw.write(v as u32, 4);
    }
    for block in &sub {
        for sb in 0..8 {
            if bits[sb] > 0 {
                bw.write(quantize(block[sb], sf[sb], bits[sb]), bits[sb] as usize);
            }
        }
    }
    out.extend(bw.finish());
    // Pad to the fixed SBC frame_length (mono/8sb/16blk/bitpool16 = 40 bytes) so the
    // stream stays frame-aligned even when an allocation underuses the bitpool.
    out.resize(40, 0);
    out
}

struct BitWriter {
    buf: Vec<u8>,
    acc: u32,
    nbits: u32,
}
impl BitWriter {
    fn new() -> Self {
        Self { buf: Vec::new(), acc: 0, nbits: 0 }
    }
    fn write(&mut self, v: u32, bits: usize) {
        for i in (0..bits).rev() {
            self.acc = (self.acc << 1) | ((v >> i) & 1);
            self.nbits += 1;
            if self.nbits == 8 {
                self.buf.push(self.acc as u8);
                self.acc = 0;
                self.nbits = 0;
            }
        }
    }
    fn finish(mut self) -> Vec<u8> {
        if self.nbits > 0 {
            self.buf.push((self.acc << (8 - self.nbits)) as u8);
        }
        self.buf
    }
}

// ---------------------------------------------------------------------------
// SbcCodec trait impl (the transport seam)
// ---------------------------------------------------------------------------

struct EncState {
    filter: [f64; 80],
    /// PCM samples not yet forming a complete 128-sample frame (carried across calls).
    residual: Vec<i16>,
}

impl Default for EncState {
    fn default() -> Self {
        // `[f64; 80]` exceeds the array length that derives `Default`, so impl by hand.
        Self { filter: [0.0; 80], residual: Vec::new() }
    }
}

/// The UV-Pro SBC codec: streaming encode (pure-Rust port) + decode (mini_sbc), each
/// carrying filter state across calls so chunked transmit/receive stays continuous.
pub struct UvproSbcCodec {
    enc: Mutex<EncState>,
    dec: Mutex<FilterState<1, 8>>,
}

impl Default for UvproSbcCodec {
    fn default() -> Self {
        Self {
            enc: Mutex::new(EncState::default()),
            dec: Mutex::new(FilterState::<1, 8>::new()),
        }
    }
}

impl UvproSbcCodec {
    pub fn new() -> Self {
        Self::default()
    }
}

fn bytes_to_pcm(b: &[u8]) -> Vec<i16> {
    b.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect()
}

fn pcm_to_bytes(p: &[i16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(p.len() * 2);
    for s in p {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

impl SbcCodec for UvproSbcCodec {
    /// Encode s16le PCM bytes → SBC bytes. Buffers a <128-sample residual across calls;
    /// emits whole 40-byte frames only. (A final partial frame is held until the next
    /// call completes it — the transport sends complete frames before `AudioEnd`.)
    fn encode(&self, pcm: &[u8]) -> Vec<u8> {
        let mut st = self.enc.lock().unwrap();
        st.residual.extend(bytes_to_pcm(pcm));
        let mut out = Vec::new();
        let mut filter = st.filter;
        let mut consumed = 0;
        while st.residual.len() - consumed >= SAMPLES_PER_FRAME {
            let frame: [i16; SAMPLES_PER_FRAME] =
                std::array::from_fn(|i| st.residual[consumed + i]);
            out.extend(encode_frame(&mut filter, &frame));
            consumed += SAMPLES_PER_FRAME;
        }
        st.filter = filter;
        st.residual.drain(..consumed);
        out
    }

    /// Decode SBC bytes → s16le PCM bytes (streaming synthesis-filter state).
    fn decode(&self, sbc: &[u8]) -> Vec<u8> {
        let mut filter = self.dec.lock().unwrap();
        pcm_to_bytes(&decode_into(&mut filter, sbc))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn golden_pcm() -> Vec<i16> {
        bytes_to_pcm(include_bytes!("testdata/sine1k_32k_mono.pcm"))
    }

    /// Decode of the ffmpeg golden vector round-trips to ~the source PCM, after the
    /// ~137-sample SBC synthesis-filterbank startup delay (proto measured MAE 6.7).
    #[test]
    fn decode_golden_sine_low_mae_after_delay() {
        let sbc = include_bytes!("testdata/sine1k_32k_mono.sbc");
        let refp = golden_pcm();
        let out = decode_sbc(sbc);
        assert_eq!(out.len(), refp.len());
        const DELAY: usize = 137;
        let n = refp.len() - DELAY;
        let mae: f64 = (0..n)
            .map(|i| (refp[i] as f64 - out[i + DELAY] as f64).abs())
            .sum::<f64>()
            / n as f64;
        assert!(mae < 50.0, "decode MAE {mae:.1} too high");
    }

    /// Our encoder produces VALID, decodable SBC: encode the source PCM, decode it
    /// back, and confirm the round-trip reconstructs the tone (steady-state, after the
    /// filterbank delay). Threshold is generous — the encoder is functional but not yet
    /// ffmpeg-parity (see the module QUALITY NOTE); this guards "decodable + roughly
    /// faithful", not bit-exactness.
    #[test]
    fn encode_round_trips_through_decoder() {
        let pcm = golden_pcm();
        let codec = UvproSbcCodec::new();
        let sbc = codec.encode(&pcm_to_bytes(&pcm));
        // 32 frames * 40 bytes for 4096 samples.
        assert_eq!(sbc.len(), (pcm.len() / SAMPLES_PER_FRAME) * 40);
        assert_eq!(sbc[0], 0x9C); // sync
        assert_eq!(sbc[1], 0x71); // 32k/16blk/mono/loudness/8sb
        assert_eq!(sbc[2], 0x10); // bitpool 16
        let out = bytes_to_pcm(&codec.decode(&sbc));
        const DELAY: usize = 137;
        let n = pcm.len().min(out.len()).saturating_sub(DELAY);
        let mae: f64 = (0..n)
            .map(|i| (pcm[i] as f64 - out[i + DELAY] as f64).abs())
            .sum::<f64>()
            / n.max(1) as f64;
        assert!(mae < 400.0, "encode round-trip MAE {mae:.1} unexpectedly high");
    }

    #[test]
    fn encode_buffers_partial_frame_residual() {
        let codec = UvproSbcCodec::new();
        // 100 samples < 128 → no full frame yet, all buffered.
        let out = codec.encode(&pcm_to_bytes(&[0i16; 100]));
        assert!(out.is_empty());
        // 28 more → completes one 128-sample frame.
        let out2 = codec.encode(&pcm_to_bytes(&[0i16; 28]));
        assert_eq!(out2.len(), 40);
    }

    #[test]
    fn decode_garbage_does_not_panic() {
        let _ = decode_sbc(&[0x00, 0x01, 0x02]);
        let _ = decode_sbc(&[]);
    }

    #[test]
    fn decode_truncated_frame_body_does_not_panic() {
        // Adrev P0 regression: valid sync+header(0x71)+bitpool(0x10)+CRC + 4 scale-factor
        // bytes, but NO audio sample body. Frame construction SUCCEEDS, then the body read
        // fails mid-frame. The RX path sees this on a lossy RF link; it must NOT panic
        // (the `for block in frame` Iterator path used to `.unwrap()` and panic here).
        let truncated = [0x9C, 0x71, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00];
        let _ = decode_sbc(&truncated);
        // Also: a real frame followed by a truncated one — must decode the good frame,
        // then stop gracefully without panicking on the partial tail.
        let codec = UvproSbcCodec::new();
        let mut sbc = codec.encode(&pcm_to_bytes(&golden_pcm()[..SAMPLES_PER_FRAME]));
        sbc.extend_from_slice(&[0x9C, 0x71, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00]);
        let _ = decode_sbc(&sbc);
    }
}
