//! Frontend-facing DTOs for the UV-Pro control profile (tuxlink-nx95).
//!
//! These serialize to camelCase JSON for the React frontend. Note that enum
//! variants do NOT inherit a struct's `rename_all`, so each enum carries its own
//! `#[serde(rename_all = ...)]` (a recurring Codex catch on this codebase).

use serde::{Deserialize, Serialize};

use super::message::{DecodedDevInfo, DecodedStatus};
use super::rf_ch::{Bandwidth, Modulation, RfCh};

/// Connection lifecycle state, distinct from a bare bool so the UI can show a
/// "connecting" spinner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ConnState {
    #[default]
    Disconnected,
    Connecting,
    Connected,
}

/// A snapshot of the live control state, broadcast on the `uvpro:status` event
/// and returned by the connect / get-status commands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UvproStatus {
    pub state: ConnState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firmware: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_channel_id: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rx_mhz: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_mhz: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<Modulation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bandwidth: Option<Bandwidth>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_name: Option<String>,
    pub is_tx: bool,
    pub is_rx: bool,
    pub squelch_open: bool,
    pub power_on: bool,
    pub gps_locked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rssi: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery_percent: Option<u8>,
    /// Set when not connected because the KISS/packet path holds the radio.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_busy_holder: Option<String>,
}

impl UvproStatus {
    /// Fold a decoded radio status into the snapshot (preserving fields the status
    /// frame doesn't carry, e.g. battery, model).
    pub fn apply_status(&mut self, s: &DecodedStatus) {
        self.is_tx = s.is_in_tx;
        self.is_rx = s.is_in_rx;
        self.squelch_open = s.is_sq;
        self.power_on = s.is_power_on;
        self.gps_locked = s.is_gps_locked;
        self.current_channel_id = Some(s.curr_channel_id);
        if s.rssi.is_some() {
            self.rssi = s.rssi;
        }
    }

    /// Fold the active channel's freq/mode/name into the snapshot.
    pub fn apply_channel(&mut self, ch: &RfCh) {
        self.current_channel_id = Some(ch.channel_id as u16);
        self.rx_mhz = Some(ch.rx_freq_hz as f64 / 1e6);
        self.tx_mhz = Some(ch.tx_freq_hz as f64 / 1e6);
        self.mode = Some(ch.rx_mod);
        self.bandwidth = Some(ch.bandwidth);
        self.channel_name = Some(ch.name_str());
    }

    pub fn apply_dev_info(&mut self, info: &DecodedDevInfo) {
        self.device_model = Some(format!("0x{:04x}", info.product_id));
        self.firmware = Some(format!("{}.{}", info.soft_ver >> 8, info.soft_ver & 0xff));
    }
}

/// A channel-memory entry for the channel list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UvproChannel {
    pub channel_id: u32,
    pub name: String,
    pub rx_mhz: f64,
    pub tx_mhz: f64,
    pub mode: Modulation,
    pub bandwidth: Bandwidth,
    pub tx_disable: bool,
}

impl UvproChannel {
    pub fn from_rfch(ch: &RfCh) -> Self {
        UvproChannel {
            channel_id: ch.channel_id as u32,
            name: ch.name_str(),
            rx_mhz: ch.rx_freq_hz as f64 / 1e6,
            tx_mhz: ch.tx_freq_hz as f64 / 1e6,
            mode: ch.rx_mod,
            bandwidth: ch.bandwidth,
            tx_disable: ch.tx_disable,
        }
    }
}

/// Device identity (returned at connect, mostly informational).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UvproDeviceInfo {
    pub model: String,
    pub firmware: String,
    pub channel_count: u32,
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
    fn status_serializes_camelcase_with_state_enum() {
        let mut s = UvproStatus {
            state: ConnState::Connected,
            ..Default::default()
        };
        s.rx_mhz = Some(146.52);
        s.mode = Some(Modulation::Fm);
        let j = serde_json::to_string(&s).unwrap();
        assert!(j.contains("\"state\":\"connected\""), "{j}");
        assert!(j.contains("\"rxMhz\":146.52"), "{j}");
        assert!(j.contains("\"mode\":\"fm\""), "{j}");
    }

    #[test]
    fn disconnected_status_omits_absent_fields() {
        let s = UvproStatus::default();
        let j = serde_json::to_string(&s).unwrap();
        assert!(j.contains("\"state\":\"disconnected\""), "{j}");
        assert!(!j.contains("rxMhz"), "{j}");
        assert!(!j.contains("rssi"), "{j}");
    }

    #[test]
    fn channel_from_rfch_maps_freq_to_mhz() {
        let ch = UvproChannel::from_rfch(
            &RfCh::decode(&hex(
                "00 08 bb b7 c0 08 bb b7 c0 00 00 00 00 50 00 43 41 4c 4c 00 00 00 00 00 00",
            ))
            .unwrap(),
        );
        assert_eq!(ch.rx_mhz, 146.52);
        assert_eq!(ch.mode, Modulation::Fm);
        assert_eq!(ch.bandwidth, Bandwidth::Wide);
        assert_eq!(ch.name, "CALL");
    }
}
