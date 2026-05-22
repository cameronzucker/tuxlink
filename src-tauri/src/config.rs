//! Tuxlink configuration types + validators + atomic-write surface.
//!
//! Spec: docs/superpowers/specs/2026-05-18-task-2-config-impl-design.md
//! bd issue: tuxlink-4mt

use serde::{Deserialize, Deserializer, Serialize};

pub const CONFIG_SCHEMA_VERSION: u32 = 1;

/// Top-level config struct. `deny_unknown_fields` is the AMD-11 drift defense:
/// any stale field (e.g. `winlink_password_present` from the pre-AMD-1 flat schema)
/// hard-fails at deserialize time rather than silently being dropped.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(deserialize_with = "deserialize_schema_version")]
    pub schema_version: u32,
    pub wizard_completed: bool,
    pub connect: ConnectConfig,
    pub identity: IdentityConfig,
    pub privacy: PrivacyConfig,
    #[serde(deserialize_with = "deserialize_optional_nonempty_string", default)]
    pub pat_mbo_address: Option<String>,
    // winlink_password_present REMOVED per AMD-11; deny_unknown_fields catches drift.
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectConfig {
    /// Set by wizard Task 9. False = offline-only deployment.
    pub connect_to_cms: bool,
    /// Per the transport-visibility anti-pattern: always explicit, never auto-selected.
    pub transport: CmsTransport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum CmsTransport {
    /// Port 8773, TLS-wrapped. v0.0.1 default.
    CmsSsl,
    /// Port 8772, plaintext. For networks blocking port 8773.
    Telnet,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityConfig {
    /// Required when `connect_to_cms = true` (CMS path requires callsign).
    /// Must be absent (`None`) when `connect_to_cms = false` (offline path forbids callsign;
    /// use `identifier` instead). Enforced by `Config::validate`. Loose validator per
    /// `validate_identity()`: nonempty + no whitespace + ≤32 + ASCII-printable.
    #[serde(deserialize_with = "deserialize_optional_nonempty_string", default)]
    pub callsign: Option<String>,
    /// Free-form station identifier for offline-mode operators (optional).
    /// Allowed on the offline path (`connect_to_cms = false`); not validated as required
    /// in v0.0.1. Same loose-validator rules as `callsign`.
    #[serde(deserialize_with = "deserialize_optional_nonempty_string", default)]
    pub identifier: Option<String>,
    /// Maidenhead grid, stored at full 6-char precision when known. Broadcast precision is
    /// governed by PrivacyConfig.position_precision (per Principle 7).
    #[serde(deserialize_with = "deserialize_optional_nonempty_string", default)]
    pub grid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivacyConfig {
    pub gps_state: GpsState,
    pub position_precision: PositionPrecision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum GpsState {
    /// No GPS device read at all.
    Off,
    /// GPS read locally; never broadcast.
    LocalUiOnly,
    /// Default. GPS read + broadcast at the chosen precision.
    BroadcastAtPrecision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum PositionPrecision {
    /// Default. Broadcasts 4-char Maidenhead (~1°).
    FourCharGrid,
    /// Opt-in. Broadcasts full 6-char (~5km).
    SixCharGrid,
}

/// Reduce a grid stored at full precision to the form that may leave the
/// application on air (tuxlink-882). The grid is *stored* at full 6-char
/// precision; this is the privacy boundary: `FourCharGrid` (default) yields the
/// first 4 characters, `SixCharGrid` (opt-in) the first 6. Char-based truncation
/// is safe for ASCII Maidenhead locators. Any broadcast surface (the CMS handshake
/// locator today) MUST pass through this rather than the raw stored grid.
pub fn broadcast_grid(grid: &str, precision: PositionPrecision) -> String {
    let keep = match precision {
        PositionPrecision::FourCharGrid => 4,
        PositionPrecision::SixCharGrid => 6,
    };
    grid.chars().take(keep).collect()
}

fn deserialize_schema_version<'de, D>(d: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let v = u32::deserialize(d)?;
    if v != CONFIG_SCHEMA_VERSION {
        return Err(serde::de::Error::custom(format!(
            "unsupported config schema_version {} (expected {})",
            v, CONFIG_SCHEMA_VERSION
        )));
    }
    Ok(v)
}

fn deserialize_optional_nonempty_string<'de, D>(d: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    // Maps JSON `null` → None; maps JSON `""` → None (treat empty-string as missing);
    // maps non-empty string → Some(s). Eliminates Some("") ambiguity per spec adrev R4 P1-1.
    let opt = <Option<String>>::deserialize(d)?;
    Ok(opt.filter(|s| !s.is_empty()))
}

/// Loose identity validator. Matches Express's `hs30.htm` "checked for basic syntax" semantics:
/// non-empty + ASCII-printable + no internal whitespace + ≤32 chars (in that order so the most
/// actionable error fires first). The CMS is authoritative for actual callsign / tactical-address
/// acceptance.
///
/// Returns `true` if `s` passes ALL rules; `false` otherwise. Use [`validate_identity_describe`]
/// to obtain the first-violated-rule slug for error synthesis.
pub fn validate_identity(s: &str) -> bool {
    validate_identity_describe(s).is_none()
}

/// Returns `Some(static-rule-slug)` for the FIRST rule violated, or `None` if input passes all rules.
/// Rule order: empty → ASCII → whitespace → length (most-actionable first per spec adrev R2 P1-3 + R4 P1-2).
pub fn validate_identity_describe(s: &str) -> Option<&'static str> {
    if s.is_empty() { return Some("must not be empty"); }
    if s.chars().any(|c| !c.is_ascii() || c.is_ascii_control()) { return Some("must be ASCII-printable"); }
    if s.chars().any(char::is_whitespace) { return Some("must not contain whitespace"); }
    if s.chars().count() > 32 { return Some("must be ≤32 chars"); }
    None
}

/// Resolve the config file path. Honors XDG_CONFIG_HOME, falls back to
/// ~/.config/tuxlink/config.json.
pub fn config_path() -> std::path::PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").expect("HOME must be set");
            std::path::PathBuf::from(home).join(".config")
        });
    base.join("tuxlink").join("config.json")
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigValidationError {
    #[error("CMS path requires identity.callsign to be set")]
    CmsPathMissingCallsign,
    #[error("offline path must NOT have identity.callsign set (use identity.identifier instead)")]
    OfflinePathHasCallsign,
    #[error("invalid identity field `{field}`: {rule}")]
    InvalidIdentity { field: &'static str, rule: &'static str },
}

impl Config {
    /// Cross-field semantic validation (can't be expressed via serde deserialize-with).
    /// Callers (wizard's `wizard_persist_cms`, `read_config`) invoke after deserialization.
    /// NOT auto-called by `write_config_atomic` — caller responsibility per spec §3.3.
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        if self.connect.connect_to_cms && self.identity.callsign.is_none() {
            return Err(ConfigValidationError::CmsPathMissingCallsign);
        }
        if !self.connect.connect_to_cms && self.identity.callsign.is_some() {
            return Err(ConfigValidationError::OfflinePathHasCallsign);
        }
        if let Some(ref c) = self.identity.callsign {
            if let Some(rule) = validate_identity_describe(c) {
                return Err(ConfigValidationError::InvalidIdentity { field: "callsign", rule });
            }
        }
        if let Some(ref i) = self.identity.identifier {
            if let Some(rule) = validate_identity_describe(i) {
                return Err(ConfigValidationError::InvalidIdentity { field: "identifier", rule });
            }
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigReadError {
    #[error("config file not found at {path}")]
    NotFound { path: std::path::PathBuf },
    #[error("io error reading {path}: {source}")]
    Io { path: std::path::PathBuf, #[source] source: std::io::Error },
    #[error("config deserialize failed: {source}")]
    Serde { #[source] source: serde_json::Error },
    #[error("config failed semantic validation: {source}")]
    Validation { #[source] source: ConfigValidationError },
}

/// Read + parse + validate the config at `config_path()`. Returns typed errors per spec §3.5.
/// Consumers: wizard plan line 525 (wizard_persist_offline) + line 617 (get_wizard_completed) —
/// both use `.ok()` to fold any error into None (first-run, malformed, etc.) and fall through
/// to a fresh wizard run.
pub fn read_config() -> Result<Config, ConfigReadError> {
    let path = config_path();
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(ConfigReadError::NotFound { path });
        }
        Err(e) => return Err(ConfigReadError::Io { path, source: e }),
    };
    let config: Config = serde_json::from_slice(&bytes)
        .map_err(|source| ConfigReadError::Serde { source })?;
    config.validate()
        .map_err(|source| ConfigReadError::Validation { source })?;
    Ok(config)
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigWriteError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("config serialize failed: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("refuse to overwrite existing config with schema_version {existing} (this binary supports v{ours}): mismatch — either downgrade (existing > ours) or backward-incompat (existing < ours)")]
    SchemaVersionMismatch { existing: u32, ours: u32 },
    #[error("refuse to overwrite existing config at {path:?}: file is a symlink (target: {target:?})")]
    ExistingFileIsSymlink { path: std::path::PathBuf, target: Option<std::path::PathBuf> },
    #[error("config path {path:?} cannot be probed: {source}")]
    ProbeReadFailed { path: std::path::PathBuf, #[source] source: std::io::Error },
    #[error("config path {path:?} has no parent directory")]
    NoParentDirectory { path: std::path::PathBuf },
}

/// Atomic single-write of `config` to `config_path()`. Returns typed errors per spec §3.4.
///
/// Atomicity contract scope: local POSIX FS (ext4/btrfs/xfs/APFS) where target file +
/// tempfile are on the same FS AND the same BTRFS subvolume. NFS / FUSE / Lustre semantics
/// undefined; BTRFS subvolume-boundary case lapses atomicity silently.
///
/// Single-instance assumption: ONE tuxlink instance writes at a time. Cross-process
/// serialization (flock) out of scope for v0.0.1.
///
/// Does NOT auto-call `config.validate()` — caller responsibility per spec §3.3.
pub fn write_config_atomic(config: &Config) -> Result<(), ConfigWriteError> {
    let path = config_path();
    let parent = path.parent()
        .ok_or_else(|| ConfigWriteError::NoParentDirectory { path: path.clone() })?;
    std::fs::create_dir_all(parent)?;

    // Symlink-detection (spec §3.4 per adrev R4 P0-2): refuse to silently replace a symlink.
    if let Ok(meta) = std::fs::symlink_metadata(&path) {
        if meta.file_type().is_symlink() {
            return Err(ConfigWriteError::ExistingFileIsSymlink {
                path: path.clone(),
                target: std::fs::read_link(&path).ok(),
            });
        }
    }

    // Schema-version mismatch refusal (both directions per adrev R4 P1-5).
    // Tolerates unparseable bytes (first-run + corruption-recovery cases).
    // Distinguishes NotFound (proceed) from other I/O errors (abort) per adrev R4 P1-4.
    match std::fs::read(&path) {
        Ok(bytes) => {
            if let Ok(probe) = serde_json::from_slice::<SchemaVersionProbe>(&bytes) {
                if probe.schema_version != CONFIG_SCHEMA_VERSION {
                    return Err(ConfigWriteError::SchemaVersionMismatch {
                        existing: probe.schema_version,
                        ours: CONFIG_SCHEMA_VERSION,
                    });
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(ConfigWriteError::ProbeReadFailed { path: path.clone(), source: e });
        }
    }

    // Same-directory tempfile → atomic persist on local POSIX FS.
    let tmp = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(tmp.as_file(), config)?;
    tmp.as_file().sync_all()?;
    tmp.persist(&path).map_err(|e| ConfigWriteError::Io(e.error))?;

    // Parent-dir fsync per adrev R2 P0-3 + R4 P0-1: rename(2) is atomic but not DURABLE
    // until the parent directory's metadata flushes. tempfile::persist does not do this.
    let parent_dir = std::fs::File::open(parent)?;
    parent_dir.sync_all()?;
    Ok(())
}

#[derive(serde::Deserialize)]
struct SchemaVersionProbe { schema_version: u32 }

#[cfg(test)]
mod tests {
    use super::*;

    // tuxlink-882: the privacy boundary. The grid is stored full; what may go on
    // air is reduced to the configured precision — 4 chars by default, 6 on opt-in.
    #[test]
    fn broadcast_grid_default_four_char_reduces_six_char_stored_grid() {
        assert_eq!(broadcast_grid("CN87ux", PositionPrecision::FourCharGrid), "CN87");
    }

    #[test]
    fn broadcast_grid_six_char_optin_keeps_full_precision() {
        assert_eq!(broadcast_grid("CN87ux", PositionPrecision::SixCharGrid), "CN87ux");
    }

    #[test]
    fn broadcast_grid_is_a_noop_when_stored_grid_already_short() {
        // A 4-char stored grid stays 4-char under either setting (nothing to reveal).
        assert_eq!(broadcast_grid("CN87", PositionPrecision::FourCharGrid), "CN87");
        assert_eq!(broadcast_grid("CN87", PositionPrecision::SixCharGrid), "CN87");
    }

    #[test]
    fn broadcast_grid_handles_empty() {
        assert_eq!(broadcast_grid("", PositionPrecision::FourCharGrid), "");
    }
}
