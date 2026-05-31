//! The Winlink message structure (winlink.org/B2F).
//!
//! A message is email-like: header lines, a blank line, then the body, then
//! any attachments. On the wire:
//!   - the `Mid` header (the message's unique id) is written first;
//!   - the remaining headers follow in alphabetical order;
//!   - each header line ends with CRLF (`\r\n`);
//!   - a blank CRLF line ends the header block;
//!   - the raw body bytes follow (the body's byte length is the `Body` header).
//!
//! Header keys are case-insensitive. Attachment handling is added in a later
//! step; this step covers building a message and writing the header+body form.

use super::proposal::Proposal;

/// A Winlink message being built (and, later, parsed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// Headers in the order they were set. Order here does not matter — the
    /// wire form is always `Mid` first, then the rest sorted alphabetically.
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    attachments: Vec<crate::winlink_backend::OutboundAttachment>,
}

impl Default for Message {
    fn default() -> Self {
        Self::new()
    }
}

impl Message {
    /// A new, empty message.
    pub fn new() -> Self {
        Self { headers: Vec::new(), body: Vec::new(), attachments: Vec::new() }
    }

    /// The attachments on this message (empty by default).
    pub fn attachments(&self) -> &[crate::winlink_backend::OutboundAttachment] {
        &self.attachments
    }

    /// Set the attachments. Callers that go through compose get filename validation
    /// automatically; direct callers (e.g. golden-vector tests building from raw bytes)
    /// are responsible for pre-validated filenames.
    ///
    /// Also synthesizes `File:` headers (one per attachment) from the attachment list,
    /// removing any pre-existing `File:` headers first so re-population is idempotent.
    pub fn set_attachments(
        &mut self,
        files: Vec<crate::winlink_backend::OutboundAttachment>,
    ) {
        // Remove any prior File: headers; they'll be re-emitted from the file list.
        self.headers.retain(|(k, _)| !k.eq_ignore_ascii_case("File"));
        for f in &files {
            self.headers.push((
                "File".to_string(),
                format!("{} {}", f.bytes.len(), encode_filename(&f.filename)),
            ));
        }
        self.attachments = files;
    }

    /// Set a header, replacing any existing value for the same key
    /// (keys are matched case-insensitively).
    pub fn set_header(&mut self, key: &str, value: &str) {
        self.headers.retain(|(k, _)| !k.eq_ignore_ascii_case(key));
        self.headers.push((key.to_string(), value.to_string()));
    }

    /// Add a header line without removing existing ones for the same key. Used
    /// for repeatable headers like `To` and `Cc`, which appear once per
    /// recipient on the wire.
    pub fn add_header(&mut self, key: &str, value: &str) {
        self.headers.push((key.to_string(), value.to_string()));
    }

    /// Set the body bytes. Also sets the `Body` header to the body's byte
    /// length, which is how the wire form announces how many body bytes follow.
    pub fn set_body(&mut self, body: Vec<u8>) {
        self.set_header("Body", &body.len().to_string());
        self.body = body;
    }

    /// Serialize to the Winlink wire format (header block + body).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let write_line = |out: &mut Vec<u8>, key: &str, value: &str| {
            out.extend_from_slice(key.as_bytes());
            out.extend_from_slice(b": ");
            out.extend_from_slice(value.as_bytes());
            out.extend_from_slice(b"\r\n");
        };

        // Mid is always written first.
        if let Some((k, v)) = self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("Mid")) {
            write_line(&mut out, k, v);
        }

        // The remaining headers follow in alphabetical order by key.
        let mut rest: Vec<&(String, String)> = self
            .headers
            .iter()
            .filter(|(k, _)| !k.eq_ignore_ascii_case("Mid"))
            .collect();
        rest.sort_by(|(a, _), (b, _)| a.cmp(b));
        for (k, v) in rest {
            write_line(&mut out, k, v);
        }

        // A blank line ends the header block; the raw body follows.
        out.extend_from_slice(b"\r\n");
        out.extend_from_slice(&self.body);

        // NEW: write attachment region if any
        if !self.attachments.is_empty() {
            out.extend_from_slice(b"\r\n");  // body→first-attachment separator
            for att in &self.attachments {
                out.extend_from_slice(&att.bytes);
                out.extend_from_slice(b"\r\n");  // post-attachment terminator
            }
        }
        out
    }

    /// Get a header value (case-insensitive key), if set.
    pub fn header(&self, key: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    /// All values for a header (case-insensitive key), in order. Repeatable
    /// headers like `To` and `Cc` can appear more than once.
    pub fn header_all(&self, key: &str) -> Vec<&str> {
        self.headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
            .collect()
    }

    /// The message body bytes.
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    /// Prepare this message for sending: the proposal line that offers it, and
    /// the compressed bytes that will travel in the framed block.
    ///
    /// The whole serialized message (headers + body) is compressed; the proposal
    /// reports both the uncompressed and compressed sizes so the other side
    /// knows what it is accepting. Returns `None` if the message has no `Mid`,
    /// which every sendable message must carry. Uses the standard compressed
    /// format (`C`).
    pub fn to_proposal(&self) -> Option<(Proposal, Vec<u8>)> {
        let mid = self.header("Mid")?.to_string();
        let bytes = self.to_bytes();
        let compressed = crate::winlink::lzhuf::compress(&bytes);
        let proposal = Proposal {
            code: 'C',
            msg_type: "EM".to_string(),
            mid,
            size: bytes.len(),
            compressed_size: compressed.len(),
        };
        Some((proposal, compressed))
    }

    /// Parse a message from the Winlink wire format (header block + body).
    ///
    /// Attachments are handled in a later step; this reads the header lines and
    /// then the body, whose length comes from the `Body` header.
    pub fn from_bytes(input: &[u8]) -> Result<Message, ParseError> {
        // The blank line (CRLF CRLF) separates the header block from the body.
        let sep = find_subslice(input, b"\r\n\r\n").ok_or(ParseError::NoHeaderTerminator)?;
        let header_block = &input[..sep];
        let after_headers = &input[sep + 4..];

        let mut msg = Message::new();
        for line in header_block.split(|&b| b == b'\n') {
            let line = line.strip_suffix(b"\r").unwrap_or(line); // drop the trailing CR
            if line.is_empty() {
                continue;
            }
            let text = std::str::from_utf8(line).map_err(|_| ParseError::NonUtf8Header)?;
            let (key, value) = text.split_once(": ").ok_or(ParseError::MalformedHeader)?;
            msg.set_header(key, value);
        }

        // The Body header gives the body length in bytes.
        let body_size = msg
            .header("Body")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0);
        if after_headers.len() < body_size {
            return Err(ParseError::TruncatedBody);
        }
        msg.body = after_headers[..body_size].to_vec();
        Ok(msg)
    }
}

/// Encode a filename for the File: header value.
///
/// ASCII filenames pass through unchanged. Non-ASCII filenames (which must
/// be Latin-1 encodable — compose-time validation in T1.3 rejects anything
/// else) are RFC 2047 Q-encoded with charset=ISO-8859-1 and lowercase `q`,
/// matching wl2k-go fbb/message.go:436-437.
fn encode_filename(name: &str) -> String {
    if name.is_ascii() {
        return name.to_string();
    }
    let mut encoded = String::from("=?ISO-8859-1?q?");
    for c in name.chars() {
        let cp = c as u32;
        if cp > 0xff {
            // Defensive: compose-level validation rejects this case.
            encoded.push('?');
            continue;
        }
        let b = cp as u8;
        // RFC 2047 Q-encoding: printable ASCII (except = ? _) emitted as-is;
        // space → _; everything else → =HH (hex).
        if b == b' ' {
            encoded.push('_');
        } else if b > 0x20 && b < 0x7f && b != b'=' && b != b'?' && b != b'_' {
            encoded.push(b as char);
        } else {
            encoded.push_str(&format!("={:02X}", b));
        }
    }
    encoded.push_str("?=");
    encoded
}

/// Find the first position of `needle` within `haystack`.
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Why a message could not be parsed from wire bytes.
#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    /// No blank line separated the header block from the body.
    NoHeaderTerminator,
    /// A header line was not in `Key: value` form.
    MalformedHeader,
    /// A header line contained bytes that are not valid UTF-8.
    NonUtf8Header,
    /// The input ended before the whole body (per the `Body` header) was read.
    TruncatedBody,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::winlink::{lzhuf, transfer};

    #[test]
    fn message_carries_attachments_field() {
        let msg = Message::new();
        assert!(msg.attachments().is_empty());
    }

    #[test]
    fn builds_a_proposal_and_compressed_body_from_a_message() {
        let mut msg = Message::new();
        msg.set_header("Mid", "TESTMID12345");
        msg.set_header("Subject", "Hello");
        msg.set_header("From", "N7CPZ");
        msg.set_body(b"Body text".to_vec());

        let (proposal, compressed) = msg.to_proposal().unwrap();

        assert_eq!(proposal.code, 'C');
        assert_eq!(proposal.msg_type, "EM");
        assert_eq!(proposal.mid, "TESTMID12345");
        assert_eq!(proposal.size, msg.to_bytes().len());
        assert_eq!(proposal.compressed_size, compressed.len());
    }

    #[test]
    fn a_message_without_a_mid_cannot_become_a_proposal() {
        let mut msg = Message::new();
        msg.set_header("Subject", "No id");
        msg.set_body(b"x".to_vec());
        assert!(msg.to_proposal().is_none());
    }

    #[test]
    fn a_message_survives_the_whole_send_then_receive_path() {
        // Build a message, turn it into a proposal + compressed body, frame it
        // for sending, read the frame back, decompress, and parse — the message
        // that comes out the far end must match the one that went in.
        let mut msg = Message::new();
        msg.set_header("Mid", "ROUNDTRIP001");
        msg.set_header("Subject", "Field report");
        msg.set_header("From", "N7CPZ");
        msg.set_header("To", "SERVICE@winlink.org");
        msg.set_body(b"All stations operational. Net control standing by.\r\n".to_vec());

        let (_proposal, compressed) = msg.to_proposal().unwrap();
        let framed = transfer::frame_block("Field report", 0, &compressed);

        let mut cursor = std::io::Cursor::new(framed);
        let block = transfer::read_block(&mut cursor).unwrap();
        let decompressed = lzhuf::decompress(&block.data).unwrap();
        let received = Message::from_bytes(&decompressed).unwrap();

        assert_eq!(received.header("Mid"), Some("ROUNDTRIP001"));
        assert_eq!(received.header("Subject"), Some("Field report"));
        assert_eq!(received.header("To"), Some("SERVICE@winlink.org"));
        assert_eq!(
            received.body(),
            b"All stations operational. Net control standing by.\r\n"
        );
    }

    #[test]
    fn serializes_with_mid_first_then_alphabetical_headers_then_body() {
        let mut msg = Message::new();
        msg.set_header("Mid", "ABC123");
        msg.set_header("To", "SERVICE@winlink.org");
        msg.set_header("From", "N7CPZ");
        msg.set_header("Subject", "Test");
        msg.set_body(b"Hello world\r\n".to_vec()); // 13 bytes

        let bytes = msg.to_bytes();

        let expected = [
            "Mid: ABC123\r\n",            // Mid is always written first
            "Body: 13\r\n",              // then the rest, alphabetical: Body, From, Subject, To
            "From: N7CPZ\r\n",
            "Subject: Test\r\n",
            "To: SERVICE@winlink.org\r\n",
            "\r\n",                       // blank line ends the header block
            "Hello world\r\n",            // raw body bytes
        ]
        .concat();
        assert_eq!(String::from_utf8(bytes).unwrap(), expected);
    }

    #[test]
    fn to_bytes_emits_file_header_and_attachment_bytes() {
        let mut msg = Message::new();
        msg.set_header("Mid", "TESTMID12345");
        msg.set_header("Subject", "Hi");
        msg.set_header("From", "N7CPZ");
        msg.set_body(b"hello".to_vec());
        msg.set_attachments(vec![
            crate::winlink_backend::OutboundAttachment {
                filename: "a.bin".into(),
                bytes: vec![0xAA, 0xBB, 0xCC],
            },
        ]);
        let bytes = msg.to_bytes();
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("\r\nFile: 3 a.bin\r\n"),
                "expected File: header, got: {s}");
        // Body section: text body, CRLF, attachment bytes, CRLF
        let body_section_start = bytes.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        let body_section = &bytes[body_section_start..];
        assert_eq!(body_section, b"hello\r\n\xAA\xBB\xCC\r\n");
    }

    #[test]
    fn to_bytes_preserves_attachment_declaration_order() {
        let mut msg = Message::new();
        msg.set_header("Mid", "MID2");
        msg.set_header("From", "N7CPZ");
        msg.set_body(b"x".to_vec());
        msg.set_attachments(vec![
            crate::winlink_backend::OutboundAttachment { filename: "a.bin".into(), bytes: vec![1] },
            crate::winlink_backend::OutboundAttachment { filename: "b.bin".into(), bytes: vec![2] },
            crate::winlink_backend::OutboundAttachment { filename: "c.bin".into(), bytes: vec![3] },
        ]);
        let bytes = msg.to_bytes();
        // Find the body region after \r\n\r\n
        let bs = bytes.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        assert_eq!(&bytes[bs..], b"x\r\n\x01\r\n\x02\r\n\x03\r\n");
        // File: headers must also be in declaration order
        let header_block = &bytes[..bs - 2];  // exclude the trailing \r\n
        let header_str = std::str::from_utf8(header_block).unwrap();
        let file_lines: Vec<&str> = header_str
            .lines()
            .filter(|l| l.starts_with("File:"))
            .collect();
        assert_eq!(file_lines, vec!["File: 1 a.bin", "File: 1 b.bin", "File: 1 c.bin"]);
    }

    #[test]
    fn to_bytes_with_zero_attachments_emits_no_trailing_crlf() {
        let mut msg = Message::new();
        msg.set_header("Mid", "MID3");
        msg.set_header("From", "N7CPZ");
        msg.set_body(b"plain".to_vec());
        // No set_attachments call.
        let bytes = msg.to_bytes();
        let bs = bytes.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        assert_eq!(&bytes[bs..], b"plain");  // exact — no trailing CRLF
    }

    #[test]
    fn parses_headers_and_body_from_wire_bytes() {
        let wire = [
            "Mid: ABC123\r\n",
            "From: N7CPZ\r\n",
            "Subject: Test\r\n",
            "To: SERVICE@winlink.org\r\n",
            "Body: 13\r\n",
            "\r\n",
            "Hello world\r\n",
        ]
        .concat();

        let msg = Message::from_bytes(wire.as_bytes()).unwrap();

        assert_eq!(msg.header("Mid"), Some("ABC123"));
        assert_eq!(msg.header("From"), Some("N7CPZ"));
        assert_eq!(msg.header("Subject"), Some("Test"));
        assert_eq!(msg.header("To"), Some("SERVICE@winlink.org"));
        assert_eq!(msg.body(), b"Hello world\r\n");
    }

    #[test]
    fn q_encodes_non_ascii_filename_with_iso_8859_1() {
        let mut msg = Message::new();
        msg.set_header("Mid", "MIDQ");
        msg.set_header("From", "N7CPZ");
        msg.set_body(b"x".to_vec());
        msg.set_attachments(vec![
            crate::winlink_backend::OutboundAttachment {
                // U+00E9 (é, Latin-1 0xE9)
                filename: "café.txt".into(),
                bytes: vec![1],
            },
        ]);
        let bytes = msg.to_bytes();
        let s = String::from_utf8_lossy(&bytes);
        // Lowercase q, charset ISO-8859-1 per wl2k-go fbb/message.go:436-437
        assert!(s.contains("File: 1 =?ISO-8859-1?q?caf=E9.txt?="),
                "expected Q-encoded filename, got: {s}");
    }

    #[test]
    fn ascii_filename_passes_through_unencoded() {
        let mut msg = Message::new();
        msg.set_header("Mid", "MIDA");
        msg.set_header("From", "N7CPZ");
        msg.set_body(b"x".to_vec());
        msg.set_attachments(vec![
            crate::winlink_backend::OutboundAttachment {
                filename: "plain.txt".into(),
                bytes: vec![1],
            },
        ]);
        let bytes = msg.to_bytes();
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("File: 1 plain.txt"),
                "expected unencoded ASCII filename, got: {s}");
    }

    #[test]
    fn to_proposal_size_includes_attachment_bytes_and_crlfs() {
        let mut msg = Message::new();
        msg.set_header("Mid", "MIDPROP");
        msg.set_header("Subject", "T");
        msg.set_header("From", "N7CPZ");
        msg.set_body(b"body".to_vec());  // 4 bytes
        msg.set_attachments(vec![
            crate::winlink_backend::OutboundAttachment {
                filename: "x.bin".into(),
                bytes: vec![0; 10],  // 10 bytes
            },
        ]);
        let (proposal, _compressed) = msg.to_proposal().unwrap();
        let raw = msg.to_bytes();
        assert_eq!(proposal.size, raw.len(),
                   "proposal.size = {}, to_bytes len = {}", proposal.size, raw.len());
    }
}
