//! The lzhuf compression used by the binary Winlink/FBB B2 protocol.
//!
//! Message bodies travel compressed. The format is LZHUF — an LZSS sliding
//! window whose tokens are then squeezed with an adaptive Huffman code — with a
//! small header on the front:
//!
//! ```text
//! [ CRC16, 2 bytes, little-endian ]
//! [ uncompressed size, 4 bytes, little-endian ]
//! [ Huffman bitstream ... ]
//! ```
//!
//! The CRC covers the size bytes and the compressed bytes (it is CRC-16/XMODEM,
//! reached here by the "append two zero bytes" convention). This file is checked
//! against `la5nta/wl2k-go`'s lzhuf, which is verified against the real Winlink
//! CMS: it must decompress wl2k-go's output and produce byte-identical output
//! when compressing the same input. No Go code ships — wl2k-go is a read-only
//! reference for the wire format.

/// The CRC-16/XMODEM lookup table (polynomial 0x1021, most-significant bit
/// first), built once at compile time.
const CRC16_TABLE: [u16; 256] = build_crc16_table();

const fn build_crc16_table() -> [u16; 256] {
    let mut table = [0u16; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = (i as u16) << 8;
        let mut bit = 0;
        while bit < 8 {
            c = if c & 0x8000 != 0 { (c << 1) ^ 0x1021 } else { c << 1 };
            bit += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
}

/// Compute the FBB B2 CRC over `data` (the size bytes followed by the compressed
/// bytes). Two zero bytes are appended before finalizing, which turns the
/// streaming CRC into the standard CRC-16/XMODEM value.
fn fbb_crc(data: &[u8]) -> u16 {
    let mut sum: u16 = 0;
    let step = |sum: u16, byte: u8| -> u16 {
        ((sum << 8) & 0xff00) ^ CRC16_TABLE[((sum >> 8) & 0xff) as usize] ^ u16::from(byte)
    };
    for &b in data {
        sum = step(sum, b);
    }
    sum = step(sum, 0);
    sum = step(sum, 0);
    sum
}

// --- LZHUF parameters (named as in the original lzhuf) ---

/// Sliding-window size (bytes).
const N: usize = 2048;
/// Lookahead-buffer size: the longest match we will encode.
const F: usize = 60;
/// Matches shorter than this are not worth encoding as a back-reference.
const THRESHOLD: usize = 2;
/// Number of distinct codes the adaptive Huffman tree handles: 256 literal
/// bytes plus the match-length codes.
const NUM_CHAR: usize = 256 - THRESHOLD + F;
/// Size of the Huffman tree's node table.
const T: usize = NUM_CHAR * 2 - 1;
/// Index of the tree's root node.
const R: usize = T - 1;
/// When the root's frequency reaches this, the tree is rebuilt with halved
/// frequencies so the counts stay bounded.
const MAX_FREQ: u16 = 0x8000;

/// Upper-six-bits position code, encode side (transcribed from the reference).
const P_CODE: [u8; 64] = [
    0x00, 0x20, 0x30, 0x40, 0x50, 0x58, 0x60, 0x68, 0x70, 0x78, 0x80, 0x88, 0x90, 0x94, 0x98, 0x9C,
    0xA0, 0xA4, 0xA8, 0xAC, 0xB0, 0xB4, 0xB8, 0xBC, 0xC0, 0xC2, 0xC4, 0xC6, 0xC8, 0xCA, 0xCC, 0xCE,
    0xD0, 0xD2, 0xD4, 0xD6, 0xD8, 0xDA, 0xDC, 0xDE, 0xE0, 0xE2, 0xE4, 0xE6, 0xE8, 0xEA, 0xEC, 0xEE,
    0xF0, 0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8, 0xF9, 0xFA, 0xFB, 0xFC, 0xFD, 0xFE, 0xFF,
];
/// Bit length of each `P_CODE` entry.
const P_LEN: [u8; 64] = [
    0x03, 0x04, 0x04, 0x04, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x06, 0x06, 0x06, 0x06,
    0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07,
    0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07,
    0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08,
];

/// The decode-side position tables are the inverse of the encode tables: given
/// a byte read from the stream, `D_CODE` recovers the upper six bits of the
/// position and `D_LEN` says how many of the byte's high bits were the code.
const D_CODE: [u8; 256] = build_decode_tables().0;
const D_LEN: [u8; 256] = build_decode_tables().1;

const fn build_decode_tables() -> ([u8; 256], [u8; 256]) {
    let mut d_code = [0u8; 256];
    let mut d_len = [0u8; 256];
    let mut u = 0;
    while u < 64 {
        let len = P_LEN[u];
        let prefix = P_CODE[u] >> (8 - len);
        let mut b = 0;
        while b < 256 {
            if (b as u8) >> (8 - len) == prefix {
                d_code[b] = u as u8;
                d_len[b] = len;
            }
            b += 1;
        }
        u += 1;
    }
    (d_code, d_len)
}

/// The adaptive Huffman tree shared by the encoder and decoder. As symbols are
/// coded their frequencies rise; the tree reshapes itself to keep frequent
/// symbols on short paths. Both sides run the same updates in lockstep, so they
/// never have to transmit the code table.
struct HuffTree {
    freq: [u16; T + 1],
    /// Parent of each node. The extra `NUM_CHAR` slots map a symbol code to its
    /// leaf node (`prnt[code + T]`).
    prnt: [usize; T + NUM_CHAR],
    /// Children: for an internal node, `son[i]` and `son[i] + 1`; a value `>= T`
    /// marks a leaf and identifies its symbol.
    son: [usize; T],
}

impl HuffTree {
    fn new() -> Self {
        let mut t = HuffTree {
            freq: [0; T + 1],
            prnt: [0; T + NUM_CHAR],
            son: [0; T],
        };
        for i in 0..NUM_CHAR {
            t.freq[i] = 1;
            t.son[i] = i + T;
            t.prnt[i + T] = i;
        }
        let mut i = 0;
        let mut j = NUM_CHAR;
        while j <= R {
            t.freq[j] = t.freq[i] + t.freq[i + 1];
            t.son[j] = i;
            t.prnt[i] = j;
            t.prnt[i + 1] = j;
            i += 2;
            j += 1;
        }
        t.freq[T] = 0xffff;
        t.prnt[R] = 0;
        t
    }

    /// Rebuild the tree from scratch with halved frequencies. Called when the
    /// root frequency hits its cap so the counts can't overflow and recent
    /// symbols keep more weight than ancient ones.
    fn reconst(&mut self) {
        // Gather the leaves into the front of the table, halving their counts.
        let mut j = 0;
        for i in 0..T {
            if self.son[i] >= T {
                self.freq[j] = (self.freq[i] + 1) / 2;
                self.son[j] = self.son[i];
                j += 1;
            }
        }
        // Rebuild the internal nodes, keeping `freq` sorted as we go.
        let mut i = 0;
        let mut j = NUM_CHAR;
        while j < T {
            let f = self.freq[i] + self.freq[i + 1];
            self.freq[j] = f;
            let mut k = j;
            while f < self.freq[k - 1] {
                k -= 1;
            }
            let last = j - k;
            self.freq.copy_within(k..k + last, k + 1);
            self.freq[k] = f;
            self.son.copy_within(k..k + last, k + 1);
            self.son[k] = i;
            i += 2;
            j += 1;
        }
        // Re-link parents.
        for i in 0..T {
            let k = self.son[i];
            if k >= T {
                self.prnt[k] = i;
            } else {
                self.prnt[k + 1] = i;
                self.prnt[k] = i;
            }
        }
    }

    /// Record that symbol `c` was just coded: bump its frequency and bubble it
    /// up so the tree stays ordered by frequency.
    fn update(&mut self, c: usize) {
        if self.freq[R] == MAX_FREQ {
            self.reconst();
        }
        let mut c = self.prnt[c + T];
        loop {
            self.freq[c] += 1;
            if self.freq[c] <= self.freq[c + 1] || c + 2 >= self.freq.len() {
                c = self.prnt[c];
                if c == 0 {
                    break;
                }
                continue;
            }
            // The node now out-ranks its neighbour; find where it belongs and
            // swap it with the highest node of equal-or-lower frequency.
            let k = self.freq[c];
            let mut l = c + 1;
            while k > self.freq[l + 1] {
                l += 1;
            }
            self.freq[c] = self.freq[l];
            self.freq[l] = k;

            let i = self.son[c];
            self.prnt[i] = l;
            if i < T {
                self.prnt[i + 1] = l;
            }
            let m = self.son[l];
            self.son[l] = i;
            self.prnt[m] = c;
            if m < T {
                self.prnt[m + 1] = c;
            }
            self.son[c] = m;

            c = self.prnt[l];
            if c == 0 {
                break;
            }
        }
    }
}

/// Reads values bit by bit from a byte slice, most-significant bit first.
struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    acc: u32,
    nbits: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        BitReader { data, pos: 0, acc: 0, nbits: 0 }
    }

    fn read_bits(&mut self, bits: u32) -> Result<u32, LzhufError> {
        while self.nbits < bits {
            let byte = *self.data.get(self.pos).ok_or(LzhufError::UnexpectedEnd)?;
            self.pos += 1;
            self.acc = (self.acc << 8) | u32::from(byte);
            self.nbits += 8;
        }
        let n = (self.acc >> (self.nbits - bits)) & ((1 << bits) - 1);
        self.nbits -= bits;
        Ok(n)
    }
}

/// Decode one symbol by walking the Huffman tree from the root, taking the
/// 0-child or 1-child according to each bit, until a leaf is reached.
fn decode_char(tree: &mut HuffTree, reader: &mut BitReader) -> Result<usize, LzhufError> {
    let mut c = tree.son[R];
    while c < T {
        c += reader.read_bits(1)? as usize;
        c = tree.son[c];
    }
    c -= T;
    tree.update(c);
    Ok(c)
}

/// Decode a back-reference position: one byte selects the upper six bits via
/// the table, then the remaining low bits are read verbatim.
fn decode_position(reader: &mut BitReader) -> Result<usize, LzhufError> {
    let byte = reader.read_bits(8)? as usize;
    let high = (D_CODE[byte] as usize) << 6;
    let mut i = byte;
    let mut remaining = D_LEN[byte] - 2;
    while remaining > 0 {
        i = (i << 1) + reader.read_bits(1)? as usize;
        remaining -= 1;
    }
    Ok(high | (i & 0x3f))
}

/// Decompress an FBB B2 lzhuf stream (`[CRC16][size][bitstream]`) back to the
/// original bytes, verifying the checksum.
pub fn decompress(input: &[u8]) -> Result<Vec<u8>, LzhufError> {
    if input.len() < 6 {
        return Err(LzhufError::TruncatedHeader);
    }
    let stored_crc = u16::from_le_bytes([input[0], input[1]]);
    let size = u32::from_le_bytes([input[2], input[3], input[4], input[5]]) as usize;

    // The checksum covers the size bytes and the compressed bytes that follow.
    if fbb_crc(&input[2..]) != stored_crc {
        return Err(LzhufError::BadChecksum);
    }

    let mut tree = HuffTree::new();
    let mut reader = BitReader::new(&input[6..]);

    // The sliding window starts full of spaces, matching the encoder.
    let mut text_buf = [b' '; N];
    let mut r = N - R;

    let mut out = Vec::with_capacity(size);
    while out.len() < size {
        let c = decode_char(&mut tree, &mut reader)?;
        if c < 256 {
            out.push(c as u8);
            text_buf[r] = c as u8;
            r = (r + 1) & (N - 1);
        } else {
            // A back-reference: copy `length` bytes from `start` in the window.
            let start = (r.wrapping_sub(decode_position(&mut reader)?).wrapping_sub(1)) & (N - 1);
            let length = c - 255 + THRESHOLD;
            for k in 0..length {
                let b = text_buf[(start + k) & (N - 1)];
                out.push(b);
                text_buf[r] = b;
                r = (r + 1) & (N - 1);
            }
        }
    }
    out.truncate(size);
    Ok(out)
}

/// The "no node here" marker for the match-finder's trees.
const NIL: usize = N;

/// Compresses bytes into the FBB B2 lzhuf format.
///
/// The match finder is a set of binary search trees over the sliding window
/// (one tree per leading byte), so each new position can quickly find the
/// longest earlier run that matches the upcoming bytes. Found runs become
/// (length, distance) back-references; everything else is a literal byte. Both
/// kinds are then Huffman-coded by the shared adaptive tree.
struct Encoder {
    tree: HuffTree,
    text_buf: [u8; N + F - 1],

    // The match-finder trees (parent / left child / right child links).
    dad: [usize; N + 1],
    lson: [usize; N + 1],
    rson: [usize; N + 257],
    match_length: usize,
    match_position: usize,

    // The output bitstream and the bit-packing accumulator.
    out: Vec<u8>,
    putbuf: u64,
    putlen: u32,

    len: usize,
    r: usize,
    s: usize,
    last_match_length: usize,
    pre_filled: bool,
    file_size: u32,
}

impl Encoder {
    fn new() -> Self {
        let mut e = Encoder {
            tree: HuffTree::new(),
            text_buf: [0u8; N + F - 1],
            dad: [0; N + 1],
            lson: [0; N + 1],
            rson: [0; N + 257],
            match_length: 0,
            match_position: 0,
            out: Vec::new(),
            putbuf: 0,
            putlen: 0,
            len: 0,
            r: N - F,
            s: 0,
            last_match_length: 0,
            pre_filled: false,
            file_size: 0,
        };
        // Empty trees: the per-first-byte roots and every node start unlinked.
        for i in (N + 1)..=(N + 256) {
            e.rson[i] = NIL;
        }
        for i in 0..N {
            e.dad[i] = NIL;
        }
        // The window starts full of spaces, up to the first write position.
        for i in 0..(N - F) {
            e.text_buf[i] = b' ';
        }
        e
    }

    /// Add the run starting at window position `pos` to its search tree, and
    /// record the longest match found along the way in `match_length` /
    /// `match_position`.
    fn insert_node(&mut self, pos: usize) {
        let mut cmp: i32 = 1;
        let mut p = N + 1 + self.text_buf[pos] as usize;
        self.rson[pos] = NIL;
        self.lson[pos] = NIL;
        self.match_length = 0;
        let mut i;
        loop {
            if cmp >= 0 {
                if self.rson[p] != NIL {
                    p = self.rson[p];
                } else {
                    self.rson[p] = pos;
                    self.dad[pos] = p;
                    return;
                }
            } else if self.lson[p] != NIL {
                p = self.lson[p];
            } else {
                self.lson[p] = pos;
                self.dad[pos] = p;
                return;
            }
            i = 1;
            while i < F {
                cmp = self.text_buf[pos + i] as i32 - self.text_buf[p + i] as i32;
                if cmp != 0 {
                    break;
                }
                i += 1;
            }
            if i > THRESHOLD {
                if i > self.match_length {
                    self.match_position = (pos.wrapping_sub(p) & (N - 1)).wrapping_sub(1);
                    self.match_length = i;
                    if self.match_length >= F {
                        break;
                    }
                }
                if i == self.match_length {
                    let c = (pos.wrapping_sub(p) & (N - 1)).wrapping_sub(1);
                    if c < self.match_position {
                        self.match_position = c;
                    }
                }
            }
        }
        // Replace node p with the new node, inheriting p's links.
        self.dad[pos] = self.dad[p];
        self.lson[pos] = self.lson[p];
        self.rson[pos] = self.rson[p];
        self.dad[self.lson[p]] = pos;
        self.dad[self.rson[p]] = pos;
        if self.rson[self.dad[p]] == p {
            self.rson[self.dad[p]] = pos;
        } else {
            self.lson[self.dad[p]] = pos;
        }
        self.dad[p] = NIL;
    }

    /// Remove the run at window position `p` from its search tree.
    fn delete_node(&mut self, p: usize) {
        if self.dad[p] == NIL {
            return;
        }
        let q;
        if self.rson[p] == NIL {
            q = self.lson[p];
        } else if self.lson[p] == NIL {
            q = self.rson[p];
        } else {
            let mut qq = self.lson[p];
            if self.rson[qq] != NIL {
                while self.rson[qq] != NIL {
                    qq = self.rson[qq];
                }
                self.rson[self.dad[qq]] = self.lson[qq];
                self.dad[self.lson[qq]] = self.dad[qq];
                self.lson[qq] = self.lson[p];
                self.dad[self.lson[p]] = qq;
            }
            self.rson[qq] = self.rson[p];
            self.dad[self.rson[p]] = qq;
            q = qq;
        }
        self.dad[q] = self.dad[p];
        if self.rson[self.dad[p]] == p {
            self.rson[self.dad[p]] = q;
        } else {
            self.lson[self.dad[p]] = q;
        }
        self.dad[p] = NIL;
    }

    fn write_all(&mut self, p: &[u8]) {
        let mut n = 0;
        // Pre-fill the lookahead buffer before we start emitting.
        while !self.pre_filled && n < p.len() {
            self.text_buf[self.r + self.len] = p[n];
            n += 1;
            self.file_size += 1;
            self.len += 1;
            self.insert_node(self.r - self.len);
            self.last_match_length = 1;
            self.pre_filled = self.len == F;
        }
        while n < p.len() {
            let c = p[n];
            self.advance(Some(c));
            n += 1;
            self.file_size += 1;
        }
    }

    fn advance(&mut self, c: Option<u8>) {
        if let Some(c) = c {
            self.text_buf[self.s] = c;
            if self.s < F - 1 {
                self.text_buf[self.s + N] = c;
            }
            self.len += 1;
        }
        self.insert_node(self.r);
        self.last_match_length -= 1;
        if self.last_match_length == 0 {
            self.encode();
        }
        self.delete_node(self.s);
        self.s = (self.s + 1) & (N - 1);
        self.r = (self.r + 1) & (N - 1);
        self.len -= 1;
    }

    /// Emit one token: either a literal byte, or a (length, position)
    /// back-reference if the match finder found a worthwhile run.
    fn encode(&mut self) {
        if self.len == 0 {
            return;
        }
        if self.match_length > self.len {
            self.match_length = self.len;
        }
        if self.match_length <= THRESHOLD {
            self.match_length = 1;
            self.encode_char(self.text_buf[self.r] as usize);
        } else {
            self.encode_char(255 - THRESHOLD + self.match_length);
            self.encode_position(self.match_position);
        }
        self.last_match_length = self.match_length;
    }

    fn encode_char(&mut self, c: usize) {
        // Walk leaf to root, building the code bit by bit.
        let mut code: u64 = 0;
        let mut len: u32 = 0;
        let mut k = self.tree.prnt[c + T];
        loop {
            code >>= 1;
            len += 1;
            if k & 1 != 0 {
                code += 0x8000;
            }
            k = self.tree.prnt[k];
            if k == R {
                break;
            }
        }
        self.put_code(len, code);
        self.tree.update(c);
    }

    fn encode_position(&mut self, c: usize) {
        // Upper six bits via the table, lower six bits verbatim.
        let i = c >> 6;
        self.put_code(P_LEN[i] as u32, (P_CODE[i] as u64) << 8);
        self.put_code(6, ((c & 0x3f) as u64) << 10);
    }

    /// Append `l` bits (held in the high end of `c`) to the output bitstream.
    fn put_code(&mut self, l: u32, c: u64) {
        self.putbuf |= c >> self.putlen;
        self.putlen += l;
        if self.putlen < 8 {
            return;
        }
        self.out.push((self.putbuf >> 8) as u8);
        self.putlen -= 8;
        if self.putlen >= 8 {
            self.out.push(self.putbuf as u8);
            self.putlen -= 8;
            self.putbuf = c << (l - self.putlen);
        } else {
            self.putbuf <<= 8;
        }
    }

    fn encode_end(&mut self) {
        if self.putlen == 0 {
            return;
        }
        self.out.push((self.putbuf >> 8) as u8);
    }

    /// Flush remaining bytes and assemble the framed output.
    fn finish(mut self) -> Vec<u8> {
        while self.len > 0 {
            self.advance(None);
        }
        self.encode();
        self.encode_end();

        let size_bytes = self.file_size.to_le_bytes();
        let mut framed = Vec::with_capacity(2 + 4 + self.out.len());
        // CRC covers the size bytes and the compressed bytes.
        let mut crc_input = size_bytes.to_vec();
        crc_input.extend_from_slice(&self.out);
        framed.extend_from_slice(&fbb_crc(&crc_input).to_le_bytes());
        framed.extend_from_slice(&size_bytes);
        framed.extend_from_slice(&self.out);
        framed
    }
}

/// Compress bytes into the FBB B2 lzhuf format
/// (`[CRC16][uncompressed size][bitstream]`).
pub fn compress(input: &[u8]) -> Vec<u8> {
    let mut encoder = Encoder::new();
    encoder.write_all(input);
    encoder.finish()
}

/// Why an lzhuf stream could not be decompressed.
#[derive(Debug, PartialEq, Eq)]
pub enum LzhufError {
    /// The input was too short to even hold the checksum and size header.
    TruncatedHeader,
    /// The stored checksum did not match the data — the stream is corrupt.
    BadChecksum,
    /// The bitstream ended before the whole message had been decoded.
    UnexpectedEnd,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_matches_the_well_known_xmodem_check_value() {
        // "123456789" is the standard CRC check string; CRC-16/XMODEM of it is
        // 0x31C3. The FBB convention of appending two zero bytes reproduces that
        // value, which confirms both the table and the accumulation step.
        assert_eq!(fbb_crc(b"123456789"), 0x31C3);
    }

    // Conformance vectors: the .lzh files were produced by la5nta/wl2k-go's
    // lzhuf (verified against the real Winlink CMS). Decompressing them must
    // reproduce the original text exactly. See testdata/lzhuf/PROVENANCE.md.
    const GETTYSBURG_TXT: &[u8] = include_bytes!("testdata/lzhuf/gettysburg.txt");
    const GETTYSBURG_LZH: &[u8] = include_bytes!("testdata/lzhuf/gettysburg.txt.lzh");
    const PI_TXT: &[u8] = include_bytes!("testdata/lzhuf/pi.txt");
    const PI_LZH: &[u8] = include_bytes!("testdata/lzhuf/pi.txt.lzh");

    #[test]
    fn decompresses_a_small_text_to_match_the_reference() {
        assert_eq!(decompress(GETTYSBURG_LZH).unwrap(), GETTYSBURG_TXT);
    }

    #[test]
    fn decompresses_a_large_text_that_rebuilds_the_huffman_tree() {
        // 100 KB of digits forces the adaptive Huffman tree past its frequency
        // cap at least once, exercising the tree-rebuild path.
        assert_eq!(decompress(PI_LZH).unwrap(), PI_TXT);
    }

    #[test]
    fn rejects_a_corrupted_stream_on_the_checksum() {
        let mut corrupt = GETTYSBURG_LZH.to_vec();
        let last = corrupt.len() - 1;
        corrupt[last] ^= 0xff; // flip bits in the compressed data
        assert_eq!(decompress(&corrupt), Err(LzhufError::BadChecksum));
    }

    #[test]
    fn compresses_a_small_text_byte_for_byte_against_the_reference() {
        assert_eq!(compress(GETTYSBURG_TXT), GETTYSBURG_LZH);
    }

    #[test]
    fn compresses_a_large_text_byte_for_byte_against_the_reference() {
        assert_eq!(compress(PI_TXT), PI_LZH);
    }

    #[test]
    fn round_trips_arbitrary_binary_data() {
        let data: Vec<u8> = (0..5000u32)
            .map(|i| ((i.wrapping_mul(37).wrapping_add(11)) ^ (i >> 3)) as u8)
            .collect();
        assert_eq!(decompress(&compress(&data)).unwrap(), data);
    }

    #[test]
    fn round_trips_empty_input() {
        assert_eq!(decompress(&compress(b"")).unwrap(), b"");
    }
}
