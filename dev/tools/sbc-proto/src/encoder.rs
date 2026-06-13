//! First-pass SBC 8-subband encoder (tuxlink-vgvn), float analysis. Calibrated by
//! round-trip through mini_sbc (the decode oracle). NOT yet bit-exact — this is the
//! iterate-against-MAE development version.
use crate::proto::PROTO;

const PI: f64 = std::f64::consts::PI;

/// Flat proto window, index n = i + 16*j (i in 0..16, j in 0..5); from PROTO[10][8] row-major.
fn c(n: usize) -> f64 { PROTO[n / 8][n % 8] }

/// Analysis filterbank: maintain X[80] history (X[0]=newest). Push 8 new samples,
/// window (16 taps over 5 phases), cosine-modulate -> 8 subband samples (float).
pub fn analyze(x: &mut [f64; 80], block: &[i16; 8]) -> [f64; 8] {
    // shift older samples back by 8, insert new (newest at X[0], reversed input order)
    for i in (8..80).rev() { x[i] = x[i - 8]; }
    for i in 0..8 { x[i] = block[7 - i] as f64; }
    let mut z = [0.0f64; 16];
    for i in 0..16 {
        let mut s = 0.0;
        for j in 0..5 { s += c(i + 16 * j) * x[i + 16 * j]; }
        z[i] = s;
    }
    let mut out = [0.0f64; 8];
    for k in 0..8 {
        let mut s = 0.0;
        for i in 0..16 {
            s += ((2 * k + 1) as f64 * (i as f64 - 4.0) * PI / 16.0).cos() * z[i];
        }
        out[k] = s;
    }
    out
}

const OFFSET8_32K: [i8; 8] = [-3, 0, 0, 0, 0, 0, 1, 2];
const BITPOOL: i32 = 16;

/// scale factor = bits to represent |v| (0..15).
pub fn scale_factor(max_abs: u32) -> u8 {
    if max_abs == 0 { 0 } else { (32 - max_abs.leading_zeros()).min(15) as u8 }
}

/// Port of mini_sbc calculate_bits (mono, Loudness, 8sb).
pub fn allocate(sf: &[u8; 8]) -> [u8; 8] {
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
        slicecount = bitneed.iter().map(|&n| {
            if n > bitslice + 1 && n < bitslice + 16 { 1 }
            else if n == bitslice + 1 { 2 } else { 0 }
        }).sum();
        if bitcount + slicecount >= BITPOOL { break; }
    }
    if bitcount + slicecount < BITPOOL { bitslice -= 1; bitcount += slicecount; }
    let mut bits = [0u8; 8];
    for sb in 0..8 {
        if bitneed[sb] < bitslice + 2 { bits[sb] = 0; }
        else { bits[sb] = ((bitneed[sb] - bitslice).min(16)) as u8; }
    }
    for sb in 0..8 {
        if bitcount >= BITPOOL { break; }
        if bits[sb] >= 2 && bits[sb] < 16 { bits[sb] += 1; bitcount += 1; }
        else if bitneed[sb] == bitslice + 1 && BITPOOL > bitcount + 1 { bits[sb] = 2; bitcount += 2; }
    }
    for sb in 0..8 {
        if bitcount >= BITPOOL { break; }
        if bits[sb] < 16 { bits[sb] += 1; bitcount += 1; }
    }
    bits
}

/// Invert mini_sbc dequant: out = (((s<<1|1)<<shift)/((1<<bits)-1)) - (1<<shift), shift=sf+3.
pub fn quantize(sample: i32, sf: u8, bits: u8) -> u32 {
    if bits == 0 { return 0; }
    let shift = sf as i64 + 1 + 2;
    let levels = (1i64 << bits) - 1;
    let one = 1i64 << shift;
    let s = (((sample as i64 + one) * levels) / one - 1) / 2;
    s.clamp(0, levels) as u32
}

/// Encode a frame of 16 blocks * 8 subbands of PCM (128 samples) -> SBC bytes.
/// `scale` is the tunable analysis->dequant-domain factor (calibrated by round-trip).
pub fn encode_frame(x: &mut [f64; 80], pcm: &[i16], scale: f64) -> Vec<u8> {
    // analyze 16 blocks -> subband ints
    let mut sub = [[0i32; 8]; 16];
    for (b, blk) in pcm.chunks_exact(8).take(16).enumerate() {
        let arr: [i16; 8] = std::array::from_fn(|i| blk[i]);
        let s = analyze(x, &arr);
        for sb in 0..8 { sub[b][sb] = (s[sb] * scale).round() as i32; }
    }
    // scale factors per subband (max over blocks)
    let mut sf = [0u8; 8];
    for sb in 0..8 {
        let m = (0..16).map(|b| sub[b][sb].unsigned_abs()).max().unwrap();
        sf[sb] = scale_factor(m);
    }
    let bits = allocate(&sf);
    // pack
    let mut out = vec![0x9C, 0x71, 0x10, 0x00]; // sync, header, bitpool, crc(placeholder)
    let mut bw = BitWriter::new();
    for sb in 0..8 { bw.write(sf[sb] as u32, 4); }
    for b in 0..16 {
        for sb in 0..8 {
            if bits[sb] > 0 { bw.write(quantize(sub[b][sb], sf[sb], bits[sb]), bits[sb] as usize); }
        }
    }
    out.extend(bw.finish());
    out
}

struct BitWriter { buf: Vec<u8>, acc: u32, nbits: u32 }
impl BitWriter {
    fn new() -> Self { Self { buf: Vec::new(), acc: 0, nbits: 0 } }
    fn write(&mut self, v: u32, bits: usize) {
        for i in (0..bits).rev() {
            self.acc = (self.acc << 1) | ((v >> i) & 1);
            self.nbits += 1;
            if self.nbits == 8 { self.buf.push(self.acc as u8); self.acc = 0; self.nbits = 0; }
        }
    }
    fn finish(mut self) -> Vec<u8> {
        if self.nbits > 0 { self.buf.push((self.acc << (8 - self.nbits)) as u8); }
        self.buf
    }
}
