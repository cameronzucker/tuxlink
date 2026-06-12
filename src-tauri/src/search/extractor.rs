//! Extractor — `Message → IndexRow`
//!
//! Bridges the Winlink message layer and the FTS5 search index. Given a parsed
//! [`Message`] plus contextual metadata that only the mailbox layer knows
//! (folder, direction, unread flag, transport), `extract` returns an
//! [`IndexRow`] with every column the index's two tables need.

use crate::winlink::message::Message;
use crate::winlink_backend::MailboxFolder;

/// One row's worth of extracted fields — both FTS5 (`subject`/`body`/`form_field_values`)
/// and structured (`messages_meta`). The Index::upsert call writes both tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexRow {
    // FTS5 columns
    pub mid: String,
    pub folder: String,              // also a meta column; UNINDEXED in FTS table
    pub subject: String,             // FTS-indexed
    pub body: String,                // FTS-indexed (decoded text/plain)
    pub form_field_values: String,   // FTS-indexed (concatenation; empty if not a form)

    // messages_meta columns
    pub from_addr: Option<String>,
    pub to_addrs: Vec<String>,
    pub cc_addrs: Vec<String>,
    pub date_sent: Option<i64>,      // unix seconds UTC
    pub date_received: Option<i64>,  // unix seconds UTC
    pub unread: bool,
    pub form_type: Option<String>,
    pub has_attachments: bool,
    pub attachment_count: u32,
    pub transport_used: Option<String>,
    pub direction: Direction,
    pub message_size: u32,
    pub routing_path: Option<String>,
    /// The FULL callsign this message is tagged with (tuxlink-2ns7): the
    /// authoring identity for Sent/Outbox, the delivery namespace for received
    /// mail. `None` for untagged (legacy / pre-Phase-4) messages.
    pub identity_tag: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Sent,
    Received,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Direction::Sent => "sent",
            Direction::Received => "received",
        }
    }
}

/// Extract one IndexRow from a parsed Message + the folder it lives in. The
/// caller supplies `direction` because some folders (e.g. Archive) host either.
/// `unread` is supplied explicitly because read-state is a sidecar file the
/// extractor cannot see (it lives on the mailbox layer).
pub fn extract(
    msg: &Message,
    folder: MailboxFolder,
    direction: Direction,
    unread: bool,
    transport_used: Option<String>,
    identity_tag: Option<String>,
) -> IndexRow {
    let mid = msg.header("Mid").unwrap_or_default().to_string();
    let subject = msg.header("Subject").unwrap_or_default().to_string();
    let from_addr = msg.header("From").map(|s| s.to_string());
    let to_addrs: Vec<String> = msg
        .header_all("To")
        .iter()
        .map(|s| s.to_string())
        .collect();
    let cc_addrs: Vec<String> = msg
        .header_all("Cc")
        .iter()
        .map(|s| s.to_string())
        .collect();
    // body() returns &[u8]; convert lossy so Latin-1 messages don't panic.
    let body = String::from_utf8_lossy(msg.body()).into_owned();

    let folder_str = match folder {
        MailboxFolder::Inbox => "inbox",
        MailboxFolder::Outbox => "outbox",
        MailboxFolder::Sent => "sent",
        MailboxFolder::Archive => "archive",
    }
    .to_string();

    let date_unix = parse_winlink_date(msg.header("Date").unwrap_or_default());
    let (date_sent, date_received) = match direction {
        Direction::Sent => (date_unix, None),
        Direction::Received => (None, date_unix),
    };

    let (form_type, form_field_values) = sniff_form(&subject, &body);

    let attachment_count = msg.header_all("File").len() as u32;
    let has_attachments = attachment_count > 0;

    let routing_path = msg.header("Via").map(|s| s.to_string());
    let message_size = body.len() as u32;

    IndexRow {
        mid,
        folder: folder_str,
        subject,
        body,
        form_field_values,
        from_addr,
        to_addrs,
        cc_addrs,
        date_sent,
        date_received,
        unread,
        form_type,
        has_attachments,
        attachment_count,
        transport_used,
        direction,
        message_size,
        routing_path,
        identity_tag,
    }
}

/// Parse Winlink `YYYY/MM/DD HH:MM` into Unix seconds UTC. Returns None on any
/// shape that does not match.
fn parse_winlink_date(s: &str) -> Option<i64> {
    let (d, t) = s.split_once(' ')?;
    let parts: Vec<&str> = d.split('/').collect();
    if parts.len() != 3 {
        return None;
    }
    let year: i32 = parts[0].parse().ok()?;
    let month: u32 = parts[1].parse().ok()?;
    let day: u32 = parts[2].parse().ok()?;
    let (h, m) = t.split_once(':')?;
    let hour: u32 = h.parse().ok()?;
    let minute: u32 = m.parse().ok()?;
    // Days since 1970-01-01 — simple Gregorian calc; correct for 1970..2100.
    let days = days_from_civil(year, month as i32, day as i32);
    Some(days as i64 * 86_400 + hour as i64 * 3600 + minute as i64 * 60)
}

/// Howard Hinnant's `days_from_civil` (public-domain reference algorithm).
fn days_from_civil(y: i32, m: i32, d: i32) -> i32 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = (y - era * 400) as u32;
    let doy =
        ((153 * (m + (if m > 2 { -3 } else { 9 })) + 2) / 5 + d - 1) as u32;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe as i32 - 719_468
}

/// Cheap form-payload sniff. v0.1 recognizes payloads whose body or subject
/// begins with `FORM: <id>` — the convention emitted by ICS-213 forms in the
/// Winlink ecosystem. Returns (form_type, concatenated_values).
fn sniff_form(subject: &str, body: &str) -> (Option<String>, String) {
    // Body wins over subject. Look for "FORM: <id>\n" on its own line.
    let form_type = body
        .lines()
        .find_map(|l| l.strip_prefix("FORM:").map(|rest| rest.trim().to_string()))
        .or_else(|| {
            subject
                .strip_prefix("FORM: ")
                .map(|s| s.trim().to_string())
        });
    if form_type.is_none() {
        return (None, String::new());
    }

    // Concatenate every "Key: value" payload line for FTS indexing. Skip the
    // FORM: line itself.
    let values: Vec<String> = body
        .lines()
        .filter(|l| !l.starts_with("FORM:"))
        .filter_map(|l| l.split_once(':').map(|(_, v)| v.trim().to_string()))
        .filter(|v| !v.is_empty())
        .collect();

    // U+2503 separator unlikely to occur in payload text
    (form_type, values.join(" \u{2503} "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::winlink::compose::compose_message;

    fn parse(raw: &[u8]) -> Message {
        Message::from_bytes(raw).expect("fixture parses")
    }

    #[test]
    fn extracts_headers_from_a_plain_message() {
        let raw = compose_message(
            "N7CPZ",
            &["W1AW"],
            &[],
            "Hello",
            "Body text",
            1_716_200_000,
        )
        .to_bytes();
        let msg = parse(&raw);
        let row = extract(
            &msg,
            MailboxFolder::Inbox,
            Direction::Received,
            /*unread=*/ true,
            /*transport_used=*/ Some("telnet".into()),
            /*identity_tag=*/ None,
        );
        assert_eq!(row.subject, "Hello");
        assert_eq!(row.from_addr.as_deref(), Some("N7CPZ"));
        assert_eq!(row.to_addrs, vec!["W1AW".to_string()]);
        assert!(row.cc_addrs.is_empty());
        assert_eq!(row.body.trim(), "Body text");
        assert_eq!(row.folder, "inbox");
        assert!(row.unread);
        assert_eq!(row.transport_used.as_deref(), Some("telnet"));
        assert_eq!(row.direction, Direction::Received);
        assert!(!row.has_attachments);
        assert_eq!(row.attachment_count, 0);
        assert_eq!(row.form_type, None);
        assert_eq!(row.form_field_values, "");
        assert!(row.message_size > 0);
        assert!(!row.mid.is_empty());
    }

    #[test]
    fn extracts_form_payload_into_form_field_values() {
        // ICS-213-shaped fixture body. The extractor concatenates form-field
        // values into `form_field_values` for FTS5; the exact shape of "form
        // detection" is up to the implementer (header sniff + body sniff).
        let raw = compose_message(
            "KX5DD",
            &["N7CPZ"],
            &[],
            "DAMAGE REPORT",
            "FORM: ICS-213\nTO: Net Control\nFROM: KX5DD\nSUBJECT: Sector 7\nMSG: poles down\n",
            1_716_200_000,
        )
        .to_bytes();
        let msg = parse(&raw);
        let row = extract(&msg, MailboxFolder::Inbox, Direction::Received, true, None, None);
        assert_eq!(row.form_type.as_deref(), Some("ICS-213"));
        assert!(row.form_field_values.contains("Net Control"));
        assert!(row.form_field_values.contains("KX5DD"));
        assert!(row.form_field_values.contains("Sector 7"));
        assert!(row.form_field_values.contains("poles down"));
    }

    #[test]
    fn date_received_set_for_received_direction_only() {
        let raw = compose_message(
            "N7CPZ",
            &["W1AW"],
            &[],
            "x",
            "y",
            1_716_200_000,
        )
        .to_bytes();
        let msg = parse(&raw);
        let recv = extract(&msg, MailboxFolder::Inbox, Direction::Received, true, None, None);
        let sent = extract(&msg, MailboxFolder::Sent, Direction::Sent, false, None, None);
        assert!(recv.date_received.is_some());
        assert!(sent.date_sent.is_some());
    }
}

/// Strip markdown syntax for FTS5 ingestion (tuxlink-0gsy / spec §9.1).
/// Conservative parse — handles the subset that docs/user-guide/*.md uses:
/// ATX headings, bold (`**...**`), italic (`_..._`), inline code
/// (`` `...` ``), links (`[text](url)`), fenced code blocks
/// (```` ```...``` ````), and unordered list markers (`-`, `*`).
///
/// Output preserves linebreaks. URLs are dropped; link text is kept.
pub fn extract_markdown(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    let mut in_code_fence = false;
    for raw_line in md.lines() {
        let line = raw_line.trim_end();
        // Fenced code-block toggle.
        if line.trim_start().starts_with("```") {
            in_code_fence = !in_code_fence;
            continue;
        }
        if in_code_fence {
            if !out.is_empty() { out.push('\n'); }
            out.push_str(line);
            continue;
        }
        // Strip leading ATX-heading marker(s) + indentation.
        let mut s = line.trim_start();
        while s.starts_with('#') { s = &s[1..]; }
        s = s.trim_start();
        // Strip leading unordered-list marker.
        if let Some(rest) = s.strip_prefix("- ") {
            s = rest;
        } else if let Some(rest) = s.strip_prefix("* ") {
            s = rest;
        }
        let stripped = strip_inline_md(s);
        if !out.is_empty() { out.push('\n'); }
        out.push_str(&stripped);
    }
    while out.ends_with('\n') { out.pop(); }
    out
}

/// Strip inline `**bold**`, `_italic_`, `` `code` ``, and `[text](url)` →
/// `text`. Left-to-right, first-match-wins so nested syntax falls through
/// naturally.
fn strip_inline_md(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        // Link: [text](url)
        if bytes[i] == b'[' {
            if let Some(close_text) = find_byte_md(&bytes[i..], b']') {
                let after_text = i + close_text + 1;
                if after_text < bytes.len() && bytes[after_text] == b'(' {
                    if let Some(close_url) = find_byte_md(&bytes[after_text..], b')') {
                        let text = &input[i + 1..i + close_text];
                        out.push_str(text);
                        i = after_text + close_url + 1;
                        continue;
                    }
                }
            }
        }
        // Bold: **text**
        if i + 1 < bytes.len() && &bytes[i..i + 2] == b"**" {
            if let Some(close) = find_seq_md(&bytes[i + 2..], b"**") {
                let text = &input[i + 2..i + 2 + close];
                out.push_str(text);
                i = i + 2 + close + 2;
                continue;
            }
        }
        // Inline code: `text`
        if bytes[i] == b'`' {
            if let Some(close) = find_byte_md(&bytes[i + 1..], b'`') {
                let text = &input[i + 1..i + 1 + close];
                out.push_str(text);
                i = i + 1 + close + 1;
                continue;
            }
        }
        // Italic: _text_ (only at word boundaries so identifiers like `foo_bar` survive)
        if bytes[i] == b'_' && (i == 0 || bytes[i - 1] == b' ') {
            if let Some(close) = find_byte_md(&bytes[i + 1..], b'_') {
                let text = &input[i + 1..i + 1 + close];
                out.push_str(text);
                i = i + 1 + close + 1;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn find_byte_md(s: &[u8], b: u8) -> Option<usize> {
    s.iter().position(|&x| x == b)
}
fn find_seq_md(s: &[u8], seq: &[u8]) -> Option<usize> {
    if seq.is_empty() || s.len() < seq.len() { return None; }
    for i in 0..=s.len() - seq.len() {
        if &s[i..i + seq.len()] == seq { return Some(i); }
    }
    None
}

#[cfg(test)]
mod markdown_tests {
    use super::*;

    #[test]
    fn strips_h1_h2_h3_markers_keeps_text() {
        assert_eq!(
            extract_markdown("# Heading 1\n## Heading 2\n### Heading 3"),
            "Heading 1\nHeading 2\nHeading 3",
        );
    }

    #[test]
    fn strips_bold_italic_code_inline_formatting() {
        assert_eq!(
            extract_markdown("**bold** _italic_ `code`"),
            "bold italic code",
        );
    }

    #[test]
    fn link_text_preserved_url_stripped() {
        assert_eq!(
            extract_markdown("See [the mailbox](03-mailbox.md) for details."),
            "See the mailbox for details.",
        );
    }

    #[test]
    fn fenced_code_block_inlined_as_text() {
        let md = "```bash\nfoo --bar\n```";
        let out = extract_markdown(md);
        assert!(out.contains("foo --bar"));
        assert!(!out.contains("```"));
    }

    #[test]
    fn unordered_list_markers_dropped() {
        assert_eq!(
            extract_markdown("- item one\n- item two"),
            "item one\nitem two",
        );
    }

    #[test]
    fn empty_input_returns_empty() {
        assert_eq!(extract_markdown(""), "");
    }
}
