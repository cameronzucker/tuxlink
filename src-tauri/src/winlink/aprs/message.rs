//! APRS message info-field encoder.
//!
//! Produces the on-the-wire info field for an outgoing APRS text message:
//! `:ADDRESSEE:text{msgid`. The encoding is pinned to direwolf's
//! `encode_aprs.c` behaviour (`memset(addressee, ' ', 9)` then
//! `memcpy(call, min(len, 9))`): the addressee occupies a fixed 9-byte field,
//! left-justified, SPACE-padded (0x20), truncated at 9 bytes; the message text
//! is capped at the APRS 67-character limit.

/// Build an APRS message info field: `:ADDRESSEE:text{msgid`.
/// addressee is left-justified, space-padded to exactly 9, truncated at 9.
/// text is truncated to the APRS 67-char limit. msgid (if present) is appended after `{`.
pub fn encode_message(addressee: &str, text: &str, msgid: Option<&str>) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(b':');
    out.extend_from_slice(&pad_addressee(addressee));
    out.push(b':');
    let text: String = text.chars().take(67).collect();
    out.extend_from_slice(text.as_bytes());
    if let Some(id) = msgid {
        out.push(b'{');
        out.extend_from_slice(id.as_bytes());
    }
    out
}

/// 9-byte addressee: left-justified, space-padded, truncated at 9.
pub(crate) fn pad_addressee(call: &str) -> [u8; 9] {
    let mut buf = [b' '; 9];
    let bytes = call.as_bytes();
    let n = bytes.len().min(9);
    buf[..n].copy_from_slice(&bytes[..n]);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_message_pads_addressee_to_9_and_appends_msgid() {
        // direwolf encoder self-test: encode_message("N2GH","some stuff", None) => ":N2GH     :some stuff"
        assert_eq!(encode_message("N2GH", "some stuff", None), b":N2GH     :some stuff".to_vec());
    }

    #[test]
    fn encode_message_with_msgid() {
        assert_eq!(encode_message("WA1XYX-15", "Howdy y'all", Some("12345")),
                   b":WA1XYX-15:Howdy y'all{12345".to_vec());
    }

    #[test]
    fn encode_message_truncates_text_to_67() {
        let long = "x".repeat(80);
        let out = encode_message("AB", &long, None);
        // prefix ":AB       :" = 11 bytes, then exactly 67 x's
        assert_eq!(out.len(), 11 + 67);
    }

    #[test]
    fn encode_message_truncates_addressee_to_9() {
        let out = encode_message("VERYLONGCALL", "hi", None);
        assert_eq!(&out[..11], b":VERYLONGC:"); // 9 chars of the call, no padding needed
    }
}
