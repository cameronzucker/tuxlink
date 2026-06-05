//! State-dir resolution with XDG fallbacks, symlink refusal, and canonical-
//! path validation (spec §6.1).

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("HOME and XDG_STATE_HOME both unset")]
    NoHome,
    #[error("path component is a symlink (refusing): {0}")]
    SymlinkComponent(PathBuf),
    #[error("canonical path escapes state home: canonical={canonical:?}, root={root:?}")]
    EscapesRoot { canonical: PathBuf, root: PathBuf },
    #[error("I/O error creating or stat'ing {path:?}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Resolve the on-disk log directory, creating it (mode 0700) if needed.
/// Returns the canonical, validated path; never a symlinked path.
pub fn resolve() -> Result<PathBuf, ResolveError> {
    let base = resolve_base()?;
    let log_dir = base.join("tuxlink").join("logs");

    // Create the directory hierarchy with mode 0700 (owner only).
    if !log_dir.exists() {
        std::fs::create_dir_all(&log_dir)
            .map_err(|e| ResolveError::Io { path: log_dir.clone(), source: e })?;
        let perms = std::fs::Permissions::from_mode(0o700);
        std::fs::set_permissions(&log_dir, perms)
            .map_err(|e| ResolveError::Io { path: log_dir.clone(), source: e })?;
    }

    // Symlink refusal on the leaf.
    let meta = std::fs::symlink_metadata(&log_dir)
        .map_err(|e| ResolveError::Io { path: log_dir.clone(), source: e })?;
    if meta.file_type().is_symlink() {
        return Err(ResolveError::SymlinkComponent(log_dir));
    }

    // Canonical-path check: canonical must be under base.
    let canonical = std::fs::canonicalize(&log_dir)
        .map_err(|e| ResolveError::Io { path: log_dir.clone(), source: e })?;
    let canonical_base = std::fs::canonicalize(&base)
        .map_err(|e| ResolveError::Io { path: base.clone(), source: e })?;
    if !canonical.starts_with(&canonical_base) {
        return Err(ResolveError::EscapesRoot { canonical, root: canonical_base });
    }

    Ok(canonical)
}

fn resolve_base() -> Result<PathBuf, ResolveError> {
    if let Ok(xdg) = std::env::var("XDG_STATE_HOME") {
        let p = PathBuf::from(&xdg);
        if p.is_absolute() {
            return Ok(p);
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        return Ok(PathBuf::from(home).join(".local").join("state"));
    }
    Err(ResolveError::NoHome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    #[test]
    fn resolves_under_xdg_state_home() {
        let tmp = tempdir().unwrap();
        std::env::set_var("XDG_STATE_HOME", tmp.path());
        let resolved = resolve().expect("should resolve");
        assert!(resolved.starts_with(tmp.path()));
        assert!(resolved.ends_with("tuxlink/logs"));
        let mode = std::fs::metadata(&resolved).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700, "directory mode must be 0700");
    }

    #[test]
    fn refuses_symlinked_log_dir() {
        let tmp = tempdir().unwrap();
        // Manually pre-create a symlink at the path we expect resolve() to create.
        let logs_parent = tmp.path().join("tuxlink");
        std::fs::create_dir_all(&logs_parent).unwrap();
        let logs = logs_parent.join("logs");
        let actual = tmp.path().join("elsewhere");
        std::fs::create_dir_all(&actual).unwrap();
        std::os::unix::fs::symlink(&actual, &logs).unwrap();

        std::env::set_var("XDG_STATE_HOME", tmp.path());
        let result = resolve();
        assert!(matches!(result, Err(ResolveError::SymlinkComponent(_))));
    }
}
