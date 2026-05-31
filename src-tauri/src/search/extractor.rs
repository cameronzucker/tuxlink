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
        assert_eq!(row.has_attachments, false);
        assert_eq!(row.attachment_count, 0);
        assert_eq!(row.form_type, None);
        assert_eq!(row.form_field_values, "");
        assert!(row.message_size > 0);
        assert_eq!(row.mid.is_empty(), false);
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
        let row = extract(&msg, MailboxFolder::Inbox, Direction::Received, true, None);
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
        let recv = extract(&msg, MailboxFolder::Inbox, Direction::Received, true, None);
        let sent = extract(&msg, MailboxFolder::Sent, Direction::Sent, false, None);
        assert!(recv.date_received.is_some());
        assert!(sent.date_sent.is_some());
    }
}
