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

/// A parsed inbound APRS message-type payload.
///
/// Mirrors the message-type packets emitted by direwolf `decode_aprs.c` and
/// parsed by aprslib `message.py`: a plain/msgID-bearing text message, an `ack`,
/// or a `rej`. The newer reply-ack `}` forms are tolerated (truncated at `}`)
/// rather than rejected so we do not choke on packets from stations that use them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AprsPayload {
    Message { addressee: String, text: String, msgid: Option<String> },
    Ack { addressee: String, msgid: String },
    Rej { addressee: String, msgid: String },
}

/// Parse an APRS message info field. Returns None if it is not a well-formed
/// message-type packet (wrong DTI, too short, malformed addressee field).
pub fn parse_info(info: &[u8]) -> Option<AprsPayload> {
    // Fixed prefix: ':' + 9-char addressee + ':' = 11 bytes minimum.
    if info.len() < 11 || info[0] != b':' || info[10] != b':' {
        return None;
    }
    let addressee = std::str::from_utf8(&info[1..10]).ok()?.trim_end_matches(' ').to_string();
    let body = std::str::from_utf8(&info[11..]).ok()?;

    // ack / rej (literal lowercase per direwolf). msgid = everything after the keyword,
    // truncated at a '}' (new reply-ack form) so "ackAB}CD" => "AB".
    for (kw, is_ack) in [("ack", true), ("rej", false)] {
        if let Some(rest) = body.strip_prefix(kw) {
            let msgid = trim_msgid(rest);
            if msgid.is_empty() { return None; } // direwolf errors on missing number
            return Some(if is_ack {
                AprsPayload::Ack { addressee, msgid }
            } else {
                AprsPayload::Rej { addressee, msgid }
            });
        }
    }

    // Plain message, optional {msgid (old) or {MM}AA (new — tolerate).
    let (text, msgid) = match body.split_once('{') {
        Some((t, id_tail)) => (t, Some(trim_msgid(id_tail))),
        None => (body, None),
    };
    let text = text.trim_end_matches(' ').to_string();
    let msgid = msgid.filter(|m| !m.is_empty());
    Some(AprsPayload::Message { addressee, text, msgid })
}

/// Extract a usable msgid from the tail after `ack`/`rej`/`{`: stop at a '}'
/// (new reply-ack delimiter) and cap at 5 chars (old-format max).
fn trim_msgid(tail: &str) -> String {
    let core = tail.split('}').next().unwrap_or("");
    core.chars().take(5).collect()
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

    #[test]
    fn parse_plain_message() {
        let p = parse_info(b":WXBOT    :HelloWorld  ").unwrap();
        assert_eq!(p, AprsPayload::Message {
            addressee: "WXBOT".into(), text: "HelloWorld".into(), msgid: None });
    }

    #[test]
    fn parse_message_with_msgid() {
        let p = parse_info(b":WA1XYX-15:Howdy y'all{12345").unwrap();
        assert_eq!(p, AprsPayload::Message {
            addressee: "WA1XYX-15".into(), text: "Howdy y'all".into(), msgid: Some("12345".into()) });
    }

    #[test]
    fn parse_ack_old_format() {
        let p = parse_info(b":WXBOT    :ack003").unwrap();
        assert_eq!(p, AprsPayload::Ack { addressee: "WXBOT".into(), msgid: "003".into() });
    }

    #[test]
    fn parse_rej_old_format() {
        let p = parse_info(b":WXBOT    :rej123").unwrap();
        assert_eq!(p, AprsPayload::Rej { addressee: "WXBOT".into(), msgid: "123".into() });
    }

    #[test]
    fn parse_new_format_ack_tolerated() {
        assert_eq!(parse_info(b":WXBOT    :ackAB}").unwrap(),
                   AprsPayload::Ack { addressee: "WXBOT".into(), msgid: "AB".into() });
        assert_eq!(parse_info(b":WXBOT    :ackAB}CD").unwrap(),
                   AprsPayload::Ack { addressee: "WXBOT".into(), msgid: "AB".into() });
    }

    #[test]
    fn parse_reply_ack_message_tolerated() {
        let p = parse_info(b":WXBOT    :HelloWorld  {AB}CD").unwrap();
        assert_eq!(p, AprsPayload::Message {
            addressee: "WXBOT".into(), text: "HelloWorld".into(), msgid: Some("AB".into()) });
    }

    #[test]
    fn parse_rejects_too_short() {
        assert!(parse_info(b":short").is_none());        // < 11 byte prefix
        assert!(parse_info(b"no colon dti").is_none());  // missing leading ':'
    }
}
