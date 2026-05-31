//! Compose an outbound Winlink message from plain fields.
//!
//! Turns a "send this to these people" request into a complete Winlink message
//! with the headers the CMS expects: a generated message id, the date, the
//! sender, the recipients, and the body. The result is a [`Message`] ready to
//! be offered as a proposal and sent.
//!
//! Mirrors the message construction in `wl2k-go/fbb` (`NewMessage` +
//! `SetDate`/`SetFrom`/`AddTo`/`SetBody`); no Go ships. The message id format is
//! our own (a 12-character base32 digest) — ids only need to be unique, not
//! byte-identical to any other client.

use md5::{Digest, Md5};
use thiserror::Error;

use super::message::Message;
use crate::winlink_backend::OutboundAttachment;

/// Errors that can occur while composing an outbound message with attachments.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ComposeError {
    #[error("filename exceeds 255-character limit ({chars} chars): {filename:?}")]
    FilenameTooLong { filename: String, chars: usize },
    #[error(
        "filename contains characters outside ISO-8859-1 \
         (Q-encoding would be lossy): {filename:?}"
    )]
    FilenameNotLatin1Encodable { filename: String },
    /// A filename contained CR, LF, or NUL — characters that would inject B2F
    /// protocol headers when serialized into `File: <size> <name>` lines.
    #[error(
        "filename contains a control character that would break B2F framing \
         (\\r, \\n, or \\0): {filename:?}"
    )]
    FilenameContainsControlChar { filename: String },
}

/// Build a Private text message ready to send.
///
/// `mycall` is the sending station's call sign. `to`/`cc` are recipient
/// addresses (bare call signs, `CALL@winlink.org`, or full email addresses).
/// `unix_secs` is the send time (seconds since the epoch) — it sets the `Date`
/// header and seeds the message id, and is taken as a parameter so the result is
/// reproducible.
pub fn compose_message(
    mycall: &str,
    to: &[&str],
    cc: &[&str],
    subject: &str,
    body: &str,
    unix_secs: u64,
) -> Message {
    let mut msg = Message::new();
    let station = normalize_address(mycall);

    msg.set_header("Mid", &generate_mid(mycall, unix_secs));
    msg.set_header("Date", &format_winlink_date(unix_secs));
    msg.set_header("Type", "Private");
    msg.set_header("From", &station);
    msg.set_header("Mbo", &station);
    for addr in to {
        msg.add_header("To", &normalize_address(addr));
    }
    for addr in cc {
        msg.add_header("Cc", &normalize_address(addr));
    }
    msg.set_header("Subject", subject);
    msg.set_header("Content-Transfer-Encoding", "8bit");
    msg.set_header("Content-Type", "text/plain; charset=ISO-8859-1");
    msg.set_body(encode_body(body));
    msg
}

/// Build a Private text message with zero or more file attachments.
///
/// Returns `Err(ComposeError::FilenameTooLong)` or
/// `Err(ComposeError::FilenameNotLatin1Encodable)` if any attachment
/// filename violates the Winlink B2F constraints. The first invalid
/// filename short-circuits; the error names the offending filename so
/// the UI can surface it.
pub fn compose_message_with_files(
    mycall: &str,
    to: &[&str],
    cc: &[&str],
    subject: &str,
    body: &str,
    attachments: &[OutboundAttachment],
    unix_secs: u64,
) -> Result<Message, ComposeError> {
    // NEW: filename validation (T1.3)
    for att in attachments {
        let chars = att.filename.chars().count();
        if chars > 255 {
            return Err(ComposeError::FilenameTooLong {
                filename: att.filename.clone(),
                chars,
            });
        }
        if !att.filename.chars().all(|c| (c as u32) <= 0xff) {
            return Err(ComposeError::FilenameNotLatin1Encodable {
                filename: att.filename.clone(),
            });
        }
        // P2.2 (Codex post-impl review): CR, LF, and NUL are valid Latin-1 code
        // points, so the check above passes them — but they would inject B2F header
        // framing when the filename lands in a `File: <size> <name>` line. Reject
        // them explicitly before any serialization can happen.
        if att.filename.contains(['\r', '\n', '\0']) {
            return Err(ComposeError::FilenameContainsControlChar {
                filename: att.filename.clone(),
            });
        }
    }
    // Build the base message via compose_message (text-only path), then attach
    // the validated files. set_body in compose_message already wrote the Body:
    // header; File: headers + the attachment serialization land in
    // Message::to_bytes (Task 1.5+).
    let mut msg = compose_message(mycall, to, cc, subject, body, unix_secs);
    msg.set_attachments(attachments.to_vec());
    Ok(msg)
}

/// Generate a 12-character message id from the call sign and the send time.
///
/// The id is the base32 of an MD5 digest, truncated — unique in practice, and
/// the CMS only requires uniqueness, not a particular derivation.
pub fn generate_mid(callsign: &str, unix_secs: u64) -> String {
    let payload = format!("{unix_secs}-{callsign}");
    let digest = Md5::digest(payload.as_bytes());
    base32_encode(&digest).chars().take(12).collect()
}

/// Encode the body: normalize line endings to CRLF, then map to Latin-1 bytes
/// (the Winlink default charset). Characters outside Latin-1 become `?`.
fn encode_body(text: &str) -> Vec<u8> {
    let crlf = text.replace("\r\n", "\n").replace('\n', "\r\n");
    crlf.chars()
        .map(|c| if (c as u32) <= 0xff { c as u8 } else { b'?' })
        .collect()
}

/// Normalize a recipient/sender address the way Winlink expects: a bare call
/// sign is upper-cased; `CALL@winlink.org` becomes the bare upper-cased call
/// sign; any other email address is prefixed `SMTP:`.
fn normalize_address(addr: &str) -> String {
    // An already-qualified `proto:addr` form is kept as-is.
    if let Some((proto, rest)) = addr.split_once(':') {
        if !proto.is_empty() && !rest.contains(':') {
            return format!("{proto}:{rest}");
        }
    }
    match addr.split_once('@') {
        None => addr.to_uppercase(),
        Some((local, domain)) if domain.eq_ignore_ascii_case("winlink.org") => {
            local.to_uppercase()
        }
        Some(_) => format!("SMTP:{addr}"),
    }
}

/// Format `unix_secs` as the Winlink date header `YYYY/MM/DD HH:MM` in UTC.
fn format_winlink_date(unix_secs: u64) -> String {
    let minute = (unix_secs / 60) % 60;
    let hour = (unix_secs / 3600) % 24;
    let (year, month, day) = days_to_ymd(unix_secs / 86_400);
    format!("{year:04}/{month:02}/{day:02} {hour:02}:{minute:02}")
}

/// Convert days since 1970-01-01 to (year, month, day) on the Gregorian calendar
/// (Howard Hinnant's `civil_from_days`).
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as u64, m, d)
}

/// Standard base32 (RFC 4648, no padding), upper-case alphabet.
fn base32_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut out = String::new();
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    for &byte in data {
        buffer = (buffer << 8) | u32::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((buffer >> bits) & 0x1f) as usize] as char);
        }
        buffer &= (1u32 << bits) - 1; // keep only the unconsumed low bits
    }
    if bits > 0 {
        out.push(ALPHABET[((buffer << (5 - bits)) & 0x1f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE32_ALPHABET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

    #[test]
    fn generates_a_twelve_character_message_id() {
        // Anchored to an independently computed base32(md5("1716200000-N7CPZ")).
        assert_eq!(generate_mid("N7CPZ", 1_716_200_000), "LIHHQU663POB");
    }

    #[test]
    fn message_ids_vary_by_time_and_call_sign() {
        let base = generate_mid("N7CPZ", 1_716_200_000);
        assert_eq!(base.len(), 12);
        assert!(base.chars().all(|c| BASE32_ALPHABET.contains(c)));
        assert_ne!(base, generate_mid("N7CPZ", 1_716_200_001));
        assert_ne!(base, generate_mid("W1AW", 1_716_200_000));
    }

    #[test]
    fn composes_a_message_with_the_standard_headers() {
        let msg = compose_message(
            "N7CPZ",
            &["W1AW"],
            &[],
            "Field report",
            "All clear.",
            1_716_200_000,
        );
        assert_eq!(msg.header("Mid"), Some("LIHHQU663POB"));
        assert_eq!(msg.header("Date"), Some("2024/05/20 10:13"));
        assert_eq!(msg.header("Type"), Some("Private"));
        assert_eq!(msg.header("From"), Some("N7CPZ"));
        assert_eq!(msg.header("Mbo"), Some("N7CPZ"));
        assert_eq!(msg.header("To"), Some("W1AW"));
        assert_eq!(msg.header("Subject"), Some("Field report"));
        assert_eq!(msg.header("Content-Transfer-Encoding"), Some("8bit"));
        assert_eq!(
            msg.header("Content-Type"),
            Some("text/plain; charset=ISO-8859-1")
        );
        assert_eq!(msg.body(), b"All clear.");
        assert_eq!(msg.header("Body"), Some("10"));
    }

    #[test]
    fn normalizes_sender_and_recipient_addresses() {
        let msg = compose_message(
            "n7cpz",
            &["w1aw@winlink.org", "ann@example.com"],
            &[],
            "Hi",
            "x",
            1_716_200_000,
        );
        assert_eq!(msg.header("From"), Some("N7CPZ")); // bare call upper-cased
        assert_eq!(msg.header("To"), Some("W1AW")); // @winlink.org stripped
                                                     // The full email is kept with an SMTP prefix (second To line).
        assert!(msg
            .to_bytes()
            .windows(20)
            .any(|w| w == b"To: SMTP:ann@example"));
    }

    #[test]
    fn normalizes_crlf_line_endings_in_the_body() {
        let msg = compose_message("N7CPZ", &["W1AW"], &[], "S", "line1\nline2", 1_716_200_000);
        assert_eq!(msg.body(), b"line1\r\nline2");
    }

    #[test]
    fn a_composed_message_can_become_a_proposal() {
        let msg = compose_message("N7CPZ", &["W1AW"], &[], "Test", "hello", 1_716_200_000);
        let (proposal, compressed) = msg.to_proposal().unwrap();
        assert_eq!(proposal.mid, "LIHHQU663POB");
        assert!(!compressed.is_empty());
    }

    #[test]
    fn compose_with_no_files_matches_text_only_path() {
        let no_files = compose_message_with_files(
            "N7CPZ", &["W1AW"], &[], "Hi", "body", &[], 1_716_200_000,
        ).expect("no filenames → cannot fail");
        let text_only = compose_message("N7CPZ", &["W1AW"], &[], "Hi", "body", 1_716_200_000);
        assert_eq!(no_files.to_bytes(), text_only.to_bytes());
    }

    #[test]
    fn compose_rejects_filename_over_255_chars() {
        let long: String = "a".repeat(256);
        let att = OutboundAttachment { filename: long.clone(), bytes: vec![1, 2, 3] };
        let err = compose_message_with_files(
            "N7CPZ", &["W1AW"], &[], "Hi", "body", &[att], 1_716_200_000,
        ).unwrap_err();
        assert!(matches!(err, ComposeError::FilenameTooLong { chars: 256, .. }),
                "expected FilenameTooLong{{chars: 256, ..}}, got {:?}", err);
    }

    #[test]
    fn compose_rejects_non_latin1_filename() {
        let att = OutboundAttachment { filename: "日本語.txt".into(), bytes: vec![1, 2] };
        let err = compose_message_with_files(
            "N7CPZ", &["W1AW"], &[], "Hi", "body", &[att], 1_716_200_000,
        ).unwrap_err();
        assert!(matches!(err, ComposeError::FilenameNotLatin1Encodable { .. }),
                "expected FilenameNotLatin1Encodable, got {:?}", err);
    }

    #[test]
    fn compose_attaches_files_to_message() {
        let att = OutboundAttachment { filename: "report.txt".into(), bytes: b"hello".to_vec() };
        let msg = compose_message_with_files(
            "N7CPZ", &["W1AW"], &[], "Hi", "body", &[att.clone()], 1_716_200_000,
        ).unwrap();
        assert_eq!(msg.attachments().len(), 1);
        assert_eq!(msg.attachments()[0].filename, "report.txt");
        assert_eq!(msg.attachments()[0].bytes, b"hello");
    }

    #[test]
    fn compose_short_circuits_on_first_invalid_filename() {
        let ok = OutboundAttachment { filename: "good.txt".into(), bytes: vec![1] };
        let bad = OutboundAttachment { filename: "日本.bin".into(), bytes: vec![2] };
        let err = compose_message_with_files(
            "N7CPZ", &["W1AW"], &[], "Hi", "body", &[ok, bad], 1_716_200_000,
        ).unwrap_err();
        match err {
            ComposeError::FilenameNotLatin1Encodable { filename } => {
                assert_eq!(filename, "日本.bin");
            }
            other => panic!("expected FilenameNotLatin1Encodable for '日本.bin', got {:?}", other),
        }
    }

    #[test]
    fn composes_multi_recipient_with_attachments() {
        let attachments = vec![
            OutboundAttachment { filename: "a.bin".into(), bytes: vec![1] },
            OutboundAttachment { filename: "b.bin".into(), bytes: vec![2] },
        ];
        let msg = compose_message_with_files(
            "N7CPZ",
            &["W1AW", "K1AB"],
            &["KE7XYZ"],
            "Multi",
            "body",
            &attachments,
            1_716_200_000,
        ).unwrap();
        let bytes = msg.to_bytes();
        let s = String::from_utf8_lossy(&bytes);
        assert_eq!(s.matches("\r\nTo: ").count(), 2, "two To: headers expected; got: {s}");
        assert_eq!(s.matches("\r\nCc: ").count(), 1, "one Cc: header expected; got: {s}");
        assert_eq!(s.matches("\r\nFile: ").count(), 2, "two File: headers expected; got: {s}");
    }

    // P2.2 (Codex post-impl review): CR, LF, NUL in filenames must be rejected
    // before serialization to prevent B2F header injection via `File: <size> <name>`.

    #[test]
    fn compose_rejects_filename_with_carriage_return() {
        let att = OutboundAttachment {
            filename: "inject\rheader.txt".into(),
            bytes: vec![1],
        };
        let err = compose_message_with_files(
            "N7CPZ", &["W1AW"], &[], "Hi", "body", &[att], 1_716_200_000,
        )
        .unwrap_err();
        assert!(
            matches!(err, ComposeError::FilenameContainsControlChar { .. }),
            "expected FilenameContainsControlChar for \\r; got: {err:?}"
        );
    }

    #[test]
    fn compose_rejects_filename_with_newline() {
        let att = OutboundAttachment {
            filename: "inject\nheader.txt".into(),
            bytes: vec![1],
        };
        let err = compose_message_with_files(
            "N7CPZ", &["W1AW"], &[], "Hi", "body", &[att], 1_716_200_000,
        )
        .unwrap_err();
        assert!(
            matches!(err, ComposeError::FilenameContainsControlChar { .. }),
            "expected FilenameContainsControlChar for \\n; got: {err:?}"
        );
    }

    #[test]
    fn compose_rejects_filename_with_nul() {
        let att = OutboundAttachment {
            filename: "inject\0header.txt".into(),
            bytes: vec![1],
        };
        let err = compose_message_with_files(
            "N7CPZ", &["W1AW"], &[], "Hi", "body", &[att], 1_716_200_000,
        )
        .unwrap_err();
        assert!(
            matches!(err, ComposeError::FilenameContainsControlChar { .. }),
            "expected FilenameContainsControlChar for \\0; got: {err:?}"
        );
    }

    #[test]
    fn compose_accepts_filename_without_control_chars() {
        // Sanity: a normal filename must still pass validation unimpeded.
        let att = OutboundAttachment {
            filename: "valid-report_2026.txt".into(),
            bytes: b"data".to_vec(),
        };
        assert!(
            compose_message_with_files(
                "N7CPZ", &["W1AW"], &[], "Hi", "body", &[att], 1_716_200_000,
            )
            .is_ok(),
            "normal filename should pass P2.2 validation"
        );
    }
}
