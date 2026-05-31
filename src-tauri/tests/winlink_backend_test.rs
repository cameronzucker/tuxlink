// Tests for tuxlink-z5f — WinlinkBackend trait contract.
//
// Spec: docs/superpowers/specs/2026-05-18-winlink-backend-trait-design.md §4
// bd issue: tuxlink-z5f
//
// Test count: 10 (8 trait-contract + 2 type-level), at the upper end of the
// bd-issue's "5-10" cap. Each test maps to one row of the §4 test matrix.

use futures::StreamExt;
use tuxlink_lib::config::{CmsTransport, PacketConfig};
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
// Test 2: send_message happy path — multipart POST returns Ok(MessageId(""))
// per Pat 1.0.0 transitional placeholder (no real MID from Pat; deleted in P9).
// ============================================================================
#[tokio::test]
async fn test_pat_backend_send_message_posts_multipart_returns_empty_mid_for_pat() {
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
        attachments: vec![],
    };
    let result = backend.send_message(msg).await.expect("send");
    assert!(
        result.0.is_empty(),
        "Pat 1.0.0 has no MID — transitional placeholder is empty string, got: {:?}",
        result
    );
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
// Test 6: NativeBackend store-backed methods — send queues to the outbox, and
// list/read see it; status starts Disconnected. (The on-air `connect` path is
// validated by the winlink::* tests + src/bin/native_cms_probe.rs, not here.)
// ============================================================================
#[tokio::test]
async fn test_native_backend_send_then_list_and_read() {
    let cfg = native_test_config();
    let tmp = tempfile::tempdir().expect("tmpdir");
    let backend = NativeBackend::new(cfg, tmp.path());

    // Fresh mailbox: status Disconnected, inbox empty.
    match backend.status() {
        BackendStatus::Disconnected => {}
        other => panic!("NativeBackend.status() must start Disconnected, got {:?}", other),
    }
    assert!(backend.list_messages(MailboxFolder::Inbox).await.unwrap().is_empty());

    // Send queues a composed message into the outbox.
    let id = backend
        .send_message(OutboundMessage {
            to: vec!["W1AW".to_string()],
            cc: vec![],
            subject: "Net check-in".to_string(),
            body: "All stations clear.".to_string(),
            date: "2024-05-20T10:13:00Z".to_string(),
            attachments: vec![],
        })
        .await
        .expect("native backend assigns a MID at queue time");

    // It is now listable + readable from the outbox.
    let outbox = backend.list_messages(MailboxFolder::Outbox).await.unwrap();
    assert_eq!(outbox.len(), 1);
    assert_eq!(outbox[0].id, id);
    assert_eq!(outbox[0].subject, "Net check-in");
    assert_eq!(outbox[0].from, "N7CPZ");

    let body = backend
        .read_message_in(MailboxFolder::Outbox, &id)
        .await
        .unwrap();
    assert_eq!(body.id, id);
    assert!(!body.raw_rfc5322.is_empty());

    // stream_log is a live (initially idle) stream.
    let _ = backend.stream_log();
}

// NOTE (tuxlink-9phd T5.1): the canonical copy of this helper lives at
// src/test_helpers.rs for use by #[cfg(test)] lib code (e.g., NativeBackend::test_fixture).
// Integration tests in `tests/` can't import #[cfg(test)] items from the lib
// crate (they're gated out in the lib's normal --test build), so this copy is
// intentionally kept here. Keep both copies in sync when changing the config
// shape; changes to config fields will cause a compile error at both sites.
fn native_test_config() -> tuxlink_lib::config::Config {
    use tuxlink_lib::config::{
        Config, ConnectConfig, GpsState, IdentityConfig, PositionPrecision, PrivacyConfig,
    };
    Config {
        schema_version: 1,
        wizard_completed: true,
        connect: ConnectConfig {
            connect_to_cms: true,
            transport: CmsTransport::CmsSsl,
            host: tuxlink_lib::config::default_cms_host(),
        },
        identity: IdentityConfig {
            callsign: Some("N7CPZ".to_string()),
            identifier: None,
            grid: Some("DM33".to_string()),
        },
        privacy: PrivacyConfig {
            gps_state: GpsState::BroadcastAtPrecision,
            position_precision: PositionPrecision::FourCharGrid,
            position_source: tuxlink_lib::config::PositionSource::Gps,
        },
        pat_mbo_address: None,
        packet: PacketConfig::default(),
        modem_ardop: None,
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
    // line proves Drop was a no-panic local cleanup.
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
        CmsTransport, Config, ConnectConfig, GpsState, IdentityConfig, PacketConfig,
        PositionPrecision, PrivacyConfig, CONFIG_SCHEMA_VERSION,
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
            host: tuxlink_lib::config::default_cms_host(),
        },
        identity: IdentityConfig {
            callsign: Some("TEST1".into()),
            identifier: None,
            grid: Some("AA00aa".into()),
        },
        privacy: PrivacyConfig {
            gps_state: GpsState::Off,
            position_precision: PositionPrecision::FourCharGrid,
            position_source: tuxlink_lib::config::PositionSource::Gps,
        },
        pat_mbo_address: None,
        packet: PacketConfig::default(),
        modem_ardop: None,
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
// Task 0.1 (tuxlink-v1p §6.2) — OutboundMessage carries attachments field.
// ============================================================================
#[test]
fn test_outbound_message_carries_attachments() {
    use tuxlink_lib::winlink_backend::{OutboundAttachment, OutboundMessage};
    let attach = OutboundAttachment {
        filename: "test.xml".to_string(),
        bytes: b"<root/>".to_vec(),
    };
    let msg = OutboundMessage {
        to: vec!["X@winlink.org".to_string()],
        cc: vec![],
        subject: "S".to_string(),
        body: "B".to_string(),
        date: "2026-05-30T00:00:00Z".to_string(),
        attachments: vec![attach.clone()],
    };
    assert_eq!(msg.attachments.len(), 1);
    assert_eq!(msg.attachments[0].filename, "test.xml");
    assert_eq!(msg.attachments[0].bytes, b"<root/>");
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
        CmsTransport, Config, ConnectConfig, GpsState, IdentityConfig, PacketConfig,
        PositionPrecision, PrivacyConfig, CONFIG_SCHEMA_VERSION,
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
            host: tuxlink_lib::config::default_cms_host(),
        },
        identity: IdentityConfig {
            callsign: Some("TUXTEST1".into()),
            identifier: None,
            grid: Some("FM18".into()),
        },
        privacy: PrivacyConfig {
            gps_state: GpsState::Off,
            position_precision: PositionPrecision::FourCharGrid,
            position_source: tuxlink_lib::config::PositionSource::Gps,
        },
        pat_mbo_address: None,
        packet: PacketConfig::default(),
        modem_ardop: None,
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

// ============================================================================
// Task 0.1 (tuxlink-9phd P0/P12) — MailboxFolder canonical definition locality
// ============================================================================
// Verifies that MailboxFolder is reachable via the canonical path
// `tuxlink_lib::winlink_backend::MailboxFolder` — not a re-export from
// pat_client. After Phase 9 deletes pat_client.rs, this path must still work.
// This test passes immediately today (via the re-export) and stays as a
// regression guard after the move so the canonical path keeps working.
#[test]
fn mailbox_folder_is_defined_in_winlink_backend() {
    let _: tuxlink_lib::winlink_backend::MailboxFolder =
        tuxlink_lib::winlink_backend::MailboxFolder::Inbox;
}

// ============================================================================
// Task 3.1 (tuxlink-9phd) — compile-time check: send_message returns
// Result<MessageId, BackendError>, NOT Result<Option<MessageId>, BackendError>.
//
// Spirit: if the trait is rolled back to return Option, this file fails to
// compile because `let _: Result<MessageId, BackendError>` won't accept a
// `Result<Option<MessageId>, BackendError>`.
// ============================================================================
#[tokio::test]
async fn native_backend_send_message_returns_message_id_not_option() {
    use tuxlink_lib::winlink_backend::{BackendError, NativeBackend, OutboundMessage};

    let cfg = native_test_config();
    let tmp = tempfile::tempdir().expect("tmpdir");
    let backend = NativeBackend::new(cfg, tmp.path());

    // The type annotation here is the compile-time assertion:
    // Result<MessageId, BackendError> must be assignable from send_message's return.
    // If the trait returns Result<Option<MessageId>, _>, this will not compile.
    let _result: Result<MessageId, BackendError> = backend
        .send_message(OutboundMessage {
            to: vec!["W1AW@winlink.org".to_string()],
            cc: vec![],
            subject: "Type-check".to_string(),
            body: "compile-time assertion".to_string(),
            date: "2026-05-30T00:00:00Z".to_string(),
            attachments: vec![],
        })
        .await;
    // Runtime: just verify it's Ok (no panic).
    _result.expect("NativeBackend::send_message must succeed with valid config");
}

// ============================================================================
// Test 11: NativeBackend::send_message stores attachments in the outbox.
//
// Spec: docs/plans/strip-pat-add-native-b2f-attachments.md §4.3
// bd issue: tuxlink-9phd / Task 4.1
//
// Verifies that msg.attachments is passed through to compose_message_with_files
// and survives the mailbox round-trip: the attachment bytes and filename must be
// readable back from the outbox via read_message_in + Message::from_bytes.
// ============================================================================
#[tokio::test]
async fn native_backend_send_message_stores_attachments_in_outbox() {
    use tuxlink_lib::winlink::message::Message;
    use tuxlink_lib::winlink_backend::{
        MailboxFolder, NativeBackend, OutboundAttachment, OutboundMessage, WinlinkBackend,
    };

    let tmp = tempfile::tempdir().expect("tmpdir");
    let cfg = native_test_config(); // already sets callsign = Some("N7CPZ")

    let backend = NativeBackend::new(cfg, tmp.path());

    let msg = OutboundMessage {
        to: vec!["W1AW".to_string()],
        cc: vec![],
        subject: "Attachment test".to_string(),
        body: "body".to_string(),
        date: "2026-05-30T12:00:00Z".to_string(),
        attachments: vec![OutboundAttachment {
            filename: "hello.bin".to_string(),
            bytes: b"hello".to_vec(),
        }],
    };

    let id = backend.send_message(msg).await.expect("send queues");

    // Read the stored bytes back and parse with Message::from_bytes.
    let body = backend
        .read_message_in(MailboxFolder::Outbox, &id)
        .await
        .expect("outbox message should be readable");
    assert_eq!(body.id, id);

    let parsed = Message::from_bytes(&body.raw_rfc5322).expect("stored bytes parse as Message");
    let atts = parsed.attachments();
    assert_eq!(atts.len(), 1, "should have exactly one attachment");
    assert_eq!(atts[0].filename, "hello.bin", "filename must be preserved");
    assert_eq!(atts[0].bytes, b"hello", "attachment bytes must be preserved");
}

// ============================================================================
// Test N: wire_log observability — run_exchange emits FC EM proposal lines and
// FS answer lines to the wire_log callback (tuxlink-9phd §Phase 4 Task 4.2).
// ============================================================================
#[test]
fn native_session_emits_wire_log_on_send() {
    use std::cell::RefCell;
    use std::io::Cursor;
    use tuxlink_lib::winlink::message::Message;
    use tuxlink_lib::winlink::session::{ExchangeConfig, OutboundMessage};

    // Build one outbound message.
    let mut msg = Message::new();
    msg.set_header("Mid", "WIRELOG0001");
    msg.set_header("Subject", "Wire log test");
    msg.set_body(b"Testing wire_log observability.\r\n".to_vec());
    let (proposal, compressed) = msg.to_proposal().expect("to_proposal");
    let out = OutboundMessage {
        proposal: proposal.clone(),
        title: "Wire log test".to_string(),
        compressed,
    };

    // Build a scripted server that accepts the proposal (FS Y).
    // In Dial role, the client takes the FIRST message turn (sends proposals),
    // then reads the FS answer from the cursor, then the server takes its turn.
    let mut server = Vec::new();
    // CMS handshake (no challenge — no password needed).
    server.extend_from_slice(b"[WL2K-5.0-B2FHM$]\rCMS>\r");
    // Server's response to our proposal: accept it (FS Y).
    // This is what the exchange reads AFTER we write our FC EM proposals.
    server.extend_from_slice(b"FS Y\r");
    // Server's message turn: nothing to offer (FQ — quit, already accepted ours).
    server.extend_from_slice(b"FQ\r");

    let mut reader = Cursor::new(server);
    let mut writer = Vec::new();
    let config = ExchangeConfig {
        mycall: "N7CPZ".into(),
        targetcall: "SERVICE".into(),
        locator: "CN87".into(),
        password: None,
    };

    // Capture wire log lines.
    let captured: RefCell<Vec<String>> = RefCell::new(Vec::new());
    let wire_log = |s: &str| captured.borrow_mut().push(s.to_string());

    tuxlink_lib::winlink::session::run_exchange(
        &mut reader,
        &mut writer,
        &config,
        vec![out],
        |_| vec![tuxlink_lib::winlink::proposal::Answer::Accept { resume_offset: 0 }],
        Some(&wire_log),
    )
    .expect("exchange should succeed");

    let lines = captured.borrow();
    assert!(
        lines.iter().any(|l| l.starts_with("FC EM ")),
        "expected FC EM in captured wire log, got: {:?}",
        lines
    );
    assert!(
        lines.iter().any(|l| l.starts_with("FS ")),
        "expected FS in captured wire log, got: {:?}",
        lines
    );
}

// ============================================================================
// Task 4.4 (tuxlink-9phd) — two-backend end-to-end exchange with attachment.
//
// Spec: docs/plans/strip-pat-add-native-b2f-attachments.md §Phase 4 Task 4.4
// bd issue: tuxlink-9phd
//
// An in-process telnet loopback: sender composes a message with an attachment,
// sends, receiver decodes via Message::from_bytes (Phase 2 parser), attachment
// bytes match. Strongest end-to-end test for the new outbound-with-attachments
// path.
//
// RADIO-1: 127.0.0.1 loopback only. Nothing is transmitted.
// ============================================================================
#[tokio::test]
async fn two_native_backends_exchange_with_attachment() {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use tuxlink_lib::native_mailbox::Mailbox;
    use tuxlink_lib::winlink::message::Message;
    use tuxlink_lib::winlink::session::{ExchangeConfig, ExchangeRole, OutboundMessage};
    use tuxlink_lib::winlink::telnet::{connect_and_exchange, Transport, CMS_TARGET_CALL};
    use tuxlink_lib::winlink_backend::{
        MailboxFolder, NativeBackend, OutboundAttachment, OutboundMessage as BackendOutbound,
        WinlinkBackend,
    };

    // -----------------------------------------------------------------------
    // Step 1: sender composes a message with an attachment and queues it.
    // -----------------------------------------------------------------------
    let sender_tmp = tempfile::tempdir().expect("sender tmpdir");
    let receiver_tmp = tempfile::tempdir().expect("receiver tmpdir");

    let sender = NativeBackend::new(native_test_config(), sender_tmp.path());
    let id = sender
        .send_message(BackendOutbound {
            to: vec!["W7AUX@winlink.org".to_string()],
            cc: vec![],
            subject: "Attachment e2e test".to_string(),
            body: "See attached.".to_string(),
            date: "2026-05-30T12:00:00Z".to_string(),
            attachments: vec![OutboundAttachment {
                filename: "test.bin".into(),
                bytes: b"hello-attachment-bytes".to_vec(),
            }],
        })
        .await
        .expect("send_message must queue the message");

    // -----------------------------------------------------------------------
    // Step 2: build the proposal from the queued outbox message.
    // -----------------------------------------------------------------------
    let sender_mailbox = Mailbox::new(sender_tmp.path());
    let body = sender_mailbox
        .read(MailboxFolder::Outbox, &id)
        .expect("outbox message must be readable");
    let msg =
        Message::from_bytes(&body.raw_rfc5322).expect("outbox bytes must parse as Message");
    let (proposal, compressed) = msg.to_proposal().expect("message must produce a proposal");
    let outbound_session = vec![OutboundMessage {
        proposal,
        title: msg.header("Subject").unwrap_or_default().to_string(),
        compressed,
    }];

    // -----------------------------------------------------------------------
    // Step 3: spawn a fake telnet server (Answer role) that receives the
    // message and stores it in the receiver's mailbox.
    //
    // Server protocol:
    //   → client dials
    //   ← server writes login prompts
    //   → client sends callsign + password
    //   ← server runs B2F Answer-role exchange (sends master handshake first)
    //   ← server stores received messages into receiver_tmp mailbox
    //
    // RADIO-1: 127.0.0.1 loopback only; nothing is transmitted.
    // -----------------------------------------------------------------------
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
    let listen_port = listener.local_addr().expect("get listener addr").port();
    let receiver_dir = receiver_tmp.path().to_path_buf();

    let server = std::thread::spawn(move || -> Vec<u8> {
        let (sock, _) = listener.accept().expect("accept");
        // Write telnet login prompts.
        let mut writer = sock.try_clone().expect("clone for write");
        writer
            .write_all(b"Callsign :\rPassword :\r")
            .expect("write login prompts");

        // Read and discard the client's callsign and password responses
        // (two CR-terminated lines).
        let mut reader = BufReader::new(sock);
        for _ in 0..2 {
            let mut line = Vec::new();
            reader
                .read_until(b'\r', &mut line)
                .expect("read login response");
        }

        // Run B2F Answer-role exchange on this socket.
        let server_config = ExchangeConfig {
            mycall: "W7AUX".into(),
            targetcall: CMS_TARGET_CALL.to_string(),
            locator: "CN87".into(),
            password: None,
        };
        let result = tuxlink_lib::winlink::session::run_exchange_with_role(
            &mut reader,
            &mut writer,
            ExchangeRole::Answer,
            &server_config,
            vec![], // server has nothing to send
            |proposals| {
                proposals
                    .iter()
                    .map(|_| tuxlink_lib::winlink::proposal::Answer::Accept { resume_offset: 0 })
                    .collect()
            },
            None,
        )
        .expect("server-side exchange must succeed");

        // Store received messages into the receiver's mailbox.
        let receiver_mailbox = Mailbox::new(&receiver_dir);
        let mut first_raw = Vec::new();
        for message in &result.received {
            let raw = message.to_bytes();
            if first_raw.is_empty() {
                first_raw = raw.clone();
            }
            receiver_mailbox
                .store(MailboxFolder::Inbox, &raw)
                .expect("store in receiver inbox");
        }
        first_raw
    });

    // -----------------------------------------------------------------------
    // Step 4: client dials the local listener and runs the exchange.
    // -----------------------------------------------------------------------
    let client_config = ExchangeConfig {
        mycall: "N7CPZ".into(),
        targetcall: CMS_TARGET_CALL.to_string(),
        locator: "CN87".into(),
        password: None,
    };
    connect_and_exchange(
        "127.0.0.1",
        listen_port,
        Transport::Plaintext,
        &client_config,
        outbound_session,
        &|_| {},
        &|_| {},
        &|_| {},
        |_| vec![],
    )
    .expect("client-side exchange must succeed");

    // Wait for the server to finish storing received messages.
    server.join().expect("server thread panicked");

    // -----------------------------------------------------------------------
    // Step 5: assert attachment survived the full pipeline.
    // -----------------------------------------------------------------------
    let receiver = NativeBackend::new(native_test_config(), receiver_tmp.path());
    let inbox = receiver
        .list_messages(MailboxFolder::Inbox)
        .await
        .expect("list receiver inbox");
    assert_eq!(inbox.len(), 1, "receiver inbox must hold exactly one message; got {inbox:?}");

    let received_body = receiver
        .read_message_in(MailboxFolder::Inbox, &inbox[0].id)
        .await
        .expect("read_message_in must succeed");
    // Verify field name — actual is raw_rfc5322 per the spec.
    let parsed = Message::from_bytes(&received_body.raw_rfc5322)
        .expect("received bytes must parse as Message");
    assert_eq!(parsed.attachments().len(), 1, "should have exactly one attachment");
    assert_eq!(parsed.attachments()[0].filename, "test.bin", "filename must be preserved");
    assert_eq!(
        parsed.attachments()[0].bytes,
        b"hello-attachment-bytes",
        "attachment bytes must be preserved"
    );
}
