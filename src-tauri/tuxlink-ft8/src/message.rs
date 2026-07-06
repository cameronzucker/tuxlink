//! FT-8 77-bit message pack/unpack (source encoding).
//!
//! # Clean-room provenance (see `PROVENANCE.md`)
//!
//! Implemented ONLY from:
//! - the QEX 2020 "The FT4 and FT8 Communication Protocols" paper
//!   (Franke/Somerville/Taylor), **Table 1** (message types + bit-field tags)
//!   and **Table 2** (field semantics + widths); and
//! - the MIT-licensed `ft8_lib` (kgoba) reference — algorithm and integer
//!   constants reproduced, but every non-trivial routine is re-expressed in
//!   idiomatic Rust rather than transliterated.
//!
//! Citations below point at `ft8_lib`'s `ft8/message.c` and `ft8/text.c`
//! (function names, since line numbers drift between revisions) and the QEX
//! Table 1/2 rows.
//!
//! # Payload representation
//!
//! FT-8 conveys exactly 77 payload bits (QEX §2). Every routine here works on a
//! [`Payload`] — a fixed `[u8; 10]` where the 77 bits are stored **MSB-first**:
//! payload bit 0 is bit 7 of `bytes[0]`, payload bit 76 is bit 3 of `bytes[9]`,
//! and the low 3 bits of `bytes[9]` are unused (always zero). This matches the
//! bit ordering the rest of the crate uses for the 174-bit codeword (QEX §4:
//! codeword bits map MSB-first onto channel-symbol triads). The `i3` type tag
//! occupies bits 74..76 (the high 3 of the last used nibble); `n3` (when `i3==0`)
//! occupies bits 71..73.
//!
//! # Scope (task T0.2)
//!
//! Implemented: type `i3=1` STANDARD (`c28 r1 c28 r1 R1 g15`), type `i3=0 n3=0`
//! FREE TEXT (`f71`), type `i3=0 n3=5` TELEMETRY (`t71`), the 28-bit standard
//! callsign codec (special tokens + basecall base-charset packing), the 15-bit
//! Maidenhead grid / report / sentinel codec, and the 10/12/22-bit callsign hash
//! with a slot-scoped hash table for `<...>`/`<CALL>` rendering.
//!
//! Deferred (see `TODO(T0.2-follow-up)` markers): EU VHF (`i3=2`), RTTY RU
//! (`i3=3`), full nonstandard-call type-4 packing, DXpedition, Field Day.

use std::collections::HashMap;

/// Number of payload bytes holding the 77 payload bits (77 bits -> 10 bytes,
/// top 3 bits of the last byte unused).
/// provenance: `ft8_lib` `ft8/message.h` `FTX_PAYLOAD_LENGTH_BYTES = 10` (MIT).
pub const PAYLOAD_BYTES: usize = 10;

/// Special sentinel token / limit constants shared by the callsign and grid codecs.
///
/// provenance: `ft8_lib` `ft8/message.c` `MAX22 / NTOKENS / MAXGRID4` (MIT).
mod codec_consts {
    /// `2^22` — the size of the 22-bit callsign-hash space.
    /// provenance: `ft8_lib` `message.c` `MAX22 = 4194304` (MIT).
    pub const MAX22: u32 = 4_194_304;
    /// Number of reserved special-token values below the hashed-callsign range.
    /// provenance: `ft8_lib` `message.c` `NTOKENS = 2063592` (MIT).
    pub const NTOKENS: u32 = 2_063_592;
    /// Largest packed 4-char Maidenhead grid value (`18*18*10*10`).
    /// provenance: `ft8_lib` `message.c` `MAXGRID4 = 32400` (MIT); QEX Table 2 `g15`.
    pub const MAXGRID4: u16 = 32_400;
    /// Multiplier for the 22-bit callsign hash (`0xAF5A2E6F3` = 47055833459).
    /// provenance: `ft8_lib` `message.c` `save_callsign()` `47055833459ull` (MIT).
    pub const HASH_MULT: u64 = 47_055_833_459;
}

// ── Character tables ────────────────────────────────────────────────────────
//
// FT-8 packs text with several fixed alphabets. Rather than reproduce ft8_lib's
// branchy `charn`/`nchar`, we express each alphabet as an explicit string and
// index into it. provenance: `ft8_lib` `ft8/text.h` table comments (MIT):

/// `" 0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ+-./?"` (42) — free-text / telemetry.
/// provenance: `ft8_lib` `text.h` `FT8_CHAR_TABLE_FULL` (MIT); QEX Table 2 `f71`.
const T_FULL: &[u8] = b" 0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ+-./?";
/// `" 0123456789...Z/"` (38) — hashed-callsign base-38 alphabet.
/// provenance: `ft8_lib` `text.h` `FT8_CHAR_TABLE_ALPHANUM_SPACE_SLASH` (MIT).
const T_ALNUM_SPACE_SLASH: &[u8] = b" 0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ/";
/// `" 0123456789...Z"` (37) — first basecall char.
/// provenance: `ft8_lib` `text.h` `FT8_CHAR_TABLE_ALPHANUM_SPACE` (MIT).
const T_ALNUM_SPACE: &[u8] = b" 0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
/// `" ABCDEFGHIJKLMNOPQRSTUVWXYZ"` (27) — basecall suffix chars.
/// provenance: `ft8_lib` `text.h` `FT8_CHAR_TABLE_LETTERS_SPACE` (MIT).
const T_LETTERS_SPACE: &[u8] = b" ABCDEFGHIJKLMNOPQRSTUVWXYZ";
/// `"0123456789...Z"` (36) — second basecall char.
/// provenance: `ft8_lib` `text.h` `FT8_CHAR_TABLE_ALPHANUM` (MIT).
const T_ALNUM: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
/// `"0123456789"` (10) — third basecall char (the digit).
/// provenance: `ft8_lib` `text.h` `FT8_CHAR_TABLE_NUMERIC` (MIT).
const T_NUMERIC: &[u8] = b"0123456789";

/// Index of `c` in `table`, or `None` if absent (`nchar` in ft8_lib).
fn nchar(c: u8, table: &[u8]) -> Option<u32> {
    table.iter().position(|&t| t == c).map(|p| p as u32)
}

/// Character at index `i` in `table` (`charn` in ft8_lib); `'_'` if out of range.
fn charn(i: u32, table: &[u8]) -> u8 {
    *table.get(i as usize).unwrap_or(&b'_')
}

// ── Payload bit container ───────────────────────────────────────────────────

/// The 77-bit FT-8 message payload, stored MSB-first in 10 bytes (see module docs).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Payload {
    /// The 10 payload bytes. Bit `n` (0-based, MSB-first) lives at
    /// `bytes[n / 8]`, mask `0x80 >> (n % 8)`.
    pub bytes: [u8; PAYLOAD_BYTES],
}

impl Payload {
    /// A zeroed payload.
    pub fn new() -> Self {
        Payload { bytes: [0; PAYLOAD_BYTES] }
    }

    /// The 3-bit `i3` message-type tag (payload bits 74..76).
    /// provenance: `ft8_lib` `message.c` `ftx_message_get_i3` (MIT); QEX Table 1.
    pub fn i3(&self) -> u8 {
        (self.bytes[9] >> 3) & 0x07
    }

    /// The 3-bit `n3` sub-type tag, meaningful only when `i3 == 0` (bits 71..73).
    /// provenance: `ft8_lib` `message.c` `ftx_message_get_n3` (MIT); QEX Table 1.
    pub fn n3(&self) -> u8 {
        ((self.bytes[8] << 2) & 0x04) | ((self.bytes[9] >> 6) & 0x03)
    }
}

/// The FT-8 message type as classified by `i3`/`n3`.
/// provenance: QEX Table 1; `ft8_lib` `message.c` `ftx_message_get_type` (MIT).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MessageType {
    /// `i3=0 n3=0` free text (`f71`).
    FreeText,
    /// `i3=0 n3=5` telemetry (`t71`).
    Telemetry,
    /// `i3=1` (and `i3=2`, deferred) standard message (`c28 r1 c28 r1 R1 g15`).
    Standard,
    /// A type recognized by tag but not implemented in T0.2.
    Unsupported {
        /// The `i3` tag observed.
        i3: u8,
        /// The `n3` tag observed (valid only when `i3 == 0`).
        n3: u8,
    },
}

impl Payload {
    /// Classify this payload by its `i3`/`n3` tags.
    pub fn message_type(&self) -> MessageType {
        let i3 = self.i3();
        match i3 {
            0 => match self.n3() {
                0 => MessageType::FreeText,
                5 => MessageType::Telemetry,
                n3 => MessageType::Unsupported { i3, n3 },
            },
            1 => MessageType::Standard,
            _ => MessageType::Unsupported { i3, n3: 0 },
        }
    }
}

/// Errors from packing a human-readable message into a payload.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PackError {
    /// The "to" (first) callsign could not be encoded.
    Callsign1,
    /// The "de" (second) callsign could not be encoded.
    Callsign2,
    /// The grid / report token could not be encoded.
    Grid,
    /// Free text exceeded 13 characters or held an out-of-alphabet character.
    FreeText,
    /// Telemetry hex was not 1..=18 hex digits (or exceeded 71 bits).
    Telemetry,
    /// The message shape is a deferred type (EU VHF, RTTY RU, nonstandard, …).
    Unsupported,
}

// ── Callsign hash table (slot-scoped) ───────────────────────────────────────

/// A slot-scoped table mapping the truncated 10/12/22-bit callsign hashes back
/// to the callsign text, so later-decoded messages that carry only a hash can
/// render `<CALL>` instead of `<...>`.
///
/// The 10/12/22-bit hashes are truncations of one 22-bit hash (the 12-bit is the
/// top 12 bits, the 10-bit is the top 10 bits), so a single 22-bit entry answers
/// all three lookups.
/// provenance: `ft8_lib` `message.c` `save_callsign` / `lookup_callsign` (MIT).
#[derive(Clone, Debug, Default)]
pub struct HashTable {
    by22: HashMap<u32, String>,
}

impl HashTable {
    /// An empty table.
    pub fn new() -> Self {
        HashTable { by22: HashMap::new() }
    }

    /// Remember `callsign` under its full 22-bit hash.
    pub fn save(&mut self, callsign: &str, hash22: u32) {
        self.by22.insert(hash22, callsign.to_string());
    }

    /// Look up a callsign by a hash of the given bit width (10, 12, or 22).
    ///
    /// The stored key is the 22-bit hash; for 10/12-bit lookups we compare the
    /// appropriate truncation.
    pub fn lookup(&self, hash: u32, bits: u8) -> Option<&str> {
        match bits {
            22 => self.by22.get(&hash).map(String::as_str),
            12 => self
                .by22
                .iter()
                .find(|(&k, _)| (k >> 10) == hash)
                .map(|(_, v)| v.as_str()),
            10 => self
                .by22
                .iter()
                .find(|(&k, _)| (k >> 12) == hash)
                .map(|(_, v)| v.as_str()),
            _ => None,
        }
    }
}

/// Compute the (22, 12, 10)-bit hash truncations of a trimmed callsign.
///
/// The callsign is packed into a base-38 integer (11 positions, space-padded on
/// the right), multiplied by a fixed constant, and the top 22 bits taken; the
/// 12- and 10-bit hashes are the top 12 and top 10 bits of that 22-bit value.
/// provenance: `ft8_lib` `message.c` `save_callsign` (MIT); QEX Table 2 `h22/h12/h10`.
pub fn callsign_hash(callsign: &str) -> Option<(u32, u32, u32)> {
    let mut n58: u64 = 0;
    let mut i = 0usize;
    for &b in callsign.as_bytes() {
        if i >= 11 {
            break;
        }
        let j = nchar(b, T_ALNUM_SPACE_SLASH)?;
        n58 = 38u64.wrapping_mul(n58).wrapping_add(j as u64);
        i += 1;
    }
    // Right-pad with spaces (index 0) to 11 positions.
    while i < 11 {
        n58 = 38u64.wrapping_mul(n58);
        i += 1;
    }
    let n22 = (((codec_consts::HASH_MULT.wrapping_mul(n58)) >> (64 - 22)) & 0x3F_FFFF) as u32;
    Some((n22, n22 >> 10, n22 >> 12))
}

// ── 28-bit standard-callsign codec ──────────────────────────────────────────

/// Pack a standard "basecall" (e.g. `K1ABC`, `W9XYZ`) into its base-charset
/// value, or `None` if it is not a valid basecall.
///
/// A basecall is 6 characters over `[alnum_space][alnum][digit][letter_space]³`,
/// with the callsign right-aligned so the mandatory digit lands in position 2.
/// (Swaziland `3DA0…` / Guinea `3X…` prefix work-arounds are deferred; see the
/// TODO.) provenance: `ft8_lib` `message.c` `pack_basecall` (MIT); QEX Table 2 `c28`.
fn pack_basecall(callsign: &str) -> Option<u32> {
    let cs = callsign.as_bytes();
    let length = cs.len();
    if length <= 2 {
        return None;
    }
    // TODO(T0.2-follow-up): 3DA0.../3X... prefix work-arounds (Swaziland/Guinea).
    let mut c6 = [b' '; 6];
    if cs[2].is_ascii_digit() && length <= 6 {
        // AB0XYZ — already digit-in-position-2, left-aligned.
        c6[..length].copy_from_slice(cs);
    } else if cs.len() >= 2 && cs[1].is_ascii_digit() && length <= 5 {
        // A0XYZ -> " A0XYZ" (shift right by one so the digit lands in position 2).
        c6[1..1 + length].copy_from_slice(cs);
    } else {
        return None;
    }

    let i0 = nchar(c6[0], T_ALNUM_SPACE)?;
    let i1 = nchar(c6[1], T_ALNUM)?;
    let i2 = nchar(c6[2], T_NUMERIC)?;
    let i3 = nchar(c6[3], T_LETTERS_SPACE)?;
    let i4 = nchar(c6[4], T_LETTERS_SPACE)?;
    let i5 = nchar(c6[5], T_LETTERS_SPACE)?;

    // Mixed-radix pack: 37 · 36 · 10 · 27 · 27 · 27.
    let mut n = i0;
    n = n * 36 + i1;
    n = n * 10 + i2;
    n = n * 27 + i3;
    n = n * 27 + i4;
    n = n * 27 + i5;
    Some(n)
}

/// Unpack a basecall from its base-charset value, right-trimmed.
/// provenance: `ft8_lib` `message.c` `unpack28` basecall branch (MIT).
fn unpack_basecall(mut n: u32) -> String {
    let mut c = [0u8; 6];
    c[5] = charn(n % 27, T_LETTERS_SPACE);
    n /= 27;
    c[4] = charn(n % 27, T_LETTERS_SPACE);
    n /= 27;
    c[3] = charn(n % 27, T_LETTERS_SPACE);
    n /= 27;
    c[2] = charn(n % 10, T_NUMERIC);
    n /= 10;
    c[1] = charn(n % 36, T_ALNUM);
    n /= 36;
    c[0] = charn(n % 37, T_ALNUM_SPACE);
    // TODO(T0.2-follow-up): 3D0->3DA0 / Q->3X prefix reversal work-arounds.
    let s: String = c.iter().map(|&b| b as char).collect();
    s.trim().to_string()
}

/// The special 28-bit token values (values below 3).
/// provenance: `ft8_lib` `message.c` `pack28`/`unpack28` (MIT).
const TOK_DE: u32 = 0;
const TOK_QRZ: u32 = 1;
const TOK_CQ: u32 = 2;

/// Pack a callsign/token into its 28-bit value plus the `/R`|`/P` suffix flag
/// (`ip`). Returns `(n28, ip)`.
///
/// Recognizes the special tokens `DE`/`QRZ`/`CQ`, a `/R` or `/P` basecall suffix,
/// standard basecalls, and (falling through) the 22-bit hashed nonstandard call.
/// `CQ nnn` / `CQ a[bcd]` modifiers are deferred.
/// provenance: `ft8_lib` `message.c` `pack28` (MIT); QEX Table 2 `c28`.
fn pack28(callsign: &str, hash: &mut HashTable) -> Option<(u32, u8)> {
    match callsign {
        "DE" => return Some((TOK_DE, 0)),
        "QRZ" => return Some((TOK_QRZ, 0)),
        "CQ" => return Some((TOK_CQ, 0)),
        _ => {}
    }
    // TODO(T0.2-follow-up): "CQ nnn" / "CQ a[bcd]" modifiers (values 3..=532443).

    let length = callsign.len();
    let mut ip = 0u8;
    let mut basecall = callsign;
    if callsign.ends_with("/P") || callsign.ends_with("/R") {
        ip = 1;
        basecall = &callsign[..length - 2];
    }

    if let Some(n) = pack_basecall(basecall) {
        // Standard callsign: record its hash so a later hashed reference resolves.
        if let Some((h22, _, _)) = callsign_hash(callsign) {
            hash.save(callsign, h22);
        }
        return Some((codec_consts::NTOKENS + codec_consts::MAX22 + n, ip));
    }

    // Nonstandard callsign: encode as its 22-bit hash.
    if (3..=11).contains(&length) {
        let (h22, _, _) = callsign_hash(callsign)?;
        hash.save(callsign, h22);
        return Some((codec_consts::NTOKENS + h22, 0));
    }

    None
}

/// Unpack a 28-bit callsign value (`ip` = suffix flag, `i3` selects `/R` vs `/P`).
/// provenance: `ft8_lib` `message.c` `unpack28` (MIT).
fn unpack28(n28: u32, ip: u8, i3: u8, hash: &HashTable) -> String {
    if n28 < codec_consts::NTOKENS {
        // Special tokens (only the three low values are implemented in T0.2).
        return match n28 {
            TOK_DE => "DE".to_string(),
            TOK_QRZ => "QRZ".to_string(),
            TOK_CQ => "CQ".to_string(),
            // TODO(T0.2-follow-up): CQ nnn / CQ a[bcd] token args (3..=532443).
            _ => "CQ".to_string(),
        };
    }
    let n = n28 - codec_consts::NTOKENS;
    if n < codec_consts::MAX22 {
        // 22-bit hashed callsign — render from the slot hash table, or `<...>`.
        return match hash.lookup(n, 22) {
            Some(call) => format!("<{}>", call),
            None => "<...>".to_string(),
        };
    }
    // Standard callsign.
    let mut call = unpack_basecall(n - codec_consts::MAX22);
    if ip != 0 {
        match i3 {
            1 => call.push_str("/R"),
            2 => call.push_str("/P"),
            _ => {}
        }
    }
    call
}

// ── 15-bit grid / report / sentinel codec ───────────────────────────────────

/// Pack the `extra` field (4-char grid, signal report, or `RRR`/`RR73`/`73`/blank)
/// into a 15-bit `g15` value plus the `R`-prefix flag (`ir`). Returns `(g15, ir)`.
/// provenance: `ft8_lib` `message.c` `packgrid` (MIT); QEX Table 2 `g15/R1/r2`.
fn packgrid(extra: &str) -> (u16, u8) {
    if extra.is_empty() {
        return (codec_consts::MAXGRID4 + 1, 0); // blank: two callsigns only
    }
    match extra {
        "RRR" => return (codec_consts::MAXGRID4 + 2, 0),
        "RR73" => return (codec_consts::MAXGRID4 + 3, 0),
        "73" => return (codec_consts::MAXGRID4 + 4, 0),
        _ => {}
    }
    let b = extra.as_bytes();
    // Standard 4-char Maidenhead grid, e.g. "FN42".
    if b.len() == 4
        && (b'A'..=b'R').contains(&b[0])
        && (b'A'..=b'R').contains(&b[1])
        && b[2].is_ascii_digit()
        && b[3].is_ascii_digit()
    {
        let mut g = (b[0] - b'A') as u16;
        g = g * 18 + (b[1] - b'A') as u16;
        g = g * 10 + (b[2] - b'0') as u16;
        g = g * 10 + (b[3] - b'0') as u16;
        return (g, 0);
    }
    // Signal report: +dd / -dd (ir=0) or R+dd / R-dd (ir=1).
    if b[0] == b'R' {
        let dd = dd_to_int(&extra[1..]);
        let irpt = (35 + dd) as u16;
        (codec_consts::MAXGRID4 + irpt, 1)
    } else {
        let dd = dd_to_int(extra);
        let irpt = (35 + dd) as u16;
        (codec_consts::MAXGRID4 + irpt, 0)
    }
}

/// Unpack a 15-bit `g15` value (`ir` = `R`-prefix flag) back to the `extra` field.
/// provenance: `ft8_lib` `message.c` `unpackgrid` (MIT).
fn unpackgrid(g15: u16, ir: u8) -> String {
    if g15 <= codec_consts::MAXGRID4 {
        let mut n = g15;
        let mut grid = [0u8; 4];
        grid[3] = b'0' + (n % 10) as u8;
        n /= 10;
        grid[2] = b'0' + (n % 10) as u8;
        n /= 10;
        grid[1] = b'A' + (n % 18) as u8;
        n /= 18;
        grid[0] = b'A' + (n % 18) as u8;
        let g: String = grid.iter().map(|&b| b as char).collect();
        if ir > 0 {
            format!("R {}", g)
        } else {
            g
        }
    } else {
        let irpt = (g15 - codec_consts::MAXGRID4) as i32;
        match irpt {
            1 => String::new(), // blank
            2 => "RRR".to_string(),
            3 => "RR73".to_string(),
            4 => "73".to_string(),
            _ => {
                // Signal report: two-digit number with a forced sign, optional "R".
                let dd = irpt - 35;
                if ir > 0 {
                    format!("R{}", fmt_report(dd))
                } else {
                    fmt_report(dd)
                }
            }
        }
    }
}

/// Format a signal-report value as a signed two-digit number (`+01`, `-12`).
/// provenance: `ft8_lib` `text.c` `int_to_dd(..., full_sign=true)` (MIT).
fn fmt_report(dd: i32) -> String {
    if dd < 0 {
        format!("-{:02}", -dd)
    } else {
        format!("+{:02}", dd)
    }
}

/// Parse a signed integer from the leading digits of `s` (`dd_to_int` in ft8_lib).
/// provenance: `ft8_lib` `text.c` `dd_to_int` (MIT).
fn dd_to_int(s: &str) -> i32 {
    let bytes = s.as_bytes();
    let (neg, mut i) = match bytes.first() {
        Some(b'-') => (true, 1),
        Some(b'+') => (false, 1),
        _ => (false, 0),
    };
    let mut result = 0i32;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        result = result * 10 + (bytes[i] - b'0') as i32;
        i += 1;
    }
    if neg {
        -result
    } else {
        result
    }
}

// ── Public pack / unpack ────────────────────────────────────────────────────

/// Normalize a human message: uppercase letters and collapse runs of spaces.
/// provenance: `ft8_lib` `text.c` `fmtmsg` (MIT).
fn fmtmsg(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_space = false;
    for c in input.chars() {
        if c == ' ' {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
        } else {
            out.extend(c.to_uppercase());
            last_space = false;
        }
    }
    out
}

/// Pack a human-readable message string into a 77-bit [`Payload`].
///
/// Tries, in order, a STANDARD (`i3=1`) message, then FREE TEXT (`i3=0 n3=0`),
/// matching ft8_lib's `ftx_message_encode` fallback chain (minus the deferred
/// nonstandard/EU-VHF branches). The `hash` table records any callsigns seen so
/// later hashed references can render.
/// provenance: `ft8_lib` `message.c` `ftx_message_encode` (MIT).
pub fn pack(message: &str, hash: &mut HashTable) -> Result<Payload, PackError> {
    let msg = fmtmsg(message.trim());
    let tokens: Vec<&str> = msg.split(' ').filter(|t| !t.is_empty()).collect();

    // Standard message needs exactly 2 or 3 tokens (to, de, [grid/report]).
    if tokens.len() == 2 || tokens.len() == 3 {
        let call_to = tokens[0];
        let call_de = tokens[1];
        let extra = tokens.get(2).copied().unwrap_or("");
        if let Ok(p) = pack_std(call_to, call_de, extra, hash) {
            return Ok(p);
        }
    }
    // Fall back to free text.
    pack_free_text(&msg)
}

/// Pack a STANDARD (`i3=1`) message from its three fields.
/// provenance: `ft8_lib` `message.c` `ftx_message_encode_std` (MIT); QEX Table 1 row `1.`.
pub fn pack_std(
    call_to: &str,
    call_de: &str,
    extra: &str,
    hash: &mut HashTable,
) -> Result<Payload, PackError> {
    // TODO(T0.2-follow-up): i3=2 (EU VHF) packs the `/P` suffix. T0.2 packs i3=1
    // only, which carries the `/R` suffix; a `/P` call must wait for i3=2, so
    // refuse here and let the caller fall back to free text rather than emit a
    // payload mis-tagged as i3=1.
    if call_to.ends_with("/P") || call_de.ends_with("/P") {
        return Err(PackError::Unsupported);
    }

    let (n28a, ipa) = pack28(call_to, hash).ok_or(PackError::Callsign1)?;
    let (n28b, ipb) = pack28(call_de, hash).ok_or(PackError::Callsign2)?;
    let (g15, ir) = packgrid(extra);

    let i3: u8 = 1;
    let n29a = (n28a << 1) | ipa as u32;
    let n29b = (n28b << 1) | ipb as u32;

    // g15 carries the R flag in bit 15 of the 16-bit packgrid return; the payload
    // stores that same value in the 15-bit field plus the standalone R1 bit.
    let g = g15 | if ir != 0 { 0x8000 } else { 0 };

    let mut p = Payload::new();
    p.bytes[0] = (n29a >> 21) as u8;
    p.bytes[1] = (n29a >> 13) as u8;
    p.bytes[2] = (n29a >> 5) as u8;
    p.bytes[3] = ((n29a << 3) as u8) | ((n29b >> 26) as u8);
    p.bytes[4] = (n29b >> 18) as u8;
    p.bytes[5] = (n29b >> 10) as u8;
    p.bytes[6] = (n29b >> 2) as u8;
    p.bytes[7] = ((n29b << 6) as u8) | ((g >> 10) as u8);
    p.bytes[8] = (g >> 2) as u8;
    p.bytes[9] = ((g << 6) as u8) | (i3 << 3);
    Ok(p)
}

/// Pack a FREE TEXT (`i3=0 n3=0`) message (up to 13 chars over `T_FULL`).
/// provenance: `ft8_lib` `message.c` `ftx_message_encode_free` (MIT); QEX Table 1 row `0.0`.
pub fn pack_free_text(text: &str) -> Result<Payload, PackError> {
    let msg = fmtmsg(text);
    if msg.chars().count() > 13 {
        return Err(PackError::FreeText);
    }
    let bytes = msg.as_bytes();
    // Accumulate a base-42 big integer across 13 positions into a 71-bit value
    // held in 9 bytes (b71), MSB-first.
    let mut b71 = [0u8; 9];
    for idx in 0..13 {
        let c = if idx < bytes.len() { bytes[idx] } else { b' ' };
        let cid = nchar(c, T_FULL).ok_or(PackError::FreeText)?;
        let mut rem = cid as u32;
        for i in (0..9).rev() {
            rem += b71[i] as u32 * 42;
            b71[i] = (rem & 0xff) as u8;
            rem >>= 8;
        }
    }
    Ok(telemetry_payload_from_b71(&b71))
}

/// Pack a TELEMETRY (`i3=0 n3=5`) message from up to 18 hex digits (71 bits).
/// provenance: `ft8_lib` `message.c` `ftx_message_encode_telemetry` (MIT); QEX Table 1 row `0.5`.
pub fn pack_telemetry(hex: &str) -> Result<Payload, PackError> {
    let hex = hex.trim();
    if hex.is_empty() || hex.len() > 18 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(PackError::Telemetry);
    }
    // Right-align 18 hex nibbles into 9 bytes (b71). Telemetry is 71 bits, so the
    // top bit must be clear.
    let padded = format!("{:0>18}", hex.to_uppercase());
    let mut b71 = [0u8; 9];
    for i in 0..9 {
        b71[i] = u8::from_str_radix(&padded[i * 2..i * 2 + 2], 16).map_err(|_| PackError::Telemetry)?;
    }
    if b71[0] & 0x80 != 0 {
        return Err(PackError::Telemetry); // exceeds 71 bits
    }
    let mut p = telemetry_payload_from_b71(&b71);
    // Tag as telemetry: i3=0, n3=5. n3 bit-fields: get_n3 reads
    // ((payload[8]<<2)&4) | ((payload[9]>>6)&3), so n3=5 (0b101) => payload[8] bit0=1,
    // payload[9] bits6..7 = 0b01.
    p.bytes[8] |= 0x01;
    p.bytes[9] = (p.bytes[9] & 0x3f) | 0x40;
    Ok(p)
}

/// Shift a 71-bit `b71` value left by one bit into a 10-byte payload (bits 0..70),
/// leaving the type bits (71..76) to be set by the caller.
/// provenance: `ft8_lib` `message.c` `ftx_message_encode_telemetry` (MIT).
fn telemetry_payload_from_b71(b71: &[u8; 9]) -> Payload {
    let mut p = Payload::new();
    let mut carry = 0u8;
    for i in (0..9).rev() {
        p.bytes[i] = (b71[i] << 1) | (carry >> 7);
        carry = b71[i] & 0x80;
    }
    p.bytes[9] = 0;
    p
}

/// Recover the 71-bit `b71` value (9 bytes) from a telemetry/free-text payload by
/// shifting right one bit.
/// provenance: `ft8_lib` `message.c` `ftx_message_decode_telemetry` (MIT).
fn b71_from_payload(p: &Payload) -> [u8; 9] {
    let mut b71 = [0u8; 9];
    let mut carry = 0u8;
    for i in 0..9 {
        b71[i] = (carry << 7) | (p.bytes[i] >> 1);
        carry = p.bytes[i] & 0x01;
    }
    b71
}

/// Unpack a payload back into a human-readable message string.
///
/// The `hash` table is consulted (and updated) so hashed callsigns render as
/// `<CALL>` when known and `<...>` when not.
/// provenance: `ft8_lib` `message.c` `ftx_message_decode` (MIT).
pub fn unpack(p: &Payload, hash: &mut HashTable) -> Result<String, PackError> {
    match p.message_type() {
        MessageType::Standard => Ok(unpack_std(p, hash)),
        MessageType::FreeText => Ok(unpack_free_text(p)),
        MessageType::Telemetry => Ok(unpack_telemetry(p)),
        MessageType::Unsupported { .. } => Err(PackError::Unsupported),
    }
}

/// Unpack a STANDARD (`i3=1`) message to `"to de extra"`.
/// provenance: `ft8_lib` `message.c` `ftx_message_decode_std` (MIT).
pub fn unpack_std(p: &Payload, hash: &HashTable) -> String {
    let b = &p.bytes;
    let n29a = ((b[0] as u32) << 21)
        | ((b[1] as u32) << 13)
        | ((b[2] as u32) << 5)
        | ((b[3] as u32) >> 3);
    let n29b = (((b[3] & 0x07) as u32) << 26)
        | ((b[4] as u32) << 18)
        | ((b[5] as u32) << 10)
        | ((b[6] as u32) << 2)
        | ((b[7] as u32) >> 6);
    let ir = (b[7] & 0x20) >> 5;
    let g15 = (((b[7] & 0x1f) as u16) << 10) | ((b[8] as u16) << 2) | ((b[9] as u16) >> 6);
    let i3 = p.i3();

    let call_to = unpack28(n29a >> 1, (n29a & 1) as u8, i3, hash);
    let call_de = unpack28(n29b >> 1, (n29b & 1) as u8, i3, hash);
    let extra = unpackgrid(g15, ir);

    join_fields(&call_to, &call_de, &extra)
}

/// Unpack a FREE TEXT (`i3=0 n3=0`) message (13-char base-42 big integer).
/// provenance: `ft8_lib` `message.c` `ftx_message_decode_free` (MIT).
pub fn unpack_free_text(p: &Payload) -> String {
    let mut b71 = b71_from_payload(p);
    let mut c14 = [0u8; 13];
    for idx in (0..13).rev() {
        // Divide the 71-bit big integer by 42, MSB-first, recovering the digit.
        let mut rem = 0u32;
        for byte in b71.iter_mut() {
            rem = (rem << 8) | *byte as u32;
            *byte = (rem / 42) as u8;
            rem %= 42;
        }
        c14[idx] = charn(rem, T_FULL);
    }
    let s: String = c14.iter().map(|&b| b as char).collect();
    s.trim().to_string()
}

/// Unpack a TELEMETRY (`i3=0 n3=5`) message to its 18-hex-digit string.
/// provenance: `ft8_lib` `message.c` `ftx_message_decode_telemetry_hex` (MIT).
pub fn unpack_telemetry(p: &Payload) -> String {
    let b71 = b71_from_payload(p);
    let mut s = String::with_capacity(18);
    for byte in &b71 {
        s.push_str(&format!("{:02X}", byte));
    }
    s
}

/// Join up to three decoded fields with single spaces, dropping empty trailing ones.
fn join_fields(f1: &str, f2: &str, f3: &str) -> String {
    let mut out = String::from(f1);
    if !f2.is_empty() {
        out.push(' ');
        out.push_str(f2);
        if !f3.is_empty() {
            out.push(' ');
            out.push_str(f3);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Standard-callsign codec KATs ────────────────────────────────────────
    //
    // Values hand-traced from ft8_lib `message.c` `pack28`/`pack_basecall` (MIT)
    // and independently cross-checked. NTOKENS=2063592, MAX22=4194304.

    #[test]
    fn pack28_special_tokens() {
        let mut h = HashTable::new();
        assert_eq!(pack28("DE", &mut h), Some((0, 0)));
        assert_eq!(pack28("QRZ", &mut h), Some((1, 0)));
        assert_eq!(pack28("CQ", &mut h), Some((2, 0)));
    }

    #[test]
    fn pack28_standard_basecall_k1abc() {
        // pack_basecall("K1ABC") = 3957069 (2x1-digit call right-aligned to " K1ABC");
        // n28 = NTOKENS + MAX22 + 3957069.
        let mut h = HashTable::new();
        let (n28, ip) = pack28("K1ABC", &mut h).unwrap();
        assert_eq!(pack_basecall("K1ABC"), Some(3_957_069));
        assert_eq!(n28, 2_063_592 + 4_194_304 + 3_957_069);
        assert_eq!(ip, 0);
    }

    #[test]
    fn pack28_1x1_call_w1a() {
        // W1A: digit in position 1 -> " W1A  "; a 3-char (2x1 short) callsign.
        let mut h = HashTable::new();
        let (n28, ip) = pack28("W1A", &mut h).unwrap();
        assert!(n28 >= 2_063_592 + 4_194_304, "W1A must pack as a standard basecall");
        assert_eq!(ip, 0);
        // round-trips back to itself
        assert_eq!(unpack_basecall(n28 - 2_063_592 - 4_194_304), "W1A");
    }

    #[test]
    fn pack28_suffix_flag_slash_r() {
        // K1ABC/R: ip=1, base value identical to K1ABC.
        let mut h = HashTable::new();
        let (n_plain, _) = pack28("K1ABC", &mut h).unwrap();
        let (n_slashr, ip) = pack28("K1ABC/R", &mut h).unwrap();
        assert_eq!(ip, 1);
        assert_eq!(n_plain, n_slashr);
        // unpack28 with ip=1, i3=1 appends /R
        assert_eq!(unpack28(n_slashr, 1, 1, &h), "K1ABC/R");
    }

    #[test]
    fn callsign_round_trip_charset() {
        // 3-char-suffix standard call.
        let mut h = HashTable::new();
        for call in ["K1ABC", "W9XYZ", "W1A", "AB0XYZ"] {
            let (n28, _) = pack28(call, &mut h).unwrap();
            let n = n28 - 2_063_592 - 4_194_304;
            assert_eq!(unpack_basecall(n), call, "basecall round-trip for {call}");
        }
    }

    // ── Grid / report codec KATs ────────────────────────────────────────────

    #[test]
    fn packgrid_fn42_kat() {
        // FN42 -> 10342 (F=5; 5*18+13=103; 103*10+4=1034; 1034*10+2=10342), ir=0.
        assert_eq!(packgrid("FN42"), (10_342, 0));
        assert_eq!(unpackgrid(10_342, 0), "FN42");
    }

    #[test]
    fn packgrid_report_and_sentinels_kat() {
        // report -12 -> MAXGRID4 + (35-12) = 32400 + 23 = 32423, ir=0.
        assert_eq!(packgrid("-12"), (32_423, 0));
        assert_eq!(unpackgrid(32_423, 0), "-12");
        // R+01 -> ir=1, MAXGRID4 + (35+1) = 32436.
        assert_eq!(packgrid("R+01"), (32_436, 1));
        assert_eq!(unpackgrid(32_436, 1), "R+01");
        // sentinels
        assert_eq!(packgrid("RRR"), (32_402, 0));
        assert_eq!(unpackgrid(32_402, 0), "RRR");
        assert_eq!(packgrid("RR73"), (32_403, 0));
        assert_eq!(unpackgrid(32_403, 0), "RR73");
        assert_eq!(packgrid("73"), (32_404, 0));
        assert_eq!(unpackgrid(32_404, 0), "73");
        // blank
        assert_eq!(packgrid(""), (32_401, 0));
        assert_eq!(unpackgrid(32_401, 0), "");
    }

    // ── Full type-1 message KATs ────────────────────────────────────────────
    //
    // Payload bytes hand-computed from ft8_lib `ftx_message_encode_std`'s exact
    // bit layout (MIT). A wrong bit-order or charset offset fails these.

    #[test]
    fn full_message_cq_k1abc_fn42_kat() {
        let mut h = HashTable::new();
        let p = pack("CQ K1ABC FN42", &mut h).unwrap();
        assert_eq!(
            p.bytes,
            [0x00, 0x00, 0x00, 0x20, 0x4d, 0xef, 0x1a, 0x8a, 0x19, 0x88]
        );
        assert_eq!(p.i3(), 1);
        let mut h2 = HashTable::new();
        assert_eq!(unpack(&p, &mut h2).unwrap(), "CQ K1ABC FN42");
    }

    #[test]
    fn full_message_k1abc_w9xyz_report_kat() {
        let mut h = HashTable::new();
        let p = pack("K1ABC W9XYZ -12", &mut h).unwrap();
        assert_eq!(
            p.bytes,
            [0x09, 0xbd, 0xe3, 0x50, 0x61, 0x49, 0xdc, 0x1f, 0xa9, 0xc8]
        );
        assert_eq!(p.i3(), 1);
        let mut h2 = HashTable::new();
        assert_eq!(unpack(&p, &mut h2).unwrap(), "K1ABC W9XYZ -12");
    }

    // ── Free-text KATs ──────────────────────────────────────────────────────

    #[test]
    fn free_text_kat() {
        // "TNX BOB 73 GL" — 4 tokens, must fall to free text (not standard).
        let p = pack_free_text("TNX BOB 73 GL").unwrap();
        assert_eq!(
            p.bytes,
            [0x63, 0xed, 0xce, 0xe2, 0xa4, 0xae, 0x07, 0xf5, 0x00, 0x00]
        );
        assert_eq!(p.i3(), 0);
        assert_eq!(p.n3(), 0);
        assert_eq!(unpack_free_text(&p), "TNX BOB 73 GL");
    }

    #[test]
    fn free_text_via_pack_fallback() {
        // 4-token message routes through pack() to free text.
        let mut h = HashTable::new();
        let p = pack("TNX BOB 73 GL", &mut h).unwrap();
        assert_eq!(p.message_type(), MessageType::FreeText);
        let mut h2 = HashTable::new();
        assert_eq!(unpack(&p, &mut h2).unwrap(), "TNX BOB 73 GL");
    }

    #[test]
    fn free_text_max_length_13() {
        let p = pack_free_text("1234567890ABC").unwrap();
        assert_eq!(unpack_free_text(&p), "1234567890ABC");
        // 14 chars is too long.
        assert_eq!(pack_free_text("1234567890ABCD"), Err(PackError::FreeText));
    }

    #[test]
    fn free_text_lowercase_and_space_normalization() {
        // lowercase uppercased, runs of spaces collapsed.
        let p = pack_free_text("hello  world").unwrap();
        assert_eq!(unpack_free_text(&p), "HELLO WORLD");
    }

    // ── Telemetry KATs ──────────────────────────────────────────────────────

    #[test]
    fn telemetry_round_trip() {
        // 18 hex digits, top bit clear (leading '1' nibble => 0x12).
        let p = pack_telemetry("123456789ABCDEF012").unwrap();
        assert_eq!(p.message_type(), MessageType::Telemetry);
        assert_eq!(p.i3(), 0);
        assert_eq!(p.n3(), 5);
        assert_eq!(unpack_telemetry(&p), "123456789ABCDEF012");
    }

    #[test]
    fn telemetry_short_input_left_padded() {
        let p = pack_telemetry("AB").unwrap();
        assert_eq!(unpack_telemetry(&p), "0000000000000000AB");
    }

    #[test]
    fn telemetry_rejects_overflow_and_bad_hex() {
        // top nibble 'F' => 0xF0, top bit set => >71 bits.
        assert_eq!(pack_telemetry("F00000000000000000"), Err(PackError::Telemetry));
        assert_eq!(pack_telemetry("XYZ"), Err(PackError::Telemetry));
        assert_eq!(pack_telemetry(""), Err(PackError::Telemetry));
    }

    // ── Hash KATs ───────────────────────────────────────────────────────────

    #[test]
    fn callsign_hash_kat() {
        // Hand-traced against ft8_lib save_callsign() (multiplier 47055833459):
        // K1ABC -> n22=2920267, n12=2851, n10=712.
        assert_eq!(callsign_hash("K1ABC"), Some((2_920_267, 2_851, 712)));
        // PJ4/K1ABC -> n22=1420834, n12=1387, n10=346.
        assert_eq!(callsign_hash("PJ4/K1ABC"), Some((1_420_834, 1_387, 346)));
        // 12/10-bit are truncations of 22-bit.
        let (h22, h12, h10) = callsign_hash("K1ABC").unwrap();
        assert_eq!(h12, h22 >> 10);
        assert_eq!(h10, h22 >> 12);
    }

    #[test]
    fn hash_table_round_trips_known_and_unknown() {
        let (h22, _, _) = callsign_hash("PJ4/K1ABC").unwrap();
        let mut h = HashTable::new();
        h.save("PJ4/K1ABC", h22);
        // known -> <CALL>
        assert_eq!(unpack28(codec_consts::NTOKENS + h22, 0, 1, &h), "<PJ4/K1ABC>");
        // unknown 22-bit hash -> <...>
        let empty = HashTable::new();
        assert_eq!(
            unpack28(codec_consts::NTOKENS + h22, 0, 1, &empty),
            "<...>"
        );
    }

    // ── Round-trip tables ───────────────────────────────────────────────────

    #[test]
    fn round_trip_message_table() {
        let table = [
            "CQ K1ABC FN42",
            "K1ABC W9XYZ -12",
            "K1ABC W9XYZ RRR",
            "K1ABC W9XYZ RR73",
            "K1ABC W9XYZ 73",
            "CQ W1A DM43",
            "K1ABC W9XYZ",
            "TNX BOB 73 GL",
        ];
        for m in table {
            let mut hp = HashTable::new();
            let p = pack(m, &mut hp).unwrap();
            let mut hu = HashTable::new();
            let back = unpack(&p, &mut hu).unwrap();
            assert_eq!(back, m, "unpack(pack({m:?})) mismatch");
            // pack(unpack(x)) == x at the payload level.
            let mut hp2 = HashTable::new();
            let p2 = pack(&back, &mut hp2).unwrap();
            assert_eq!(p2.bytes, p.bytes, "pack(unpack) payload mismatch for {m:?}");
        }
    }

    #[test]
    fn message_type_classification() {
        let mut h = HashTable::new();
        assert_eq!(
            pack("CQ K1ABC FN42", &mut h).unwrap().message_type(),
            MessageType::Standard
        );
        assert_eq!(
            pack("TNX BOB 73 GL", &mut h).unwrap().message_type(),
            MessageType::FreeText
        );
        assert_eq!(
            pack_telemetry("123456789ABCDEF012").unwrap().message_type(),
            MessageType::Telemetry
        );
    }
}
