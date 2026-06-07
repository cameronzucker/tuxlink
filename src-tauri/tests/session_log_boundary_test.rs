use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::broadcast;
use tuxlink_lib::logging::{event::LoggedEvent, ui_consumer};
use tuxlink_lib::session_log::SessionLogState;

fn event(
    seq: u64,
    target: &str,
    msg: &str,
    fields: BTreeMap<String, serde_json::Value>,
) -> LoggedEvent {
    LoggedEvent {
        v: 1,
        ts: "2026-06-07T04:52:00.000000Z".into(),
        boot: "019a0000-0000-7000-8000-000000000001".into(),
        seq,
        level: "info".into(),
        target: target.into(),
        module: Some(target.into()),
        file: None,
        line: None,
        pid: Some(1234),
        thread: None,
        attempt_id: None,
        spans: vec![],
        msg: msg.into(),
        fields,
    }
}

async fn wait_for_len(log: &SessionLogState, expected: usize) {
    let deadline = Instant::now() + Duration::from_millis(500);
    while Instant::now() < deadline {
        if log.snapshot().len() == expected {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(log.snapshot().len(), expected);
}

#[tokio::test(flavor = "current_thread")]
async fn diagnostic_startup_events_do_not_reach_connection_session_log() {
    let (tx, rx) = broadcast::channel(16);
    let session_log = Arc::new(SessionLogState::new(16));
    ui_consumer::spawn(rx, session_log.clone());

    for (seq, target, msg) in [
        (1, "tuxlink::position::gpsd", "gpsd connected"),
        (2, "tuxlink::bootstrap", "bootstrap action decided"),
        (3, "tuxlink::logging::env_probes::keyring", "probe snapshot"),
    ] {
        tx.send(event(seq, target, msg, BTreeMap::new()))
            .expect("diagnostic event should broadcast");
    }

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        session_log.snapshot().is_empty(),
        "diagnostic tracing events must stay out of the operator-facing connection log"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn explicit_session_log_opt_in_events_still_reach_session_log() {
    let (tx, rx) = broadcast::channel(16);
    let session_log = Arc::new(SessionLogState::new(16));
    ui_consumer::spawn(rx, session_log.clone());

    let mut fields = BTreeMap::new();
    fields.insert("session_log".into(), serde_json::json!(true));
    tx.send(event(7, "tuxlink::winlink::session", "dial start", fields))
        .expect("session event should broadcast");

    wait_for_len(&session_log, 1).await;
    let snapshot = session_log.snapshot();
    assert_eq!(snapshot[0].seq, 7);
    assert_eq!(
        snapshot[0].message,
        "[tuxlink::winlink::session] dial start"
    );
}
