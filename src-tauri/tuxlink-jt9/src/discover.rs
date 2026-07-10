//! jt9 binary discovery + engine version probe.
//!
//! Order: explicit config override (must exist and be a file) > `jt9` on
//! PATH. Version comes from the SIBLING `wsjtx_app_version -v` (jt9 itself
//! has no version flag — verified: `--version` → "unrecognised option",
//! exit 0). Fallback: "jt9 (version unknown)".

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq)]
pub enum DiscoverError {
    OverrideMissing(PathBuf),
    NotOnPath,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Jt9Binary {
    pub jt9_path: PathBuf,
    pub engine_version: String,
}

pub fn discover_jt9(config_override: Option<&Path>) -> Result<Jt9Binary, DiscoverError> {
    let jt9_path = match config_override {
        Some(p) => {
            if !p.is_file() {
                return Err(DiscoverError::OverrideMissing(p.to_path_buf()));
            }
            p.to_path_buf()
        }
        None => which_jt9().ok_or(DiscoverError::NotOnPath)?,
    };
    let engine_version = probe_version(&jt9_path);
    Ok(Jt9Binary { jt9_path, engine_version })
}

fn which_jt9() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join("jt9")).find(|c| c.is_file())
}

/// Deadline for the sibling probe. A hung `wsjtx_app_version` (NFS stall,
/// frozen cgroup, a broken build) must not wedge jt9 discovery — it is
/// killed and discovery falls back to UNKNOWN rather than blocking forever
/// on an unbounded `.output()`.
const PROBE_DEADLINE: Duration = Duration::from_secs(2);
const PROBE_POLL_INTERVAL: Duration = Duration::from_millis(50);

fn probe_version(jt9_path: &Path) -> String {
    const UNKNOWN: &str = "jt9 (version unknown)";
    let Some(dir) = jt9_path.parent() else { return UNKNOWN.into() };
    let sibling = dir.join("wsjtx_app_version");
    if !sibling.is_file() {
        return UNKNOWN.into();
    }
    let mut child = match Command::new(&sibling)
        .arg("-v")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return UNKNOWN.into(),
    };

    let deadline = Instant::now() + PROBE_DEADLINE;
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => break,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return UNKNOWN.into();
                }
                std::thread::sleep(PROBE_POLL_INTERVAL);
            }
            Err(_) => return UNKNOWN.into(),
        }
    }

    // The child has exited: reading its (tiny, one-line) stdout here cannot
    // deadlock — the pipe was never full and nothing is still writing to it.
    let mut out = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_string(&mut out);
    }
    let s = out.trim().to_string();
    if s.is_empty() { UNKNOWN.into() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn fake_bin_dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("tuxlink-jt9-disc-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn install_fake(dir: &Path, name: &str, script: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, script).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    #[test]
    fn override_wins_and_version_comes_from_sibling() {
        let d = fake_bin_dir().join("t1");
        std::fs::create_dir_all(&d).unwrap();
        let jt9 = install_fake(&d, "jt9", "#!/bin/sh\nexit 0\n");
        install_fake(&d, "wsjtx_app_version", "#!/bin/sh\n[ \"$1\" = \"-v\" ] && echo 'WSJT-X 2.7.0'\n");
        let got = discover_jt9(Some(&jt9)).unwrap();
        assert_eq!(got.jt9_path, jt9);
        assert_eq!(got.engine_version, "WSJT-X 2.7.0");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn version_unknown_when_sibling_absent() {
        let d = fake_bin_dir().join("t2");
        std::fs::create_dir_all(&d).unwrap();
        let jt9 = install_fake(&d, "jt9", "#!/bin/sh\nexit 0\n");
        let got = discover_jt9(Some(&jt9)).unwrap();
        assert_eq!(got.engine_version, "jt9 (version unknown)");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn hung_version_probe_falls_back_to_unknown() {
        // A wedged wsjtx_app_version (real-world: NFS stall, cgroup freeze,
        // a bad build that hangs on startup) must not wedge jt9 discovery
        // forever. The probe must give up within a bounded deadline and
        // fall back to UNKNOWN. sleep 15 (not 30) keeps a red run's runtime
        // tolerable: pre-fix, .output() blocks for the full 15s before the
        // elapsed-time assertion catches it.
        let d = fake_bin_dir().join("t3");
        std::fs::create_dir_all(&d).unwrap();
        let jt9 = install_fake(&d, "jt9", "#!/bin/sh\nexit 0\n");
        install_fake(&d, "wsjtx_app_version", "#!/bin/sh\nexec sleep 15\n");
        let t0 = Instant::now();
        let got = discover_jt9(Some(&jt9)).unwrap();
        assert!(
            t0.elapsed() < Duration::from_secs(10),
            "hung version probe must not wedge discovery, took {:?}",
            t0.elapsed()
        );
        assert_eq!(got.engine_version, "jt9 (version unknown)");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn missing_override_is_an_error_not_a_fallback() {
        // A configured-but-broken override must be loud, not silently
        // fall back to PATH (operator set it for a reason).
        let missing = PathBuf::from("/nonexistent/custom-jt9");
        assert_eq!(discover_jt9(Some(&missing)), Err(DiscoverError::OverrideMissing(missing)));
    }
}
