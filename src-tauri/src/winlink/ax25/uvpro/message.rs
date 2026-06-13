//! Benshi `Message` codec (tuxlink-nx95): the request encoders the session sends
//! and the reply/event decoder for inbound frames.
//!
//! Header: `command_group:u16 (BASIC=2) + is_reply:1bit + command:u15 + body`,
//! big-endian. `is_reply` is the MSB of byte 2. Decode routes by `(is_reply,
//! command)`; unknown ids decode to `Frame::Unknown` (never an error) because the
//! radio also pushes events we don't model (e.g. `DATA_RXD`, auto-enabled
//! alongside `HT_STATUS_CHANGED`). All encoders + decoders are pinned to golden
//! vectors derived from benlink (`docs/design/uvpro-benshi-golden-vectors.md`).

use super::bits::{BitReader, BitWriter};
use super::rf_ch::RfCh;

const GROUP_BASIC: u64 = 2;

// BasicCommand ids (benlink protocol/command/message.py).
const CMD_GET_DEV_INFO: u64 = 4;
const CMD_READ_STATUS: u64 = 5;
const CMD_REGISTER_NOTIFICATION: u64 = 6;
const CMD_EVENT_NOTIFICATION: u64 = 9;
const CMD_READ_SETTINGS: u64 = 10;
const CMD_WRITE_SETTINGS: u64 = 11;
const CMD_READ_RF_CH: u64 = 13;
const CMD_WRITE_RF_CH: u64 = 14;
const CMD_GET_HT_STATUS: u64 = 20;
const CMD_HT_SEND_DATA: u64 = 31;

/// Events the radio pushes after `REGISTER_NOTIFICATION`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    HtStatusChanged = 1,
    DataRxd = 2,
    HtChChanged = 5,
    HtSettingsChanged = 6,
}

/// `PowerStatusType` selector for `READ_STATUS`.
const POWER_BATTERY_LEVEL_AS_PERCENTAGE: u64 = 4;

fn header(command: u64, is_reply: bool) -> BitWriter {
    let mut w = BitWriter::new();
    w.write_uint(GROUP_BASIC, 16);
    w.write_bool(is_reply);
    w.write_uint(command, 15);
    w
}

// ---- request encoders (host → radio) ----

pub fn encode_get_dev_info() -> Vec<u8> {
    let mut w = header(CMD_GET_DEV_INFO, false);
    w.write_uint(3, 8); // GetDevInfoBody is the literal byte 0x03
    w.into_bytes()
}

pub fn encode_read_rf_ch(channel_id: u8) -> Vec<u8> {
    let mut w = header(CMD_READ_RF_CH, false);
    w.write_uint(channel_id as u64, 8);
    w.into_bytes()
}

pub fn encode_read_battery_pct() -> Vec<u8> {
    let mut w = header(CMD_READ_STATUS, false);
    w.write_uint(POWER_BATTERY_LEVEL_AS_PERCENTAGE, 16);
    w.into_bytes()
}

pub fn encode_register_notification(ev: EventType) -> Vec<u8> {
    let mut w = header(CMD_REGISTER_NOTIFICATION, false);
    w.write_uint(ev as u64, 8);
    w.into_bytes()
}

pub fn encode_get_ht_status() -> Vec<u8> {
    header(CMD_GET_HT_STATUS, false).into_bytes()
}

pub fn encode_read_settings() -> Vec<u8> {
    header(CMD_READ_SETTINGS, false).into_bytes()
}

pub fn encode_write_rf_ch(ch: &RfCh) -> Vec<u8> {
    let mut w = header(CMD_WRITE_RF_CH, false);
    w.write_bytes(&ch.encode());
    w.into_bytes()
}

pub fn encode_write_settings(settings_raw: &[u8]) -> Vec<u8> {
    let mut w = header(CMD_WRITE_SETTINGS, false);
    w.write_bytes(settings_raw);
    w.into_bytes()
}

/// Encode an `HT_SEND_DATA` request carrying one APRS/AX.25 TNC fragment — the
/// native data path that rides the same GAIA connection as control (tuxlink-7my9).
// TODO(tuxlink-7my9): drop the allow when Task 7/8 gives send_aprs_frame a live
// caller; until then this TX encoder is reached only from dead code + tests.
#[allow(dead_code)]
pub fn encode_ht_send_data(frag: &super::tncdata::TncDataFragment) -> Vec<u8> {
    let mut w = header(CMD_HT_SEND_DATA, false);
    w.write_bytes(&frag.encode_body());
    w.into_bytes()
}

// ---- decode (radio → host) ----

/// Live radio status (from `GET_HT_STATUS` reply or `HT_STATUS_CHANGED` event).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DecodedStatus {
    pub is_power_on: bool,
    pub is_in_tx: bool,
    pub is_sq: bool,
    pub is_in_rx: bool,
    pub is_scan: bool,
    pub is_radio: bool,
    pub curr_channel_id: u16,
    pub is_gps_locked: bool,
    /// 0..100, present only on the extended status (older firmware omits it).
    pub rssi: Option<u8>,
}

/// Decoded device info (we keep the fields the control UI uses).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DecodedDevInfo {
    pub product_id: u16,
    pub soft_ver: u16,
    pub channel_count: u8,
    pub support_vfo: bool,
    pub support_dmr: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    ChannelChanged { channel: RfCh },
    StatusChanged { status: DecodedStatus },
    /// Inbound APRS/AX.25 data fragment (`DATA_RXD`) — the native data path.
    /// The session reassembles these into whole frames (tuxlink-7my9).
    DataReceived { fragment: super::tncdata::TncDataFragment },
    /// Any event we deliberately ignore.
    OtherIgnored { event_type: u8 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    StatusReply { reply_status: u8, status: Option<DecodedStatus> },
    ChannelReply { reply_status: u8, channel: Option<RfCh> },
    WriteRfChReply { reply_status: u8, channel_id: u8 },
    BatteryReply { reply_status: u8, value: u8 },
    DevInfoReply { reply_status: u8, info: Option<DecodedDevInfo> },
    SettingsReply { reply_status: u8, settings_raw: Vec<u8> },
    WriteSettingsReply { reply_status: u8 },
    SendDataReply { reply_status: u8 },
    Event(Event),
    Unknown { command: u16, is_reply: bool },
}

fn decode_status(r: &mut BitReader, ext: bool) -> DecodedStatus {
    let is_power_on = r.read_bool();
    let is_in_tx = r.read_bool();
    let is_sq = r.read_bool();
    let is_in_rx = r.read_bool();
    let _double_channel = r.read_uint(2);
    let is_scan = r.read_bool();
    let is_radio = r.read_bool();
    let curr_ch_lower = r.read_uint(4) as u16;
    let is_gps_locked = r.read_bool();
    let _is_hfp_connected = r.read_bool();
    let _is_aoc_connected = r.read_bool();
    let _unknown = r.read_uint(1);
    let (rssi, curr_ch_upper) = if ext {
        let raw = r.read_uint(4) as f64;
        let _curr_region = r.read_uint(6);
        let upper = r.read_uint(4) as u16;
        let _pad = r.read_uint(2);
        (Some((raw * 100.0 / 15.0).round() as u8), upper)
    } else {
        (None, 0)
    };
    DecodedStatus {
        is_power_on,
        is_in_tx,
        is_sq,
        is_in_rx,
        is_scan,
        is_radio,
        curr_channel_id: (curr_ch_upper << 4) | curr_ch_lower,
        is_gps_locked,
        rssi,
    }
}

fn decode_dev_info(body: &[u8]) -> Option<DecodedDevInfo> {
    // body = DevInfo (after the reply_status byte). Need ≥ 9 bytes to reach
    // channel_count + freq_range_count.
    if body.len() < 9 {
        return None;
    }
    let mut r = BitReader::new(body);
    let _vendor_id = r.read_uint(8);
    let product_id = r.read_uint(16) as u16;
    let _hw_ver = r.read_uint(8);
    let soft_ver = r.read_uint(16) as u16;
    // 6 capability bools
    for _ in 0..6 {
        r.read_bool();
    }
    let _region_count = r.read_uint(6);
    let _support_noaa = r.read_bool();
    let _gmrs = r.read_bool();
    let support_vfo = r.read_bool();
    let support_dmr = r.read_bool();
    let channel_count = r.read_uint(8) as u8;
    Some(DecodedDevInfo {
        product_id,
        soft_ver,
        channel_count,
        support_vfo,
        support_dmr,
    })
}

/// Decode one full `Message` (the `data` of a GAIA frame) into a `Frame`.
pub fn decode_frame(bytes: &[u8]) -> Frame {
    if bytes.len() < 4 {
        return Frame::Unknown { command: 0, is_reply: false };
    }
    let mut r = BitReader::new(bytes);
    let _group = r.read_uint(16);
    let is_reply = r.read_bool();
    let command = r.read_uint(15);
    let body = &bytes[4..];

    match (is_reply, command) {
        (true, CMD_GET_HT_STATUS) => {
            if body.is_empty() {
                return Frame::StatusReply { reply_status: 0xff, status: None };
            }
            let reply_status = body[0];
            let status = if reply_status == 0 {
                let mut br = BitReader::new(&body[1..]);
                let ext = body.len() > 4; // StatusExt is 4 bytes, Status is 2 (+1 reply_status)
                Some(decode_status(&mut br, ext))
            } else {
                None
            };
            Frame::StatusReply { reply_status, status }
        }
        (false, CMD_EVENT_NOTIFICATION) => {
            if body.is_empty() {
                return Frame::Event(Event::OtherIgnored { event_type: 0 });
            }
            let event_type = body[0];
            match event_type {
                x if x == EventType::HtChChanged as u8 => match RfCh::decode(&body[1..]) {
                    Some(channel) => Frame::Event(Event::ChannelChanged { channel }),
                    None => Frame::Event(Event::OtherIgnored { event_type }),
                },
                x if x == EventType::HtStatusChanged as u8 => {
                    let mut br = BitReader::new(&body[1..]);
                    let ext = body.len() > 4; // event_type(1) + StatusExt(4) vs Status(2)
                    Frame::Event(Event::StatusChanged { status: decode_status(&mut br, ext) })
                }
                x if x == EventType::DataRxd as u8 => {
                    match super::tncdata::TncDataFragment::decode_body(&body[1..]) {
                        Some(fragment) => Frame::Event(Event::DataReceived { fragment }),
                        None => Frame::Event(Event::OtherIgnored { event_type }),
                    }
                }
                _ => Frame::Event(Event::OtherIgnored { event_type }),
            }
        }
        (true, CMD_READ_STATUS) => {
            // reply_status(8) + power_status_type(16) + value(8 for level/pct)
            if body.len() < 4 {
                return Frame::BatteryReply { reply_status: body.first().copied().unwrap_or(0xff), value: 0 };
            }
            Frame::BatteryReply { reply_status: body[0], value: body[3] }
        }
        (true, CMD_READ_RF_CH) => {
            let reply_status = body.first().copied().unwrap_or(0xff);
            let channel = if reply_status == 0 { RfCh::decode(&body[1..]) } else { None };
            Frame::ChannelReply { reply_status, channel }
        }
        (true, CMD_WRITE_RF_CH) => Frame::WriteRfChReply {
            reply_status: body.first().copied().unwrap_or(0xff),
            channel_id: body.get(1).copied().unwrap_or(0),
        },
        (true, CMD_GET_DEV_INFO) => {
            let reply_status = body.first().copied().unwrap_or(0xff);
            let info = if reply_status == 0 { decode_dev_info(&body[1..]) } else { None };
            Frame::DevInfoReply { reply_status, info }
        }
        (true, CMD_READ_SETTINGS) => {
            let reply_status = body.first().copied().unwrap_or(0xff);
            let settings_raw = if reply_status == 0 && body.len() > 1 { body[1..].to_vec() } else { Vec::new() };
            Frame::SettingsReply { reply_status, settings_raw }
        }
        (true, CMD_WRITE_SETTINGS) => Frame::WriteSettingsReply {
            reply_status: body.first().copied().unwrap_or(0xff),
        },
        (true, CMD_HT_SEND_DATA) => Frame::SendDataReply {
            reply_status: body.first().copied().unwrap_or(0xff),
        },
        _ => Frame::Unknown { command: command as u16, is_reply },
    }
}

#[cfg(test)]
mod tests {
    use super::super::rf_ch::Modulation;
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace()
            .map(|h| u8::from_str_radix(h, 16).unwrap())
            .collect()
    }

    #[test]
    fn encodes_request_headers_and_bodies() {
        assert_eq!(encode_get_ht_status(), hex("00 02 00 14"));
        assert_eq!(encode_read_rf_ch(0), hex("00 02 00 0d 00"));
        assert_eq!(encode_read_battery_pct(), hex("00 02 00 05 00 04"));
        assert_eq!(
            encode_register_notification(EventType::HtStatusChanged),
            hex("00 02 00 06 01")
        );
        assert_eq!(encode_get_dev_info(), hex("00 02 00 04 03"));
    }

    #[test]
    fn decodes_status_reply_with_rssi() {
        match decode_frame(&hex("00 02 80 14 00 b4 3c c0 00")) {
            Frame::StatusReply { reply_status: 0, status: Some(s) } => {
                assert!(!s.is_in_tx);
                assert!(s.is_in_rx);
                assert!(s.is_sq);
                assert_eq!(s.curr_channel_id, 3);
                assert_eq!(s.rssi, Some(80));
                assert!(s.is_gps_locked);
            }
            f => panic!("wrong frame: {f:?}"),
        }
    }

    #[test]
    fn decodes_ch_changed_event() {
        let bytes = hex("00 02 00 09 05 05 1a 95 6b 80 1a 95 6b 80 00 00 00 00 40 00 55 48 46 00 00 00 00 00 00 00");
        match decode_frame(&bytes) {
            Frame::Event(Event::ChannelChanged { channel }) => {
                assert_eq!(channel.channel_id, 5);
                assert_eq!(channel.tx_mod, Modulation::Fm);
                assert_eq!(channel.name_str(), "UHF");
                assert_eq!(channel.rx_freq_hz, 446_000_000);
            }
            f => panic!("wrong frame: {f:?}"),
        }
    }

    #[test]
    fn decodes_battery_reply() {
        match decode_frame(&hex("00 02 80 05 00 00 04 49")) {
            Frame::BatteryReply { reply_status: 0, value } => assert_eq!(value, 73),
            f => panic!("wrong: {f:?}"),
        }
    }

    #[test]
    fn decodes_write_rf_ch_reply_ok() {
        assert!(matches!(
            decode_frame(&hex("00 02 80 0e 00 00")),
            Frame::WriteRfChReply { reply_status: 0, channel_id: 0 }
        ));
    }

    #[test]
    fn unknown_command_is_not_an_error() {
        assert!(matches!(
            decode_frame(&hex("00 02 00 7f")),
            Frame::Unknown { is_reply: false, .. }
        ));
    }

    #[test]
    fn data_rxd_event_decodes_to_a_fragment() {
        // EVENT_NOTIFICATION(9) + event_type=2 (DATA_RXD) + fragment body. As of
        // tuxlink-7my9 this is the native APRS data path, no longer ignored.
        // byte 0xde = is_final(1) with_channel_id(1) fragment_id(0x1e); the
        // trailing byte (0xef) is the channel id, leaving data = [ad be].
        match decode_frame(&hex("00 02 00 09 02 de ad be ef")) {
            Frame::Event(Event::DataReceived { fragment }) => {
                assert!(fragment.is_final);
                assert_eq!(fragment.fragment_id, 0x1e);
                assert_eq!(fragment.channel_id, Some(0xef));
                assert_eq!(fragment.data, vec![0xad, 0xbe]);
            }
            f => panic!("wrong: {f:?}"),
        }
    }

    #[test]
    fn ht_send_data_encodes_with_header_and_fragment_body() {
        // header(31,false) = 00 02 00 1f; body for {not-final, frag 0, no
        // channel id, data [0x41]} = 0x00 0x41.
        let frag = super::super::tncdata::TncDataFragment {
            is_final: false,
            fragment_id: 0,
            channel_id: None,
            data: vec![0x41],
        };
        assert_eq!(encode_ht_send_data(&frag), hex("00 02 00 1f 00 41"));
    }

    #[test]
    fn ht_send_data_reply_decodes_status() {
        // header(31,true) = 00 02 80 1f, then reply_status byte.
        assert!(matches!(
            decode_frame(&hex("00 02 80 1f 00")),
            Frame::SendDataReply { reply_status: 0 }
        ));
        assert!(matches!(
            decode_frame(&hex("00 02 80 1f 05")),
            Frame::SendDataReply { reply_status: 5 }
        ));
    }

    #[test]
    fn write_rf_ch_round_trips_through_encoder() {
        // encode a channel, wrap-free, ensure the header is WRITE_RF_CH
        let ch = RfCh::decode(&hex(
            "00 08 bb b7 c0 08 bb b7 c0 00 00 00 00 50 00 43 41 4c 4c 00 00 00 00 00 00",
        ))
        .unwrap();
        let enc = encode_write_rf_ch(&ch);
        assert_eq!(&enc[..4], &hex("00 02 00 0e")[..]);
        assert_eq!(enc.len(), 4 + 25);
    }

    /// End-to-end (Task 10): a real outbound APRS message survives the full native
    /// data path — fragment → DATA_RXD wire frame → decode_frame → reassemble — and
    /// the APRS codec then recovers the original sender callsign + message text. Proves
    /// the Benshi fragment layer is transparent to the APRS protocol above it.
    #[test]
    fn aprs_message_round_trips_through_native_fragment_layer_e2e() {
        use super::super::tncdata::{fragment_ax25, Reassembler};
        use crate::winlink::aprs::framebuild::{build_ui_frame, extract_inbound};
        use crate::winlink::aprs::identity::AprsIdentity;
        use crate::winlink::aprs::message::{encode_message, parse_info, AprsPayload};
        use crate::winlink::ax25::frame::{Address, Frame as Ax25Frame};

        // 1. Build a real outbound APRS message frame (raw AX.25). The text is long
        //    enough (and < the 67-char APRS cap) that the frame spans >1 fragment.
        let identity = AprsIdentity {
            source: Address { call: "N0CALL".into(), ssid: 0 },
            tocall: Address { call: "APZTUX".into(), ssid: 0 },
            path: vec![],
        };
        let text = "native fragment round-trip: callsign and text must survive";
        let info = encode_message("KK6XYZ", text, Some("42"));
        let original = build_ui_frame(&identity, &info).encode().unwrap();

        // 2. Fragment it exactly as the native TX path (send_aprs_frame) would.
        let frags = fragment_ax25(&original);
        assert!(frags.len() >= 2, "test frame should span more than one fragment");

        // 3. Wrap each fragment as the radio would (a DATA_RXD EVENT_NOTIFICATION),
        //    decode through the real decode_frame, feed the recovered fragment to the
        //    reassembler — mirroring the inbound path in Driver::apply_event.
        let mut ra = Reassembler::new();
        let mut recovered: Option<Vec<u8>> = None;
        for frag in &frags {
            let mut w = header(CMD_EVENT_NOTIFICATION, false);
            w.write_uint(EventType::DataRxd as u64, 8);
            w.write_bytes(&frag.encode_body());
            match decode_frame(&w.into_bytes()) {
                Frame::Event(Event::DataReceived { fragment }) => {
                    if let Some(done) = ra.push(&fragment) {
                        recovered = Some(done);
                    }
                }
                f => panic!("expected DataReceived, got {f:?}"),
            }
        }

        // 4. The reassembled bytes equal the original AX.25 frame (layer is transparent).
        let recovered = recovered.expect("the final fragment should complete the frame");
        assert_eq!(recovered, original);

        // 5. The APRS codec recovers the original sender callsign + message text.
        let decoded = Ax25Frame::decode(&recovered).unwrap();
        let (sender, info) = extract_inbound(&decoded).expect("addressed inbound frame");
        assert_eq!(sender, "N0CALL");
        match parse_info(&info).expect("message payload") {
            AprsPayload::Message { addressee, text: got, msgid } => {
                assert_eq!(addressee, "KK6XYZ");
                assert_eq!(got, text);
                assert_eq!(msgid.as_deref(), Some("42"));
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }
}
