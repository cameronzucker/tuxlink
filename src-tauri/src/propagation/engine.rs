//! voacapl invocation. Resolves the bundled binary + itshfbc data, runs headless
//! in a per-call scratch run dir (RAII-cleaned), captures voacapx.out, bounded by
//! a timeout that kills a runaway process. Pure offline compute: no network, no TX,
//! no writes outside the scratch dir.

use std::path::{Path, PathBuf};
use std::time::Duration;
use super::PropagationError;

/// Default bound on a single voacapl run. METHOD-30 24-hour runs complete in well
/// under a second; this is a generous runaway-guard, not a tuning knob.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub struct EnginePaths {
    /// Path to the `voacapl` binary (resolved from the Tauri sidecar dir in Task 6).
    pub binary: PathBuf,
    /// Path to the read-only bundled `itshfbc` root (coeffs/antennas/database/geo*).
    pub itshfbc_root: PathBuf,
}

/// Run voacapl for the given deck text; returns raw voacapx.out text.
/// `scratch_parent` is where the per-call temp dir is created (Task 6 passes the
/// app cache dir; it must already exist — fail closed, never silently use /tmp).
pub fn run_voacapl(
    paths: &EnginePaths,
    deck_text: &str,
    scratch_parent: &Path,
) -> Result<String, PropagationError> {
    run_voacapl_with_timeout(paths, deck_text, scratch_parent, DEFAULT_TIMEOUT)
}

pub fn run_voacapl_with_timeout(
    paths: &EnginePaths,
    deck_text: &str,
    scratch_parent: &Path,
    timeout: Duration,
) -> Result<String, PropagationError> {
    // Fail early if the binary doesn't exist, before doing any scratch work.
    if !paths.binary.exists() {
        return Err(PropagationError::BinaryNotFound(
            paths.binary.display().to_string(),
        ));
    }

    // Build the per-call scratch itshfbc root (RAII-cleaned on all exit paths).
    let scratch = make_scratch_itshfbc(paths, scratch_parent)?;
    let run_dir = scratch.path().join("run");

    // Write the input deck.
    std::fs::write(run_dir.join("voacapx.dat"), deck_text)?;

    // Spawn voacapl. stdin=null, stdout=null (voacapl ignores stdout),
    // stderr=piped so we can drain it (a full pipe would deadlock the process).
    use std::process::{Command, Stdio};
    let mut child = Command::new(&paths.binary)
        .arg(scratch.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;

    // Drain stderr on a separate thread so a full stderr buffer can't deadlock.
    let stderr_handle = {
        let stderr = child.stderr.take().expect("stderr was piped");
        std::thread::spawn(move || {
            use std::io::Read;
            let mut buf = String::new();
            let mut reader = std::io::BufReader::new(stderr);
            let _ = reader.read_to_string(&mut buf);
            buf
        })
    };

    // Poll until done or timeout.
    let deadline = std::time::Instant::now() + timeout;
    let status = loop {
        match child.try_wait()? {
            Some(s) => break s,
            None => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    // Join the stderr thread to avoid a thread leak.
                    let _ = stderr_handle.join();
                    return Err(PropagationError::RunFailed(format!(
                        "voacapl exceeded {timeout:?}"
                    )));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    };

    let stderr_text = stderr_handle.join().unwrap_or_default();

    if !status.success() {
        return Err(PropagationError::RunFailed(format!(
            "exit {:?}: {}",
            status.code(),
            stderr_text.trim()
        )));
    }

    // Read the output file. Keep `scratch` alive until AFTER this read so RAII
    // doesn't delete the tempdir (and the file with it) before we're done.
    let out_path = run_dir.join("voacapx.out");
    if !out_path.exists() {
        return Err(PropagationError::RunFailed(
            "voacapl produced no voacapx.out".to_string(),
        ));
    }
    let output = std::fs::read_to_string(&out_path)?;

    // `scratch` drops here → TempDir::drop() calls std::fs::remove_dir_all on the
    // scratch root, which removes symlinks themselves (not their targets), so the
    // bundled read-only itshfbc trees (coeffs/antennas/database/geo*) are safe.
    drop(scratch);

    Ok(output)
}

/// Create a per-call scratch itshfbc root inside `parent`.
///
/// Layout:
/// ```
/// <tempdir>/
///   run/           ← writable; voacapl writes voacapx.out here
///   coeffs  → <itshfbc_root>/coeffs       (symlink, if src exists)
///   antennas → <itshfbc_root>/antennas    (symlink, if src exists)
///   database → <itshfbc_root>/database    (symlink, if src exists)
///   geocity  → <itshfbc_root>/geocity     (symlink, if src exists)
///   geonatio → <itshfbc_root>/geonatio   (symlink, if src exists)
///   geostate → <itshfbc_root>/geostate   (symlink, if src exists)
/// ```
///
/// Using `tempfile::TempDir::new_in(parent)` ensures:
/// - The scratch dir is RAII-removed on all exit paths (success/error/panic-unwind).
/// - If `parent` doesn't exist or isn't writable, `TempDir::new_in` errors
///   immediately → `PropagationError::Io`. We do NOT fall back to
///   `std::env::temp_dir()` (shared-Pi predictable-name TOCTOU risk, F10).
///
/// `std::fs::remove_dir_all` (used by TempDir on drop) removes symlinks themselves,
/// NOT their targets — so dropping the TempDir does NOT delete the bundled itshfbc
/// trees. Safe to symlink and let RAII clean up.
///
/// The symlink calls are guarded with `#[cfg(unix)]`; on non-unix hosts the function
/// returns a clear error because voacapl requires Linux.
fn make_scratch_itshfbc(
    paths: &EnginePaths,
    parent: &Path,
) -> Result<tempfile::TempDir, PropagationError> {
    #[cfg(not(unix))]
    return Err(PropagationError::RunFailed(
        "voacapl engine requires a unix host".to_string(),
    ));

    #[cfg(unix)]
    {
        let scratch = tempfile::TempDir::new_in(parent)?;
        std::fs::create_dir_all(scratch.path().join("run"))?;

        // Symlink each read-only subtree that exists in the bundled itshfbc root.
        // voacapl opens database/, coeffs/, and antennas/ with status='old' (read-only);
        // only run/ is written.
        for sub in &["coeffs", "antennas", "database", "geocity", "geonatio", "geostate"] {
            let src = paths.itshfbc_root.join(sub);
            if src.exists() {
                let dst = scratch.path().join(sub);
                std::os::unix::fs::symlink(&src, &dst)?;
            }
        }

        Ok(scratch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn missing_binary_is_clear_error() {
        let parent = tempfile::tempdir().expect("tempdir");
        let paths = EnginePaths {
            binary: PathBuf::from("/nonexistent/voacapl"),
            itshfbc_root: PathBuf::from("/tmp"),
        };
        let result = run_voacapl(&paths, "dummy deck", parent.path());
        match result {
            Err(PropagationError::BinaryNotFound(p)) => {
                assert!(p.contains("voacapl"), "path should mention binary name: {p}");
            }
            other => panic!("expected BinaryNotFound, got {other:?}"),
        }
    }

    /// Test that `make_scratch_itshfbc` constructs the expected layout:
    /// - `run/` directory exists
    /// - symlinks are created for subtrees that exist in the fake itshfbc root
    /// - subtrees that DON'T exist don't get a symlink
    #[cfg(unix)]
    #[test]
    fn scratch_root_has_run_dir_and_symlinks() {
        // Build a fake bundled itshfbc root with only some of the subtrees present.
        let fake_itshfbc = tempfile::tempdir().expect("fake itshfbc tempdir");
        std::fs::create_dir_all(fake_itshfbc.path().join("coeffs")).unwrap();
        std::fs::create_dir_all(fake_itshfbc.path().join("database")).unwrap();
        // "antennas", "geocity", "geonatio", "geostate" deliberately absent.

        let scratch_parent = tempfile::tempdir().expect("scratch parent tempdir");
        let paths = EnginePaths {
            binary: PathBuf::from("/nonexistent/voacapl"), // not used by make_scratch_itshfbc
            itshfbc_root: fake_itshfbc.path().to_path_buf(),
        };

        let scratch = make_scratch_itshfbc(&paths, scratch_parent.path())
            .expect("make_scratch_itshfbc should succeed");

        // run/ directory must exist and be a real directory (not a symlink).
        let run_dir = scratch.path().join("run");
        assert!(run_dir.exists(), "run/ should exist");
        assert!(run_dir.is_dir(), "run/ should be a directory");
        assert!(!run_dir.symlink_metadata().unwrap().file_type().is_symlink(),
            "run/ should not be a symlink");

        // Present subtrees must be symlinked.
        for sub in &["coeffs", "database"] {
            let link = scratch.path().join(sub);
            assert!(link.exists(), "{sub} symlink target should exist (link resolves)");
            assert!(
                link.symlink_metadata().unwrap().file_type().is_symlink(),
                "{sub} should be a symlink"
            );
        }

        // Absent subtrees must NOT have a symlink.
        for sub in &["antennas", "geocity", "geonatio", "geostate"] {
            let link = scratch.path().join(sub);
            assert!(
                !link.exists() && link.symlink_metadata().is_err(),
                "{sub} should not exist in scratch (source was absent)"
            );
        }
    }

    /// Test that a runaway process is killed within the timeout and the error
    /// message is clear.
    ///
    /// Strategy: write a shell script that sleeps for 5s, set the timeout to
    /// 300ms, and assert we get RunFailed in well under 5s.
    #[cfg(unix)]
    #[test]
    fn timeout_kills_runaway() {
        use std::io::Write;

        // Build a fake itshfbc root (needs database/ so make_scratch_itshfbc doesn't
        // error on a missing subtree — but missing is fine; they're just skipped).
        let fake_itshfbc = tempfile::tempdir().expect("fake itshfbc");

        // Write the "runaway" binary: a shell script that sleeps longer than the timeout.
        let bin_dir = tempfile::tempdir().expect("bin dir");
        let bin_path = bin_dir.path().join("sleep_runner.sh");
        {
            let mut f = std::fs::File::create(&bin_path).unwrap();
            // Use `exec` so the shell process is REPLACED by `sleep`, not a grandchild.
            // This ensures `child.kill()` kills the sleep process directly, not just
            // the shell wrapper (which would leave the sleep orphaned and cause wait() to
            // block for the full 5s while the grandchild runs).
            f.write_all(b"#!/bin/sh\nexec sleep 5\n").unwrap();
        }
        // Make it executable.
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin_path, std::fs::Permissions::from_mode(0o755)).unwrap();

        let scratch_parent = tempfile::tempdir().expect("scratch parent");
        let paths = EnginePaths {
            binary: bin_path.clone(),
            itshfbc_root: fake_itshfbc.path().to_path_buf(),
        };

        let start = std::time::Instant::now();
        let result = run_voacapl_with_timeout(
            &paths,
            "deck",
            scratch_parent.path(),
            Duration::from_millis(300),
        );
        let elapsed = start.elapsed();

        // Must error.
        match result {
            Err(PropagationError::RunFailed(msg)) => {
                assert!(
                    msg.contains("exceeded"),
                    "error should mention timeout: {msg}"
                );
            }
            other => panic!("expected RunFailed(timeout), got {other:?}"),
        }

        // Must return well before the 5s sleep completes.
        // We allow 3s total (300ms timeout + poll overhead + kill latency on a loaded Pi).
        assert!(
            elapsed < Duration::from_secs(3),
            "timeout test took too long: {elapsed:?} (expected < 3s)"
        );
    }
}
