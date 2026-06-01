//! Submit-body parser for HTML Forms — handles
//! `application/x-www-form-urlencoded` and `multipart/form-data` while
//! preserving repeated field names (WLE table rows / checkbox groups) and
//! identifying the submitter button (WLE distinguishes Submit vs Cancel
//! via a `name="Submit"` button value).
//!
//! Per the 2026-06-01 WLE snapshot recon, the stock Standard Forms POST
//! `multipart/form-data` (with `enctype="multipart/form-data"` on the
//! `<form>` element). urlencoded handling is for defense / completeness
//! and is exercised by some custom-form templates.
//!
//! Design reference: §5.3 (hardening + Codex adrev).
//! Plan: docs/superpowers/plans/2026-06-01-html-forms-p1-webview-infra.md
//!       Task 5.

use std::collections::HashMap;

use bytes::Bytes;
use multer::Multipart;
use percent_encoding::percent_decode_str;

/// Parsed body — fields plus an optional submitter button value.
#[derive(Debug, Default, Clone)]
pub struct ParsedBody {
    /// Field name → ordered list of values. Repeated names (table rows,
    /// checkbox groups) keep their submission order.
    pub fields: HashMap<String, Vec<String>>,
    /// Submitter button value if a button with `name="Submit"` (or
    /// equivalent — see [`SUBMITTER_FIELD_NAMES`]) was clicked.
    /// `None` for programmatic submits.
    pub submitter: Option<String>,
}

/// Field names that, when present in the submitted body, identify a
/// submitter button. WLE templates typically use `Submit` (capitalized);
/// custom forms sometimes use `submit` (lowercase) or `Send`.
const SUBMITTER_FIELD_NAMES: &[&str] = &["Submit", "submit", "Send"];

fn record_submitter_if_match(out: &mut ParsedBody, name: &str, value: &str) {
    if out.submitter.is_some() {
        return;
    }
    if SUBMITTER_FIELD_NAMES.iter().any(|n| *n == name) {
        out.submitter = Some(value.to_string());
    }
}

/// Parse an `application/x-www-form-urlencoded` body. Preserves repeated
/// names and decodes `%XX` escapes plus `+` → space.
pub fn parse_urlencoded(body: &str) -> Result<ParsedBody, String> {
    let mut out = ParsedBody::default();
    for pair in body.split('&') {
        if pair.is_empty() {
            continue;
        }
        let mut iter = pair.splitn(2, '=');
        let raw_key = iter.next().unwrap_or("");
        let raw_val = iter.next().unwrap_or("");
        let key = percent_decode_str(&raw_key.replace('+', " "))
            .decode_utf8_lossy()
            .into_owned();
        let val = percent_decode_str(&raw_val.replace('+', " "))
            .decode_utf8_lossy()
            .into_owned();
        record_submitter_if_match(&mut out, &key, &val);
        out.fields.entry(key).or_default().push(val);
    }
    Ok(out)
}

/// Parse a `multipart/form-data` body. `boundary` is the value from the
/// Content-Type header.
///
/// Preserves repeated names and surfaces the submitter button value. File
/// uploads are recorded as text-only (the bytes are coerced via
/// [`multer::Field::text`]); the WLE stock forms do not use `type="file"`
/// inputs, so this is fine for now. The plan notes file-upload support as
/// a follow-up bd if a custom form ever needs it.
pub async fn parse_multipart(boundary: &str, body: Bytes) -> Result<ParsedBody, String> {
    let mut out = ParsedBody::default();
    let stream = futures::stream::once(async move { Ok::<_, std::io::Error>(body) });
    let mut mp = Multipart::new(stream, boundary);
    while let Some(field) = mp.next_field().await.map_err(|e| e.to_string())? {
        let name = field.name().unwrap_or("").to_string();
        let val = field.text().await.map_err(|e| e.to_string())?;
        record_submitter_if_match(&mut out, &name, &val);
        out.fields.entry(name).or_default().push(val);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[test]
    fn urlencoded_preserves_repeated_names() {
        let parsed = parse_urlencoded(
            "Subject=Test&Body=hello&Name=W6ABC&Name=W7DEF&Submit=Submit",
        )
        .unwrap();
        assert_eq!(parsed.fields["Subject"], vec!["Test".to_string()]);
        // Two `Name` entries must round-trip as a 2-element vec, not coalesce.
        assert_eq!(
            parsed.fields["Name"],
            vec!["W6ABC".to_string(), "W7DEF".to_string()]
        );
        assert_eq!(parsed.submitter, Some("Submit".to_string()));
    }

    #[test]
    fn urlencoded_handles_url_escapes() {
        let parsed = parse_urlencoded("Subject=hello%20world&Body=line1%0Aline2").unwrap();
        assert_eq!(parsed.fields["Subject"][0], "hello world");
        assert_eq!(parsed.fields["Body"][0], "line1\nline2");
    }

    #[test]
    fn urlencoded_handles_plus_as_space() {
        let parsed = parse_urlencoded("Subject=hello+world").unwrap();
        assert_eq!(parsed.fields["Subject"][0], "hello world");
    }

    #[test]
    fn urlencoded_handles_empty_body() {
        let parsed = parse_urlencoded("").unwrap();
        assert!(parsed.fields.is_empty());
        assert!(parsed.submitter.is_none());
    }

    #[test]
    fn urlencoded_recognizes_lowercase_submit_button() {
        // Some custom-form templates use lowercase `submit` instead of
        // WLE's stock `Submit`.
        let parsed = parse_urlencoded("Body=x&submit=Send").unwrap();
        assert_eq!(parsed.submitter, Some("Send".to_string()));
    }

    #[test]
    fn urlencoded_recognizes_send_button() {
        let parsed = parse_urlencoded("Body=x&Send=Go").unwrap();
        assert_eq!(parsed.submitter, Some("Go".to_string()));
    }

    #[test]
    fn urlencoded_ignores_value_only_pair() {
        // A trailing `&value` (no `=`) should not crash; the key is "value"
        // and the value is "".
        let parsed = parse_urlencoded("Body=x&trailing").unwrap();
        assert_eq!(parsed.fields["Body"][0], "x");
        assert_eq!(parsed.fields["trailing"][0], "");
    }

    #[tokio::test]
    async fn multipart_preserves_repeated_names_and_submitter() {
        let boundary = "----testboundary";
        let body = format!(
            "--{b}\r\nContent-Disposition: form-data; name=\"Subject\"\r\n\r\nT\r\n\
             --{b}\r\nContent-Disposition: form-data; name=\"Name\"\r\n\r\nA\r\n\
             --{b}\r\nContent-Disposition: form-data; name=\"Name\"\r\n\r\nB\r\n\
             --{b}\r\nContent-Disposition: form-data; name=\"Submit\"\r\n\r\nSend\r\n\
             --{b}--\r\n",
            b = boundary
        );
        let parsed = parse_multipart(boundary, Bytes::from(body)).await.unwrap();
        assert_eq!(parsed.fields["Subject"][0], "T");
        assert_eq!(parsed.fields["Name"], vec!["A".to_string(), "B".to_string()]);
        assert_eq!(parsed.submitter, Some("Send".to_string()));
    }

    #[tokio::test]
    async fn multipart_handles_single_field() {
        let boundary = "----b";
        let body = format!(
            "--{b}\r\nContent-Disposition: form-data; name=\"Only\"\r\n\r\nvalue\r\n--{b}--\r\n",
            b = boundary
        );
        let parsed = parse_multipart(boundary, Bytes::from(body)).await.unwrap();
        assert_eq!(parsed.fields["Only"][0], "value");
        assert!(parsed.submitter.is_none());
    }

    #[tokio::test]
    async fn multipart_handles_unicode_field_value() {
        let boundary = "----b";
        let body = format!(
            "--{b}\r\nContent-Disposition: form-data; name=\"Subject\"\r\n\r\nGr\u{00fc}\u{00df}e\r\n--{b}--\r\n",
            b = boundary
        );
        let parsed = parse_multipart(boundary, Bytes::from(body)).await.unwrap();
        assert_eq!(parsed.fields["Subject"][0], "Gr\u{00fc}\u{00df}e");
    }
}
