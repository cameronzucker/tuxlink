//! Active-channel selection via the Benshi `Settings` block (tuxlink-nx95).
//!
//! `Radio.cs` (HTCommander) shows the active channel is `Settings.channel_a`
//! (VFO A) / `channel_b` (VFO B); switching it is a `WRITE_SETTINGS` with the new
//! value, NOT a `WRITE_RF_CH`. `Settings` is a 22-byte block of ~50 packed fields;
//! rather than decode every field (and risk corrupting one on re-encode), we keep
//! the raw bytes from `READ_SETTINGS` and patch ONLY the channel nibbles in place,
//! then write the whole block back — preserving every other setting exactly.
//!
//! Nibble offsets pinned by a diff-of-encodings golden test (benlink):
//!   channel_a = byte 0 high nibble (lower 4b) + byte 9 high nibble (upper 4b)
//!   channel_b = byte 0 low  nibble (lower 4b) + byte 9 low  nibble (upper 4b)

pub const SETTINGS_LEN: usize = 22;
const LOWER_BYTE: usize = 0;
const UPPER_BYTE: usize = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vfo {
    A,
    B,
}

/// Patch the active channel for `vfo` into a copy of the raw 22-byte `Settings`
/// block, leaving every other field untouched. Returns `None` if `raw` is not the
/// expected length. `channel_id` is split into a low nibble (byte 0) and high
/// nibble (byte 9); ids 0..=255 are representable.
pub fn patch_channel(raw: &[u8], vfo: Vfo, channel_id: u8) -> Option<Vec<u8>> {
    if raw.len() != SETTINGS_LEN {
        return None;
    }
    let mut out = raw.to_vec();
    let lower = channel_id & 0x0F;
    let upper = (channel_id >> 4) & 0x0F;
    match vfo {
        Vfo::A => {
            out[LOWER_BYTE] = (out[LOWER_BYTE] & 0x0F) | (lower << 4);
            out[UPPER_BYTE] = (out[UPPER_BYTE] & 0x0F) | (upper << 4);
        }
        Vfo::B => {
            out[LOWER_BYTE] = (out[LOWER_BYTE] & 0xF0) | lower;
            out[UPPER_BYTE] = (out[UPPER_BYTE] & 0xF0) | upper;
        }
    }
    Some(out)
}

/// Read back the active channel id for `vfo` from a raw `Settings` block.
pub fn channel_of(raw: &[u8], vfo: Vfo) -> Option<u8> {
    if raw.len() != SETTINGS_LEN {
        return None;
    }
    let (lower, upper) = match vfo {
        Vfo::A => ((raw[LOWER_BYTE] >> 4) & 0x0F, (raw[UPPER_BYTE] >> 4) & 0x0F),
        Vfo::B => (raw[LOWER_BYTE] & 0x0F, raw[UPPER_BYTE] & 0x0F),
    };
    Some((upper << 4) | lower)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace()
            .map(|h| u8::from_str_radix(h, 16).unwrap())
            .collect()
    }

    // sample Settings with channel_a=(upper 0, lower 1) → byte0 high nibble 1,
    // channel_b=(0,2) → byte0 low nibble 2; uppers 0 at byte 9.
    const SAMPLE: &str = "12 13 94 0a 51 60 04 02 28 00 00 00 04 00 00 00 00 00 00 00 00 00";

    #[test]
    fn reads_back_initial_channels() {
        let raw = hex(SAMPLE);
        assert_eq!(channel_of(&raw, Vfo::A), Some(1));
        assert_eq!(channel_of(&raw, Vfo::B), Some(2));
    }

    #[test]
    fn patch_channel_a_round_trips() {
        let raw = hex(SAMPLE);
        let out = patch_channel(&raw, Vfo::A, 200).unwrap();
        assert_eq!(out.len(), SETTINGS_LEN);
        assert_eq!(channel_of(&out, Vfo::A), Some(200));
        // channel_b unchanged
        assert_eq!(channel_of(&out, Vfo::B), Some(2));
    }

    #[test]
    fn patch_changes_only_the_two_nibble_bytes() {
        let raw = hex(SAMPLE);
        let out = patch_channel(&raw, Vfo::A, 200).unwrap();
        for i in 0..SETTINGS_LEN {
            if i == LOWER_BYTE || i == UPPER_BYTE {
                continue;
            }
            assert_eq!(out[i], raw[i], "byte {i} changed unexpectedly");
        }
    }

    #[test]
    fn patch_b_preserves_a() {
        let raw = hex(SAMPLE);
        let out = patch_channel(&raw, Vfo::B, 15).unwrap();
        assert_eq!(channel_of(&out, Vfo::A), Some(1));
        assert_eq!(channel_of(&out, Vfo::B), Some(15));
    }

    #[test]
    fn rejects_wrong_length() {
        assert!(patch_channel(&[0u8; 10], Vfo::A, 1).is_none());
    }
}
