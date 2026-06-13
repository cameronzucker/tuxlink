//! APRS UI-frame builder + inbound extractor.
//!
//! APRS rides AX.25 Unnumbered Information (UI) frames. Outbound: build a UI
//! `Frame` from an [`AprsIdentity`] (dest = tocall, src = our source, digis =
//! path) plus the APRS message bytes. Inbound: pull the sender callsign and the
//! info bytes out of a received UI frame, ignoring connected-mode (non-UI)
//! traffic that shares the channel.

use super::identity::AprsIdentity;
use crate::winlink::ax25::frame::{Address, Control, Frame, Path};

/// Build an APRS UI frame: dest = tocall, src = our source, digis = path,
/// control = UI, info = the APRS message bytes (PID 0xF0 added by `Frame::encode`).
pub fn build_ui_frame(id: &AprsIdentity, info: &[u8]) -> Frame {
    Frame {
        path: Path {
            dest: id.tocall.clone(),
            src: id.source.clone(),
            digis: id.path.clone(),
        },
        control: Control::Ui { pf: false },
        info: info.to_vec(),
    }
}

/// Format a callsign+ssid as `"CALL-SSID"` (or bare `"CALL"` for ssid 0).
pub fn fmt_callsign(a: &Address) -> String {
    if a.ssid == 0 {
        a.call.clone()
    } else {
        format!("{}-{}", a.call, a.ssid)
    }
}

/// Format an AX.25 UI frame as a literal APRS TNC2 monitor string:
/// `SRC>DEST[,DIGI1,DIGI2]:info-text`. Used by the dev raw-capture path
/// (tuxlink-iehg) to record exactly what is on the wire — including frames NOT
/// addressed to us — so the on-air message format (e.g. a no-recipient packet's
/// blank addressee) can be ground-truthed against a real radio rather than
/// assumed. Lossy-decodes the info field; non-UTF-8 bytes become U+FFFD.
pub fn to_tnc2(frame: &Frame) -> String {
    let mut s = format!(
        "{}>{}",
        fmt_callsign(&frame.path.src),
        fmt_callsign(&frame.path.dest)
    );
    for digi in &frame.path.digis {
        s.push(',');
        s.push_str(&fmt_callsign(digi));
    }
    s.push(':');
    s.push_str(&String::from_utf8_lossy(&frame.info));
    s
}

/// Extract `(sender "CALL-SSID", info bytes)` from an inbound UI frame.
/// Returns `None` for non-UI frames (connected-mode traffic — ignore it).
pub fn extract_inbound(frame: &Frame) -> Option<(String, Vec<u8>)> {
    if !matches!(frame.control, Control::Ui { .. }) {
        return None;
    }
    Some((fmt_callsign(&frame.path.src), frame.info.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_and_decode_aprs_ui_frame_round_trips() {
        let id = AprsIdentity {
            source: Address { call: "N0CALL".into(), ssid: 9 },
            tocall: Address { call: "APZTUX".into(), ssid: 0 },
            path: vec![
                Address { call: "WIDE1".into(), ssid: 1 },
                Address { call: "WIDE2".into(), ssid: 1 },
            ],
        };
        let info = b":KK6XYZ   :hello{01".to_vec();
        let frame = build_ui_frame(&id, &info);
        let bytes = frame.encode().unwrap();

        let decoded = crate::winlink::ax25::frame::Frame::decode(&bytes).unwrap();
        assert_eq!(decoded.path.dest.call, "APZTUX");
        assert_eq!(decoded.path.src.call, "N0CALL");
        assert_eq!(decoded.path.src.ssid, 9);
        assert!(matches!(
            decoded.control,
            crate::winlink::ax25::frame::Control::Ui { .. }
        ));
        assert_eq!(decoded.info, info);
    }

    #[test]
    fn extract_inbound_returns_sender_and_info() {
        let inbound = crate::winlink::ax25::frame::Frame {
            path: Path {
                dest: Address { call: "APZTUX".into(), ssid: 0 },
                src: Address { call: "KK6XYZ".into(), ssid: 7 },
                digis: vec![],
            },
            control: crate::winlink::ax25::frame::Control::Ui { pf: false },
            info: b":N0CALL-9 :hi there{04".to_vec(),
        };
        let (sender, info) = extract_inbound(&inbound).unwrap();
        assert_eq!(sender, "KK6XYZ-7");
        assert_eq!(info, b":N0CALL-9 :hi there{04");
    }

    #[test]
    fn extract_inbound_rejects_non_ui_frame() {
        let i = crate::winlink::ax25::frame::Frame {
            path: Path {
                dest: Address { call: "A".into(), ssid: 0 },
                src: Address { call: "B".into(), ssid: 0 },
                digis: vec![],
            },
            control: crate::winlink::ax25::frame::Control::Sabm { pf: true },
            info: vec![],
        };
        assert!(extract_inbound(&i).is_none());
    }
}
