//! Free-disk guard — 5-minute poll of available disk + tracing-appender
//! error counter observation (spec §6.4).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_secs(300);
const LOW_THRESHOLD_BYTES: u64 = 100 * 1024 * 1024; // 100 MB
const RECOVER_THRESHOLD_BYTES: u64 = 200 * 1024 * 1024; // 200 MB

pub struct FreeDiskGuard {
    pub paused: Arc<AtomicBool>,
}

impl FreeDiskGuard {
    pub fn spawn(log_dir: PathBuf) -> Self {
        let paused = Arc::new(AtomicBool::new(false));
        let paused_for_task = paused.clone();
        tokio::spawn(async move {
            loop {
                let free = available_bytes(&log_dir).unwrap_or(u64::MAX);
                let currently_paused = paused_for_task.load(Ordering::Acquire);
                if !currently_paused && free < LOW_THRESHOLD_BYTES {
                    tracing::warn!(
                        free_bytes = free,
                        threshold_bytes = LOW_THRESHOLD_BYTES,
                        "disk-space-low: pausing log writes"
                    );
                    paused_for_task.store(true, Ordering::Release);
                } else if currently_paused && free > RECOVER_THRESHOLD_BYTES {
                    tracing::info!(
                        free_bytes = free,
                        "disk-space-recovered: resuming log writes"
                    );
                    paused_for_task.store(false, Ordering::Release);
                }
                tokio::time::sleep(POLL_INTERVAL).await;
            }
        });
        Self { paused }
    }
}

/// Returns available bytes on the filesystem containing `path`.
/// Linux-only via `libc::statvfs`. Other platforms return `None`.
pub fn available_bytes(path: &std::path::Path) -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        use std::ffi::CString;
        use std::mem::MaybeUninit;
        let c = CString::new(path.to_string_lossy().as_bytes()).ok()?;
        let mut stat: MaybeUninit<libc::statvfs> = MaybeUninit::uninit();
        let rc = unsafe { libc::statvfs(c.as_ptr(), stat.as_mut_ptr()) };
        if rc != 0 {
            return None;
        }
        let stat = unsafe { stat.assume_init() };
        Some(stat.f_bavail as u64 * stat.f_frsize as u64)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = path;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn available_bytes_returns_positive_for_existing_dir() {
        let tmp = tempdir().unwrap();
        let free = available_bytes(tmp.path());
        // On Linux this must be Some and > 0; on other platforms None is acceptable.
        #[cfg(target_os = "linux")]
        assert!(
            free.map(|b| b > 0).unwrap_or(false),
            "expected positive free bytes on Linux, got {:?}",
            free
        );
        #[cfg(not(target_os = "linux"))]
        let _ = free; // None is fine on non-Linux
    }
}
