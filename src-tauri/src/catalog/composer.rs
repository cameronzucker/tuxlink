//! Catalog-request message composer.
//!
//! Wire format (verified empirically against N7CPZ's WLE outbox; see
//! `docs/design/2026-06-02-cms-request-protocol-grounding.md` §"Source 2"):
//!
//! ```text
//! To:      INQUIRY@winlink.org
//! Subject: REQUEST
//! Body:    <FILENAME>
//!          [<FILENAME>]
//!          [<FILENAME>]
//! ```
//!
//! One filename per line; multiple filenames per message are explicitly
//! supported (verified fixture: `5YTNBV3JOZA8.mime` body = `PUB_PACKET\r\n
//! PUB_VARA`). The CMS replies with one separate Private message per
//! filename.
//!
//! This module owns body composition only — `compose_message` from
//! `crate::winlink::compose` does the actual Message struct + headers.

use crate::winlink::compose::compose_message;
use crate::winlink::message::Message;

/// The canonical recipient for WLE catalog inquiries.
pub const INQUIRY_RECIPIENT: &str = "INQUIRY@winlink.org";

/// The canonical Subject header for WLE catalog inquiries.
pub const INQUIRY_SUBJECT: &str = "REQUEST";

#[derive(Debug, thiserror::Error)]
pub enum InquiryComposeError {
    #[error("no filenames selected")]
    Empty,
    #[error("filename {0:?} contains a newline (would split into multiple inquiries)")]
    FilenameContainsNewline(String),
    #[error("filename {0:?} is empty after trim")]
    EmptyFilename(String),
}

/// Build the request body — newline-joined filenames. Returns
/// `Err(Empty)` for a zero-length list (the UI should disable the
/// send button rather than reach this). Returns
/// `Err(FilenameContainsNewline)` if any filename has a `\n` or `\r`
/// (would silently split into multiple inquiries — guard against bad
/// catalog data or UI bugs).
pub fn build_inquiry_body(filenames: &[&str]) -> Result<String, InquiryComposeError> {
    if filenames.is_empty() {
        return Err(InquiryComposeError::Empty);
    }
    for &f in filenames {
        let trimmed = f.trim();
        if trimmed.is_empty() {
            return Err(InquiryComposeError::EmptyFilename(f.to_string()));
        }
        if f.contains('\n') || f.contains('\r') {
            return Err(InquiryComposeError::FilenameContainsNewline(f.to_string()));
        }
    }
    Ok(filenames.join("\n"))
}

/// Compose a catalog-request `Message` ready to drop into the outbox.
/// `mycall` is the sending station's call sign; `unix_secs` is the send
/// time (parameterized for reproducible tests).
pub fn compose_inquiry_message(
    mycall: &str,
    filenames: &[&str],
    unix_secs: u64,
) -> Result<Message, InquiryComposeError> {
    let body = build_inquiry_body(filenames)?;
    Ok(compose_message(
        mycall,
        &[INQUIRY_RECIPIENT],
        &[],
        INQUIRY_SUBJECT,
        &body,
        unix_secs,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_filename_list_is_rejected() {
        assert!(matches!(build_inquiry_body(&[]), Err(InquiryComposeError::Empty)));
    }

    #[test]
    fn single_filename_body_is_just_the_filename() {
        // Real N7CPZ outbox sample: 0NXS7HZNEKA7.mime body = "WCVS.JPG"
        assert_eq!(build_inquiry_body(&["WCVS.JPG"]).unwrap(), "WCVS.JPG");
    }

    #[test]
    fn multi_filename_body_is_newline_joined() {
        // Real N7CPZ outbox sample: 5YTNBV3JOZA8.mime — RMS list request.
        // Body literally `PUB_PACKET\r\nPUB_VARA` (the CRLF is MIME line
        // ending; the canonical content is two filenames on two lines).
        assert_eq!(
            build_inquiry_body(&["PUB_PACKET", "PUB_VARA"]).unwrap(),
            "PUB_PACKET\nPUB_VARA"
        );
        // Real N7CPZ outbox sample: 3TK09WKG9QBC.mime
        assert_eq!(
            build_inquiry_body(&["AZ_ZON_NOFLA", "CMS_TRAFFIC"]).unwrap(),
            "AZ_ZON_NOFLA\nCMS_TRAFFIC"
        );
    }

    #[test]
    fn filename_with_embedded_newline_is_rejected() {
        // A bad-catalog defense: filenames must be one-line.
        let err = build_inquiry_body(&["GOOD", "BAD\nINJECTED"]).unwrap_err();
        assert!(matches!(err, InquiryComposeError::FilenameContainsNewline(_)));
    }

    #[test]
    fn whitespace_only_filename_is_rejected() {
        let err = build_inquiry_body(&["   "]).unwrap_err();
        assert!(matches!(err, InquiryComposeError::EmptyFilename(_)));
    }

    #[test]
    fn compose_sets_canonical_headers() {
        let msg = compose_inquiry_message("N7CPZ", &["PUB_PACKET", "PUB_VARA"], 1_716_200_000).unwrap();
        assert_eq!(msg.header("Subject").unwrap(), INQUIRY_SUBJECT);
        // To: compose_message::normalize_address strips @winlink.org from
        // Winlink-internal addresses, so INQUIRY@winlink.org becomes the
        // bare canonical 'INQUIRY'. This matches the existing outgoing
        // convention used by message_send for all CALL@winlink.org targets.
        let tos = msg.header_all("To");
        assert_eq!(tos, vec!["INQUIRY"]);
        // From should reflect mycall (also normalized — bare upper).
        assert_eq!(msg.header("From").unwrap(), "N7CPZ");
    }

    #[test]
    fn compose_body_matches_n7cpz_outbox_golden() {
        // Golden: the actual body in 5YTNBV3JOZA8.mime is two filenames
        // joined by a line break. compose_message uses 8bit transfer-encoding
        // with raw text bodies (no quoted-printable rewriting here).
        let msg = compose_inquiry_message("N7CPZ", &["PUB_PACKET", "PUB_VARA"], 1_716_200_000).unwrap();
        let body = std::str::from_utf8(msg.body()).unwrap();
        assert!(body.contains("PUB_PACKET"), "body should contain PUB_PACKET");
        assert!(body.contains("PUB_VARA"), "body should contain PUB_VARA");
        // They should be on separate lines.
        let lines: Vec<&str> = body.lines().collect();
        assert!(lines.contains(&"PUB_PACKET"), "PUB_PACKET should be a standalone line: {body:?}");
        assert!(lines.contains(&"PUB_VARA"), "PUB_VARA should be a standalone line: {body:?}");
    }
}
