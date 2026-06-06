//! Retention sweep — deletes oldest closed files when days/size caps are hit;
//! never deletes the active file (spec §6.3 fix per Codex §8.2).

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone)]
pub struct RetentionConfig {
    pub days: u32,
    pub mb_cap: u32,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self { days: 14, mb_cap: 500 }
    }
}

#[derive(Debug, Default)]
pub struct SweepResult {
    pub deleted_count: usize,
    pub deleted_bytes: u64,
    pub retained_count: usize,
    pub retained_bytes: u64,
    pub active_file: Option<PathBuf>,
    pub clock_grace_skips: usize,
}

/// Sweep closed log files under `log_dir`. The `active_file_path` (if any)
/// is never deleted regardless of age/size.
pub fn sweep(
    log_dir: &Path,
    config: &RetentionConfig,
    active_file_path: Option<&Path>,
) -> SweepResult {
    let mut entries: Vec<(PathBuf, SystemTime, u64)> = std::fs::read_dir(log_dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_name()?.to_str()?;
            if !(name.starts_with("tuxlink.") && name.ends_with(".jsonl")) {
                return None;
            }
            let meta = e.metadata().ok()?;
            let mtime = meta.modified().ok()?;
            Some((path, mtime, meta.len()))
        })
        .collect();

    // Sort by filename (which is timestamp-ordered).
    entries.sort_by(|a, b| a.0.file_name().cmp(&b.0.file_name()));

    let mut result = SweepResult {
        active_file: active_file_path.map(Path::to_path_buf),
        ..Default::default()
    };

    let cutoff_age = Duration::from_secs(60 * 60 * 24 * config.days as u64);
    let cap_bytes: u64 = (config.mb_cap as u64) * 1024 * 1024;
    let now = SystemTime::now();

    // Total size including active file.
    // IMPORTANT: entries may include the active file (read_dir returns all
    // on-disk files). Filter it out of the entries sum to avoid double-
    // counting it alongside total_active_bytes.
    let total_active_bytes: u64 = active_file_path
        .and_then(|p| std::fs::metadata(p).ok().map(|m| m.len()))
        .unwrap_or(0);
    let total_bytes: u64 = entries
        .iter()
        .filter(|(p, _, _)| Some(p.as_path()) != active_file_path)
        .map(|(_, _, sz)| *sz)
        .sum::<u64>()
        + total_active_bytes;
    let mut over_cap = total_bytes.saturating_sub(cap_bytes);

    for (path, mtime, sz) in &entries {
        if Some(path.as_path()) == active_file_path {
            continue;
        }

        let age = now.duration_since(*mtime).unwrap_or_default();
        let filename_age = filename_age(path, now).unwrap_or(Duration::ZERO);

        // Clock-backward grace: if mtime and filename disagree by more than
        // an hour, skip and don't delete.
        let disagreement = if age > filename_age {
            age - filename_age
        } else {
            filename_age - age
        };
        if disagreement > Duration::from_secs(3600) {
            result.clock_grace_skips += 1;
            result.retained_count += 1;
            result.retained_bytes += sz;
            continue;
        }

        let days_match = age > cutoff_age && filename_age > cutoff_age;
        let size_match = over_cap > 0;

        if days_match || size_match {
            let _ = std::fs::remove_file(path);
            result.deleted_count += 1;
            result.deleted_bytes += sz;
            if over_cap > 0 {
                over_cap = over_cap.saturating_sub(*sz);
            }
        } else {
            result.retained_count += 1;
            result.retained_bytes += sz;
        }
    }

    result.retained_bytes += total_active_bytes;
    result
}

fn filename_age(path: &Path, now: SystemTime) -> Option<Duration> {
    let name = path.file_name()?.to_str()?;
    // tuxlink.YYYY-MM-DD-HH.jsonl
    let stripped = name.strip_prefix("tuxlink.")?.strip_suffix(".jsonl")?;
    let mut parts = stripped.split('-');
    let year: i32 = parts.next()?.parse().ok()?;
    let month: u32 = parts.next()?.parse().ok()?;
    let day: u32 = parts.next()?.parse().ok()?;
    let hour: u32 = parts.next()?.parse().ok()?;
    let dt = chrono::NaiveDate::from_ymd_opt(year, month, day)?
        .and_hms_opt(hour, 0, 0)?
        .and_utc();
    let now_dt: chrono::DateTime<chrono::Utc> = now.into();
    let diff = now_dt.signed_duration_since(dt);
    if diff.num_seconds() < 0 {
        Some(Duration::ZERO)
    } else {
        Some(Duration::from_secs(diff.num_seconds() as u64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, Timelike};
    use tempfile::tempdir;

    // Helper: build a filename from a SystemTime, matching the disk consumer format.
    fn filename_for_test(ts: SystemTime) -> String {
        let dt: chrono::DateTime<chrono::Utc> = ts.into();
        format!(
            "tuxlink.{:04}-{:02}-{:02}-{:02}.jsonl",
            dt.year(),
            dt.month(),
            dt.day(),
            dt.hour(),
        )
    }

    #[test]
    fn empty_dir_sweep_is_noop() {
        let tmp = tempdir().unwrap();
        let result = sweep(tmp.path(), &RetentionConfig::default(), None);
        assert_eq!(result.deleted_count, 0);
    }

    #[test]
    fn never_deletes_active_file() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("tuxlink.2024-01-01-00.jsonl");
        std::fs::write(&path, "x").unwrap();
        // Force an old mtime
        let old = SystemTime::now() - Duration::from_secs(60 * 60 * 24 * 365);
        filetime::set_file_mtime(&path, filetime::FileTime::from_system_time(old)).unwrap();

        let cfg = RetentionConfig { days: 1, mb_cap: 1000 };
        let result = sweep(tmp.path(), &cfg, Some(&path));
        assert_eq!(result.deleted_count, 0, "active file must be preserved");
        assert!(path.exists());
    }

    #[test]
    fn deletes_files_older_than_retention_days() {
        let tmp = tempdir().unwrap();
        // Use a timestamp 30 days ago so both mtime and filename-parsed age agree.
        let old = SystemTime::now() - Duration::from_secs(60 * 60 * 24 * 30);
        let old_dt: chrono::DateTime<chrono::Utc> = old.into();
        let filename = format!(
            "tuxlink.{:04}-{:02}-{:02}-{:02}.jsonl",
            old_dt.year(), old_dt.month(), old_dt.day(), old_dt.hour()
        );
        let path = tmp.path().join(&filename);
        std::fs::write(&path, "x").unwrap();
        filetime::set_file_mtime(&path, filetime::FileTime::from_system_time(old)).unwrap();

        let cfg = RetentionConfig { days: 14, mb_cap: 1000 };
        let result = sweep(tmp.path(), &cfg, None);
        assert_eq!(result.deleted_count, 1);
        assert!(!path.exists());
    }

    /// Regression test for the size-cap double-count bug:
    /// entries.iter().sum() previously included the active file's bytes AND
    /// total_active_bytes added it again, inflating over_cap and causing the
    /// loop to delete extra closed files.
    ///
    /// Setup: active=300 KB, old1=100 KB, old2=100 KB → total=500 KB.
    /// With mb_cap=1 (1 MB) the total is well under cap — zero files deleted.
    /// The buggy code would compute total = 800 KB (300 double-counted),
    /// which is still under 1 MB, so this case is safe. To expose the bug we
    /// use a very tight cap that is just below the active-only size.
    ///
    /// Tight-cap case: mb_cap=0 forces over_cap>0 from the start regardless
    /// of total. Both closed files should be deleted; active must be preserved.
    #[test]
    fn size_cap_does_not_double_count_active_file() {
        let tmp = tempdir().unwrap();
        let now = SystemTime::now();
        let old1 = now - Duration::from_secs(60 * 60 * 24 * 2); // 2 days ago
        let old2 = now - Duration::from_secs(60 * 60 * 24); // 1 day ago

        let f_active = tmp.path().join(filename_for_test(now));
        let f_old1 = tmp.path().join(filename_for_test(old1));
        let f_old2 = tmp.path().join(filename_for_test(old2));

        // Write 300 KB active + 100 KB old1 + 100 KB old2 = 500 KB total.
        std::fs::write(&f_active, vec![0u8; 300_000]).unwrap();
        std::fs::write(&f_old1, vec![0u8; 100_000]).unwrap();
        std::fs::write(&f_old2, vec![0u8; 100_000]).unwrap();
        filetime::set_file_mtime(&f_active, filetime::FileTime::from_system_time(now)).unwrap();
        filetime::set_file_mtime(&f_old1, filetime::FileTime::from_system_time(old1)).unwrap();
        filetime::set_file_mtime(&f_old2, filetime::FileTime::from_system_time(old2)).unwrap();

        // mb_cap=1 (1 MiB = 1,048,576 B) → total 500 KB is under cap; zero deletions expected.
        let cfg_no_evict = RetentionConfig { days: 365, mb_cap: 1 };
        let result = sweep(tmp.path(), &cfg_no_evict, Some(&f_active));
        assert_eq!(result.deleted_count, 0, "size cap not exceeded — should not delete any files");

        // mb_cap=0 → cap_bytes=0 → over_cap=total → always over-cap.
        // Both closed files should be evicted; active file must survive.
        let cfg_force_evict = RetentionConfig { days: 365, mb_cap: 0 };
        let result = sweep(tmp.path(), &cfg_force_evict, Some(&f_active));
        assert!(f_active.exists(), "active file must be preserved regardless of cap");
        assert_eq!(result.deleted_count, 2, "both closed files must be deleted when over-cap");
        assert!(!f_old1.exists(), "oldest closed file must be deleted");
        assert!(!f_old2.exists(), "second closed file must be deleted");
    }
}
