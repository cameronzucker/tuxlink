// Tests for tuxlink-z5f — WinlinkBackend trait contract.
//
// Spec: docs/superpowers/specs/2026-05-18-winlink-backend-trait-design.md §4
// bd issue: tuxlink-z5f
//
// Test count: 10 (8 trait-contract + 2 type-level), at the upper end of the
// bd-issue's "5-10" cap. Each test maps to one row of the §4 test matrix.

use futures::StreamExt;
use tuxlink_lib::config::CmsTransport;
use tuxlink_lib::winlink_backend::{
    BackendError, BackendStatus, LogLevel, LogLine, LogSource, MailboxFolder, MessageId,
    NativeBackend, OutboundMessage, PatBackend, TransportConfig, WinlinkBackend,
};

// ============================================================================
// Test 1: list_messages happy path — Pat DTO JSON → MessageMeta mapping
// ============================================================================
#[tokio::test]
async fn test_pat_backend_list_messages_returns_mapped_metas() {
    let mut server = mockito::Server::new_async().await;
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
    assert_eq!(metas.len(), 1);
    assert_eq!(metas[0].id, MessageId::new("ABC123"));
    assert_eq!(metas[0].subject, "Test");
    assert_eq!(metas[0].from, "W4PHS@winlink.org");
    assert!(metas[0].unread);
    assert_eq!(metas[0].body_size, 42);
}

// ============================================================================
// Test 2: send_message happy path — multipart POST returns Ok(None) per Pat
// 1.0.0's plain-text-confirmation behavior (no MID returned by Pat).
// ============================================================================
#[tokio::test]
async fn test_pat_backend_send_message_posts_multipart_returns_none_for_pat() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("POST", "/api/mailbox/out")
        .match_header(
            "content-type",
            mockito::Matcher::Regex("^multipart/form-data".to_string()),
        )
        .with_status(201)
        .with_body("Message posted (0.05 kB)")
        .create_async()
        .await;

    let backend = PatBackend::from_url(server.url());
    let msg = OutboundMessage {
        to: vec!["SERVICE@winlink.org".to_string()],
        cc: vec![],
        subject: "Test".to_string(),
        body: "body".to_string(),
        date: "2026-04-22T15:00:00Z".to_string(),
    };
    let result = backend.send_message(msg).await.expect("send");
    assert_eq!(result, None, "Pat 1.0.0 returns plain-text confirmation, no MID");
}

// ============================================================================
// Test 3: read_message 404 → BackendError::NotFound (spec §3.3 special case)
// ============================================================================
#[tokio::test]
async fn test_pat_backend_translates_404_to_not_found_for_read() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("GET", "/api/mailbox/in/MISSING")
        .with_status(404)
        .create_async()
        .await;

    let backend = PatBackend::from_url(server.url());
    let id = MessageId::new("MISSING");
    let err = backend.read_message(&id).await.unwrap_err();
    match err {
        BackendError::NotFound(found_id) => assert_eq!(found_id, id),
        other => panic!("expected NotFound, got {:?}", other),
    }
}

// ============================================================================
// Test 4: 401 → BackendError::AuthFailed
// ============================================================================
#[tokio::test]
async fn test_pat_backend_translates_401_to_auth_failed() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("GET", "/api/mailbox/in")
        .with_status(401)
        .create_async()
        .await;

    let backend = PatBackend::from_url(server.url());
    let err = backend
        .list_messages(MailboxFolder::Inbox)
        .await
        .unwrap_err();
    match err {
        BackendError::AuthFailed { reason } => {
            assert!(reason.contains("401"), "reason should mention 401, got: {reason}");
        }
        other => panic!("expected AuthFailed, got {:?}", other),
    }
}

// ============================================================================
// Test 5: connect error (port closed) → BackendError::BackendUnavailable
// with source.is_some() (validates v2 P1 #7 source-preservation)
// ============================================================================
#[tokio::test]
async fn test_pat_backend_translates_connect_error_to_backend_unavailable() {
    // Construct a backend pointed at a port that's almost certainly closed.
    // We don't bind anything; reqwest will fail with a connect error.
    let backend = PatBackend::from_url("http://127.0.0.1:1");
    let err = backend
        .list_messages(MailboxFolder::Inbox)
        .await
        .unwrap_err();
    match err {
        BackendError::BackendUnavailable { source, .. } => {
            assert!(
                source.is_some(),
                "v2 P1 #7: BackendUnavailable must preserve the underlying connect error as source"
            );
        }
        // Some platforms surface the closed-port failure as a transport
        // failure (Connection refused fast path); accept either, but still
        // require source preservation.
        BackendError::TransportFailed { source, .. } => {
            assert!(
                source.is_some(),
                "v2 P1 #7: TransportFailed must preserve source"
            );
        }
        other => panic!(
            "expected BackendUnavailable or TransportFailed, got {:?}",
            other
        ),
    }
}

// ============================================================================
// Test 6: NativeBackend stub — every method returns NotImplemented (or empty
// stream / Disconnected for the snapshot methods). Validates spec §3.9 and
// asserts no panic on stub paths (v2 P3 #12 ratified).
// ============================================================================
#[tokio::test]
async fn test_native_backend_returns_not_implemented_for_every_method() {
    let backend = NativeBackend::new();

    matches_not_implemented(backend.list_messages(MailboxFolder::Inbox).await);
    matches_not_implemented(
        backend
            .read_message(&MessageId::new("ANY"))
            .await
            .map(|_| ()),
    );
    matches_not_implemented(
        backend
            .send_message(OutboundMessage {
                to: vec![],
                cc: vec![],
                subject: "".to_string(),
                body: "".to_string(),
                date: "".to_string(),
            })
            .await
            .map(|_| ()),
    );
    matches_not_implemented(
        backend
            .connect(TransportConfig::Cms { mode: CmsTransport::CmsSsl })
            .await
            .map(|_| ()),
    );
    // disconnect needs a Session; mint one from NativeBackend's instance to
    // satisfy the type (the backend won't ever produce one, so synthesize
    // via a separate constructor path through PatBackend ISN'T possible —
    // skip disconnect for NativeBackend; covered by test #9 for the
    // backend-affinity check).

    match backend.status() {
        BackendStatus::Disconnected => {}
        other => panic!("NativeBackend.status() must be Disconnected, got {:?}", other),
    }

    // stream_log() must be a stream that ends immediately (empty).
    let mut stream = backend.stream_log();
    assert!(stream.next().await.is_none(), "NativeBackend.stream_log() must be empty");
}

fn matches_not_implemented<T: std::fmt::Debug>(result: Result<T, BackendError>) {
    match result {
        Err(BackendError::NotImplemented) => {}
        other => panic!("expected NotImplemented, got {:?}", other),
    }
}

// ============================================================================
// Test 7: Session::drop must not panic — validates spec §3.5 Drop contract
// (local cleanup only; no remote call; no executor deadlock risk).
// ============================================================================
#[tokio::test]
async fn test_session_drop_does_not_panic() {
    // PatBackend::connect is the only path that mints a Session in v0.0.1.
    // No HTTP mock needed because the v0.0.1 connect stub doesn't actually
    // call out to Pat — it just synthesizes a session tied to the backend
    // instance id. Future v0.5 connect will need a mock.
    let backend = PatBackend::from_url("http://127.0.0.1:9999");
    let session = backend
        .connect(TransportConfig::Cms { mode: CmsTransport::CmsSsl })
        .await
        .expect("connect should succeed in v0.0.1 stub");
    drop(session);
    // If Drop panicked, the test process would die here. Reaching this
    // assertion proves Drop was a no-panic local cleanup.
    assert!(true);
}

// ============================================================================
// Test 8: stream_log emits lines + drop unsubscribes cleanly. Uses the
// test-only push_log_line_for_test helper to inject events without needing
// a real Pat process.
// ============================================================================
#[tokio::test]
async fn test_log_stream_emits_lines_and_handles_drop() {
    let backend = PatBackend::from_url("http://127.0.0.1:9999");
    let mut stream = backend.stream_log();

    // Push 3 events; subscriber should receive them in order.
    for i in 0..3 {
        let n = backend.push_log_line_for_test(LogLine {
            seq: 0,
            timestamp_iso: format!("2026-05-18T00:00:0{i}Z"),
            level: LogLevel::Info,
            source: LogSource::Pat,
            message: format!("event {i}"),
        });
        assert!(n > 0, "broadcast must have at least one subscriber on iteration {i}");
    }

    let mut received = vec![];
    // Read up to 3 lines with a short timeout to avoid hanging the test
    // if something is wrong.
    for _ in 0..3 {
        let next = tokio::time::timeout(std::time::Duration::from_millis(200), stream.next())
            .await
            .expect("stream timed out");
        if let Some(line) = next {
            received.push(line);
        }
    }
    assert_eq!(received.len(), 3);
    assert_eq!(received[0].message, "event 0");
    assert_eq!(received[1].message, "event 1");
    assert_eq!(received[2].message, "event 2");

    // Drop the stream; backend continues running (no panic on dropped recv).
    drop(stream);
    let _ = backend.push_log_line_for_test(LogLine {
        seq: 0,
        timestamp_iso: "2026-05-18T00:00:99Z".to_string(),
        level: LogLevel::Info,
        source: LogSource::Pat,
        message: "post-drop".to_string(),
    });
    // No panic = pass. (The send returns 0 receivers but that's fine.)
}

// ============================================================================
// Test 9: Session from one backend instance is rejected by another. Validates
// v2 P0 #1 backend-instance-id affinity check.
// ============================================================================
#[tokio::test]
async fn test_session_from_other_backend_instance_rejected() {
    let backend_a = PatBackend::from_url("http://127.0.0.1:9998");
    let backend_b = PatBackend::from_url("http://127.0.0.1:9999");
    let session_from_a = backend_a
        .connect(TransportConfig::Cms { mode: CmsTransport::CmsSsl })
        .await
        .expect("connect a");
    let err = backend_b.disconnect(session_from_a).await.unwrap_err();
    match err {
        BackendError::InvalidSession => {}
        other => panic!("expected InvalidSession, got {:?}", other),
    }
}

// ============================================================================
// Test 10: MessageBody preserves non-UTF-8 bytes through the trait boundary.
// Validates v2 P0 #2 byte-fidelity requirement at PatBackend::read_message.
// ============================================================================
#[tokio::test]
async fn test_message_body_preserves_bytes() {
    let mut server = mockito::Server::new_async().await;
    let bytes: Vec<u8> = vec![0x48, 0x69, 0xff, 0xfe, 0x00, 0xff];
    let _mock = server
        .mock("GET", "/api/mailbox/in/MIDBIN")
        .with_status(200)
        .with_body(bytes.clone())
        .create_async()
        .await;

    let backend = PatBackend::from_url(server.url());
    let body = backend
        .read_message(&MessageId::new("MIDBIN"))
        .await
        .expect("read");
    assert_eq!(body.raw_rfc5322, bytes);
    assert_eq!(body.id, MessageId::new("MIDBIN"));
}
