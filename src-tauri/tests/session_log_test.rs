use tuxlink_lib::session_log::SessionLogState;
use tuxlink_lib::winlink_backend::{LogLine, LogLevel, LogSource};

fn line(msg: &str) -> LogLine {
    LogLine { seq: 0, timestamp_iso: "2026-05-20T00:00:00Z".into(),
              level: LogLevel::Info, source: LogSource::Backend, message: msg.into() }
}

#[test]
fn append_assigns_monotonic_seq_starting_at_1() {
    let log = SessionLogState::new(8);
    let s1 = log.append(line("a"));
    let s2 = log.append(line("b"));
    assert_eq!((s1, s2), (1, 2), "append returns the assigned monotonic seq");
    let snap = log.snapshot();
    assert_eq!(snap.iter().map(|l| l.seq).collect::<Vec<_>>(), vec![1, 2]);
    assert_eq!(snap.iter().map(|l| l.message.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
}

#[test]
fn snapshot_since_returns_only_newer_lines() {
    let log = SessionLogState::new(8);
    for m in ["a", "b", "c"] { log.append(line(m)); }
    let since_1 = log.snapshot_since(1);
    assert_eq!(since_1.iter().map(|l| l.seq).collect::<Vec<_>>(), vec![2, 3]);
    assert!(log.snapshot_since(99).is_empty(), "no lines after a future cursor");
}

#[test]
fn bounded_capacity_evicts_oldest_but_seq_keeps_climbing() {
    let log = SessionLogState::new(2);
    for m in ["a", "b", "c"] { log.append(line(m)); }
    let snap = log.snapshot();
    assert_eq!(snap.len(), 2, "ring buffer is bounded");
    assert_eq!(snap.iter().map(|l| l.seq).collect::<Vec<_>>(), vec![2, 3], "oldest evicted; seq never resets");
}
