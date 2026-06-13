//! `c1` audio keying opcodes over the GAIA control channel (tuxlink-bcsy).
//!
//! The decompiled vendor app keys/de-keys the audio path with a small `c1` enum
//! (`v4/c1.java`) sent via `W0(c1, byte[])` → `K(3, c1.ordinal(), data)`
//! (`v4/l1.java:630`): GAIA **command_group 3**, command = the `c1` ordinal, empty
//! body. That is a different group from the basic control commands in `message.rs`
//! (group 2, the `v` enum). The byte layout is otherwise identical:
//! `command_group:u16 + is_reply:1bit + command:15bit + body`, big-endian.
//!
//! KEYING DEFAULT IS IMPLICIT — benlink's working send POC sends NONE of these
//! (opening the audio channel + streaming `AudioData` keys TX; `AudioEnd` de-keys).
//! These opcodes are wired only when [`super::transport::KeyingMode::Explicit`] is
//! selected, which is gated on the operator HCI snoop confirming the app keys via
//! GAIA. Built now so flipping to Explicit is a one-line change, not new RE.

use super::super::gaia::gaia_wrap;

/// GAIA command group carrying the `c1` audio opcodes (decompile `K(3, ...)`).
const GROUP_AUDIO: u16 = 3;

/// The `c1` audio-control opcodes. Discriminants are the Java enum ordinals
/// (`UNKNOWN=0` is intentionally absent — it is never sent).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioKey {
    TxAudio = 1,
    TxAudioStop = 2,
    RxAudio = 3,
    RxAudioStop = 4,
}

impl AudioKey {
    /// Raw `Message` bytes (no GAIA wrap): `group:u16` + `(is_reply=0 | command:15):u16`,
    /// big-endian, empty body. Since the opcodes are 1..=4 (< 0x8000) and requests
    /// are never replies, the second `u16` is just the command value.
    pub fn to_message(self) -> Vec<u8> {
        let command = self as u16; // is_reply = 0 (MSB clear); command in low 15 bits
        let mut out = Vec::with_capacity(4);
        out.extend_from_slice(&GROUP_AUDIO.to_be_bytes());
        out.extend_from_slice(&command.to_be_bytes());
        out
    }

    /// GAIA-wrapped bytes ready to write on the control channel.
    pub fn to_gaia(self) -> Vec<u8> {
        gaia_wrap(&self.to_message())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace()
            .map(|h| u8::from_str_radix(h, 16).unwrap())
            .collect()
    }

    #[test]
    fn opcodes_encode_to_group_3_messages() {
        assert_eq!(AudioKey::TxAudio.to_message(), hex("00 03 00 01"));
        assert_eq!(AudioKey::TxAudioStop.to_message(), hex("00 03 00 02"));
        assert_eq!(AudioKey::RxAudio.to_message(), hex("00 03 00 03"));
        assert_eq!(AudioKey::RxAudioStop.to_message(), hex("00 03 00 04"));
    }

    #[test]
    fn gaia_wrap_matches_golden() {
        // gaia_wrap: ff 01 00 <n = msg.len()-4 = 0> <msg>
        assert_eq!(AudioKey::TxAudio.to_gaia(), hex("ff 01 00 00 00 03 00 01"));
        assert_eq!(AudioKey::TxAudioStop.to_gaia(), hex("ff 01 00 00 00 03 00 02"));
    }
}
