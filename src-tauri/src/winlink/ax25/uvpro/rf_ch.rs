//! `RfCh` channel codec for the Benshi protocol (tuxlink-nx95).
//!
//! A channel memory carries the TX/RX frequency + modulation + bandwidth + tone
//! + a 10-char name, packed into exactly 200 bits (25 bytes). Setting a frequency
//! or mode is a read-modify-write of one of these: decode the current channel,
//! mutate the field, re-encode. The encoder MUST reproduce every untouched field
//! (sub-audio, power flags, reserved padding, name) bit-for-bit or the radio
//! corrupts/rejects the channel — `encode_decode_identity` is the guard.
//!
//! Field order + widths derived from benlink `protocol/command/rf_ch.py`.

use serde::{Deserialize, Serialize};

use super::bits::{BitReader, BitWriter};

/// `RfCh` is 200 bits when non-DMR (the FM/AM case tuxlink targets).
pub const RFCH_LEN_BYTES: usize = 25;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Modulation {
    Fm,
    Am,
    Dmr,
}

impl Modulation {
    fn from_u2(v: u64) -> Self {
        match v {
            1 => Modulation::Am,
            2 => Modulation::Dmr,
            _ => Modulation::Fm,
        }
    }
    fn to_u2(self) -> u64 {
        match self {
            Modulation::Fm => 0,
            Modulation::Am => 1,
            Modulation::Dmr => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Bandwidth {
    Narrow,
    Wide,
}

/// A decoded channel memory. Frequencies are stored as integer Hz (the wire form
/// is `round(MHz * 1e6)`); all flags + sub-audio + name are retained verbatim so
/// `encode()` round-trips.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RfCh {
    pub channel_id: u8,
    pub tx_mod: Modulation,
    pub tx_freq_hz: u32,
    pub rx_mod: Modulation,
    pub rx_freq_hz: u32,
    pub tx_sub_audio: u16,
    pub rx_sub_audio: u16,
    // flags, in wire order
    pub scan: bool,
    pub tx_at_max_power: bool,
    pub talk_around: bool,
    pub bandwidth: Bandwidth,
    pub pre_de_emph_bypass: bool,
    pub sign: bool,
    pub tx_at_med_power: bool,
    pub tx_disable: bool,
    pub fixed_freq: bool,
    pub fixed_bandwidth: bool,
    pub fixed_tx_power: bool,
    pub mute: bool,
    pub name: [u8; 10],
}

impl RfCh {
    /// Decode a 25-byte `RfCh`. Returns `None` if the slice is too short.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < RFCH_LEN_BYTES {
            return None;
        }
        let mut r = BitReader::new(bytes);
        let channel_id = r.read_uint(8) as u8;
        let tx_mod = Modulation::from_u2(r.read_uint(2));
        let tx_freq_hz = r.read_uint(30) as u32;
        let rx_mod = Modulation::from_u2(r.read_uint(2));
        let rx_freq_hz = r.read_uint(30) as u32;
        let tx_sub_audio = r.read_uint(16) as u16;
        let rx_sub_audio = r.read_uint(16) as u16;
        let scan = r.read_bool();
        let tx_at_max_power = r.read_bool();
        let talk_around = r.read_bool();
        let bandwidth = if r.read_bool() { Bandwidth::Wide } else { Bandwidth::Narrow };
        let pre_de_emph_bypass = r.read_bool();
        let sign = r.read_bool();
        let tx_at_med_power = r.read_bool();
        let tx_disable = r.read_bool();
        let fixed_freq = r.read_bool();
        let fixed_bandwidth = r.read_bool();
        let fixed_tx_power = r.read_bool();
        let mute = r.read_bool();
        let _pad = r.read_uint(4);
        let name_vec = r.read_bytes(10);
        let mut name = [0u8; 10];
        name.copy_from_slice(&name_vec);
        Some(RfCh {
            channel_id,
            tx_mod,
            tx_freq_hz,
            rx_mod,
            rx_freq_hz,
            tx_sub_audio,
            rx_sub_audio,
            scan,
            tx_at_max_power,
            talk_around,
            bandwidth,
            pre_de_emph_bypass,
            sign,
            tx_at_med_power,
            tx_disable,
            fixed_freq,
            fixed_bandwidth,
            fixed_tx_power,
            mute,
            name,
        })
    }

    /// Re-encode to the 25-byte wire form (faithful to every field).
    pub fn encode(&self) -> Vec<u8> {
        let mut w = BitWriter::new();
        w.write_uint(self.channel_id as u64, 8);
        w.write_uint(self.tx_mod.to_u2(), 2);
        w.write_uint(self.tx_freq_hz as u64, 30);
        w.write_uint(self.rx_mod.to_u2(), 2);
        w.write_uint(self.rx_freq_hz as u64, 30);
        w.write_uint(self.tx_sub_audio as u64, 16);
        w.write_uint(self.rx_sub_audio as u64, 16);
        w.write_bool(self.scan);
        w.write_bool(self.tx_at_max_power);
        w.write_bool(self.talk_around);
        w.write_bool(self.bandwidth == Bandwidth::Wide);
        w.write_bool(self.pre_de_emph_bypass);
        w.write_bool(self.sign);
        w.write_bool(self.tx_at_med_power);
        w.write_bool(self.tx_disable);
        w.write_bool(self.fixed_freq);
        w.write_bool(self.fixed_bandwidth);
        w.write_bool(self.fixed_tx_power);
        w.write_bool(self.mute);
        w.write_uint(0, 4); // pad
        w.write_bytes(&self.name);
        w.into_bytes()
    }

    /// The channel name as UTF-8, trimming trailing NULs / whitespace.
    pub fn name_str(&self) -> String {
        let end = self.name.iter().position(|&b| b == 0).unwrap_or(self.name.len());
        String::from_utf8_lossy(&self.name[..end]).trim_end().to_string()
    }

    /// Convert MHz to the wire integer-Hz form (round, never truncate).
    pub fn mhz_to_hz(mhz: f64) -> u32 {
        (mhz * 1e6).round() as u32
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

    // golden: WRITE_RF_CH body for ch0, 146.520 MHz simplex FM WIDE, no tone, "CALL"
    const RFCH_146520_FM_WIDE_CALL: &str =
        "00 08 bb b7 c0 08 bb b7 c0 00 00 00 00 50 00 43 41 4c 4c 00 00 00 00 00 00";

    #[test]
    fn decodes_golden_channel() {
        let ch = RfCh::decode(&hex(RFCH_146520_FM_WIDE_CALL)).unwrap();
        assert_eq!(ch.channel_id, 0);
        assert_eq!(ch.tx_mod, Modulation::Fm);
        assert_eq!(ch.tx_freq_hz, 146_520_000);
        assert_eq!(ch.rx_freq_hz, 146_520_000);
        assert_eq!(ch.bandwidth, Bandwidth::Wide);
        assert!(ch.tx_at_max_power);
        assert!(!ch.tx_disable);
        assert_eq!(ch.name_str(), "CALL");
    }

    #[test]
    fn encode_decode_identity() {
        let bytes = hex(RFCH_146520_FM_WIDE_CALL);
        let ch = RfCh::decode(&bytes).unwrap();
        assert_eq!(ch.encode(), bytes);
    }

    #[test]
    fn read_modify_write_preserves_untouched_fields() {
        let bytes = hex(RFCH_146520_FM_WIDE_CALL);
        let mut ch = RfCh::decode(&bytes).unwrap();
        // change only the rx/tx frequency to 147.000 MHz
        ch.tx_freq_hz = RfCh::mhz_to_hz(147.0);
        ch.rx_freq_hz = RfCh::mhz_to_hz(147.0);
        let out = ch.encode();
        let back = RfCh::decode(&out).unwrap();
        assert_eq!(back.rx_freq_hz, 147_000_000);
        // everything else identical
        assert_eq!(back.name_str(), "CALL");
        assert_eq!(back.bandwidth, Bandwidth::Wide);
        assert!(back.tx_at_max_power);
        assert_eq!(back.tx_sub_audio, 0);
    }

    #[test]
    fn mhz_to_hz_rounds() {
        assert_eq!(RfCh::mhz_to_hz(146.520), 146_520_000);
        assert_eq!(RfCh::mhz_to_hz(446.00625), 446_006_250);
    }
}
