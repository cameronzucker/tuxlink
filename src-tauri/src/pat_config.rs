//! Pat config render at PatProcess spawn time.
//!
//! Spec: docs/superpowers/specs/2026-05-19-pat-config-render-design.md (v2)
//! bd issue: tuxlink-756
//!
//! Why this module exists: the cred-handling refactor (tuxlink-pat#2 + tuxlink#59)
//! deleted `cfg.SecureLoginPassword` and `AuxAddr.Password` from Pat's config;
//! passwords now live in the OS keyring. The wizard writes tuxlink's config +
//! the keyring entry but does NOT write Pat's own `~/.config/pat/config.json`.
//! Without something filling that gap, Pat spawns with no callsign / no locator
//! and is nonfunctional. This module renders Pat's config from tuxlink's config
//! at PatProcess::spawn time, closing the gap.
//!
//! See spec §3 for the field-mapping rationale + per-field decisions.
//! See spec §3.8 for the deferred-work surface (CMS-SSL routing → v0.5 Step 5).

use serde::Serialize;
use std::io::Write;
use std::path::Path;
use thiserror::Error;

use crate::config::Config as TuxlinkConfig;

/// Pat config schema fields populated by this renderer. Kept as a sorted const
/// slice so future drift between this renderer and Pat's actual expected fields
/// is easy to spot in tests.
pub const PAT_CONFIG_SCHEMA_FIELDS: &[&str] = &[
    "auto_download_size_limit",
    "auxiliary_addresses",
    "http_addr",
    "locator",
    "mycall",
    "service_codes",
];

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum PatConfigError {
    /// Required field missing from tuxlink config — caller passed a Config that
    /// doesn't carry enough information to render a working Pat config. Most
    /// common: `connect_to_cms = true` but `identity.callsign = None`.
    /// `Config::validate` should have rejected this upstream; defense-in-depth.
    #[error("Pat config render: required field missing: {0}")]
    MissingRequiredField(String),

    /// Caller passed an offline-mode tuxlink config. No Pat config should be
    /// written when tuxlink runs in offline mode (no Pat process spawned).
    /// This is a caller bug; calling code should not invoke pat_config render
    /// when `connect.connect_to_cms = false`.
    #[error("Pat config render called with offline-mode tuxlink config")]
    OfflineModeNoConfigNeeded,

    /// `serde_json::to_string` failed during render. Should never happen for
    /// our schema (no Float NaN, no map with non-string keys). Source preserved.
    #[error("Pat config render: serde error: {0}")]
    RenderFailed(#[source] serde_json::Error),

    /// File I/O failed during atomic write (tempfile creation, persist, fsync).
    #[error("Pat config write: I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Render Pat's `config.json` content from tuxlink's config. Pure function —
/// no I/O. Returns the JSON as a `String`.
///
/// Returns `Err(MissingRequiredField)` if `connect.connect_to_cms = true` but
/// `identity.callsign = None`. Returns `Err(OfflineModeNoConfigNeeded)` if
/// `connect.connect_to_cms = false`.
pub fn render_pat_config(tuxlink_config: &TuxlinkConfig) -> Result<String, PatConfigError> {
    if !tuxlink_config.connect.connect_to_cms {
        return Err(PatConfigError::OfflineModeNoConfigNeeded);
    }
    let callsign = tuxlink_config.identity.callsign.as_deref().ok_or_else(|| {
        PatConfigError::MissingRequiredField("identity.callsign".to_string())
    })?;

    let pat_config = PatConfigDto {
        mycall: callsign.to_string(),
        auxiliary_addresses: vec![],
        locator: tuxlink_config
            .identity
            .grid
            .as_deref()
            .unwrap_or("")
            .to_string(),
        auto_download_size_limit: -1,
        service_codes: vec!["PUBLIC".to_string()],
        http_addr: String::new(),
    };

    serde_json::to_string_pretty(&pat_config).map_err(PatConfigError::RenderFailed)
}

/// Render + atomically write Pat config to `dest`. Mirrors
/// `crate::config::write_config_atomic`'s exact pattern (per tuxlink-756 v2
/// Codex R1 P1 #1): same-directory tempfile → write via tempfile handle →
/// fsync tempfile → persist → fsync parent directory (surfacing errors).
///
/// Creates `dest`'s parent directory if it doesn't exist (matches the
/// `XDG_CONFIG_HOME/pat/` first-run case).
pub fn write_pat_config_atomic(
    tuxlink_config: &TuxlinkConfig,
    dest: &Path,
) -> Result<(), PatConfigError> {
    let json = render_pat_config(tuxlink_config)?;

    // Normalize parent path (R1 P2 #1): filter out empty-string parent
    // returned for basename-only relative paths (e.g., dest = "pat-config.json").
    let parent = dest
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    std::fs::create_dir_all(parent)?;

    // Same-directory tempfile → write via tempfile HANDLE (not path) → fsync
    // → persist (atomic rename) → fsync parent dir.
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.as_file_mut().write_all(json.as_bytes())?;
    tmp.as_file().sync_all()?;
    tmp.persist(dest).map_err(|e| PatConfigError::Io(e.error))?;

    // Parent-dir fsync (R1 P1 #1): rename(2) is atomic but not DURABLE until
    // the parent directory's metadata flushes. SURFACE errors.
    let parent_dir = std::fs::File::open(parent)?;
    parent_dir.sync_all()?;
    Ok(())
}

/// Wire-format DTO for Pat's Config struct. Field names match Pat's
/// `json:"..."` tags (snake_case). Only fields tuxlink-756 populates; Pat's
/// UnmarshalJSON tolerates missing fields per Go's default behavior.
#[derive(Debug, Serialize)]
struct PatConfigDto {
    mycall: String,
    auxiliary_addresses: Vec<String>, // always empty in v0.0.1
    locator: String,
    auto_download_size_limit: i64,
    service_codes: Vec<String>,
    http_addr: String,
}
