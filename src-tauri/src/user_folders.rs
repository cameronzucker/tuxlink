//! User-folder registry — Phase 2 of the unified user-folders mechanism
//! (tuxlink-f62f).
//!
//! Spec: docs/superpowers/specs/2026-06-02-user-folders-design.md §3.1 (storage
//! model) + §6 D4/D7 (`.folders.json` sidecar, slug `[a-z0-9-]+`, reserved
//! names).
//!
//! Folders are directories under the mailbox root, keyed by slug. The display
//! name + creation time live in `<root>/.folders.json`. The slug is stable
//! across renames (only the display name changes), so messages don't move on
//! disk when a folder is renamed.
//!
//! System folders (Inbox/Sent/Outbox/Archive) are NOT in the registry — they
//! live in the `MailboxFolder` enum and use their own directory names. User
//! folder slugs MUST NOT collide with system folder names; the reserved-name
//! list enforces that at creation time.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::winlink_backend::BackendError;

/// The on-disk registry filename, at the mailbox root.
pub const REGISTRY_FILENAME: &str = ".folders.json";

/// Reserved folder display names + slugs. The slug derived from any of these
/// also collides with a system folder directory, so user creation must fail.
/// Comparison is case-insensitive.
pub const RESERVED_NAMES: &[&str] = &["inbox", "sent", "outbox", "drafts", "archive", "deleted"];

/// Slug constraints (spec §6 D7):
/// - Lowercase ASCII letters, digits, hyphens.
/// - 1–40 characters.
/// - No leading or trailing hyphen.
/// - No consecutive hyphens.
pub const SLUG_MIN_LEN: usize = 1;
pub const SLUG_MAX_LEN: usize = 40;

/// Display-name constraints (spec §6 D7):
/// - Trimmed length 3–40.
/// - No control characters.
/// - Slashes and dots forbidden (path-safety; folder dir derived from slug
///   anyway but we keep the display name conservatively printable).
pub const DISPLAY_MIN_LEN: usize = 3;
pub const DISPLAY_MAX_LEN: usize = 40;

/// One user folder as it lives in `.folders.json`. The slug is the on-disk
/// directory name + the wire identifier; the display name is what the UI
/// renders. `created_at` is RFC 3339 UTC for stable ordering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserFolder {
    pub slug: String,
    pub display_name: String,
    pub created_at: String,
}

/// The registry shape on disk. `version` is forward-compat for future schema
/// changes; unknown fields are tolerated by serde's default behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Registry {
    pub version: u32,
    pub folders: Vec<UserFolder>,
}

impl Default for Registry {
    fn default() -> Self {
        Registry { version: 1, folders: Vec::new() }
    }
}

/// Validate a candidate slug. Returns the slug on success or a
/// human-readable error string on failure. The error is surfaced to the
/// frontend via `BackendError::MessageRejected`.
pub fn validate_slug(slug: &str) -> Result<(), String> {
    if slug.len() < SLUG_MIN_LEN {
        return Err("slug is empty".into());
    }
    if slug.len() > SLUG_MAX_LEN {
        return Err(format!("slug exceeds {SLUG_MAX_LEN} characters"));
    }
    let bytes = slug.as_bytes();
    if bytes[0] == b'-' || bytes[bytes.len() - 1] == b'-' {
        return Err("slug must not start or end with '-'".into());
    }
    let mut prev_dash = false;
    for &b in bytes {
        let is_dash = b == b'-';
        if is_dash && prev_dash {
            return Err("slug must not contain consecutive '-'".into());
        }
        if !(b.is_ascii_lowercase() || b.is_ascii_digit() || is_dash) {
            return Err("slug may only contain lowercase letters, digits, and '-'".into());
        }
        prev_dash = is_dash;
    }
    for r in RESERVED_NAMES {
        if slug == *r {
            return Err(format!("'{slug}' is reserved for a system folder"));
        }
    }
    Ok(())
}

/// Derive a slug from a display name: lowercase, ASCII-only, spaces → '-',
/// non-alphanumeric stripped, runs of '-' collapsed, trimmed of leading/
/// trailing '-'. Mirrors common URL-slug conventions.
///
/// Example: "ARES Drills" → "ares-drills"; "Disaster Prep 2026" → "disaster-prep-2026"
///
/// Returns the slug; caller still runs `validate_slug` against it because some
/// inputs (e.g. "!!!") slug to the empty string, which is invalid.
pub fn slug_from_display(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut prev_dash = false;
    for c in name.chars() {
        let lo = c.to_ascii_lowercase();
        if lo.is_ascii_alphanumeric() {
            out.push(lo);
            prev_dash = false;
        } else if lo == ' ' || lo == '-' || lo == '_' {
            if !prev_dash && !out.is_empty() {
                out.push('-');
                prev_dash = true;
            }
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// Validate a display name. Same character set as the slug derivation step
/// expects + the length bounds.
pub fn validate_display_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.len() < DISPLAY_MIN_LEN {
        return Err(format!("display name must be at least {DISPLAY_MIN_LEN} characters"));
    }
    if trimmed.chars().count() > DISPLAY_MAX_LEN {
        return Err(format!("display name must be at most {DISPLAY_MAX_LEN} characters"));
    }
    for c in trimmed.chars() {
        if c.is_control() {
            return Err("display name must not contain control characters".into());
        }
        if c == '/' || c == '\\' {
            return Err("display name must not contain '/' or '\\'".into());
        }
    }
    for r in RESERVED_NAMES {
        if trimmed.eq_ignore_ascii_case(r) {
            return Err(format!("'{trimmed}' is reserved for a system folder"));
        }
    }
    Ok(())
}

/// Load the registry from `<root>/.folders.json`. A missing file → empty
/// registry (first-launch path). A malformed file → empty registry plus a
/// warning to stderr; the caller decides whether to surface that to the UI.
pub fn load_registry(root: &Path) -> Registry {
    let path = root.join(REGISTRY_FILENAME);
    match fs::read_to_string(&path) {
        Ok(s) => match serde_json::from_str::<Registry>(&s) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("user_folders: {} is malformed, starting empty: {e}", path.display());
                Registry::default()
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Registry::default(),
        Err(e) => {
            eprintln!("user_folders: failed to read {}: {e}", path.display());
            Registry::default()
        }
    }
}

/// Save the registry atomically — write to `<root>/.folders.json.tmp`, then
/// rename over. Avoids a half-written registry if the process is killed
/// mid-write.
pub fn save_registry(root: &Path, reg: &Registry) -> Result<(), BackendError> {
    let final_path = root.join(REGISTRY_FILENAME);
    let tmp_path = root.join(format!("{REGISTRY_FILENAME}.tmp"));
    fs::create_dir_all(root)?;
    let json = serde_json::to_string_pretty(reg).map_err(|e| BackendError::Internal {
        msg: format!("registry serialization failed: {e}"),
        source: None,
    })?;
    fs::write(&tmp_path, json)?;
    fs::rename(&tmp_path, &final_path)?;
    Ok(())
}

/// Return the on-disk directory for a user-folder slug. Caller is responsible
/// for ensuring `slug` is validated (validate_slug); this function does NOT
/// sanitize.
pub fn folder_dir(root: &Path, slug: &str) -> PathBuf {
    root.join(slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_slug_accepts_simple() {
        assert!(validate_slug("ares").is_ok());
        assert!(validate_slug("ares-drills").is_ok());
        assert!(validate_slug("a").is_ok());
        assert!(validate_slug("ke7var-thread-2026").is_ok());
    }

    #[test]
    fn validate_slug_rejects_bad_chars() {
        assert!(validate_slug("ARES").is_err()); // uppercase
        assert!(validate_slug("ares drills").is_err()); // space
        assert!(validate_slug("ares_drills").is_err()); // underscore
        assert!(validate_slug("ares/drills").is_err()); // slash
        assert!(validate_slug("ares.drills").is_err()); // dot
    }

    #[test]
    fn validate_slug_rejects_leading_trailing_dashes() {
        assert!(validate_slug("-ares").is_err());
        assert!(validate_slug("ares-").is_err());
        assert!(validate_slug("-").is_err());
    }

    #[test]
    fn validate_slug_rejects_consecutive_dashes() {
        assert!(validate_slug("ares--drills").is_err());
    }

    #[test]
    fn validate_slug_rejects_empty_and_too_long() {
        assert!(validate_slug("").is_err());
        let long = "a".repeat(SLUG_MAX_LEN + 1);
        assert!(validate_slug(&long).is_err());
    }

    #[test]
    fn validate_slug_rejects_reserved_names() {
        for r in RESERVED_NAMES {
            assert!(validate_slug(r).is_err(), "{r} must be reserved");
        }
    }

    #[test]
    fn slug_from_display_canonicalizes() {
        assert_eq!(slug_from_display("ARES Drills"), "ares-drills");
        assert_eq!(slug_from_display("Disaster Prep 2026"), "disaster-prep-2026");
        assert_eq!(slug_from_display("  KE7VAR thread  "), "ke7var-thread");
        assert_eq!(slug_from_display("a/b\\c"), "abc"); // path chars stripped
        assert_eq!(slug_from_display("multi   spaces"), "multi-spaces"); // collapsed
    }

    #[test]
    fn slug_from_display_handles_pathological_input() {
        assert_eq!(slug_from_display(""), "");
        assert_eq!(slug_from_display("!!!"), "");
        assert_eq!(slug_from_display("---"), "");
    }

    #[test]
    fn validate_display_accepts_typical_names() {
        assert!(validate_display_name("ARES Drills").is_ok());
        assert!(validate_display_name("KE7VAR thread").is_ok());
        assert!(validate_display_name("June").is_ok());
    }

    #[test]
    fn validate_display_rejects_short_long_and_reserved() {
        assert!(validate_display_name("ab").is_err());
        let long = "a".repeat(DISPLAY_MAX_LEN + 1);
        assert!(validate_display_name(&long).is_err());
        assert!(validate_display_name("Inbox").is_err());
        assert!(validate_display_name("ARCHIVE").is_err()); // case-insensitive
    }

    #[test]
    fn validate_display_rejects_path_and_control_chars() {
        assert!(validate_display_name("a/b").is_err());
        assert!(validate_display_name("a\\b").is_err());
        assert!(validate_display_name("a\nb").is_err());
    }

    #[test]
    fn load_returns_empty_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let reg = load_registry(dir.path());
        assert_eq!(reg.folders.len(), 0);
        assert_eq!(reg.version, 1);
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let reg = Registry {
            version: 1,
            folders: vec![UserFolder {
                slug: "ares-drills".into(),
                display_name: "ARES Drills".into(),
                created_at: "2026-06-02T22:00:00Z".into(),
            }],
        };
        save_registry(dir.path(), &reg).unwrap();
        let loaded = load_registry(dir.path());
        assert_eq!(loaded.folders, reg.folders);
    }

    #[test]
    fn load_returns_empty_on_malformed_json() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(REGISTRY_FILENAME), b"not json").unwrap();
        let reg = load_registry(dir.path());
        assert_eq!(reg.folders.len(), 0);
    }

    #[test]
    fn folder_dir_uses_root_and_slug() {
        let p = folder_dir(Path::new("/tmp/mbox"), "ares-drills");
        assert_eq!(p, PathBuf::from("/tmp/mbox/ares-drills"));
    }
}
