//! Tuxlink configuration types + validators + atomic-write surface.
//!
//! Spec: docs/superpowers/specs/2026-05-18-task-2-config-impl-design.md
//! bd issue: tuxlink-4mt

use crate::winlink::ax25::KissLinkConfig;
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
    #[deprecated(
        note = "pat_mbo_address is unused after the Pat strip (ADR 0016); future writers \
                should not set it. Tracked for removal in a future major bump."
    )]
    #[serde(deserialize_with = "deserialize_optional_nonempty_string", default, skip_serializing)]
    pub pat_mbo_address: Option<String>,
    // winlink_password_present REMOVED per AMD-11; deny_unknown_fields catches drift.
    /// AX.25 packet transport settings (additive; defaults when absent). See
    /// `PacketConfig`. `#[serde(default)]` is the migration for old files.
    #[serde(default)]
    pub packet: PacketConfig,
    /// ARDOP HF modem settings (additive; absent until the operator configures ARDOP).
    /// `#[serde(default)]` migrates old config files that predate this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modem_ardop: Option<ArdopUiConfig>,
    /// VARA modem settings (additive; absent until the operator configures VARA).
    /// `#[serde(default)]` migrates old config files that predate this field.
    /// VARA is a third-party closed-source modem that runs as a separate
    /// process exposing two TCP sockets (cmd + data). Tuxlink connects as a
    /// client; tuxlink does NOT manage the VARA process lifecycle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modem_vara: Option<VaraUiConfig>,
    /// Telnet-P2P listener settings (additive; defaults when absent). The
    /// allowlist + station password live OUTSIDE this struct (the allowlist in
    /// `<config-dir>/listener/telnet/allowed_stations.json`, the password in
    /// the OS keyring); this struct carries only the bind + TTL knobs.
    ///
    /// bd: tuxlink-xehu
    #[serde(default)]
    pub telnet_listen: TelnetListenUiConfig,
    /// Network Post Office relay favorites (additive; empty when absent).
    /// `#[serde(default)]` migrates old config files that predate this field.
    /// bd: tuxlink-6c9y.
    #[serde(default)]
    pub network_po_favorites: Vec<RelayFavorite>,
    /// Prompt the operator to select which pending inbound messages to download
    /// on a CMS connect (WLE "Review Pending Messages" parity), instead of
    /// auto-downloading all. Default TRUE = review before download (the WLE emcomm
    /// default); operators opt out to auto-download-all via the dashboard ribbon's
    /// "On connect" control (tuxlink-pmp5). `#[serde(default = ...)]` migrates
    /// configs that predate this field (absent → true), satisfying
    /// `deny_unknown_fields` (the field is now KNOWN).
    #[serde(default = "default_review_inbound_before_download")]
    pub review_inbound_before_download: bool,
    /// LAN map-tile source (tuxlink-dyop Phase 8). `None` = no LAN source
    /// configured; the map serves the bundled offline base map (`StatusKind::Bundled`).
    /// Set by `configure_tile_source` only AFTER the source validates+activates
    /// (geodetic CRS + reachable host serving a real image). Carries NO auth field
    /// by design — `TileSource` has no credentials, so no secret is ever written to
    /// disk (keyring-later if auth is ever needed). `#[serde(default)]` migrates
    /// configs that predate this field (absent → `None`); the field is now KNOWN,
    /// so `deny_unknown_fields` is satisfied. `skip_serializing_if` keeps a no-source
    /// config byte-identical to its pre-dyop shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map_tile_source: Option<crate::tiles::TileSource>,
}

/// A saved Network Post Office relay server entry.
///
/// Dedup key: `(host case-insensitive, port)`. The `callsign` and `label`
/// are display/routing metadata only — they do NOT affect the dedup check.
/// `deny_unknown_fields` is the AMD-11 drift defense for this sub-type too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelayFavorite {
    /// Callsign of the relay station (display + B2F login). Non-empty.
    pub callsign: String,
    /// Operator-supplied human label (e.g. "Home mesh relay"). May be empty.
    pub label: String,
    /// Hostname or IP address. Non-empty. Dedup key (case-insensitive).
    pub host: String,
    /// TCP port. Dedup key (exact). Default relay port is 8772.
    pub port: u16,
}

/// Serde default for [`Config::review_inbound_before_download`]: `true` — review
/// before download, the WLE emcomm default (tuxlink-pmp5). A free fn because
/// serde's `default = "..."` takes a path and `bool`'s own `Default` is `false`.
fn default_review_inbound_before_download() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectConfig {
    /// Set by wizard Task 9. False = offline-only deployment.
    pub connect_to_cms: bool,
    /// Per the transport-visibility anti-pattern: always explicit, never auto-selected.
    pub transport: CmsTransport,
    /// CMS server host the operator dials (tuxlink-3o0). User-switchable in the
    /// inline SettingsPanel, replacing the former hardcoded `CMS_HOST` const +
    /// hidden `TUXLINK_CMS_HOST` env var (env stays a dev override on top of this).
    /// Default `cms-z.winlink.org` (the dev target that accepts the unregistered
    /// client; production `server.winlink.org` rejects it until tuxlink is
    /// registered). `#[serde(default)]` migrates pre-3o0 configs (no `host` key)
    /// transparently — `host` is now a KNOWN field, so `deny_unknown_fields` is
    /// satisfied.
    #[serde(default = "default_cms_host")]
    pub host: String,
}

/// The default CMS host (tuxlink-3o0). `cms-z.winlink.org` is the dev target that
/// accepts tuxlink's unregistered client SID; production `server.winlink.org`
/// rejects it until tuxlink is registered with Winlink. Mirrors the former
/// `winlink_backend::CMS_HOST` const value. `pub` so the wizard (first-run config
/// construction) and tests can reference the single canonical default.
pub fn default_cms_host() -> String {
    "cms-z.winlink.org".into()
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum PositionSource {
    /// Operator has manually entered a grid square; GPS is not used for position.
    Manual,
    /// Default. Position is derived from the GPS receiver.
    Gps,
}

fn default_position_source() -> PositionSource {
    PositionSource::Gps
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivacyConfig {
    pub gps_state: GpsState,
    pub position_precision: PositionPrecision,
    /// Active position source (tuxlink-686). Default `Gps` (GPS-on-by-default
    /// convention); a deliberate manual grid entry pins this to `Manual` at runtime.
    /// `#[serde(default)]` migrates pre-686 configs transparently (additive field).
    #[serde(default = "default_position_source")]
    pub position_source: PositionSource,
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

/// Serde-friendly mirror of P2's `winlink::ax25::Ax25Params` (which carries a
/// `Duration` that does not round-trip JSON cleanly). Persisted form stores the
/// T1 timer as milliseconds; `into_params()` converts to the runtime type.
/// Defaults are the 1200-baud values (match `Ax25Params::default`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Ax25ParamsConfig {
    pub txdelay: u8,
    pub persistence: u8,
    pub slot_time: u8,
    pub paclen: u16,
    pub maxframe: u8,
    pub t1_ms: u64,
    pub n2_retries: u8,
}

impl Default for Ax25ParamsConfig {
    fn default() -> Self {
        // 1200-baud defaults; cross-checked against P2's Ax25Params::default.
        Ax25ParamsConfig {
            txdelay: 30,
            persistence: 63,
            slot_time: 10,
            paclen: 128,
            maxframe: 4,
            t1_ms: 3000,
            n2_retries: 10,
        }
    }
}

impl Ax25ParamsConfig {
    /// Convert to P2's runtime `Ax25Params` type. T1 is honored verbatim — tuxlink-2y4
    /// REVERTED the uhc RF floor (`MIN_RF_T1_MS`): it tripled worst-case airtime
    /// (3 s → 10 s per retransmit) and was the wrong lever. Runaway connect airtime is
    /// now bounded by the connect cap (`Ax25Params::connect_timeout` + a ≤2-SABM key
    /// limit in `datalink::connect`), not by inflating the retransmit timer.
    pub fn into_params(self) -> crate::winlink::ax25::Ax25Params {
        crate::winlink::ax25::Ax25Params {
            txdelay: self.txdelay,
            persistence: self.persistence,
            slot_time: self.slot_time,
            paclen: self.paclen as usize,
            maxframe: self.maxframe,
            t1: std::time::Duration::from_millis(self.t1_ms),
            n2_retries: self.n2_retries,
            // connect_timeout (the RADIO-1 airtime ceiling) is a fixed safety default,
            // not yet operator-tunable from the persisted [packet] section.
            ..Default::default()
        }
    }
}

/// The `[packet]` config section (spec §4.5): the AX.25 packet transport's
/// sticky, persisted settings. Global station SSID is sticky across restarts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PacketConfig {
    /// Global, sticky station SSID (0–15). Operate as `<callsign>-<ssid>`.
    pub ssid: u8,
    /// The last KISS link the operator used (TCP host:port or serial device+baud).
    /// `None` until the operator configures one. Deserialized leniently (tuxlink-efo):
    /// an unrecognized variant degrades to `None` instead of bricking the whole read.
    #[serde(default, deserialize_with = "deserialize_lenient_link")]
    pub link: Option<KissLinkConfig>,
    /// AX.25 timing/windowing knobs (1200-baud defaults).
    pub params: Ax25ParamsConfig,
    /// Idle-listening default-on (spec §4.5): arm `answer()` when not dialing.
    pub listen_default: bool,
}

/// Deserialize `packet.link` leniently (tuxlink-efo): an unrecognized variant
/// (forward/sideways schema skew across concurrent dev builds — the original symptom
/// was a Bluetooth-aware build's config bricking a non-Bluetooth build) degrades to
/// `None` rather than erroring the whole config read. Reads the value as a generic
/// JSON value first (always succeeds for valid JSON), then tries to convert it to a
/// `KissLinkConfig`; any failure (unknown variant, missing/extra fields) yields
/// `None` so the rest of the config still loads.
fn deserialize_lenient_link<'de, D>(de: D) -> Result<Option<KissLinkConfig>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(de)?;
    Ok(value.and_then(|v| serde_json::from_value::<KissLinkConfig>(v).ok()))
}

impl Default for PacketConfig {
    fn default() -> Self {
        PacketConfig {
            ssid: 0,
            link: None,
            params: Ax25ParamsConfig::default(),
            listen_default: true, // spec §4.5: listen is default-on
        }
    }
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

/// Resolve the config file path. Precedence: `TUXLINK_CONFIG_DIR` (tuxlink-efo dev
/// override) > `XDG_CONFIG_HOME` > `~/.config`, ending in `.../tuxlink/config.json`
/// (or `<TUXLINK_CONFIG_DIR>/config.json`).
pub fn config_path() -> std::path::PathBuf {
    resolve_config_path(
        std::env::var_os("TUXLINK_CONFIG_DIR"),
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
}

/// Pure resolver behind [`config_path`] (testable without process-global env).
/// `TUXLINK_CONFIG_DIR` (tuxlink-efo) is a tuxlink-specific override so a per-worktree
/// dev build points at its OWN config dir — concurrent builds then stop contaminating
/// one shared `~/.config/tuxlink/config.json` (the dev cousin of the Vite :1420
/// collision). The dir holds `config.json` directly. Falls back to `XDG_CONFIG_HOME`,
/// then `~/.config`.
fn resolve_config_path(
    tuxlink_config_dir: Option<std::ffi::OsString>,
    xdg_config_home: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> std::path::PathBuf {
    if let Some(dir) = tuxlink_config_dir {
        return std::path::PathBuf::from(dir).join("config.json");
    }
    let base = xdg_config_home
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let home = home.expect("HOME must be set");
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
    #[error("packet.ssid {ssid} is out of the 0–15 AX.25 range")]
    PacketSsidOutOfRange { ssid: u8 },
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
        if self.packet.ssid > 15 {
            return Err(ConfigValidationError::PacketSsidOutOfRange { ssid: self.packet.ssid });
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

/// Frontend-shaped ARDOP HF modem settings. Persisted as `[modem_ardop]` in config;
/// Task 3.3 (`modem_ardop_connect`) translates this into `ArdopConfig::extra_args` at
/// spawn time. `deny_unknown_fields` is intentionally absent here — the ARDOP config
/// is additive and forward-compat relaxation is acceptable for a new section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArdopUiConfig {
    /// Path or name of the ARDOP binary (e.g. `"ardopcf"`).
    pub binary: String,
    /// ALSA capture device (e.g. `"plughw:1,0"`).
    pub capture_device: String,
    /// ALSA playback device (e.g. `"plughw:1,0"`).
    pub playback_device: String,
    /// Serial device for PTT control. `None` = VOX or CAT PTT.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ptt_serial_path: Option<String>,
    /// ARDOP command/control port (default 8515).
    pub cmd_port: u16,
    /// ARDOP ARQ bandwidth in Hz. One of {200, 500, 1000, 2000}. None means
    /// "let ardopcf use its default" (typically 2000 Hz, but the operator may
    /// have set a different default via the WebGUI or persistent config).
    /// The value, if set, is sent as `ARQBW <hz> FORCED` during init_tnc so
    /// the client-side preference overrides the server's preference for
    /// outbound calls (tuxlink-j0ij).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bandwidth_hz: Option<u32>,
    /// ardopcf built-in WebGUI port. `None` (the default) means "derive from
    /// `cmd_port - 1`" per ardopcf's documented convention (default
    /// 8515 → 8514). An explicit `Some(port)` overrides the derivation so the
    /// operator can pin the WebGUI to a known port when ardopcf is built or
    /// configured to bind somewhere non-standard.
    ///
    /// Persisted shape decided by operator smoke 2026-05-31 round 3: the
    /// "Open WebGUI" button targets `http://localhost:<webgui_port>/` and the
    /// spawn passes `-G <webgui_port>` — both ends MUST read from the same
    /// source, so the derivation logic and the override flag are colocated on
    /// the config struct rather than recomputed at each call site.
    ///
    /// Use [`ArdopUiConfig::resolved_webgui_port`] to read the effective
    /// port; do not access this field directly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webgui_port: Option<u16>,
}

impl ArdopUiConfig {
    /// Resolve the effective WebGUI port: explicit `webgui_port` if set,
    /// otherwise `cmd_port - 1` (ardopcf's documented convention).
    ///
    /// Returns `None` when `cmd_port < 2` AND no explicit override is set —
    /// that case can't derive a valid bindable TCP port and the WebGUI is
    /// disabled in the spawn (no `-G` flag emitted). Both `build_ardop_extra_args`
    /// and the frontend `onOpenWebGuiClick` consult this single helper so they
    /// agree on the port regardless of operator overrides.
    pub fn resolved_webgui_port(&self) -> Option<u16> {
        if let Some(p) = self.webgui_port {
            return Some(p);
        }
        if self.cmd_port >= 2 {
            Some(self.cmd_port - 1)
        } else {
            None
        }
    }
}

impl Default for ArdopUiConfig {
    fn default() -> Self {
        Self {
            binary: "ardopcf".into(),
            capture_device: String::new(),
            playback_device: String::new(),
            ptt_serial_path: None,
            cmd_port: 8515,
            bandwidth_hz: None,
            // None → derive from cmd_port - 1 (8514 with the default cmd_port).
            // Operator can pin explicitly via the radio panel when ardopcf's
            // build/config has the WebGUI somewhere non-standard.
            webgui_port: None,
        }
    }
}

/// Frontend-shaped VARA modem settings. Persisted as `[modem_vara]` in config.
/// Phase 2 (bd-tuxlink-dfmf) — minimal TCP-transport config; full session-state
/// integration (B2F over VARA, RADIO-1 connect-to-peer) arrives in Phase 3.
///
/// VARA differs from ARDOP in two ways tuxlink models:
///   1. VARA is a separate third-party process tuxlink does NOT spawn — only
///      `host` + `cmd_port` + `data_port` are needed (no `binary`, no audio
///      device hints; VARA handles its own audio).
///   2. VARA exposes 3 variants — HF Standard (2300 Hz), HF Tactical (2750
///      Hz), and VARA FM (~6800 Hz). The variant is selected operator-side
///      via `bandwidth_hz` and which VARA instance the operator pointed
///      tuxlink at (different binaries listen on different ports).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaraUiConfig {
    /// VARA cmd-socket host. Default `127.0.0.1` (local-machine VARA).
    pub host: String,
    /// VARA command socket port. Default `8300`.
    pub cmd_port: u16,
    /// VARA data socket port. Default `8301` (conventionally `cmd_port + 1`).
    pub data_port: u16,
    /// VARA bandwidth in Hz. Common values: 500 (narrow HF), 2300 (HF
    /// Standard), 2750 (HF Tactical), ~6800 (VARA FM). `None` = leave VARA
    /// at whatever bandwidth it was last configured for (don't send `BW` at
    /// session start).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bandwidth_hz: Option<u32>,
}

impl Default for VaraUiConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            cmd_port: 8300,
            data_port: 8301,
            bandwidth_hz: None,
        }
    }
}

// ============================================================================
// Telnet-P2P listener config (tuxlink-xehu)
// ============================================================================

/// Telnet-P2P listener settings. The allowlist + station password live OUTSIDE
/// this struct (allowlist in `<config-dir>/listener/telnet/allowed_stations.json`,
/// password in the OS keyring) so this struct carries only the bind + TTL knobs.
///
/// ## Defaults (DIVERGE from WLE)
///
/// | Knob       | tuxlink default | WLE default     | Why                                |
/// |------------|-----------------|-----------------|------------------------------------|
/// | `port`     | 8774            | 8774            | parity (telnet-p2p.md §1)          |
/// | `bind_addr`| `"127.0.0.1"`   | `"Default"` ≈ 0.0.0.0 | telnet-p2p.md §9.3 — operator opts into LAN |
/// | `ttl_secs` | 3600 (1 hour)   | infinite        | RADIO-1 framing — arming = consent for armed window |
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct TelnetListenUiConfig {
    /// TCP port the listener binds. Default 8774 per
    /// `dev/scratch/winlink-re/findings/telnet-p2p.md §1` (NOT 8772 — that's
    /// the RMS-Relay hub port).
    pub port: u16,
    /// Bind address. Default `127.0.0.1` (loopback) — DIVERGES from WLE's
    /// "all interfaces" default per telnet-p2p.md §9.3. Operator opts into
    /// LAN/all by setting this to `"0.0.0.0"` or a specific NIC address.
    pub bind_addr: String,
    /// Arm-window TTL in seconds. Default 3600 (1 hour). Operator can set
    /// shorter for narrower consent windows.
    pub ttl_secs: u64,
}

impl Default for TelnetListenUiConfig {
    fn default() -> Self {
        Self {
            port: 8774,
            bind_addr: "127.0.0.1".into(),
            ttl_secs: 3600,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // tuxlink-686: position_source defaults to Gps when the field is absent from an
    // existing (schema_version 1) config. This is the additive-migration test: old
    // config files that predate the field must load without error and resolve Gps.
    #[test]
    fn position_source_defaults_to_gps_when_absent_from_config() {
        let json = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{
                    "gps_state": "BroadcastAtPrecision",
                    "position_precision": "FourCharGrid"
                }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION
        );
        let config: Config = serde_json::from_str(&json)
            .expect("config without position_source should deserialize");
        assert_eq!(
            config.privacy.position_source,
            PositionSource::Gps,
            "missing position_source must default to Gps"
        );
    }

    // tuxlink-3o0: the additive-migration test for `connect.host`. An OLD
    // ConnectConfig JSON (only `connect_to_cms` + `transport`, NO `host` key —
    // the pre-3o0 shape) must deserialize with `host` defaulting to
    // cms-z.winlink.org. `host` is now a KNOWN field, so the struct's
    // `deny_unknown_fields` is satisfied; `#[serde(default = "default_cms_host")]`
    // supplies the value when the key is absent.
    #[test]
    fn connect_host_defaults_to_cms_z_when_absent_from_config() {
        let json = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": true, "transport": "CmsSsl" }},
                "identity": {{ "callsign": "W1TEST", "identifier": null, "grid": null }},
                "privacy": {{
                    "gps_state": "BroadcastAtPrecision",
                    "position_precision": "FourCharGrid"
                }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION
        );
        let config: Config = serde_json::from_str(&json)
            .expect("config without connect.host should deserialize");
        assert_eq!(
            config.connect.host, "cms-z.winlink.org",
            "missing connect.host must default to cms-z.winlink.org"
        );
    }

    // tuxlink-dyop Phase 8: an OLD config JSON that predates `map_tile_source`
    // (the pre-dyop shape — no `map_tile_source` key) must deserialize with the
    // field defaulting to `None`. The field is now KNOWN, so `deny_unknown_fields`
    // is satisfied; `#[serde(default)]` supplies `None` when the key is absent.
    #[test]
    fn map_tile_source_defaults_to_none_when_absent_from_config() {
        let json = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{
                    "gps_state": "BroadcastAtPrecision",
                    "position_precision": "FourCharGrid"
                }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION
        );
        let config: Config = serde_json::from_str(&json)
            .expect("config without map_tile_source should deserialize");
        assert!(
            config.map_tile_source.is_none(),
            "missing map_tile_source must default to None"
        );
    }

    // tuxlink-dyop Phase 8: a configured `map_tile_source` round-trips through
    // serialize → deserialize (proves on-disk persistence of an activated source).
    #[test]
    fn map_tile_source_round_trips_when_set() {
        let json = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{
                    "gps_state": "BroadcastAtPrecision",
                    "position_precision": "FourCharGrid"
                }},
                "map_tile_source": {{
                    "url": "http://192.168.1.5:8080/tiles/",
                    "crs": "Geodetic",
                    "scheme": "Xyz",
                    "minZoom": 0,
                    "maxZoom": 16,
                    "cacheBudgetMb": 384,
                    "attribution": null,
                    "label": "shack"
                }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION
        );
        let config: Config = serde_json::from_str(&json)
            .expect("config with map_tile_source should deserialize");
        let src = config.map_tile_source.as_ref().expect("source present");
        assert_eq!(src.url, "http://192.168.1.5:8080/tiles/");
        assert_eq!(src.label, "shack");
        // Round-trip back out and parse again — the serialized form re-parses
        // (camelCase field names match the TileSource serde contract).
        let reser = serde_json::to_string(&config).unwrap();
        let back: Config = serde_json::from_str(&reser).unwrap();
        assert_eq!(back.map_tile_source.unwrap().url, "http://192.168.1.5:8080/tiles/");
    }

    // tuxlink-3o0: a configured host round-trips (proves persistence, not just
    // the default).
    #[test]
    fn connect_host_round_trips_when_set() {
        let json = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": true, "transport": "Telnet", "host": "server.winlink.org" }},
                "identity": {{ "callsign": "W1TEST", "identifier": null, "grid": null }},
                "privacy": {{
                    "gps_state": "BroadcastAtPrecision",
                    "position_precision": "FourCharGrid"
                }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION
        );
        let config: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(config.connect.host, "server.winlink.org");
        let reserialized = serde_json::to_string(&config).unwrap();
        let reloaded: Config = serde_json::from_str(&reserialized).unwrap();
        assert_eq!(reloaded.connect.host, "server.winlink.org");
    }

    // tuxlink-bsiy: the opt-in `review_inbound_before_download` preference
    // round-trips through serde when set to true (proves persistence, not just
    // the default).
    #[test]
    fn review_inbound_before_download_round_trips_when_true() {
        let mut cfg: Config = serde_json::from_str(&sample_config_json_without_packet()).unwrap();
        cfg.review_inbound_before_download = true;
        let serialized = serde_json::to_string(&cfg).unwrap();
        let reloaded: Config = serde_json::from_str(&serialized).unwrap();
        assert!(
            reloaded.review_inbound_before_download,
            "review_inbound_before_download=true must survive a serialize→deserialize round-trip"
        );
    }

    // tuxlink-pmp5: review-before-download is now the DEFAULT (the WLE emcomm
    // default). An OLD config JSON with NO `review_inbound_before_download` key
    // (every config that predates this field) must deserialize with the field
    // defaulting to TRUE. The field is KNOWN to the struct, so
    // `deny_unknown_fields` stays satisfied; the serde default fn supplies true
    // when the key is absent.
    #[test]
    fn review_inbound_before_download_defaults_true_when_absent_from_config() {
        let json = sample_config_json_without_packet();
        assert!(
            !json.contains("review_inbound_before_download"),
            "fixture must omit the key for this migration test to be meaningful"
        );
        let cfg: Config = serde_json::from_str(&json)
            .expect("config without review_inbound_before_download should deserialize");
        assert!(
            cfg.review_inbound_before_download,
            "missing review_inbound_before_download must default to true (review before download)"
        );
    }

    // tuxlink-efo: a packet.link variant THIS build doesn't know (forward/sideways
    // schema skew across concurrent dev builds — the original symptom was a
    // Bluetooth-aware build's config bricking a non-Bluetooth build) must NOT brick
    // app-open. read_config degrades the unparseable link to None; the rest of the
    // config is preserved.
    #[test]
    fn unknown_packet_link_variant_degrades_to_none_not_brick() {
        let json = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "BroadcastAtPrecision", "position_precision": "FourCharGrid" }},
                "packet": {{ "ssid": 7, "link": {{ "Telepathy": {{ "mac": "00:11:22" }} }} }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION
        );
        let config: Config = serde_json::from_str(&json)
            .expect("an unknown packet.link variant must degrade to None, not error the whole read");
        assert_eq!(config.packet.link, None, "the unknown link variant degrades to None");
        assert_eq!(config.packet.ssid, 7, "the rest of the packet section is preserved");
        assert_eq!(
            config.identity.identifier.as_deref(),
            Some("W1TEST"),
            "identity (and the rest of the config) is preserved through the degradation"
        );
    }

    // tuxlink-efo regression guard: a KNOWN link variant still parses to Some — the
    // lenient degradation must not swallow valid links.
    #[test]
    fn known_packet_link_variant_still_parses_to_some() {
        let json = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "BroadcastAtPrecision", "position_precision": "FourCharGrid" }},
                "packet": {{ "ssid": 7, "link": {{ "Bluetooth": {{ "mac": "38:D2:00:01:55:5C" }} }} }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION
        );
        let config: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(
            config.packet.link,
            Some(KissLinkConfig::Bluetooth { mac: "38:D2:00:01:55:5C".into() }),
            "a known link variant must round-trip to Some, not degrade"
        );
    }

    // tuxlink-efo: a tuxlink-specific config-dir override so a per-worktree dev build
    // points at its OWN config and concurrent builds stop contaminating one shared
    // ~/.config/tuxlink/config.json. Takes precedence over XDG_CONFIG_HOME; the dir
    // holds config.json directly. Tested via the pure resolver (no process-global env).
    #[test]
    fn resolve_config_path_prefers_tuxlink_config_dir() {
        assert_eq!(
            resolve_config_path(Some("/tmp/wt-a".into()), Some("/xdg".into()), Some("/home/u".into())),
            std::path::PathBuf::from("/tmp/wt-a/config.json")
        );
    }

    #[test]
    fn resolve_config_path_falls_back_to_xdg_then_home() {
        assert_eq!(
            resolve_config_path(None, Some("/xdg".into()), Some("/home/u".into())),
            std::path::PathBuf::from("/xdg/tuxlink/config.json")
        );
        assert_eq!(
            resolve_config_path(None, None, Some("/home/u".into())),
            std::path::PathBuf::from("/home/u/.config/tuxlink/config.json")
        );
    }

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

    fn sample_config_json_without_packet() -> String {
        // A v1-shaped config with NO `packet` key — proves the field defaults.
        serde_json::json!({
            "schema_version": CONFIG_SCHEMA_VERSION,
            "wizard_completed": true,
            "connect": { "connect_to_cms": false, "transport": "Telnet" },
            "identity": { "callsign": null, "identifier": "FIELD-1", "grid": "CN87" },
            "privacy": { "gps_state": "Off", "position_precision": "FourCharGrid" },
            "pat_mbo_address": null
        })
        .to_string()
    }

    #[test]
    fn config_defaults_packet_section_when_absent() {
        let json = sample_config_json_without_packet();
        let cfg: Config = serde_json::from_str(&json).unwrap();
        let packet = cfg.packet;
        assert_eq!(packet.ssid, 0, "SSID defaults to 0");
        assert!(packet.listen_default, "listen is default-on (spec §4.5)");
        assert!(packet.link.is_none(), "no last KISS link until the operator sets one");
    }

    #[test]
    fn packet_config_round_trips_with_sticky_ssid_and_link() {
        // Persist an SSID + a TCP KISS link + tuned params, reload, assert sticky.
        let mut cfg: Config = serde_json::from_str(&sample_config_json_without_packet()).unwrap();
        cfg.packet = PacketConfig {
            ssid: 7,
            link: Some(KissLinkConfig::Tcp {
                host: "127.0.0.1".into(),
                port: 8001,
            }),
            params: Ax25ParamsConfig { paclen: 128, maxframe: 4, ..Default::default() },
            listen_default: false,
        };
        let serialized = serde_json::to_string(&cfg).unwrap();
        let reloaded: Config = serde_json::from_str(&serialized).unwrap();
        assert_eq!(reloaded.packet.ssid, 7);
        assert!(!reloaded.packet.listen_default);
        assert_eq!(reloaded.packet.params.paclen, 128);
        match reloaded.packet.link {
            Some(KissLinkConfig::Tcp { host, port }) => {
                assert_eq!(host, "127.0.0.1");
                assert_eq!(port, 8001);
            }
            other => panic!("expected a TCP KISS link, got {other:?}"),
        }
    }

    #[test]
    fn packet_ssid_above_15_is_rejected() {
        let mut cfg: Config = serde_json::from_str(&sample_config_json_without_packet()).unwrap();
        cfg.packet.ssid = 16;
        let err = cfg.validate().unwrap_err();
        assert!(
            matches!(err, ConfigValidationError::PacketSsidOutOfRange { ssid: 16 }),
            "expected PacketSsidOutOfRange, got {err:?}"
        );
    }

    // --- tuxlink-2y4: AX.25 connect T1 is honored verbatim (uhc floor reverted) ---
    // The uhc RF floor (MIN_RF_T1_MS = 10 s) tripled worst-case airtime and was the
    // wrong lever for the runaway-keying incident; 2y4 reverted it. Runaway airtime is
    // bounded by datalink::connect's ≤2-SABM key limit + connect_timeout cap, NOT by
    // inflating the retransmit timer. into_params now passes T1 through unchanged.

    #[test]
    fn into_params_honors_a_short_t1_verbatim_no_floor() {
        // The historical 3 s auto-default is passed through as-is — NOT floored to 10 s
        // (tuxlink-2y4 reverted the uhc floor).
        let cfg = Ax25ParamsConfig { t1_ms: 3000, ..Ax25ParamsConfig::default() };
        assert_eq!(
            cfg.into_params().t1,
            std::time::Duration::from_millis(3000),
            "T1 must be honored verbatim — the uhc RF floor was reverted (2y4)"
        );
    }

    #[test]
    fn into_params_honors_a_long_configured_t1_verbatim() {
        // A longer configured T1 is the operator's choice — passed through verbatim.
        let cfg = Ax25ParamsConfig { t1_ms: 15_000, ..Ax25ParamsConfig::default() };
        assert_eq!(
            cfg.into_params().t1,
            std::time::Duration::from_millis(15_000),
            "a configured T1 must be honored verbatim"
        );
    }

    #[test]
    fn into_params_sets_the_radio1_connect_timeout_ceiling() {
        // tuxlink-2y4: every runtime params carries the RADIO-1 connect airtime ceiling.
        let cfg = Ax25ParamsConfig::default();
        assert_eq!(
            cfg.into_params().connect_timeout,
            std::time::Duration::from_secs(25),
            "into_params must carry the connect_timeout safety ceiling"
        );
    }

    // --- tuxlink-4ek: ArdopUiConfig persistence tests ---

    #[test]
    fn ardop_ui_config_round_trips_through_json() {
        let cfg = ArdopUiConfig {
            binary: "ardopcf".into(),
            capture_device: "plughw:1,0".into(),
            playback_device: "plughw:1,0".into(),
            ptt_serial_path: Some("/dev/ttyUSB0".into()),
            cmd_port: 8515,
            bandwidth_hz: None,
            webgui_port: None,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: ArdopUiConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.binary, "ardopcf");
        assert_eq!(back.cmd_port, 8515);
        assert_eq!(back.ptt_serial_path.as_deref(), Some("/dev/ttyUSB0"));
    }

    // --- tuxlink-j0ij: ArdopUiConfig.bandwidth_hz persistence tests ---

    #[test]
    fn ardop_ui_config_round_trips_with_bandwidth_hz_some() {
        // bandwidth_hz: Some(500) → serializes to {... "bandwidth_hz": 500 ...},
        // deserializes back to Some(500). Round-trip preserves the operator's
        // ARQ-bandwidth preference across config writes (tuxlink-j0ij).
        let cfg = ArdopUiConfig {
            binary: "ardopcf".into(),
            capture_device: "plughw:1,0".into(),
            playback_device: "plughw:1,0".into(),
            ptt_serial_path: None,
            cmd_port: 8515,
            bandwidth_hz: Some(500),
            webgui_port: None,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(
            json.contains("\"bandwidth_hz\":500"),
            "serialized config must contain bandwidth_hz: 500 — got: {json}"
        );
        let back: ArdopUiConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.bandwidth_hz,
            Some(500),
            "bandwidth_hz must round-trip Some(500) verbatim"
        );
    }

    #[test]
    fn ardop_ui_config_round_trips_with_bandwidth_hz_none() {
        // bandwidth_hz: None → serializes WITHOUT a "bandwidth_hz" key
        // (skip_serializing_if = "Option::is_none"), deserializes back to None
        // (the Default::default for Option). Migration path for pre-j0ij configs.
        let cfg = ArdopUiConfig {
            binary: "ardopcf".into(),
            capture_device: "plughw:1,0".into(),
            playback_device: "plughw:1,0".into(),
            ptt_serial_path: None,
            cmd_port: 8515,
            bandwidth_hz: None,
            webgui_port: None,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(
            !json.contains("bandwidth_hz"),
            "None bandwidth_hz must be omitted from serialized JSON — got: {json}"
        );

        // Also verify a config WITHOUT the field deserializes (None default).
        let no_bw_json = r#"{"binary":"ardopcf","capture_device":"","playback_device":"","cmd_port":8515}"#;
        let back: ArdopUiConfig = serde_json::from_str(no_bw_json).unwrap();
        assert_eq!(
            back.bandwidth_hz, None,
            "pre-j0ij config (no bandwidth_hz key) must deserialize as None"
        );
    }

    // --- Operator smoke 2026-05-31 round 3: webgui_port override + resolution ---

    #[test]
    fn resolved_webgui_port_falls_back_to_cmd_port_minus_one() {
        // Default: webgui_port = None → derive from cmd_port - 1. With the
        // default cmd_port=8515, this yields 8514 — the value the frontend
        // expects and the value the `-G` spawn flag sets. Single helper rules
        // out the drift class that caused round-3's "connection refused."
        let cfg = ArdopUiConfig::default();
        assert_eq!(cfg.resolved_webgui_port(), Some(8514));
    }

    #[test]
    fn resolved_webgui_port_uses_explicit_override_when_set() {
        // Operator pin: explicit webgui_port wins over the derivation. Lets
        // an operator point both ends (spawn + button) at a non-conventional
        // ardopcf build/configuration.
        let cfg = ArdopUiConfig {
            cmd_port: 8515,
            webgui_port: Some(9999),
            ..Default::default()
        };
        assert_eq!(cfg.resolved_webgui_port(), Some(9999));
    }

    #[test]
    fn resolved_webgui_port_returns_none_when_unresolvable() {
        // cmd_port < 2 AND no explicit override → no valid port can be
        // derived; the spawn omits `-G` and the frontend surfaces an error.
        for low in [0u16, 1u16] {
            let cfg = ArdopUiConfig {
                cmd_port: low,
                webgui_port: None,
                ..Default::default()
            };
            assert_eq!(cfg.resolved_webgui_port(), None,
                "cmd_port={low}: must be unresolvable without override");
        }
        // ... but an explicit override still wins even when cmd_port is too low:
        let cfg = ArdopUiConfig {
            cmd_port: 0,
            webgui_port: Some(8514),
            ..Default::default()
        };
        assert_eq!(cfg.resolved_webgui_port(), Some(8514));
    }

    #[test]
    fn ardop_ui_config_round_trips_with_webgui_port_override() {
        // Operator-pinned webgui_port must round-trip cleanly. Mirrors the
        // bandwidth_hz pattern (skip_serializing_if when None).
        let cfg = ArdopUiConfig {
            binary: "ardopcf".into(),
            capture_device: "plughw:1,0".into(),
            playback_device: "plughw:1,0".into(),
            ptt_serial_path: None,
            cmd_port: 8515,
            bandwidth_hz: None,
            webgui_port: Some(9080),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(
            json.contains("\"webgui_port\":9080"),
            "serialized config must contain webgui_port: 9080 — got: {json}"
        );
        let back: ArdopUiConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.webgui_port, Some(9080));
    }

    #[test]
    fn ardop_ui_config_omits_webgui_port_when_none() {
        let cfg = ArdopUiConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(
            !json.contains("webgui_port"),
            "None webgui_port must be omitted from serialized JSON — got: {json}"
        );
        // Pre-existing configs that lack the key must deserialize as None
        // (additive migration; consistent with bandwidth_hz).
        let no_wg_json = r#"{"binary":"ardopcf","capture_device":"","playback_device":"","cmd_port":8515}"#;
        let back: ArdopUiConfig = serde_json::from_str(no_wg_json).unwrap();
        assert_eq!(back.webgui_port, None);
    }

    #[test]
    fn config_with_modem_ardop_some_then_none_round_trips() {
        // Build a minimal Config JSON with modem_ardop set, verify it round-trips.
        let json = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }},
                "modem_ardop": {{
                    "binary": "ardopcf",
                    "capture_device": "plughw:1,0",
                    "playback_device": "plughw:1,0",
                    "cmd_port": 8515,
                    "bandwidth_hz": 500
                }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION
        );
        let cfg: Config = serde_json::from_str(&json)
            .expect("Config with modem_ardop must deserialize");
        assert!(cfg.modem_ardop.is_some(), "modem_ardop should be Some");
        let ardop = cfg.modem_ardop.as_ref().unwrap();
        assert_eq!(ardop.binary, "ardopcf");
        assert_eq!(ardop.cmd_port, 8515);
        assert!(ardop.ptt_serial_path.is_none(), "absent ptt_serial_path defaults to None");
        assert_eq!(
            ardop.bandwidth_hz,
            Some(500),
            "bandwidth_hz must deserialize when present (tuxlink-j0ij)"
        );

        // Round-trip: serialize and reload.
        let reserialized = serde_json::to_string(&cfg).unwrap();
        let reloaded: Config = serde_json::from_str(&reserialized).unwrap();
        assert!(reloaded.modem_ardop.is_some());
        assert_eq!(
            reloaded.modem_ardop.as_ref().unwrap().bandwidth_hz,
            Some(500),
            "bandwidth_hz must survive a serialize→deserialize round-trip"
        );

        // Verify modem_ardop is absent from a config that never had it (migration path).
        let json_no_ardop = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION
        );
        let cfg_no_ardop: Config = serde_json::from_str(&json_no_ardop)
            .expect("old config without modem_ardop must deserialize (migration)");
        assert!(cfg_no_ardop.modem_ardop.is_none(), "modem_ardop must default to None when absent");
    }

    // --- tuxlink-dfmf: VaraUiConfig persistence + migration tests ---

    #[test]
    fn vara_ui_config_defaults_to_localhost_8300_8301() {
        let cfg = VaraUiConfig::default();
        assert_eq!(cfg.host, "127.0.0.1");
        assert_eq!(cfg.cmd_port, 8300);
        assert_eq!(cfg.data_port, 8301);
        assert_eq!(cfg.bandwidth_hz, None);
    }

    #[test]
    fn vara_ui_config_round_trips_through_serde() {
        let cfg = VaraUiConfig {
            host: "192.168.1.50".into(),
            cmd_port: 8400,
            data_port: 8401,
            bandwidth_hz: Some(2750),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: VaraUiConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back, cfg);
    }

    #[test]
    fn vara_ui_config_omits_bandwidth_when_none() {
        let cfg = VaraUiConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(
            !json.contains("bandwidth_hz"),
            "None bandwidth must NOT appear in serialized output (skip_serializing_if), got: {json}"
        );
    }

    #[test]
    fn config_modem_vara_round_trips_when_some() {
        let json = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }},
                "modem_vara": {{
                    "host": "192.168.1.50",
                    "cmd_port": 8400,
                    "data_port": 8401,
                    "bandwidth_hz": 2750
                }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION
        );
        let cfg: Config = serde_json::from_str(&json).expect("Config with modem_vara must deserialize");
        assert!(cfg.modem_vara.is_some());
        let vara = cfg.modem_vara.as_ref().unwrap();
        assert_eq!(vara.host, "192.168.1.50");
        assert_eq!(vara.cmd_port, 8400);
        assert_eq!(vara.bandwidth_hz, Some(2750));
    }

    #[test]
    fn config_modem_vara_absent_migrates_to_none() {
        let json = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION
        );
        let cfg: Config = serde_json::from_str(&json)
            .expect("old config without modem_vara must deserialize (migration)");
        assert!(cfg.modem_vara.is_none(), "modem_vara must default to None when absent");
    }

    // --- tuxlink-6c9y: RelayFavorite persistence + migration tests ---

    // Migration: a Config JSON WITHOUT `network_po_favorites` deserializes
    // to an empty Vec (proves `#[serde(default)]`). Mirrors
    // `config_modem_vara_absent_migrates_to_none` above.
    #[test]
    fn config_network_po_favorites_absent_migrates_to_empty_vec() {
        let json = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION
        );
        let cfg: Config = serde_json::from_str(&json)
            .expect("old config without network_po_favorites must deserialize (migration)");
        assert!(
            cfg.network_po_favorites.is_empty(),
            "network_po_favorites must default to empty Vec when absent"
        );
    }

    // Round-trip: a Config with one RelayFavorite serializes and deserializes
    // back equal. Mirrors `vara_ui_config_round_trips_through_serde` above.
    #[test]
    fn relay_favorite_round_trips_through_serde() {
        let fav = RelayFavorite {
            callsign: "W7AUX".into(),
            label: "Home mesh relay".into(),
            host: "192.168.1.100".into(),
            port: 8772,
        };
        let json = serde_json::to_string(&fav).unwrap();
        let back: RelayFavorite = serde_json::from_str(&json).unwrap();
        assert_eq!(back, fav);
    }

    // A full Config round-trip carrying a non-empty network_po_favorites Vec.
    #[test]
    fn config_with_network_po_favorites_round_trips() {
        let fav = RelayFavorite {
            callsign: "W7AUX".into(),
            label: "Test relay".into(),
            host: "relay.local".into(),
            port: 8772,
        };
        let json_in = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }},
                "network_po_favorites": [{{
                    "callsign": "W7AUX",
                    "label": "Test relay",
                    "host": "relay.local",
                    "port": 8772
                }}]
            }}"#,
            ver = CONFIG_SCHEMA_VERSION
        );
        let cfg: Config = serde_json::from_str(&json_in)
            .expect("Config with network_po_favorites must deserialize");
        assert_eq!(cfg.network_po_favorites.len(), 1);
        assert_eq!(cfg.network_po_favorites[0], fav);

        // Round-trip through serialization.
        let serialized = serde_json::to_string(&cfg).unwrap();
        let cfg2: Config = serde_json::from_str(&serialized).unwrap();
        assert_eq!(cfg2.network_po_favorites, cfg.network_po_favorites);
    }
}
