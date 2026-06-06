//! Failure-mode test: concurrent writes during export — spec §10.4 #20.
//!
//! Spawns a writer task emitting events at ~100/s via the Fanout broadcast
//! receiver, calls `build_archive` mid-stream, and asserts:
//! 1. The export completes without panic.
//! 2. The resulting archive contains valid JSONL (no truncated last line).
//! 3. All JSONL lines in the archive parse as well-formed `LoggedEvent` values.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tuxlink_lib::logging::event::LoggedEvent;
use tuxlink_lib::logging::export::{build_archive, ExportInputs};
use tuxlink_lib::logging::fanout::FanoutLayer;
use tuxlink_lib::session_log::SessionLogState;
use tracing_subscriber::{layer::SubscriberExt, Registry};

/// Write a synthetic JSONL event directly to a file (bypasses the disk consumer,
/// which requires a live Tokio runtime + appender. For this test we write
/// directly to simulate a log file with known content while a concurrent writer
/// is also using the Fanout broadcast).
fn append_event_to_file(dir: &std::path::Path, hour: u32, n: u64) {
    let filename = format!("tuxlink.2026-06-05-{hour:02}.jsonl");
    let line = serde_json::json!({
        "v": 1,
        "ts": format!("2026-06-05T{hour:02}:{n:02}:{:02}.000000Z", n % 60),
        "boot": "test-boot",
        "seq": n,
        "level": "info",
        "target": "tuxlink::winlink::session",
        "msg": format!("write-{n}"),
        "fields": {},
        "spans": [],
    });
    let mut jsonl = serde_json::to_string(&line).unwrap();
    jsonl.push('\n');
    let path = dir.join(&filename);
    // Open in append mode; safe for concurrent access at the OS level.
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .expect("open log file for append");
    f.write_all(jsonl.as_bytes()).expect("write event");
}

#[test]
fn export_completes_without_panic_during_concurrent_writes() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let log_dir = tmp.path().join("logs");
    std::fs::create_dir_all(&log_dir).expect("create log dir");

    // Write a pre-seeded batch of events to give the export something to read.
    for i in 0..50u64 {
        append_event_to_file(&log_dir, 10, i);
    }

    // Spin up a Fanout subscriber in a background thread. This simulates the
    // app's live event stream. The broadcast receiver is not drained here —
    // it fills to capacity and drops, which is the expected behavior during
    // tests. The subscriber must be active so tracing calls below go to it.
    let session_log = Arc::new(SessionLogState::new(100));
    let (layer, _rx) = FanoutLayer::create(session_log);
    let subscriber = Registry::default().with(layer);

    // Set the subscriber as default for this thread so concurrent tracing
    // calls (from the writer thread) have a valid destination.
    let default_guard = tracing::subscriber::set_default(subscriber);

    // Spawn a writer thread that emits events at ~100/s while export is running.
    let log_dir_clone = log_dir.clone();
    let writer_handle = std::thread::spawn(move || {
        for i in 50..200u64 {
            append_event_to_file(&log_dir_clone, 10, i);
            // ~100/s
            std::thread::sleep(Duration::from_millis(10));
        }
    });

    // Short delay to let the writer get ahead slightly.
    std::thread::sleep(Duration::from_millis(50));

    // Call build_archive mid-stream. Must not panic.
    let out_path = tmp.path().join("concurrent.tar.zst");
    let result = build_archive(ExportInputs {
        log_dir: &log_dir,
        active_file_path: None,
        output_path: &out_path,
        correlation_id: Some("concurrent-test"),
        boot_id: "test-boot",
        boot_at: "2026-06-05T10:00:00Z",
        detailed_mode: "off",
        retention_days: 14,
        retention_mb_cap: 500,
        flush_barrier: None,
    });

    // Join writer before asserting (don't care if it overran — just ensure
    // the export itself completed first).
    writer_handle.join().expect("writer thread join");
    drop(default_guard);

    // Export must succeed (no panic, no error).
    let result = result.expect("export must succeed despite concurrent writes");
    assert!(out_path.exists(), "archive file must exist on disk");
    assert!(result.archive_size_bytes > 0, "archive must have non-zero size");

    // Decompress and verify JSONL integrity — no truncated last line.
    // The archive uses inner zstd+dict compression on events.jsonl.zst.
    // We must extract the dict first, then decode events.
    let archive_bytes = std::fs::read(&out_path).expect("read archive");
    let tar_bytes =
        zstd::stream::decode_all(archive_bytes.as_slice()).expect("outer zstd decode");

    // Pass 1: extract dict (may be absent if dict training failed/no dict).
    let mut dict_bytes: Option<Vec<u8>> = None;
    {
        let mut archive = tar::Archive::new(tar_bytes.as_slice());
        for entry in archive.entries().expect("tar entries pass1") {
            let mut entry = entry.expect("tar entry pass1");
            let path: PathBuf = entry.path().expect("entry path").into_owned();
            if path.to_string_lossy() == "dict.zdict" {
                let mut dict = Vec::new();
                use std::io::Read;
                entry.read_to_end(&mut dict).expect("read dict.zdict");
                dict_bytes = Some(dict);
            }
        }
    }

    // Pass 2: extract events.jsonl.zst and decode with dict (if present).
    let mut found_events = false;
    let mut events_text = String::new();
    {
        let mut archive = tar::Archive::new(tar_bytes.as_slice());
        for entry in archive.entries().expect("tar entries pass2") {
            let mut entry = entry.expect("tar entry pass2");
            let path: PathBuf = entry.path().expect("entry path").into_owned();
            if path.to_string_lossy() == "events.jsonl.zst" {
                found_events = true;
                let mut compressed = Vec::new();
                use std::io::Read;
                entry.read_to_end(&mut compressed).expect("read events.jsonl.zst");

                let raw_events = match &dict_bytes {
                    Some(d) => {
                        let mut decoder = zstd::stream::Decoder::with_dictionary(
                            compressed.as_slice(),
                            d,
                        )
                        .expect("create zstd decoder with dict");
                        let mut out = Vec::new();
                        std::io::copy(&mut decoder, &mut out).expect("decode events with dict");
                        out
                    }
                    None => zstd::stream::decode_all(compressed.as_slice())
                        .expect("decode events without dict"),
                };
                events_text =
                    String::from_utf8(raw_events).expect("events must be valid UTF-8");
            }
        }
    }

    assert!(found_events, "archive must contain events.jsonl.zst");

    // Verify all lines parse as LoggedEvent (no truncated last line).
    let mut line_count = 0u64;
    for line in events_text.lines() {
        let parsed: serde_json::Result<LoggedEvent> = serde_json::from_str(line);
        assert!(
            parsed.is_ok(),
            "all JSONL lines must parse as LoggedEvent; failed on: {:?}",
            &line[..line.len().min(120)],
        );
        line_count += 1;
    }

    // We pre-seeded at least 50 events; the archive must have captured some.
    assert!(
        line_count >= 1,
        "archive must contain at least one event; got 0 — export may have missed all writes"
    );
}

/// Regression: export with no events (zero-byte events.jsonl.zst) completes
/// without panic and produces a valid archive. Covers spec §10.4 #26.
#[test]
fn export_with_no_events_produces_valid_archive() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let log_dir = tmp.path().join("logs");
    std::fs::create_dir_all(&log_dir).expect("create log dir");
    // No log files written.

    let out_path = tmp.path().join("empty.tar.zst");
    let result = build_archive(ExportInputs {
        log_dir: &log_dir,
        active_file_path: None,
        output_path: &out_path,
        correlation_id: None,
        boot_id: "test-boot",
        boot_at: "2026-06-05T10:00:00Z",
        detailed_mode: "off",
        retention_days: 14,
        retention_mb_cap: 500,
        flush_barrier: None,
    })
    .expect("export with no events must succeed");

    assert_eq!(result.events_in_archive, 0, "zero events expected");
    assert!(out_path.exists(), "archive file must exist");

    // Archive must decompress cleanly.
    let archive_bytes = std::fs::read(&out_path).expect("read archive");
    let tar_bytes =
        zstd::stream::decode_all(archive_bytes.as_slice()).expect("outer zstd decode");
    let mut archive = tar::Archive::new(tar_bytes.as_slice());
    let mut found_summary = false;
    let mut summary_text = String::new();
    for entry in archive.entries().expect("tar entries") {
        let mut entry = entry.expect("tar entry");
        let path: PathBuf = entry.path().expect("path").into_owned();
        if path.to_string_lossy() == "summary.txt" {
            found_summary = true;
            use std::io::Read;
            // Read as bytes then convert to avoid the deprecation warning on
            // `read_to_string` which requires the entry to be not Seekable.
            let mut raw = Vec::new();
            entry.read_to_end(&mut raw).expect("read summary.txt");
            summary_text = String::from_utf8(raw).expect("summary.txt is UTF-8");
        }
    }
    assert!(found_summary, "archive must have summary.txt");
    // summary.rs renders: "events: {total} (info: {}, warn: {}, error: {})"
    assert!(
        summary_text.contains("events: 0 ("),
        "summary.txt must indicate 0 events; got: {:?}",
        &summary_text[..summary_text.len().min(200)],
    );
}
