//! ARDOP wire-level codec (TCP host-protocol mode).
//!
//! TCP-mode framing per ardopcf docs and wl2k-go `transport/ardop/frame.go`:
//! - Cmd socket (default 8515): `<ASCII>\r`-terminated lines, both directions.
//!   No CRC, no type prefix (CRC + "C:" prefix are serial-mode-only).
//! - Data socket (default 8516) inbound: `[u16 BE length][3-byte type][payload]`.
//! - Data socket outbound: raw bytes (TNC frames them for TX).

/// Encode a command for the ARDOP TCP cmd socket.
///
/// Appends a single `\r` terminator. No "C:" prefix and no CRC — those are
/// serial/non-TCP framing only (see wl2k-go `frame.go` `writeCtrlFrame`).
pub fn encode_cmd_line(line: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(line.len() + 1);
    out.extend_from_slice(line.as_bytes());
    out.push(b'\r');
    out
}

#[cfg(test)]
mod cmd_line_tests {
    use super::*;

    #[test]
    fn encode_cmd_line_appends_cr_and_no_prefix() {
        // ARDOP TCP-mode cmd socket: bare ASCII line terminated by \r. No "C:" prefix
        // (that prefix is only for the non-TCP/serial transport, per wl2k-go frame.go).
        let out = encode_cmd_line("MYCALL N7CPZ");
        assert_eq!(out, b"MYCALL N7CPZ\r");
    }

    #[test]
    fn encode_cmd_line_handles_no_args() {
        assert_eq!(encode_cmd_line("INITIALIZE"), b"INITIALIZE\r");
    }

    #[test]
    fn decode_lines_splits_on_cr_only() {
        // The cmd socket reader yields complete \r-terminated lines.
        let mut buf = Vec::new();
        let mut out = Vec::new();
        feed_and_drain(&mut buf, &mut out, b"NEWSTATE DISC\rCONNECTED W7ABC 500\r");
        assert_eq!(
            out,
            vec![
                "NEWSTATE DISC".to_string(),
                "CONNECTED W7ABC 500".to_string()
            ]
        );
    }

    #[test]
    fn decode_lines_holds_partial_until_cr() {
        let mut buf = Vec::new();
        let mut out = Vec::new();
        feed_and_drain(&mut buf, &mut out, b"NEWSTATE ");
        assert!(out.is_empty(), "no CR yet -> no line yielded");
        feed_and_drain(&mut buf, &mut out, b"DISC\r");
        assert_eq!(out, vec!["NEWSTATE DISC".to_string()]);
    }

    // Helper for tests: append `chunk` to `buf`, drain any complete \r-terminated
    // lines into `out`.
    fn feed_and_drain(buf: &mut Vec<u8>, out: &mut Vec<String>, chunk: &[u8]) {
        buf.extend_from_slice(chunk);
        while let Some(pos) = buf.iter().position(|&b| b == b'\r') {
            let line = String::from_utf8(buf.drain(..pos).collect()).expect("ascii");
            buf.drain(..1); // drop the \r
            out.push(line);
        }
    }
}
