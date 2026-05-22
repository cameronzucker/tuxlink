//! KISS framing: wraps AX.25 frames for delivery to a TNC over serial/TCP.
//! KISS spec: FEND (0xC0) delimiters; FEND/FESC escaping inside the body.

pub const FEND: u8 = 0xC0;
pub const FESC: u8 = 0xDB;
pub const TFEND: u8 = 0xDC;
pub const TFESC: u8 = 0xDD;

/// Wrap an AX.25 frame body in a KISS data frame (port 0, command 0).
pub fn kiss_data_frame(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 4);
    out.push(FEND);
    out.push(0x00); // data frame, port 0
    for &b in body {
        match b {
            FEND => out.extend_from_slice(&[FESC, TFEND]),
            FESC => out.extend_from_slice(&[FESC, TFESC]),
            _ => out.push(b),
        }
    }
    out.push(FEND);
    out
}

#[cfg(test)]
mod kiss_encode_tests {
    use super::*;
    #[test]
    fn wraps_data_frame_with_fend_and_port_zero() {
        let out = kiss_data_frame(&[0x01, 0x02, 0x03]);
        assert_eq!(out, vec![FEND, 0x00, 0x01, 0x02, 0x03, FEND]);
    }
    #[test]
    fn escapes_fend_and_fesc_in_body() {
        let out = kiss_data_frame(&[FEND, FESC, 0xAA]);
        assert_eq!(out, vec![FEND, 0x00, FESC, TFEND, FESC, TFESC, 0xAA, FEND]);
    }
}
