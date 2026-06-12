//! Big-endian, bit-granular codec for the Benshi/Vero wire format (tuxlink-nx95).
//!
//! The native UV-Pro protocol packs fields at bit granularity, MSB-first, with
//! fields concatenated across byte boundaries (e.g. a 16-bit command group, a
//! 1-bit `is_reply`, a 15-bit command, a 30-bit frequency). `BitWriter` appends
//! N-bit big-endian values and zero-pads the final byte; `BitReader` reads them
//! back in the same order. Messages are ≤ ~30 bytes, so a `Vec<bool>` backing is
//! used for clarity over a packed cursor — the golden-vector tests are the proof
//! of correctness, derived from benlink's authoritative encoder.

/// Accumulates bits MSB-first and renders them to bytes (final byte zero-padded).
pub struct BitWriter {
    bits: Vec<bool>,
}

impl Default for BitWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl BitWriter {
    pub fn new() -> Self {
        Self { bits: Vec::new() }
    }

    /// Append the low `n` bits of `v`, most-significant bit first.
    pub fn write_uint(&mut self, v: u64, n: u32) {
        for i in (0..n).rev() {
            self.bits.push((v >> i) & 1 == 1);
        }
    }

    pub fn write_bool(&mut self, b: bool) {
        self.bits.push(b);
    }

    /// Append raw bytes (MSB-first within each byte). Used for fixed-width string
    /// / opaque fields like `name_str`.
    pub fn write_bytes(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.write_uint(*b as u64, 8);
        }
    }

    pub fn bit_len(&self) -> usize {
        self.bits.len()
    }

    /// Render to bytes; the final partial byte (if any) is zero-padded on the low
    /// side, matching benlink's encoder.
    pub fn into_bytes(self) -> Vec<u8> {
        let mut out = vec![0u8; self.bits.len().div_ceil(8)];
        for (i, b) in self.bits.iter().enumerate() {
            if *b {
                out[i / 8] |= 0x80 >> (i % 8);
            }
        }
        out
    }
}

/// Reads N-bit big-endian values from a byte slice, MSB-first.
pub struct BitReader<'a> {
    bytes: &'a [u8],
    pos: usize, // bit position
}

impl<'a> BitReader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    /// Read `n` bits as an unsigned integer (MSB-first). Panics only if the slice
    /// is too short — callers decode frames whose length they have already
    /// validated against the field layout, and `remaining_bits` guards the rest.
    pub fn read_uint(&mut self, n: u32) -> u64 {
        let mut v = 0u64;
        for _ in 0..n {
            let byte = self.bytes[self.pos / 8];
            let bit = (byte >> (7 - (self.pos % 8))) & 1;
            v = (v << 1) | bit as u64;
            self.pos += 1;
        }
        v
    }

    pub fn read_bool(&mut self) -> bool {
        self.read_uint(1) == 1
    }

    /// Read `n` whole bytes (only valid when byte-aligned; the Benshi fixed-width
    /// string fields are byte-aligned in every message we decode).
    pub fn read_bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.read_uint(8) as u8).collect()
    }

    pub fn remaining_bits(&self) -> usize {
        self.bytes.len() * 8 - self.pos
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_then_reads_back_across_byte_boundary() {
        // command_group(16)=2, is_reply=0, command(15)=0x14 → GET_HT_STATUS golden header
        let mut w = BitWriter::new();
        w.write_uint(0x0002, 16);
        w.write_bool(false);
        w.write_uint(0x14, 15);
        assert_eq!(w.into_bytes(), vec![0x00, 0x02, 0x00, 0x14]);

        // is_reply=1 variant: byte 2 MSB set → 0x80
        let mut r = BitReader::new(&[0x00, 0x02, 0x80, 0x14]);
        assert_eq!(r.read_uint(16), 0x0002);
        assert!(r.read_bool());
        assert_eq!(r.read_uint(15), 0x14);
    }

    #[test]
    fn packs_u30_freq_with_mod_prefix() {
        // tx_mod(2)=00 (FM) then tx_freq u30 = 146_520_000 → 08 bb b7 c0
        let mut w = BitWriter::new();
        w.write_uint(0, 2);
        w.write_uint(146_520_000, 30);
        assert_eq!(w.into_bytes(), vec![0x08, 0xbb, 0xb7, 0xc0]);

        let mut r = BitReader::new(&[0x08, 0xbb, 0xb7, 0xc0]);
        assert_eq!(r.read_uint(2), 0);
        assert_eq!(r.read_uint(30), 146_520_000);
    }

    #[test]
    fn round_trips_bytes_field() {
        let mut w = BitWriter::new();
        w.write_uint(0, 4); // unaligned prefix
        w.write_bytes(&[0x43, 0x41]);
        // 0000 0100_0011 0100_0001 → 04 34 1(pad0) ... check via reader instead
        let bytes = w.into_bytes();
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read_uint(4), 0);
        assert_eq!(r.read_bytes(2), vec![0x43, 0x41]);
    }

    #[test]
    fn zero_pads_final_byte() {
        let mut w = BitWriter::new();
        w.write_uint(0b101, 3);
        // 101 → 1010_0000 = 0xA0
        assert_eq!(w.into_bytes(), vec![0xA0]);
    }
}
