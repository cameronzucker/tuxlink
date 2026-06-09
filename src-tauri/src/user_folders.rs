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

/// The current on-disk registry schema version. v2 added `parent_slug`
/// (one-level nesting, tuxlink-ka3z). A registry whose `version` exceeds this
/// is from a newer build and MUST NOT be silently rewritten (forward-corruption
/// guard — see `load_registry`).
pub const CURRENT_REGISTRY_VERSION: u32 = 2;

/// Maximum folder nesting depth (spec D1): top-level folder → subfolder. A
/// subfolder is a leaf. Cycle prevention is structural at this cap.
pub const MAX_FOLDER_DEPTH: usize = 2;

/// One user folder as it lives in `.folders.json`. The slug is the on-disk
/// directory name + the wire identifier; the display name is what the UI
/// renders. `created_at` is RFC 3339 UTC for stable ordering. `parent_slug`
/// (schema v2) names the parent folder, or `None` for a top-level folder; the
/// on-disk directory stays flat (`root/<slug>`) regardless of nesting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserFolder {
    pub slug: String,
    pub display_name: String,
    pub created_at: String,
    /// Parent folder slug, or `None` for a top-level folder. `#[serde(default)]`
    /// makes a v1 registry (records without this field) load with every folder
    /// top-level (spec D2). `skip_serializing_if` keeps a top-level folder's
    /// JSON free of a `null` key so the TS `parentSlug?: string` shape matches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_slug: Option<String>,
}

/// The registry shape on disk. `version` gates schema migration (see
/// `normalize_to_current`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Registry {
    pub version: u32,
    pub folders: Vec<UserFolder>,
}

impl Default for Registry {
    fn default() -> Self {
        Registry { version: CURRENT_REGISTRY_VERSION, folders: Vec::new() }
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
        } else if (lo == ' ' || lo == '-' || lo == '_') && !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
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
    let mut reg = match fs::read_to_string(&path) {
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
    };
    if reg.version > CURRENT_REGISTRY_VERSION {
        // Newer-than-known registry: do NOT normalize. Preserve the version so a
        // subsequent `save_registry` REFUSES to overwrite it (forward-corruption
        // guard, finding #6). Read paths see the parsed folders; no write occurs.
        eprintln!(
            "user_folders: {} is version {} (newer than supported {}); preserving as-is, writes will be refused",
            path.display(),
            reg.version,
            CURRENT_REGISTRY_VERSION
        );
        return reg;
    }
    // v <= CURRENT: migrate/self-heal in memory so the next save persists the
    // current version (finding #5) and never renders a folder invisible (#4).
    normalize_to_current(&mut reg);
    reg
}

/// Save the registry atomically — write to `<root>/.folders.json.tmp`, then
/// rename over. Avoids a half-written registry if the process is killed
/// mid-write. Refuses to write a registry whose `version` exceeds
/// `CURRENT_REGISTRY_VERSION` (forward-corruption guard, finding #6).
pub fn save_registry(root: &Path, reg: &Registry) -> Result<(), BackendError> {
    if reg.version > CURRENT_REGISTRY_VERSION {
        return Err(BackendError::Internal {
            msg: format!(
                "refusing to write registry version {} (newer than supported {})",
                reg.version, CURRENT_REGISTRY_VERSION
            ),
            source: None,
        });
    }
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

/// Direct children of `slug` (folders whose `parent_slug == Some(slug)`). Depth
/// is capped at 2 (spec D1), so children are always leaves.
pub fn children_slugs(reg: &Registry, slug: &str) -> Vec<String> {
    reg.folders
        .iter()
        .filter(|f| f.parent_slug.as_deref() == Some(slug))
        .map(|f| f.slug.clone())
        .collect()
}

/// True if `slug` names a folder present in the registry.
fn folder_exists(reg: &Registry, slug: &str) -> bool {
    reg.folders.iter().any(|f| f.slug == slug)
}

/// True if `slug` names a top-level folder (present, `parent_slug == None`).
fn is_top_level(reg: &Registry, slug: &str) -> bool {
    reg.folders.iter().any(|f| f.slug == slug && f.parent_slug.is_none())
}

/// Validate that `parent` is a legal parent for a folder being CREATED (spec
/// D4): the parent must exist and be top-level (so the new child lands at
/// depth 2, never deeper). Returns a human-readable error on rejection.
pub fn validate_create_parent(reg: &Registry, parent: &str) -> Result<(), String> {
    if !folder_exists(reg, parent) {
        return Err(format!("unknown parent folder '{parent}'"));
    }
    if !is_top_level(reg, parent) {
        return Err(format!(
            "'{parent}' cannot be a parent: it must be a top-level folder (nesting is capped at two levels)"
        ));
    }
    Ok(())
}

/// Validate a re-parent of an EXISTING `slug` to `new_parent` (`None` = promote
/// to top level) against the D4 rule set: `slug` must exist; the target must not
/// be `slug` itself; the target must be an existing top-level folder; and `slug`
/// must have no children (moving a folder-with-children under a parent would
/// create a third level). Promotion to top level is always structurally valid.
pub fn validate_reparent(
    reg: &Registry,
    slug: &str,
    new_parent: Option<&str>,
) -> Result<(), String> {
    if !folder_exists(reg, slug) {
        return Err(format!("unknown folder '{slug}'"));
    }
    match new_parent {
        None => Ok(()),
        Some(parent) => {
            if parent == slug {
                return Err("a folder cannot be its own parent".into());
            }
            validate_create_parent(reg, parent)?;
            if !children_slugs(reg, slug).is_empty() {
                return Err(
                    "this folder has subfolders; move or remove them before nesting it".into(),
                );
            }
            Ok(())
        }
    }
}

/// Integrity oracle: return a list of structural problems in `reg`. An empty
/// list means the registry satisfies the 2-level invariant (unique slugs; every
/// `parent_slug` resolves to an existing top-level folder; no self-parent; no
/// depth > 2). Used by `normalize_to_current` and as a test oracle.
pub fn validate_registry(reg: &Registry) -> Vec<String> {
    let mut problems = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for f in &reg.folders {
        if !seen.insert(&f.slug) {
            problems.push(format!("duplicate slug '{}'", f.slug));
        }
    }
    for f in &reg.folders {
        if let Some(parent) = &f.parent_slug {
            if parent == &f.slug {
                problems.push(format!("'{}' is its own parent", f.slug));
            } else if !folder_exists(reg, parent) {
                problems.push(format!("'{}' has dangling parent '{}'", f.slug, parent));
            } else if !is_top_level(reg, parent) {
                problems.push(format!(
                    "'{}' nests under non-top-level '{}' (depth > {})",
                    f.slug, parent, MAX_FOLDER_DEPTH
                ));
            }
        }
    }
    problems
}

/// Migrate a freshly-loaded registry to the current schema, in place. Sets the
/// version to `CURRENT_REGISTRY_VERSION` and SELF-HEALS any structural problem
/// by promoting the offending folder to top level (`parent_slug = None`) rather
/// than hiding it — a dangling/over-deep `parent_slug` must never make a folder
/// vanish from the tree (finding #4; also protects the future WLE-import path).
/// Each heal is logged. Idempotent: a clean registry is unchanged except for the
/// version field.
pub fn normalize_to_current(reg: &mut Registry) {
    reg.version = CURRENT_REGISTRY_VERSION;

    let slugs: std::collections::HashSet<String> =
        reg.folders.iter().map(|f| f.slug.clone()).collect();

    // Pass 1: heal self-parents and dangling parents.
    for f in &mut reg.folders {
        if let Some(parent) = &f.parent_slug {
            if parent == &f.slug || !slugs.contains(parent) {
                eprintln!(
                    "user_folders: healing '{}' (invalid parent '{}') to top level",
                    f.slug, parent
                );
                f.parent_slug = None;
            }
        }
    }

    // Pass 2: heal depth > 2 — a parent that is itself a subfolder. Recompute
    // the top-level set after pass 1 so a healed orphan counts as a valid parent.
    let top_level: std::collections::HashSet<String> = reg
        .folders
        .iter()
        .filter(|f| f.parent_slug.is_none())
        .map(|f| f.slug.clone())
        .collect();
    for f in &mut reg.folders {
        if let Some(parent) = &f.parent_slug {
            if !top_level.contains(parent) {
                eprintln!(
                    "user_folders: healing '{}' (parent '{}' is not top-level) to top level",
                    f.slug, parent
                );
                f.parent_slug = None;
            }
        }
    }
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
        assert_eq!(reg.version, CURRENT_REGISTRY_VERSION);
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
                parent_slug: None,
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

    // ---- Nested folders (tuxlink-ka3z): schema v2, migration, validation ----

    fn uf(slug: &str, parent: Option<&str>) -> UserFolder {
        UserFolder {
            slug: slug.into(),
            display_name: slug.into(),
            created_at: "2026-06-09T00:00:00Z".into(),
            parent_slug: parent.map(|s| s.to_string()),
        }
    }

    #[test]
    fn v1_registry_loads_with_all_folders_top_level() {
        let dir = tempfile::tempdir().unwrap();
        let v1 = r#"{"version":1,"folders":[
            {"slug":"nets","display_name":"Nets","created_at":"2026-06-02T22:00:00Z"}
        ]}"#;
        fs::write(dir.path().join(REGISTRY_FILENAME), v1).unwrap();
        let reg = load_registry(dir.path());
        assert_eq!(reg.folders.len(), 1);
        assert_eq!(reg.folders[0].parent_slug, None);
    }

    #[test]
    fn v1_registry_is_rewritten_to_v2_on_next_save() {
        let dir = tempfile::tempdir().unwrap();
        let v1 = r#"{"version":1,"folders":[
            {"slug":"nets","display_name":"Nets","created_at":"2026-06-02T22:00:00Z"}
        ]}"#;
        fs::write(dir.path().join(REGISTRY_FILENAME), v1).unwrap();
        // load normalizes in memory; a subsequent save must persist version 2.
        let reg = load_registry(dir.path());
        assert_eq!(reg.version, CURRENT_REGISTRY_VERSION);
        save_registry(dir.path(), &reg).unwrap();
        let raw = fs::read_to_string(dir.path().join(REGISTRY_FILENAME)).unwrap();
        assert!(raw.contains("\"version\": 2"), "on-disk registry must be v2, got: {raw}");
    }

    #[test]
    fn v2_roundtrips_parent_slug() {
        let dir = tempfile::tempdir().unwrap();
        let reg = Registry {
            version: 2,
            folders: vec![uf("nets", None), uf("ares", Some("nets"))],
        };
        save_registry(dir.path(), &reg).unwrap();
        let loaded = load_registry(dir.path());
        assert_eq!(loaded.folders, reg.folders);
    }

    #[test]
    fn new_registry_default_is_current_version() {
        assert_eq!(Registry::default().version, CURRENT_REGISTRY_VERSION);
    }

    #[test]
    fn top_level_folder_serializes_without_parent_key() {
        // A4 / finding #7: Option must be ABSENT, not null, so TS parentSlug?:string holds.
        let json = serde_json::to_string(&uf("nets", None)).unwrap();
        assert!(!json.contains("parent_slug"), "top-level folder must omit the key: {json}");
        let json_child = serde_json::to_string(&uf("ares", Some("nets"))).unwrap();
        assert!(json_child.contains("\"parent_slug\":\"nets\""), "{json_child}");
    }

    #[test]
    fn future_version_is_preserved_and_save_refused() {
        // finding #6: a v99 file must NOT be silently defaulted or overwritten.
        let dir = tempfile::tempdir().unwrap();
        let v99 = r#"{"version":99,"folders":[
            {"slug":"future","display_name":"Future","created_at":"2026-06-02T22:00:00Z","wat":true}
        ]}"#;
        fs::write(dir.path().join(REGISTRY_FILENAME), v99).unwrap();
        let reg = load_registry(dir.path());
        assert_eq!(reg.version, 99, "newer version must be preserved, not normalized");
        assert!(save_registry(dir.path(), &reg).is_err(), "writing a newer registry must be refused");
    }

    #[test]
    fn normalize_self_heals_dangling_parent_to_top_level() {
        // finding #4: a dangling parent must not make a folder vanish.
        let mut reg = Registry { version: 1, folders: vec![uf("ares", Some("ghost"))] };
        normalize_to_current(&mut reg);
        assert_eq!(reg.version, CURRENT_REGISTRY_VERSION);
        assert_eq!(reg.folders[0].parent_slug, None);
        assert!(validate_registry(&reg).is_empty());
    }

    #[test]
    fn normalize_self_heals_depth_three_to_top_level() {
        let mut reg = Registry {
            version: 1,
            folders: vec![uf("nets", None), uf("ares", Some("nets")), uf("kingco", Some("ares"))],
        };
        normalize_to_current(&mut reg);
        // kingco pointed at a subfolder (depth 3) → promoted to top level.
        let kingco = reg.folders.iter().find(|f| f.slug == "kingco").unwrap();
        assert_eq!(kingco.parent_slug, None);
        assert!(validate_registry(&reg).is_empty());
    }

    #[test]
    fn validate_registry_flags_problems() {
        let reg = Registry {
            version: 2,
            folders: vec![uf("a", Some("a")), uf("b", Some("ghost"))],
        };
        let problems = validate_registry(&reg);
        assert!(problems.iter().any(|p| p.contains("own parent")), "{problems:?}");
        assert!(problems.iter().any(|p| p.contains("dangling")), "{problems:?}");
    }

    #[test]
    fn children_slugs_returns_direct_children_only() {
        let reg = Registry {
            version: 2,
            folders: vec![uf("nets", None), uf("ares", Some("nets")), uf("satern", Some("nets")), uf("weather", None)],
        };
        let mut kids = children_slugs(&reg, "nets");
        kids.sort();
        assert_eq!(kids, vec!["ares".to_string(), "satern".to_string()]);
        assert!(children_slugs(&reg, "weather").is_empty());
    }

    #[test]
    fn validate_create_parent_requires_existing_top_level() {
        let reg = Registry {
            version: 2,
            folders: vec![uf("nets", None), uf("ares", Some("nets"))],
        };
        assert!(validate_create_parent(&reg, "nets").is_ok());
        assert!(validate_create_parent(&reg, "ghost").is_err()); // missing
        assert!(validate_create_parent(&reg, "ares").is_err()); // subfolder
    }

    #[test]
    fn validate_reparent_enforces_d4_rule_set() {
        let reg = Registry {
            version: 2,
            folders: vec![uf("nets", None), uf("ares", Some("nets")), uf("weather", None)],
        };
        // self-parent
        assert!(validate_reparent(&reg, "nets", Some("nets")).is_err());
        // parent is a subfolder (cap)
        assert!(validate_reparent(&reg, "weather", Some("ares")).is_err());
        // missing parent
        assert!(validate_reparent(&reg, "weather", Some("ghost")).is_err());
        // moving a folder-with-children under a parent → depth 3
        assert!(validate_reparent(&reg, "nets", Some("weather")).is_err());
        // unknown source
        assert!(validate_reparent(&reg, "ghost", Some("nets")).is_err());
        // valid: leaf under a top-level
        assert!(validate_reparent(&reg, "weather", Some("nets")).is_ok());
        // valid: promote a folder-with-children to top level
        assert!(validate_reparent(&reg, "nets", None).is_ok());
    }
}
