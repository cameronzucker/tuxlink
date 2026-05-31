//! Golden-vector conformance for native B2F outbound serialization.
//!
//! Fixture `LPE5NXDVLVSQ.b2f` vendored from wl2k-go v1.0.1 (MIT-licensed).
//! See fixtures/wl2k-go/LICENSE-wl2k-go.txt for attribution.
//!
//! This test asserts byte-for-byte equality between the Rust serializer and
//! wl2k-go's reference output for a real Winlink message with one binary
//! attachment.

use tuxlink_lib::winlink::message::Message;
use tuxlink_lib::winlink_backend::OutboundAttachment;

const FIXTURE: &[u8] = include_bytes!("fixtures/wl2k-go/LPE5NXDVLVSQ.b2f");

#[test]
fn serializes_lpe5nxdvlvsq_byte_for_byte() {
    // Per spec rev-3 §3.1, the fixture layout is:
    //   headers + \r\n\r\n + body (104 bytes per Body:) + \r\n
    //   + jpg (31028 bytes per File:) + \r\n
    let sep = FIXTURE.windows(4).position(|w| w == b"\r\n\r\n")
        .expect("fixture has \\r\\n\\r\\n header/body separator");
    let body_start = sep + 4;
    let body_end = body_start + 104;  // per fixture's Body: header
    let jpg_start = body_end + 2;     // skip the body→file CRLF
    let jpg_end = jpg_start + 31028;  // per fixture's File: header
    let jpg = &FIXTURE[jpg_start..jpg_end];
    let body_bytes = &FIXTURE[body_start..body_end];

    // Rev-2 correction (Plan R1 P0 + R3 P0-6): build Message DIRECTLY via
    // headers + set_body(bytes). compose_message_with_files takes &str body
    // and would panic from_utf8 on the Latin-1 body's æ/ø bytes. Direct
    // construction sidesteps that — this test verifies the SERIALIZER, not
    // compose.
    let mut msg = Message::new();
    msg.set_header("Mid", "LPE5NXDVLVSQ");
    msg.set_header("Date", "2016/07/20 19:21");
    msg.set_header("From", "LA5NTA");
    msg.set_header("Mbo", "LA5NTA");
    msg.set_header("To", "LA4TTA");
    msg.set_header("Subject", "73 fra Brekke");
    msg.set_header("Type", "Private");
    msg.set_header("Content-Transfer-Encoding", "8bit");
    msg.set_header("Content-Type", "text/plain; charset=ISO-8859-1");
    msg.set_body(body_bytes.to_vec());
    msg.set_attachments(vec![OutboundAttachment {
        filename: "1469042410710.jpg".into(),
        bytes: jpg.to_vec(),
    }]);

    let produced = msg.to_bytes();
    assert_eq!(
        produced, FIXTURE.to_vec(),
        "Rust output diverges from wl2k-go fixture; \
         produced.len()={}, fixture.len()={}",
        produced.len(), FIXTURE.len()
    );
}
