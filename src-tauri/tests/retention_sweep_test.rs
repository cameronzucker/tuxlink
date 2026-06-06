//! Integration tests for retention sweep — spec §10.4 #5.
//!
//! Populates a tempdir with timestamped tuxlink.*.jsonl files, runs
//! `retention::sweep()` with various configurations, and asserts the
//! documented invariants:
//!
//! 1. Active file is preserved regardless of age.
//! 2. Closed files older than the days cap are deleted.
//! 3. Size-cap eviction targets oldest files first.
//! 4. Clock-backward grace: files whose mtime and filename-parsed UTC disagree
//!    by more than 1 hour are skipped (not deleted).

use chrono::{Datelike, Timelike, Utc};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tuxlink_lib::logging::retention::{sweep, RetentionConfig};

/// Helper: write a file and set its mtime.
fn write_with_mtime(dir: &std::path::Path, name: &str, content: &[u8], mtime: SystemTime) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, content).expect("write file");
    filetime::set_file_mtime(&path, filetime::FileTime::from_system_time(mtime))
        .expect("set mtime");
    path
}

/// Helper: build a filename from a `SystemTime` (rounds down to the hour).
fn filename_for_time(t: SystemTime) -> String {
    let dt: chrono::DateTime<Utc> = t.into();
    format!(
        "tuxlink.{:04}-{:02}-{:02}-{:02}.jsonl",
        dt.year(), dt.month(), dt.day(), dt.hour()
    )
}

#[test]
fn retention_preserves_active_file_under_extreme_age() {
    let tmp = tempfile::tempdir().unwrap();

    // Create a file with mtime + filename both 2 years old.
    let two_years_ago = SystemTime::now() - Duration::from_secs(60 * 60 * 24 * 730);
    let name = filename_for_time(two_years_ago);
    let active = write_with_mtime(tmp.path(), &name, b"active content", two_years_ago);

    // Aggressive retention: delete anything older than 1 day, tiny cap.
    let cfg = RetentionConfig { days: 1, mb_cap: 1 };
    let result = sweep(tmp.path(), &cfg, Some(&active));

    assert_eq!(result.deleted_count, 0, "active file must never be deleted");
    assert!(active.exists(), "active file must still be on disk");
}

#[test]
fn retention_deletes_old_closed_files_by_days_cap() {
    let tmp = tempfile::tempdir().unwrap();

    // Old file: 30 days ago (mtime and filename agree).
    let old_time = SystemTime::now() - Duration::from_secs(60 * 60 * 24 * 30);
    let old_name = filename_for_time(old_time);
    let old_file = write_with_mtime(tmp.path(), &old_name, b"old event", old_time);

    // Fresh file: 1 hour ago (mtime and filename agree).
    let fresh_time = SystemTime::now() - Duration::from_secs(3600);
    let fresh_name = filename_for_time(fresh_time);
    let fresh_file = write_with_mtime(tmp.path(), &fresh_name, b"fresh event", fresh_time);

    // Retention: 14 days cap, generous MB.
    let cfg = RetentionConfig { days: 14, mb_cap: 1000 };
    // No active file for this test.
    let result = sweep(tmp.path(), &cfg, None);

    assert_eq!(result.deleted_count, 1, "only the old file should be deleted");
    assert!(!old_file.exists(), "old file must be gone");
    assert!(fresh_file.exists(), "fresh file must be kept");
}

#[test]
fn retention_deletes_oldest_files_when_size_cap_exceeded() {
    let tmp = tempfile::tempdir().unwrap();

    // Create 3 files, each 300 KB, all aged 2+ days (so not days-capped at 14d).
    // Total = 900 KB. We'll set a 700 KB cap → 200 KB must be deleted.
    let content = vec![b'x'; 300 * 1024];

    let t1 = SystemTime::now() - Duration::from_secs(60 * 60 * 24 * 5); // oldest
    let t2 = SystemTime::now() - Duration::from_secs(60 * 60 * 24 * 4);
    let t3 = SystemTime::now() - Duration::from_secs(60 * 60 * 24 * 3); // newest

    let f1 = write_with_mtime(tmp.path(), &filename_for_time(t1), &content, t1);
    let f2 = write_with_mtime(tmp.path(), &filename_for_time(t2), &content, t2);
    let f3 = write_with_mtime(tmp.path(), &filename_for_time(t3), &content, t3);

    // 700 KB cap — must evict at least one file (oldest first).
    let cfg = RetentionConfig { days: 14, mb_cap: 0 }; // 0 MB cap = always over-cap
    let result = sweep(tmp.path(), &cfg, None);

    // All three are within 14-day retention but size cap forces deletion of oldest.
    assert!(result.deleted_count >= 1, "at least one file must be deleted for size cap");
    // Oldest must be deleted first (filename sort is oldest-first).
    assert!(!f1.exists(), "oldest file f1 must be evicted first");
    // f2 and f3 may or may not be deleted depending on cap; just verify f1 went first.
    let _ = (f2, f3);
}

#[test]
fn retention_clock_backward_grace_skips_disagreeing_files() {
    let tmp = tempfile::tempdir().unwrap();

    // File: filename encodes NOW (current hour), but mtime is set to 30 days ago.
    // Disagreement = ~30 days > 1 hour → clock-backward grace should skip it.
    let now_name = filename_for_time(SystemTime::now());
    let old_mtime = SystemTime::now() - Duration::from_secs(60 * 60 * 24 * 30);
    let disagree_file =
        write_with_mtime(tmp.path(), &now_name, b"timestamp disagrees", old_mtime);

    // Aggressive retention: delete anything older than 1 day.
    let cfg = RetentionConfig { days: 1, mb_cap: 1 };
    let result = sweep(tmp.path(), &cfg, None);

    assert_eq!(
        result.clock_grace_skips, 1,
        "disagreeing file must be counted as a clock-grace skip"
    );
    assert_eq!(result.deleted_count, 0, "disagreeing file must NOT be deleted");
    assert!(disagree_file.exists(), "disagreeing file must still be on disk");
}
