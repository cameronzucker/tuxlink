// Tests for tuxlink-zsm (Task 12) + tuxlink-y5c (Task 13) — UI command layer.
//
// Task 12: UiError projection, MessageMetaDto mapping (incl. To +
// hasAttachments graceful degradation), folder parsing, and the
// read_message_in folder generalization.
//
// Task 13: RFC5322 parse at the message_read command boundary via
// parse_raw_rfc5322 (spec §5.3). Covers multipart attachments, Winlink
// form detection, non-UTF-8 lossy decode, NotFound + oversized-input paths.
//
// Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §2, §3, §5.3, §6
//
// These exercise the public surface that the Tauri commands are built from.
// Command fns take `tauri::State`, which can't be constructed without a
// running Tauri app, so their logic is covered via their component functions
// here + the BackendState unit tests in-crate; the live IPC round-trip is
// smoke-verified at M2 (testing-pitfalls: static tests verify logic, not
// rendered widgets).

use tuxlink_lib::ui_commands::{parse_folder, parse_raw_rfc5322, MessageMetaDto, ParsedMessageDto, UiError};
use tuxlink_lib::winlink_backend::{
    BackendError, MailboxFolder, MessageId, PatBackend, WinlinkBackend,
};

// ============================================================================
// Task-12 test (1): mailbox_list maps MessageMeta → DTO incl. to + hasAttachments
// (mockito Pat fixture). Drives the list end-to-end through PatBackend +
// the DTO mapping the command uses.
// ============================================================================
#[tokio::test]
async fn test_list_maps_meta_to_dto_including_to_and_has_attachments() {
    let mut server = mockito::Server::new_async().await;
    // Pat 1.0.0's REAL list shape — no To, no attachment field. Verifies the
    // graceful-degradation default (to=[], hasAttachments=false).
    let _mock = server
        .mock("GET", "/api/mailbox/in")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"[{"MID":"ABC123","Subject":"Test","From":{"Addr":"W4PHS@winlink.org"},"Date":"2026-04-22T15:00:00Z","Unread":true,"BodySize":42}]"#,
        )
        .create_async()
        .await;

    let backend = PatBackend::from_url(server.url());
    let metas = backend
        .list_messages(MailboxFolder::Inbox)
        .await
        .expect("list_messages");
    let dtos: Vec<MessageMetaDto> = metas.into_iter().map(MessageMetaDto::from).collect();

    assert_eq!(dtos.len(), 1);
    assert_eq!(dtos[0].id, "ABC123");
    assert_eq!(dtos[0].subject, "Test");
    assert_eq!(dtos[0].from, "W4PHS@winlink.org");
    assert!(dtos[0].unread);
    assert_eq!(dtos[0].body_size, 42);
    // Graceful degradation: Pat omits these, so they default.
    assert_eq!(dtos[0].to, Vec::<String>::new(), "Pat list DTO omits To → empty");
    assert!(!dtos[0].has_attachments, "Pat list DTO omits attachments → false");
}

// ============================================================================
// Forward-compat: when a backend DOES provide To + a Files array, the DTO
// carries them. Proves the degradation is graceful (default), not hardcoded.
// ============================================================================
#[tokio::test]
async fn test_list_populates_to_and_has_attachments_when_pat_provides_them() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("GET", "/api/mailbox/sent")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"[{"MID":"S1","Subject":"Re: ICS-213","From":{"Addr":"KK4XYZ@winlink.org"},"To":[{"Addr":"W4PHS@winlink.org"},{"Addr":"N0CALL@winlink.org"}],"Date":"2026-05-19T12:00:00Z","Unread":false,"BodySize":900,"Files":[{"Name":"form.xml"}]}]"#,
        )
        .create_async()
        .await;

    let backend = PatBackend::from_url(server.url());
    let metas = backend
        .list_messages(MailboxFolder::Sent)
        .await
        .expect("list sent");
    let dtos: Vec<MessageMetaDto> = metas.into_iter().map(MessageMetaDto::from).collect();

    assert_eq!(dtos.len(), 1);
    assert_eq!(
        dtos[0].to,
        vec!["W4PHS@winlink.org".to_string(), "N0CALL@winlink.org".to_string()]
    );
    assert!(dtos[0].has_attachments, "non-empty Files array → hasAttachments true");
}

// ============================================================================
// Task-12 test (2): folder string parse — drafts/deleted rejected/handled,
// real folders map, unknown rejected.
// ============================================================================
#[test]
fn test_parse_folder_maps_backend_folders() {
    assert!(matches!(parse_folder("inbox"), Ok(MailboxFolder::Inbox)));
    assert!(matches!(parse_folder("outbox"), Ok(MailboxFolder::Outbox)));
    assert!(matches!(parse_folder("sent"), Ok(MailboxFolder::Sent)));
    assert!(matches!(parse_folder("archive"), Ok(MailboxFolder::Archive)));
}

#[test]
fn test_parse_folder_rejects_local_and_disabled_folders() {
    // Drafts is local (localStorage), never a backend command.
    match parse_folder("drafts") {
        Err(UiError::Internal { detail }) => assert!(detail.contains("local")),
        other => panic!("expected Internal for drafts, got {other:?}"),
    }
    // Deleted is a disabled placeholder in v0.0.1.
    match parse_folder("deleted") {
        Err(UiError::Unavailable { reason }) => assert!(reason.contains("Deleted")),
        other => panic!("expected Unavailable for deleted, got {other:?}"),
    }
    // Unknown folder string.
    match parse_folder("garbage") {
        Err(UiError::Internal { detail }) => assert!(detail.contains("unknown")),
        other => panic!("expected Internal for unknown, got {other:?}"),
    }
}

// ============================================================================
// Task-12 test (7): read_message_in(Inbox, id) == old read_message(id)
// (back-compat). The trait's default read_message forwards to
// read_message_in(Inbox, id); both must hit the same bytes.
// ============================================================================
#[tokio::test]
async fn test_read_message_in_inbox_matches_read_message_back_compat() {
    let mut server = mockito::Server::new_async().await;
    let body = b"Subject: hi\r\n\r\nbody";
    let _mock = server
        .mock("GET", "/api/mailbox/in/MID1")
        .with_status(200)
        .with_body(body.as_slice())
        // Both calls hit the same inbox URL; expect 2 requests.
        .expect(2)
        .create_async()
        .await;

    let backend = PatBackend::from_url(server.url());
    let id = MessageId::new("MID1");

    let via_in = backend
        .read_message_in(MailboxFolder::Inbox, &id)
        .await
        .expect("read_message_in");
    let via_compat = backend.read_message(&id).await.expect("read_message");

    assert_eq!(via_in.raw_rfc5322, body);
    assert_eq!(via_compat.raw_rfc5322, via_in.raw_rfc5322);
    assert_eq!(via_compat.id, via_in.id);
}

// ============================================================================
// read_message_in reads from the requested folder (not always Inbox) — the
// whole point of the generalization. A Sent read must hit /api/mailbox/sent.
// ============================================================================
#[tokio::test]
async fn test_read_message_in_uses_requested_folder() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("GET", "/api/mailbox/sent/SENTMID")
        .with_status(200)
        .with_body(b"sent body".as_slice())
        .create_async()
        .await;

    let backend = PatBackend::from_url(server.url());
    let body = backend
        .read_message_in(MailboxFolder::Sent, &MessageId::new("SENTMID"))
        .await
        .expect("read sent");
    assert_eq!(body.raw_rfc5322, b"sent body");
}

// ============================================================================
// Task-12 test (8) component: UiError projection is exhaustive + correct for
// the variants the command surfaces. (The BackendState None → NotConfigured
// path is unit-tested in-crate in app_backend.rs; here we verify the error
// MAPPING that the command applies via `?`.)
// ============================================================================
#[test]
fn test_ui_error_maps_all_backend_error_variants() {
    use std::io::{Error as IoError, ErrorKind};

    assert_eq!(
        UiError::from(BackendError::NotConfigured("offline".into())),
        UiError::NotConfigured("offline".into())
    );
    assert_eq!(
        UiError::from(BackendError::NotFound(MessageId::new("X"))),
        UiError::NotFound("X".into())
    );
    assert_eq!(
        UiError::from(BackendError::AuthFailed { reason: "401".into() }),
        UiError::AuthFailed { reason: "401".into() }
    );
    assert_eq!(
        UiError::from(BackendError::MessageRejected("bad".into())),
        UiError::Rejected("bad".into())
    );
    assert_eq!(UiError::from(BackendError::Cancelled), UiError::Cancelled);
    // NotImplemented → Unavailable with the canonical v0.0.1 reason.
    assert_eq!(
        UiError::from(BackendError::NotImplemented),
        UiError::Unavailable { reason: "not implemented in v0.0.1".into() }
    );
    // InvalidSession → Internal (Codex finding 6 — must not be dropped).
    assert_eq!(
        UiError::from(BackendError::InvalidSession),
        UiError::Internal { detail: "invalid session".into() }
    );
    // Io → Internal carrying the io error's Display.
    match UiError::from(BackendError::Io(IoError::new(ErrorKind::Other, "disk gone"))) {
        UiError::Internal { detail } => assert!(detail.contains("disk gone")),
        other => panic!("expected Internal for Io, got {other:?}"),
    }
    // TransportFailed → Transport, folding the source chain into reason.
    match UiError::from(BackendError::TransportFailed {
        reason: "timeout".into(),
        source: Some(Box::new(IoError::new(ErrorKind::TimedOut, "deadline"))),
    }) {
        UiError::Transport { reason } => {
            assert!(reason.contains("timeout"));
            assert!(reason.contains("deadline"), "source chain folded in: {reason}");
        }
        other => panic!("expected Transport, got {other:?}"),
    }
    // BackendUnavailable → Unavailable.
    match UiError::from(BackendError::BackendUnavailable {
        reason: "no sidecar".into(),
        source: None,
    }) {
        UiError::Unavailable { reason } => assert!(reason.contains("no sidecar")),
        other => panic!("expected Unavailable, got {other:?}"),
    }
    // Internal → Internal, folding source.
    match UiError::from(BackendError::Internal { msg: "boom".into(), source: None }) {
        UiError::Internal { detail } => assert_eq!(detail, "boom"),
        other => panic!("expected Internal, got {other:?}"),
    }
}

// ============================================================================
// UiError serializes with the tag/content shape the TS UiError union mirrors
// (spec §3.1). A struct-variant nests under "detail"; a newtype variant puts
// the scalar directly in "detail".
// ============================================================================
#[test]
fn test_ui_error_serializes_with_tag_content_shape() {
    let auth = serde_json::to_value(UiError::AuthFailed { reason: "401".into() }).unwrap();
    assert_eq!(auth["kind"], "AuthFailed");
    assert_eq!(auth["detail"]["reason"], "401");

    let nf = serde_json::to_value(UiError::NotFound("MID9".into())).unwrap();
    assert_eq!(nf["kind"], "NotFound");
    assert_eq!(nf["detail"], "MID9");

    let cancelled = serde_json::to_value(UiError::Cancelled).unwrap();
    assert_eq!(cancelled["kind"], "Cancelled");
}

// NOTE: MessageMetaDto camelCase-serialization is tested in-crate (in
// `ui_commands.rs`'s `#[cfg(test)]` module) because `MessageMeta` is
// `#[non_exhaustive]` and cannot be struct-literal-constructed from this
// external integration-test crate. The DTO mapping from a real backend is
// exercised end-to-end by `test_list_maps_meta_to_dto_*` above.

// ============================================================================
// Task-13 tests (tuxlink-y5c) — RFC5322 parse at the command boundary
// Spec §5.3, §6 Task 13
// ============================================================================

/// Build a minimal RFC5322 message from parts. `headers` is a slice of
/// `(Name, Value)` pairs; `body` is the text/plain body (may be empty).
fn simple_rfc5322(headers: &[(&str, &str)], body: &str) -> Vec<u8> {
    let mut out = String::new();
    for (name, val) in headers {
        out.push_str(&format!("{name}: {val}\r\n"));
    }
    out.push_str("\r\n");
    out.push_str(body);
    out.into_bytes()
}

// ============================================================================
// Task-13 test (1): simple RFC5322 message parses subject/from/to/cc/date/body
// ============================================================================
#[test]
fn test_parse_rfc5322_extracts_headers_and_body() {
    let raw = simple_rfc5322(
        &[
            ("Message-ID", "<MID1@winlink.org>"),
            ("Subject", "ICS-213 Test"),
            ("From", "W4PHS@winlink.org"),
            ("To", "KK4XYZ@winlink.org, N0CALL@winlink.org"),
            ("Cc", "W1AW@winlink.org"),
            ("Date", "Mon, 19 May 2026 14:05:00 +0000"),
        ],
        "Hello from the ARES net.",
    );

    let dto = parse_raw_rfc5322("MID1", &raw).expect("parse should succeed");
    assert_eq!(dto.id, "MID1");
    assert_eq!(dto.subject, "ICS-213 Test");
    assert_eq!(dto.from, "W4PHS@winlink.org");
    assert!(dto.to.iter().any(|a| a.contains("KK4XYZ")), "to: {:?}", dto.to);
    assert!(dto.to.iter().any(|a| a.contains("N0CALL")), "to: {:?}", dto.to);
    assert!(dto.cc.iter().any(|a| a.contains("W1AW")), "cc: {:?}", dto.cc);
    assert!(dto.body.contains("Hello from the ARES net."));
    assert!(!dto.is_form);
    assert!(dto.attachments.is_empty());
}

// ============================================================================
// Task-13 test (2): multipart message → attachment names listed + text/plain body
// ============================================================================
#[test]
fn test_parse_multipart_lists_attachment_names_and_body() {
    // A MIME multipart/mixed message with text/plain + an attachment.
    let raw = b"Message-ID: <MID2@winlink.org>\r\n\
Subject: Net Report\r\n\
From: KK4XYZ@winlink.org\r\n\
To: W4PHS@winlink.org\r\n\
Date: Mon, 19 May 2026 15:00:00 +0000\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=\"boundary42\"\r\n\
\r\n\
--boundary42\r\n\
Content-Type: text/plain; charset=us-ascii\r\n\
\r\n\
ARES net check-in log.\r\n\
--boundary42\r\n\
Content-Type: application/octet-stream\r\n\
Content-Disposition: attachment; filename=\"net_log.txt\"\r\n\
Content-Transfer-Encoding: base64\r\n\
\r\n\
aGVsbG8=\r\n\
--boundary42--\r\n";

    let dto = parse_raw_rfc5322("MID2", raw).expect("multipart parse");
    assert!(dto.body.contains("ARES net check-in log."), "body: {:?}", dto.body);
    assert_eq!(dto.attachments.len(), 1, "one attachment");
    assert_eq!(dto.attachments[0].filename, "net_log.txt");
    assert!(!dto.is_form);
}

// ============================================================================
// Task-13 test (3): Winlink form payload (`<?xml`) → isForm true
// ============================================================================
#[test]
fn test_parse_form_payload_sets_is_form() {
    let xml_body = "<?xml version=\"1.0\"?><ICS213><Msg>Test</Msg></ICS213>";
    let raw = simple_rfc5322(
        &[
            ("Message-ID", "<FORMID@winlink.org>"),
            ("Subject", "ICS-213 (Winlink)"),
            ("From", "W4PHS@winlink.org"),
            ("To", "KK4XYZ@winlink.org"),
            ("Date", "Mon, 19 May 2026 14:00:00 +0000"),
            ("Content-Type", "text/plain; charset=utf-8"),
        ],
        xml_body,
    );

    let dto = parse_raw_rfc5322("FORMID", &raw).expect("form parse");
    assert!(dto.is_form, "XML-starting body must be flagged as a form");
}

// ============================================================================
// Task-13 test (4): non-UTF-8 bytes → lossy decode, no panic
// ============================================================================
#[test]
fn test_parse_non_utf8_body_decodes_lossily_no_panic() {
    // Build a valid RFC5322 structure with invalid UTF-8 in the body.
    let mut raw = b"Message-ID: <BADENC@winlink.org>\r\nSubject: S\r\nFrom: A\r\nTo: B\r\nDate: Mon, 19 May 2026 00:00:00 +0000\r\n\r\n".to_vec();
    raw.extend_from_slice(b"Good prefix ");
    raw.extend_from_slice(&[0xff, 0xfe]); // invalid UTF-8
    raw.extend_from_slice(b" tail");

    // Should not panic; lossy decode replaces the invalid bytes with U+FFFD.
    let dto = parse_raw_rfc5322("BADENC", &raw).expect("non-UTF-8 must not error");
    assert!(
        dto.body.contains("Good prefix"),
        "body prefix present: {:?}",
        dto.body
    );
    assert!(
        dto.body.contains('\u{FFFD}') || dto.body.contains("tail"),
        "lossy replacement or tail present: {:?}",
        dto.body
    );
}

// ============================================================================
// Task-13 test (5): message_read on missing MID → NotFound via PatBackend
// ============================================================================
#[tokio::test]
async fn test_message_read_missing_mid_returns_not_found() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("GET", "/api/mailbox/in/MISSINGMID")
        .with_status(404)
        .create_async()
        .await;

    let backend = tuxlink_lib::winlink_backend::PatBackend::from_url(server.url());
    let result = backend
        .read_message_in(
            tuxlink_lib::winlink_backend::MailboxFolder::Inbox,
            &tuxlink_lib::winlink_backend::MessageId::new("MISSINGMID"),
        )
        .await;

    match result {
        Err(tuxlink_lib::winlink_backend::BackendError::NotFound(_)) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}

// ============================================================================
// Task-13 test (6): oversized input → parse_raw_rfc5322 returns UiError
// Spec §5.3 (Codex verdict V3): cap parse input; surface a parser-failure state.
// ============================================================================
#[test]
fn test_parse_oversized_input_returns_error() {
    // 5 MiB of zeros — well over the parse size cap.
    let huge: Vec<u8> = vec![b'X'; 5 * 1024 * 1024];
    let result = parse_raw_rfc5322("BIG", &huge);
    match result {
        Err(UiError::Internal { .. }) => {}
        other => panic!("expected Internal UiError for oversized input, got {other:?}"),
    }
}

// ============================================================================
// ParsedMessageDto serializes camelCase (isForm, attachments) — mirrors
// ParsedMessage in src/mailbox/types.ts.
// ============================================================================
#[test]
fn test_parsed_message_dto_serializes_camel_case() {
    let dto = ParsedMessageDto {
        id: "MID3".into(),
        subject: "S".into(),
        from: "F".into(),
        to: vec!["T".into()],
        cc: vec![],
        date: "2026-05-19T00:00:00Z".into(),
        body: "B".into(),
        attachments: vec![tuxlink_lib::ui_commands::AttachmentMetaDto {
            filename: "file.txt".into(),
            size: 42,
        }],
        is_form: false,
        routing: None,
    };
    let v = serde_json::to_value(&dto).unwrap();
    assert_eq!(v["isForm"], false);
    assert_eq!(v["attachments"][0]["filename"], "file.txt");
    assert_eq!(v["attachments"][0]["size"], 42);
    assert_eq!(v["routing"], serde_json::Value::Null);
}
