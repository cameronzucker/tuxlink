// PatClient is async (per tuxlink-z5f impl-phase discovery: reqwest::blocking
// spawns an inner tokio runtime that panics if dropped in async test context).
// All HTTP-touching tests use #[tokio::test] + Server::new_async + .await.

use tuxlink_lib::pat_client::{MailboxFolder, PatClient, PatClientError};

#[tokio::test]
async fn test_list_inbox_parses_pat_json() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server.mock("GET", "/api/mailbox/in")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"[{"MID":"ABC123","Subject":"Test","From":{"Addr":"W4PHS@winlink.org"},"Date":"2026-04-22T15:00:00Z","Unread":true,"BodySize":42}]"#)
        .create_async().await;

    let client = PatClient::new(server.url());
    let messages = client.list(MailboxFolder::Inbox).await.expect("list inbox");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].mid, "ABC123");
    assert_eq!(messages[0].subject, "Test");
    assert_eq!(messages[0].from, "W4PHS@winlink.org");
    assert!(messages[0].unread);
}

#[tokio::test]
async fn test_list_inbox_surfaces_http_error() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server.mock("GET", "/api/mailbox/in").with_status(500).create_async().await;
    let client = PatClient::new(server.url());
    assert!(client.list(MailboxFolder::Inbox).await.is_err());
}

#[tokio::test]
async fn test_send_message_posts_multipart_form_data() {
    // Pat's POST /api/mailbox/out expects multipart/form-data with lowercase
    // fields to/subject/body/date and returns plain text "Message posted (X.XX kB)"
    // HTTP 201 — NOT JSON. Verified against la5nta/pat handler 2026-05-18 by
    // willow-raven-arroyo + Pat-API verification subagent; matches both v1.0.0
    // and master byte-for-byte.
    let mut server = mockito::Server::new_async().await;
    let _mock = server.mock("POST", "/api/mailbox/out")
        .match_header("content-type", mockito::Matcher::Regex("^multipart/form-data".to_string()))
        .with_status(201)
        .with_body("Message posted (0.05 kB)")
        .create_async().await;
    let client = PatClient::new(server.url());
    client.send(&["SERVICE@winlink.org"], "Test", "body", "2026-04-22T15:00:00Z").await.expect("send");
}

// ============================================================================
// tuxlink-z5f Phase 0 prereqs (spec §3.8.0): PatClient::read + Clone + MailboxFolder derives
// ============================================================================

#[tokio::test]
async fn test_read_returns_body_bytes_for_utf8_body() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/api/mailbox/in/MID123")
        .with_status(200)
        .with_header("content-type", "text/plain")
        .with_body(b"raw body bytes")
        .create_async().await;
    let client = PatClient::new(server.url());
    let result = client.read(MailboxFolder::Inbox, "MID123").await.expect("read");
    assert_eq!(result, b"raw body bytes");
    mock.assert_async().await;
}

#[tokio::test]
async fn test_read_preserves_non_utf8_bytes() {
    // Validates v2 P0 #2: byte-fidelity at the read boundary so the trait's
    // MessageBody.raw_rfc5322: Vec<u8> can carry MIME attachments unmolested.
    let mut server = mockito::Server::new_async().await;
    let body: Vec<u8> = vec![0x48, 0x69, 0xff, 0xfe, 0x00, 0xff];
    let mock = server
        .mock("GET", "/api/mailbox/in/MID456")
        .with_status(200)
        .with_body(body.clone())
        .create_async().await;
    let client = PatClient::new(server.url());
    let result = client.read(MailboxFolder::Inbox, "MID456").await.expect("read");
    assert_eq!(result, body);
    mock.assert_async().await;
}

#[tokio::test]
async fn test_read_404_returns_status_error() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("GET", "/api/mailbox/in/MISSING")
        .with_status(404)
        .create_async().await;
    let client = PatClient::new(server.url());
    let err = client.read(MailboxFolder::Inbox, "MISSING").await.unwrap_err();
    match err {
        tuxlink_lib::pat_client::PatClientError::Status(404) => {} // OK
        other => panic!("expected Status(404), got {:?}", other),
    }
}

#[tokio::test]
async fn test_clone_yields_independent_handle_to_same_server() {
    // Validates v2 P1 #4 prereq: PatClient: Clone. Both clones must hit the
    // mock server independently; mockito's expect(2) verifies the count.
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("GET", "/api/mailbox/in")
        .with_status(200)
        .with_body("[]")
        .expect(2)
        .create_async().await;
    let client = PatClient::new(server.url());
    let client_clone = client.clone();
    let _ = client.list(MailboxFolder::Inbox).await.expect("first list");
    let _ = client_clone.list(MailboxFolder::Inbox).await.expect("clone list");
}

#[test]
fn test_mailbox_folder_clone_copy_and_debug_are_derived() {
    // Validates v2 P1 #5 prereq: MailboxFolder must be Clone + Copy + Debug
    // so the trait's MailboxFolder re-export carries useful semantics.
    // No tokio runtime needed — purely a type-level check.
    let f = MailboxFolder::Inbox;
    let f2 = f.clone();
    let _ = format!("{:?}", f2);
    let f3 = f; // Copy semantics — f is not consumed by this line.
    let _ = format!("{:?}", f3);
    let _ = format!("{:?}", f); // f still usable after Copy.
}

// ── tuxlink-f1a: read-side byte cap ──────────────────────────────────────────

#[tokio::test]
async fn test_read_rejects_body_over_cap() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("GET", "/api/mailbox/in/BIG")
        .with_status(200)
        .with_body("x".repeat(100)) // 100 bytes
        .create_async()
        .await;

    // Tiny cap so we don't need a multi-MiB fixture.
    let client = PatClient::new(server.url()).with_max_read_bytes(16);
    let result = client.read(MailboxFolder::Inbox, "BIG").await;
    assert!(
        matches!(result, Err(PatClientError::TooLarge { cap: 16 })),
        "expected TooLarge {{ cap: 16 }}, got {result:?}"
    );
}

#[tokio::test]
async fn test_read_accepts_body_within_cap() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("GET", "/api/mailbox/in/OK")
        .with_status(200)
        .with_body("hello")
        .create_async()
        .await;

    let client = PatClient::new(server.url()).with_max_read_bytes(1024);
    let body = client.read(MailboxFolder::Inbox, "OK").await.expect("read within cap");
    assert_eq!(body, b"hello");
}
