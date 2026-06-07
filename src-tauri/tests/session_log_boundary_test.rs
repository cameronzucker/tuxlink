use std::sync::Arc;

use tracing_subscriber::{layer::SubscriberExt, Registry};
use tuxlink_lib::logging::fanout::FanoutLayer;
use tuxlink_lib::session_log::SessionLogState;
use tuxlink_lib::winlink_backend::{LogLevel, LogLine, LogSource};

#[test]
fn diagnostic_fanout_events_do_not_reach_connection_session_log() {
    let session_log = Arc::new(SessionLogState::new(16));
    let (layer, mut rx) = FanoutLayer::create();
    let subscriber = Registry::default().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(target: "tuxlink::position::gpsd", "gpsd connected");
        tracing::info!(target: "tuxlink::bootstrap", "bootstrap action decided");
        tracing::info!(
            target: "tuxlink::logging::env_probes::network",
            "probe snapshot"
        );
    });

    let first = rx.try_recv().expect("diagnostic event should broadcast");
    assert_eq!(first.target, "tuxlink::position::gpsd");
    assert_eq!(first.msg, "gpsd connected");

    assert!(
        session_log.snapshot().is_empty(),
        "diagnostic fanout must not mutate the operator-facing connection log"
    );
}

#[test]
fn production_diagnostic_logging_cannot_opt_into_session_log() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src_root = manifest_dir.join("src");
    let forbidden = [
        "session_log=true",
        "\"session_log\"",
        "pub mod ui_consumer",
        "ui_consumer::spawn",
        "append_with_seq",
    ];

    for entry in walkdir::WalkDir::new(src_root) {
        let entry = entry.expect("walk source tree");
        if !entry.file_type().is_file()
            || entry.path().extension().and_then(|e| e.to_str()) != Some("rs")
        {
            continue;
        }

        let src = std::fs::read_to_string(entry.path())
            .unwrap_or_else(|e| panic!("read {}: {e}", entry.path().display()));
        for needle in forbidden {
            assert!(
                !src.contains(needle),
                "{} contains forbidden diagnostic-to-session-log bridge marker {needle:?}",
                entry.path().display()
            );
        }
    }
}

#[test]
fn p2p_wire_callback_is_session_logged_as_wire_source() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src = std::fs::read_to_string(manifest_dir.join("src/ui_commands.rs"))
        .expect("read ui_commands.rs");
    let marker = "let app_wire = app.clone();";
    let wire_section = src
        .split_once(marker)
        .map(|(_, rest)| rest)
        .and_then(|rest| rest.split_once(")\n    .await"))
        .map(|(section, _)| section)
        .expect("P2P connect callback section should exist");

    assert!(
        wire_section.contains("LogSource::Wire"),
        "P2P raw wire callback must retain LogSource::Wire for operator raw-output filtering"
    );
    assert!(
        !wire_section.contains("LogSource::Transport"),
        "P2P raw wire callback setup must not classify wire lines as transport progress"
    );
}

#[test]
fn diagnostic_fanout_does_not_consume_operator_session_log_sequence() {
    let session_log = Arc::new(SessionLogState::new(16));
    let (layer, mut rx) = FanoutLayer::create();
    let subscriber = Registry::default().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(target: "tuxlink::bootstrap", "bootstrap action decided");
    });
    rx.try_recv().expect("diagnostic event should broadcast");

    let seq = session_log.append(LogLine {
        seq: 0,
        timestamp_iso: "2026-06-07T05:14:00.000Z".into(),
        level: LogLevel::Info,
        source: LogSource::Transport,
        message: "operator-visible connection line".into(),
    });

    assert_eq!(
        seq, 1,
        "diagnostic fanout must not advance the operator-facing session-log cursor"
    );
}
