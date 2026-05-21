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
    ingest_pat_line, BackendError, BackendStatus, LogLevel, LogLine, LogSource, MailboxFolder,
    MessageId, NativeBackend, OutboundMessage, PatBackend, TransportConfig, WinlinkBackend,
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

// ============================================================================
// Task C (tuxlink-22l §11.2) — ingest_pat_line: per-line bridge ingest
// ============================================================================
// Maps one raw Pat stderr line → LogLine, appends it to the durable ring
// buffer (which assigns the monotonic seq), and broadcasts it live. Tested
// directly (always-runs unit test) so the bridge's per-line behavior is
// covered without a real Pat process. See spec §3.2 line→LogLine mapping.
#[test]
fn ingest_pat_line_appends_and_broadcasts_with_seq() {
    use tokio::sync::broadcast;
    use tuxlink_lib::session_log::SessionLogState;

    let buf = SessionLogState::new(8);
    let (tx, _rx) = broadcast::channel(16);

    let l = ingest_pat_line("starting http".into(), &buf, &tx);

    // The returned LogLine carries the seq assigned by the durable buffer.
    assert_eq!(l.seq, 1, "first appended line gets seq=1 from the buffer");
    assert_eq!(l.source, LogSource::Pat, "Pat stderr lines are LogSource::Pat");
    assert_eq!(l.level, LogLevel::Info, "v0.0.1 maps every Pat line to Info");
    assert_eq!(l.message, "starting http", "message is the raw line verbatim");

    // The same line landed in the durable buffer with the same seq.
    let snap = buf.snapshot();
    assert_eq!(snap.len(), 1, "exactly one line stored");
    assert_eq!(snap[0].seq, 1, "stored line carries the assigned seq");
    assert_eq!(snap[0].message, "starting http");
}

// ============================================================================
// FIX 2 (tuxlink-22l Codex R2) — failed Pat start drains stderr diagnostics
// into the durable buffer.
// ============================================================================
// When `PatProcess::spawn` returns Err, Pat's pre-exit stderr lines were
// forwarded into the mpsc receiver before EOF, but the bridge thread is not
// started on the Err path — so without draining, those failure diagnostics are
// lost. FIX 2 drains the receiver into the durable buffer before returning, so
// the failure cause survives in `session_log_snapshot` + the buffer-polling
// drain (FIX 1). This test drives the real `PatBackend::spawn` against a fake
// `/bin/sh` fixture (Part-97-safe: NOT real Pat, never connects/sends) that
// prints a diagnostic to stderr and exits non-zero WITHOUT announcing a port —
// exercising the no-announce error path. It asserts (a) spawn returns
// BackendUnavailable, and (b) the diagnostic line reached the durable buffer.
#[test]
fn spawn_failure_drains_pat_stderr_into_durable_buffer() {
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Arc;
    use tuxlink_lib::config::{
        CmsTransport, Config, ConnectConfig, GpsState, IdentityConfig, PositionPrecision,
        PrivacyConfig, CONFIG_SCHEMA_VERSION,
    };
    use tuxlink_lib::session_log::SessionLogState;
    use tuxlink_lib::winlink_backend::{PatBackend, PatBackendSpawnOptions};

    let tmp = tempfile::tempdir().expect("tmpdir");

    // Fake "pat" that emits a recognizable failure diagnostic to stderr and
    // exits 1 WITHOUT ever printing the listen-address needle. PatProcess's
    // reader forwards the line, then sees EOF (process exited) → spawn takes the
    // "exited before announcing" error path. The diagnostic is the line FIX 2
    // must rescue into the durable buffer.
    let fixture = tmp.path().join("failing-pat.sh");
    std::fs::write(
        &fixture,
        "#!/bin/sh\necho 'FATAL: bind: address already in use' 1>&2\nexit 1\n",
    )
    .expect("write fixture");
    let mut perms = std::fs::metadata(&fixture).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&fixture, perms).unwrap();

    let cfg = Config {
        schema_version: CONFIG_SCHEMA_VERSION,
        wizard_completed: true,
        connect: ConnectConfig {
            connect_to_cms: true,
            transport: CmsTransport::CmsSsl,
        },
        identity: IdentityConfig {
            callsign: Some("TEST1".into()),
            identifier: None,
            grid: Some("AA00aa".into()),
        },
        privacy: PrivacyConfig {
            gps_state: GpsState::Off,
            position_precision: PositionPrecision::FourCharGrid,
        },
        pat_mbo_address: None,
    };

    let buf = Arc::new(SessionLogState::new(64));
    let result = PatBackend::spawn(
        PatBackendSpawnOptions {
            binary: fixture,
            config_path: tmp.path().join("config.json"),
            mbox_dir: tmp.path().join("mbox"),
            pid_file: tmp.path().join("pat.pid"),
            tuxlink_config: cfg,
        },
        buf.clone(),
    );

    // (a) spawn fails with BackendUnavailable (Pat failed to start), source kept.
    match result {
        Ok(_) => panic!("spawn must fail when Pat exits without announcing"),
        Err(BackendError::BackendUnavailable { source, .. }) => {
            assert!(source.is_some(), "BackendUnavailable preserves the io::Error source");
        }
        Err(other) => panic!("expected BackendUnavailable, got {other:?}"),
    }

    // (b) FIX 2: Pat's pre-exit stderr diagnostic was drained into the durable
    // buffer (so it reaches the UI via snapshot + the polling drain). Without
    // FIX 2 the buffer would be empty here.
    let snap = buf.snapshot();
    assert!(
        snap.iter().any(|l| l.message.contains("address already in use")),
        "FIX 2: failed-start stderr diagnostics must land in the durable buffer; got: {:?}",
        snap.iter().map(|l| &l.message).collect::<Vec<_>>()
    );
    assert!(
        snap.iter().all(|l| l.source == LogSource::Pat),
        "drained failure lines are LogSource::Pat"
    );
}

// ============================================================================
// Task C (tuxlink-22l §11.2) — PatBackend::spawn against REAL Pat, http mode.
// ============================================================================
// Part 97: http mode only; never connect/send. Operator/CI runs this via
// --ignored. This test spawns a real Pat process in HTTP-only mode (no CMS
// target, no transmission) and asserts the spawn lifecycle: the HTTP client
// reaches Pat, the stderr bridge delivered Pat's startup lines to the durable
// buffer, and the freshly-spawned backend reports Disconnected (NOT Connected
// — Pat's HTTP server being up is not a CMS link; adrev #10). It is #[ignore]d
// so the always-run suite never launches a real binary; the headless build
// MUST NOT run it (CLAUDE.md live-radio rule).
#[tokio::test]
#[ignore]
async fn spawn_against_real_pat_http_mode() {
    use std::path::PathBuf;
    use std::sync::Arc;
    use tuxlink_lib::config::{
        CmsTransport, Config, ConnectConfig, IdentityConfig, PositionPrecision, PrivacyConfig,
        GpsState, CONFIG_SCHEMA_VERSION,
    };
    use tuxlink_lib::session_log::SessionLogState;
    use tuxlink_lib::winlink_backend::{PatBackend, PatBackendSpawnOptions};

    // Resolve the Pat binary: PAT_BINARY override, else the system default.
    let binary: PathBuf = std::env::var("PAT_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/usr/bin/pat"));

    // Skip cleanly if the binary is absent or a zero-byte stub (the debug
    // sidecar is a 0-byte placeholder; adrev #12). No panic — this is the
    // documented "skips if absent" path.
    match std::fs::metadata(&binary) {
        Ok(m) if m.len() == 0 => {
            eprintln!(
                "SKIP spawn_against_real_pat_http_mode: {} is a zero-byte stub",
                binary.display()
            );
            return;
        }
        Err(_) => {
            eprintln!(
                "SKIP spawn_against_real_pat_http_mode: {} not found (set PAT_BINARY)",
                binary.display()
            );
            return;
        }
        Ok(_) => {}
    }

    // Tempdirs for config / mbox / pid so nothing touches operator state.
    let tmp = tempfile::tempdir().expect("tempdir");
    let config_path = tmp.path().join("config.json");
    let mbox_dir = tmp.path().join("mbox");
    let pid_file = tmp.path().join("pat.pid");

    // CMS config fixture (callsign required on the CMS path). The HTTP mode
    // serves the local API only; no connect/send is issued by this test.
    let cfg = Config {
        schema_version: CONFIG_SCHEMA_VERSION,
        wizard_completed: true,
        connect: ConnectConfig {
            connect_to_cms: true,
            transport: CmsTransport::CmsSsl,
        },
        identity: IdentityConfig {
            callsign: Some("TUXTEST1".into()),
            identifier: None,
            grid: Some("FM18".into()),
        },
        privacy: PrivacyConfig {
            gps_state: GpsState::Off,
            position_precision: PositionPrecision::FourCharGrid,
        },
        pat_mbo_address: None,
    };

    let buf = Arc::new(SessionLogState::new(64));
    let backend = PatBackend::spawn(
        PatBackendSpawnOptions {
            binary,
            config_path,
            mbox_dir,
            pid_file,
            tuxlink_config: cfg,
        },
        buf.clone(),
    )
    .expect("PatBackend::spawn should succeed against a real Pat in http mode");

    // The HTTP client reaches Pat's local API (an empty inbox is Ok).
    let listed = backend.list_messages(MailboxFolder::Inbox).await;
    assert!(listed.is_ok(), "list_messages(Inbox) should be Ok, got {listed:?}");

    // The stderr bridge delivered Pat's startup lines into the durable buffer.
    assert!(
        !buf.snapshot().is_empty(),
        "bridge should have appended Pat startup lines to the durable buffer"
    );

    // A freshly-spawned backend is Disconnected — Pat's HTTP server being up
    // is NOT a CMS link (adrev #10). Connected only comes from operator connect().
    assert!(
        matches!(backend.status(), BackendStatus::Disconnected),
        "spawned backend must report Disconnected, got {:?}",
        backend.status()
    );

    // Drop the backend → graceful shutdown (SIGTERM→reap) of the Pat child.
    drop(backend);
}
