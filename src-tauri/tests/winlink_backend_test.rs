// Tests for tuxlink-z5f — WinlinkBackend trait contract.
//
// Spec: docs/superpowers/specs/2026-05-18-winlink-backend-trait-design.md §4
// bd issue: tuxlink-z5f
//
// Pat-specific tests (PatBackend::from_url, PatBackend::spawn,
// ingest_pat_line, etc.) were deleted in tuxlink-9phd Phase 9 along with
// PatBackend itself. Tests that remain cover NativeBackend and
// backend-agnostic type guarantees.

use tuxlink_lib::config::{CmsTransport, PacketConfig};
use tuxlink_lib::winlink_backend::{
    BackendStatus, MailboxFolder, MessageId, NativeBackend,
    OutboundMessage, WinlinkBackend,
};

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
    backend.set_active_identity(tuxlink_lib::identity::SessionIdentity::full(
        tuxlink_lib::identity::IdentityHandle::for_test(
            tuxlink_lib::identity::Callsign::parse("N7CPZ").unwrap(),
        ),
    ));

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
#[allow(deprecated)] // sets pat_mbo_address on Config literal; field deprecated per tuxlink-9phd T8.1
fn native_test_config() -> tuxlink_lib::config::Config {
    use tuxlink_lib::config::{
        Config, ConnectConfig, GpsState, IdentityConfig, PositionPrecision, PrivacyConfig,
    };
    Config {
        schema_version: tuxlink_lib::config::CONFIG_SCHEMA_VERSION,
        wizard_completed: true,
        connect: ConnectConfig {
            connect_to_cms: true,
            transport: CmsTransport::CmsSsl,
            host: tuxlink_lib::config::default_cms_host(),
        },
        identity: IdentityConfig {
            active_full: Some("N7CPZ".to_string()),
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
        modem_vara: None,
        telnet_listen: tuxlink_lib::config::TelnetListenUiConfig::default(),
        network_po_favorites: Vec::new(),
        review_inbound_before_download: false,
        map_tile_source: None,
        aredn_master_node_host: None,
        aprs: tuxlink_lib::config::AprsConfig::default(),
        trash_auto_purge: true,
        trash_retention_days: 30,
        close_to_tray: true,
        close_prompt_seen: false,
    }
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
    backend.set_active_identity(tuxlink_lib::identity::SessionIdentity::full(
        tuxlink_lib::identity::IdentityHandle::for_test(
            tuxlink_lib::identity::Callsign::parse("N7CPZ").unwrap(),
        ),
    ));

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
    backend.set_active_identity(tuxlink_lib::identity::SessionIdentity::full(
        tuxlink_lib::identity::IdentityHandle::for_test(
            tuxlink_lib::identity::Callsign::parse("N7CPZ").unwrap(),
        ),
    ));

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
    use tuxlink_lib::winlink::session::{ExchangeConfig, OutboundMessage, SessionIntent};

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
        intent: SessionIntent::Cms,
    };

    // Capture wire log lines.
    let captured: RefCell<Vec<String>> = RefCell::new(Vec::new());
    let wire_log = |s: &str| captured.borrow_mut().push(s.to_string());

    tuxlink_lib::winlink::session::run_exchange(
        &mut reader,
        &mut writer,
        &config,
        vec![out],
        |_, _| Ok(vec![tuxlink_lib::winlink::proposal::Answer::Accept { resume_offset: 0 }]),
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
    use tuxlink_lib::winlink::session::{ExchangeConfig, ExchangeRole, OutboundMessage, SessionIntent};
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
    sender.set_active_identity(tuxlink_lib::identity::SessionIdentity::full(
        tuxlink_lib::identity::IdentityHandle::for_test(
            tuxlink_lib::identity::Callsign::parse("N7CPZ").unwrap(),
        ),
    ));
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
            intent: SessionIntent::Cms,
        };
        let result = tuxlink_lib::winlink::session::run_exchange_with_role(
            &mut reader,
            &mut writer,
            ExchangeRole::Answer,
            &server_config,
            vec![], // server has nothing to send
            |proposals, _manifest| {
                Ok(proposals
                    .iter()
                    .map(|_| tuxlink_lib::winlink::proposal::Answer::Accept { resume_offset: 0 })
                    .collect())
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
        intent: SessionIntent::Cms,
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
        |_, _| Ok(vec![]),
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
